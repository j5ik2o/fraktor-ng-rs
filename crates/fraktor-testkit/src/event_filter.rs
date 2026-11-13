//! The predicate that a test uses to mute an expected log event.

mod event_filter_value;
mod event_sink_registry;

pub use event_filter_value::EventFilter;
pub use event_sink_registry::EventSinkRegistry;

use std::sync::Arc;

use fraktor_actor::kernel::actor_system::ActorSystem;

impl EventFilter {
  /// Runs `body` while muting the events this filter matches, then checks that
  /// exactly one of them was published.
  ///
  /// Mirrors `EventFilter.intercept`, which installs the filter on the event
  /// stream, runs the body, and fails when the number of matching events differs
  /// from the expected count. The ported tests all expect one occurrence, which
  /// is Pekko's `occurrences = 1`; the port arms the filter on the registry the
  /// system was created with, drives the system to completion, reads the count,
  /// and disarms the filter.
  ///
  /// # Panics
  ///
  /// Panics when the system has no event sink, or when the number of matching
  /// events after the body differs from one.
  pub fn intercept<R>(&self, system: &mut ActorSystem, body: impl FnOnce() -> R) -> R {
    let registry = registry_of(system).expect("the actor system has no event sink to intercept");
    let index = registry.arm(self.clone());
    let outcome = body();
    system.run_until_idle();
    let matched = registry.matched(index);
    registry.disarm(index);
    assert_eq!(matched, 1, "expected 1 event matching {self:?} but found {matched}");
    outcome
  }
}

/// Returns the registry the system publishes to, if any.
fn registry_of(system: &ActorSystem) -> Option<Arc<EventSinkRegistry>> {
  let sink = system.event_sink()?;
  sink.into_any().downcast::<EventSinkRegistry>().ok()
}
