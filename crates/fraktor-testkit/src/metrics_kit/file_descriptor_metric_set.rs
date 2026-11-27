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

//! The set of the file descriptor gauges.

use crate::metrics_kit::FileDescriptorGauge;

/// The three file descriptor gauges and the names they register under.
///
/// Mirrors `org.apache.pekko.testkit.metrics.FileDescriptorMetricSet`.
pub struct FileDescriptorMetricSet;

impl FileDescriptorMetricSet {
  /// Creates the set.
  pub fn new() -> Self {
    Self
  }

  /// Returns the gauges with the names the original registers them under,
  /// before the names are sanitized into registry keys.
  pub fn gauges(&self) -> [(&'static str, FileDescriptorGauge); 3] {
    [
      ("file-descriptors.open", FileDescriptorGauge::open()),
      ("file-descriptors.max", FileDescriptorGauge::max()),
      ("file-descriptors.ratio", FileDescriptorGauge::ratio()),
    ]
  }
}

impl Default for FileDescriptorMetricSet {
  fn default() -> Self {
    Self::new()
  }
}
