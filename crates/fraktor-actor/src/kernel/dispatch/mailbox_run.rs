// SPDX-License-Identifier: Apache-2.0
//
// Licensed to the Apache Software Foundation (ASF) under one or more
// license agreements; and to You under the Apache License, version 2.0:
//
//   https://www.apache.org/licenses/LICENSE-2.0
//
// This file is part of the Apache Pekko project, which was derived from Akka.
//
// Copyright (C) 2009-2022 Lightbend Inc. <https://www.lightbend.com>
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.

//! One run of a mailbox: the work the kernel hands to the adaptor.

use crate::kernel::dispatch::Mailbox;
use crate::kernel::dispatch::MailboxCell;
use crate::kernel::dispatch::RunOutcome;
use crate::kernel::sync::Arc;

#[cfg(test)]
#[path = "mailbox_run_test.rs"]
mod tests;

/// The right to run one mailbox once, held by whoever won its scheduled bit.
///
/// Mirrors `org.apache.pekko.dispatch.Mailbox.run` (`Mailbox.scala:262`), cut
/// where the kernel meets the adaptor (design of #234, section 3.8, idea 1):
/// the adaptor calls [`run`](Self::run), and the run tells it whether to run
/// again instead of scheduling itself. Only
/// [`Mailbox::try_schedule`] makes one.
pub struct MailboxRun<C> {
  mailbox: Arc<Mailbox<C>>,
}

impl<C: MailboxCell> MailboxRun<C> {
  pub(crate) fn new(mailbox: Arc<Mailbox<C>>) -> Self {
    Self { mailbox }
  }

  /// Runs the mailbox once (design of #234, section 3.5):
  ///
  /// 1. takes the actor out of the mailbox, and panics when it is not there
  ///    (invariant I1);
  /// 2. unless the mailbox is closed, handles the waiting system messages,
  ///    then normal messages up to the throughput and the deadline, with the
  ///    system messages that arrive after each one;
  /// 3. puts the actor back, also when the actor panics;
  /// 4. clears the scheduled bit;
  /// 5. reserves the mailbox again, in case a message came while the bit was
  ///    set.
  ///
  /// Returns [`RunOutcome::Again`] when step 5 won the bit, with the bit still
  /// set, and [`RunOutcome::Idle`] when it lost: nothing is left to do, or
  /// another caller won it and submits the mailbox itself.
  pub fn run(self) -> RunOutcome<C> {
    let mailbox = &*self.mailbox;
    {
      let mut guard = CellGuard { mailbox, cell: Some(mailbox.take_cell()) };
      let cell = guard.cell.as_mut().expect("the guard holds the cell until it is dropped");
      if !mailbox.is_closed() {
        mailbox.process_all_system_messages(cell);
        mailbox.process_mailbox(cell);
      }
    }
    mailbox.set_as_idle();
    if mailbox.reserve() {
      RunOutcome::Again(self)
    } else {
      RunOutcome::Idle
    }
  }
}

// Puts the actor back when the run ends, and when the actor panics out of it,
// so that the next run does not find the slot empty and panic with I1.
struct CellGuard<'a, C: MailboxCell> {
  mailbox: &'a Mailbox<C>,
  cell: Option<C>,
}

impl<C: MailboxCell> Drop for CellGuard<'_, C> {
  fn drop(&mut self) {
    if let Some(cell) = self.cell.take() {
      self.mailbox.put_back_cell(cell);
    }
  }
}
