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

//! A clock backed by the standard library's monotonic instant.

use std::time::Instant;

use core::time::Duration;

use crate::kernel::Clock;

/// Reports monotonic time from the standard library's [`Instant`].
///
/// Mirrors the `System.nanoTime` clock that Pekko's restart window reads. The
/// epoch is the instant the clock is created, which is enough because a window
/// only compares elapsed durations.
pub struct SystemClock {
  epoch: Instant,
}

impl SystemClock {
  /// Creates a clock whose epoch is now.
  pub fn new() -> Self {
    Self { epoch: Instant::now() }
  }
}

impl Default for SystemClock {
  fn default() -> Self {
    Self::new()
  }
}

impl Clock for SystemClock {
  fn monotonic(&self) -> Duration {
    self.epoch.elapsed()
  }
}
