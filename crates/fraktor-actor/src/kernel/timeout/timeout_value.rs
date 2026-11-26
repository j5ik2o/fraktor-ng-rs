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

//! A wrapper around a duration that makes the meaning of the duration explicit.

use core::time::Duration;

#[cfg(test)]
#[path = "timeout_value_test.rs"]
mod tests;

/// A duration that denotes how long a caller is willing to wait.
///
/// Mirrors `org.apache.pekko.util.Timeout`: it wraps a duration so that a bare
/// duration cannot be passed where a timeout is expected, and it compares by
/// value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Timeout {
  duration: Duration,
}

impl Timeout {
  /// Creates a timeout from the given duration.
  pub fn new(duration: Duration) -> Self {
    Self { duration }
  }

  /// Returns the wrapped duration.
  pub fn duration(&self) -> Duration {
    self.duration
  }
}
