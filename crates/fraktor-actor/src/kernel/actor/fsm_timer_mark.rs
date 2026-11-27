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

//! The mark the executor delivers to a machine when one of its timers fires.

use alloc::string::String;

/// How a named timer repeats.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FsmTimerMode {
  /// Fires once.
  Single,
  /// Fires again one delay after each firing.
  FixedDelay,
  /// Fires again one interval after the previous deadline.
  FixedRate,
}

/// The internal message a machine receives when a timer the executor holds
/// for it comes due.
///
/// Mirrors the `FSM.Timer` envelope of the original: a named timer carries
/// the name and the generation it was started with, and a state timeout
/// carries the generation of the state timer that was armed, so a mark
/// struck for a replaced or cancelled timer never matches the row that
/// stands in the table now. Every mark also carries the identity of the
/// machine instance that armed it, so a mark left in a mailbox across a
/// restart is dropped by the new instance.
#[derive(Clone, Debug)]
pub(crate) enum FsmTimerMark {
  /// A named timer came due.
  Named {
    /// The instance that armed the timer.
    instance_id: u64,
    /// The name the timer was started under.
    name: String,
    /// The generation the timer was started with.
    generation: u64,
  },
  /// The state timer came due.
  StateTimeout {
    /// The instance that armed the timer.
    instance_id: u64,
    /// The generation the state timer was armed with.
    generation: u64,
  },
}
