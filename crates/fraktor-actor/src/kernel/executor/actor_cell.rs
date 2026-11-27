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

#[cfg(feature = "test-support")]
use crate::kernel::actor::test_support::ActorHandle;
use crate::kernel::{
  actor::{Actor, ActorContext, BehaviorStack, Props},
  actor_ref::ActorRef,
  executor::Executor,
  message::Message,
  supervision::{FailureCatcher, RunOutcome, SupervisorStrategy},
};

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
  post_stop_suppressed: bool,
  failed_start: bool,
  failed_sender: Option<ActorRef>,
  #[cfg(feature = "test-support")]
  handle: Option<ActorHandle>,
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
      post_stop_suppressed: false,
      failed_start: false,
      failed_sender: None,
      #[cfg(feature = "test-support")]
      handle: None,
    }
  }

  /// Attaches the handle a test started this actor through, which the
  /// executor's test-support face writes into as the actor starts,
  /// restarts, and stops.
  #[cfg(feature = "test-support")]
  pub(crate) fn set_handle(&mut self, handle: ActorHandle) {
    self.handle = Some(handle);
  }

  /// Returns the handle a test started this actor through, when one is
  /// attached.
  #[cfg(feature = "test-support")]
  pub(crate) fn handle(&self) -> Option<&ActorHandle> {
    self.handle.as_ref()
  }

  /// Takes the snapshot the current instance publishes for a test's
  /// probe, or `None` when it publishes none.
  #[cfg(feature = "test-support")]
  pub(crate) fn probe_snapshot(&self) -> Option<Box<dyn Any + Send + Sync>> {
    self.behavior.as_ref().and_then(|behavior| behavior.underlying_snapshot())
  }

  /// Returns the key of the actor's entry in the executor.
  pub(crate) fn key(&self) -> (u64, u32) {
    self.self_ref.key()
  }

  /// Returns the handle of the actor that this cell runs.
  pub(crate) fn self_ref(&self) -> ActorRef {
    self.self_ref.clone()
  }

  /// Returns the handle of the actor that started this actor, when it has one.
  pub(crate) fn parent(&self) -> Option<ActorRef> {
    self.parent.clone()
  }

  /// Returns the strategy this actor applies to its children.
  pub(crate) fn strategy(&self) -> &SupervisorStrategy {
    &self.strategy
  }

  /// Returns whether the recipe has built the behaviour for this cell.
  ///
  /// A cell that never finishes `pre_start` has no behaviour, and Pekko does not
  /// run `handleChildTerminated` for such a parent.
  pub(crate) fn is_created(&self) -> bool {
    self.behavior.is_some()
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
    #[cfg(feature = "test-support")]
    if let Some(handle) = &self.handle {
      handle.mark_started();
    }
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
    #[cfg(feature = "test-support")]
    if let Some(handle) = &self.handle {
      handle.note_restart();
    }
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

  /// Returns whether the start of this cell failed: the mark that there
  /// is no instance, which a retry of the start clears.
  pub(crate) fn has_failed_start(&self) -> bool {
    self.failed_start
  }

  /// Records that the start failed.
  pub(crate) fn record_start_failure(&mut self) {
    self.failed_start = true;
  }

  /// Forgets the failed start, as a retry of the start does, and returns
  /// whether there was one.
  pub(crate) fn take_start_failure(&mut self) -> bool {
    core::mem::take(&mut self.failed_start)
  }

  /// Returns the sender of the message whose handling failed, which
  /// `post_stop` and `pre_restart` see as `sender()` (Pekko's
  /// `currentMessage`, which a failure leaves on the failed envelope).
  pub(crate) fn failed_sender(&self) -> Option<ActorRef> {
    self.failed_sender.clone()
  }

  /// Records or clears the sender of the failed message.
  pub(crate) fn set_failed_sender(&mut self, sender: Option<ActorRef>) {
    self.failed_sender = sender;
  }
}
