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

//! Supplementary regression tests for [`WildcardIndex`](super::WildcardIndex).
//!
//! Mirrors `org.apache.pekko.util.WildcardIndex`.
//!
//! These pin the behaviours the ported spec does not exercise: the two
//! intentional differences from the original (section 2.2 of the design
//! document for #187) and the original behaviours the port writes as-is,
//! each with the result the reviewers obtained by running the original under
//! sbt (probes Q1 to Q6, S1, D1 and D2).

use super::WildcardIndex;

fn insert(index: &WildcardIndex<i32>, elems: &[&str], value: i32) -> WildcardIndex<i32> {
  index.insert(elems, value).expect("the probes insert no double wildcard suffix")
}

fn find(index: &WildcardIndex<i32>, elems: &[&str]) -> Option<i32> {
  index.find(elems).copied()
}

// Difference 1 (the original's Q2): the original looks a suffix child up by
// the unstripped element, so inserting ["a*", "c"] after ["a*", "b"] loses
// the "b" route. The port keeps both.
#[test]
fn reinserting_a_suffix_pattern_keeps_the_earlier_route() {
  let tree = insert(&insert(&WildcardIndex::new(), &["a*", "b"], 1), &["a*", "c"], 2);
  assert_eq!(find(&tree, &["ax", "b"]), Some(1));
  assert_eq!(find(&tree, &["ax", "c"]), Some(2));
}

// Difference 2 (the original's Q1): the original reports a root holding only
// a suffix child as empty and never matches. The port's root check counts
// suffix children.
#[test]
fn a_root_holding_only_a_suffix_pattern_still_matches() {
  let tree = insert(&WildcardIndex::new(), &["a*"], 1);
  assert!(!tree.is_empty());
  assert_eq!(find(&tree, &["a"]), Some(1));
  assert_eq!(find(&tree, &["ab"]), Some(1));
}

// Difference 2 is root-only: the fallback to the double-wildcard tree still
// uses the original's per-node emptiness, which does not count suffix
// children (the rereview's S1). Fixing the per-node check instead would
// change this answer from Some(2) to None.
#[test]
fn the_double_wildcard_fallback_still_uses_the_original_node_emptiness() {
  let tree = insert(&insert(&WildcardIndex::new(), &["x", "a*"], 1), &["**"], 2);
  assert_eq!(find(&tree, &["x"]), Some(2));
}

// Written as-is (Q3): the fallback to the double-wildcard tree happens only
// when the reached node is structurally empty. Here the walk reaches the
// node for "a" in the pattern ["a", "b*"], which holds a suffix child but is
// structurally empty, so the "**" route answers.
#[test]
fn fallback_happens_only_when_the_reached_node_is_structurally_empty() {
  let tree = insert(&insert(&WildcardIndex::new(), &["a", "b*"], 1), &["**"], 9);
  assert_eq!(find(&tree, &["a"]), Some(9));
  // A query the single-wildcard walk cannot start also falls back.
  assert_eq!(find(&tree, &["ax"]), Some(9));
}

// Written as-is (Q4): matching is greedy and never backtracks. The exact
// child "b" is chosen for the second element even though only the "*" route
// could match the third.
#[test]
fn matching_is_greedy_and_never_backtracks() {
  let tree = insert(&insert(&WildcardIndex::new(), &["a", "*", "c"], 1), &["a", "b", "d"], 2);
  assert_eq!(find(&tree, &["a", "b", "c"]), None);
  assert_eq!(find(&tree, &["a", "b", "d"]), Some(2));
  assert_eq!(find(&tree, &["a", "x", "c"]), Some(1));
}

// Written as-is (the rereview's D1 and D2): the double-wildcard tree never
// consults suffix children, so a route through a suffix pattern ending in
// "**" never matches.
#[test]
fn the_double_wildcard_tree_never_consults_suffix_children() {
  let tree = insert(&WildcardIndex::new(), &["a*", "**"], 1);
  assert_eq!(find(&tree, &["ab", "x"]), None);
  assert_eq!(find(&tree, &["a", "x"]), None);
  let tree = insert(&WildcardIndex::new(), &["x", "a*", "**"], 1);
  assert_eq!(find(&tree, &["x", "ab", "y"]), None);
}

// A failed insert changes nothing, and the original index is immutable.
#[test]
fn a_failed_insert_changes_nothing_and_insert_never_mutates() {
  let tree = insert(&WildcardIndex::new(), &["a"], 1);
  assert!(tree.insert(&["a**"], 2).is_err());
  assert_eq!(find(&tree, &["a"]), Some(1));
  let extended = insert(&tree, &["b"], 2);
  assert_eq!(find(&tree, &["b"]), None);
  assert_eq!(find(&extended, &["b"]), Some(2));
}
