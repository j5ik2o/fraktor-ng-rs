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
  replies: alloc::vec::Vec<crate::kernel::message::Message>,
}

impl<S, D> FsmState<S, D> {
  /// Moves to `state`, which the transition handlers hear about even when it
  /// is the current state.
  ///
  /// Mirrors `FSM.goto(nextStateName)`, whose state notifies the handlers.
  pub fn goto(state: S) -> Self {
    Self { target: Some(state), data: None, replies: alloc::vec::Vec::new() }
  }

  /// Keeps the current state without telling the transition handlers.
  ///
  /// Mirrors `FSM.stay()`, which is `goto(stateName)` with the notification
  /// switched off.
  pub fn stay() -> Self {
    Self { target: None, data: None, replies: alloc::vec::Vec::new() }
  }

  /// Replaces the data the machine keeps in the next state.
  ///
  /// Mirrors `FSM.State.using(nextStateData)`.
  pub fn using(mut self, data: D) -> Self {
    self.data = Some(data);
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
    self.replies.push(crate::kernel::message::Message::new(message));
    self
  }

  pub(crate) fn into_parts(self) -> (Option<S>, Option<D>, alloc::vec::Vec<crate::kernel::message::Message>) {
    (self.target, self.data, self.replies)
  }
}
