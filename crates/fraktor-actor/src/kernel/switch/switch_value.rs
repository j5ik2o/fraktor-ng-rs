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

//! The switch itself: one atomic state and one lock.

use core::sync::atomic::{AtomicBool, Ordering};

use spin::Mutex;

use super::{locked_switch::LockedSwitch, revert_guard::RevertGuard};

#[cfg(test)]
#[path = "switch_value_test.rs"]
mod tests;

/// An atomic switch that can be either on or off.
///
/// Mirrors `org.apache.pekko.util.Switch` (in the original's
/// `LockUtil.scala`): the state is an `AtomicBoolean`, and the operations
/// the original marks as locking synchronize on the switch itself. The
/// port keeps the state in the single `AtomicBool` and uses a
/// `spin::Mutex<()>` purely as the lock, so the immediate operations
/// (`is_on`, `if_on`, and the other `if` forms) never take it.
///
/// Calling a locking operation of the same switch from inside
/// [`Switch::locked`] deadlocks, because the port's lock is not
/// reentrant; use the [`LockedSwitch`] window that `locked` hands to its
/// closure instead. The original's monitor is reentrant, so this is a
/// deliberate difference, and on a `no_std` target the owning thread
/// cannot be identified to prevent it in the type.
pub struct Switch {
  on: AtomicBool,
  lock: Mutex<()>,
}

impl Switch {
  /// Creates a switch that starts on if `start_as_on` is true, and off
  /// otherwise.
  pub fn new(start_as_on: bool) -> Self {
    Self { on: AtomicBool::new(start_as_on), lock: Mutex::new(()) }
  }

  /// Returns whether the switch is immediately on (no locking).
  pub fn is_on(&self) -> bool {
    self.on.load(Ordering::SeqCst)
  }

  /// Returns whether the switch is immediately off (no locking).
  pub fn is_off(&self) -> bool {
    !self.is_on()
  }

  /// Switches the switch on if it is off, under the lock. Returns whether
  /// the state changed.
  pub fn switch_on(&self) -> bool {
    let _lock = self.lock.lock();
    self.on.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_ok()
  }

  /// Switches the switch off if it is on, under the lock. Returns whether
  /// the state changed.
  pub fn switch_off(&self) -> bool {
    let _lock = self.lock.lock();
    self.on.compare_exchange(true, false, Ordering::SeqCst, Ordering::SeqCst).is_ok()
  }

  /// Switches the switch on and runs `action` under the lock, if it was
  /// off. Returns `Ok(true)` if the state changed and the action ran,
  /// `Ok(false)` if the switch was already on (the action does not run).
  ///
  /// If the action returns `Err` or panics, the state is reverted to off
  /// while the lock is still held, as the original reverts on any
  /// `Throwable`.
  pub fn switch_on_with<E>(&self, action: impl FnOnce() -> Result<(), E>) -> Result<bool, E> {
    let _lock = self.lock.lock();
    self.transcend(false, action)
  }

  /// Switches the switch off and runs `action` under the lock, if it was
  /// on. The mirror of [`Switch::switch_on_with`].
  pub fn switch_off_with<E>(&self, action: impl FnOnce() -> Result<(), E>) -> Result<bool, E> {
    let _lock = self.lock.lock();
    self.transcend(true, action)
  }

  // The original's `transcend`: flip the state with a CAS, run the action,
  // and let the revert guard (declared after the caller's lock guard, so
  // it drops first) put the state back unless the action succeeded.
  pub(super) fn transcend<E>(&self, from: bool, action: impl FnOnce() -> Result<(), E>) -> Result<bool, E> {
    if self.on.compare_exchange(from, !from, Ordering::SeqCst, Ordering::SeqCst).is_err() {
      return Ok(false);
    }
    let mut guard = RevertGuard::new(&self.on, from);
    action()?;
    guard.defuse();
    Ok(true)
  }

  /// Runs `action` and returns whether it ran, if the switch is
  /// immediately on (no locking).
  pub fn if_on(&self, action: impl FnOnce()) -> bool {
    if self.is_on() {
      action();
      true
    } else {
      false
    }
  }

  /// Runs `action` and returns whether it ran, if the switch is
  /// immediately off (no locking).
  pub fn if_off(&self, action: impl FnOnce()) -> bool {
    if self.is_off() {
      action();
      true
    } else {
      false
    }
  }

  /// Runs `action` and returns its value, if the switch is immediately on
  /// (no locking).
  pub fn if_on_yield<T>(&self, action: impl FnOnce() -> T) -> Option<T> {
    if self.is_on() {
      Some(action())
    } else {
      None
    }
  }

  /// Runs `action` and returns its value, if the switch is immediately
  /// off (no locking).
  pub fn if_off_yield<T>(&self, action: impl FnOnce() -> T) -> Option<T> {
    if self.is_off() {
      Some(action())
    } else {
      None
    }
  }

  /// Runs `action` under the lock and returns whether it ran, if the
  /// switch is on.
  pub fn while_on(&self, action: impl FnOnce()) -> bool {
    let _lock = self.lock.lock();
    if self.is_on() {
      action();
      true
    } else {
      false
    }
  }

  /// Runs `action` under the lock and returns whether it ran, if the
  /// switch is off.
  pub fn while_off(&self, action: impl FnOnce()) -> bool {
    let _lock = self.lock.lock();
    if self.is_off() {
      action();
      true
    } else {
      false
    }
  }

  /// Runs `action` under the lock and returns its value, if the switch is
  /// on.
  pub fn while_on_yield<T>(&self, action: impl FnOnce() -> T) -> Option<T> {
    let _lock = self.lock.lock();
    if self.is_on() {
      Some(action())
    } else {
      None
    }
  }

  /// Runs `action` under the lock and returns its value, if the switch is
  /// off.
  pub fn while_off_yield<T>(&self, action: impl FnOnce() -> T) -> Option<T> {
    let _lock = self.lock.lock();
    if self.is_off() {
      Some(action())
    } else {
      None
    }
  }

  /// Runs `on` or `off` under the lock, depending on the state, and
  /// returns its value.
  pub fn fold<T>(&self, on: impl FnOnce() -> T, off: impl FnOnce() -> T) -> T {
    let _lock = self.lock.lock();
    if self.is_on() {
      on()
    } else {
      off()
    }
  }

  /// Runs `code` while holding this switch's lock, protected from
  /// concurrent modification of the switch status, and returns its value.
  ///
  /// The closure receives a [`LockedSwitch`] window that reads and
  /// changes the state without taking the lock again; that window is how
  /// the original's reentrant monitor is expressed in the port. Calling a
  /// locking operation on this switch itself inside `code` deadlocks.
  pub fn locked<T>(&self, code: impl FnOnce(&LockedSwitch<'_>) -> T) -> T {
    let _lock = self.lock.lock();
    let held = LockedSwitch::new(self);
    code(&held)
  }

  // The state cell, for the locked window.
  pub(super) fn state(&self) -> &AtomicBool {
    &self.on
  }
}
