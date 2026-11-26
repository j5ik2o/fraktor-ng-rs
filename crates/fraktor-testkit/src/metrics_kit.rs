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

//! Measurements for tests: gauges, histograms and timers kept in a registry
//! and printed by a console reporter.
//!
//! Mirrors the `org.apache.pekko.testkit.metrics` package of Pekko's testkit
//! tests (`MetricsKit`, `MetricsKitOps`, `MetricKeyDSL` and the metric types).

mod averaging_gauge;
mod console_reporter;
mod console_reporter_settings;
mod file_descriptor_gauge;
mod file_descriptor_metric_set;
mod hdr_histogram;
mod known_ops_in_timespan_timer;
mod metric;
mod metric_key;
mod metrics_error;
mod metrics_kit_settings;
mod metrics_kit_value;
mod metrics_reporter_kind;

pub use averaging_gauge::AveragingGauge;
pub use console_reporter::ConsoleReporter;
pub use console_reporter_settings::ConsoleReporterSettings;
pub use file_descriptor_gauge::FileDescriptorGauge;
pub use file_descriptor_metric_set::FileDescriptorMetricSet;
pub use hdr_histogram::HdrHistogram;
pub use known_ops_in_timespan_timer::KnownOpsInTimespanTimer;
pub use metric::Metric;
pub use metric_key::MetricKey;
pub use metrics_error::MetricsError;
pub use metrics_kit_settings::MetricsKitSettings;
pub use metrics_kit_value::MetricsKit;
pub use metrics_reporter_kind::MetricsReporterKind;
