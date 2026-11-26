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

//! The standard library's wake signal: a flag behind a mutex and a condvar.

use std::sync::Condvar;
use std::sync::Mutex;
use std::sync::MutexGuard;
use std::time::Duration;
use std::time::Instant;

use crate::kernel::dispatch::WakeSignal;

#[cfg(test)]
#[path = "condvar_signal_test.rs"]
mod tests;

/// A [`WakeSignal`] a waiting thread sleeps on with a condvar.
///
/// The flag stays raised until a wait takes it, so a `notify` before the
/// wait is not lost. A poisoned lock still yields its flag: a panic while
/// notifying must not keep a waiter asleep.
#[derive(Debug, Default)]
pub struct CondvarSignal {
  raised: Mutex<bool>,
  wakeup: Condvar,
}

impl CondvarSignal {
  /// Creates a signal that is not raised.
  pub fn new() -> Self {
    Self::default()
  }

  fn lock(&self) -> MutexGuard<'_, bool> {
    self.raised.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
  }
}

impl WakeSignal for CondvarSignal {
  fn notify(&self) {
    *self.lock() = true;
    self.wakeup.notify_all();
  }

  fn wait(&self, max: Duration) -> bool {
    let deadline = Instant::now() + max;
    let mut raised = self.lock();
    while !*raised {
      let left = deadline.saturating_duration_since(Instant::now());
      if left.is_zero() {
        return false;
      }
      raised = match self.wakeup.wait_timeout(raised, left) {
        | Ok((guard, _)) => guard,
        | Err(poisoned) => poisoned.into_inner().0,
      };
    }
    *raised = false;
    true
  }
}
