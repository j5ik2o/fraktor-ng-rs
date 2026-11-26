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

// Regression tests for the review follow-up of the metrics kit: the total
// count overflow of a single record, the atomic reading of the averaging
// gauge, and the failure handling of report-and-clear.
//
// These are not ports of Pekko cases and are not counted in the ledger.

use core::time::Duration;

use std::io;
use std::io::Write;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;

use fraktor_testkit::metrics_kit::{
  AveragingGauge, ConsoleReporterSettings, HdrHistogram, MetricKey, MetricsError, MetricsKit, MetricsKitSettings,
  MetricsReporterKind,
};

fn ok<T>(result: Result<T, MetricsError>) -> T {
  result.unwrap_or_else(|error| panic!("unexpected metrics error: {error}"))
}

fn kit_key() -> MetricKey {
  MetricKey::from_string("metrics-kit")
}

fn settings(reporters: Vec<MetricsReporterKind>, interval: Duration) -> MetricsKitSettings {
  MetricsKitSettings {
    reporters,
    console: ConsoleReporterSettings { scheduled_report_interval: interval, verbose: false },
  }
}

/// An output whose writes always fail.
struct FailingOutput;

impl Write for FailingOutput {
  fn write(&mut self, _buf: &[u8]) -> io::Result<usize> {
    Err(io::Error::new(io::ErrorKind::BrokenPipe, "the output is broken"))
  }

  fn flush(&mut self) -> io::Result<()> {
    Err(io::Error::new(io::ErrorKind::BrokenPipe, "the output is broken"))
  }
}

/// An output a test reads back.
#[derive(Clone, Default)]
struct CapturedOutput {
  bytes: Arc<Mutex<Vec<u8>>>,
}

impl CapturedOutput {
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

#[test]
fn a_single_record_past_the_total_count_limit_fails_and_changes_nothing() {
  let histogram = ok(HdrHistogram::new(100_000_000, 4, "ns"));
  ok(histogram.update_with_count(0, i64::MAX));
  ok(histogram.update_with_count(0, i64::MAX));
  ok(histogram.update(0));
  assert_eq!(histogram.count(), u64::MAX, "the total count is at its limit");

  assert!(histogram.update(1).is_err(), "one more record does not fit");
  assert_eq!(histogram.count(), u64::MAX, "the failed record changed nothing");
}

#[test]
fn recording_at_and_above_the_trackable_value_keeps_the_failure_atomic() {
  let histogram = ok(HdrHistogram::new(100_000_000, 4, "ns"));
  ok(histogram.update_with_count(100_000_000, 3));
  // Above the trackable value but inside the highest bucket.
  ok(histogram.update_with_count(100_000_001, 2));
  assert_eq!(histogram.count(), 5);

  assert!(histogram.update_with_count(10_000_000_000, 4).is_err(), "outside every bucket");
  assert_eq!(histogram.count(), 5, "the failed bulk record changed nothing");
}

#[test]
fn the_averaging_gauge_reads_one_state_while_threads_add() {
  let gauge = AveragingGauge::new();
  let writers: Vec<_> = (0..4)
    .map(|_| {
      let gauge = gauge.clone();
      thread::spawn(move || {
        for _ in 0..1_000 {
          gauge.add(2);
        }
      })
    })
    .collect();
  // Every value ever added is 2, so any consistent reading is exactly 2,
  // and a reading that mixes the sum of one state with the count of
  // another is not.
  while writers.iter().any(|writer| !writer.is_finished()) {
    let value = gauge.value();
    assert!(value.is_nan() || value == 2.0, "a mixed reading gave {value}");
  }
  for writer in writers {
    writer.join().expect("the writer thread");
  }
  assert_eq!(gauge.value(), 2.0);
}

#[test]
fn report_and_clear_keeps_the_metrics_when_the_output_fails() {
  let kit =
    MetricsKit::with_output(settings(vec![MetricsReporterKind::Console], Duration::ZERO), Box::new(FailingOutput));
  ok(kit.average_gauge(&kit_key().child("avg-size"))).add(4);

  kit.report_and_clear_metrics();

  assert_eq!(kit.registered_metrics().len(), 1, "a failed report does not clear the metrics");
}

#[test]
fn report_and_clear_removes_the_metrics_when_the_output_succeeds() {
  let output = CapturedOutput::default();
  let kit =
    MetricsKit::with_output(settings(vec![MetricsReporterKind::Console], Duration::ZERO), Box::new(output.clone()));
  ok(kit.average_gauge(&kit_key().child("avg-size"))).add(4);

  kit.report_and_clear_metrics();

  assert!(kit.registered_metrics().is_empty(), "a successful report clears the metrics");
  assert!(output.len() > 0, "the report was printed");
}

#[test]
fn concurrent_shutdowns_both_wait_for_the_reporter_thread() {
  let output = CapturedOutput::default();
  let kit = Arc::new(MetricsKit::with_output(
    settings(vec![MetricsReporterKind::Console], Duration::from_millis(20)),
    Box::new(output.clone()),
  ));
  ok(kit.average_gauge(&kit_key().child("avg-size"))).add(4);

  let deadline = Instant::now() + Duration::from_secs(10);
  while output.len() == 0 {
    assert!(Instant::now() < deadline, "no scheduled report within the deadline");
    thread::sleep(Duration::from_millis(5));
  }

  let (sender, receiver) = mpsc::channel();
  let callers: Vec<_> = (0..2)
    .map(|_| {
      let kit = Arc::clone(&kit);
      let sender = sender.clone();
      thread::spawn(move || {
        kit.shutdown_metrics();
        sender.send(()).expect("the receiver is waiting");
      })
    })
    .collect();
  for _ in &callers {
    receiver.recv_timeout(Duration::from_secs(10)).expect("both shutdown calls return");
  }
  for caller in callers {
    caller.join().expect("the caller thread");
  }

  let printed = output.len();
  thread::sleep(Duration::from_millis(200));
  assert_eq!(output.len(), printed, "no scheduled report after both shutdowns returned");
}
