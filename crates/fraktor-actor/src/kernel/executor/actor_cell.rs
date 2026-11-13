//! One actor inside the executor, with the behaviour it runs.

use alloc::boxed::Box;
use core::any::Any;

use crate::kernel::actor::Actor;
use crate::kernel::actor::ActorContext;
use crate::kernel::actor::Props;
use crate::kernel::actor_ref::ActorRef;
use crate::kernel::message::Message;
use crate::kernel::supervision::SupervisorStrategy;

/// An actor that the executor owns and drives.
///
/// Mirrors the mailbox-and-actor pair that an `org.apache.pekko.actor.ActorCell`
/// keeps: the executor owns the behaviour, the recipe that created it, and the
/// handle, and lends the context out only while a message is being handled. The
/// recipe and the strategy are kept so the cell can be re-created after a
/// failure, as a parent does for its child.
pub(crate) struct ActorCell {
  behavior: Option<Box<dyn Actor>>,
  self_ref: ActorRef,
  parent: Option<ActorRef>,
  props: Props,
  strategy: SupervisorStrategy,
  restart_count: u32,
}

impl ActorCell {
  /// Creates a cell before the actor exists, so the executor can build the
  /// actor under a catcher that already belongs to a cell.
  pub(crate) fn pending(
    self_ref: ActorRef,
    parent: Option<ActorRef>,
    props: Props,
    strategy: SupervisorStrategy,
  ) -> Self {
    Self { behavior: None, self_ref, parent, props, strategy, restart_count: 0 }
  }

  /// Returns whether this cell is the actor that `recipient` addresses.
  pub(crate) fn accepts(&self, recipient: &ActorRef) -> bool {
    &self.self_ref == recipient
  }

  /// Returns the handle of the actor that this cell runs.
  pub(crate) fn self_ref(&self) -> ActorRef {
    self.self_ref.clone()
  }

  /// Returns whether this cell was started by `parent`.
  ///
  /// A child is told apart by the handle of its parent, not by the shape of its
  /// path: two different parents may give a child the same name, and a path
  /// prefix would then match the wrong actor.
  pub(crate) fn is_child_of(&self, parent: &ActorRef) -> bool {
    self.parent.as_ref() == Some(parent)
  }

  /// Returns the strategy this actor applies to its children.
  pub(crate) fn strategy(&self) -> &SupervisorStrategy {
    &self.strategy
  }

  /// Returns the number of restart requests this actor has already made.
  pub(crate) fn restart_count(&self) -> u32 {
    self.restart_count
  }

  /// Records one more restart request.
  pub(crate) fn record_restart_request(&mut self) {
    self.restart_count = self.restart_count.saturating_add(1);
  }

  /// Builds the behaviour from the recipe and runs its `pre_start`.
  pub(crate) fn create(&mut self) {
    let mut behavior = self.props.create_actor();
    behavior.pre_start();
    self.behavior = Some(behavior);
  }

  /// Hands one message to the behaviour.
  pub(crate) fn deliver(&mut self, context: &mut ActorContext<'_>, message: Message) {
    if let Some(behavior) = self.behavior.as_mut() {
      behavior.receive(context, message);
    }
  }

  /// Runs `pre_restart` on the current behaviour.
  pub(crate) fn pre_restart(&mut self, reason: &(dyn Any + Send)) {
    if let Some(behavior) = self.behavior.as_mut() {
      behavior.pre_restart(reason);
    }
  }

  /// Replaces the behaviour with a fresh instance from the recipe and runs its
  /// `pre_start`, which is what Pekko's default `postRestart` does.
  ///
  /// Mirrors what Pekko does for a restarted child: the old instance is
  /// discarded, the recipe creates the next one, and `postRestart` runs on it
  /// before any further message is handled. The handle stays the same, so a
  /// handle taken before the restart reaches the new instance, as an
  /// `ActorRef` does in Pekko.
  pub(crate) fn restart(&mut self) {
    let mut behavior = self.props.create_actor();
    behavior.post_restart();
    self.behavior = Some(behavior);
  }

  /// Runs `post_stop` on the behaviour, if the actor was created.
  pub(crate) fn post_stop(&mut self) {
    if let Some(behavior) = self.behavior.as_mut() {
      behavior.post_stop();
    }
  }
}
