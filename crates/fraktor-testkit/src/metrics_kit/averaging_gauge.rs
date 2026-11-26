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

//! A gauge that reports the arithmetic mean of the values added to it.

/// The arithmetic mean of the values added so far. Clones share the state.
///
/// Mirrors `org.apache.pekko.testkit.metrics.AveragingGauge`.
#[derive(Clone)]
pub struct AveragingGauge {
  _private: (),
}

impl AveragingGauge {
  /// Creates a gauge with no values.
  pub fn new() -> Self {
    todo!()
  }

  /// Adds one value.
  ///
  /// Mirrors `add(Long)`.
  pub fn add(&self, value: i64) {
    let _ = value;
    todo!()
  }

  /// Adds every value of `values`.
  ///
  /// Mirrors `add(Seq[Long])`.
  pub fn add_slice(&self, values: &[i64]) {
    let _ = values;
    todo!()
  }

  /// Returns the sum divided by the number of values, `NaN` when empty.
  ///
  /// Mirrors `getValue`.
  pub fn value(&self) -> f64 {
    todo!()
  }
}

impl Default for AveragingGauge {
  fn default() -> Self {
    Self::new()
  }
}
