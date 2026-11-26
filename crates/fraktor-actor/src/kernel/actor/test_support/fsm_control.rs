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

//! The control messages a test sends to a state machine it drives.

use alloc::string::String;
use core::time::Duration;

use spin::Mutex;

use crate::kernel::actor::test_support::FsmTimerKind;
use crate::kernel::message::Message;
use crate::kernel::sync::Arc;

/// One instruction from a test to the machine it watches, delivered in
/// mailbox order like any other message.
///
/// Mirrors the calls `org.apache.pekko.testkit.TestFSMRef` makes on the
/// actor it wraps (`setState`, the timer starters, `cancelTimer`): the
/// port cannot hand out the actor, so each call travels as a message and
/// the machine itself carries it out in `receive`. The numbered variants
/// carry the sequence number the test assigned when it sent them; the
/// machine reports the highest number it has processed through its
/// [`FsmHandle`](crate::kernel::actor::test_support::FsmHandle), which is
/// how a test waits until everything it sent has taken effect. The two
/// firing marks carry no number and never advance it.
pub enum FsmControl<S, D> {
  /// Moves the machine as `setState` does: a `None` element keeps the
  /// current value, and `timeout` arms the state timer.
  ///
  /// The data travels behind a lock because a message is read through a
  /// shared reference while the machine needs the value itself; the
  /// machine takes it out, so a second reading finds nothing and keeps
  /// the current data.
  SetState {
    /// The sequence number the test assigned to this message.
    sequence: u64,
    /// The state to move to, or `None` to keep the current state.
    state: Option<S>,
    /// The data to replace the current data with, behind its lock, or
    /// `None` to keep the current data.
    data: Option<Arc<Mutex<Option<D>>>>,
    /// The state timeout to arm, or `None` to arm none.
    timeout: Option<Duration>,
  },
  /// Starts a named timer, replacing any timer of the same name.
  StartTimer {
    /// The sequence number the test assigned to this message.
    sequence: u64,
    /// The name the timer is started under; the empty string is a name
    /// like any other.
    name: String,
    /// How the timer repeats after it fires.
    kind: FsmTimerKind,
    /// The message the machine receives when the timer fires.
    message: Message,
    /// How long after the start, and between firings, the timer waits.
    delay: Duration,
  },
  /// Cancels a named timer; cancelling a name that is not started does
  /// nothing.
  CancelTimer {
    /// The sequence number the test assigned to this message.
    sequence: u64,
    /// The name of the timer to cancel.
    name: String,
  },
  /// Does nothing; a test sends it last and waits until its number has
  /// been processed, which proves everything sent before it has been.
  Sentinel {
    /// The sequence number the test assigned to this message.
    sequence: u64,
  },
  /// Reports that a named timer reached its deadline. The machine
  /// delivers the timer's message only when `generation` is still the
  /// generation of the timer of that name in its table; a mark for a
  /// cancelled or replaced timer is dropped.
  TimerMark {
    /// The name of the timer that fired.
    name: String,
    /// The generation the timer was started with.
    generation: u64,
  },
  /// Reports that the state timer reached its deadline. The machine
  /// delivers `StateTimeout` only when `generation` is still the
  /// machine's generation, which every received ordinary message and
  /// every accepted timer mark advances.
  StateTimeoutMark {
    /// The generation the state timer was armed with.
    generation: u64,
  },
}
