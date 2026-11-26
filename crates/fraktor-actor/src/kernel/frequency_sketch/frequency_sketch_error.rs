// SPDX-License-Identifier: Apache-2.0
//
// Licensed to the Apache Software Foundation (ASF) under one or more
// license agreements; and to You under the Apache License, version 2.0:
//
//   https://www.apache.org/licenses/LICENSE-2.0
//
// This file is part of the Apache Pekko project, which was derived from Akka.
//
// Copyright (C) 2021-2022 Lightbend Inc. <https://www.lightbend.com>
//
// Copyright 2015 Ben Manes. All Rights Reserved.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.

//! The error a frequency sketch build reports.

use core::fmt;

/// The reason a frequency sketch constructor rejected its settings.
///
/// Mirrors the `require` checks of `FrequencySketch` and `FastFrequencySketch`
/// in `org.apache.pekko.util`, in the order they run, so the first failure
/// wins. A negative depth, which the original reports as a negative array
/// size, is an error here too.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrequencySketchError {
  /// The width multiplier times the capacity rounded up to a power of two
  /// does not fit in an `i32`.
  WidthOverflow {
    /// The computed width.
    width: i64,
  },
  /// The computed width is not a power of two.
  WidthNotPowerOfTwo {
    /// The computed width.
    width: i32,
  },
  /// The computed width is zero or negative, so the sketch has no counters to
  /// index. The original accepts such a width and fails on the first update.
  ///
  /// A capacity above `2^30` rounds up to `i32::MIN`, as in the original, so
  /// the 64-bit product is negative and its low 32 bits are zero for the
  /// default multiplier. Such a capacity is reported here, not as
  /// [`WidthOverflow`](Self::WidthOverflow), because the original's overflow
  /// check does not catch it either.
  WidthNotPositive {
    /// The computed width.
    width: i32,
  },
  /// The counter size is not 2, 4, 8, 16, 32 or 64 bits.
  UnsupportedCounterBits {
    /// The requested counter size.
    counter_bits: i32,
  },
  /// The depth is negative.
  NegativeDepth {
    /// The requested depth.
    depth: i32,
  },
}

impl fmt::Display for FrequencySketchError {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      | Self::WidthOverflow { width } => {
        write!(formatter, "computed width ({width}) exceeds Int.MaxValue; reduce capacity or widthMultiplier")
      },
      | Self::WidthNotPowerOfTwo { width } => write!(formatter, "width must be a power of two, got {width}"),
      | Self::WidthNotPositive { width } => write!(formatter, "width must be positive, got {width}"),
      | Self::UnsupportedCounterBits { counter_bits } => {
        write!(formatter, "counterBits must be 2, 4, 8, 16, 32, or 64 bits, got {counter_bits}")
      },
      | Self::NegativeDepth { depth } => write!(formatter, "depth must not be negative, got {depth}"),
    }
  }
}

impl core::error::Error for FrequencySketchError {}
