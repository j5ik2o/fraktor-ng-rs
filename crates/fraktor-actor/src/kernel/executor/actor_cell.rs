//! One actor inside the executor, with the behaviour it runs.

use alloc::boxed::Box;
use core::any::Any;

use crate::kernel::actor::Actor;
use crate::kernel::actor::ActorContext;
use crate::kernel::actor::BehaviorStack;
use crate::kernel::actor::Props;
use crate::kernel::actor_ref::ActorRef;
use crate::kernel::message::Message;
use crate::kernel::supervision::SupervisorStrategy;

/// An actor that the executor owns and drives.
///
/// Mirrors the mailbox-and-actor pair that an `org.apache.pekko.actor.ActorCell`
/// keeps: the executor owns the behaviour, the behaviour stack that `become`
/// installs, the recipe that created it, and the handle, and lends the context
/// out only while a message is being handled. The recipe and the strategy are
/// kept so the cell can be re-created after a failure, as a parent does for its
/// child.
pub(crate) struct ActorCell {
  behavior: Option<Box<dyn Actor>>,
  stack: BehaviorStack,
  self_ref: ActorRef,
  parent: Option<ActorRef>,
  props: Props,
  strategy: SupervisorStrategy,
  restart_count: u32,
  post_stop_suppressed: bool,
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
    Self {
      behavior: None,
      stack: BehaviorStack::new(),
      self_ref,
      parent,
      props,
      strategy,
      restart_count: 0,
      post_stop_suppressed: false,
    }
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

  /// Returns the handle of the actor that started this actor, when it has one.
  pub(crate) fn parent(&self) -> Option<ActorRef> {
    self.parent.clone()
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

  /// Takes the behaviour stack out of the cell so the executor can lend it to
  /// the context while the actor runs.
  pub(crate) fn take_stack(&mut self) -> BehaviorStack {
    core::mem::replace(&mut self.stack, BehaviorStack::new())
  }

  /// Puts the behaviour stack back after the actor has run.
  pub(crate) fn restore_stack(&mut self, stack: BehaviorStack) {
    self.stack = stack;
  }

  /// Builds the behaviour from the recipe and runs its `pre_start`.
  ///
  /// The stack starts empty, so a `change_behavior` in `pre_start` installs the
  /// frame that answers the first message; with none installed the initial
  /// `receive` answers, as Pekko's `newActor` does when the stack is empty.
  ///
  /// The strategy the behaviour applies to its own children is read from
  /// `Actor::supervisor_strategy` and registered with the executor before
  /// `pre_start`, so a child that `pre_start` starts is placed under it.
  pub(crate) fn create(&mut self, context: &mut ActorContext<'_>) {
    let mut behavior = self.props.create_actor();
    context.register_parent_strategy(behavior.supervisor_strategy());
    behavior.pre_start(context);
    self.behavior = Some(behavior);
  }

  /// Hands one message to the current behaviour.
  pub(crate) fn deliver(&mut self, context: &mut ActorContext<'_>, message: Message) {
    if let Some(behavior) = self.behavior.as_mut() {
      context.run_behavior(behavior.as_mut(), message);
    }
  }

  /// Runs `pre_restart` on the current behaviour.
  pub(crate) fn pre_restart(&mut self, reason: &(dyn Any + Send)) {
    if let Some(behavior) = self.behavior.as_mut() {
      behavior.pre_restart(reason);
    }
  }

  /// Replaces the behaviour with a fresh instance from the recipe and runs its
  /// `post_restart`, which is what Pekko's default `postRestart` does.
  ///
  /// The executor clears the behaviour stack before it calls this, as Pekko's
  /// `clearActorFields` does, so a `become` from before the restart is gone and
  /// the initial `receive` answers again. Mirrors what Pekko does for a
  /// restarted child: the old instance is discarded, the recipe creates the
  /// next one, and `postRestart` runs on it before any further message is
  /// handled. The handle stays the same, so a handle taken before the restart
  /// reaches the new instance. Like `create`, this registers the re-created
  /// instance's child-facing strategy from `Actor::supervisor_strategy` right
  /// after the recipe builds it and before `post_restart` runs, so the strategy
  /// is built from the fields the recipe sets, as Pekko evaluates
  /// `supervisorStrategy` when it constructs the instance.
  pub(crate) fn restart(&mut self, context: &mut ActorContext<'_>) {
    let mut behavior = self.props.create_actor();
    context.register_parent_strategy(behavior.supervisor_strategy());
    behavior.post_restart(context);
    self.behavior = Some(behavior);
  }

  /// Runs `post_stop` on the behaviour, if the actor was created.
  pub(crate) fn post_stop(&mut self) {
    if let Some(behavior) = self.behavior.as_mut() {
      behavior.post_stop();
    }
  }

  /// Records that the stop path must not run this instance's `post_stop` again.
  ///
  /// The default `preRestart` already runs `post_stop` on the old instance, as
  /// Pekko's does. When the restart fails after that, the cell is still the old
  /// instance and the stop path would run its `post_stop` a second time; this
  /// marks that it already ran, so the stop finishes without repeating it.
  pub(crate) fn suppress_post_stop(&mut self) {
    self.post_stop_suppressed = true;
  }

  /// Returns whether the stop path must skip this instance's `post_stop`.
  pub(crate) fn post_stop_suppressed(&self) -> bool {
    self.post_stop_suppressed
  }
}
