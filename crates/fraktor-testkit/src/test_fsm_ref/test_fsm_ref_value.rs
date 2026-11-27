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

use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use fraktor_actor::kernel::actor::test_support::FsmControl;
use fraktor_actor::kernel::actor::test_support::FsmHandle;
use fraktor_actor::kernel::actor::test_support::FsmTimerKind;
use fraktor_actor::kernel::actor::{ActorCreationError, Fsm, Props};
use fraktor_actor::kernel::actor_ref::ActorRef;
use fraktor_actor::kernel::actor_system::ActorSystem;
use fraktor_actor::kernel::message::Message;
use fraktor_actor::kernel::supervision::SupervisorStrategy;
use spin::Mutex;

#[cfg(test)]
#[path = "test_fsm_ref_value_test.rs"]
mod tests;

/// A handle on a running state machine that reads its state name and data,
/// sets its state, and starts and cancels its timers from outside the
/// machine. The executor fires the timers; the handle's records of them
/// are for reading.
///
/// Mirrors `org.apache.pekko.testkit.TestFSMRef`.
pub struct TestFsmRef<S, D> {
  actor_ref: ActorRef,
  handle: FsmHandle<S, D>,
  name: String,
  supervisor: ActorRef,
  stop_driver: Arc<AtomicBool>,
  _machine: PhantomData<(S, D)>,
}

impl<S, D> Drop for TestFsmRef<S, D> {
  fn drop(&mut self) {
    self.stop_driver.store(true, Ordering::Relaxed);
  }
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
  ///
  /// # Panics
  ///
  /// Panics when the machine fails to start, as Pekko's creation
  /// throws the initialization failure instead of handing out a
  /// reference.
  pub fn new<F>(system: &mut ActorSystem, factory: F) -> Self
  where
    F: Fn() -> Fsm<S, D> + Send + Sync + 'static,
  {
    match Self::start(system, factory, None, None) {
      | Ok(started) => started,
      | Err(error) => panic!("{error}"),
    }
  }

  /// Starts the machine `factory` builds below the guardian, under `name`.
  ///
  /// Mirrors `TestFSMRef(factory, name)`.
  ///
  /// # Errors
  ///
  /// Returns [`ActorCreationError`] when the name is invalid or in use.
  ///
  /// # Panics
  ///
  /// Panics when the machine fails to start, as Pekko's creation
  /// throws the initialization failure instead of handing out a
  /// reference; the combined creation error has no case for it.
  pub fn with_name<F>(system: &mut ActorSystem, factory: F, name: &str) -> Result<Self, ActorCreationError>
  where
    F: Fn() -> Fsm<S, D> + Send + Sync + 'static,
  {
    Self::start(system, factory, None, Some(name))
  }

  /// Starts the machine `factory` builds as a child of `supervisor`, under a
  /// generated name. The user guardian is a supervisor like any other:
  /// naming it creates the machine below it, as omitting it does.
  ///
  /// Mirrors `TestFSMRef(factory, supervisor)`.
  ///
  /// # Panics
  ///
  /// Panics when the machine fails to start, as Pekko's creation
  /// throws the initialization failure instead of handing out a
  /// reference.
  pub fn with_supervisor<F>(system: &mut ActorSystem, factory: F, supervisor: &ActorRef) -> Self
  where
    F: Fn() -> Fsm<S, D> + Send + Sync + 'static,
  {
    match Self::start(system, factory, Some(supervisor), None) {
      | Ok(started) => started,
      | Err(error) => panic!("{error}"),
    }
  }

  /// Starts the machine `factory` builds as a child of `supervisor`, under
  /// `name`.
  ///
  /// Mirrors `TestFSMRef(factory, supervisor, name)`.
  ///
  /// # Errors
  ///
  /// Returns [`ActorCreationError`] when the name is invalid or in use.
  ///
  /// # Panics
  ///
  /// Panics when the machine fails to start, as Pekko's creation
  /// throws the initialization failure instead of handing out a
  /// reference; the combined creation error has no case for it.
  pub fn with_supervisor_and_name<F>(
    system: &mut ActorSystem,
    factory: F,
    supervisor: &ActorRef,
    name: &str,
  ) -> Result<Self, ActorCreationError>
  where
    F: Fn() -> Fsm<S, D> + Send + Sync + 'static,
  {
    Self::start(system, factory, Some(supervisor), Some(name))
  }

  fn start<F>(
    system: &mut ActorSystem,
    factory: F,
    supervisor: Option<&ActorRef>,
    name: Option<&str>,
  ) -> Result<Self, ActorCreationError>
  where
    F: Fn() -> Fsm<S, D> + Send + Sync + 'static,
  {
    let handle = FsmHandle::new();
    let attached = handle.clone();
    let factory = Arc::new(factory);
    // The wrapped factory attaches the one handle to every instance it
    // builds, so a restarted machine writes into the same snapshot and
    // the same counters as the instance it replaces.
    let props = Props::new(move || {
      let mut machine = factory();
      machine.attach_handle(attached.clone());
      machine
    });
    let (actor_ref, supervisor) = match (supervisor, name) {
      | (None, None) => {
        let actor_ref = system.actor_of(props);
        let supervisor = system.resolve_actor_ref(&actor_ref.path().parent());
        (actor_ref, supervisor)
      },
      | (None, Some(name)) => {
        let actor_ref = system.actor_of_named(props, name)?;
        let supervisor = system.resolve_actor_ref(&actor_ref.path().parent());
        (actor_ref, supervisor)
      },
      | (Some(parent), name) => {
        let actor_ref =
          system.downgrade().try_spawn_child(parent, props, name, SupervisorStrategy::default_strategy())?;
        (actor_ref, parent.clone())
      },
    };
    // Pekko's `TestFSMRef` builds the actor on a calling-thread
    // dispatcher, so a machine that fails to start throws at creation
    // and no reference is handed out. Here the machine starts in its own
    // run, so the dispatcher is run
    // until the machine has started or stopped; work queued before it
    // runs too. The contract is then a check: a machine the supervisor
    // stopped during its start is terminated at this point, and the
    // constructor fails instead of handing out a reference whose reads
    // would keep answering with the starting state. Stage 7b's
    // calling-thread dispatcher replaces this loop.
    while !handle.has_started() && !system.is_actor_terminated(&actor_ref) {
      if !system.run_one() {
        break;
      }
    }
    if system.is_actor_terminated(&actor_ref) {
      panic!("the machine at {} failed to start", actor_ref.path());
    }
    handle.set_actor_ref(actor_ref.clone());
    let name = actor_ref.path().name().to_string();
    let stop_driver = Arc::new(AtomicBool::new(false));
    // The flag also records the machine's end when the machine stops
    // for good, not only when this handle is dropped: the system runs
    // the registered callback at the final termination. A restart does
    // not run it, so a restarted machine's handle keeps reading as
    // alive. The callback runs while the executor is locked, so it
    // only compares the reference and sets the flag.
    let termination_flag = stop_driver.clone();
    let watched = actor_ref.clone();
    system.downgrade().on_termination(move |stopped| {
      if *stopped == watched {
        termination_flag.store(true, Ordering::Relaxed);
      }
    });
    Ok(Self { actor_ref, handle, name, supervisor, stop_driver, _machine: PhantomData })
  }

  fn send_control(&self, control: FsmControl<S, D>) {
    self.actor_ref.tell(control, self.actor_ref.clone());
  }

  /// Returns the handle of the running machine.
  pub fn actor_ref(&self) -> ActorRef {
    self.actor_ref.clone()
  }

  /// Sends `message` to the machine, naming `sender` as the sender.
  ///
  /// Mirrors `TestFSMRef.!`.
  pub fn tell<M: Any + Send + Sync>(&self, message: M, sender: ActorRef) {
    self.actor_ref.tell(message, sender);
  }

  /// Returns the name the machine was started under.
  pub fn name(&self) -> &str {
    &self.name
  }

  /// Returns the parent the machine was started under.
  pub fn supervisor(&self) -> ActorRef {
    self.supervisor.clone()
  }

  /// Returns the current state name.
  ///
  /// Mirrors `TestFSMRef.stateName`.
  pub fn state_name(&self) -> S {
    self.handle.state_name().expect("the machine has written its starting state")
  }

  /// Returns a copy of the current state data.
  ///
  /// Mirrors `TestFSMRef.stateData`.
  pub fn state_data(&self) -> D {
    self.handle.state_data().expect("the machine has written its starting state")
  }

  /// Moves the machine to `state_name` with `state_data`, keeping the current
  /// value of each element that is `None`, and arms the state timer when
  /// `timeout` is given.
  ///
  /// Mirrors `TestFSMRef.setState`.
  pub fn set_state(&self, state_name: Option<S>, state_data: Option<D>, timeout: Option<Duration>) {
    let sequence = self.handle.allocate_sequence();
    let data = state_data.map(|data| Arc::new(Mutex::new(Some(data))));
    self.send_control(FsmControl::SetState { sequence, state: state_name, data, timeout });
  }

  /// Mirrors `TestFSMRef.startTimerWithFixedDelay`.
  pub fn start_timer_with_fixed_delay(&self, name: &str, message: Message, delay: Duration) {
    self.start_timer(name, message, delay, FsmTimerKind::FixedDelay);
  }

  /// Mirrors `TestFSMRef.startTimerAtFixedRate`.
  pub fn start_timer_at_fixed_rate(&self, name: &str, message: Message, interval: Duration) {
    self.start_timer(name, message, interval, FsmTimerKind::FixedRate);
  }

  /// Mirrors `TestFSMRef.startSingleTimer`.
  pub fn start_single_timer(&self, name: &str, message: Message, delay: Duration) {
    self.start_timer(name, message, delay, FsmTimerKind::Single);
  }

  fn start_timer(&self, name: &str, message: Message, delay: Duration, kind: FsmTimerKind) {
    // The same condition `FsmView` puts on its repeating timers: a fixed delay
    // or fixed rate of zero would fire without end, which Pekko's scheduler
    // rejects. The check runs before the control message is sent, so the
    // machine never sees the start. A single timer may still have a delay of
    // zero, which fires at once.
    if matches!(kind, FsmTimerKind::FixedDelay | FsmTimerKind::FixedRate) {
      assert!(
        !delay.is_zero(),
        "FSM timer {name}: a repeating timer needs a delay greater than zero (Pekko rejects it as creating an infinite loop)"
      );
    }
    let sequence = self.handle.allocate_sequence();
    self.send_control(FsmControl::StartTimer { sequence, name: name.to_string(), kind, message, delay });
  }

  /// Mirrors `TestFSMRef.cancelTimer`.
  pub fn cancel_timer(&self, name: &str) {
    let sequence = self.handle.allocate_sequence();
    self.send_control(FsmControl::CancelTimer { sequence, name: name.to_string() });
  }

  /// Mirrors `TestFSMRef.isTimerActive`.
  pub fn is_timer_active(&self, name: &str) -> bool {
    self.handle.is_timer_active(name)
  }

  /// Mirrors `TestFSMRef.isStateTimerActive`.
  pub fn is_state_timer_active(&self) -> bool {
    self.handle.is_state_timer_active()
  }

  /// Sends the numbered control message that does nothing and returns its
  /// number.
  pub fn send_sentinel(&self) -> u64 {
    self.handle.send_sentinel()
  }

  /// Returns the highest number among the control messages the machine has
  /// processed.
  pub fn processed_sequence(&self) -> u64 {
    self.handle.processed_sequence()
  }
}
