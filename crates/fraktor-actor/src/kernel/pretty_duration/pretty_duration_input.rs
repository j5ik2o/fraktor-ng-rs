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

//! The input of the pretty duration formatting: a finite duration or one
//! of the three special values of the Scala `Duration`.

use core::{num::NonZeroUsize, time::Duration};

use alloc::string::String;

use super::pretty_duration_format::format_g;

/// A duration to format, mirroring the Scala `Duration` the original
/// `PrettyDuration` works on: a finite value, positive or negative
/// infinity, or the undefined value.
///
/// The finite value is a `core::time::Duration`, which cannot be negative,
/// so negative finite durations of the original are outside this type (the
/// original prints those in nanoseconds only).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PrettyDurationInput {
  /// A finite, non-negative duration.
  Finite(Duration),
  /// Positive infinity (`Duration.Inf` in the original).
  Infinite,
  /// Negative infinity (`Duration.MinusInf` in the original).
  MinusInfinite,
  /// The undefined duration (`Duration.Undefined` in the original).
  Undefined,
}

// The units in descending order: nanoseconds per unit and the abbreviation
// the original's `abbreviate` returns.
const UNITS: [(u128, &str); 7] = [
  (86_400_000_000_000, "d"),
  (3_600_000_000_000, "h"),
  (60_000_000_000, "min"),
  (1_000_000_000, "s"),
  (1_000_000, "ms"),
  (1_000, "μs"),
  (1, "ns"),
];

impl PrettyDurationInput {
  /// Formats this duration with the most appropriate unit and 4 digits of
  /// precision, as the original's `pretty` does.
  pub fn pretty(&self) -> String {
    self.pretty_with(false, NonZeroUsize::new(4).expect("4 is not zero"))
  }

  /// Formats this duration with the most appropriate unit, as the
  /// original's `pretty(includeNanos, precision)` does.
  ///
  /// The nanoseconds are kept in `u128` and never saturated: the original's
  /// `FiniteDuration` is built within ±(2^63−1) ns, while a
  /// `core::time::Duration` reaches further, and that range is formatted
  /// from the full value (an extension the original has no case for).
  pub fn pretty_with(&self, include_nanos: bool, precision: NonZeroUsize) -> String {
    match self {
      | Self::Finite(duration) => {
        let nanos = duration.as_nanos();
        let (unit_nanos, abbreviation) = Self::choose_unit(nanos);
        let value = nanos as f64 / unit_nanos as f64;
        let mut out = format_g(value, precision.get());
        out.push(' ');
        out.push_str(abbreviation);
        if include_nanos {
          out.push_str(&alloc::format!(" ({nanos} ns)"));
        }
        out
      },
      | Self::Infinite => String::from("∞ (infinity)"),
      | Self::MinusInfinite => String::from("-∞ (minus infinity)"),
      | Self::Undefined => String::from("undefined"),
    }
  }

  // The original's `chooseUnit`: the first unit whose converted value is
  // greater than zero, else nanoseconds. The conversions are truncating
  // divisions of the full `u128` nanoseconds.
  fn choose_unit(nanos: u128) -> (u128, &'static str) {
    for (unit_nanos, abbreviation) in UNITS {
      if nanos / unit_nanos > 0 {
        return (unit_nanos, abbreviation);
      }
    }
    (1, "ns")
  }
}
