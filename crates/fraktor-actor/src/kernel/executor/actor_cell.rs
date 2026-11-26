// SPDX-License-Identifier: Apache-2.0
//
// Licensed to the Apache Software Foundation (ASF) under one or more
// license agreements; and to You under the Apache License, version 2.0:
//
//   https://www.apache.org/licenses/LICENSE-2.0
//
// This file is part of the Apache Pekko project, which was derived from Akka.
//
// Copyright (C) 2009-2022 Lightbend Inc. <https://www.lightbend.com>
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.

//! One actor inside the executor, with the behaviour it runs.

use alloc::boxed::Box;
use core::any::Any;

use crate::kernel::actor::Actor;
use crate::kernel::actor::ActorContext;
use crate::kernel::actor::BehaviorStack;
use crate::kernel::actor::Props;
use crate::kernel::actor_ref::ActorRef;
use crate::kernel::executor::Executor;
use crate::kernel::message::Message;
use crate::kernel::supervision::FailureCatcher;
use crate::kernel::supervision::RestartHistory;
use crate::kernel::supervision::RunOutcome;
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
  strategy_inherited: bool,
  restart_history: RestartHistory,
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
    strategy_inherited: bool,
  ) -> Self {
    Self {
      behavior: None,
      stack: BehaviorStack::new(),
      self_ref,
      parent,
      props,
      strategy,
      strategy_inherited,
      restart_history: RestartHistory::new(),
      post_stop_suppressed: false,
    }
  }

  /// Returns the key of the actor's entry in the executor.
  pub(crate) fn key(&self) -> (u64, u32) {
    self.self_ref.key()
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

  /// Returns whether this actor's strategy is inherited from its parent.
  ///
  /// A named child started through `ActorContext::actorOf` inherits it, so the
  /// supervisor decision reads the parent's current strategy instead of a copy
  /// taken at creation. A child started under an explicit strategy keeps the
  /// one its caller chose.
  pub(crate) fn strategy_inherited(&self) -> bool {
    self.strategy_inherited
  }

  /// Returns whether the recipe has built the behaviour for this cell.
  ///
  /// A cell that never finishes `pre_start` has no behaviour, and Pekko does not
  /// run `handleChildTerminated` for such a parent.
  pub(crate) fn is_created(&self) -> bool {
    self.behavior.is_some()
  }

  /// Returns the number of restart requests this actor has already made.
  pub(crate) fn restart_count(&self) -> u32 {
    self.restart_history.count()
  }

  /// Records one more restart request.
  pub(crate) fn record_restart_request(&mut self) {
    self.restart_history.record_plain();
  }

  /// Returns this actor's restart history, for a time-windowed strategy.
  pub(crate) fn restart_history(&self) -> RestartHistory {
    self.restart_history
  }

  /// Replaces this actor's restart history after a permission check.
  pub(crate) fn set_restart_history(&mut self, history: RestartHistory) {
    self.restart_history = history;
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

  /// Hands one message from `sender` to the current behaviour, under
  /// `catcher` when there is one, and returns how it ended with the sender.
  ///
  /// The context borrows the cell's own handle and behaviour stack, so the
  /// delivery copies neither, and a panic that unwinds out of it leaves the
  /// stack in the cell.
  pub(crate) fn receive(
    &mut self,
    executor: &mut Executor,
    message: Message,
    sender: ActorRef,
    catcher: Option<&dyn FailureCatcher>,
  ) -> (RunOutcome, ActorRef) {
    let Self { behavior, stack, self_ref, .. } = self;
    let mut context = ActorContext::new(self_ref, sender, executor, stack);
    let outcome = match (behavior.as_mut(), catcher) {
      | (Some(behavior), Some(catcher)) => {
        let mut message = Some(message);
        catcher.run(&mut || {
          if let Some(message) = message.take() {
            context.run_behavior(behavior.as_mut(), message);
          }
        })
      },
      | (Some(behavior), None) => {
        context.run_behavior(behavior.as_mut(), message);
        RunOutcome::Completed
      },
      | (None, _) => RunOutcome::Completed,
    };
    (outcome, context.into_sender())
  }

  /// Runs `pre_restart` on the current behaviour.
  pub(crate) fn pre_restart(&mut self, context: &mut ActorContext<'_>, reason: &(dyn Any + Send)) {
    if let Some(behavior) = self.behavior.as_mut() {
      behavior.pre_restart(context, reason);
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

  /// Runs the context-carrying stop hook on the behaviour, if the actor was
  /// created.
  ///
  /// The hook defaults to `post_stop`, so a behaviour that only overrides
  /// `post_stop` keeps working, while a behaviour that needs the message's
  /// original sender in `post_stop_with_context` receives it. This is the hook
  /// the stop path runs, so the failing child's original sender reaches only
  /// it.
  pub(crate) fn post_stop_with_context(&mut self, context: &mut ActorContext<'_>) {
    if let Some(behavior) = self.behavior.as_mut() {
      behavior.post_stop_with_context(context);
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
