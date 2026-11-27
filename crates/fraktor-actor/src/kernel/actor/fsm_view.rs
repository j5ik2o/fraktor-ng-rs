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

//! A view of a state machine, handed to its state functions.

use alloc::string::String;
use alloc::vec::Vec;
use core::time::Duration;

use crate::kernel::actor::fsm_timer_mark::FsmTimerMode;
use crate::kernel::message::Message;
use crate::kernel::sync::Arc;

/// One named timer in the live table a view shares with its machine.
pub(crate) struct FsmTimerEntry {
  pub(crate) generation: u64,
  pub(crate) mode: FsmTimerMode,
  pub(crate) message: Message,
}

/// One timer operation a handler asked for, waiting to be reflected into
/// the executor's deadline index when the handler returns.
pub(crate) enum FsmTimerOp {
  /// Start (or replace) a named timer.
  Start {
    /// The name the timer runs under.
    name: String,
    /// The generation the timer was started with.
    generation: u64,
    /// How the timer repeats.
    mode: FsmTimerMode,
    /// The message the timer delivers.
    message: Message,
    /// The delay or interval of the timer.
    delay: Duration,
  },
  /// Cancel a named timer.
  Cancel {
    /// The name of the timer to cancel.
    name: String,
  },
}

/// The live named-timer table of a machine, shared between the machine
/// and the views it hands to its handlers.
///
/// A start or a cancel through a view updates this table at once, so a
/// later `is_timer_active` in the same handler answers with the state
/// the handler just made, as Pekko's `startTimer` and `cancelTimer` do.
/// The operations are also queued here, and the machine reflects them
/// into the executor's deadline index when the handler returns.
pub(crate) struct FsmTimerState {
  /// The live named timers, by name.
  pub(crate) timers: Vec<(String, FsmTimerEntry)>,
  /// The counter named-timer generations come from.
  pub(crate) named_timer_gen: u64,
  /// The operations waiting to be reflected into the executor.
  pub(crate) pending_ops: Vec<FsmTimerOp>,
}

impl FsmTimerState {
  pub(crate) fn new() -> Self {
    Self { timers: Vec::new(), named_timer_gen: 0, pending_ops: Vec::new() }
  }

  pub(crate) fn start(&mut self, name: &str, message: Message, delay: Duration, mode: FsmTimerMode) {
    self.named_timer_gen += 1;
    let generation = self.named_timer_gen;
    self.timers.retain(|(used, _)| used != name);
    self.timers.push((name.into(), FsmTimerEntry { generation, mode, message: message.clone() }));
    self.pending_ops.push(FsmTimerOp::Start { name: name.into(), generation, mode, message, delay });
  }

  pub(crate) fn cancel(&mut self, name: &str) {
    self.timers.retain(|(used, _)| used != name);
    self.pending_ops.push(FsmTimerOp::Cancel { name: name.into() });
  }
}

/// A view of the machine, handed to state functions and to transition
/// handlers that ask for one.
///
/// Mirrors what an FSM subclass reads from itself while it handles an
/// event: the number of transition listeners, the current state name,
/// and the named timers, which a subclass starts and cancels on itself.
/// The listener count and the state name are a snapshot taken just
/// before the function runs; subscriptions and listener stops are
/// handled before or after a state function within one delivery, never
/// during it, so the counts are exact for the delivery being handled.
/// The timer table is live: a start or a cancel is visible to a later
/// read in the same handler at once.
pub struct FsmView<S> {
  listener_count: usize,
  state_name: S,
  timers: Option<Arc<spin::Mutex<FsmTimerState>>>,
}

impl<S> FsmView<S> {
  /// Creates a view that shares the machine's live timer table.
  pub(crate) fn with_timers(listener_count: usize, state_name: S, timers: Arc<spin::Mutex<FsmTimerState>>) -> Self {
    Self { listener_count, state_name, timers: Some(timers) }
  }

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

  /// Starts a timer that delivers `message` to the machine once, after
  /// `delay`.
  ///
  /// Mirrors `FSM.startSingleTimer`. Starting a name that already runs
  /// replaces it. The start is reflected in the live table at once, so
  /// [`is_timer_active`](Self::is_timer_active) answers for it in the
  /// same handler. A delay of zero fires at the next firing entry, as
  /// the original executes a zero-delay one-shot at once.
  pub fn start_single_timer(&self, name: &str, message: Message, delay: Duration) {
    if let Some(timers) = &self.timers {
      timers.lock().start(name, message, delay, FsmTimerMode::Single);
    }
  }

  /// Starts a timer that delivers `message`, then again one `delay`
  /// after each delivery, until it is cancelled.
  ///
  /// Mirrors `FSM.startTimerWithFixedDelay`.
  ///
  /// # Panics
  ///
  /// Panics when `delay` is zero, as Pekko's scheduler rejects a
  /// repeating timer whose period is not positive.
  pub fn start_timer_with_fixed_delay(&self, name: &str, message: Message, delay: Duration) {
    assert!(
      !delay.is_zero(),
      "FSM timer {name}: a repeating timer needs a delay greater than zero (Pekko rejects it as creating an infinite loop)"
    );
    if let Some(timers) = &self.timers {
      timers.lock().start(name, message, delay, FsmTimerMode::FixedDelay);
    }
  }

  /// Starts a timer that delivers `message` every `interval`, at a
  /// fixed rate, until it is cancelled.
  ///
  /// Mirrors `FSM.startTimerAtFixedRate`.
  ///
  /// # Panics
  ///
  /// Panics when `interval` is zero, as Pekko's scheduler rejects a
  /// repeating timer whose period is not positive.
  pub fn start_timer_at_fixed_rate(&self, name: &str, message: Message, interval: Duration) {
    assert!(
      !interval.is_zero(),
      "FSM timer {name}: a repeating timer needs a delay greater than zero (Pekko rejects it as creating an infinite loop)"
    );
    if let Some(timers) = &self.timers {
      timers.lock().start(name, message, interval, FsmTimerMode::FixedRate);
    }
  }

  /// Cancels the timer running under `name`, if any.
  ///
  /// Mirrors `FSM.cancelTimer`. Cancelling a name that does not run
  /// changes nothing. The cancel is reflected in the live table at
  /// once, so [`is_timer_active`](Self::is_timer_active) answers for it
  /// in the same handler.
  pub fn cancel_timer(&self, name: &str) {
    if let Some(timers) = &self.timers {
      timers.lock().cancel(name);
    }
  }

  /// Returns whether a timer runs under `name`.
  ///
  /// Mirrors `FSM.isTimerActive`, reading the live table, so a start or
  /// a cancel made earlier in the same handler is already reflected.
  pub fn is_timer_active(&self, name: &str) -> bool {
    match &self.timers {
      | Some(timers) => timers.lock().timers.iter().any(|(used, _)| used == name),
      | None => false,
    }
  }
}
