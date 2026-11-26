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

//! How a named timer of a state machine repeats after it fires.

/// The firing pattern of one named timer.
///
/// Mirrors the three timer starters of `org.apache.pekko.actor.FSM`:
/// `startSingleTimer`, `startTimerWithFixedDelay`, and
/// `startTimerAtFixedRate`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FsmTimerKind {
  /// Fires once, then leaves the timer table.
  Single,
  /// Fires, then waits the delay after each firing before firing again.
  FixedDelay,
  /// Fires on a schedule anchored at the moment it was started.
  FixedRate,
}
