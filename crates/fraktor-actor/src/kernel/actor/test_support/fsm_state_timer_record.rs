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

//! What the firing side of a test-held state machine knows about the
//! state timer.

use core::time::Duration;

/// What the firing side needs to know about the state timer: the
/// generation it was armed with and how long it waits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FsmStateTimerRecord {
  generation: u64,
  delay: Duration,
}

impl FsmStateTimerRecord {
  /// Creates a record for a state timer armed with `generation`.
  pub fn new(generation: u64, delay: Duration) -> Self {
    Self { generation, delay }
  }

  /// Returns the generation the state timer was armed with.
  pub fn generation(&self) -> u64 {
    self.generation
  }

  /// Returns how long the state timer waits before it fires.
  pub fn delay(&self) -> Duration {
    self.delay
  }
}
