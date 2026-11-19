//! The behaviour of an actor.

use core::any::Any;

use crate::kernel::actor::ActorContext;
use crate::kernel::message::Message;
use crate::kernel::supervision::SupervisorStrategy;

/// The behaviour that an actor shows when a message reaches it.
///
/// Mirrors `org.apache.pekko.actor.Actor.receive`, which Pekko invokes through
/// a partial function. The port makes it a method so that the executor can call
/// it with the actor's context lent out for the duration of the call.
///
/// The lifecycle hooks mirror `Actor.preStart`, `Actor.postStop`,
/// `Actor.preRestart`, and `Actor.postRestart`. Pekko's defaults are kept:
/// `pre_restart` stops all children and runs `post_stop` unless an actor overrides it, and
/// `post_restart` runs `pre_start` unless an actor overrides it.
pub trait Actor {
  /// Handles one message.
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message);

  /// Runs after the actor has been started and before its first message.
  ///
  /// Mirrors `org.apache.pekko.actor.Actor.preStart`, which does nothing by
  /// default. The context is lent so an actor may `change_behavior` here, which
  /// is what `context.become` in a Pekko constructor does: the behaviour is
  /// active before the first message arrives.
  fn pre_start(&mut self, context: &mut ActorContext<'_>) {
    let _ = context;
  }

  /// Runs after the actor has stopped.
  ///
  /// Mirrors `org.apache.pekko.actor.Actor.postStop`, which does nothing by
  /// default.
  fn post_stop(&mut self) {}

  /// Runs on the old instance before it is replaced by a restart.
  ///
  /// Mirrors `org.apache.pekko.actor.Actor.preRestart`. Pekko's default stops
  /// the children and then runs `postStop`. The `reason` is
  /// the cause of the failure that triggered the restart, given as the port's
  /// error event, which an actor can downcast when it needs the cause.
  fn pre_restart(&mut self, context: &mut ActorContext<'_>, reason: &(dyn Any + Send)) {
    let _ = reason;
    context.stop_children();
    self.post_stop();
  }

  /// Runs on the new instance after a restart re-created the actor.
  ///
  /// Mirrors `org.apache.pekko.actor.Actor.postRestart`, whose default runs
  /// `preStart`.
  fn post_restart(&mut self, context: &mut ActorContext<'_>) {
    self.pre_start(context);
  }

  /// Returns the strategy this actor applies to the children it starts.
  ///
  /// Mirrors `Actor.supervisorStrategy`, which a Pekko actor overrides to
  /// choose how it treats a failing child. The executor reads it right after it
  /// builds the instance from the recipe and before `pre_start` runs, so an
  /// override must build its strategy from the fields the recipe set, not from
  /// state `pre_start` installs. The default is
  /// `SupervisorStrategy.default_strategy`.
  fn supervisor_strategy(&self) -> SupervisorStrategy {
    SupervisorStrategy::default_strategy()
  }
}
