//! The message that adds an actor to a broadcaster's subscriber set.

use crate::kernel::actor_ref::ActorRef;

/// Asks the broadcaster to add its actor to the set it delivers to.
///
/// Mirrors `org.apache.pekko.routing.Listen`. The actor carried by the message
/// is the one that starts receiving; the broadcaster that handles the message
/// owns the set. Registering the same actor twice leaves one entry.
pub struct Subscribe {
  actor: ActorRef,
}

impl Subscribe {
  /// Creates a message that adds `actor` to the set of the broadcaster it
  /// reaches.
  pub fn new(actor: ActorRef) -> Self {
    Self { actor }
  }

  /// Returns the actor to add.
  pub fn actor(&self) -> &ActorRef {
    &self.actor
  }
}
