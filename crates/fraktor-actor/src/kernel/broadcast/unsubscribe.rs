//! The message that removes an actor from a broadcaster's subscriber set.

use crate::kernel::actor_ref::ActorRef;

/// Asks the broadcaster to remove its actor from the set it delivers to.
///
/// Mirrors `org.apache.pekko.routing.Deafen`. The removal takes the entry for
/// the actor out of the set; it does not stop the actor, which stays alive and
/// simply receives no further deliveries.
pub struct Unsubscribe {
  actor: ActorRef,
}

impl Unsubscribe {
  /// Creates a message that removes `actor` from the set of the broadcaster it
  /// reaches.
  pub fn new(actor: ActorRef) -> Self {
    Self { actor }
  }

  /// Returns the actor to remove.
  pub fn actor(&self) -> &ActorRef {
    &self.actor
  }
}
