// SPDX-License-Identifier: Apache-2.0
//
// Licensed to the Apache Software Foundation (ASF) under one or more
// license agreements; and to You under the Apache License, version 2.0:
//
//   https://www.apache.org/licenses/LICENSE-2.0
//
// This file is part of the Apache Pekko project, which was derived from Akka.
//
// Copyright (C) 2018-2022 Lightbend Inc. <https://www.lightbend.com>
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.

//! A strictly positive cache time-to-live.

use core::time::Duration;

use super::NonPositiveTtlError;

#[cfg(test)]
#[path = "ttl_test.rs"]
mod tests;

/// A cache time-to-live that is strictly greater than zero.
///
/// Mirrors `org.apache.pekko.io.dns.CachePolicy.Ttl`, whose `fromPositive`
/// rejects a non-positive value. A [`Duration`] cannot be negative, so the type
/// carries the original's negative check and only zero reaches
/// [`from_positive`](Self::from_positive) as a rejection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ttl {
  value: Duration,
}

impl Ttl {
  /// Turns a strictly positive [`Duration`] into a TTL.
  ///
  /// Returns [`NonPositiveTtlError`] for a zero duration, as the original's
  /// `fromPositive` does.
  pub fn from_positive(value: Duration) -> Result<Self, NonPositiveTtlError> {
    if value.is_zero() {
      return Err(NonPositiveTtlError);
    }
    Ok(Self { value })
  }

  /// Returns the TTL duration.
  pub fn value(&self) -> Duration {
    self.value
  }
}
