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

//! A timer of a known number of operations, stopped once.

use core::time::Duration;
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// The state the clones of a [`KnownOpsInTimespanTimer`] share.
struct TimerState {
  started_at: Instant,
  stopped_at: Mutex<Option<Instant>>,
}

/// Measures the time a known number of operations take. Clones share the
/// state.
///
/// Mirrors `org.apache.pekko.testkit.metrics.KnownOpsInTimespanTimer`.
#[derive(Clone)]
pub struct KnownOpsInTimespanTimer {
  expected_ops: i64,
  state: Arc<TimerState>,
}

impl KnownOpsInTimespanTimer {
  /// Starts a timer of `expected_ops` operations.
  pub fn new(expected_ops: i64) -> Self {
    Self { expected_ops, state: Arc::new(TimerState { started_at: Instant::now(), stopped_at: Mutex::new(None) }) }
  }

  /// Stops the timer. Only the first call returns `true`.
  pub fn stop(&self) -> bool {
    let mut stopped_at = self.state.stopped_at.lock().expect("timer lock");
    if stopped_at.is_some() {
      return false;
    }
    *stopped_at = Some(Instant::now());
    true
  }

  /// Returns the expected number of operations.
  ///
  /// Mirrors `getCount`.
  pub fn count(&self) -> i64 {
    self.expected_ops
  }

  /// Returns the time between the start and the stop, `None` until stopped.
  pub fn elapsed(&self) -> Option<Duration> {
    let stopped_at = self.state.stopped_at.lock().expect("timer lock");
    stopped_at.map(|stopped| stopped.duration_since(self.state.started_at))
  }

  /// Returns the average nanoseconds per operation, `None` until stopped.
  ///
  /// Mirrors `avgDuration`.
  pub fn avg_duration_nanos(&self) -> Option<i64> {
    self.elapsed().map(|elapsed| (elapsed.as_nanos() as f64 / self.expected_ops as f64) as i64)
  }

  /// Returns the operations per second, `None` until stopped.
  ///
  /// Mirrors `opsPerSecond`.
  pub fn ops_per_second(&self) -> Option<f64> {
    self.elapsed().map(|elapsed| self.expected_ops as f64 / (elapsed.as_nanos() as f64 / 1_000_000_000.0))
  }
}
