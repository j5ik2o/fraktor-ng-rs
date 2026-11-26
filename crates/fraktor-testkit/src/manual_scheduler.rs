// SPDX-License-Identifier: Apache-2.0
//
// Licensed to the Apache Software Foundation (ASF) under one or more
// license agreements; and to You under the Apache License, version 2.0:
//
//   https://www.apache.org/licenses/LICENSE-2.0
//
// This file is part of the Apache Pekko project, which was derived from Akka.
//
// Copyright (C) 2018-2022 Lightbend Inc. <https://www.lightbend.com>
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.

//! A scheduler that a test advances by hand instead of by the wall clock.
//!
//! Mirrors `org.apache.pekko.testkit.ExplicitlyTriggeredScheduler`: time only
//! moves when `time_passes` is called, and every task whose deadline is at or
//! before the new time runs on the calling thread before `time_passes` returns.
//! The port keeps Pekko's guarantee that a zero initial delay runs during
//! `schedule`, and drops Pekko's real-time `Thread.sleep`, so a test never waits
//! for the wall clock.

mod manual_scheduler_task;
mod registration;
mod registration_id;
mod shared_state;

pub use manual_scheduler_task::ManualSchedulerTask;

#[cfg(test)]
#[path = "manual_scheduler_test.rs"]
mod tests;

use core::time::Duration;

use std::sync::Arc;

use registration::Registration;
use shared_state::SharedState;

/// A callback the scheduler runs when a registration becomes due.
type Callback = Arc<dyn Fn() + Send + Sync>;

/// Drives scheduled callbacks from a clock that only a test moves.
///
/// A `ManualScheduler` owns the current time and the table of registrations.
/// `schedule` registers a repeating callback and `schedule_once` a one-shot
/// callback; `time_passes` advances the clock and runs every callback that came
/// due, in deadline order, on the calling thread.
///
/// # Synchronous execution
///
/// A callback always runs on the thread that calls `schedule` or `time_passes`,
/// and `time_passes` returns only after every callback due within the advance
/// has finished. A test therefore observes the result of every callback as soon
/// as `time_passes` returns, without waiting for another thread.
///
/// # Shared clock and table
///
/// The scheduler keeps one current time and one registration table behind an
/// internal mutex. The [`ManualSchedulerTask`] handles reach that same table
/// through a weak reference so a cancellation acts on the registration the
/// scheduler is about to run. A callback runs outside the lock, so a callback
/// that cancels another registration, or schedules new work, does not deadlock;
/// a registration cancelled earlier in the same advance is skipped instead of
/// run.
///
/// # Driving the scheduler
///
/// Drive one scheduler from one thread. Concurrent calls to `schedule`,
/// `schedule_once`, and `time_passes` on the same scheduler are unsupported.
/// Cancellation handles can be used from other threads.
///
/// A callback may register more work, but calling `time_passes` from a callback
/// panics. The scheduler re-reads the earliest deadline after each callback,
/// so work due within the current advance runs before `time_passes` returns.
///
/// # Refused registrations
///
/// `schedule` and `schedule_once` panic before touching the state when the
/// first deadline would overflow the time range. `schedule` also rejects a zero
/// interval or an immediate repeat whose next deadline would overflow.
/// `time_passes` panics before running any callback
/// when the advance overflows the current time, or when a registration pending
/// at the start would overflow a reschedule during that advance. All its due
/// repetitions are checked before the clock or table changes. Work created by
/// callbacks is checked before its own execution; effects of callbacks that
/// have already run are not rolled back.
///
/// # Cancellation
///
/// `schedule` and `schedule_once` return a [`ManualSchedulerTask`] that cancels
/// that one registration. Cancellation acts on the registration, not on the
/// callback, so the same callback scheduled twice yields two tasks that cancel
/// independently. Cancelling an executing repeat prevents its future runs.
///
/// # Panics in the callback
///
/// A panic in a callback unwinds out of `time_passes` or `schedule`. A
/// repeating callback that panics is not rescheduled, because the scheduler
/// re-registers it only after the callback returns normally. An unwinding
/// `time_passes` releases the advance so the scheduler can be advanced again;
/// callback effects and the clock at its deadline are not rolled back.
pub struct ManualScheduler {
  shared: Arc<SharedState>,
}

impl ManualScheduler {
  /// Creates a scheduler whose clock starts at zero with no registrations.
  pub fn new() -> Self {
    Self { shared: Arc::new(SharedState::new()) }
  }

  /// Registers `callback` to run every `interval` after `initial_delay`.
  ///
  /// When `initial_delay` is zero the callback runs during this call, before it
  /// returns. A positive delay runs on the first `time_passes` that reaches the
  /// deadline. The callback runs again every `interval` thereafter.
  ///
  /// # Panics
  ///
  /// Panics when `interval` is zero or a deadline computed during registration
  /// overflows the time range. Neither leaves a registration behind.
  pub fn schedule(&self, initial_delay: Duration, interval: Duration, callback: Callback) -> ManualSchedulerTask {
    self.register(initial_delay, Registration::repeating(interval, callback))
  }

  /// Registers `callback` to run once after `delay`.
  ///
  /// When `delay` is zero the callback runs during this call, before it
  /// returns. Otherwise it runs on the first `time_passes` that reaches the
  /// deadline, and never again.
  ///
  /// # Panics
  ///
  /// Panics when the deadline overflows the time range, leaving no
  /// registration behind.
  pub fn schedule_once(&self, delay: Duration, callback: Callback) -> ManualSchedulerTask {
    self.register(delay, Registration::once(callback))
  }

  /// Advances the clock by `amount` and runs every callback that comes due.
  ///
  /// Callbacks run in deadline order, and the clock is set to each callback's
  /// deadline before it runs, so a callback that schedules another observes the
  /// time it was scheduled for. After the last due callback runs, the clock is
  /// set to the new time. When `amount` is zero this runs every callback whose
  /// deadline equals the current time.
  ///
  /// # Panics
  ///
  /// Panics before running any callback when the advance overflows the current
  /// time, or when any due repetition of a registration pending at the start
  /// would overflow its next deadline. These checks leave the clock and
  /// registrations unchanged. Work created by callbacks is checked before its
  /// own execution. Also panics if a scheduled callback is already executing.
  pub fn time_passes(&self, amount: Duration) {
    let _advance = self.shared.begin_advance().expect("time_passes must not be called from a scheduled callback");
    let target = add("the advanced time", self.shared.now(), amount);
    self.shared.validate_advance(target);
    self.drain_until(target);
    self.shared.set_now(target);
  }

  fn register(&self, delay: Duration, registration: Registration) -> ManualSchedulerTask {
    let deadline = add("the first deadline of a registration", self.shared.now(), delay);
    let immediate = delay.is_zero();
    let next_deadline = registration.interval().filter(|_| immediate).map(|interval| next_repeat(deadline, interval));

    let id = self.shared.insert(registration, deadline);
    if immediate {
      let _advance = self.shared.begin_advance();
      if let Some(running) = self.shared.start(id) {
        running.run(next_deadline);
      }
    }
    ManualSchedulerTask::new(Arc::downgrade(&self.shared), id)
  }

  fn drain_until(&self, target: Duration) {
    // Run each due callback in turn, re-reading the earliest due registration
    // after every callback so a repeating registration that reschedules itself,
    // or a callback that schedules new work, is picked up. The callback runs
    // outside the table lock, and a callback that cancels another registration
    // makes it disappear from the next read, so it is skipped without running.
    while let Some(due) = self.shared.next_due_at_or_before(target) {
      // Validate the reschedule before the callback runs, so a repeating
      // registration that cannot advance its deadline refuses here without
      // running and without touching the clock.
      let next_deadline = due.interval().map(|interval| next_repeat(due.deadline(), interval));
      if let Some(running) = self.shared.start(due.id()) {
        self.shared.set_now(due.deadline());
        running.run(next_deadline);
      }
    }
  }
}

impl Default for ManualScheduler {
  fn default() -> Self {
    Self::new()
  }
}

fn add(what: &str, now: Duration, amount: Duration) -> Duration {
  now.checked_add(amount).unwrap_or_else(|| panic!("{what} must fit the time range"))
}

fn next_repeat(deadline: Duration, interval: Duration) -> Duration {
  add("the next deadline of a repeating registration", deadline, interval)
}
