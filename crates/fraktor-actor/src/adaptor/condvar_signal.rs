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

use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
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
/// wait is not lost. A `notify` with no thread waiting only raises the flag:
/// it takes the lock and wakes the condvar only for a waiter, so the system
/// that raises it for every new piece of work does not pay for a wakeup no
/// one waits for. A poisoned lock still serves: a panic while notifying must
/// not keep a waiter asleep.
#[derive(Debug, Default)]
pub struct CondvarSignal {
  raised: AtomicBool,
  waiters: AtomicUsize,
  sleep: Mutex<()>,
  wakeup: Condvar,
}

impl CondvarSignal {
  /// Creates a signal that is not raised.
  pub fn new() -> Self {
    Self::default()
  }

  fn lock(&self) -> MutexGuard<'_, ()> {
    self.sleep.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
  }
}

impl WakeSignal for CondvarSignal {
  fn notify(&self) {
    // Sequentially consistent with the waiter's two steps: either this sees
    // the waiter, or the waiter sees the flag.
    self.raised.store(true, Ordering::SeqCst);
    if self.waiters.load(Ordering::SeqCst) > 0 {
      let _sleep = self.lock();
      self.wakeup.notify_all();
    }
  }

  fn wait(&self, max: Duration) -> bool {
    let deadline = Instant::now() + max;
    self.waiters.fetch_add(1, Ordering::SeqCst);
    let mut sleep = self.lock();
    let raised = loop {
      if self.raised.swap(false, Ordering::SeqCst) {
        break true;
      }
      let left = deadline.saturating_duration_since(Instant::now());
      if left.is_zero() {
        break false;
      }
      sleep = match self.wakeup.wait_timeout(sleep, left) {
        | Ok((guard, _)) => guard,
        | Err(poisoned) => poisoned.into_inner().0,
      };
    };
    drop(sleep);
    self.waiters.fetch_sub(1, Ordering::SeqCst);
    raised
  }
}
