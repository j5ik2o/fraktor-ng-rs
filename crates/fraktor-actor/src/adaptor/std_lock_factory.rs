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

//! The std lock for the kernel's lock port.

use std::sync::Mutex;

use crate::kernel::sync::LockFactory;
use crate::kernel::sync::RawLock;

#[cfg(test)]
#[path = "std_lock_factory_test.rs"]
mod tests;

/// Makes locks on `std::sync::Mutex`, which put a waiting thread to sleep.
///
/// A thread pool with more threads than cores needs a lock like this: a
/// spinning waiter keeps the holder from running (design of #234, section
/// 3.2). `std::sync::Mutex` is chosen over `parking_lot` so that no
/// dependency is added. A section that unwinds poisons the mutex; the lock
/// ignores the poison, as `ChannelBoundary` does, because a section only
/// moves values and leaves nothing half done.
#[derive(Clone, Copy, Debug, Default)]
pub struct StdLockFactory;

impl LockFactory for StdLockFactory {
  fn new_lock(&self) -> Box<dyn RawLock> {
    Box::new(StdRawLock(Mutex::new(())))
  }
}

struct StdRawLock(Mutex<()>);

// SAFETY: `Mutex` lets one guard live at a time and runs `section` once under
// it, and it orders the sections as any mutex does. The guard is dropped when `section` unwinds, which unlocks, and the
// poison that the unwind leaves is ignored.
unsafe impl RawLock for StdRawLock {
  fn lock(&self, section: &mut dyn FnMut()) {
    let _guard = self.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    section();
  }
}
