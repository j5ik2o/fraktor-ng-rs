//! The clock and registration table shared by a scheduler and its tasks.

use core::time::Duration;

use std::collections::BTreeMap;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::Mutex;
use std::sync::MutexGuard;

use super::registration::Registration;
use super::registration_id::RegistrationId;

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
  deadline: Duration,
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

impl Drop for AdvanceGuard<'_> {
  fn drop(&mut self) {
    self.shared.advancing.store(false, Ordering::SeqCst);
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
    inner.registrations.insert(id, Entry { deadline, registration });
    id
  }

  /// Stores a registration that has just run under its existing identifier at
  /// its next `deadline`, keeping the cancel handle of that registration valid.
  pub(crate) fn insert_at(&self, id: RegistrationId, registration: Registration, deadline: Duration) {
    self.lock().registrations.insert(id, Entry { deadline, registration });
  }

  /// Returns the earliest registration due at or before `target`, if any.
  pub(crate) fn next_due_at_or_before(&self, target: Duration) -> Option<DueRegistration> {
    let inner = self.lock();
    inner
      .registrations
      .iter()
      .filter(|(_, entry)| entry.deadline <= target)
      .map(|(id, entry)| (entry.deadline, *id, entry.registration.interval()))
      .min_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)))
      .map(|(deadline, id, interval)| DueRegistration { id, deadline, interval })
  }

  /// Removes the registration with `id` and returns its callback.
  pub(crate) fn take_callback(&self, id: RegistrationId) -> Option<super::Callback> {
    self.lock().registrations.remove(&id).map(|entry| entry.registration.callback())
  }

  /// Acquires an advance and ties its release to the returned guard.
  ///
  /// Returns `None` when another advance is already running, which happens when
  /// a callback calls `time_passes` again. The scheduler refuses that re-entry
  /// so the clock cannot move backwards underneath the outer advance.
  pub(crate) fn begin_advance(&self) -> Option<AdvanceGuard<'_>> {
    self.advancing.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).ok()?;
    Some(AdvanceGuard { shared: self })
  }

  /// Checks every pending repetition through `target` before any callback runs.
  pub(crate) fn validate_advance(&self, target: Duration) {
    let inner = self.lock();
    for entry in inner.registrations.values().filter(|entry| entry.deadline <= target) {
      if let Some(interval) = entry.registration.interval() {
        // Subtract the remainder to find the last due time without walking
        // every repetition. Nanoseconds fit in u128 even at Duration::MAX.
        let remainder = (target - entry.deadline).as_nanos() % interval.as_nanos();
        let last = target.as_nanos() - remainder;
        assert!(
          last + interval.as_nanos() <= Duration::MAX.as_nanos(),
          "the next deadline of a repeating registration must fit the time range"
        );
      }
    }
  }

  /// Removes the registration with `id` because it was cancelled, returning
  /// whether one was present.
  pub(crate) fn remove(&self, id: RegistrationId) -> bool {
    self.lock().registrations.remove(&id).is_some()
  }

  fn lock(&self) -> MutexGuard<'_, Inner> {
    self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
  }
}
