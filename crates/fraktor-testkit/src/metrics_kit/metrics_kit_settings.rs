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

//! The settings of a metrics kit.

use crate::metrics_kit::{ConsoleReporterSettings, MetricsReporterKind};

/// The settings of a metrics kit, typed in place of the original's config.
///
/// Mirrors `MetricsKitSettings`.
pub struct MetricsKitSettings {
  /// The reporters to start.
  pub reporters: Vec<MetricsReporterKind>,
  /// The settings of the console reporter.
  pub console: ConsoleReporterSettings,
}

impl Default for MetricsKitSettings {
  /// The values of the testkit tests' `reference.conf`.
  fn default() -> Self {
    todo!()
  }
}
