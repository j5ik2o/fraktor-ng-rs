//! The event bus that tests every classifier against the event.

use alloc::boxed::Box;
use alloc::vec::Vec;

/// The step that decides whether one classifier takes one event.
type Matches<C, E> = Box<dyn Fn(&C, &E) -> bool>;

/// The step that hands one event to one subscriber.
type Deliver<E, S> = Box<dyn Fn(&E, &S)>;

/// An event bus that scans every classifier when it publishes.
///
/// Mirrors `org.apache.pekko.event.ScanningClassification`: the table holds the
/// same classifier-and-subscriber pairs as a lookup bus, and `matches` decides
/// for each pair whether it receives the event. The port scans in insertion
/// order, which is the order the ported cases observe.
pub struct ScanningEventBus<E, C: PartialEq, S: PartialEq> {
  matches: Matches<C, E>,
  publish: Deliver<E, S>,
  subscribers: Vec<(C, S)>,
}

impl<E, C: PartialEq, S: PartialEq> ScanningEventBus<E, C, S> {
  /// Creates a bus that matches with `matches` and delivers with `publish`.
  pub fn new<F, P>(matches: F, publish: P) -> Self
  where
    F: Fn(&C, &E) -> bool + 'static,
    P: Fn(&E, &S) + 'static,
  {
    Self { matches: Box::new(matches), publish: Box::new(publish), subscribers: Vec::new() }
  }

  /// Subscribes `subscriber` to `to`.
  ///
  /// Mirrors `EventBus.subscribe`: a new pair answers `true` and one the table
  /// already holds answers `false`.
  pub fn subscribe(&mut self, subscriber: S, to: C) -> bool {
    if self.subscribers.iter().any(|(classifier, existing)| classifier == &to && existing == &subscriber) {
      return false;
    }
    self.subscribers.push((to, subscriber));
    true
  }

  /// Removes the subscription of `subscriber` from `from`.
  ///
  /// Mirrors `EventBus.unsubscribe`: a pair the table holds answers `true` and
  /// one it does not hold answers `false`.
  pub fn unsubscribe(&mut self, subscriber: &S, from: &C) -> bool {
    match self.subscribers.iter().position(|(classifier, existing)| classifier == from && existing == subscriber) {
      | Some(index) => {
        self.subscribers.remove(index);
        true
      },
      | None => false,
    }
  }

  /// Publishes `event` to every subscriber whose classifier matches it.
  pub fn publish(&mut self, event: &E) {
    for (classifier, subscriber) in &self.subscribers {
      if (self.matches)(classifier, event) {
        (self.publish)(event, subscriber);
      }
    }
  }
}
