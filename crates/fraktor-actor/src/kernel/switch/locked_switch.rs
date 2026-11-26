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

//! The window into a switch whose lock the caller already holds.

use core::{marker::PhantomData, sync::atomic::Ordering};

use super::switch_value::Switch;

/// A window into a [`Switch`] whose lock is already held by the current
/// call, handed to the closure of [`Switch::locked`].
///
/// Every operation here works on the switch's single atomic state without
/// taking the lock again, which is how the port expresses the reentrancy
/// of the original's monitor. The window borrows the switch, so it cannot
/// be carried out of the `locked` call, and the marker field makes it
/// neither `Send` nor `Sync`: it cannot be handed to another thread, so a
/// change through the window always comes from the thread holding the
/// lock, as the Quint model in `crates/fraktor-actor/specs/switch.qnt`
/// assumes.
pub struct LockedSwitch<'a> {
  switch: &'a Switch,
  not_send_or_sync: PhantomData<*mut ()>,
}

impl<'a> LockedSwitch<'a> {
  pub(super) fn new(switch: &'a Switch) -> Self {
    Self { switch, not_send_or_sync: PhantomData }
  }

  /// Returns whether the switch is on.
  pub fn is_on(&self) -> bool {
    self.switch.is_on()
  }

  /// Returns whether the switch is off.
  pub fn is_off(&self) -> bool {
    self.switch.is_off()
  }

  /// Switches the switch on if it is off, without taking the lock.
  /// Returns whether the state changed.
  pub fn switch_on(&self) -> bool {
    self.switch.state().compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_ok()
  }

  /// Switches the switch off if it is on, without taking the lock.
  /// Returns whether the state changed.
  pub fn switch_off(&self) -> bool {
    self.switch.state().compare_exchange(true, false, Ordering::SeqCst, Ordering::SeqCst).is_ok()
  }

  /// Switches the switch on and runs `action`, if it was off, without
  /// taking the lock. If the action returns `Err` or panics, the state is
  /// reverted while the caller's lock is still held.
  pub fn switch_on_with<E>(&self, action: impl FnOnce() -> Result<(), E>) -> Result<bool, E> {
    self.switch.transcend(false, action)
  }

  /// Switches the switch off and runs `action`, if it was on, without
  /// taking the lock. The mirror of [`LockedSwitch::switch_on_with`].
  pub fn switch_off_with<E>(&self, action: impl FnOnce() -> Result<(), E>) -> Result<bool, E> {
    self.switch.transcend(true, action)
  }
}
