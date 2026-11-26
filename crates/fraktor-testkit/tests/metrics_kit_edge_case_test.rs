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

// Edge cases of the metrics kit (the design of #183,
// docs/design/issue-183-metrics-kit.md §10 step 3, with the behaviour §2 to §4
// fix: the reporter settings and lifecycle, the registry's clear and report).
//
// These are not ports of Pekko cases and are not counted in the ledger.

use core::time::Duration;

use std::io;
use std::io::Write;
use std::panic;
use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;

use fraktor_testkit::metrics_kit::{
  ConsoleReporterSettings, HdrHistogram, KnownOpsInTimespanTimer, Metric, MetricKey, MetricsError, MetricsKit,
  MetricsKitSettings, MetricsReporterKind,
};

/// An output a test reads back.
#[derive(Clone, Default)]
struct CapturedOutput {
  bytes: Arc<Mutex<Vec<u8>>>,
}

impl CapturedOutput {
  fn text(&self) -> String {
    String::from_utf8(self.bytes.lock().unwrap().clone()).expect("the report is UTF-8")
  }

  fn len(&self) -> usize {
    self.bytes.lock().unwrap().len()
  }
}

impl Write for CapturedOutput {
  fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
    self.bytes.lock().unwrap().extend_from_slice(buf);
    Ok(buf.len())
  }

  fn flush(&mut self) -> io::Result<()> {
    Ok(())
  }
}

fn ok<T>(result: Result<T, MetricsError>) -> T {
  result.unwrap_or_else(|error| panic!("unexpected metrics error: {error}"))
}

fn kit_key() -> MetricKey {
  MetricKey::from_string("metrics-kit")
}

fn settings(reporters: Vec<MetricsReporterKind>, interval: Duration, verbose: bool) -> MetricsKitSettings {
  MetricsKitSettings { reporters, console: ConsoleReporterSettings { scheduled_report_interval: interval, verbose } }
}

fn captured_kit(settings: MetricsKitSettings) -> (MetricsKit, CapturedOutput) {
  let output = CapturedOutput::default();
  (MetricsKit::with_output(settings, Box::new(output.clone())), output)
}

fn keys(kit: &MetricsKit) -> Vec<String> {
  kit.registered_metrics().into_keys().collect()
}

fn same_or_close(left: f64, right: f64) -> bool {
  if left.is_nan() || right.is_nan() {
    left.is_nan() && right.is_nan()
  } else if left.is_infinite() || right.is_infinite() {
    left == right
  } else {
    (left - right).abs() <= 1e-9 * left.abs().max(right.abs())
  }
}

const LONG_INTERVAL: Duration = Duration::from_secs(60);
const PROMPT: Duration = Duration::from_secs(10);

// --- the timer -----------------------------------------------------------------

#[test]
fn timed_with_known_ops_stops_the_timer_when_the_block_panics() {
  let kit = MetricsKit::new(MetricsKitSettings::default());

  let outcome =
    panic::catch_unwind(AssertUnwindSafe(|| kit.timed_with_known_ops(&kit_key(), 10, || -> i32 { panic!("boom") })));

  assert!(outcome.is_err(), "the panic propagates out of timed_with_known_ops");
  let metrics = kit.registered_metrics();
  let Some(Metric::KnownOpsInTimespanTimer(timer)) = metrics.get("metrics-kit") else {
    panic!("no KnownOpsInTimespanTimer is registered under metrics-kit");
  };
  assert!(!timer.stop(), "the timer was stopped while the panic unwound");
  assert!(timer.elapsed().is_some(), "a stopped timer has an elapsed time");
  assert_eq!(timer.count(), 10);
}

#[test]
fn an_unstopped_timer_has_no_elapsed_time() {
  let timer = KnownOpsInTimespanTimer::new(10);

  assert_eq!(timer.count(), 10);
  assert_eq!(timer.elapsed(), None);
  assert_eq!(timer.avg_duration_nanos(), None);
  assert_eq!(timer.ops_per_second(), None);
}

#[test]
fn a_stopped_timer_computes_as_the_original_for_every_operation_count() {
  for ops in [10, 1, 0, -3, i64::MAX, i64::MIN] {
    let timer = KnownOpsInTimespanTimer::new(ops);
    thread::sleep(Duration::from_millis(1));
    assert!(timer.stop(), "ops {ops}: the first stop");

    let elapsed = timer.elapsed().expect("a stopped timer has an elapsed time");
    let elapsed_nanos = elapsed.as_nanos() as f64;
    // avgDuration = (elapsedTime.toDouble / expectedOps).toLong
    let expected_avg = (elapsed_nanos / ops as f64) as i64;
    // opsPerSecond = expectedOps.toDouble / (elapsedTime.toDouble / 1 s in ns)
    let expected_ops_per_second = ops as f64 / (elapsed_nanos / 1_000_000_000.0);
    assert_eq!(timer.avg_duration_nanos(), Some(expected_avg), "ops {ops}: avg_duration_nanos");
    let ops_per_second = timer.ops_per_second().expect("a stopped timer has a rate");
    assert!(
      same_or_close(ops_per_second, expected_ops_per_second),
      "ops {ops}: ops_per_second {ops_per_second}, expected {expected_ops_per_second}"
    );
    assert_eq!(timer.count(), ops);
  }
}

// --- kinds and keys --------------------------------------------------------------

#[test]
fn a_key_holds_one_kind_and_a_conflict_leaves_the_registry_alone() {
  let kit = MetricsKit::new(MetricsKitSettings::default());
  let key = kit_key().child("conflict");
  let gauge = ok(kit.average_gauge(&key));
  gauge.add(3);
  let ran = AtomicBool::new(false);

  let result = kit.timed_with_known_ops(&key, 10, || ran.store(true, Ordering::SeqCst));

  let Err(error) = result else { panic!("a timer was registered over a gauge") };
  assert!(matches!(error, MetricsError::KindConflict { .. }), "unexpected error: {error}");
  assert!(error.to_string().contains("metrics-kit.conflict"), "the message names the key: {error}");
  assert!(!ran.load(Ordering::SeqCst), "the block does not run when the key holds another kind");
  assert_eq!(keys(&kit), ["metrics-kit.conflict"]);
  let metrics = kit.registered_metrics();
  let Some(Metric::AveragingGauge(registered)) = metrics.get("metrics-kit.conflict") else {
    panic!("the gauge is no longer registered");
  };
  registered.add(5);
  assert_eq!(gauge.value(), 4.0, "the registered gauge is the same gauge");
}

#[test]
fn a_histogram_registers_below_its_key_so_a_gauge_of_the_same_key_does_not_conflict() {
  let kit = MetricsKit::new(MetricsKitSettings::default());
  let key = kit_key().child("hist");

  ok(kit.average_gauge(&key));
  ok(kit.hdr_histogram(&key, 100_000_000, 4, "ns"));

  assert_eq!(keys(&kit), ["metrics-kit.hist", "metrics-kit.hist.hdr-histogram"]);
  let Err(error) = kit.average_gauge(&key.child("hdr-histogram")) else {
    panic!("a gauge was registered over a histogram");
  };
  assert!(matches!(error, MetricsError::KindConflict { .. }), "unexpected error: {error}");
}

#[test]
fn getting_a_histogram_again_returns_it_without_checking_the_arguments() {
  let kit = MetricsKit::new(MetricsKitSettings::default());
  let key = kit_key().child("hist");
  let first = ok(kit.hdr_histogram(&key, 100_000_000, 4, "ns"));
  ok(first.update(7));

  let again = ok(kit.hdr_histogram(&key, 0, 99, "ms"));

  assert_eq!(again.unit(), "ns", "the registered histogram keeps its unit");
  assert_eq!(again.count(), 1, "the registered histogram keeps its values");
  ok(again.update(8));
  assert_eq!(first.count(), 2, "both handles share the histogram");
}

// --- histogram bounds ------------------------------------------------------------

#[test]
fn a_histogram_records_up_to_the_top_of_its_last_bucket() {
  let histogram = ok(HdrHistogram::new(100_000_000, 4, "ns"));

  for value in [0, 100_000_000, 100_000_001, 134_217_727] {
    ok(histogram.update(value));
  }
  for value in [134_217_728, 10_000_000_000, -1, i64::MIN] {
    let Err(error) = histogram.update(value) else { panic!("{value} was recorded") };
    assert!(error.to_string().contains("can not be stored in this histogram"), "{value}: {error}");
  }

  assert_eq!(histogram.count(), 4, "failed updates do not count");
  assert_eq!(histogram.min(), 0);
  assert_eq!(histogram.max(), 134_217_727);
  assert_eq!(histogram.unit(), "ns");
}

#[test]
fn the_overflow_message_names_the_value_and_the_bounds() {
  let histogram = ok(HdrHistogram::new(100_000_000, 4, "ns"));

  let Err(error) = histogram.update(10_000_000_000) else { panic!("10 s in nanoseconds was recorded") };

  assert_eq!(error.to_string(), "Given value 10000000000 can not be stored in this histogram (min: 1, max: 100000000)");
}

#[test]
fn an_empty_histogram_has_zero_min_and_max() {
  let histogram = ok(HdrHistogram::new(100_000_000, 4, ""));

  assert_eq!(histogram.count(), 0);
  assert_eq!(histogram.min(), 0);
  assert_eq!(histogram.max(), 0);
  assert_eq!(histogram.unit(), "");
}

#[test]
fn update_with_count_rejects_negative_counts_and_records_nothing_for_zero() {
  let histogram = ok(HdrHistogram::new(100_000_000, 4, "ns"));

  assert!(histogram.update_with_count(10, -1).is_err(), "a negative count is an error");
  assert_eq!(histogram.count(), 0);
  ok(histogram.update_with_count(50, 0));
  assert_eq!(histogram.count(), 0, "a zero count records nothing");
  assert_eq!(histogram.max(), 0, "a zero count does not move the maximum");

  ok(histogram.update_with_count(20, 5));
  assert_eq!(histogram.count(), 5);
  assert_eq!(histogram.max(), 20);
  let Err(error) = histogram.update_with_count(134_217_728, 3) else { panic!("an out-of-range value was recorded") };
  assert!(error.to_string().contains("can not be stored in this histogram"), "{error}");
  assert_eq!(histogram.count(), 5, "an out-of-range value records nothing");
}

#[test]
fn update_with_count_records_nothing_when_the_total_count_would_overflow() {
  let histogram = ok(HdrHistogram::new(100_000_000, 4, "ns"));
  ok(histogram.update_with_count(1, i64::MAX));
  ok(histogram.update_with_count(1, i64::MAX));
  let total = 2 * (i64::MAX as u64);
  assert_eq!(histogram.count(), total);

  assert!(histogram.update_with_count(1, i64::MAX).is_err(), "the total count would overflow");
  assert_eq!(histogram.count(), total, "nothing is recorded");
}

#[test]
fn a_histogram_rejects_bounds_and_digits_the_original_rejects() {
  for (highest, digits) in [(1, 4), (0, 4), (-5, 4), (i64::MIN, 4), (100_000_000, 6), (100_000_000, u8::MAX)] {
    assert!(HdrHistogram::new(highest, digits, "ns").is_err(), "highest {highest}, digits {digits}");
  }
  for (highest, digits) in [(2, 0), (2, 5), (100_000_000, 0), (100_000_000, 5), (i64::MAX, 4)] {
    ok(HdrHistogram::new(highest, digits, "ns"));
  }
}

// --- file descriptors ------------------------------------------------------------

#[test]
fn measuring_file_descriptors_registers_all_or_nothing() {
  let kit = MetricsKit::new(MetricsKitSettings::default());
  let key = kit_key().child("file-desc");
  ok(kit.average_gauge(&key.child("file-descriptors").child("file-descriptors.max")));

  assert!(kit.measure_file_descriptors(&key).is_err(), "one of the three keys is taken");

  assert_eq!(keys(&kit), ["metrics-kit.file-desc.file-descriptors.file-descriptors-max"]);
  let metrics = kit.registered_metrics();
  assert!(
    matches!(
      metrics.get("metrics-kit.file-desc.file-descriptors.file-descriptors-max"),
      Some(Metric::AveragingGauge(_))
    ),
    "the gauge stays registered"
  );
}

#[test]
fn measuring_file_descriptors_twice_under_one_key_fails_and_keeps_the_first_registration() {
  let kit = MetricsKit::new(MetricsKitSettings::default());
  let key = kit_key().child("file-desc");
  ok(kit.measure_file_descriptors(&key));
  let first = keys(&kit);

  assert!(kit.measure_file_descriptors(&key).is_err(), "registerAll does not reuse existing gauges");

  assert_eq!(keys(&kit), first);
  ok(kit.measure_file_descriptors(&kit_key().child("other")));
  assert_eq!(kit.registered_metrics().len(), 6, "another key registers three more gauges");
}

// --- clear and report ------------------------------------------------------------

#[test]
fn clear_empties_the_registry_and_report_leaves_it_alone() {
  let (kit, _output) = captured_kit(MetricsKitSettings::default());
  ok(kit.average_gauge(&kit_key().child("avg-size")));
  ok(kit.hdr_histogram(&kit_key().child("hist"), 100_000_000, 4, "ns"));
  ok(kit.timed_with_known_ops(&kit_key().child("timer"), 10, || ()));
  let registered = keys(&kit);

  kit.report_metrics();
  assert_eq!(keys(&kit), registered, "report_metrics keeps every metric");

  kit.report_and_clear_metrics();
  assert!(kit.registered_metrics().is_empty(), "report_and_clear_metrics removes every metric");

  let gauge = ok(kit.average_gauge(&kit_key().child("avg-size")));
  assert!(gauge.value().is_nan(), "a key can be registered afresh after a clear");
  kit.clear_metrics();
  assert!(kit.registered_metrics().is_empty(), "clear_metrics removes every metric");
}

// --- settings and the reporter ---------------------------------------------------

#[test]
fn the_default_settings_are_the_test_reference_conf() {
  let settings = MetricsKitSettings::default();

  assert!(matches!(settings.reporters.as_slice(), [MetricsReporterKind::Console]), "reporters = [console]");
  assert_eq!(settings.console.scheduled_report_interval, Duration::ZERO, "scheduled-report-interval = 0 ms");
  assert!(!settings.console.verbose, "verbose = false");
}

#[test]
fn report_metrics_prints_every_registered_key() {
  let (kit, output) = captured_kit(MetricsKitSettings::default());
  ok(kit.average_gauge(&kit_key().child("avg-size"))).add(4);
  ok(ok(kit.hdr_histogram(&kit_key().child("hist"), 100_000_000, 4, "ns")).update(5));
  ok(kit.timed_with_known_ops(&kit_key().child("timer"), 10, || ()));

  kit.report_metrics();

  let text = output.text();
  for key in ["metrics-kit.avg-size", "metrics-kit.hist.hdr-histogram", "metrics-kit.timer"] {
    assert!(text.contains(key), "the report lacks {key}:\n{text}");
  }
}

#[test]
fn without_reporters_report_metrics_prints_nothing() {
  let (kit, output) = captured_kit(settings(Vec::new(), Duration::ZERO, false));
  ok(kit.average_gauge(&kit_key().child("avg-size"))).add(4);

  kit.report_metrics();
  kit.report_and_clear_metrics();

  assert_eq!(output.len(), 0, "no reporter, no output");
}

#[test]
fn a_repeated_console_entry_starts_one_reporter() {
  let report = |reporters: Vec<MetricsReporterKind>| {
    let (kit, output) = captured_kit(settings(reporters, Duration::ZERO, false));
    ok(kit.average_gauge(&kit_key().child("avg-size"))).add(4);
    kit.report_metrics();
    output.text().matches("metrics-kit.avg-size").count()
  };

  let once = report(vec![MetricsReporterKind::Console]);
  let twice = report(vec![MetricsReporterKind::Console, MetricsReporterKind::Console]);

  assert!(once > 0, "the console reporter prints the key");
  assert_eq!(twice, once, "a repeated console entry prints as one reporter");
}

#[test]
fn verbose_reports_add_the_percentile_distribution() {
  let report = |verbose: bool| {
    let (kit, output) = captured_kit(settings(vec![MetricsReporterKind::Console], Duration::ZERO, verbose));
    let histogram = ok(kit.hdr_histogram(&kit_key().child("hist"), 100_000_000, 4, "ns"));
    for value in 0..1_000 {
      ok(histogram.update(value * 1_000));
    }
    kit.report_metrics();
    output.text()
  };

  let plain = report(false);
  let verbose = report(true);

  assert!(verbose.len() > plain.len(), "the verbose report adds lines:\n{plain}\n---\n{verbose}");
}

// --- the scheduled reports -------------------------------------------------------

#[test]
fn scheduled_reports_print_until_shutdown() {
  let (kit, output) = captured_kit(settings(vec![MetricsReporterKind::Console], Duration::from_millis(20), false));
  ok(kit.average_gauge(&kit_key().child("avg-size"))).add(4);

  let deadline = Instant::now() + PROMPT;
  while !output.text().contains("metrics-kit.avg-size") {
    assert!(Instant::now() < deadline, "no scheduled report within {PROMPT:?}");
    thread::sleep(Duration::from_millis(10));
  }
  kit.shutdown_metrics();
  let printed = output.len();
  thread::sleep(Duration::from_millis(200));

  assert_eq!(output.len(), printed, "no scheduled report after shutdown_metrics");
}

#[test]
fn shutdown_ends_a_long_wait_at_once_and_is_idempotent() {
  let (kit, output) = captured_kit(settings(vec![MetricsReporterKind::Console], LONG_INTERVAL, false));
  ok(kit.average_gauge(&kit_key().child("avg-size"))).add(4);

  let started = Instant::now();
  kit.shutdown_metrics();
  assert!(started.elapsed() < PROMPT, "shutdown_metrics waited {:?}", started.elapsed());

  let again = Instant::now();
  kit.shutdown_metrics();
  assert!(again.elapsed() < PROMPT, "a second shutdown_metrics waited {:?}", again.elapsed());
  drop(kit);
  assert_eq!(output.len(), 0, "the long interval never passed, so nothing was printed");
}

#[test]
fn dropping_the_kit_without_shutdown_stops_the_scheduled_reports() {
  let (kit, output) = captured_kit(settings(vec![MetricsReporterKind::Console], Duration::from_millis(20), false));
  ok(kit.average_gauge(&kit_key().child("avg-size"))).add(4);
  let deadline = Instant::now() + PROMPT;
  while output.len() == 0 {
    assert!(Instant::now() < deadline, "no scheduled report within {PROMPT:?}");
    thread::sleep(Duration::from_millis(10));
  }

  let started = Instant::now();
  drop(kit);
  assert!(started.elapsed() < PROMPT, "drop waited {:?}", started.elapsed());
  let printed = output.len();
  thread::sleep(Duration::from_millis(200));

  assert_eq!(output.len(), printed, "no scheduled report after the drop");
}

#[test]
fn dropping_the_kit_ends_a_long_wait_at_once() {
  let (kit, _output) = captured_kit(settings(vec![MetricsReporterKind::Console], LONG_INTERVAL, false));

  let started = Instant::now();
  drop(kit);

  assert!(started.elapsed() < PROMPT, "drop waited {:?}", started.elapsed());
}
