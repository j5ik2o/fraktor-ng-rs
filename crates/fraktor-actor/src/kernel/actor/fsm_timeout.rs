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

//! A per-transition override of a state's timeout.

use core::time::Duration;

/// The state timeout a single transition selects, overriding the timeout
/// the state it enters was registered with.
///
/// Mirrors the `Duration` a Pekko `FSM.State.forMax(timeout)` carries:
/// a finite duration arms the state timer for that long, and the infinite
/// duration disables the state's own timeout for this entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FsmTimeout {
  /// Arm the state timer for this long.
  After(Duration),
  /// Do not arm the state timer for this entry.
  Infinite,
}
