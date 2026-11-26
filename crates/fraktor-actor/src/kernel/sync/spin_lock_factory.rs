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

//! The kernel's default lock: a spin lock.

use alloc::boxed::Box;

use crate::kernel::sync::LockFactory;
use crate::kernel::sync::RawLock;

/// Makes spin locks.
///
/// This is the kernel's default for a system that one thread drives, where
/// nobody waits for a lock (design of #234, section 3.2). A thread pool with
/// more threads than cores, or an embedded target where an interrupt may take
/// the same lock, needs the adaptor's lock instead: a spinning thread keeps
/// the holder from running.
#[derive(Clone, Copy, Debug, Default)]
pub struct SpinLockFactory;

impl LockFactory for SpinLockFactory {
  fn new_lock(&self) -> Box<dyn RawLock> {
    Box::new(SpinRawLock(spin::Mutex::new(())))
  }
}

struct SpinRawLock(spin::Mutex<()>);

// SAFETY: `spin::Mutex` lets one guard live at a time and runs `section` once
// under it. Taking it is an acquire and dropping the guard a release. The guard is dropped when `section` unwinds, which unlocks, and a
// spin lock has no poisoning.
unsafe impl RawLock for SpinRawLock {
  fn lock(&self, section: &mut dyn FnMut()) {
    let _guard = self.0.lock();
    section();
  }
}
