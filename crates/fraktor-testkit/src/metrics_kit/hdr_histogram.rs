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

use std::sync::{Arc, Mutex};

use crate::metrics_kit::MetricsError;

/// The lowest discernible value, the one the original builds with.
const LOWEST_DISCERNIBLE_VALUE: i64 = 1;

/// The distribution of recorded values. Clones share the state.
///
/// Mirrors `org.apache.pekko.testkit.metrics.HdrHistogram`.
#[derive(Clone)]
pub struct HdrHistogram {
  inner: Arc<Mutex<hdrhistogram::Histogram<u64>>>,
  unit: String,
  highest_trackable_value: i64,
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
    if number_of_significant_value_digits > 5 {
      return Err(MetricsError::InvalidArgument {
        reason: format!(
          "number_of_significant_value_digits must be between 0 and 5: {number_of_significant_value_digits}"
        ),
      });
    }
    if highest_trackable_value < 2 * LOWEST_DISCERNIBLE_VALUE {
      return Err(MetricsError::InvalidArgument {
        reason: format!(
          "highest_trackable_value must be at least twice the lowest discernible value: {highest_trackable_value}"
        ),
      });
    }
    let inner = hdrhistogram::Histogram::<u64>::new_with_bounds(
      LOWEST_DISCERNIBLE_VALUE as u64,
      highest_trackable_value as u64,
      number_of_significant_value_digits,
    )
    .map_err(|error| MetricsError::InvalidArgument { reason: format!("invalid histogram bounds: {error}") })?;
    Ok(Self { inner: Arc::new(Mutex::new(inner)), unit: unit.to_owned(), highest_trackable_value })
  }

  /// The error of a value this histogram cannot store.
  fn out_of_range(&self, value: i64) -> MetricsError {
    MetricsError::HistogramValueOutOfRange {
      value,
      lowest_discernible_value: LOWEST_DISCERNIBLE_VALUE,
      highest_trackable_value: self.highest_trackable_value,
    }
  }

  /// Records one value.
  ///
  /// # Errors
  ///
  /// Returns [`MetricsError`] when the value cannot be stored.
  pub fn update(&self, value: i64) -> Result<(), MetricsError> {
    if value < 0 {
      return Err(self.out_of_range(value));
    }
    let mut inner = self.inner.lock().expect("histogram lock");
    inner.record(value as u64).map_err(|_| self.out_of_range(value))
  }

  /// Records `value` `count` times.
  ///
  /// # Errors
  ///
  /// Returns [`MetricsError`] when the value cannot be stored or the count
  /// is invalid.
  pub fn update_with_count(&self, value: i64, count: i64) -> Result<(), MetricsError> {
    if count < 0 {
      return Err(MetricsError::InvalidArgument { reason: format!("count must not be negative: {count}") });
    }
    if count == 0 {
      return Ok(());
    }
    if value < 0 {
      return Err(self.out_of_range(value));
    }
    let mut inner = self.inner.lock().expect("histogram lock");
    if u128::from(inner.len()) + count as u128 > u128::from(u64::MAX) {
      return Err(MetricsError::InvalidArgument {
        reason: format!("recording {count} more values would overflow the total count"),
      });
    }
    // Record into a copy first, so a value that cannot be stored leaves
    // the histogram untouched.
    let mut trial = inner.clone();
    trial.record_n(value as u64, count as u64).map_err(|_| self.out_of_range(value))?;
    *inner = trial;
    Ok(())
  }

  /// Returns the number of recorded values.
  pub fn count(&self) -> u64 {
    self.inner.lock().expect("histogram lock").len()
  }

  /// Returns the unit shown when printing.
  pub fn unit(&self) -> &str {
    &self.unit
  }

  /// Returns the lowest recorded value, `0` when empty.
  ///
  /// Mirrors `getData.getMinValue`.
  pub fn min(&self) -> u64 {
    self.inner.lock().expect("histogram lock").min()
  }

  /// Returns the highest recorded value, `0` when empty.
  ///
  /// Mirrors `getData.getMaxValue`.
  pub fn max(&self) -> u64 {
    self.inner.lock().expect("histogram lock").max()
  }

  /// Returns the value at `percentile`, for the console report.
  pub(crate) fn value_at_percentile(&self, percentile: f64) -> u64 {
    self.inner.lock().expect("histogram lock").value_at_percentile(percentile)
  }
}
