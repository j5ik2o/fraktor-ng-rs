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

//! A drop guard that reverts a switch's state unless it is defused.

use core::sync::atomic::{AtomicBool, Ordering};

/// Reverts the state in `on` back to `from` on drop, unless defused.
///
/// The guard is declared after the switch's lock guard, so it drops while
/// the lock is still held. The revert itself is the
/// original's `compareAndSet(!from, from)`: under the lock a store would
/// give the same result, but the original's form is kept. Because the
/// revert runs on drop, it covers both an action that returns `Err` and an
/// action that panics, matching the original, which catches every
/// `Throwable`.
pub(super) struct RevertGuard<'a> {
  on: &'a AtomicBool,
  from: bool,
  armed: bool,
}

impl<'a> RevertGuard<'a> {
  pub(super) fn new(on: &'a AtomicBool, from: bool) -> Self {
    Self { on, from, armed: true }
  }

  /// Keeps the change: the state stays as the caller set it.
  pub(super) fn defuse(&mut self) {
    self.armed = false;
  }
}

impl Drop for RevertGuard<'_> {
  fn drop(&mut self) {
    if self.armed {
      self.on.compare_exchange(!self.from, self.from, Ordering::SeqCst, Ordering::SeqCst).ok();
    }
  }
}
