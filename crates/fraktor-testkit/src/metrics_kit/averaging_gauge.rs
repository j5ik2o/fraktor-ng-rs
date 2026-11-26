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

use std::sync::{Arc, Mutex};

/// The state the clones of an [`AveragingGauge`] share.
///
/// The sum and the count sit behind one lock, so an update changes both
/// together and a read always divides a sum by the count of the same state.
struct GaugeState {
  sum_and_count: Mutex<(i64, i64)>,
}

/// The arithmetic mean of the values added so far. Clones share the state.
///
/// Mirrors `org.apache.pekko.testkit.metrics.AveragingGauge`.
#[derive(Clone)]
pub struct AveragingGauge {
  state: Arc<GaugeState>,
}

impl AveragingGauge {
  /// Creates a gauge with no values.
  pub fn new() -> Self {
    Self { state: Arc::new(GaugeState { sum_and_count: Mutex::new((0, 0)) }) }
  }

  /// Adds one value.
  ///
  /// Mirrors `add(Long)`.
  pub fn add(&self, value: i64) {
    let mut sum_and_count = self.state.sum_and_count.lock().expect("gauge lock");
    sum_and_count.0 = sum_and_count.0.wrapping_add(value);
    sum_and_count.1 = sum_and_count.1.wrapping_add(1);
  }

  /// Adds every value of `values`.
  ///
  /// Mirrors `add(Seq[Long])`.
  pub fn add_slice(&self, values: &[i64]) {
    let sum = values.iter().copied().fold(0_i64, i64::wrapping_add);
    let mut sum_and_count = self.state.sum_and_count.lock().expect("gauge lock");
    sum_and_count.0 = sum_and_count.0.wrapping_add(sum);
    sum_and_count.1 = sum_and_count.1.wrapping_add(values.len() as i64);
  }

  /// Returns the sum divided by the number of values, `NaN` when empty.
  ///
  /// Mirrors `getValue`.
  pub fn value(&self) -> f64 {
    let sum_and_count = self.state.sum_and_count.lock().expect("gauge lock");
    sum_and_count.0 as f64 / sum_and_count.1 as f64
  }
}

impl Default for AveragingGauge {
  fn default() -> Self {
    Self::new()
  }
}
