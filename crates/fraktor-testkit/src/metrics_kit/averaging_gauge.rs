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

use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

/// The state the clones of an [`AveragingGauge`] share.
struct GaugeState {
  sum: AtomicI64,
  count: AtomicI64,
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
    Self { state: Arc::new(GaugeState { sum: AtomicI64::new(0), count: AtomicI64::new(0) }) }
  }

  /// Adds one value.
  ///
  /// Mirrors `add(Long)`.
  pub fn add(&self, value: i64) {
    self.state.sum.fetch_add(value, Ordering::SeqCst);
    self.state.count.fetch_add(1, Ordering::SeqCst);
  }

  /// Adds every value of `values`.
  ///
  /// Mirrors `add(Seq[Long])`.
  pub fn add_slice(&self, values: &[i64]) {
    let sum = values.iter().copied().fold(0_i64, i64::wrapping_add);
    self.state.sum.fetch_add(sum, Ordering::SeqCst);
    self.state.count.fetch_add(values.len() as i64, Ordering::SeqCst);
  }

  /// Returns the sum divided by the number of values, `NaN` when empty.
  ///
  /// Mirrors `getValue`.
  pub fn value(&self) -> f64 {
    self.state.sum.load(Ordering::SeqCst) as f64 / self.state.count.load(Ordering::SeqCst) as f64
  }
}

impl Default for AveragingGauge {
  fn default() -> Self {
    Self::new()
  }
}
