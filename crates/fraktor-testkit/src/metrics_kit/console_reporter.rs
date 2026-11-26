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

//! The reporter that prints the registered metrics.

use std::collections::BTreeMap;
use std::io;
use std::io::Write;

use fraktor_actor::kernel::pretty_duration::PrettyDurationInput;

use crate::metrics_kit::hdr_histogram::PERCENTILES;
use crate::metrics_kit::Metric;

/// Prints every registered metric to an output.
///
/// Mirrors `org.apache.pekko.testkit.metrics.reporter.PekkoConsoleReporter`.
pub struct ConsoleReporter {
  verbose: bool,
}

impl ConsoleReporter {
  /// Creates a reporter. `verbose` adds the percentile distribution of
  /// histograms to the report.
  pub fn new(verbose: bool) -> Self {
    Self { verbose }
  }

  /// Prints `metrics`, in key order.
  ///
  /// # Errors
  ///
  /// Returns the first write or flush error of `output`. Lines written
  /// before the error stay written, so reporting the same metrics again
  /// after a failure can print those lines twice.
  pub fn report(&self, metrics: &BTreeMap<String, Metric>, output: &mut dyn Write) -> io::Result<()> {
    for (key, metric) in metrics {
      match metric {
        | Metric::AveragingGauge(gauge) => {
          writeln!(output, "{key}: gauge value={}", gauge.value())?;
        },
        | Metric::FileDescriptorGauge(gauge) => {
          writeln!(output, "{key}: gauge value={}", gauge.value())?;
        },
        | Metric::HdrHistogram(histogram) => {
          let (count, min, max, percentiles) = histogram.report_values(self.verbose);
          writeln!(output, "{key}: histogram count={count} min={min} max={max} unit={}", histogram.unit())?;
          if let Some(values) = percentiles {
            for (percentile, value) in PERCENTILES.iter().zip(values.iter()) {
              writeln!(output, "  {percentile}%: {value}")?;
            }
          }
        },
        | Metric::KnownOpsInTimespanTimer(timer) => {
          let elapsed = match timer.elapsed() {
            | Some(elapsed) => PrettyDurationInput::Finite(elapsed).pretty(),
            | None => "not stopped".to_owned(),
          };
          writeln!(output, "{key}: timer count={} elapsed={elapsed}", timer.count())?;
        },
      }
    }
    output.flush()
  }
}
