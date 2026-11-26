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

//! The handle a test holds on a state machine it started.

use core::any::Any;
use core::marker::PhantomData;
use core::time::Duration;

use fraktor_actor::kernel::actor::{ActorCreationError, Fsm};
use fraktor_actor::kernel::actor_ref::ActorRef;
use fraktor_actor::kernel::actor_system::ActorSystem;
use fraktor_actor::kernel::message::Message;

/// A handle on a running state machine that reads its state name and data,
/// sets its state, and drives its timers from outside the machine.
///
/// Mirrors `org.apache.pekko.testkit.TestFSMRef`.
pub struct TestFsmRef<S, D> {
  _machine: PhantomData<(S, D)>,
}

impl<S, D> TestFsmRef<S, D>
where
  S: Clone + PartialEq + Send + Sync + 'static,
  D: Clone + Send + 'static,
{
  /// Starts the machine `factory` builds below the guardian, under a
  /// generated name.
  ///
  /// Mirrors `TestFSMRef(factory)`.
  pub fn new<F>(system: &mut ActorSystem, factory: F) -> Self
  where
    F: Fn() -> Fsm<S, D> + Send + Sync + 'static,
  {
    let _ = (system, factory);
    todo!()
  }

  /// Starts the machine `factory` builds below the guardian, under `name`.
  ///
  /// Mirrors `TestFSMRef(factory, name)`.
  ///
  /// # Errors
  ///
  /// Returns [`ActorCreationError`] when the name is invalid or in use.
  pub fn with_name<F>(system: &mut ActorSystem, factory: F, name: &str) -> Result<Self, ActorCreationError>
  where
    F: Fn() -> Fsm<S, D> + Send + Sync + 'static,
  {
    let _ = (system, factory, name);
    todo!()
  }

  /// Starts the machine `factory` builds as a child of `supervisor`, under a
  /// generated name.
  ///
  /// Mirrors `TestFSMRef(factory, supervisor)`.
  pub fn with_supervisor<F>(system: &mut ActorSystem, factory: F, supervisor: &ActorRef) -> Self
  where
    F: Fn() -> Fsm<S, D> + Send + Sync + 'static,
  {
    let _ = (system, factory, supervisor);
    todo!()
  }

  /// Starts the machine `factory` builds as a child of `supervisor`, under
  /// `name`.
  ///
  /// Mirrors `TestFSMRef(factory, supervisor, name)`.
  ///
  /// # Errors
  ///
  /// Returns [`ActorCreationError`] when the name is invalid or in use.
  pub fn with_supervisor_and_name<F>(
    system: &mut ActorSystem,
    factory: F,
    supervisor: &ActorRef,
    name: &str,
  ) -> Result<Self, ActorCreationError>
  where
    F: Fn() -> Fsm<S, D> + Send + Sync + 'static,
  {
    let _ = (system, factory, supervisor, name);
    todo!()
  }

  /// Returns the handle of the running machine.
  pub fn actor_ref(&self) -> ActorRef {
    todo!()
  }

  /// Sends `message` to the machine, naming `sender` as the sender.
  ///
  /// Mirrors `TestFSMRef.!`.
  pub fn tell<M: Any + Send + Sync>(&self, message: M, sender: ActorRef) {
    let _ = (message, sender);
    todo!()
  }

  /// Returns the name the machine was started under.
  pub fn name(&self) -> &str {
    todo!()
  }

  /// Returns the parent the machine was started under.
  pub fn supervisor(&self) -> ActorRef {
    todo!()
  }

  /// Returns the current state name.
  ///
  /// Mirrors `TestFSMRef.stateName`.
  pub fn state_name(&self) -> S {
    todo!()
  }

  /// Returns a copy of the current state data.
  ///
  /// Mirrors `TestFSMRef.stateData`.
  pub fn state_data(&self) -> D {
    todo!()
  }

  /// Moves the machine to `state_name` with `state_data`, keeping the current
  /// value of each element that is `None`, and arms the state timer when
  /// `timeout` is given.
  ///
  /// Mirrors `TestFSMRef.setState`.
  pub fn set_state(&self, state_name: Option<S>, state_data: Option<D>, timeout: Option<Duration>) {
    let _ = (state_name, state_data, timeout);
    todo!()
  }

  /// Mirrors `TestFSMRef.startTimerWithFixedDelay`.
  pub fn start_timer_with_fixed_delay(&self, name: &str, message: Message, delay: Duration) {
    let _ = (name, message, delay);
    todo!()
  }

  /// Mirrors `TestFSMRef.startTimerAtFixedRate`.
  pub fn start_timer_at_fixed_rate(&self, name: &str, message: Message, interval: Duration) {
    let _ = (name, message, interval);
    todo!()
  }

  /// Mirrors `TestFSMRef.startSingleTimer`.
  pub fn start_single_timer(&self, name: &str, message: Message, delay: Duration) {
    let _ = (name, message, delay);
    todo!()
  }

  /// Mirrors `TestFSMRef.cancelTimer`.
  pub fn cancel_timer(&self, name: &str) {
    let _ = name;
    todo!()
  }

  /// Mirrors `TestFSMRef.isTimerActive`.
  pub fn is_timer_active(&self, name: &str) -> bool {
    let _ = name;
    todo!()
  }

  /// Mirrors `TestFSMRef.isStateTimerActive`.
  pub fn is_state_timer_active(&self) -> bool {
    todo!()
  }

  /// Sends the numbered control message that does nothing and returns its
  /// number.
  pub fn send_sentinel(&self) -> u64 {
    todo!()
  }

  /// Returns the highest number among the control messages the machine has
  /// processed.
  pub fn processed_sequence(&self) -> u64 {
    todo!()
  }
}
