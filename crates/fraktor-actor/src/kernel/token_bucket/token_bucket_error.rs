// SPDX-License-Identifier: Apache-2.0
//
// Licensed to the Apache Software Foundation (ASF) under one or more
// license agreements; and to You under the Apache License, version 2.0:
//
//   https://www.apache.org/licenses/LICENSE-2.0
//
// This file is part of the Apache Pekko project, which was derived from Akka.
//
// Copyright (C) 2015-2022 Lightbend Inc. <https://www.lightbend.com>
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.

//! The error a token bucket build or offer reports.

use core::fmt;

/// The reason a token bucket rejected its parameters or an offer's cost.
///
/// Mirrors the `require` checks and the cost check of
/// `org.apache.pekko.util.TokenBucket`, which the original reports as
/// `IllegalArgumentException`. The checks run in the original's order, so the
/// first failure wins.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenBucketError {
  /// The capacity is negative.
  NegativeCapacity {
    /// The requested capacity.
    capacity: i64,
  },
  /// The time between tokens is zero or negative.
  NonPositivePeriod {
    /// The requested time between tokens, in nanoseconds.
    nanos_between_tokens: i64,
  },
  /// The cost of an offer is negative.
  NegativeCost {
    /// The offered cost.
    cost: i64,
  },
}

impl fmt::Display for TokenBucketError {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      | Self::NegativeCapacity { capacity } => write!(formatter, "capacity must be non-negative, got {capacity}"),
      | Self::NonPositivePeriod { nanos_between_tokens } => {
        write!(formatter, "time between tokens must be larger than zero nanoseconds, got {nanos_between_tokens}")
      },
      | Self::NegativeCost { cost } => write!(formatter, "cost must be non-negative, got {cost}"),
    }
  }
}

impl core::error::Error for TokenBucketError {}
