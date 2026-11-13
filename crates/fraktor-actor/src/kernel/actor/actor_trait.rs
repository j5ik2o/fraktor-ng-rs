//! The behaviour of an actor.

use crate::kernel::actor::ActorContext;
use crate::kernel::message::Message;

/// The behaviour that an actor shows when a message reaches it.
///
/// Mirrors `org.apache.pekko.actor.Actor.receive`, which Pekko invokes through
/// a partial function. The port makes it a method so that the executor can call
/// it with the actor's context lent out for the duration of the call.
///
/// Pekko actors may be re-created after a failure, so the port also offers
/// `post_restart`, which a parent runs on the new instance.
pub trait Actor {
  /// Handles one message.
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message);

  /// Runs after the actor has been re-created following a failure.
  ///
  /// Mirrors `org.apache.pekko.actor.Actor.postRestart(reason)`, without the
  /// cause because the port does not model the failure as a value that
  /// crosses the executor. Does nothing by default, as in Pekko.
  fn post_restart(&mut self) {}
}
