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

//! The reference-counted pointers the kernel shares values through.
//!
//! The kernel names `Arc` and `Weak` through these aliases so that it does not
//! assume atomic compare-and-set. Targets with pointer-sized atomics use
//! `alloc::sync`; the others use `portable_atomic_util`, whose atomics fall
//! back to a critical section that the adaptor provides.
//!
//! It also holds the lock port (design of #234, section 3.2, rule C2). The
//! kernel never assumes a kind of lock: a [`LockFactory`] that the adaptor
//! passes makes a [`RawLock`] for each value the kernel guards, and the
//! kernel reaches the value only through a closed section (`LockCell::with`).
//! [`SpinLockFactory`] is the kernel's default for a system that one thread
//! drives.

// syn-check: allow(tests-location, module-wiring) -- 所有権モデルの条件1の数え上げ（設計 §11.11.7、オーナーの許可 2026-11-27、元帥の項目472）
#[cfg(test)]
pub(crate) mod counts;
mod lock_cell;
mod lock_factory;
mod raw_lock;
mod spin_lock_factory;

pub(crate) use lock_cell::LockCell;
pub use lock_factory::LockFactory;
pub use raw_lock::RawLock;
pub use spin_lock_factory::SpinLockFactory;

#[cfg(target_has_atomic = "ptr")]
pub(crate) type Arc<T> = alloc::sync::Arc<T>;
#[cfg(not(target_has_atomic = "ptr"))]
pub(crate) type Arc<T> = portable_atomic_util::Arc<T>;

#[cfg(target_has_atomic = "ptr")]
pub(crate) type Weak<T> = alloc::sync::Weak<T>;
#[cfg(not(target_has_atomic = "ptr"))]
pub(crate) type Weak<T> = portable_atomic_util::Weak<T>;

/// Takes `mutex` with `try_lock`, panicking when it is already held.
///
/// Stage 1 of #234 replaces the `RefCell` borrows of the system handles with
/// this lock: a re-entrant or concurrent call must fail with the same
/// "already borrowed" panic the double borrow produced, and must not spin
/// waiting for a lock its own call stack already holds (design section 2.3
/// and decision D5 in section 9.3).
pub(crate) fn lock<T>(mutex: &spin::Mutex<T>) -> spin::MutexGuard<'_, T> {
  mutex.try_lock().unwrap_or_else(|| panic!("already borrowed: re-entrant or concurrent use"))
}
