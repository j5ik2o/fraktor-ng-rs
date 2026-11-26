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

//! The clock and registration table shared by a scheduler and its tasks.

use core::time::Duration;

use std::collections::BTreeMap;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::Mutex;
use std::sync::MutexGuard;

use super::registration::Registration;
use super::registration_id::RegistrationId;
use super::Callback;

/// The scheduler's current time and the registrations keyed by identifier.
///
/// The state is shared between the [`ManualScheduler`] that drives it and the
/// [`ManualSchedulerTask`] handles that cancel through it. A mutex keeps the
/// clock and table consistent, matching Pekko's concurrent map for the surface
/// the tests use; the lock guards only table operations, never a callback, so a
/// callback that cancels another registration cannot deadlock.
///
/// [`ManualScheduler`]: super::ManualScheduler
/// [`ManualSchedulerTask`]: super::ManualSchedulerTask
pub(crate) struct SharedState {
  inner: Mutex<Inner>,
  advancing: AtomicBool,
}

struct Inner {
  now: Duration,
  next_id: RegistrationId,
  registrations: BTreeMap<RegistrationId, Entry>,
}

struct Entry {
  // A running repeat stays in the table for cancellation but is not due.
  deadline: Option<Duration>,
  registration: Registration,
}

/// One registration that has come due, with the identifier and deadline it runs
/// under.
pub(crate) struct DueRegistration {
  id: RegistrationId,
  deadline: Duration,
  interval: Option<Duration>,
}

/// Releases an advance only for the caller that acquired it, including unwind.
pub(crate) struct AdvanceGuard<'a> {
  shared: &'a SharedState,
}

/// Keeps a running registration cancellable and clears it if the callback panics.
pub(crate) struct RunningRegistration<'a> {
  shared: &'a SharedState,
  id: RegistrationId,
  callback: Callback,
}

impl Drop for AdvanceGuard<'_> {
  fn drop(&mut self) {
    self.shared.advancing.store(false, Ordering::SeqCst);
  }
}

impl RunningRegistration<'_> {
  /// Runs the callback and restores the next deadline if the repeat is still registered.
  pub(crate) fn run(self, next_deadline: Option<Duration>) {
    (self.callback)();
    if let Some(deadline) = next_deadline {
      if let Some(entry) = self.shared.lock().registrations.get_mut(&self.id) {
        entry.deadline = Some(deadline);
      }
    }
  }
}

impl Drop for RunningRegistration<'_> {
  fn drop(&mut self) {
    let retired = {
      let mut inner = self.shared.lock();
      let still_running = inner.registrations.get(&self.id).is_some_and(|entry| entry.deadline.is_none());
      if still_running {
        inner.registrations.remove(&self.id)
      } else {
        None
      }
    };
    drop(retired);
  }
}

impl DueRegistration {
  /// Returns the identifier of this registration.
  pub(crate) fn id(&self) -> RegistrationId {
    self.id
  }

  /// Returns the deadline this registration runs at.
  pub(crate) fn deadline(&self) -> Duration {
    self.deadline
  }

  /// Returns the interval if the registration repeats.
  pub(crate) fn interval(&self) -> Option<Duration> {
    self.interval
  }
}

impl SharedState {
  /// Creates the state with a zero clock and no registrations.
  pub(crate) fn new() -> Self {
    Self {
      inner: Mutex::new(Inner {
        now: Duration::ZERO,
        next_id: RegistrationId::default(),
        registrations: BTreeMap::new(),
      }),
      advancing: AtomicBool::new(false),
    }
  }

  /// Returns the current time.
  pub(crate) fn now(&self) -> Duration {
    self.lock().now
  }

  /// Sets the current time.
  pub(crate) fn set_now(&self, now: Duration) {
    self.lock().now = now;
  }

  /// Stores `registration` at `deadline` and returns the identifier it now has.
  pub(crate) fn insert(&self, registration: Registration, deadline: Duration) -> RegistrationId {
    let mut inner = self.lock();
    let id = inner.next_id;
    inner.next_id = id.next();
    inner.registrations.insert(id, Entry { deadline: Some(deadline), registration });
    id
  }

  /// Returns the earliest registration due at or before `target`, if any.
  pub(crate) fn next_due_at_or_before(&self, target: Duration) -> Option<DueRegistration> {
    let inner = self.lock();
    inner
      .registrations
      .iter()
      .filter_map(|(id, entry)| Some((entry.deadline?, *id, entry.registration.interval())))
      .filter(|(deadline, _, _)| *deadline <= target)
      .min_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)))
      .map(|(deadline, id, interval)| DueRegistration { id, deadline, interval })
  }

  /// Claims a registration for execution and removes one-shots from the table.
  pub(crate) fn start(&self, id: RegistrationId) -> Option<RunningRegistration<'_>> {
    let mut inner = self.lock();
    let entry = inner.registrations.get_mut(&id)?;
    entry.deadline.take()?;
    let callback = entry.registration.callback();
    if entry.registration.interval().is_none() {
      inner.registrations.remove(&id);
    }
    Some(RunningRegistration { shared: self, id, callback })
  }

  /// Acquires an advance and ties its release to the returned guard.
  ///
  /// Returns `None` when a clock advance or an immediate callback already owns
  /// execution. The clock cannot advance again until that owner returns.
  pub(crate) fn begin_advance(&self) -> Option<AdvanceGuard<'_>> {
    self.advancing.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).ok()?;
    Some(AdvanceGuard { shared: self })
  }

  /// Checks every pending repetition through `target` before any callback runs.
  pub(crate) fn validate_advance(&self, target: Duration) {
    let inner = self.lock();
    for entry in inner.registrations.values() {
      if let (Some(deadline), Some(interval)) = (entry.deadline, entry.registration.interval()) {
        let Some(elapsed) = target.checked_sub(deadline) else { continue };
        // Subtract the remainder to find the last due time without walking
        // every repetition. Nanoseconds fit in u128 even at Duration::MAX.
        let remainder = elapsed.as_nanos() % interval.as_nanos();
        let last = target.as_nanos() - remainder;
        assert!(
          last + interval.as_nanos() <= Duration::MAX.as_nanos(),
          "the next deadline of a repeating registration must fit the time range"
        );
      }
    }
  }

  /// Removes a pending or running registration and drops it outside the lock.
  pub(crate) fn remove(&self, id: RegistrationId) -> bool {
    let removed = self.lock().registrations.remove(&id);
    removed.is_some()
  }

  fn lock(&self) -> MutexGuard<'_, Inner> {
    self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
  }
}
