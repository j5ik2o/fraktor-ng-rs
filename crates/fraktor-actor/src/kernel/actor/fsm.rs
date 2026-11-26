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
use crate::kernel::actor_ref::ActorRef;
use crate::kernel::message::Message;

/// A state function: answers an event in one state, or returns `None` when it
/// does not handle the event.
type StateFunction<S, D> =
  Box<dyn FnMut(&mut ActorContext<'_>, &FsmView<S>, &Message, &D) -> Option<FsmState<S, D>> + Send>;

/// A transition handler: hears every reported move from one state to another,
/// with the data before and after the move.
type TransitionHandler<S, D> = Box<dyn FnMut(&mut ActorContext<'_>, &S, &S, &D, &D) + Send>;

/// The message that asks the machine to report its transitions to a listener.
///
/// Mirrors `FSM.SubscribeTransitionCallBack`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubscribeTransitionCallBack(pub ActorRef);

/// The message that asks the machine to stop reporting to a listener.
///
/// Mirrors `FSM.UnsubscribeTransitionCallBack`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnsubscribeTransitionCallBack(pub ActorRef);

/// The first report a new listener receives: the state the machine is in.
///
/// Mirrors `FSM.CurrentState`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CurrentState<S> {
  /// The machine that sent the report.
  pub fsm: ActorRef,
  /// The state the machine was in when it was subscribed.
  pub state: S,
}

/// A report of one transition, sent to every listener.
///
/// Mirrors `FSM.Transition`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Transition<S> {
  /// The machine that sent the report.
  pub fsm: ActorRef,
  /// The state the machine left.
  pub from: S,
  /// The state the machine entered.
  pub to: S,
}

/// The internal message the machine receives when a listener it watches
/// stops, through `watch_with`. It is not part of the public protocol:
/// a listener's stop is the machine's own bookkeeping, kept apart from
/// the `Terminated` a user's own watch would receive (design of #194, M4).
#[derive(Clone, Debug)]
struct ListenerTerminated(ActorRef);

/// A read-only view of the machine, handed to state functions.
///
/// Mirrors what an FSM subclass reads from itself while it handles an
/// event: the number of transition listeners and the current state
/// name. The view is a snapshot taken just before the function runs;
/// subscriptions and listener stops are handled before or after a
/// state function within one delivery, never during it, so the counts
/// are exact for the delivery being handled (design of #194, H1).
pub struct FsmView<S> {
  listener_count: usize,
  state_name: S,
}

impl<S> FsmView<S> {
  /// Returns how many listeners the machine currently reports to.
  ///
  /// Mirrors `listeners.size` read inside an FSM subclass.
  pub fn listener_count(&self) -> usize {
    self.listener_count
  }

  /// Returns the name of the state the machine is handling an event in.
  ///
  /// Mirrors `stateName`.
  pub fn state_name(&self) -> &S {
    &self.state_name
  }
}

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
/// `goto`, `stay`, and `using`, transition notices to handlers and to
/// subscribed listeners, `when_unhandled`, and replies recorded with
/// `replying`. Timers, state timeouts, and `onTermination` are not part
/// of it.
pub struct Fsm<S, D> {
  current: Option<(S, D)>,
  state_functions: Vec<(S, StateFunction<S, D>)>,
  transition_handlers: Vec<TransitionHandler<S, D>>,
  unhandled_function: Option<StateFunction<S, D>>,
  listeners: Vec<ActorRef>,
  watched_listeners: Vec<ActorRef>,
  initialized: bool,
}

impl<S, D> Fsm<S, D>
where
  S: Clone + PartialEq + Send + Sync + 'static,
  D: Send + 'static,
{
  /// Creates a machine with no state, no state functions, and no handlers.
  pub fn new() -> Self {
    Self {
      current: None,
      state_functions: Vec::new(),
      transition_handlers: Vec::new(),
      unhandled_function: None,
      listeners: Vec::new(),
      watched_listeners: Vec::new(),
      initialized: false,
    }
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
  /// the event to the next function, then to
  /// [`when_unhandled`](Self::when_unhandled), and finally to the default,
  /// which keeps the current state.
  ///
  /// The function also receives an [`FsmView`], the read-only face of
  /// the machine, as a Pekko subclass reads `listeners` and `stateName`
  /// from itself.
  pub fn when<F>(&mut self, state: S, function: F)
  where
    F: FnMut(&mut ActorContext<'_>, &FsmView<S>, &Message, &D) -> Option<FsmState<S, D>> + Send + 'static,
  {
    self.state_functions.push((state, Box::new(function)));
  }

  /// Adds the fallback state function, called when every state function
  /// of the current state answered `None`.
  ///
  /// Mirrors `FSM.whenUnhandled`. Without one, an unhandled event keeps
  /// the current state and data, which is what Pekko's default
  /// `whenUnhandled` does after it logs a warning.
  pub fn when_unhandled<F>(&mut self, function: F)
  where
    F: FnMut(&mut ActorContext<'_>, &FsmView<S>, &Message, &D) -> Option<FsmState<S, D>> + Send + 'static,
  {
    self.unhandled_function = Some(Box::new(function));
  }

  /// Adds a handler that hears every reported transition.
  ///
  /// Mirrors `FSM.onTransition(transitionHandler)`. Handlers run in the order
  /// they were added, with the state the machine leaves and the state it
  /// enters. This handler does not receive the data; use
  /// [`on_transition_with_data`](Self::on_transition_with_data) for that.
  /// Both forms share one list, so handlers of the two forms run in the
  /// order they were added.
  pub fn on_transition<F>(&mut self, handler: F)
  where
    F: FnMut(&mut ActorContext<'_>, &S, &S) + Send + 'static,
  {
    let mut handler = handler;
    self.transition_handlers.push(Box::new(move |context, from, to, _previous, _next| {
      handler(context, from, to);
    }));
  }

  /// Adds a handler that hears every reported transition together with
  /// the data before and after the move.
  ///
  /// Mirrors reading `stateData` and `nextStateData` inside
  /// `onTransition`. The next data exists only for the duration of the
  /// handler call: the machine keeps no other reference to it once the
  /// transition is done, which is the shape case 8 of
  /// `FSMTransitionSpec` checks.
  pub fn on_transition_with_data<F>(&mut self, handler: F)
  where
    F: FnMut(&mut ActorContext<'_>, &S, &S, &D, &D) + Send + 'static,
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

  fn view(&self) -> Option<FsmView<S>> {
    let (state, _) = self.current.as_ref()?;
    Some(FsmView { listener_count: self.listeners.len(), state_name: state.clone() })
  }

  fn apply_event(&mut self, context: &mut ActorContext<'_>, event: &Message) -> Option<FsmState<S, D>> {
    let view = self.view()?;
    let (state, data) = self.current.as_ref()?;
    let state = state.clone();
    let mut result = None;
    for (name, function) in self.state_functions.iter_mut().filter(|(name, _)| name == &state) {
      let _ = name;
      if let Some(next) = function(context, &view, event, data) {
        result = Some(next);
        break;
      }
    }
    if result.is_none() {
      if let Some(function) = self.unhandled_function.as_mut() {
        let (_, data) = self.current.as_ref()?;
        result = function(context, &view, event, data);
      }
    }
    result
  }

  /// Moves to `next`, telling the handlers when Pekko's `makeTransition` would.
  ///
  /// A `goto` into a state with no state function keeps the current state and
  /// data, tells no handler, notifies no listener, and sends no reply.
  /// Pekko's `makeTransition` records the failure
  /// `Next state %s does not exist` through `terminate`, which logs it and runs
  /// `onTermination`, but does not stop the actor; the port has no
  /// `onTermination` hook, so only the unchanged state remains.
  ///
  /// When the move happens, the replies recorded with `replying` are
  /// sent to the event's sender first, then the handlers run, then the
  /// listeners are told, which is Pekko's order.
  fn make_transition(
    &mut self,
    context: &mut ActorContext<'_>,
    target: Option<S>,
    data: Option<D>,
    replies: Vec<Message>,
    sender: ActorRef,
  ) {
    let Some((current_state, current_data)) = self.current.take() else {
      return;
    };
    let notifies = target.is_some();
    let next_state = target.unwrap_or_else(|| current_state.clone());
    if !self.has_state(&next_state) {
      self.current = Some((current_state, current_data));
      return;
    }
    let self_ref = context.self_ref();
    for reply in replies {
      context.tell_message(&sender, reply, self_ref.clone());
    }
    let reported = notifies || current_state != next_state;
    match data {
      | Some(next_data) => {
        if reported {
          for handler in &mut self.transition_handlers {
            handler(context, &current_state, &next_state, &current_data, &next_data);
          }
          self.notify_listeners(context, &current_state, &next_state);
        }
        self.current = Some((next_state, next_data));
      },
      | None => {
        if reported {
          for handler in &mut self.transition_handlers {
            handler(context, &current_state, &next_state, &current_data, &current_data);
          }
          self.notify_listeners(context, &current_state, &next_state);
        }
        self.current = Some((next_state, current_data));
      },
    }
  }

  fn notify_listeners(&mut self, context: &mut ActorContext<'_>, from: &S, to: &S) {
    if self.listeners.is_empty() {
      return;
    }
    let self_ref = context.self_ref();
    let notice = Transition { fsm: self_ref, from: from.clone(), to: to.clone() };
    for listener in &self.listeners {
      context.tell(listener, notice.clone());
    }
  }

  fn add_listener(&mut self, context: &mut ActorContext<'_>, listener: ActorRef) {
    // Pekko answers every subscribe with the current state, even a
    // repeated one; the set itself holds the listener once.
    if !self.listeners.contains(&listener) {
      self.listeners.push(listener.clone());
    }
    if !context.is_watching(&listener) {
      context.watch_with(&listener, ListenerTerminated(listener.clone()));
      self.watched_listeners.push(listener.clone());
    }
    // When the user already watches the listener, Pekko's `addListener`
    // logs a warning and does not take the watch over: the machine
    // neither unwatches it nor removes it from `watched_listeners` here,
    // so the user's `Terminated` is never intercepted. The warning is
    // the executor's unhandled-style log line in Pekko; the port has no
    // per-actor logger, so the branch is only the absence of action.
    let self_ref = context.self_ref();
    let state = self.current.as_ref().map(|(state, _)| state.clone());
    if let Some(state) = state {
      context.tell(&listener, CurrentState { fsm: self_ref, state });
    }
  }

  fn remove_listener(&mut self, context: &mut ActorContext<'_>, listener: &ActorRef) {
    self.listeners.retain(|existing| existing != listener);
    if self.watched_listeners.contains(listener) {
      self.watched_listeners.retain(|existing| existing != listener);
      context.unwatch(listener);
    }
  }
}

impl<S, D> Default for Fsm<S, D>
where
  S: Clone + PartialEq + Send + Sync + 'static,
  D: Send + 'static,
{
  fn default() -> Self {
    Self::new()
  }
}

impl<S, D> Actor for Fsm<S, D>
where
  S: Clone + PartialEq + Send + Sync + 'static,
  D: Send + 'static,
{
  fn pre_start(&mut self, context: &mut ActorContext<'_>) {
    if self.initialized {
      let initial = self.current.as_ref().map(|(state, _)| state.clone());
      let sender = context.self_ref();
      self.make_transition(context, initial, None, Vec::new(), sender);
    }
  }

  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    if let Some(subscribe) = message.downcast_ref::<SubscribeTransitionCallBack>() {
      let listener = subscribe.0.clone();
      self.add_listener(context, listener);
      return;
    }
    if let Some(unsubscribe) = message.downcast_ref::<UnsubscribeTransitionCallBack>() {
      let listener = unsubscribe.0.clone();
      self.remove_listener(context, &listener);
      return;
    }
    if let Some(terminated) = message.downcast_ref::<ListenerTerminated>() {
      let listener = terminated.0.clone();
      self.listeners.retain(|existing| existing != &listener);
      self.watched_listeners.retain(|existing| existing != &listener);
      return;
    }
    let sender = context.sender();
    if let Some(next) = self.apply_event(context, &message) {
      let (target, data, replies) = next.into_parts();
      self.make_transition(context, target, data, replies, sender);
    }
  }
}
