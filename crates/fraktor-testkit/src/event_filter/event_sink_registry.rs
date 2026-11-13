//! The sink that a spec attaches to its system, and the filters a test arms.

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::MutexGuard;
use std::vec::Vec;

use fraktor_actor::kernel::actor::Logging::EventSink;
use fraktor_actor::kernel::actor::Logging::LogEvent;

use crate::event_filter::EventFilter;

/// Publishes events to the armed filters that a spec has registered.
///
/// Mirrors the part of `org.apache.pekko.testkit.EventFilter` that mutes the
/// events an intercept registered: Pekko installs a filter on the event stream
/// and counts the events that match it. The port keeps the same registry, keyed
/// by nothing but "the filters armed on this spec", because a spec owns one
/// system and the ported tests arm one filter at a time.
///
/// A spec hands an `Arc` of this to its system; a test arms a filter on it for
/// the duration of `intercept` and reads the count afterwards.
pub struct EventSinkRegistry {
  armed: Mutex<Vec<ArmedFilter>>,
}

/// One filter that an `intercept` armed, with the events it matched.
struct ArmedFilter {
  filter: EventFilter,
  matched: usize,
}

impl EventSinkRegistry {
  /// Creates an empty registry.
  pub fn new() -> Self {
    Self { armed: Mutex::new(Vec::new()) }
  }

  /// Arms `filter` and returns its index and a shared handle to the registry.
  ///
  /// The caller keeps the index, runs the test body, and reads the count with
  /// [`EventSinkRegistry::matched`].
  pub fn arm(&self, filter: EventFilter) -> usize {
    let mut armed = self.lock();
    armed.push(ArmedFilter { filter, matched: 0 });
    armed.len() - 1
  }

  /// Removes the filter armed at `index`.
  pub fn disarm(&self, index: usize) {
    let mut armed = self.lock();
    if index < armed.len() {
      armed.remove(index);
    }
  }

  /// Returns how many events the filter armed at `index` has matched.
  pub fn matched(&self, index: usize) -> usize {
    self.lock().get(index).map(|armed| armed.matched).unwrap_or(0)
  }

  fn lock(&self) -> MutexGuard<'_, Vec<ArmedFilter>> {
    self.armed.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
  }
}

impl EventSink for EventSinkRegistry {
  fn publish(&self, event: &dyn LogEvent) {
    let mut armed = self.lock();
    for entry in armed.iter_mut() {
      if entry.filter.apply(event) {
        entry.matched += 1;
      }
    }
  }

  fn into_any(self: Arc<Self>) -> Arc<dyn core::any::Any + Send + Sync> {
    self
  }
}

impl Default for EventSinkRegistry {
  fn default() -> Self {
    Self::new()
  }
}
