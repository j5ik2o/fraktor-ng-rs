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

//! What the firing side of a test-held state machine knows about a timer.

use core::time::Duration;

use crate::kernel::actor::test_support::FsmTimerKind;

/// What the firing side needs to know about one named timer: the
/// generation it was started with, how it repeats, and how long it waits
/// between firings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FsmTimerRecord {
  generation: u64,
  kind: FsmTimerKind,
  delay: Duration,
}

impl FsmTimerRecord {
  /// Creates a record for a timer started with `generation`.
  pub fn new(generation: u64, kind: FsmTimerKind, delay: Duration) -> Self {
    Self { generation, kind, delay }
  }

  /// Returns the generation the timer was started with.
  pub fn generation(&self) -> u64 {
    self.generation
  }

  /// Returns how the timer repeats after it fires.
  pub fn kind(&self) -> FsmTimerKind {
    self.kind
  }

  /// Returns how long the timer waits between firings.
  pub fn delay(&self) -> Duration {
    self.delay
  }
}
