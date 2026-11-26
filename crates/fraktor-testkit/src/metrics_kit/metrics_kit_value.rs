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

use core::time::Duration;
use std::collections::BTreeMap;
use std::io::Write;
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;

use crate::metrics_kit::{
  AveragingGauge, ConsoleReporter, FileDescriptorMetricSet, HdrHistogram, Metric, MetricKey, MetricsError,
  MetricsKitSettings, MetricsReporterKind,
};

/// Where the scheduled report thread is in its life.
enum ReporterThreadState {
  /// The thread is running; the handle is taken out only to join it.
  Running(Option<JoinHandle<()>>),
  /// One caller is joining the thread; other callers wait for `Finished`.
  Joining,
  /// There is no thread, or the thread has been joined.
  Finished,
}

/// The state a [`MetricsKit`] and its scheduled report thread share.
struct MetricsKitShared {
  registry: Mutex<BTreeMap<String, Metric>>,
  output: Mutex<Box<dyn Write + Send>>,
  console_reporter: Option<ConsoleReporter>,
  scheduled_report_interval: Duration,
  stop_requested: Mutex<bool>,
  stop_condvar: Condvar,
  reporter_thread: Mutex<ReporterThreadState>,
  reporter_thread_changed: Condvar,
}

impl MetricsKitShared {
  /// Prints the registered metrics once, if a reporter is running.
  /// Returns whether the output succeeded; without a reporter, printing
  /// nothing counts as success.
  fn report_once(&self) -> bool {
    let snapshot = self.registry.lock().expect("registry lock").clone();
    self.report_snapshot(&snapshot)
  }

  /// Prints `snapshot` once, if a reporter is running. Returns whether
  /// the output succeeded; without a reporter, printing nothing counts
  /// as success.
  fn report_snapshot(&self, snapshot: &BTreeMap<String, Metric>) -> bool {
    let Some(reporter) = &self.console_reporter else {
      return true;
    };
    let mut output = self.output.lock().expect("output lock");
    reporter.report(snapshot, &mut *output).is_ok()
  }

  /// Takes the registered metrics out of the registry and returns them.
  /// Metrics registered after the take stay registered.
  fn take_metrics(&self) -> BTreeMap<String, Metric> {
    let mut registry = self.registry.lock().expect("registry lock");
    let snapshot = registry.clone();
    registry.retain(|key, _| !snapshot.contains_key(key));
    snapshot
  }

  /// Puts back metrics taken out by [`Self::take_metrics`], except keys
  /// that were registered again in the meantime.
  fn restore_metrics(&self, snapshot: BTreeMap<String, Metric>) {
    let mut registry = self.registry.lock().expect("registry lock");
    for (key, metric) in snapshot {
      registry.entry(key).or_insert(metric);
    }
  }

  /// Stops the scheduled report thread, if any. Every caller waits until
  /// the thread has ended; calls after that do nothing. A report the
  /// thread had already started when the stop was requested can still
  /// finish before this returns, so the boundary of the guarantee is
  /// this method returning, not the stop being requested.
  fn stop_reporter_thread(&self) {
    {
      let mut stop_requested = self.stop_requested.lock().expect("stop lock");
      *stop_requested = true;
    }
    self.stop_condvar.notify_all();
    let mut state = self.reporter_thread.lock().expect("reporter thread lock");
    loop {
      match &mut *state {
        | ReporterThreadState::Finished => return,
        | ReporterThreadState::Joining => {
          state = self.reporter_thread_changed.wait(state).expect("reporter thread condvar");
        },
        | ReporterThreadState::Running(handle) => {
          let handle = handle.take().expect("a running reporter thread has its handle");
          *state = ReporterThreadState::Joining;
          drop(state);
          drop(handle.join());
          let mut state = self.reporter_thread.lock().expect("reporter thread lock");
          *state = ReporterThreadState::Finished;
          self.reporter_thread_changed.notify_all();
          return;
        },
      }
    }
  }
}

/// The kind name of a registered metric, for conflict errors.
fn kind_name(metric: &Metric) -> &'static str {
  match metric {
    | Metric::AveragingGauge(_) => "AveragingGauge",
    | Metric::HdrHistogram(_) => "HdrHistogram",
    | Metric::KnownOpsInTimespanTimer(_) => "KnownOpsInTimespanTimer",
    | Metric::FileDescriptorGauge(_) => "FileDescriptorGauge",
  }
}

/// The error of asking `key` for another kind than the registered one.
fn kind_conflict(key: &MetricKey, existing: &Metric, requested: &'static str) -> MetricsError {
  MetricsError::KindConflict { key: key.as_str().to_owned(), existing: kind_name(existing), requested }
}

/// Stops its timer when dropped, on a normal return and on a panic alike.
struct StopOnDrop<'a> {
  timer: &'a crate::metrics_kit::KnownOpsInTimespanTimer,
}

impl Drop for StopOnDrop<'_> {
  fn drop(&mut self) {
    self.timer.stop();
  }
}

/// Registers metrics under keys and reports them.
///
/// When the scheduled reporter writes to the standard output (the output
/// of [`MetricsKit::new`]), no `StdoutLock` may be held while
/// [`MetricsKit::shutdown_metrics`] runs or the kit is dropped: the
/// reporter thread waits for the output lock, and stopping waits for the
/// reporter thread, so holding the lock across a stop deadlocks. Release
/// the output lock first, then stop.
///
/// Mirrors `org.apache.pekko.testkit.metrics.MetricsKit` and `MetricsKitOps`.
pub struct MetricsKit {
  shared: Arc<MetricsKitShared>,
}

impl MetricsKit {
  /// Creates a kit that reports to the standard output.
  pub fn new(settings: MetricsKitSettings) -> Self {
    Self::with_output(settings, Box::new(std::io::stdout()))
  }

  /// Creates a kit that reports to `output`.
  pub fn with_output(settings: MetricsKitSettings, output: Box<dyn Write + Send>) -> Self {
    let has_console = settings.reporters.iter().any(|kind| matches!(kind, MetricsReporterKind::Console));
    let shared = Arc::new(MetricsKitShared {
      registry: Mutex::new(BTreeMap::new()),
      output: Mutex::new(output),
      console_reporter: has_console.then(|| ConsoleReporter::new(settings.console.verbose)),
      scheduled_report_interval: settings.console.scheduled_report_interval,
      stop_requested: Mutex::new(false),
      stop_condvar: Condvar::new(),
      reporter_thread: Mutex::new(ReporterThreadState::Finished),
      reporter_thread_changed: Condvar::new(),
    });
    if shared.console_reporter.is_some() && shared.scheduled_report_interval > Duration::ZERO {
      let thread_shared = Arc::clone(&shared);
      let handle = std::thread::spawn(move || loop {
        let stop_requested = thread_shared.stop_requested.lock().expect("stop lock");
        let (stop_requested, _) = thread_shared
          .stop_condvar
          .wait_timeout_while(stop_requested, thread_shared.scheduled_report_interval, |stop| !*stop)
          .expect("stop condvar");
        if *stop_requested {
          break;
        }
        drop(stop_requested);
        thread_shared.report_once();
      });
      *shared.reporter_thread.lock().expect("reporter thread lock") = ReporterThreadState::Running(Some(handle));
    }
    Self { shared }
  }

  /// Returns a snapshot of the registered metrics.
  ///
  /// Mirrors `registeredMetrics`.
  pub fn registered_metrics(&self) -> BTreeMap<String, Metric> {
    self.shared.registry.lock().expect("registry lock").clone()
  }

  /// Returns the averaging gauge under `key`, registering it if absent.
  ///
  /// # Errors
  ///
  /// Returns [`MetricsError::KindConflict`] when `key` holds another kind.
  pub fn average_gauge(&self, key: &MetricKey) -> Result<AveragingGauge, MetricsError> {
    let mut registry = self.shared.registry.lock().expect("registry lock");
    match registry.get(key.as_str()) {
      | Some(Metric::AveragingGauge(gauge)) => Ok(gauge.clone()),
      | Some(existing) => Err(kind_conflict(key, existing, "AveragingGauge")),
      | None => {
        let gauge = AveragingGauge::new();
        registry.insert(key.as_str().to_owned(), Metric::AveragingGauge(gauge.clone()));
        Ok(gauge)
      },
    }
  }

  /// Runs `run` with a timer of `ops` operations under `key`, stopping the
  /// timer when `run` returns or panics.
  ///
  /// # Errors
  ///
  /// Returns [`MetricsError::KindConflict`] when `key` holds another kind.
  pub fn timed_with_known_ops<T>(&self, key: &MetricKey, ops: i64, run: impl FnOnce() -> T) -> Result<T, MetricsError> {
    let timer = {
      let mut registry = self.shared.registry.lock().expect("registry lock");
      match registry.get(key.as_str()) {
        | Some(Metric::KnownOpsInTimespanTimer(timer)) => timer.clone(),
        | Some(existing) => return Err(kind_conflict(key, existing, "KnownOpsInTimespanTimer")),
        | None => {
          let timer = crate::metrics_kit::KnownOpsInTimespanTimer::new(ops);
          registry.insert(key.as_str().to_owned(), Metric::KnownOpsInTimespanTimer(timer.clone()));
          timer
        },
      }
    };
    let stop_on_drop = StopOnDrop { timer: &timer };
    let value = run();
    drop(stop_on_drop);
    Ok(value)
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
    let derived_key = key.child("hdr-histogram");
    let mut registry = self.shared.registry.lock().expect("registry lock");
    match registry.get(derived_key.as_str()) {
      | Some(Metric::HdrHistogram(histogram)) => Ok(histogram.clone()),
      | Some(existing) => Err(kind_conflict(&derived_key, existing, "HdrHistogram")),
      | None => {
        let histogram = HdrHistogram::new(highest_trackable_value, number_of_significant_value_digits, unit)?;
        registry.insert(derived_key.as_str().to_owned(), Metric::HdrHistogram(histogram.clone()));
        Ok(histogram)
      },
    }
  }

  /// Registers the three file descriptor gauges under
  /// `key / "file-descriptors"`, all or none.
  ///
  /// # Errors
  ///
  /// Returns [`MetricsError`] when one of the keys is registered already.
  pub fn measure_file_descriptors(&self, key: &MetricKey) -> Result<(), MetricsError> {
    let prefix = key.child("file-descriptors");
    let gauges = FileDescriptorMetricSet::new().gauges();
    let mut registry = self.shared.registry.lock().expect("registry lock");
    for (name, _) in &gauges {
      let derived_key = prefix.child(name);
      if registry.contains_key(derived_key.as_str()) {
        return Err(MetricsError::InvalidArgument {
          reason: format!("key {} is already registered", derived_key.as_str()),
        });
      }
    }
    for (name, gauge) in gauges {
      registry.insert(prefix.child(name).as_str().to_owned(), Metric::FileDescriptorGauge(gauge));
    }
    Ok(())
  }

  /// Prints every metric with every reporter. A failed write is not
  /// reported to the caller, as in the original, and every metric stays
  /// registered.
  ///
  /// Mirrors `reportMetrics`.
  pub fn report_metrics(&self) {
    self.shared.report_once();
  }

  /// Prints every metric, then removes the printed ones. Metrics
  /// registered while the report is printed stay registered, and when
  /// the output fails the printed metrics stay registered too, so a
  /// later report can print them again.
  ///
  /// Mirrors `reportAndClearMetrics`.
  pub fn report_and_clear_metrics(&self) {
    let snapshot = self.shared.take_metrics();
    if !self.shared.report_snapshot(&snapshot) {
      self.shared.restore_metrics(snapshot);
    }
  }

  /// Removes every metric.
  ///
  /// Mirrors `clearMetrics()`.
  pub fn clear_metrics(&self) {
    self.shared.registry.lock().expect("registry lock").clear();
  }

  /// Stops the scheduled reports and waits until the reporter thread has
  /// ended, also when another caller is already stopping it. Calls after
  /// the thread has ended do nothing.
  ///
  /// Mirrors `shutdownMetrics`.
  pub fn shutdown_metrics(&self) {
    self.shared.stop_reporter_thread();
  }
}

impl Drop for MetricsKit {
  fn drop(&mut self) {
    self.shared.stop_reporter_thread();
  }
}
