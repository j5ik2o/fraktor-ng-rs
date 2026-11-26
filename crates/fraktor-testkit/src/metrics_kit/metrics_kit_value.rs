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

//! The registry of metrics and its reporters.

use std::collections::BTreeMap;
use std::io::Write;

use crate::metrics_kit::{AveragingGauge, HdrHistogram, Metric, MetricKey, MetricsError, MetricsKitSettings};

/// Registers metrics under keys and reports them.
///
/// Mirrors `org.apache.pekko.testkit.metrics.MetricsKit` and `MetricsKitOps`.
pub struct MetricsKit {
  _private: (),
}

impl MetricsKit {
  /// Creates a kit that reports to the standard output.
  pub fn new(settings: MetricsKitSettings) -> Self {
    let _ = settings;
    todo!()
  }

  /// Creates a kit that reports to `output`.
  pub fn with_output(settings: MetricsKitSettings, output: Box<dyn Write + Send>) -> Self {
    let _ = (settings, output);
    todo!()
  }

  /// Returns a snapshot of the registered metrics.
  ///
  /// Mirrors `registeredMetrics`.
  pub fn registered_metrics(&self) -> BTreeMap<String, Metric> {
    todo!()
  }

  /// Returns the averaging gauge under `key`, registering it if absent.
  ///
  /// # Errors
  ///
  /// Returns [`MetricsError::KindConflict`] when `key` holds another kind.
  pub fn average_gauge(&self, key: &MetricKey) -> Result<AveragingGauge, MetricsError> {
    let _ = key;
    todo!()
  }

  /// Runs `run` with a timer of `ops` operations under `key`, stopping the
  /// timer when `run` returns or panics.
  ///
  /// # Errors
  ///
  /// Returns [`MetricsError::KindConflict`] when `key` holds another kind.
  pub fn timed_with_known_ops<T>(&self, key: &MetricKey, ops: i64, run: impl FnOnce() -> T) -> Result<T, MetricsError> {
    let _ = (key, ops, run);
    todo!()
  }

  /// Returns the histogram under `key / "hdr-histogram"`, registering it if
  /// absent.
  ///
  /// # Errors
  ///
  /// Returns [`MetricsError`] when the key holds another kind, or when a new
  /// histogram cannot be built from the arguments.
  pub fn hdr_histogram(
    &self,
    key: &MetricKey,
    highest_trackable_value: i64,
    number_of_significant_value_digits: u8,
    unit: &str,
  ) -> Result<HdrHistogram, MetricsError> {
    let _ = (key, highest_trackable_value, number_of_significant_value_digits, unit);
    todo!()
  }

  /// Registers the three file descriptor gauges under
  /// `key / "file-descriptors"`, all or none.
  ///
  /// # Errors
  ///
  /// Returns [`MetricsError`] when one of the keys is registered already.
  pub fn measure_file_descriptors(&self, key: &MetricKey) -> Result<(), MetricsError> {
    let _ = key;
    todo!()
  }

  /// Prints every metric with every reporter.
  ///
  /// Mirrors `reportMetrics`.
  pub fn report_metrics(&self) {
    todo!()
  }

  /// Prints every metric, then removes them all.
  ///
  /// Mirrors `reportAndClearMetrics`.
  pub fn report_and_clear_metrics(&self) {
    todo!()
  }

  /// Removes every metric.
  ///
  /// Mirrors `clearMetrics()`.
  pub fn clear_metrics(&self) {
    todo!()
  }

  /// Stops the scheduled reports. Calls after the first do nothing.
  ///
  /// Mirrors `shutdownMetrics`.
  pub fn shutdown_metrics(&self) {
    todo!()
  }
}

impl Drop for MetricsKit {
  fn drop(&mut self) {}
}
