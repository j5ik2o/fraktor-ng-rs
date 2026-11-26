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

//! The status word of a mailbox.

use portable_atomic::AtomicU32;
use portable_atomic::Ordering;

#[cfg(test)]
#[path = "mailbox_status_test.rs"]
mod tests;

const CLOSED: u32 = 1;
const SCHEDULED: u32 = 2;
const HAS_SYSTEM_MESSAGES: u32 = 4;
const SUSPEND_UNIT: u32 = 8;
// What a run must not see to take a normal message: closed or suspended.
const SHOULD_NOT_PROCESS: u32 = !(SCHEDULED | HAS_SYSTEM_MESSAGES);

/// The status of a mailbox in one atomic word.
///
/// Mirrors the `status` of `org.apache.pekko.dispatch.Mailbox`
/// (`Mailbox.scala:47-55`, `:160-230`): bit 0 is closed and bit 1 is scheduled.
/// Bit 2 is the "has system messages" flag, which Pekko does not have, and the
/// suspend count lives from bit 3 up (design of #234, section 3.8, idea 5).
/// Closed never clears; once it is set, the other bits mean nothing.
///
/// The operations are the status part of `specs/mailbox_scheduling.qnt`.
/// Scheduling takes two steps there and here: the caller reads
/// [`can_be_scheduled`](Self::can_be_scheduled), then calls
/// [`set_as_scheduled`](Self::set_as_scheduled), one `fetch_or` that looks at
/// the closed and scheduled bits only, as Pekko's `setAsScheduled` looks only
/// at `shouldScheduleMask`.
pub(crate) struct MailboxStatus(AtomicU32);

impl MailboxStatus {
  pub(crate) const fn new() -> Self {
    Self(AtomicU32::new(0))
  }

  fn load(&self) -> u32 {
    self.0.load(Ordering::Acquire)
  }

  pub(crate) fn is_closed(&self) -> bool {
    self.load() & CLOSED != 0
  }

  pub(crate) fn is_suspended(&self) -> bool {
    self.load() >= SUSPEND_UNIT
  }

  /// Returns whether the "has system messages" flag is up.
  pub(crate) fn has_system_messages(&self) -> bool {
    self.load() & HAS_SYSTEM_MESSAGES != 0
  }

  /// Returns whether a run may take a normal message: open and not suspended
  /// (Pekko's `shouldProcessMessage`).
  pub(crate) fn should_process_message(&self) -> bool {
    self.load() & SHOULD_NOT_PROCESS == 0
  }

  /// Returns whether a run would have work, given what the queues hold
  /// (Pekko's `canBeScheduledForExecution` without the hints): an open
  /// mailbox runs for any message, a suspended one for a system message only,
  /// a closed one never.
  pub(crate) fn can_be_scheduled(&self, has_messages: bool, has_system_messages: bool) -> bool {
    let status = self.load();
    if status & CLOSED != 0 {
      false
    } else if status >= SUSPEND_UNIT {
      has_system_messages
    } else {
      has_messages || has_system_messages
    }
  }

  /// Sets the scheduled bit with one `fetch_or`, and returns whether this
  /// call turned it on in an open mailbox. Only that caller submits the
  /// mailbox to the pool.
  pub(crate) fn set_as_scheduled(&self) -> bool {
    self.0.fetch_or(SCHEDULED, Ordering::AcqRel) & (CLOSED | SCHEDULED) == 0
  }

  /// Clears the scheduled bit at the end of a run (Pekko's `setAsIdle`).
  pub(crate) fn set_as_idle(&self) {
    self.0.fetch_and(!SCHEDULED, Ordering::AcqRel);
  }

  /// Raises the "has system messages" flag. A sender of a system message
  /// raises it after the enqueue, inside the gate's lock.
  pub(crate) fn raise_system_flag(&self) {
    self.0.fetch_or(HAS_SYSTEM_MESSAGES, Ordering::AcqRel);
  }

  /// Lowers the flag. A run lowers it before
  /// it empties the system queue; the other order would leave a message that
  /// arrives in between in the queue with the flag down.
  pub(crate) fn clear_system_flag(&self) {
    self.0.fetch_and(!HAS_SYSTEM_MESSAGES, Ordering::AcqRel);
  }

  /// Adds one to the suspend count and returns whether the mailbox was not
  /// suspended before. A closed mailbox stays as it is and returns `false`.
  pub(crate) fn suspend(&self) -> bool {
    let mut status = self.load();
    loop {
      if status & CLOSED != 0 {
        return false;
      }
      let next = status.checked_add(SUSPEND_UNIT).expect("the suspend count of a mailbox overflowed");
      match self.0.compare_exchange_weak(status, next, Ordering::AcqRel, Ordering::Acquire) {
        | Ok(_) => return status < SUSPEND_UNIT,
        | Err(actual) => status = actual,
      }
    }
  }

  /// Takes one from the suspend count, stopping at zero, and returns whether
  /// the mailbox is no longer suspended. A closed mailbox stays as it is and
  /// returns `false`.
  pub(crate) fn resume(&self) -> bool {
    let mut status = self.load();
    loop {
      if status & CLOSED != 0 {
        return false;
      }
      let next = if status < SUSPEND_UNIT { status } else { status - SUSPEND_UNIT };
      match self.0.compare_exchange_weak(status, next, Ordering::AcqRel, Ordering::Acquire) {
        | Ok(_) => return next < SUSPEND_UNIT,
        | Err(actual) => status = actual,
      }
    }
  }

  /// Sets the closed bit and returns whether this call closed the mailbox.
  pub(crate) fn become_closed(&self) -> bool {
    self.0.fetch_or(CLOSED, Ordering::AcqRel) & CLOSED == 0
  }
}
