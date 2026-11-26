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

//! The settings of the console reporter.

use core::time::Duration;

/// The settings of the console reporter.
///
/// Mirrors `pekko.test.metrics.reporter.console`.
pub struct ConsoleReporterSettings {
  /// The interval of the scheduled reports. Zero turns them off.
  pub scheduled_report_interval: Duration,
  /// Whether the reports include the percentile distribution.
  pub verbose: bool,
}
