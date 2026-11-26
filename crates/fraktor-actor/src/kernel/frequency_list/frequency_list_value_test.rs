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

//! Unit tests for [`FrequencyList`](super::FrequencyList).

use alloc::{sync::Arc, vec, vec::Vec};
use core::{
  sync::atomic::{AtomicU64, Ordering},
  time::Duration,
};

use super::FrequencyList;
use crate::kernel::Clock;

// Reports whole seconds, the same shape `recency_list_value_test.rs` uses.
struct SecondsClock(AtomicU64);

impl Clock for SecondsClock {
  fn monotonic(&self) -> Duration {
    Duration::from_secs(self.0.load(Ordering::SeqCst))
  }
}

fn list_at(seconds: u64) -> (Arc<SecondsClock>, FrequencyList<&'static str>) {
  let clock = Arc::new(SecondsClock(AtomicU64::new(seconds)));
  let clock_port: Arc<dyn Clock> = clock.clone();
  let list = FrequencyList::new(false, Some(clock_port));
  (clock, list)
}

// The original compares `timestamp < now - d` for `removeOverallLeastRecentOutside`
// (FrequencyList.scala:183) and `timestamp > now - d` for
// `removeOverallMostRecentWithin` (:189). Both are strict, so a value updated
// exactly at the boundary is neither outside nor within and stays put. The
// original spec never places an update exactly on the boundary, so only this
// test distinguishes strict from non-strict comparisons.
#[test]
fn a_value_updated_exactly_at_the_boundary_stays_for_both_windows() {
  let (clock, mut list) = list_at(10);
  list.update("oldest");
  clock.0.store(13, Ordering::SeqCst);
  list.update("boundary");
  clock.0.store(15, Ordering::SeqCst);
  list.update("recent");

  // now = 18, d = 5s, so the boundary is 13. "oldest" at 10 is strictly older
  // and is removed; "boundary" at 13 stays; "recent" at 15 stays.
  clock.0.store(18, Ordering::SeqCst);
  assert_eq!(list.remove_overall_least_recent_outside(Duration::from_secs(5)), vec!["oldest"]);
  assert_eq!(list.overall_least_to_most_recent().copied().collect::<Vec<_>>(), vec!["boundary", "recent"]);

  // now = 18, d = 3s, so the boundary is 15. "recent" at 15 is not strictly
  // within, "boundary" at 13 is not either, so nothing is removed.
  assert_eq!(list.remove_overall_most_recent_within(Duration::from_secs(3)), Vec::<&str>::new());
  assert_eq!(list.overall_least_to_most_recent().copied().collect::<Vec<_>>(), vec!["boundary", "recent"]);
}

#[test]
#[should_panic(expected = "overall recency is not enabled")]
fn removing_the_overall_least_recent_without_a_clock_panics() {
  let mut list = FrequencyList::<&str>::empty(false);
  list.update("a");

  list.remove_overall_least_recent(1);
}

#[test]
#[should_panic(expected = "overall recency is not enabled")]
fn removing_the_overall_most_recent_without_a_clock_panics() {
  let mut list = FrequencyList::<&str>::empty(false);
  list.update("a");

  list.remove_overall_most_recent(1);
}

#[test]
#[should_panic(expected = "overall recency is not enabled")]
fn removing_the_overall_least_recent_outside_a_window_without_a_clock_panics() {
  let mut list = FrequencyList::<&str>::empty(false);
  list.update("a");

  list.remove_overall_least_recent_outside(Duration::from_secs(1));
}

#[test]
#[should_panic(expected = "overall recency is not enabled")]
fn removing_the_overall_most_recent_within_a_window_without_a_clock_panics() {
  let mut list = FrequencyList::<&str>::empty(false);
  list.update("a");

  list.remove_overall_most_recent_within(Duration::from_secs(1));
}

// A value that the list orders and looks up by `id` alone.
#[derive(Clone, Debug)]
struct Keyed {
  id: u32,
  label: &'static str,
}

impl PartialEq for Keyed {
  fn eq(&self, other: &Self) -> bool {
    self.id == other.id
  }
}

impl Eq for Keyed {}

impl PartialOrd for Keyed {
  fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
    Some(self.cmp(other))
  }
}

impl Ord for Keyed {
  fn cmp(&self, other: &Self) -> core::cmp::Ordering {
    self.id.cmp(&other.id)
  }
}

fn keyed_list_with_an_equal_update() -> FrequencyList<Keyed> {
  let clock: Arc<dyn Clock> = Arc::new(SecondsClock(AtomicU64::new(0)));
  let mut list = FrequencyList::new(false, Some(clock));
  list.update(Keyed { id: 7, label: "old" });
  list.update(Keyed { id: 8, label: "other" });
  list.update(Keyed { id: 7, label: "new" });
  list
}

fn labels<'a>(values: impl Iterator<Item = &'a Keyed>) -> Vec<&'static str> {
  values.map(|keyed| keyed.label).collect()
}

#[test]
fn updating_a_held_value_keeps_the_value_first_added() {
  let mut list = keyed_list_with_an_equal_update();

  assert_eq!(labels(list.least_to_most_frequent()), vec!["other", "old"]);
  assert_eq!(labels(list.most_to_least_frequent()), vec!["old", "other"]);
  assert_eq!(labels(list.overall_least_to_most_recent()), vec!["other", "old"]);
  assert_eq!(labels(list.overall_most_to_least_recent()), vec!["old", "other"]);
  assert_eq!(labels(list.remove_most_frequent(1).iter()), vec!["old"]);

  let mut list_for_overall = keyed_list_with_an_equal_update();
  assert_eq!(labels(list_for_overall.remove_overall_most_recent(1).iter()), vec!["old"]);

  // The held value also survives another move after the equal update.
  list.update(Keyed { id: 8, label: "newer" });
  assert_eq!(labels(list.least_to_most_frequent()), vec!["other"]);
  assert_eq!(labels(list.overall_least_to_most_recent()), vec!["other"]);
}

// Under dynamic aging, skipping `a` (priority 1) removes `b` (priority 3) and
// raises the age to 3, so `c` and `d` enter at priority 4. Removing `a` then
// lowers the age to 1, and updating `c` gives it priority 2 + 1 = 3. Pekko's
// `findNextOrElseInsert` only walks forward from `c`'s bucket, so it places the
// new bucket right after priority 4 (FrequencyList.scala:222-231), and the next
// least frequent removal takes `d` before the lower priority `c`.
#[test]
fn an_age_lowered_after_a_skipping_removal_keeps_pekkos_bucket_order() {
  let mut list = FrequencyList::<&'static str>::empty(true);
  list.update("a");
  list.update("b").update("b").update("b");
  assert_eq!(list.remove_least_frequent_skipping(1, &"a"), vec!["b"]);
  list.update("c").update("d");
  assert_eq!(list.least_to_most_frequent().copied().collect::<Vec<_>>(), vec!["a", "c", "d"]);
  assert_eq!(list.remove_least_frequent(1), vec!["a"]);

  list.update("c");

  assert_eq!(list.least_to_most_frequent().copied().collect::<Vec<_>>(), vec!["d", "c"]);
  assert_eq!(list.remove_least_frequent(1), vec!["d"]);
  assert_eq!(list.least_to_most_frequent().copied().collect::<Vec<_>>(), vec!["c"]);
}
