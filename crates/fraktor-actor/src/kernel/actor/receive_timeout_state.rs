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

//! The receive timeout state of one actor.

use core::time::Duration;

use crate::kernel::actor::ReceiveTimeoutSetting;

/// What the one scheduled firing of a receive timeout is doing.
///
/// Mirrors the `task` of Pekko's `dungeon.ReceiveTimeout.State`: a
/// `Cancellable` that is empty, that is still to run, or that has run and
/// put its `ReceiveTimeout` in the mailbox.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ReceiveTimeoutTask {
  /// No firing is scheduled.
  Empty,
  /// A firing is scheduled for `deadline`, carrying `timeout`.
  Scheduled {
    /// The monotonic time at which the timeout fires.
    deadline: Duration,
    /// The configured duration the fired message carries.
    timeout: Duration,
  },
  /// The firing happened and its message is in the mailbox, unhandled.
  Fired,
}

/// The receive timeout state of one actor: the setting it asked for last,
/// the one scheduled firing, and the version that tells a change apart
/// across one message handling.
///
/// Mirrors `dungeon.ReceiveTimeout.State(timeout, task, version)` of
/// Pekko. The version is compared only across one handling, as in the
/// original; it never suppresses a `ReceiveTimeout` that already reached
/// the mailbox, which is always delivered.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ReceiveTimeoutState {
  timeout: ReceiveTimeoutSetting,
  task: ReceiveTimeoutTask,
  version: u32,
}

impl ReceiveTimeoutState {
  /// Creates the state of an actor that sets its timeout for the first
  /// time, at version 1, as `setReceiveTimeout` does in the original.
  pub(crate) fn new(timeout: ReceiveTimeoutSetting) -> Self {
    Self { timeout, task: ReceiveTimeoutTask::Empty, version: 1 }
  }

  /// The setting the actor asked for last.
  pub(crate) fn setting(&self) -> ReceiveTimeoutSetting {
    self.timeout
  }

  /// The version of the state, for the comparison across one handling.
  pub(crate) fn version(&self) -> u32 {
    self.version
  }

  /// The scheduled firing, when one is still to run.
  pub(crate) fn scheduled(&self) -> Option<(Duration, Duration)> {
    match self.task {
      | ReceiveTimeoutTask::Scheduled { deadline, timeout } => Some((deadline, timeout)),
      | _ => None,
    }
  }

  /// Records a new setting, as `setReceiveTimeout` does: the setting is
  /// replaced and the version advances; nothing is rescheduled here.
  pub(crate) fn set_receive_timeout(&mut self, timeout: ReceiveTimeoutSetting) {
    self.timeout = timeout;
    self.version = self.version.wrapping_add(1);
  }

  /// Cancels the scheduled firing, as `cancelReceiveTimeoutTask` does.
  pub(crate) fn cancel_task(&mut self) {
    if self.task != ReceiveTimeoutTask::Empty {
      self.task = ReceiveTimeoutTask::Empty;
      self.version = self.version.wrapping_add(1);
    }
  }

  /// Runs `checkReceiveTimeout`: with a finite setting, reschedule when
  /// asked to or when nothing is scheduled; with any other setting,
  /// cancel what is scheduled. A deadline past the clock's range
  /// schedules nothing.
  pub(crate) fn check(&mut self, reschedule: bool, now: Duration) {
    match self.timeout {
      | ReceiveTimeoutSetting::Finite(timeout) => {
        if reschedule || self.task == ReceiveTimeoutTask::Empty {
          self.task = match now.checked_add(timeout) {
            | Some(deadline) => ReceiveTimeoutTask::Scheduled { deadline, timeout },
            | None => ReceiveTimeoutTask::Empty,
          };
          self.version = self.version.wrapping_add(1);
        }
      },
      | _ => self.cancel_task(),
    }
  }

  /// Marks the scheduled firing as fired: the deadline is gone and its
  /// message is in the mailbox, so one firing puts exactly one message.
  pub(crate) fn mark_fired(&mut self) {
    self.task = ReceiveTimeoutTask::Fired;
  }
}
