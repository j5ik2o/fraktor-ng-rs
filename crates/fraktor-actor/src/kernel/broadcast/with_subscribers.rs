//! The message that applies an operation to each current subscriber.

use crate::kernel::actor_ref::ActorRef;

/// Runs a closure once for each actor in the broadcaster's subscriber set.
///
/// Mirrors `org.apache.pekko.routing.WithListeners`. The closure decides what
/// to do with each subscriber, so the test names the operation while the
/// broadcaster owns the set. The closure must be `Send + Sync + 'static`
/// because it travels as a message payload and outlives the call that sends it.
pub struct WithSubscribers {
  operation: Box<dyn Fn(&ActorRef) + Send + Sync + 'static>,
}

impl WithSubscribers {
  /// Creates a message that applies `operation` to each subscriber.
  pub fn new<F>(operation: F) -> Self
  where
    F: Fn(&ActorRef) + Send + Sync + 'static,
  {
    Self { operation: Box::new(operation) }
  }

  pub(crate) fn apply(&self, subscriber: &ActorRef) {
    (self.operation)(subscriber);
  }
}
