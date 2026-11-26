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

//! A value that the kernel reaches only through a lock.

use alloc::boxed::Box;
use core::cell::UnsafeCell;

use crate::kernel::sync::RawLock;

#[cfg(test)]
#[path = "lock_cell_test.rs"]
mod tests;

/// A value guarded by a [`RawLock`], reached only inside [`with`](Self::with).
///
/// No user code runs inside the lock (design of #234, invariant I16): the
/// closure gets the `&mut T` and nothing else, so it cannot reach an actor, a
/// reference or a handler. A closure only moves values in and out; a value
/// that may run user code when it is dropped, such as an envelope, is moved
/// out and dropped after `with` returns.
pub(crate) struct LockCell<T> {
  lock: Box<dyn RawLock>,
  value: UnsafeCell<T>,
}

// SAFETY: the value moves with the cell, so the cell is `Send` when `T` is.
unsafe impl<T: Send> Send for LockCell<T> {}
// SAFETY: every access to the value goes through `with`, which holds the lock;
// the `RawLock` contract lets one section run at a time. As with a mutex, only
// `T: Send` is needed, because the value is handed to one thread at a time.
unsafe impl<T: Send> Sync for LockCell<T> {}

impl<T> LockCell<T> {
  pub(crate) fn new(lock: Box<dyn RawLock>, value: T) -> Self {
    Self { lock, value: UnsafeCell::new(value) }
  }

  /// Runs `f` on the value while holding the lock, and returns what it returns.
  ///
  /// Calling `with` on the same cell from inside `f` never returns: the lock
  /// is not re-entrant. A spin lock spins and a thread lock blocks.
  pub(crate) fn with<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
    let mut f = Some(f);
    let mut result = None;
    let value = self.value.get();
    self.lock.lock(&mut || {
      let f = f.take().expect("RawLock::lock runs the section only once");
      // SAFETY: the lock is held, so no other section of this cell runs and
      // no other reference to the value exists until the section ends.
      result = Some(f(unsafe { &mut *value }));
    });
    result.expect("RawLock::lock runs the section")
  }
}
