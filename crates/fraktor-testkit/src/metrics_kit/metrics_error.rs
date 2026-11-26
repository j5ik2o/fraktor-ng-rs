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

//! The errors of the metrics kit.

use core::fmt;

/// An error of the metrics kit.
#[derive(Debug)]
pub enum MetricsError {
  /// The key is registered for a different kind of metric.
  KindConflict {
    /// The key.
    key: String,
    /// The kind already registered.
    existing: &'static str,
    /// The kind asked for.
    requested: &'static str,
  },
  /// The value cannot be stored in the histogram.
  HistogramValueOutOfRange {
    /// The value.
    value: i64,
    /// The lowest discernible value of the histogram.
    lowest_discernible_value: i64,
    /// The highest trackable value of the histogram.
    highest_trackable_value: i64,
  },
  /// An argument is outside what the metric accepts.
  InvalidArgument {
    /// What is wrong.
    reason: String,
  },
}

impl fmt::Display for MetricsError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      | Self::KindConflict { key, existing, requested } => {
        write!(f, "Key: [{key}] is already for different kind of metric! Was [{existing}], expected [{requested}]")
      },
      | Self::HistogramValueOutOfRange { value, lowest_discernible_value, highest_trackable_value } => {
        write!(
          f,
          "Given value {value} can not be stored in this histogram (min: {lowest_discernible_value}, max: {highest_trackable_value})"
        )
      },
      | Self::InvalidArgument { reason } => f.write_str(reason),
    }
  }
}

impl std::error::Error for MetricsError {}
