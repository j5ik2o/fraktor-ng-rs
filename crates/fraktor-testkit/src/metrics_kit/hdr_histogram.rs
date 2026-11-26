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

//! A histogram of recorded values.

use crate::metrics_kit::MetricsError;

/// The distribution of recorded values. Clones share the state.
///
/// Mirrors `org.apache.pekko.testkit.metrics.HdrHistogram`.
#[derive(Clone)]
pub struct HdrHistogram {
  _private: (),
}

impl HdrHistogram {
  /// Creates a histogram that tracks values up to `highest_trackable_value`.
  ///
  /// # Errors
  ///
  /// Returns [`MetricsError`] when the bounds or the digits are invalid.
  pub fn new(
    highest_trackable_value: i64,
    number_of_significant_value_digits: u8,
    unit: &str,
  ) -> Result<Self, MetricsError> {
    let _ = (highest_trackable_value, number_of_significant_value_digits, unit);
    todo!()
  }

  /// Records one value.
  ///
  /// # Errors
  ///
  /// Returns [`MetricsError`] when the value cannot be stored.
  pub fn update(&self, value: i64) -> Result<(), MetricsError> {
    let _ = value;
    todo!()
  }

  /// Records `value` `count` times.
  ///
  /// # Errors
  ///
  /// Returns [`MetricsError`] when the value cannot be stored or the count
  /// is invalid.
  pub fn update_with_count(&self, value: i64, count: i64) -> Result<(), MetricsError> {
    let _ = (value, count);
    todo!()
  }

  /// Returns the number of recorded values.
  pub fn count(&self) -> u64 {
    todo!()
  }

  /// Returns the unit shown when printing.
  pub fn unit(&self) -> &str {
    todo!()
  }

  /// Returns the lowest recorded value, `0` when empty.
  ///
  /// Mirrors `getData.getMinValue`.
  pub fn min(&self) -> u64 {
    todo!()
  }

  /// Returns the highest recorded value, `0` when empty.
  ///
  /// Mirrors `getData.getMaxValue`.
  pub fn max(&self) -> u64 {
    todo!()
  }
}
