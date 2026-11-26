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

//! Unit tests for the crate-private pieces of the switch (outside the
//! original SwitchSpec): the revert guard reverts on drop and keeps the
//! change once defused.

use core::sync::atomic::{AtomicBool, Ordering};

use super::super::revert_guard::RevertGuard;

#[test]
fn revert_guard_reverts_on_drop() {
  let on = AtomicBool::new(true);
  {
    let _guard = RevertGuard::new(&on, false);
  }
  assert!(!on.load(Ordering::SeqCst), "the guard reverted the flipped state back to `from`");
}

#[test]
fn revert_guard_keeps_the_change_once_defused() {
  let on = AtomicBool::new(true);
  {
    let mut guard = RevertGuard::new(&on, false);
    guard.defuse();
  }
  assert!(on.load(Ordering::SeqCst), "a defused guard leaves the state alone");
}
