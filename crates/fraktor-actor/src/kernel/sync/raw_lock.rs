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
/// a lock built on a critical section, and a system that one thread drives can
/// spin. The lock is called through `dyn`,
/// so its type never reaches the kernel's public types.
///
/// # Safety
///
/// The kernel hands out `&mut` access to the guarded value inside `section`,
/// so an implementation must:
///
/// - run `section` exactly once per call to [`lock`](Self::lock);
/// - never run two sections of the same lock at the same time, on any thread
///   or in any interrupt, and also not on the same thread: a call to `lock`
///   from inside a section of the same lock must not run its section (it may
///   panic, or block forever, but it must not enter);
/// - make every write of one section visible to the sections that run after
///   it: releasing the lock is a release and taking it is an acquire, as for
///   any mutex, so that the end of a section happens before the start of the
///   next one, also on another thread;
/// - release the lock when `section` unwinds.
///
/// The same-thread rule is where a lock differs from
/// `critical_section::with` and from embassy-sync's `CriticalSectionRawMutex`
/// and `ThreadModeRawMutex`: those let the same thread enter again, which is
/// sound for embassy-sync's `Mutex` only because it hands out `&T` and leaves
/// the borrow check to a `RefCell`. An implementation on them must add its own
/// check, such as a flag per lock that a nested `lock` finds set and panics
/// on, before it runs `section`.
///
/// Besides what safety needs, the kernel expects one more thing: a lock
/// ignores any poisoning an unwind leaves behind, so that the next section
/// still runs.
pub unsafe trait RawLock: Send + Sync {
  /// Runs `section` while holding the lock.
  fn lock(&self, section: &mut dyn FnMut());
}
