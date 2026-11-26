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

use crate::kernel::actor::ActorCreationError;
use crate::kernel::actor::Props;
use crate::kernel::actor_ref::ActorRef;
use crate::kernel::executor::Executor;
use crate::kernel::message::Message;
use crate::kernel::supervision::SupervisorStrategy;
use crate::kernel::sync::lock;
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
    lock(&self.upgrade()).try_spawn_child_from_weak(parent, props, name, strategy)
  }

  /// Starts a handle whose shard runs `handler` for each delivered message.
  pub fn function_ref<H>(&self, name: &str, handler: H) -> ActorRef
  where
    H: Fn(&ActorRef, Message) + Send + Sync + 'static,
  {
    lock(&self.upgrade()).function_ref(name, handler)
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
    lock(&self.upgrade()).on_termination(callback);
  }

  /// Registers `watcher` to receive the `Terminated` notice of `target`.
  pub fn watch(&self, watcher: &ActorRef, target: &ActorRef) {
    lock(&self.upgrade()).watch(watcher, target);
  }

  /// Stops `watcher` from receiving the notice of `target`.
  ///
  /// Mirrors `ActorContext.unwatch` reached through a non-owning handle.
  pub fn unwatch(&self, watcher: &ActorRef, target: &ActorRef) {
    lock(&self.upgrade()).unwatch(watcher, target);
  }

  /// Stops `target`, telling its watchers that it is gone.
  ///
  /// Mirrors `ActorSystem.stop` reached through a non-owning handle, which is
  /// how a probe or a test that only holds a `Weak` back-edge stops an actor.
  pub fn stop(&self, target: &ActorRef) {
    lock(&self.upgrade()).stop_at_boundary(target);
  }

  /// Returns whether `target` no longer has a running actor behind it.
  ///
  /// Mirrors
  /// [`ActorSystem::is_actor_terminated`](crate::kernel::actor_system::ActorSystem::is_actor_terminated)
  /// reached through a non-owning handle: a handle reports false while its actor
  /// runs and true once the actor has stopped. The scope and its limits are the
  /// ones `watch` documents.
  pub fn is_actor_terminated(&self, target: &ActorRef) -> bool {
    lock(&self.upgrade()).is_actor_terminated(target)
  }

  /// Delivers every message that is waiting until nothing is left.
  pub fn run_until_idle(&self) {
    lock(&self.upgrade()).run_until_idle();
  }

  /// Delivers at most one message and returns whether it delivered one.
  pub fn run_one(&self) -> bool {
    lock(&self.upgrade()).run_one()
  }

  /// Waits at most `max` for one message from the boundary and moves it onto
  /// the local queue. Returns whether a message arrived.
  pub fn await_envelope(&self, max: Duration) -> bool {
    lock(&self.upgrade()).await_envelope(max)
  }
}

impl Clone for WeakActorSystem {
  fn clone(&self) -> Self {
    Self { name: self.name.clone(), executor: self.executor.clone() }
  }
}
