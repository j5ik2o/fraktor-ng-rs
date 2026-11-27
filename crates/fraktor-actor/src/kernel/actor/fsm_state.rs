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

//! The next state a finite state machine moves to after an event.

use alloc::boxed::Box;

use crate::kernel::actor::ActorContext;
use crate::kernel::actor::FsmTimeout;
use crate::kernel::actor::FsmView;
use crate::kernel::message::Message;

/// A state function, as a `whenUnhandled` replacement carried by a state.
pub(crate) type UnhandledFunction<S, D> =
  Box<dyn FnMut(&mut ActorContext<'_>, &FsmView<S>, &Message, &D) -> Option<FsmState<S, D>> + Send>;

/// The `whenUnhandled` change a state asks its machine to make when the
/// machine applies it.
/// The pieces [`FsmState::into_parts`] hands back: the target state,
/// the replacement data, the replies, the per-transition timeout
/// override, and the `whenUnhandled` change.
pub(crate) type FsmStateParts<S, D> =
  (Option<S>, Option<D>, alloc::vec::Vec<Message>, Option<FsmTimeout>, Option<UnhandledChange<S, D>>);

pub(crate) enum UnhandledChange<S, D> {
  /// Replace the unhandled function.
  Set(UnhandledFunction<S, D>),
  /// Restore the default unhandled behaviour.
  Clear,
}

/// What a state function of an [`Fsm`](crate::kernel::actor::Fsm) answers for
/// one event: the state to move to, whether the move is reported to the
/// transition handlers, and the data to keep.
///
/// Mirrors `org.apache.pekko.actor.FSM.State` as `goto`, `stay`, and `using`
/// build it. Pekko's `goto` and `stay` are methods of the machine that copy the
/// current data; the port builds the value without the machine and lets the
/// machine keep its current data unless `using` replaced it.
pub struct FsmState<S, D> {
  target: Option<S>,
  data: Option<D>,
  replies: alloc::vec::Vec<Message>,
  timeout: Option<FsmTimeout>,
  unhandled: Option<UnhandledChange<S, D>>,
}

impl<S, D> FsmState<S, D> {
  /// Moves to `state`, which the transition handlers hear about even when it
  /// is the current state.
  ///
  /// Mirrors `FSM.goto(nextStateName)`, whose state notifies the handlers.
  pub fn goto(state: S) -> Self {
    Self { target: Some(state), data: None, replies: alloc::vec::Vec::new(), timeout: None, unhandled: None }
  }

  /// Keeps the current state without telling the transition handlers.
  ///
  /// Mirrors `FSM.stay()`, which is `goto(stateName)` with the notification
  /// switched off.
  pub fn stay() -> Self {
    Self { target: None, data: None, replies: alloc::vec::Vec::new(), timeout: None, unhandled: None }
  }

  /// Replaces the data the machine keeps in the next state.
  ///
  /// Mirrors `FSM.State.using(nextStateData)`.
  pub fn using(mut self, data: D) -> Self {
    self.data = Some(data);
    self
  }

  /// Selects the state timeout of this entry, overriding the timeout the
  /// state was registered with.
  ///
  /// Mirrors `FSM.State.forMax(timeout)`: a finite duration arms the state
  /// timer for that long, and [`FsmTimeout::Infinite`] disables the state's
  /// own timeout for this entry.
  pub fn for_max(mut self, timeout: FsmTimeout) -> Self {
    self.timeout = Some(timeout);
    self
  }

  /// Replaces the machine's `whenUnhandled` function when the machine
  /// applies this state.
  ///
  /// Mirrors calling `whenUnhandled` inside a state function of the
  /// original: the replacement takes effect from the next event.
  pub fn with_unhandled<F>(mut self, function: F) -> Self
  where
    F: FnMut(&mut ActorContext<'_>, &FsmView<S>, &Message, &D) -> Option<FsmState<S, D>> + Send + 'static,
  {
    self.unhandled = Some(UnhandledChange::Set(Box::new(function)));
    self
  }

  /// Restores the machine's default unhandled behaviour when the machine
  /// applies this state.
  ///
  /// Mirrors `whenUnhandled(NullFunction)` of the original: the default,
  /// which logs a warning and stays, takes over from the next event.
  pub fn clear_unhandled(mut self) -> Self {
    self.unhandled = Some(UnhandledChange::Clear);
    self
  }

  /// Records a reply to send to the sender of the event being handled,
  /// before the transition handlers run and before the listeners are
  /// told.
  ///
  /// Mirrors `FSM.State.replying(msg)`. Pekko's `makeTransition` sends
  /// the replies first; calling `replying` more than once sends every
  /// recorded reply, in the order they were recorded. When the move
  /// names a state that does not exist, no reply is sent, as Pekko
  /// sends none when the transition does not happen.
  pub fn replying<M: core::any::Any + Send + Sync>(mut self, message: M) -> Self {
    self.replies.push(Message::new(message));
    self
  }

  /// Returns the state this value moves to, if it names one.
  pub(crate) fn target_ref(&self) -> Option<&S> {
    self.target.as_ref()
  }

  pub(crate) fn into_parts(self) -> FsmStateParts<S, D> {
    (self.target, self.data, self.replies, self.timeout, self.unhandled)
  }
}
