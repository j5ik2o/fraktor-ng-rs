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

//! Unit tests for [`RestartHistory`](super::RestartHistory).

use core::time::Duration;

use crate::kernel::supervision::RestartHistory;
use crate::kernel::supervision::RestartLimit;

// Mirrors `ChildRestartStats.requestRestartPermission` for `(Some(retries), None)`:
// every request counts and is allowed until the count passes the limit.
#[test]
fn a_bounded_count_allows_up_to_the_limit_without_a_window() {
  let mut history = RestartHistory::new();
  assert!(history.request_permission(RestartLimit::new(2), None, Duration::ZERO));
  assert!(history.request_permission(RestartLimit::new(2), None, Duration::ZERO));
  assert!(!history.request_permission(RestartLimit::new(2), None, Duration::ZERO));
}

// Mirrors `(Some(retries), _) if retries < 1 => false`: a non-positive limit
// denies the first request at once.
#[test]
fn a_zero_limit_denies_at_once() {
  let mut history = RestartHistory::new();
  assert!(!history.request_permission(RestartLimit::new(0), None, Duration::ZERO));
}

// The first request opens the window at `now`, and the window stays open for
// exactly `window`, the boundary included, as Pekko's `(now - start) <= window`.
#[test]
fn the_window_boundary_is_inclusive() {
  let window = Duration::from_secs(10);
  let mut history = RestartHistory::new();
  assert!(history.request_permission(RestartLimit::new(1), Some(window), Duration::ZERO));
  assert!(!history.request_permission(RestartLimit::new(1), Some(window), window));
}

// Once the window has passed, Pekko closes it and opens a new one with a count
// of one, so the next request is allowed even when the limit was reached.
#[test]
fn an_expired_window_reopens_and_allows() {
  let window = Duration::from_secs(10);
  let mut history = RestartHistory::new();
  assert!(history.request_permission(RestartLimit::new(1), Some(window), Duration::ZERO));
  assert!(!history.request_permission(RestartLimit::new(1), Some(window), window));
  assert!(history.request_permission(RestartLimit::new(1), Some(window), window + Duration::from_secs(1)));
}
