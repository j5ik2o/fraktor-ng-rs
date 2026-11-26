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

// Ported from Pekko v2.0.0 (SHA 9fbd2283b1cf701c90a1223bf621942c191db61f):
//   testkit/src/test/scala/org/apache/pekko/testkit/metrics/MetricsKitSpec.scala
//
// Cases (original line numbers):
//   - "allow measuring file descriptor usage" (L40-44): ported.
//   - "allow to measure time, on known number of operations" (L46-50): ported.
//   - "allow to measure average value using Gauge, given multiple values"
//     (L52-61): ported.
//   - "measure values in histogram" (L63-74): ported.
//   - "fail with human readable error when the histogram overflowed"
//     (L76-85): ported.
//
// Original test names are kept as comments next to each ported case. The
// original's checks that have no assertion (the block's value, the timer's
// count, the number of recorded values) are written out as assertions; the
// design (docs/design/issue-183-metrics-kit.md §1) lists them.

use core::time::Duration;

use std::thread;

use fraktor_testkit::metrics_kit::{Metric, MetricKey, MetricsError, MetricsKit, MetricsKitSettings};

/// The spec's fixture. The original mixes `MetricsKit` in with
/// `metricsConfig = ConfigFactory.load()`, uses `KitKey` as the root of every
/// key, calls `clearMetrics()` after each case and `shutdownMetrics()` after
/// all of them. Each test owns its kit here, so the drop does both.
struct MetricsKitSpec {
  kit: MetricsKit,
  kit_key: MetricKey,
}

impl MetricsKitSpec {
  fn new() -> Self {
    Self { kit: MetricsKit::new(MetricsKitSettings::default()), kit_key: MetricKey::from_string("metrics-kit") }
  }
}

impl Drop for MetricsKitSpec {
  fn drop(&mut self) {
    if thread::panicking() {
      return;
    }
    self.kit.clear_metrics();
    assert!(self.kit.registered_metrics().is_empty(), "clear_metrics leaves the registry empty");
    self.kit.shutdown_metrics();
  }
}

fn ok<T>(result: Result<T, MetricsError>) -> T {
  result.unwrap_or_else(|error| panic!("unexpected metrics error: {error}"))
}

fn nanos(duration: Duration) -> i64 {
  i64::try_from(duration.as_nanos()).expect("the duration fits in i64 nanoseconds")
}

// "allow measuring file descriptor usage"
#[test]
fn metrics_kit_allows_measuring_file_descriptor_usage() {
  let spec = MetricsKitSpec::new();

  ok(spec.kit.measure_file_descriptors(&spec.kit_key.child("file-desc")));

  let matching = spec.kit.registered_metrics().keys().filter(|key| key.contains("file-descriptor")).count();
  assert!(matching > 0, "no registered key contains \"file-descriptor\"");
}

// "allow to measure time, on known number of operations"
#[test]
fn metrics_kit_allows_measuring_time_on_known_number_of_operations() {
  let spec = MetricsKitSpec::new();

  let value = ok(spec.kit.timed_with_known_ops(&spec.kit_key, 10, || 2 + 2));

  assert_eq!(value, 4, "the block's value is returned");
  let metrics = spec.kit.registered_metrics();
  let Some(Metric::KnownOpsInTimespanTimer(timer)) = metrics.get("metrics-kit") else {
    panic!("no KnownOpsInTimespanTimer is registered under metrics-kit");
  };
  assert_eq!(timer.count(), 10, "getCount is the expected number of operations");
  // The original stops the timer in `finally`.
  assert!(!timer.stop(), "the timer is stopped once the block returns");
  assert!(timer.elapsed().is_some(), "a stopped timer has an elapsed time");
}

// "allow to measure average value using Gauge, given multiple values"
#[test]
fn metrics_kit_allows_measuring_average_value_using_gauge_given_multiple_values() {
  let spec = MetricsKitSpec::new();
  let sizes = [1_i64, 2, 3];

  let avg = ok(spec.kit.average_gauge(&spec.kit_key.child("avg-size")));

  avg.add_slice(&sizes);
  avg.add(4);

  assert_eq!(avg.value(), 2.5);
}

// "measure values in histogram"
#[test]
fn metrics_kit_measures_values_in_histogram() {
  let spec = MetricsKitSpec::new();
  let max_millis = nanos(Duration::from_millis(100));
  let hist = ok(spec.kit.hdr_histogram(&spec.kit_key.child("hist"), max_millis, 4, "ns"));

  for _ in 1..=11 {
    for i in 0_i64..=1_579_331 {
      ok(hist.update(i));
    }
  }

  ok(hist.update(1_579_331));
  spec.kit.report_metrics();

  assert_eq!(hist.count(), 11 * 1_579_332 + 1, "every update is recorded");
}

// "fail with human readable error when the histogram overflowed"
#[test]
fn metrics_kit_fails_with_human_readable_error_when_histogram_overflowed() {
  let spec = MetricsKitSpec::new();
  let max_millis = nanos(Duration::from_millis(100));
  let hist = ok(spec.kit.hdr_histogram(&spec.kit_key.child("hist"), max_millis, 4, "ns"));

  let Err(error) = hist.update(nanos(Duration::from_secs(10))) else {
    panic!("recording 10 seconds in nanoseconds succeeded");
  };

  let message = error.to_string();
  assert!(message.contains("can not be stored in this histogram"), "unexpected message: {message}");
}
