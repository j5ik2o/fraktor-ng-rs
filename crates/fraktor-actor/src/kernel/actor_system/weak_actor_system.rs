// SPDX-License-Identifier: Apache-2.0
//
// Licensed to the Apache Software Foundation (ASF) under one or more
// license agreements; and to You under the Apache License, version 2.0:
//
//   https://www.apache.org/licenses/LICENSE-2.0
//
// This file is part of the Apache Pekko project, which was derived from Akka.
//
// Copyright (C) 2017-2022 Lightbend Inc. <https://www.lightbend.com>
//
// Copyright (C) 2009-2022 Lightbend Inc. <https://www.lightbend.com>
//
// Copyright (C) 2014-2022 Lightbend Inc. <https://www.lightbend.com>
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.

//! A handle to an actor system that does not keep it alive.

use alloc::string::String;
use core::time::Duration;

#[cfg(feature = "test-support")]
use crate::kernel::actor::test_support::ActorHandle;
use crate::kernel::actor::ActorCreationError;
use crate::kernel::actor::Props;
use crate::kernel::actor_ref::ActorRef;
use crate::kernel::executor::Executor;
use crate::kernel::message::Message;
#[cfg(feature = "test-support")]
use crate::kernel::supervision::RunOutcome;
use crate::kernel::supervision::SupervisorStrategy;
use crate::kernel::sync::Arc;
use crate::kernel::sync::Weak;

/// A handle to an actor system that does not keep it alive.
///
/// Mirrors the way a test probe reaches the system it runs in: the probe must
/// not own the system, because the test owns it, yet the probe still creates
/// children, watches actors, and drives messages. The handle holds a `Weak`
/// back-edge, which the ownership model permits. Calling it after the system
/// is gone panics, so a probe used after its spec has been torn down fails
/// loudly instead of silently doing nothing.
///
/// The handle is `Send + Sync` from stage 1 of #234, but the executor is
/// still guarded by one lock that a call takes with `try_lock`: before
/// stage 5, using the system from several threads at the same time panics
/// with "already borrowed: re-entrant or concurrent use", as a call from
/// inside `receive` does.
///
/// Once the system has been terminated, `spawn_child` and `function_ref` panic
/// with the same cause as `ActorSystem::actor_of`, because the executor owns
/// that state. Pekko's `attachChild` refuses a defunct guardian for the same
/// reason (`pekko/actor/.../dungeon/Children.scala:313-314`).
pub struct WeakActorSystem {
  name: String,
  executor: Weak<spin::Mutex<Executor>>,
}

impl WeakActorSystem {
  pub(crate) fn new(name: String, executor: Weak<spin::Mutex<Executor>>) -> Self {
    Self { name, executor }
  }

  fn upgrade(&self) -> Arc<spin::Mutex<Executor>> {
    self.executor.upgrade().unwrap_or_else(|| panic!("the actor system {} is gone", self.name))
  }

  /// Starts a child of `parent`.
  ///
  /// Mirrors `ActorContext.actorOf` reached through a non-owning handle. A
  /// named request goes through the same registry as
  /// [`ActorSystem::actor_of`](crate::kernel::actor_system::ActorSystem) and a
  /// name a live child uses is refused with
  /// [`InvalidActorNameException`](crate::kernel::actor::InvalidActorNameException).
  /// The handle cannot return an error to its caller, so a refused name ends the
  /// call with the message Pekko throws from `attachChild`.
  pub fn spawn_child(
    &self,
    parent: &ActorRef,
    props: Props,
    name: Option<&str>,
    strategy: SupervisorStrategy,
  ) -> ActorRef {
    match self.try_spawn_child(parent, props, name, strategy) {
      | Ok(child) => child,
      | Err(error) => panic!("{error}"),
    }
  }

  /// Starts a child of `parent`, answering with the refusal instead of
  /// throwing it.
  ///
  /// The checks are the ones a top-level creation runs: the name, that
  /// the system and the parent still take children, that the name is
  /// free, and the configuration of the recipe. A caller that can report
  /// a refusal, such as a test handle that starts a child under a
  /// supervisor it was given, uses this entry; [`spawn_child`](Self::spawn_child)
  /// is this call with the refusal thrown, as Pekko throws it.
  ///
  /// This entry verifies the configuration, which
  /// [`ActorContext::actor_of`](crate::kernel::actor::ActorContext::actor_of)
  /// does not: the same recipe with an undefined dispatcher name is
  /// refused here and accepted there. That difference is a known
  /// deviation; issue #247
  /// (https://github.com/j5ik2o/fraktor-ng-rs/issues/247) aligns
  /// `actor_of` with this entry so both verify the configuration.
  ///
  /// # Errors
  ///
  /// Returns [`ActorCreationError`] when the name, the parent's state,
  /// or the configuration is refused.
  pub fn try_spawn_child(
    &self,
    parent: &ActorRef,
    props: Props,
    name: Option<&str>,
    strategy: SupervisorStrategy,
  ) -> Result<ActorRef, ActorCreationError> {
    Executor::lend(&self.upgrade(), |executor| executor.try_spawn_child_from_weak(parent, props, name, strategy))
  }

  /// Starts an actor a test wraps, attaching `handle` to its cell.
  ///
  /// Mirrors the creation `TestActorRef` performs: `supervisor` is the
  /// parent the actor is started under, or the user guardian when it is
  /// omitted. The checks are the ones a top-level creation runs, and
  /// the strategy the child is placed under follows the supervisor, as
  /// the executor's entry documents.
  ///
  /// # Errors
  ///
  /// Returns [`ActorCreationError`] when the name, the parent's state,
  /// or the configuration is refused.
  ///
  /// # Panics
  ///
  /// Panics when the supervisor is an actor that has not started yet.
  #[cfg(feature = "test-support")]
  pub fn try_spawn_test_actor(
    &self,
    supervisor: Option<&ActorRef>,
    props: Props,
    name: Option<&str>,
    handle: ActorHandle,
  ) -> Result<ActorRef, ActorCreationError> {
    Executor::lend(&self.upgrade(), |executor| executor.try_spawn_test_actor(supervisor, props, name, handle))
  }

  /// Runs `target`'s behaviour once with `message`, outside its
  /// mailbox, naming `sender` as the sender.
  ///
  /// Mirrors `TestActorRef.receive`. A failure of the behaviour is
  /// returned as [`RunOutcome::Failed`] instead of reaching the
  /// supervisor.
  ///
  /// # Panics
  ///
  /// Panics when the actor has stopped.
  #[cfg(feature = "test-support")]
  pub fn receive_directly(&self, target: &ActorRef, message: Message, sender: ActorRef) -> RunOutcome {
    Executor::lend(&self.upgrade(), |executor| executor.receive_directly(target, message, sender))
  }

  /// Records `strategy` as the one `target` applies to the children it
  /// supervises.
  ///
  /// For an actor this is what `supervisor_strategy` does at its start;
  /// the test kit uses it for its test actor, a function reference
  /// whose children Pekko's `TestActor` places under a stopping
  /// strategy by delegation.
  #[cfg(feature = "test-support")]
  pub fn register_supervisor_strategy(&self, target: &ActorRef, strategy: SupervisorStrategy) {
    Executor::lend(&self.upgrade(), |executor| executor.register_parent_strategy(target.clone(), strategy));
  }

  /// Resolves `path` to a handle, as
  /// [`ActorSystem::resolve_actor_ref`](crate::kernel::actor_system::ActorSystem::resolve_actor_ref)
  /// does, reached through a non-owning handle.
  #[cfg(feature = "test-support")]
  pub fn resolve_actor_ref(&self, path: &crate::kernel::actor_path::ActorPath) -> ActorRef {
    Executor::lend(&self.upgrade(), |executor| executor.resolve_actor_ref(path))
  }

  /// Returns the handle that absorbs messages which no actor takes,
  /// as [`ActorSystem::dead_letters`](crate::kernel::actor_system::ActorSystem::dead_letters)
  /// does, reached through a non-owning handle.
  #[cfg(feature = "test-support")]
  pub fn dead_letters(&self) -> ActorRef {
    Executor::lend(&self.upgrade(), |executor| executor.dead_letters())
  }

  /// Starts a handle whose shard runs `handler` for each delivered message.
  pub fn function_ref<H>(&self, name: &str, handler: H) -> ActorRef
  where
    H: Fn(&ActorRef, Message) + Send + Sync + 'static,
  {
    Executor::lend(&self.upgrade(), |executor| executor.function_ref(name, handler))
  }

  /// Registers `callback` to run with the handle of every actor that stops.
  ///
  /// Mirrors the notification that Pekko's `ActorClassificationUnsubscriber`
  /// receives for the actors it watches. A component that is not an actor, such
  /// as an event bus, has no mailbox to receive a `Terminated` notice, so it
  /// registers here instead. The callback must not lock the executor, which
  /// is already locked while it runs.
  pub fn on_termination<F>(&self, callback: F)
  where
    F: Fn(&ActorRef) + Send + 'static,
  {
    Executor::lend(&self.upgrade(), |executor| executor.on_termination(callback));
  }

  /// Registers `watcher` to receive the `Terminated` notice of `target`.
  pub fn watch(&self, watcher: &ActorRef, target: &ActorRef) {
    Executor::lend(&self.upgrade(), |executor| executor.watch(watcher, target));
  }

  /// Stops `watcher` from receiving the notice of `target`.
  ///
  /// Mirrors `ActorContext.unwatch` reached through a non-owning handle.
  pub fn unwatch(&self, watcher: &ActorRef, target: &ActorRef) {
    Executor::lend(&self.upgrade(), |executor| executor.unwatch(watcher, target));
  }

  /// Asks `target` to stop. Its watchers are told once it has stopped.
  ///
  /// Mirrors `ActorSystem.stop` reached through a non-owning handle, which is
  /// how a probe or a test that only holds a `Weak` back-edge stops an actor.
  /// The stop takes effect once the dispatcher has run the target.
  pub fn stop(&self, target: &ActorRef) {
    Executor::lend(&self.upgrade(), |executor| executor.stop(target));
  }

  /// Returns whether `target` no longer has a running actor behind it.
  ///
  /// Mirrors
  /// [`ActorSystem::is_actor_terminated`](crate::kernel::actor_system::ActorSystem::is_actor_terminated)
  /// reached through a non-owning handle: a handle reports false while its actor
  /// runs and true once the actor has stopped. The scope and its limits are the
  /// ones `watch` documents.
  pub fn is_actor_terminated(&self, target: &ActorRef) -> bool {
    Executor::lend(&self.upgrade(), |executor| executor.is_actor_terminated(target))
  }

  /// Runs every mailbox that has work, and delivers every message waiting at
  /// the boundary, until nothing is left. A mailbox runs up to the
  /// throughput of its dispatcher at a time.
  pub fn run_until_idle(&self) {
    Executor::lend(&self.upgrade(), |executor| executor.run_until_idle());
  }

  /// Delivers at most one message of the manually driven dispatcher and
  /// returns whether it delivered one.
  ///
  /// This is a stepping aid for tests that check a deadline between
  /// messages; it is not one run of a mailbox. A run of a mailbox handles up
  /// to `throughput` messages; `run_one` hands a single message to a single
  /// actor and then returns, whatever the throughput of its dispatcher is.
  /// Use `run_until_idle` to drive the system the way a dispatcher does.
  ///
  /// It keeps running ready mailboxes that only have system messages to
  /// handle until it delivers one message or nothing is ready, so `false`
  /// means there was nothing left to drive.
  pub fn run_one(&self) -> bool {
    Executor::lend(&self.upgrade(), |executor| executor.run_one())
  }

  /// Waits at most `max` for work for the system: a mailbox with a message
  /// to handle, a message at the boundary, or a message that one of the
  /// system's function references handled. Returns whether work came,
  /// without delivering anything; call `run_one` or `run_until_idle` to
  /// deliver it.
  pub fn await_envelope(&self, max: Duration) -> bool {
    Executor::lend(&self.upgrade(), |executor| executor.await_envelope(max))
  }
}

impl Clone for WeakActorSystem {
  fn clone(&self) -> Self {
    Self { name: self.name.clone(), executor: self.executor.clone() }
  }
}
