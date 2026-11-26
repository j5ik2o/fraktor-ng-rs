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

//! A finite state machine that runs as an actor.

use alloc::boxed::Box;
use alloc::vec::Vec;

use crate::kernel::actor::Actor;
use crate::kernel::actor::ActorContext;
use crate::kernel::actor::FsmState;
use crate::kernel::message::Message;

/// A state function: answers an event in one state, or returns `None` when it
/// does not handle the event.
type StateFunction<S, D> = Box<dyn FnMut(&mut ActorContext<'_>, &Message, &D) -> Option<FsmState<S, D>> + Send>;

/// A transition handler: hears every reported move from one state to another.
type TransitionHandler<S> = Box<dyn FnMut(&mut ActorContext<'_>, &S, &S) + Send>;

/// An actor whose behaviour is a state name `S`, the data `D` it carries, and a
/// state function per state.
///
/// Mirrors `org.apache.pekko.actor.FSM` as `AbstractFSM` exposes it to Java.
/// A Pekko machine is a subclass whose constructor calls `startWith`, `when`,
/// `onTransition`, and `initialize`; the port builds the same machine as a
/// value inside the [`Props`](crate::kernel::actor::Props) factory, which is the
/// constructor body of the original. State functions and transition handlers
/// are closures that receive the actor's context, which stands for the
/// `getSelf()`, `sender()`, `stash()`, and `unstashAll()` a Pekko subclass reads
/// from itself.
///
/// The port covers starting, handling events, moving between states with
/// `goto`, `stay`, and `using`, and transition notices. Timers, state
/// timeouts, `whenUnhandled`, `onTermination`, and the `SubscribeTransitionCallBack`
/// listeners are not part of it.
pub struct Fsm<S, D> {
  current: Option<(S, D)>,
  state_functions: Vec<(S, StateFunction<S, D>)>,
  transition_handlers: Vec<TransitionHandler<S>>,
  initialized: bool,
}

impl<S, D> Fsm<S, D>
where
  S: Clone + PartialEq + Send + 'static,
  D: Send + 'static,
{
  /// Creates a machine with no state, no state functions, and no handlers.
  pub fn new() -> Self {
    Self { current: None, state_functions: Vec::new(), transition_handlers: Vec::new(), initialized: false }
  }

  /// Sets the state and data the machine starts in.
  ///
  /// Mirrors `FSM.startWith(stateName, stateData)`.
  pub fn start_with(&mut self, state: S, data: D) {
    self.current = Some((state, data));
  }

  /// Adds a state function for `state`.
  ///
  /// Mirrors `FSM.when(stateName)(stateFunction)`. A state that already has a
  /// function tries the earlier one first and falls back to the new one, as
  /// Pekko composes them with `orElse`. A function that answers `None` leaves
  /// the event unhandled; an unhandled event keeps the current state, which is
  /// what Pekko's default `whenUnhandled` does after it logs a warning.
  pub fn when<F>(&mut self, state: S, function: F)
  where
    F: FnMut(&mut ActorContext<'_>, &Message, &D) -> Option<FsmState<S, D>> + Send + 'static,
  {
    self.state_functions.push((state, Box::new(function)));
  }

  /// Adds a handler that hears every reported transition.
  ///
  /// Mirrors `FSM.onTransition(transitionHandler)`. Handlers run in the order
  /// they were added, with the state the machine leaves and the state it
  /// enters.
  pub fn on_transition<F>(&mut self, handler: F)
  where
    F: FnMut(&mut ActorContext<'_>, &S, &S) + Send + 'static,
  {
    self.transition_handlers.push(Box::new(handler));
  }

  /// Makes the machine report its move into the initial state once the actor
  /// starts.
  ///
  /// Mirrors `FSM.initialize()`, which runs the transition from the initial
  /// state to itself. Pekko runs it inside the constructor; the port runs it in
  /// `pre_start`, the first point at which the actor has a context, and before
  /// the first message.
  ///
  /// # Panics
  ///
  /// Panics when `start_with` was not called, as Pekko throws
  /// `IllegalStateException`. The panic happens while the recipe builds the
  /// actor, so the actor fails to start.
  pub fn initialize(&mut self) {
    assert!(self.current.is_some(), "You must call `startWith` before calling `initialize`");
    self.initialized = true;
  }

  fn has_state(&self, state: &S) -> bool {
    self.state_functions.iter().any(|(name, _)| name == state)
  }

  fn apply_event(&mut self, context: &mut ActorContext<'_>, event: &Message) -> Option<FsmState<S, D>> {
    let (state, data) = self.current.as_ref()?;
    self
      .state_functions
      .iter_mut()
      .filter(|(name, _)| name == state)
      .find_map(|(_, function)| function(context, event, data))
  }

  /// Moves to `next`, telling the handlers when Pekko's `makeTransition` would.
  ///
  /// A `goto` into a state with no state function keeps the current state and
  /// data and tells no handler. Pekko's `makeTransition` records the failure
  /// `Next state %s does not exist` through `terminate`, which logs it and runs
  /// `onTermination`, but does not stop the actor; the port has neither hook,
  /// so only the unchanged state remains.
  fn make_transition(&mut self, context: &mut ActorContext<'_>, target: Option<S>, data: Option<D>) {
    let Some((current_state, current_data)) = self.current.take() else {
      return;
    };
    let notifies = target.is_some();
    let next_state = target.unwrap_or_else(|| current_state.clone());
    if !self.has_state(&next_state) {
      self.current = Some((current_state, current_data));
      return;
    }
    if notifies || current_state != next_state {
      for handler in &mut self.transition_handlers {
        handler(context, &current_state, &next_state);
      }
    }
    self.current = Some((next_state, data.unwrap_or(current_data)));
  }
}

impl<S, D> Default for Fsm<S, D>
where
  S: Clone + PartialEq + Send + 'static,
  D: Send + 'static,
{
  fn default() -> Self {
    Self::new()
  }
}

impl<S, D> Actor for Fsm<S, D>
where
  S: Clone + PartialEq + Send + 'static,
  D: Send + 'static,
{
  fn pre_start(&mut self, context: &mut ActorContext<'_>) {
    if self.initialized {
      let initial = self.current.as_ref().map(|(state, _)| state.clone());
      self.make_transition(context, initial, None);
    }
  }

  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    if let Some(next) = self.apply_event(context, &message) {
      let (target, data) = next.into_parts();
      self.make_transition(context, target, data);
    }
  }
}
