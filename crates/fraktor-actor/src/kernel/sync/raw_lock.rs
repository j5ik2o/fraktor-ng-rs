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

//! The port for one lock that the kernel takes.

/// A lock that runs one closed section at a time.
///
/// The kernel guards a value with a lock it does not choose: the adaptor's
/// [`LockFactory`](super::LockFactory) makes one for each value. A thread pool
/// passes a lock that puts a waiting thread to sleep, an embedded target passes
/// a critical section, and a system that one thread drives can spin
/// (design of #234, section 3.2, rule C2). The lock is called through `dyn`,
/// so its type never reaches the kernel's public types.
///
/// # Safety
///
/// The kernel hands out `&mut` access to the guarded value inside `section`,
/// so an implementation must:
///
/// - run `section` exactly once per call to [`lock`](Self::lock);
/// - never run two sections of the same lock at the same time, on any thread
///   or in any interrupt;
/// - release the lock when `section` unwinds, and ignore any poisoning that
///   the unwind leaves behind, so that the next section still runs.
pub unsafe trait RawLock: Send + Sync {
  /// Runs `section` while holding the lock.
  fn lock(&self, section: &mut dyn FnMut());
}
