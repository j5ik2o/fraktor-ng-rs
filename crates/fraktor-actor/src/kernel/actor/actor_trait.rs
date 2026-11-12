//! The behaviour of an actor.

use crate::kernel::actor::ActorContext;
use crate::kernel::message::Message;

/// The behaviour that an actor shows when a message reaches it.
///
/// Mirrors `org.apache.pekko.actor.Actor.receive`, which Pekko invokes through
/// a partial function. The port makes it a method so that the executor can call
/// it with the actor's context lent out for the duration of the call.
pub trait Actor {
  /// Handles one message.
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message);
}
