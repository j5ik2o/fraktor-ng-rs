//! The behaviour of an actor.

use core::any::Any;

use crate::kernel::actor::ActorContext;
use crate::kernel::message::Message;

/// The behaviour that an actor shows when a message reaches it.
///
/// Mirrors `org.apache.pekko.actor.Actor.receive`, which Pekko invokes through
/// a partial function. The port makes it a method so that the executor can call
/// it with the actor's context lent out for the duration of the call.
///
/// The lifecycle hooks mirror `Actor.preStart`, `Actor.postStop`,
/// `Actor.preRestart`, and `Actor.postRestart`. Pekko's defaults are kept:
/// `pre_restart` runs `post_stop` unless an actor overrides it, and
/// `post_restart` runs `pre_start` unless an actor overrides it.
pub trait Actor {
  /// Handles one message.
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message);

  /// Runs after the actor has been started and before its first message.
  ///
  /// Mirrors `org.apache.pekko.actor.Actor.preStart`, which does nothing by
  /// default.
  fn pre_start(&mut self) {}

  /// Runs after the actor has stopped.
  ///
  /// Mirrors `org.apache.pekko.actor.Actor.postStop`, which does nothing by
  /// default.
  fn post_stop(&mut self) {}

  /// Runs on the old instance before it is replaced by a restart.
  ///
  /// Mirrors `org.apache.pekko.actor.Actor.preRestart`. Pekko's default stops
  /// the children and then runs `postStop`; this port has no child-stopping
  /// step at this point, so the default only runs `post_stop`. The `reason` is
  /// the cause of the failure that triggered the restart, given as the port's
  /// error event, which an actor can downcast when it needs the cause.
  fn pre_restart(&mut self, reason: &(dyn Any + Send)) {
    let _ = reason;
    self.post_stop();
  }

  /// Runs on the new instance after a restart re-created the actor.
  ///
  /// Mirrors `org.apache.pekko.actor.Actor.postRestart`, whose default runs
  /// `preStart`.
  fn post_restart(&mut self) {
    self.pre_start();
  }
}
