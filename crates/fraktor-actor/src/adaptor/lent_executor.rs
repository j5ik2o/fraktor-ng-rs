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

//! The record of the executors lent on a thread.
//!
//! The kernel's `Executor::lend` registers each lend here, so a
//! test-support entry reached from inside a lend — a `TestActorRef`
//! built from `context.system()` while the actor runs — finds the
//! executor it is already inside instead of locking it again and being
//! refused as a concurrent use. The record is a thread local, so it
//! lives in the adaptor: the kernel cannot use the standard library.

use std::cell::RefCell;

use crate::kernel::executor::Executor;

thread_local! {
  /// The executors lent on this thread, innermost last, named by the
  /// mutex that guards each of them and the borrow the lend holds.
  static LENT_EXECUTORS: RefCell<Vec<(*const spin::Mutex<Executor>, *mut Executor)>> =
    const { RefCell::new(Vec::new()) };
}

/// Records one lend in the thread's record for the lend's duration.
pub(crate) struct LentRegistration;

impl LentRegistration {
  pub(crate) fn register(mutex: &spin::Mutex<Executor>, executor: &mut Executor) -> Self {
    LENT_EXECUTORS.with(|lent| {
      lent.borrow_mut().push((mutex as *const spin::Mutex<Executor>, executor as *mut Executor));
    });
    Self
  }
}

impl Drop for LentRegistration {
  fn drop(&mut self) {
    LENT_EXECUTORS.with(|lent| {
      lent.borrow_mut().pop();
    });
  }
}

/// Returns the executor lent under `mutex` on this thread, when a lend
/// of it is active on this call stack.
pub(crate) fn active_lent_executor(mutex: &spin::Mutex<Executor>) -> Option<*mut Executor> {
  LENT_EXECUTORS.with(|lent| {
    lent.borrow().iter().rev().find(|(lent_mutex, _)| core::ptr::eq(*lent_mutex, mutex)).map(|(_, pointer)| *pointer)
  })
}
