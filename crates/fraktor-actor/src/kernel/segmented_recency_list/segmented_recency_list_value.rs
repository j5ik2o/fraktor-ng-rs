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
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.

//! A mutable recency list whose values move up through segments as they are
//! updated again.

use alloc::{collections::BTreeMap, vec::Vec};
use core::time::Duration;

use crate::kernel::sync::Arc;
use crate::kernel::Clock;

#[cfg(test)]
#[path = "segmented_recency_list_value_test.rs"]
mod tests;

/// Orders values by recency within segments, for segmented least recently
/// used eviction.
///
/// Mirrors `org.apache.pekko.util.SegmentedRecencyList`. Level 0 is the
/// probationary segment and the higher levels are protected. A new value
/// enters level 0. Updating a held value promotes it one level, or moves it to
/// the most recent end of the highest level. A level that grows over its limit
/// demotes its least recent values to the level below. Each segment orders its
/// values from the least to the most recently placed.
///
/// With a clock, the list also stamps every update and keeps the overall
/// recency across all segments, so that values not updated within a window
/// can be removed from every segment at once.
///
/// Pekko pairs doubly linked lists with a hash map. The kernel has no hash
/// map, so the port keeps each order in a map keyed by a placement sequence
/// number and looks values up in an ordered map, as
/// [`RecencyList`](crate::kernel::recency_list::RecencyList) does. Values
/// therefore need `Ord` instead of a hash. Each placement, lookup and removal
/// takes logarithmic time, so an operation that demotes or removes `k` values
/// takes `O(k log n)` instead of Pekko's `O(k)`.
pub struct SegmentedRecencyList<A> {
  limits: SegmentLimits,
  segments: Vec<BTreeMap<u64, A>>,
  nodes: BTreeMap<A, Node>,
  overall: Option<OverallRecency<A>>,
  next_sequence: u64,
}

#[derive(Clone, Copy)]
struct Node {
  level: usize,
  sequence: u64,
  overall_sequence: u64,
}

// The limit of each segment, from the probationary level 0 up, with their
// total. A list always has at least one segment. The total saturates at
// `usize::MAX`, which no list can exceed.
struct SegmentLimits {
  per_level: Vec<usize>,
  total: usize,
}

impl SegmentLimits {
  fn new(limits: &[usize]) -> Self {
    assert!(!limits.is_empty(), "a segmented recency list needs at least one segment");
    let total = limits.iter().fold(0usize, |total, limit| total.saturating_add(*limit));
    Self { per_level: limits.to_vec(), total }
  }

  fn segment_count(&self) -> usize {
    self.per_level.len()
  }

  fn of(&self, level: usize) -> usize {
    self.per_level[level]
  }

  fn total(&self) -> usize {
    self.total
  }

  fn excess_over_total(&self, len: usize) -> usize {
    len.saturating_sub(self.total)
  }
}

struct OverallRecency<A> {
  clock: Arc<dyn Clock>,
  by_recency: BTreeMap<u64, Stamped<A>>,
}

struct Stamped<A> {
  value: A,
  timestamp: Duration,
}

impl<A: Ord + Clone> SegmentedRecencyList<A> {
  /// Creates an empty list with one segment per limit and no overall recency.
  ///
  /// # Panics
  ///
  /// Panics if `limits` is empty.
  pub fn empty(limits: &[usize]) -> Self {
    Self::new(limits, None)
  }

  /// Creates an empty list with one segment per limit. With a clock, the list
  /// also keeps the overall recency across segments.
  ///
  /// # Panics
  ///
  /// Panics if `limits` is empty.
  pub fn new(limits: &[usize], clock: Option<Arc<dyn Clock>>) -> Self {
    let limits = SegmentLimits::new(limits);
    Self {
      segments: (0..limits.segment_count()).map(|_| BTreeMap::new()).collect(),
      limits,
      nodes: BTreeMap::new(),
      overall: clock.map(|clock| OverallRecency { clock, by_recency: BTreeMap::new() }),
      next_sequence: 0,
    }
  }

  /// Returns the number of values in all segments.
  pub fn len(&self) -> usize {
    self.nodes.len()
  }

  /// Returns whether the list holds no values.
  pub fn is_empty(&self) -> bool {
    self.nodes.is_empty()
  }

  /// Returns the number of values in the segment at `level`.
  ///
  /// # Panics
  ///
  /// Panics if `level` is not a segment of this list.
  pub fn len_of(&self, level: usize) -> usize {
    self.segments[level].len()
  }

  /// Returns whether the list holds the given value.
  pub fn contains(&self, value: &A) -> bool {
    self.nodes.contains_key(value)
  }

  /// Iterates the values of the segment at `level` from the least to the most
  /// recent.
  ///
  /// # Panics
  ///
  /// Panics if `level` is not a segment of this list.
  pub fn least_to_most_recent_of(&self, level: usize) -> impl Iterator<Item = &A> {
    self.segments[level].values()
  }

  /// Adds the value to the probationary segment, or promotes it if the list
  /// already holds it.
  ///
  /// When the list already holds an equal value, it keeps the value it holds
  /// and only moves it, as Pekko keeps the existing node.
  pub fn update(&mut self, value: A) -> &mut Self {
    match self.nodes.get(&value).copied() {
      | Some(node) => self.promote(&value, node),
      | None => self.insert(value),
    }
    self
  }

  /// Removes the least recent values over the total limit and returns them
  /// from the least recent on.
  ///
  /// The protected segments first demote what they hold over their own
  /// limits, from the highest level down, so the removed values all come from
  /// the probationary segment.
  pub fn remove_least_recent_over_limit(&mut self) -> Vec<A> {
    if self.len() <= self.limits.total() {
      return Vec::new();
    }
    for level in (1..self.segments.len()).rev() {
      self.adjust(level);
    }
    let excess = self.limits.excess_over_total(self.len());
    let sequences: Vec<u64> = self.segments[0].keys().take(excess).copied().collect();
    sequences.into_iter().filter_map(|sequence| self.remove_placed(0, sequence)).collect()
  }

  /// Removes the values, from any segment, last updated earlier than
  /// `duration` before now and returns them from the least recent on. A value
  /// updated exactly `duration` before now stays, and a window that reaches
  /// before the clock's epoch keeps every value.
  ///
  /// # Panics
  ///
  /// Panics if the list was created without a clock, as Pekko throws when
  /// overall recency is not enabled.
  pub fn remove_overall_least_recent_outside(&mut self, duration: Duration) -> Vec<A> {
    let Some(overall) = &self.overall else {
      panic!("overall recency is not enabled");
    };
    let Some(earliest_kept) = overall.clock.monotonic().checked_sub(duration) else {
      return Vec::new();
    };
    let expired: Vec<A> = overall
      .by_recency
      .values()
      .take_while(|stamped| stamped.timestamp < earliest_kept)
      .map(|stamped| stamped.value.clone())
      .collect();
    expired
      .into_iter()
      .filter_map(|value| {
        let node = self.nodes.get(&value).copied()?;
        self.remove_placed(node.level, node.sequence)
      })
      .collect()
  }

  /// Replaces the segment limits. The list applies them on the next removal
  /// over the limit.
  ///
  /// # Panics
  ///
  /// Panics if `limits` does not have one limit per segment.
  pub fn update_limits(&mut self, limits: &[usize]) {
    let limits = SegmentLimits::new(limits);
    assert_eq!(limits.segment_count(), self.segments.len(), "the new limits must keep one limit per segment");
    self.limits = limits;
  }

  fn take_sequence(&mut self) -> u64 {
    let sequence = self.next_sequence;
    self.next_sequence += 1;
    sequence
  }

  fn place(&mut self, level: usize, value: A) -> u64 {
    let sequence = self.take_sequence();
    self.segments[level].insert(sequence, value);
    sequence
  }

  fn stamp(&mut self, value: A) -> u64 {
    let sequence = self.take_sequence();
    if let Some(overall) = &mut self.overall {
      let timestamp = overall.clock.monotonic();
      overall.by_recency.insert(sequence, Stamped { value, timestamp });
    }
    sequence
  }

  fn insert(&mut self, value: A) {
    let sequence = self.place(0, value.clone());
    let overall_sequence = self.stamp(value.clone());
    self.nodes.insert(value, Node { level: 0, sequence, overall_sequence });
  }

  fn promote(&mut self, value: &A, node: Node) {
    let Some(held) = self.segments[node.level].remove(&node.sequence) else {
      return;
    };
    let highest = self.segments.len() - 1;
    let level = if node.level == highest { highest } else { node.level + 1 };
    let sequence = self.place(level, held);
    self.set_placement(value, level, sequence);
    if level != node.level {
      self.adjust(level);
    }
    let held = self.overall.as_mut().and_then(|overall| overall.by_recency.remove(&node.overall_sequence));
    if let Some(stamped) = held {
      let overall_sequence = self.stamp(stamped.value);
      if let Some(node) = self.nodes.get_mut(value) {
        node.overall_sequence = overall_sequence;
      }
    }
  }

  fn adjust(&mut self, level: usize) {
    let excess = self.segments[level].len().saturating_sub(self.limits.of(level));
    let sequences: Vec<u64> = self.segments[level].keys().take(excess).copied().collect();
    for sequence in sequences {
      self.demote(level, sequence);
    }
  }

  fn demote(&mut self, level: usize, sequence: u64) {
    let Some(value) = self.segments[level].remove(&sequence) else {
      return;
    };
    let key = value.clone();
    let sequence = self.place(level - 1, value);
    self.set_placement(&key, level - 1, sequence);
  }

  fn set_placement(&mut self, value: &A, level: usize, sequence: u64) {
    if let Some(node) = self.nodes.get_mut(value) {
      node.level = level;
      node.sequence = sequence;
    }
  }

  fn remove_placed(&mut self, level: usize, sequence: u64) -> Option<A> {
    let value = self.segments[level].remove(&sequence)?;
    if let Some(node) = self.nodes.remove(&value) {
      if let Some(overall) = &mut self.overall {
        overall.by_recency.remove(&node.overall_sequence);
      }
    }
    Some(value)
  }
}
