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
use alloc::string::String;
use alloc::vec::Vec;
use core::time::Duration;

use portable_atomic::AtomicU64;
use portable_atomic::Ordering;

use crate::kernel::actor::fsm_state::UnhandledChange;
use crate::kernel::actor::fsm_view::FsmTimerOp;
use crate::kernel::actor::fsm_view::FsmTimerState;
#[cfg(feature = "test-support")]
use crate::kernel::actor::test_support::FsmControl;
#[cfg(feature = "test-support")]
use crate::kernel::actor::test_support::FsmHandle;
#[cfg(feature = "test-support")]
use crate::kernel::actor::test_support::FsmStateTimerRecord;
#[cfg(feature = "test-support")]
use crate::kernel::actor::test_support::FsmTimerKind;
#[cfg(feature = "test-support")]
use crate::kernel::actor::test_support::FsmTimerRecord;
use crate::kernel::actor::Actor;
use crate::kernel::actor::ActorContext;
use crate::kernel::actor::CurrentState;
use crate::kernel::actor::FsmState;
use crate::kernel::actor::FsmTimeout;
use crate::kernel::actor::FsmTimerMark;
use crate::kernel::actor::FsmView;
use crate::kernel::actor::StateTimeout;
use crate::kernel::actor::SubscribeTransitionCallBack;
use crate::kernel::actor::Transition;
use crate::kernel::actor::UnsubscribeTransitionCallBack;
use crate::kernel::actor_ref::ActorRef;
use crate::kernel::message::Message;
use crate::kernel::sync::Arc;

/// The counter machine instance identities come from.
static NEXT_INSTANCE: AtomicU64 = AtomicU64::new(1);

/// A state function: answers an event in one state, or returns `None` when it
/// does not handle the event.
type StateFunction<S, D> =
  Box<dyn FnMut(&mut ActorContext<'_>, &FsmView<S>, &Message, &D) -> Option<FsmState<S, D>> + Send>;

/// A transition handler: hears every reported move from one state to another,
/// with the data before and after the move.
type PlainTransitionHandler<S, D> = Box<dyn FnMut(&mut ActorContext<'_>, &S, &S, &D, &D) + Send>;

/// A transition handler that also receives the machine's view, so it can
/// start and cancel named timers while a move is reported.
type ViewTransitionHandler<S> = Box<dyn FnMut(&mut ActorContext<'_>, &FsmView<S>, &S, &S) + Send>;

/// One registered transition handler, kept in the order it was added.
///
/// The plain form hears the move with the data before and after it;
/// the view form also receives the machine's view. The two forms
/// share this one list, so handlers of either form run in the order
/// they were added, as the original's single `onTransition` chain
/// does.
enum TransitionHandler<S, D> {
  Plain(PlainTransitionHandler<S, D>),
  WithView(ViewTransitionHandler<S>),
}

/// The internal message the machine receives when a listener it watches
/// stops, through `watch_with`. It is not part of the public protocol:
/// a listener's stop is the machine's own bookkeeping, kept apart from
/// the `Terminated` a user's own watch would receive.
#[derive(Clone, Debug)]
struct ListenerTerminated(ActorRef);

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
/// `replying`. `onTermination` is not part of it. Named timers and the
/// state timeout are driven by the executor's deadline index: the
/// machine keeps the live timer table, the generations, and the
/// judgement of firing marks in `receive`, and the executor sends a
/// mark when a deadline comes. Under the `test-support` feature a
/// machine a test holds by a handle also mirrors its timers into the
/// handle, and the test's control messages drive the same table.
pub struct Fsm<S, D> {
  current: Option<(S, D)>,
  state_functions: Vec<(S, StateFunction<S, D>)>,
  state_timeouts: Vec<(S, Option<Duration>)>,
  transition_handlers: Vec<TransitionHandler<S, D>>,
  unhandled_function: Option<StateFunction<S, D>>,
  listeners: Vec<ActorRef>,
  watched_listeners: Vec<ActorRef>,
  initialized: bool,
  timer_state: Arc<spin::Mutex<FsmTimerState>>,
  instance_id: u64,
  state_generation: u64,
  state_timer_armed: Option<u64>,
  state_display: Option<fn(&S) -> String>,
  #[cfg(feature = "test-support")]
  handle: Option<FsmHandle<S, D>>,
  #[cfg(feature = "test-support")]
  state_timer: Option<u64>,
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
      state_timeouts: Vec::new(),
      transition_handlers: Vec::new(),
      unhandled_function: None,
      listeners: Vec::new(),
      watched_listeners: Vec::new(),
      initialized: false,
      timer_state: Arc::new(spin::Mutex::new(FsmTimerState::new())),
      instance_id: NEXT_INSTANCE.fetch_add(1, Ordering::Relaxed),
      state_generation: 0,
      state_timer_armed: None,
      state_display: None,
      #[cfg(feature = "test-support")]
      handle: None,
      #[cfg(feature = "test-support")]
      state_timer: None,
    }
  }

  /// Sets the state and data the machine starts in.
  ///
  /// Mirrors `FSM.startWith(stateName, stateData)`.
  pub fn start_with(&mut self, state: S, data: D) {
    self.current = Some((state, data));
    #[cfg(feature = "test-support")]
    if let (Some(handle), Some((state, data))) = (&self.handle, &self.current) {
      handle.write_snapshot(state, data);
    }
  }

  /// Attaches the handle a test reads this machine through and sends it
  /// control messages with.
  ///
  /// The handle starts over for this instance: its snapshot becomes the
  /// state the machine starts in, so a test reads the starting state from
  /// the moment the machine exists, before any message and whether or
  /// not [`initialize`](Self::initialize) is called, and its timer
  /// records are emptied, as a restarted machine holds no timers. The
  /// factory a test wraps calls this once per (re)creation with the same
  /// handle, so the handle's sequence numbers and generation counter run
  /// on across a restart.
  ///
  /// Only a machine built for a test carries a handle; a machine built
  /// the usual way never pays for one. Reading the snapshot copies the
  /// data, which is why the copy is set up here, the one place that
  /// requires `D: Clone`: Pekko's `stateData` hands out a reference,
  /// which the port's ownership model does not allow for the actor
  /// itself, so the snapshot is the reference's replacement.
  #[cfg(feature = "test-support")]
  pub fn attach_handle(&mut self, handle: FsmHandle<S, D>)
  where
    D: Clone,
  {
    let cloner: crate::kernel::actor::test_support::FsmDataCloner<D> =
      crate::kernel::sync::Arc::new(|data: &D| data.clone());
    match self.current.as_ref() {
      | Some((state, data)) => handle.install(Some((state, data)), cloner),
      | None => handle.install(None, cloner),
    }
    self.handle = Some(handle);
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

  /// Adds a state function for `state`, with the state's own timeout.
  ///
  /// Mirrors `FSM.when(stateName, stateTimeout = ...)(stateFunction)`.
  /// A state that already has a function tries the earlier one first and
  /// falls back to the new one, as Pekko composes them with `orElse`,
  /// and the timeouts compose as `timeout.or(existing)`: a registration
  /// with a timeout replaces the state's timeout, and one without
  /// keeps it.
  pub fn when_with_state_timeout<F>(&mut self, state: S, timeout: Option<Duration>, function: F)
  where
    F: FnMut(&mut ActorContext<'_>, &FsmView<S>, &Message, &D) -> Option<FsmState<S, D>> + Send + 'static,
  {
    match self.state_timeouts.iter_mut().find(|(name, _)| name == &state) {
      | Some((_, slot)) => {
        if timeout.is_some() {
          *slot = timeout;
        }
      },
      | None => self.state_timeouts.push((state.clone(), timeout)),
    }
    self.state_functions.push((state, Box::new(function)));
  }

  /// Adds a handler that hears every reported transition together with
  /// the machine's view, so it can start and cancel named timers.
  ///
  /// Mirrors the part of `FSM.onTransition` that reads the machine
  /// itself: the handler receives an [`FsmView`] whose timer table is
  /// live, and the operations it makes are reflected into the
  /// executor's deadline index when the handler returns. The view's
  /// state name is the state the machine leaves, as the original runs
  /// its transition handlers before it updates the current state.
  pub fn on_transition_with_view<F>(&mut self, handler: F)
  where
    F: FnMut(&mut ActorContext<'_>, &FsmView<S>, &S, &S) + Send + 'static,
  {
    self.transition_handlers.push(TransitionHandler::WithView(Box::new(handler)));
  }

  /// Registers `Display` as the way state names are printed in the
  /// warning for an unhandled event.
  ///
  /// Mirrors the `toString` of the original's state names. Without this
  /// registration the warning names the state by its type name. The
  /// registration is a separate call, rather than a bound on
  /// [`Fsm`]'s state type, so machines whose states are not `Display`
  /// keep building as they did.
  pub fn with_state_display(&mut self)
  where
    S: core::fmt::Display,
  {
    self.state_display = Some(|state: &S| alloc::format!("{state}"));
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
    self.transition_handlers.push(TransitionHandler::Plain(Box::new(move |context, from, to, _previous, _next| {
      handler(context, from, to);
    })));
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
    self.transition_handlers.push(TransitionHandler::Plain(Box::new(handler)));
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
    Some(FsmView::with_timers(self.listeners.len(), state.clone(), self.timer_state.clone()))
  }

  /// Returns the text the warning for an unhandled event names `state`
  /// by: its `Display` when one was registered, else its type name.
  fn state_text(&self, state: &S) -> String {
    match self.state_display {
      | Some(display) => display(state),
      | None => core::any::type_name::<S>().into(),
    }
  }

  /// Reflects the timer operations the handlers made into the
  /// executor's deadline index, in the order they were made.
  fn flush_timer_ops(&mut self, context: &mut ActorContext<'_>) {
    let ops: Vec<FsmTimerOp> = {
      let mut state = self.timer_state.lock();
      core::mem::take(&mut state.pending_ops)
    };
    for op in ops {
      match op {
        | FsmTimerOp::Start { name, generation, mode, message, delay } => {
          context.schedule_fsm_named_timer(&name, generation, mode, message, delay, self.instance_id);
        },
        | FsmTimerOp::Cancel { name } => {
          context.cancel_fsm_named_timer(&name);
          #[cfg(feature = "test-support")]
          if let Some(handle) = &self.handle {
            handle.remove_timer_record(&name);
          }
        },
      }
    }
    #[cfg(feature = "test-support")]
    if let Some(handle) = &self.handle {
      let state = self.timer_state.lock();
      for (name, entry) in &state.timers {
        let kind = match entry.mode {
          | crate::kernel::actor::FsmTimerMode::Single => FsmTimerKind::Single,
          | crate::kernel::actor::FsmTimerMode::FixedDelay => FsmTimerKind::FixedDelay,
          | crate::kernel::actor::FsmTimerMode::FixedRate => FsmTimerKind::FixedRate,
        };
        handle.set_timer_record(name, FsmTimerRecord::new(entry.generation, kind, Duration::ZERO));
      }
    }
  }

  /// Cancels the state timer, in the executor and in the machine's own
  /// record, and advances the state generation, as Pekko's `receive`
  /// does for every ordinary message it takes.
  fn cancel_state_timer_with_context(&mut self, context: &mut ActorContext<'_>) {
    self.state_generation += 1;
    self.state_timer_armed = None;
    context.cancel_fsm_state_timer();
    #[cfg(feature = "test-support")]
    self.cancel_state_timer();
  }

  /// Arms the state timer for `timeout` with the current generation.
  fn arm_state_timer_with_context(&mut self, context: &mut ActorContext<'_>, timeout: Duration) {
    let generation = self.state_generation;
    self.state_timer_armed = Some(generation);
    context.schedule_fsm_state_timer(generation, timeout, self.instance_id);
    #[cfg(feature = "test-support")]
    self.arm_state_timer(timeout);
  }

  /// Applies one answered state: flushes the timer operations, runs
  /// the move, and arms the state timer the move selects. Returns the
  /// `whenUnhandled` change the state carries; the caller applies it
  /// with [`apply_unhandled_change`](Self::apply_unhandled_change), so
  /// a caller that offers the event once more can hold the change
  /// back until the second offer has run, as a change takes effect
  /// from the next event.
  ///
  /// The timer selection is Pekko's `makeTransition` rule: a
  /// `for_max` finite duration arms for that long, `for_max` infinite
  /// arms nothing, and without an override the timeout the entered
  /// state was registered with arms, if it has one.
  fn apply_next(
    &mut self,
    context: &mut ActorContext<'_>,
    next: FsmState<S, D>,
    sender: ActorRef,
  ) -> Option<UnhandledChange<S, D>> {
    let (target, data, replies, timeout, unhandled) = next.into_parts();
    self.flush_timer_ops(context);
    let moved = self.make_transition(context, target, data, replies, sender);
    self.flush_timer_ops(context);
    if !moved {
      // A move into a state that does not exist cancels every timer,
      // as the original's `terminate` on that path does.
      let names: Vec<String> = self.timer_state.lock().timers.iter().map(|(name, _)| name.clone()).collect();
      self.timer_state.lock().timers.clear();
      for name in &names {
        context.cancel_fsm_named_timer(name);
      }
      self.state_timer_armed = None;
      context.cancel_fsm_state_timer();
      return unhandled;
    }
    let selected = match timeout {
      | Some(FsmTimeout::Infinite) => None,
      | Some(FsmTimeout::After(duration)) => Some(duration),
      | None => self.current.as_ref().and_then(|(state, _)| {
        self.state_timeouts.iter().find(|(name, _)| name == state).and_then(|(_, timeout)| *timeout)
      }),
    };
    if let Some(duration) = selected {
      self.arm_state_timer_with_context(context, duration);
    }
    unhandled
  }

  /// Applies a `whenUnhandled` change an answered state carried.
  fn apply_unhandled_change(&mut self, change: Option<UnhandledChange<S, D>>) {
    if let Some(change) = change {
      match change {
        | UnhandledChange::Set(function) => self.unhandled_function = Some(function),
        | UnhandledChange::Clear => self.unhandled_function = None,
      }
    }
  }

  /// Returns whether `event` is a text message whose text names
  /// `state`: the original's `case Event(x: State, _) => goto(x)`
  /// shape, where the event is the state it enters, carried as text
  /// by the port.
  fn event_names_state(&self, event: &Message, state: &S) -> bool {
    let text = match event.downcast_ref::<&'static str>() {
      | Some(text) => String::from(*text),
      | None => match event.downcast_ref::<String>() {
        | Some(text) => text.clone(),
        | None => return false,
      },
    };
    text == self.state_text(state)
  }

  /// Runs one event through the state functions and the move it
  /// answers, and returns whether a function handled it.
  ///
  /// When the move enters a different state and the event names the
  /// state it enters, the event is offered once more to the entered
  /// state's functions in the same turn: the original arms a state's
  /// timers from the handler of the event that names the state (and,
  /// for one case, from the transition into it), and the port's state
  /// functions are written the same way. The second answer's move,
  /// data, replies, and timeout are not applied; only its timer
  /// operations and its `whenUnhandled` change are, because the first
  /// answer already decided the move.
  fn offer_event(&mut self, context: &mut ActorContext<'_>, event: &Message, reoffer: bool) -> bool {
    self.offer_event_forwarding(context, event, reoffer, None)
  }

  /// [`Self::offer_event`] with the timer forwarding a named timer's
  /// receipt performs: `forward` names the timer and the sender its
  /// mark arrived from, and a handled message goes to that sender
  /// before the move is applied, as the original's handler sends it
  /// before the transition completes. A handling that cancels the
  /// timer does not forward its message.
  fn offer_event_forwarding(
    &mut self,
    context: &mut ActorContext<'_>,
    event: &Message,
    reoffer: bool,
    forward: Option<(&str, ActorRef)>,
  ) -> bool {
    let Some(handling_state) = self.current.as_ref().map(|(state, _)| state.clone()) else {
      return false;
    };
    let sender = context.sender();
    let next = match self.apply_event(context, event) {
      | Some(next) => next,
      | None => {
        self.apply_unhandled(context, event);
        return false;
      },
    };
    if let Some((timer_name, mark_sender)) = forward {
      let cancels = {
        let state = self.timer_state.lock();
        state.pending_ops.iter().any(|op| matches!(op, FsmTimerOp::Cancel { name } if name == timer_name))
      };
      let self_ref = context.self_ref();
      if !cancels && mark_sender != self_ref {
        context.tell_message(&mark_sender, event.clone(), self_ref);
      }
    }
    let target = next.target_ref().cloned();
    let mut final_change = self.apply_next(context, next, sender);
    if reoffer {
      if let Some(entered) = target {
        let now_in_entered = entered != handling_state
          && self.event_names_state(event, &entered)
          && self.current.as_ref().is_some_and(|(state, _)| state == &entered);
        if now_in_entered {
          // The first answer's `whenUnhandled` change is still held
          // back here: a change takes effect from the next event, so
          // the second offer of this event reads the fallback that
          // stood when the event arrived. The timer operations the
          // second offer makes are flushed whether it answers or
          // not, and a change the second answer carries replaces the
          // first answer's as the one that takes effect.
          if let Some(second) = self.apply_event(context, event) {
            let (_, _, _, _, unhandled) = second.into_parts();
            if unhandled.is_some() {
              final_change = unhandled;
            }
          }
          self.flush_timer_ops(context);
        }
      }
    }
    self.apply_unhandled_change(final_change);
    true
  }

  /// Runs the default for an event no function handled: the warning,
  /// then a `stay` through the transition path, which re-arms the
  /// current state's own timeout.
  fn apply_unhandled(&mut self, context: &mut ActorContext<'_>, event: &Message) {
    let Some((state, _)) = self.current.as_ref() else {
      return;
    };
    let state = state.clone();
    let text = alloc::format!("unhandled event {} in state {}", event.display_text(), self.state_text(&state));
    context.publish_warning(&text);
    let sender = context.sender();
    let change = self.apply_next(context, FsmState::stay(), sender);
    self.apply_unhandled_change(change);
  }

  fn apply_event(&mut self, context: &mut ActorContext<'_>, event: &Message) -> Option<FsmState<S, D>> {
    let view = self.view()?;
    let (state, data) = self.current.as_ref()?;
    let state = state.clone();
    let mut result = None;
    for (_, function) in self.state_functions.iter_mut().filter(|(name, _)| name == &state) {
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
  /// listeners are told, which is Pekko's order. Returns whether the move
  /// happened, which a caller that arms the state timer only for a move
  /// that took place needs to know.
  fn make_transition(
    &mut self,
    context: &mut ActorContext<'_>,
    target: Option<S>,
    data: Option<D>,
    replies: Vec<Message>,
    sender: ActorRef,
  ) -> bool {
    let Some((current_state, current_data)) = self.current.take() else {
      return false;
    };
    let notifies = target.is_some();
    let next_state = target.unwrap_or_else(|| current_state.clone());
    if !self.has_state(&next_state) {
      self.current = Some((current_state, current_data));
      return false;
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
            match handler {
              | TransitionHandler::Plain(handler) => {
                handler(context, &current_state, &next_state, &current_data, &next_data);
              },
              | TransitionHandler::WithView(handler) => {
                let view = FsmView::with_timers(self.listeners.len(), current_state.clone(), self.timer_state.clone());
                handler(context, &view, &current_state, &next_state);
              },
            }
          }
          self.notify_listeners(context, &current_state, &next_state);
        }
        self.current = Some((next_state, next_data));
      },
      | None => {
        if reported {
          for handler in &mut self.transition_handlers {
            match handler {
              | TransitionHandler::Plain(handler) => {
                handler(context, &current_state, &next_state, &current_data, &current_data);
              },
              | TransitionHandler::WithView(handler) => {
                let view = FsmView::with_timers(self.listeners.len(), current_state.clone(), self.timer_state.clone());
                handler(context, &view, &current_state, &next_state);
              },
            }
          }
          self.notify_listeners(context, &current_state, &next_state);
        }
        self.current = Some((next_state, current_data));
      },
    }
    #[cfg(feature = "test-support")]
    if let (Some(handle), Some((state, data))) = (&self.handle, &self.current) {
      handle.write_snapshot(state, data);
    }
    true
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

  /// Cancels the state timer and advances the machine's generation, as
  /// Pekko's `receive` does for every message it takes: a state timer
  /// armed before this message can no longer fire, because its mark
  /// carries the older generation. The reads as active flag goes with
  /// it. A machine without a handle holds no state timer, so there is
  /// nothing to cancel.
  #[cfg(feature = "test-support")]
  fn cancel_state_timer(&mut self) {
    if let Some(handle) = &self.handle {
      handle.next_generation();
      self.state_timer = None;
      handle.set_state_timer(None, false);
    }
  }

  /// Arms the state timer with the machine's current generation, so the
  /// next received ordinary message, which advances the generation,
  /// cancels it.
  #[cfg(feature = "test-support")]
  fn arm_state_timer(&mut self, timeout: Duration) {
    if let Some(handle) = &self.handle {
      let generation = handle.generation();
      self.state_timer = Some(generation);
      handle.set_state_timer(Some(FsmStateTimerRecord::new(generation, timeout)), true);
    }
  }

  #[cfg(feature = "test-support")]
  fn note_processed(&self, sequence: u64) {
    if let Some(handle) = &self.handle {
      handle.note_processed(sequence);
    }
  }

  /// Runs one event through the state functions and the move it answers,
  /// as the ordinary path of `receive` does.
  #[cfg(feature = "test-support")]
  fn process_event(&mut self, context: &mut ActorContext<'_>, event: &Message) {
    let sender = context.sender();
    match self.apply_event(context, event) {
      | Some(next) => {
        let change = self.apply_next(context, next, sender);
        self.apply_unhandled_change(change);
      },
      | None => self.apply_unhandled(context, event),
    }
  }

  /// Carries out one control message from the test that drives this
  /// machine, in mailbox order, as the design of the `TestFSMRef` port
  /// records. Timer operations and the sentinel leave the state timer
  /// alone: in Pekko they are calls on the actor's proxies, not messages
  /// through `receive`, so they neither cancel the state timer nor
  /// advance the generation.
  #[cfg(feature = "test-support")]
  fn handle_control(&mut self, context: &mut ActorContext<'_>, control: &FsmControl<S, D>) {
    match control {
      | FsmControl::SetState { sequence, state, data, timeout } => {
        // Setting the state is a move from inside the machine, so the
        // state timer of the state that is left is cancelled first, as
        // the promise `setState` documents; Pekko's own call path keeps
        // the old timer armed, which the port does not copy.
        self.cancel_state_timer_with_context(context);
        let target = state.clone().or_else(|| self.current.as_ref().map(|(name, _)| name.clone()));
        let data = data.as_ref().and_then(|slot| slot.lock().take());
        let mut applied = false;
        if let Some(target) = target {
          let sender = context.sender();
          applied = self.make_transition(context, Some(target), data, Vec::new(), sender);
        }
        if applied {
          if let Some(timeout) = timeout {
            self.arm_state_timer_with_context(context, *timeout);
          }
        }
        self.note_processed(*sequence);
      },
      | FsmControl::StartTimer { sequence, name, kind, message, delay } => {
        // A timer a test starts stands in the same live table and the
        // same executor index as a timer a handler starts through a
        // view, so the executor is the one place that fires it, and a
        // cancel from either side reaches it.
        let mode = match kind {
          | FsmTimerKind::Single => crate::kernel::actor::FsmTimerMode::Single,
          | FsmTimerKind::FixedDelay => crate::kernel::actor::FsmTimerMode::FixedDelay,
          | FsmTimerKind::FixedRate => crate::kernel::actor::FsmTimerMode::FixedRate,
        };
        {
          let mut state = self.timer_state.lock();
          state.start(name, message.clone(), *delay, mode);
        }
        self.flush_timer_ops(context);
        self.note_processed(*sequence);
      },
      | FsmControl::CancelTimer { sequence, name } => {
        // The cancel goes through the same table and the same queued
        // operation as a handler's cancel, so flushing it removes the
        // executor's deadline and the handle's record together.
        {
          let mut state = self.timer_state.lock();
          state.cancel(name);
        }
        self.flush_timer_ops(context);
        self.note_processed(*sequence);
      },
      | FsmControl::Sentinel { sequence } => {
        self.note_processed(*sequence);
      },
      | FsmControl::TimerMark { name, generation } => {
        // The mark is judged against the one live table, as the
        // executor's mark is.
        let accepted = {
          let state = self.timer_state.lock();
          state
            .timers
            .iter()
            .find(|(used, _)| used == name)
            .and_then(|(_, entry)| (entry.generation == *generation).then(|| (entry.mode, entry.message.clone())))
        };
        if let Some((mode, message)) = accepted {
          if mode == crate::kernel::actor::FsmTimerMode::Single {
            self.timer_state.lock().timers.retain(|(used, _)| used != name);
            if let Some(handle) = &self.handle {
              handle.remove_timer_record(name);
            }
          }
          // A timer receipt is an ordinary receipt for the state timer:
          // it cancels it and advances the generation, as Pekko's does.
          self.cancel_state_timer_with_context(context);
          self.process_event(context, &message);
        }
      },
      | FsmControl::StateTimeoutMark { generation } => {
        // The armed slot alone judges the mark. Every path that
        // invalidates the state timer clears the slot (an ordinary
        // message, an accepted timer receipt, and `SetState` all cancel
        // it first), and a re-armed timer holds a later generation, so
        // a stale mark never matches. The shared counter is not part of
        // the judgement: starting a named timer advances it for the
        // timer's own generation, and Pekko keeps the state timeout's
        // generation apart from the named timers' generations, so that
        // advance must not invalidate the state timer.
        if self.state_timer == Some(*generation) {
          // The receipt itself neither cancels the state timer nor
          // advances the generation, as in Pekko: the timer still reads
          // as active until the next ordinary message.
          self.process_event(context, &Message::new(StateTimeout));
        }
      },
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
      let moved = self.make_transition(context, initial, None, Vec::new(), sender);
      self.flush_timer_ops(context);
      if moved {
        let selected = self.current.as_ref().and_then(|(state, _)| {
          self.state_timeouts.iter().find(|(name, _)| name == state).and_then(|(_, timeout)| *timeout)
        });
        if let Some(duration) = selected {
          self.arm_state_timer_with_context(context, duration);
        }
      }
    }
    #[cfg(feature = "test-support")]
    if let Some(handle) = &self.handle {
      handle.mark_started();
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
    if let Some(mark) = message.downcast_ref::<FsmTimerMark>() {
      match mark.clone() {
        | FsmTimerMark::Named { instance_id, name, generation } => {
          if instance_id != self.instance_id {
            return;
          }
          let accepted = {
            let state = self.timer_state.lock();
            state
              .timers
              .iter()
              .find(|(used, _)| used == &name)
              .and_then(|(_, entry)| (entry.generation == generation).then(|| (entry.mode, entry.message.clone())))
          };
          let Some((mode, timer_message)) = accepted else {
            return;
          };
          if mode == crate::kernel::actor::FsmTimerMode::Single {
            self.timer_state.lock().timers.retain(|(used, _)| used != &name);
            #[cfg(feature = "test-support")]
            if let Some(handle) = &self.handle {
              handle.remove_timer_record(&name);
            }
          }
          // A timer receipt is an ordinary receipt for the state
          // timer: it cancels it and advances the generation.
          self.cancel_state_timer_with_context(context);
          let mark_sender = context.sender();
          // The original's machine forwards a timer message it handled
          // to the tester it was built with; the port's machine is
          // built without one, so the message goes to the sender the
          // mark arrived from, which is the sender of the event that
          // armed the timer.
          self.offer_event_forwarding(context, &timer_message, true, Some((&name, mark_sender)));
        },
        | FsmTimerMark::StateTimeout { instance_id, generation } => {
          if instance_id != self.instance_id {
            return;
          }
          if self.state_timer_armed != Some(generation) || self.state_generation != generation {
            return;
          }
          // The receipt itself neither cancels the state timer nor
          // advances the generation, as in Pekko: the timer still
          // reads as armed until the next ordinary message.
          let event = Message::new(StateTimeout);
          self.offer_event(context, &event, true);
        },
      }
      return;
    }
    #[cfg(feature = "test-support")]
    if let Some(control) = message.downcast_ref::<FsmControl<S, D>>() {
      self.handle_control(context, control);
      return;
    }
    // Every ordinary message cancels the state timer, handled or not,
    // as Pekko's `receive` does before it runs the state function. The
    // subscribe, unsubscribe, and listener-terminated receipts above
    // are the exceptions Pekko names, and they return before this point.
    self.cancel_state_timer_with_context(context);
    self.offer_event(context, &message, true);
  }

  fn post_stop_with_context(&mut self, context: &mut ActorContext<'_>) {
    // The executor's deadlines belong to this instance: a restart
    // builds a new instance under the same reference, and a repeating
    // timer the old instance armed must not keep firing at it, as the
    // original's `terminate` cancels every timer of the machine it
    // ends. The plain `post_stop` cleanup runs as well, so a stop
    // through either hook leaves the same state behind.
    context.clear_fsm_timers();
    self.post_stop();
  }

  fn post_stop(&mut self) {
    self.timer_state.lock().timers.clear();
    self.state_timer_armed = None;
    // A stopped machine leaves an empty snapshot and no timers behind,
    // so a test that reads the handle of a stopped machine sees that it
    // is gone, and a restarted machine starts from the fresh snapshot
    // its new instance writes when it attaches.
    #[cfg(feature = "test-support")]
    if let Some(handle) = &self.handle {
      handle.clear();
    }
  }
}
