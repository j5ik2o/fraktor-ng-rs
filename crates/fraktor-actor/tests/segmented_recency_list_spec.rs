// SPDX-License-Identifier: Apache-2.0
//
// Licensed to the Apache Software Foundation (ASF) under one or more
// license agreements; and to You under the Apache License, version 2.0:
//
//   https://www.apache.org/licenses/LICENSE-2.0
//
// This file is part of the Apache Pekko project, which was derived from Akka.
//
// Copyright (C) 2016-2022 Lightbend Inc. <https://www.lightbend.com>
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.

// Checked against Pekko v2.0.0 (SHA 9fbd2283b1cf701c90a1223bf621942c191db61f): the upstream test
// file changes only by ab12a8f5ec #2766, 870d4499cc #2235; the cases and their inputs are
// unchanged.
// The record is docs/baseline/pekko-v2.0.0-migration/batch-3.md.
//
// Ported from Pekko v1.7.1 (SHA 9a54d1294c3ac7821bbed3c940769c3d5027d1ff):
//   actor-tests/src/test/scala/org/apache/pekko/util/SegmentedRecencyListSpec.scala
//
// The original's four cases under "SegmentedRecencyList must" are kept as one
// test each, named after the case, with the original name as a comment. Every
// operation from each case runs in the original order, and each `check` stays
// where the original puts it.
//
// The original drives the fourth case with `SegmentedRecencyListSpec.TestClock`,
// a `RecencyList.Clock` that advances one second per `tick()` and answers
// `earlierTime(duration)` as `currentTime() - duration.toSeconds`. The port's
// clock is the kernel's monotonic `Clock` port, which reports a `Duration`
// instead of a unitless `Long`. The list subtracts the window from that
// `Duration` itself, so a test clock that reports whole seconds gives the same
// window boundaries as the original's `earlierTime`.
//
// The original builds the first three cases with `SegmentedRecencyList.empty(limits)`
// (no clock, `OptionVal.None`) and the fourth with
// `new SegmentedRecencyList(initialLimits, OptionVal.Some(clock))`. The port's
// `empty` takes a slice of `usize` limits and `new` takes a slice plus
// `Option<Arc<dyn Clock>>`, matching the `RecencyList::new(Arc<dyn Clock>)`
// precedent from #124.
//
// The original's `check` starts each segment with
// `expectedSegment.forall(recency.contains)` and discards the result, so that
// line asserts nothing. The port keeps it as an evaluation whose result is
// discarded, and does not turn it into an assertion: faithfulness to the
// original wins over turning a no-op into a stronger check here. `check(recency, Nil)`
// therefore only confirms that the overall size is 0, because the original's
// per-segment loop does not run for an empty list.
//
// The test calls the production `SegmentedRecencyList`. Until it exists the
// file does not compile: that compile error is the intended red for this node.

use core::{
  sync::atomic::{AtomicU64, Ordering},
  time::Duration,
};
use std::sync::Arc;

use fraktor_actor::kernel::{segmented_recency_list::SegmentedRecencyList, Clock};

// The original's `SegmentedRecencyListSpec.TestClock`: durations are always in
// seconds.
struct TestClock {
  time: AtomicU64,
}

impl TestClock {
  fn new() -> Self {
    Self { time: AtomicU64::new(0) }
  }

  fn tick(&self) {
    self.time.fetch_add(1, Ordering::SeqCst);
  }
}

impl Clock for TestClock {
  fn monotonic(&self) -> Duration {
    Duration::from_secs(self.time.load(Ordering::SeqCst))
  }
}

// The original's `check(recency, expectedSegments)`.
fn check(recency: &SegmentedRecencyList<&'static str>, expected_segments: &[&[&'static str]]) {
  assert_eq!(recency.len(), expected_segments.iter().map(|segment| segment.len()).sum::<usize>());
  for (level, expected_segment) in expected_segments.iter().enumerate() {
    // The original discards this result as well.
    let _discarded = expected_segment.iter().all(|value| recency.contains(value));
    assert_eq!(recency.least_to_most_recent_of(level).copied().collect::<Vec<_>>(), *expected_segment);
    assert_eq!(recency.len_of(level), expected_segment.len());
  }
}

// "SegmentedRecencyList must track recency of elements across 2 segments"
#[test]
fn segmented_recency_list_must_track_recency_of_elements_across_2_segments() {
  let mut recency = SegmentedRecencyList::empty(&[2, 3]);

  check(&recency, &[]);

  recency.update("a").update("b").update("c").update("d").update("e").update("f");
  check(&recency, &[&["a", "b", "c", "d", "e", "f"], &[]]);

  assert_eq!(recency.remove_least_recent_over_limit(), vec!["a"]);
  check(&recency, &[&["b", "c", "d", "e", "f"], &[]]);

  recency.update("a").update("b").update("c").update("d");
  check(&recency, &[&["e", "f", "a"], &["b", "c", "d"]]);

  assert_eq!(recency.remove_least_recent_over_limit(), vec!["e"]);
  check(&recency, &[&["f", "a"], &["b", "c", "d"]]);

  recency.update("a");
  check(&recency, &[&["f", "b"], &["c", "d", "a"]]);

  assert_eq!(recency.remove_least_recent_over_limit(), Vec::<&str>::new());
  check(&recency, &[&["f", "b"], &["c", "d", "a"]]);

  recency.update("a").update("b").update("c").update("d").update("e").update("f").update("g");
  check(&recency, &[&["a", "e", "b", "g"], &["c", "d", "f"]]);

  assert_eq!(recency.remove_least_recent_over_limit(), vec!["a", "e"]);
  check(&recency, &[&["b", "g"], &["c", "d", "f"]]);
}

// "SegmentedRecencyList must track recency of elements across 4 segments"
#[test]
fn segmented_recency_list_must_track_recency_of_elements_across_4_segments() {
  let mut recency = SegmentedRecencyList::empty(&[4, 3, 2, 1]);

  check(&recency, &[]);

  recency.update("a").update("b").update("c").update("d").update("e").update("f");
  check(&recency, &[&["a", "b", "c", "d", "e", "f"], &[], &[], &[]]);

  assert_eq!(recency.remove_least_recent_over_limit(), Vec::<&str>::new());
  check(&recency, &[&["a", "b", "c", "d", "e", "f"], &[], &[], &[]]);

  recency.update("g").update("h").update("i").update("j").update("k").update("l");
  check(&recency, &[&["a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l"], &[], &[], &[]]);

  assert_eq!(recency.remove_least_recent_over_limit(), vec!["a", "b"]);
  check(&recency, &[&["c", "d", "e", "f", "g", "h", "i", "j", "k", "l"], &[], &[], &[]]);

  recency.update("b").update("d").update("f").update("h").update("j").update("l");
  check(&recency, &[&["c", "e", "g", "i", "k", "b", "d", "f"], &["h", "j", "l"], &[], &[]]);

  recency.update("h").update("i").update("j").update("k").update("l").update("m");
  check(&recency, &[&["c", "e", "g", "b", "d", "f", "m"], &["i", "k", "h"], &["j", "l"], &[]]);

  assert_eq!(recency.remove_least_recent_over_limit(), vec!["c", "e"]);
  check(&recency, &[&["g", "b", "d", "f", "m"], &["i", "k", "h"], &["j", "l"], &[]]);

  recency.update("j").update("k").update("l").update("m").update("n").update("o").update("p");
  check(&recency, &[&["g", "b", "d", "f", "n", "o", "p"], &["i", "h", "m"], &["k", "j"], &["l"]]);

  assert_eq!(recency.remove_least_recent_over_limit(), vec!["g", "b", "d"]);
  check(&recency, &[&["f", "n", "o", "p"], &["i", "h", "m"], &["k", "j"], &["l"]]);
}

// "SegmentedRecencyList must allow protected limits to be updated"
#[test]
fn segmented_recency_list_must_allow_protected_limits_to_be_updated() {
  let mut recency = SegmentedRecencyList::empty(&[2, 8]);

  check(&recency, &[]);

  recency.update("a").update("b").update("c").update("d").update("e").update("f");
  recency.update("g").update("h").update("i").update("j").update("k").update("l");
  check(&recency, &[&["a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l"], &[]]);

  recency.update("a").update("b").update("c").update("d").update("e").update("f");
  recency.update("g").update("h").update("i").update("j").update("k").update("l");
  check(&recency, &[&["a", "b", "c", "d"], &["e", "f", "g", "h", "i", "j", "k", "l"]]);

  assert_eq!(recency.remove_least_recent_over_limit(), vec!["a", "b"]);
  check(&recency, &[&["c", "d"], &["e", "f", "g", "h", "i", "j", "k", "l"]]);

  recency.update_limits(&[1, 4]);

  assert_eq!(recency.remove_least_recent_over_limit(), vec!["c", "d", "e", "f", "g"]);
  check(&recency, &[&["h"], &["i", "j", "k", "l"]]);
}

// "SegmentedRecencyList must remove overall least recent elements"
#[test]
fn segmented_recency_list_must_remove_overall_least_recent_elements() {
  let clock = Arc::new(TestClock::new());
  let clock_port: Arc<dyn Clock> = clock.clone();
  let mut recency = SegmentedRecencyList::new(&[5, 5], Some(clock_port));

  check(&recency, &[]);

  clock.tick(); // time = 1
  recency.update("a").update("b").update("c").update("d").update("e").update("f").update("g");
  check(&recency, &[&["a", "b", "c", "d", "e", "f", "g"], &[]]);

  clock.tick(); // time = 2
  recency.update("a").update("b").update("c").update("d").update("e").update("f").update("g");
  check(&recency, &[&["a", "b"], &["c", "d", "e", "f", "g"]]);

  clock.tick(); // time = 3
  recency.update("h").update("i").update("j").update("k").update("j").update("k");
  check(&recency, &[&["a", "b", "h", "i", "c", "d"], &["e", "f", "g", "j", "k"]]);

  clock.tick(); // time = 4
  recency.update("l").update("m").update("n").update("n").update("o");
  check(&recency, &[&["a", "b", "h", "i", "c", "d", "l", "m", "e", "o"], &["f", "g", "j", "k", "n"]]);

  clock.tick(); // time = 5
  assert_eq!(
    recency.remove_overall_least_recent_outside(Duration::from_secs(2)),
    vec!["a", "b", "c", "d", "e", "f", "g"]
  );
  check(&recency, &[&["h", "i", "l", "m", "o"], &["j", "k", "n"]]);
}
