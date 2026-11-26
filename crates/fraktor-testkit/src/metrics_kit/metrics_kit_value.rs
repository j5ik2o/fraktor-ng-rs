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

/// The state a [`MetricsKit`] and its scheduled report thread share.
struct MetricsKitShared {
  registry: Mutex<BTreeMap<String, Metric>>,
  output: Mutex<Box<dyn Write + Send>>,
  console_reporter: Option<ConsoleReporter>,
  scheduled_report_interval: Duration,
  stop_requested: Mutex<bool>,
  stop_condvar: Condvar,
  reporter_thread: Mutex<Option<JoinHandle<()>>>,
}

/// Prints the registered metrics once, if a reporter is running.
fn report_once(shared: &MetricsKitShared) {
  let Some(reporter) = &shared.console_reporter else {
    return;
  };
  let snapshot = shared.registry.lock().expect("registry lock").clone();
  let mut output = shared.output.lock().expect("output lock");
  reporter.report(&snapshot, &mut *output);
}

/// Stops the scheduled report thread, if any. Later calls do nothing.
fn stop_reporter_thread(shared: &MetricsKitShared) {
  {
    let mut stop_requested = shared.stop_requested.lock().expect("stop lock");
    *stop_requested = true;
  }
  shared.stop_condvar.notify_all();
  let handle = shared.reporter_thread.lock().expect("reporter thread lock").take();
  if let Some(handle) = handle {
    let _ = handle.join();
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
      reporter_thread: Mutex::new(None),
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
        report_once(&thread_shared);
      });
      *shared.reporter_thread.lock().expect("reporter thread lock") = Some(handle);
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

  /// Prints every metric with every reporter.
  ///
  /// Mirrors `reportMetrics`.
  pub fn report_metrics(&self) {
    report_once(&self.shared);
  }

  /// Prints every metric, then removes them all.
  ///
  /// Mirrors `reportAndClearMetrics`.
  pub fn report_and_clear_metrics(&self) {
    self.report_metrics();
    self.clear_metrics();
  }

  /// Removes every metric.
  ///
  /// Mirrors `clearMetrics()`.
  pub fn clear_metrics(&self) {
    self.shared.registry.lock().expect("registry lock").clear();
  }

  /// Stops the scheduled reports. Calls after the first do nothing.
  ///
  /// Mirrors `shutdownMetrics`.
  pub fn shutdown_metrics(&self) {
    stop_reporter_thread(&self.shared);
  }
}

impl Drop for MetricsKit {
  fn drop(&mut self) {
    stop_reporter_thread(&self.shared);
  }
}
