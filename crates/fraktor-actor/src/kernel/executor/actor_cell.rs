//! One actor inside the executor, with the behaviour it runs.

use alloc::boxed::Box;

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
  behavior: Box<dyn Actor>,
  self_ref: ActorRef,
  parent: Option<ActorRef>,
  props: Props,
  strategy: SupervisorStrategy,
}

impl ActorCell {
  pub(crate) fn new(
    behavior: Box<dyn Actor>,
    self_ref: ActorRef,
    parent: Option<ActorRef>,
    props: Props,
    strategy: SupervisorStrategy,
  ) -> Self {
    Self { behavior, self_ref, parent, props, strategy }
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
  pub(crate) fn strategy(&self) -> SupervisorStrategy {
    self.strategy
  }

  /// Hands one message to the behaviour.
  pub(crate) fn deliver(&mut self, context: &mut ActorContext<'_>, message: Message) {
    self.behavior.receive(context, message);
  }

  /// Replaces the behaviour with a fresh instance from the recipe and runs its
  /// `post_restart`.
  ///
  /// Mirrors what Pekko does for a restarted child: the old instance is
  /// discarded, the recipe creates the next one, and `postRestart` runs on it
  /// before any further message is handled. The handle stays the same, so a
  /// handle taken before the restart reaches the new instance, as an
  /// `ActorRef` does in Pekko.
  pub(crate) fn restart(&mut self) {
    let mut behavior = self.props.create_actor();
    behavior.post_restart();
    self.behavior = behavior;
  }
}
