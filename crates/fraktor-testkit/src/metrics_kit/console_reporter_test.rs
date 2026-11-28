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

//! Unit tests for the writing step of the console reporter: readings made
//! by hand, rendered with a given line separator.
//!
//! The expected text is taken from `docs/design/issue-299/console-reporter-examples.expected`,
//! the output of Pekko's `PekkoConsoleReporter` run on the original
//! libraries, with three changes (design §15): the entries of a kind are in
//! key order, the timer banner names `KnownOpsInTimespanTimer`, and lines
//! end with the separator given to `render`. The `R` numbers are the items
//! of design §15.
//!
//! These are not ports of Pekko cases and are not counted in the ledger.

use std::io;

use super::{render, FileDescriptorReading, MetricReading};

const AVERAGING_GAUGE_BANNER: &str = "-- AveragingGauge --------------------------------------------------------------";
const FILE_DESCRIPTOR_OPEN_BANNER: &str =
  "-- FileDescriptorMetricSet$$anon$1 ---------------------------------------------";
const FILE_DESCRIPTOR_MAX_BANNER: &str =
  "-- FileDescriptorMetricSet$$anon$2 ---------------------------------------------";
const FILE_DESCRIPTOR_RATIO_BANNER: &str =
  "-- FileDescriptorRatioGauge ----------------------------------------------------";
const TIMER_BANNER: &str = "-- KnownOpsInTimespanTimer -----------------------------------------------------";
const HISTOGRAM_BANNER: &str = "-- HdrHistogram ----------------------------------------------------------------";

const MAX_KEY: &str = "metrics-kit.file-desc.file-descriptors.file-descriptors-max";
const OPEN_KEY: &str = "metrics-kit.file-desc.file-descriptors.file-descriptors-open";
const RATIO_KEY: &str = "metrics-kit.file-desc.file-descriptors.file-descriptors-ratio";
const HISTOGRAM_KEY: &str = "metrics-kit.hist.hdr-histogram";

/// Each line followed by `separator`.
fn text(lines: &[&str], separator: &str) -> String {
  lines.iter().map(|line| format!("{line}{separator}")).collect()
}

/// Renders `readings` into a buffer and returns the text and the result.
fn rendered(readings: Vec<(&str, MetricReading)>, verbose: bool, separator: &str) -> (String, io::Result<()>) {
  let readings: Vec<(String, MetricReading)> =
    readings.into_iter().map(|(key, reading)| (key.to_owned(), reading)).collect();
  let mut output = Vec::new();
  let result = render(&readings, verbose, separator, &mut output);
  (String::from_utf8(output).expect("the report is UTF-8"), result)
}

/// A histogram built as `HdrHistogram::new(100_000_000, 4, ..)` builds
/// its own, holding `values`.
fn snapshot_of(values: &[u64]) -> hdrhistogram::Histogram<u64> {
  let mut histogram =
    hdrhistogram::Histogram::<u64>::new_with_bounds(1, 100_000_000, 4).expect("valid histogram bounds");
  for value in values {
    histogram.record(*value).expect("a trackable value");
  }
  histogram
}

fn averaging_gauge(value: f64) -> MetricReading {
  MetricReading::AveragingGauge { value }
}

fn timer(ops: i64, elapsed_nanos: i64) -> MetricReading {
  MetricReading::KnownOpsInTimespanTimer { ops, elapsed_nanos }
}

fn histogram(values: &[u64]) -> MetricReading {
  MetricReading::HdrHistogram { snapshot: snapshot_of(values), unit: "ns".to_owned() }
}

fn file_descriptor(reading: FileDescriptorReading) -> MetricReading {
  MetricReading::FileDescriptor(reading)
}

// R2
#[test]
fn r2_each_reading_names_its_kind_in_an_80_character_banner() {
  let cases = [
    (averaging_gauge(2.5), AVERAGING_GAUGE_BANNER),
    (file_descriptor(FileDescriptorReading::Open(Some(42))), FILE_DESCRIPTOR_OPEN_BANNER),
    (file_descriptor(FileDescriptorReading::Max(Some(1024))), FILE_DESCRIPTOR_MAX_BANNER),
    (file_descriptor(FileDescriptorReading::Ratio(0.5)), FILE_DESCRIPTOR_RATIO_BANNER),
    (timer(10, 1_234_567), TIMER_BANNER),
    (histogram(&[5]), HISTOGRAM_BANNER),
  ];

  for (reading, banner) in cases {
    assert_eq!(banner.chars().count(), 80, "{banner:?}");
    let (output, result) = rendered(vec![("k", reading)], false, "\n");
    assert!(result.is_ok(), "{banner:?}: {result:?}");
    assert_eq!(output.split('\n').next(), Some(banner));
  }
}

// R3
#[test]
fn r3_an_averaging_gauge_and_file_descriptors_share_the_gauge_section() {
  let (output, result) = rendered(
    vec![
      ("metrics-kit.avg-size", averaging_gauge(2.5)),
      (MAX_KEY, file_descriptor(FileDescriptorReading::Max(Some(1024)))),
      (OPEN_KEY, file_descriptor(FileDescriptorReading::Open(Some(42)))),
      (RATIO_KEY, file_descriptor(FileDescriptorReading::Ratio(0.041015625))),
    ],
    false,
    "\n",
  );

  assert!(result.is_ok(), "{result:?}");
  assert_eq!(
    output,
    text(
      &[
        AVERAGING_GAUGE_BANNER,
        "  metrics-kit.avg-size",
        "             value = 2.5",
        "  metrics-kit.file-desc.file-descriptors.file-descriptors-max",
        "             value = 1024",
        "  metrics-kit.file-desc.file-descriptors.file-descriptors-open",
        "             value = 42",
        "  metrics-kit.file-desc.file-descriptors.file-descriptors-ratio",
        "             value = 0.041015625",
        "",
        AVERAGING_GAUGE_BANNER,
        "  metrics-kit.avg-size",
        "                avg = 2.50",
        "",
        "",
      ],
      "\n"
    )
  );
}

// R4
#[test]
fn r4_file_descriptors_alone_are_headed_by_the_limit_gauge() {
  let (output, result) = rendered(
    vec![
      (MAX_KEY, file_descriptor(FileDescriptorReading::Max(Some(1024)))),
      (OPEN_KEY, file_descriptor(FileDescriptorReading::Open(Some(42)))),
      (RATIO_KEY, file_descriptor(FileDescriptorReading::Ratio(0.041015625))),
    ],
    false,
    "\n",
  );

  assert!(result.is_ok(), "{result:?}");
  assert_eq!(
    output,
    text(
      &[
        FILE_DESCRIPTOR_MAX_BANNER,
        "  metrics-kit.file-desc.file-descriptors.file-descriptors-max",
        "             value = 1024",
        "  metrics-kit.file-desc.file-descriptors.file-descriptors-open",
        "             value = 42",
        "  metrics-kit.file-desc.file-descriptors.file-descriptors-ratio",
        "             value = 0.041015625",
        "",
        "",
      ],
      "\n"
    )
  );
}

// R5
#[test]
fn r5_a_limit_of_zero_prints_zero_and_a_nan_ratio() {
  let (output, result) = rendered(
    vec![
      (MAX_KEY, file_descriptor(FileDescriptorReading::Max(Some(0)))),
      (OPEN_KEY, file_descriptor(FileDescriptorReading::Open(Some(42)))),
      (RATIO_KEY, file_descriptor(FileDescriptorReading::Ratio(f64::NAN))),
    ],
    false,
    "\n",
  );

  assert!(result.is_ok(), "{result:?}");
  assert_eq!(
    output,
    text(
      &[
        FILE_DESCRIPTOR_MAX_BANNER,
        "  metrics-kit.file-desc.file-descriptors.file-descriptors-max",
        "             value = 0",
        "  metrics-kit.file-desc.file-descriptors.file-descriptors-open",
        "             value = 42",
        "  metrics-kit.file-desc.file-descriptors.file-descriptors-ratio",
        "             value = NaN",
        "",
        "",
      ],
      "\n"
    )
  );
}

// R6
#[test]
fn r6_file_descriptors_without_readings_stop_the_report_at_the_limit() {
  let (output, result) = rendered(
    vec![
      (MAX_KEY, file_descriptor(FileDescriptorReading::Max(None))),
      (OPEN_KEY, file_descriptor(FileDescriptorReading::Open(None))),
      (RATIO_KEY, file_descriptor(FileDescriptorReading::Ratio(f64::NAN))),
    ],
    false,
    "\n",
  );

  assert_eq!(
    output,
    text(&[FILE_DESCRIPTOR_MAX_BANNER, "  metrics-kit.file-desc.file-descriptors.file-descriptors-max"], "\n")
  );
  assert_eq!(result.map_err(|error| error.kind()), Err(io::ErrorKind::Unsupported));
}

// R7
#[test]
fn r7_the_ratio_alone_prints_nan_without_readings() {
  let (output, result) =
    rendered(vec![(RATIO_KEY, file_descriptor(FileDescriptorReading::Ratio(f64::NAN)))], false, "\n");

  assert!(result.is_ok(), "{result:?}");
  assert_eq!(
    output,
    text(
      &[
        FILE_DESCRIPTOR_RATIO_BANNER,
        "  metrics-kit.file-desc.file-descriptors.file-descriptors-ratio",
        "             value = NaN",
        "",
        "",
      ],
      "\n"
    )
  );
}

// R10
#[test]
fn r10_timers_print_time_ops_per_second_and_average() {
  let (output, result) = rendered(
    vec![
      ("metrics-kit.t1-ms", timer(10, 1_234_567)),
      ("metrics-kit.t2-zero", timer(10, 0)),
      ("metrics-kit.t3-unstopped", timer(10, -123_456_789_012_345)),
      ("metrics-kit.t4-no-ops", timer(0, 1_000)),
      ("metrics-kit.t5-no-ops-zero", timer(0, 0)),
    ],
    false,
    "\n",
  );

  assert!(result.is_ok(), "{result:?}");
  assert_eq!(
    output,
    text(
      &[
        TIMER_BANNER,
        "  metrics-kit.t1-ms",
        "               ops = 10",
        "              time = 1.235 ms",
        "             ops/s = 8100.01",
        "               avg = 123.5 \u{3BC}s",
        "  metrics-kit.t2-zero",
        "               ops = 10",
        "              time = 0.000 ns",
        "             ops/s = Infinity",
        "               avg = 0.000 ns",
        "  metrics-kit.t3-unstopped",
        "               ops = 10",
        "              time = -1.235e+14 ns",
        "             ops/s = -0.00",
        "               avg = -1.235e+13 ns",
        "  metrics-kit.t4-no-ops",
        "               ops = 0",
        "              time = 1.000 \u{3BC}s",
        "             ops/s = 0.00",
        "               avg = 1.068e+05 d",
        "  metrics-kit.t5-no-ops-zero",
        "               ops = 0",
        "              time = 0.000 ns",
        "             ops/s = NaN",
        "               avg = 0.000 ns",
        "",
        "",
      ],
      "\n"
    )
  );
}

// R11
#[test]
fn r11_an_average_of_long_min_value_stops_the_report() {
  let (output, result) =
    rendered(vec![("metrics-kit.t6-unstopped-no-ops", timer(0, -123_456_789_012_345))], false, "\n");

  assert_eq!(
    output,
    text(
      &[
        TIMER_BANNER,
        "  metrics-kit.t6-unstopped-no-ops",
        "               ops = 0",
        "              time = -1.235e+14 ns",
        "             ops/s = -0.00",
      ],
      "\n"
    )
  );
  assert_eq!(result.map_err(|error| error.kind()), Err(io::ErrorKind::InvalidInput));
}

// R20
#[test]
fn r20_kinds_are_printed_as_gauges_timers_histograms_then_averaging_gauges() {
  let (output, result) = rendered(
    vec![
      ("metrics-kit.avg-size", averaging_gauge(2.5)),
      (HISTOGRAM_KEY, histogram(&[5])),
      ("metrics-kit.timer", timer(10, 1_234_567)),
    ],
    false,
    "\n",
  );

  assert!(result.is_ok(), "{result:?}");
  assert_eq!(
    output,
    text(
      &[
        AVERAGING_GAUGE_BANNER,
        "  metrics-kit.avg-size",
        "             value = 2.5",
        "",
        TIMER_BANNER,
        "  metrics-kit.timer",
        "               ops = 10",
        "              time = 1.235 ms",
        "             ops/s = 8100.01",
        "               avg = 123.5 \u{3BC}s",
        "",
        HISTOGRAM_BANNER,
        "  metrics-kit.hist.hdr-histogram",
        "               min = 5 ns",
        "               max = 5 ns",
        "              mean = 5.00 ns",
        "            stddev = 0.00",
        "              75% <= 5 ns",
        "              95% <= 5 ns",
        "              98% <= 5 ns",
        "              99% <= 5 ns",
        "            99.9% <= 5 ns",
        "",
        AVERAGING_GAUGE_BANNER,
        "  metrics-kit.avg-size",
        "                avg = 2.50",
        "",
        "",
      ],
      "\n"
    )
  );
}

// R21
#[test]
fn r21_a_crlf_separator_ends_every_line_but_the_distribution() {
  let (output, result) = rendered(vec![(HISTOGRAM_KEY, histogram(&[0, 1, 2, 3, 4]))], true, "\r\n");

  assert!(result.is_ok(), "{result:?}");
  let expected = text(
    &[
      HISTOGRAM_BANNER,
      "  metrics-kit.hist.hdr-histogram",
      "               min = 0 ns",
      "               max = 4 ns",
      "              mean = 2.00 ns",
      "            stddev = 1.41",
      "              75% <= 3 ns",
      "              95% <= 4 ns",
      "              98% <= 4 ns",
      "              99% <= 4 ns",
      "            99.9% <= 4 ns",
    ],
    "\r\n",
  ) + &text(
    &[
      "       Value     Percentile TotalCount 1/(1-Percentile)",
      "",
      "      0.0000 0.000000000000          1           1.00",
      "      0.0000 0.100000000000          1           1.11",
      "      0.0000 0.200000000000          1           1.25",
      "      1.0000 0.300000000000          2           1.43",
      "      1.0000 0.400000000000          2           1.67",
      "      2.0000 0.500000000000          3           2.00",
      "      2.0000 0.550000000000          3           2.22",
      "      2.0000 0.600000000000          3           2.50",
      "      3.0000 0.650000000000          4           2.86",
      "      3.0000 0.700000000000          4           3.33",
      "      3.0000 0.750000000000          4           4.00",
      "      3.0000 0.775000000000          4           4.44",
      "      3.0000 0.800000000000          4           5.00",
      "      4.0000 0.825000000000          5           5.71",
      "      4.0000 1.000000000000          5",
      "#[Mean    =       2.0000, StdDeviation   =       1.4142]",
      "#[Max     =       4.0000, Total count    =            5]",
      "#[Buckets =           13, SubBuckets     =        32768]",
    ],
    "\n",
  ) + &text(&["", ""], "\r\n");
  assert_eq!(output, expected);
}
