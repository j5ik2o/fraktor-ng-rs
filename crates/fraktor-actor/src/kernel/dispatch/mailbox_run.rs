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

use alloc::boxed::Box;

use crate::kernel::dispatch::Mailbox;
use crate::kernel::dispatch::MailboxCell;
use crate::kernel::dispatch::RunOutcome;
use crate::kernel::dispatch::RunReport;
use crate::kernel::dispatch::Throughput;
use crate::kernel::sync::Arc;

#[cfg(test)]
#[path = "mailbox_run_test.rs"]
mod tests;

/// The right to run one mailbox once, held by whoever won its scheduled bit.
///
/// Mirrors `org.apache.pekko.dispatch.Mailbox.run` (`Mailbox.scala:262`), cut
/// where the kernel meets the dispatcher (design of #234, section 3.8, idea
/// 1): the dispatcher calls [`run`](Self::run), and the run tells it whether
/// to run again instead of scheduling itself. Only
/// [`Mailbox::try_schedule`] makes one.
pub(crate) struct MailboxRun<C> {
  mailbox: Arc<Mailbox<C>>,
}

impl<C: MailboxCell> MailboxRun<C> {
  pub(crate) fn new(mailbox: Arc<Mailbox<C>>) -> Self {
    Self { mailbox }
  }

  /// Returns whether the mailbox of this run has closed.
  pub(crate) fn mailbox_is_closed(&self) -> bool {
    self.mailbox.is_closed()
  }

  /// Returns whether this is a run of `mailbox`.
  pub(crate) fn is_for(&self, mailbox: &Mailbox<C>) -> bool {
    core::ptr::eq(&*self.mailbox, mailbox)
  }

  /// Runs the mailbox once (design of #234, section 3.5):
  ///
  /// 1. unless the mailbox is closed, takes the actor out of it, and panics
  ///    when it is not there (invariant I1);
  /// 2. handles the waiting system messages, then normal messages up to the
  ///    throughput and the deadline, with the system messages that arrive
  ///    after each one, until the actor yields;
  /// 3. puts the actor back, unless the mailbox has closed: then the actor
  ///    is dropped, outside every lock;
  /// 4. clears the scheduled bit;
  /// 5. reserves the mailbox again, in case a message came while the bit was
  ///    set.
  ///
  /// `env` is what the run lends the actor, and `budget`, when given, takes
  /// the place of the mailbox's throughput (`run_one` passes one).
  ///
  /// The outcome is [`RunOutcome::Again`] when step 5 won the bit, with the
  /// bit still set, and [`RunOutcome::Idle`] when it lost: nothing is left to
  /// do, or another caller won it and submits the mailbox itself.
  ///
  /// When the actor panics out of the run, steps 3 to 5 still happen, as
  /// they do in the `finally` of Pekko's `Mailbox.run`: the system message
  /// flag is raised again while a message is left, and a run won in step 5
  /// goes to the dispatcher. The panic goes on to the caller.
  pub(crate) fn run(self, env: &mut C::Env, budget: Option<Throughput>) -> RunReport<C> {
    let mailbox = &*self.mailbox;
    let mut delivered = 0;
    if !mailbox.is_closed() {
      let mut guard = RunGuard { mailbox: &self.mailbox, cell: Some(mailbox.take_cell()), finished: false };
      let cell = guard.cell.as_mut().expect("the guard holds the cell until it is dropped");
      mailbox.process_all_system_messages(cell, env);
      delivered = mailbox.process_mailbox(cell, env, budget);
      guard.finished = true;
    }
    mailbox.set_as_idle();
    let outcome = if mailbox.reserve() { RunOutcome::Again(self) } else { RunOutcome::Idle };
    RunReport { outcome, delivered }
  }
}

// Ends the run when the actor panics out of it, as the `finally` of Pekko's
// `Mailbox.run` does, so that the next run does not find the slot empty and
// panic with I1, the bit does not stay set, and no system message is hidden
// behind a lowered flag.
struct RunGuard<'a, C: MailboxCell> {
  mailbox: &'a Arc<Mailbox<C>>,
  cell: Option<Box<C>>,
  finished: bool,
}

impl<C: MailboxCell> Drop for RunGuard<'_, C> {
  fn drop(&mut self) {
    if let Some(cell) = self.cell.take() {
      if self.mailbox.is_closed() {
        drop(cell);
      } else {
        self.mailbox.put_back_cell(cell);
      }
    }
    if !self.finished {
      self.mailbox.raise_system_flag_if_queued();
      self.mailbox.set_as_idle();
      Mailbox::schedule(self.mailbox);
    }
  }
}
