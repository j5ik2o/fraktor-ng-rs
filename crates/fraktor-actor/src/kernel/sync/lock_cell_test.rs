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

//! Unit tests for [`LockCell`](super::LockCell) on the kernel's spin lock.

use std::panic;
use std::thread;
use std::vec::Vec;

use super::LockCell;
use crate::kernel::sync::Arc;
use crate::kernel::sync::LockFactory;
use crate::kernel::sync::SpinLockFactory;

#[test]
fn with_returns_what_the_closure_returns_and_keeps_the_change() {
  let cell = LockCell::new(SpinLockFactory.new_lock(), 1);

  let before = cell.with(|value| core::mem::replace(value, 2));

  assert_eq!(before, 1);
  assert_eq!(cell.with(|value| *value), 2);
}

// The lock comes off when the closure unwinds, so the next `with` runs.
#[test]
fn a_panic_inside_with_releases_the_lock() {
  let cell = LockCell::new(SpinLockFactory.new_lock(), 0);

  let unwound = panic::catch_unwind(panic::AssertUnwindSafe(|| cell.with(|_| panic!("inside the section"))));

  assert!(unwound.is_err());
  assert_eq!(cell.with(|value| *value), 0);
}

#[test]
fn threads_take_turns_on_the_value() {
  let cell = Arc::new(LockCell::new(SpinLockFactory.new_lock(), 0_u64));
  let workers: Vec<_> = (0..4)
    .map(|_| {
      let cell = Arc::clone(&cell);
      thread::spawn(move || {
        for _ in 0..10_000 {
          cell.with(|value| *value += 1);
        }
      })
    })
    .collect();
  for worker in workers {
    worker.join().expect("a worker");
  }

  assert_eq!(cell.with(|value| *value), 40_000);
}
