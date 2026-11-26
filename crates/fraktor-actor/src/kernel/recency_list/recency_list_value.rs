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

//! A mutable list that orders values from the least to the most recently
//! updated.

use alloc::{collections::BTreeMap, sync::Arc, vec::Vec};
use core::time::Duration;

use crate::kernel::Clock;

#[cfg(test)]
#[path = "recency_list_value_test.rs"]
mod tests;

/// Orders values from the least to the most recently updated, and removes
/// them from either end by count or by the time of their last update.
///
/// Mirrors `org.apache.pekko.util.RecencyList`, the internal list that least
/// and most recently used eviction policies build on. Updating a value stamps
/// it with the clock's current time and moves it to the most recent end;
/// updating values in the same instant keeps the order of the updates.
///
/// Pekko pairs a doubly linked list with a hash map. The kernel has no hash
/// map, so the port keeps the order in a map keyed by an update sequence
/// number and looks values up in an ordered map. Values therefore need `Ord`
/// instead of a hash, and the operations take logarithmic time.
///
/// Pekko's clock answers `earlierTime(duration)` in its own unit. The port
/// reads the kernel's monotonic [`Clock`] port and subtracts the window from
/// that [`Duration`] itself. A window that reaches before the clock's epoch
/// covers every update.
pub struct RecencyList<A> {
  clock: Arc<dyn Clock>,
  next_sequence: u64,
  sequence_of: BTreeMap<A, u64>,
  by_recency: BTreeMap<u64, Entry<A>>,
}

struct Entry<A> {
  value: A,
  timestamp: Duration,
}

impl<A: Ord + Clone> RecencyList<A> {
  /// Creates an empty list that stamps updates with the given clock.
  pub fn new(clock: Arc<dyn Clock>) -> Self {
    Self { clock, next_sequence: 0, sequence_of: BTreeMap::new(), by_recency: BTreeMap::new() }
  }

  /// Returns the number of values in the list.
  pub fn len(&self) -> usize {
    self.sequence_of.len()
  }

  /// Returns whether the list holds no values.
  pub fn is_empty(&self) -> bool {
    self.sequence_of.is_empty()
  }

  /// Returns whether the list holds the given value.
  pub fn contains(&self, value: &A) -> bool {
    self.sequence_of.contains_key(value)
  }

  /// Stamps the value with the current time and makes it the most recent,
  /// adding it if the list does not hold it yet.
  ///
  /// When the list already holds an equal value, it keeps the value it holds
  /// and only moves it, as Pekko keeps the existing node.
  pub fn update(&mut self, value: A) -> &mut Self {
    let timestamp = self.clock.monotonic();
    let sequence = self.next_sequence;
    self.next_sequence += 1;
    let value = match self.sequence_of.insert(value.clone(), sequence) {
      | Some(previous) => self.by_recency.remove(&previous).map_or(value, |held| held.value),
      | None => value,
    };
    self.by_recency.insert(sequence, Entry { value, timestamp });
    self
  }

  /// Removes the value if the list holds it.
  pub fn remove(&mut self, value: &A) -> &mut Self {
    if let Some(sequence) = self.sequence_of.remove(value) {
      self.by_recency.remove(&sequence);
    }
    self
  }

  /// Iterates the values from the least to the most recently updated.
  pub fn least_to_most_recent(&self) -> impl Iterator<Item = &A> {
    self.by_recency.values().map(|entry| &entry.value)
  }

  /// Iterates the values from the most to the least recently updated.
  pub fn most_to_least_recent(&self) -> impl Iterator<Item = &A> {
    self.by_recency.values().rev().map(|entry| &entry.value)
  }

  /// Removes up to `n` of the least recent values and returns them from the
  /// least recent on.
  pub fn remove_least_recent(&mut self, n: usize) -> Vec<A> {
    self.remove_least_recent_skipping(n, 0)
  }

  /// Skips the `skip` least recent values, then removes up to `n` of the next
  /// ones and returns them from the least recent on.
  pub fn remove_least_recent_skipping(&mut self, n: usize, skip: usize) -> Vec<A> {
    let sequences: Vec<u64> = self.by_recency.keys().skip(skip).take(n).copied().collect();
    self.remove_sequences(sequences)
  }

  /// Removes up to `n` of the most recent values and returns them from the
  /// most recent on.
  pub fn remove_most_recent(&mut self, n: usize) -> Vec<A> {
    self.remove_most_recent_skipping(n, 0)
  }

  /// Skips the `skip` most recent values, then removes up to `n` of the next
  /// ones and returns them from the most recent on.
  pub fn remove_most_recent_skipping(&mut self, n: usize, skip: usize) -> Vec<A> {
    let sequences: Vec<u64> = self.by_recency.keys().rev().skip(skip).take(n).copied().collect();
    self.remove_sequences(sequences)
  }

  /// Removes the values last updated earlier than `duration` before now and
  /// returns them from the least recent on. A value updated exactly
  /// `duration` before now stays.
  pub fn remove_least_recent_outside(&mut self, duration: Duration) -> Vec<A> {
    let Some(earliest_kept) = self.clock.monotonic().checked_sub(duration) else {
      return Vec::new();
    };
    let sequences: Vec<u64> = self
      .by_recency
      .iter()
      .take_while(|(_, entry)| entry.timestamp < earliest_kept)
      .map(|(sequence, _)| *sequence)
      .collect();
    self.remove_sequences(sequences)
  }

  /// Removes the values last updated later than `duration` before now and
  /// returns them from the most recent on. A value updated exactly `duration`
  /// before now stays.
  pub fn remove_most_recent_within(&mut self, duration: Duration) -> Vec<A> {
    let latest_kept = self.clock.monotonic().checked_sub(duration);
    let sequences: Vec<u64> = self
      .by_recency
      .iter()
      .rev()
      .take_while(|(_, entry)| latest_kept.is_none_or(|latest_kept| entry.timestamp > latest_kept))
      .map(|(sequence, _)| *sequence)
      .collect();
    self.remove_sequences(sequences)
  }

  fn remove_sequences(&mut self, sequences: Vec<u64>) -> Vec<A> {
    sequences
      .into_iter()
      .filter_map(|sequence| self.by_recency.remove(&sequence))
      .map(|entry| {
        self.sequence_of.remove(&entry.value);
        entry.value
      })
      .collect()
  }
}
