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

//! A mutable frequency list that orders values by how often they were updated.

use alloc::{collections::BTreeMap, vec::Vec};
use core::time::Duration;

use crate::kernel::sync::Arc;
use crate::kernel::Clock;

#[cfg(test)]
#[path = "frequency_list_value_test.rs"]
mod tests;

/// Orders values from the least to the most frequently updated, and removes
/// them by count or by the time of their last update.
///
/// Mirrors `org.apache.pekko.util.FrequencyList`, the internal list that least
/// and most frequently used eviction policies build on. Values with the same
/// frequency are ordered by when they were last updated, the least recent
/// first. Updating a value raises its frequency; a new value starts at
/// frequency one.
///
/// With a clock, the list also stamps every update and keeps the overall
/// recency across all values, so that values not updated within a window can
/// be removed. Dynamic aging, when enabled, raises the effective frequency of a
/// whole insertion by the age recorded on each least-frequent removal, so that
/// entries that survived earlier evictions age together with the list.
///
/// Under dynamic aging the buckets stay in Pekko's order, which is not always
/// sorted by priority. Placing an updated value searches only forward from its
/// current bucket: it walks past every next bucket whose priority is below the
/// new one, uses the next bucket if its priority is equal, and otherwise
/// inserts a bucket after the last one it walked. It never moves back. A
/// removal that skips a value can lower the age, so a value updated after
/// that can get a priority below the bucket it leaves and still land after
/// that bucket. A less frequent removal then takes the values in front of it
/// first. The port keeps that order.
///
/// Pekko pairs a doubly linked list of frequency buckets with a doubly linked
/// list of values in each bucket and a hash map of values. The kernel has no
/// hash map, so the port keeps the buckets in a double-linked list keyed by
/// bucket id, each bucket's values in a map keyed by a placement sequence
/// number, and looks values up in an ordered map, as
/// [`RecencyList`](crate::kernel::recency_list::RecencyList) does. Values
/// therefore need `Ord` instead of a hash. Each value remembers its placement
/// sequence, so placing a value in a bucket, looking it up and taking it out
/// of its bucket each take logarithmic time, and an operation that walks `k`
/// values takes `O(k log n)` instead of Pekko's `O(k)`. Under dynamic aging,
/// finding the target bucket walks the buckets forward as Pekko does, with
/// each step taking logarithmic time.
pub struct FrequencyList<A> {
  buckets: FrequencyBuckets<A>,
  nodes: BTreeMap<A, Node>,
  overall: Option<OverallRecency<A>>,
  aging: Option<Age>,
  next_sequence: u64,
}

// A frequency bucket. Values with the same `priority` live in `values`, ordered
// from the least to the most recently placed. `less` and `more` link the
// buckets from the least to the most frequent, the way Pekko's
// `DoubleLinkedList[FrequencyNode]` links them.
struct Bucket<A> {
  priority: Priority,
  less: Option<u64>,
  more: Option<u64>,
  values: BTreeMap<u64, A>,
}

// What the list remembers about a value: how many times it was updated, which
// bucket it sits in, its placement sequence within that bucket, and where it
// sits in the overall recency order.
#[derive(Clone, Copy)]
struct Node {
  accesses: Accesses,
  bucket: u64,
  bucket_sequence: u64,
  overall_sequence: Option<u64>,
}

// How many times the list has seen a value updated, counting the update that
// added it.
#[derive(Clone, Copy)]
struct Accesses(u64);

impl Accesses {
  const FIRST: Self = Self(1);

  fn next(self) -> Self {
    Self(self.0 + 1)
  }
}

// The age that dynamic aging adds to every priority it assigns: the priority of
// the value last removed as the least frequent, and zero before that.
#[derive(Clone, Copy, Default)]
struct Age(u64);

impl Age {
  // Pekko's `age = node.frequency.priority` on a least frequent removal.
  fn of_removed(priority: Priority) -> Self {
    Self(priority.0)
  }
}

// The key of a frequency bucket: the accesses of its values, plus the age when
// dynamic aging is on.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Priority(u64);

impl Priority {
  fn of(accesses: Accesses, aging: Option<Age>) -> Self {
    Self(accesses.0 + aging.map_or(0, |age| age.0))
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

// The double-linked list of frequency buckets. A bucket is identified by the
// `u64` id it was allocated. The list keeps the least and the most frequent
// bucket ids in `first` and `last`, and each bucket names its `less`/`more`
// neighbour.
//
// Keeping the link fields in `Bucket` and the bucket store here mirrors Pekko's
// `DoubleLinkedList[FrequencyNode]`, where `getFirstOrElsePrepend`,
// `getNextOrElseInsert` and `findNextOrElseInsert` move along `getNext`.
struct FrequencyBuckets<A> {
  buckets: BTreeMap<u64, Bucket<A>>,
  first: Option<u64>,
  last: Option<u64>,
  next_bucket_id: u64,
}

impl<A> FrequencyBuckets<A> {
  fn new() -> Self {
    Self { buckets: BTreeMap::new(), first: None, last: None, next_bucket_id: 0 }
  }

  fn first(&self) -> Option<u64> {
    self.first
  }

  fn last(&self) -> Option<u64> {
    self.last
  }

  fn priority_of(&self, bucket: u64) -> Priority {
    self.buckets[&bucket].priority
  }

  fn less_of(&self, bucket: u64) -> Option<u64> {
    self.buckets[&bucket].less
  }

  fn more_of(&self, bucket: u64) -> Option<u64> {
    self.buckets[&bucket].more
  }

  fn values_of(&self, bucket: u64) -> &BTreeMap<u64, A> {
    &self.buckets[&bucket].values
  }

  fn values_of_mut(&mut self, bucket: u64) -> &mut BTreeMap<u64, A> {
    &mut self.buckets.get_mut(&bucket).expect("the bucket exists").values
  }

  fn allocate(&mut self, priority: Priority) -> u64 {
    let bucket = self.next_bucket_id;
    self.next_bucket_id += 1;
    self.buckets.insert(bucket, Bucket { priority, less: None, more: None, values: BTreeMap::new() });
    bucket
  }

  fn link(&mut self, less: Option<u64>, bucket: u64, more: Option<u64>) {
    self.buckets.get_mut(&bucket).expect("the bucket to link exists").less = less;
    self.buckets.get_mut(&bucket).expect("the bucket to link exists").more = more;
    match less {
      | Some(less) => self.buckets.get_mut(&less).expect("the less bucket exists").more = Some(bucket),
      | None => self.first = Some(bucket),
    }
    match more {
      | Some(more) => self.buckets.get_mut(&more).expect("the more bucket exists").less = Some(bucket),
      | None => self.last = Some(bucket),
    }
  }

  fn unlink(&mut self, bucket: u64) {
    let less = self.buckets[&bucket].less;
    let more = self.buckets[&bucket].more;
    match less {
      | Some(less) => self.buckets.get_mut(&less).expect("the less bucket exists").more = more,
      | None => self.first = more,
    }
    match more {
      | Some(more) => self.buckets.get_mut(&more).expect("the more bucket exists").less = less,
      | None => self.last = less,
    }
  }

  fn prepend(&mut self, bucket: u64) {
    let first = self.first;
    self.link(None, bucket, first);
  }

  fn insert_after(&mut self, previous: u64, bucket: u64) {
    let next = self.more_of(previous);
    self.link(Some(previous), bucket, next);
  }

  fn remove(&mut self, bucket: u64) {
    self.unlink(bucket);
    self.buckets.remove(&bucket);
  }

  // Pekko's `getFirstOrElsePrepend`: use the first bucket if it passes the
  // check, otherwise prepend a new one.
  fn first_or_else_prepend(&mut self, priority: Priority) -> u64 {
    match self.first {
      | Some(first) if self.priority_of(first) == priority => first,
      | _ => {
        let bucket = self.allocate(priority);
        self.prepend(bucket);
        bucket
      },
    }
  }

  // Pekko's `getNextOrElseInsert`: use the next bucket if it passes the check,
  // otherwise insert a new one right after `bucket`.
  fn next_or_else_insert(&mut self, bucket: u64, priority: Priority) -> u64 {
    match self.more_of(bucket) {
      | Some(next) if self.priority_of(next) == priority => next,
      | _ => {
        let new = self.allocate(priority);
        self.insert_after(bucket, new);
        new
      },
    }
  }

  // Pekko's `findNextOrElseInsert`: walk forward while the next bucket is
  // `priority < target`, then use the next bucket if it equals the target,
  // otherwise insert a new one right after the last walked bucket.
  fn find_next_or_else_insert(&mut self, bucket: u64, priority: Priority) -> u64 {
    let mut current = bucket;
    while let Some(next) = self.more_of(current) {
      if self.priority_of(next) < priority {
        current = next;
      } else {
        break;
      }
    }
    self.next_or_else_insert(current, priority)
  }
}

impl<A: Ord + Clone> FrequencyList<A> {
  /// Creates an empty list without overall recency.
  pub fn empty(dynamic_aging: bool) -> Self {
    Self::new(dynamic_aging, None)
  }

  /// Creates an empty list. With a clock, the list also keeps the overall
  /// recency across all values.
  pub fn new(dynamic_aging: bool, clock: Option<Arc<dyn Clock>>) -> Self {
    Self {
      buckets: FrequencyBuckets::new(),
      nodes: BTreeMap::new(),
      overall: clock.map(|clock| OverallRecency { clock, by_recency: BTreeMap::new() }),
      aging: dynamic_aging.then(Age::default),
      next_sequence: 0,
    }
  }

  /// Returns the number of values in the list.
  pub fn len(&self) -> usize {
    self.nodes.len()
  }

  /// Returns whether the list holds no values.
  pub fn is_empty(&self) -> bool {
    self.nodes.is_empty()
  }

  /// Returns whether the list holds the given value.
  pub fn contains(&self, value: &A) -> bool {
    self.nodes.contains_key(value)
  }

  /// Iterates the values from the least to the most frequently updated. Values
  /// with the same frequency come in the order they were last updated.
  pub fn least_to_most_frequent(&self) -> impl Iterator<Item = &A> {
    self.forward_sequence().flat_map(|bucket| self.buckets.values_of(bucket).values())
  }

  /// Iterates the values from the most to the least frequently updated. Values
  /// with the same frequency come in the reverse of their last-updated order.
  pub fn most_to_least_frequent(&self) -> impl Iterator<Item = &A> {
    self.backward_sequence().flat_map(|bucket| self.buckets.values_of(bucket).values().rev())
  }

  /// Iterates the values from the least to the most recently updated.
  ///
  /// The iterator is empty when the list was created without a clock.
  pub fn overall_least_to_most_recent(&self) -> impl Iterator<Item = &A> {
    self.overall.iter().flat_map(|overall| overall.by_recency.values().map(|stamped| &stamped.value))
  }

  /// Iterates the values from the most to the least recently updated.
  ///
  /// The iterator is empty when the list was created without a clock.
  pub fn overall_most_to_least_recent(&self) -> impl Iterator<Item = &A> {
    self.overall.iter().flat_map(|overall| overall.by_recency.values().rev().map(|stamped| &stamped.value))
  }

  /// Raises the frequency of the value, or adds it at frequency one. When the
  /// list already holds an equal value, it keeps the value it holds and only
  /// moves it, as Pekko keeps the existing node.
  pub fn update(&mut self, value: A) -> &mut Self {
    match self.nodes.get(&value).copied() {
      | Some(node) => {
        self.increase_frequency(&value, node);
        self.stamp(&value);
      },
      | None => {
        let (bucket, bucket_sequence) = self.add_initial_frequency(value.clone());
        self
          .nodes
          .insert(value.clone(), Node { accesses: Accesses::FIRST, bucket, bucket_sequence, overall_sequence: None });
        self.stamp(&value);
      },
    }
    self
  }

  /// Removes the value if the list holds it.
  pub fn remove(&mut self, value: A) -> &mut Self {
    if let Some(node) = self.nodes.get(&value).copied() {
      self.remove_node(&value, node);
    }
    self
  }

  /// Removes up to `n` of the least frequent values and returns them from the
  /// least frequent on. Under dynamic aging, each removal records the removed
  /// value's frequency as the new age.
  pub fn remove_least_frequent(&mut self, n: usize) -> Vec<A> {
    let victims = self.forward_values(n, None);
    victims.into_iter().map(|value| self.remove_least_frequent_node(value)).collect()
  }

  /// Skips the values equal to `skip`, then removes up to `n` of the next least
  /// frequent values and returns them from the least frequent on.
  ///
  /// Under dynamic aging, removing a value with a higher priority than the one
  /// skipped raises the age, and removing the skipped value later lowers it.
  /// Values updated after that can sit behind values with a higher priority,
  /// as in Pekko (see the type's documentation).
  pub fn remove_least_frequent_skipping(&mut self, n: usize, skip: &A) -> Vec<A> {
    let victims = self.forward_values(n, Some(skip));
    victims.into_iter().map(|value| self.remove_least_frequent_node(value)).collect()
  }

  /// Removes up to `n` of the most frequent values and returns them from the
  /// most frequent on.
  pub fn remove_most_frequent(&mut self, n: usize) -> Vec<A> {
    let victims = self.backward_values(n, None);
    victims.into_iter().filter_map(|value| self.remove_node_by_value(value)).collect()
  }

  /// Skips the values equal to `skip`, then removes up to `n` of the next most
  /// frequent values and returns them from the most frequent on.
  pub fn remove_most_frequent_skipping(&mut self, n: usize, skip: &A) -> Vec<A> {
    let victims = self.backward_values(n, Some(skip));
    victims.into_iter().filter_map(|value| self.remove_node_by_value(value)).collect()
  }

  /// Removes up to `n` of the least recently updated values and returns them
  /// from the least recent on.
  ///
  /// # Panics
  ///
  /// Panics if the list was created without a clock, as Pekko throws when
  /// overall recency is not enabled.
  pub fn remove_overall_least_recent(&mut self, n: usize) -> Vec<A> {
    let victims = self.overall_least_recent_values(n);
    victims.into_iter().filter_map(|value| self.remove_node_by_value(value)).collect()
  }

  /// Removes up to `n` of the most recently updated values and returns them
  /// from the most recent on.
  ///
  /// # Panics
  ///
  /// Panics if the list was created without a clock, as Pekko throws when
  /// overall recency is not enabled.
  pub fn remove_overall_most_recent(&mut self, n: usize) -> Vec<A> {
    let victims = self.overall_most_recent_values(n);
    victims.into_iter().filter_map(|value| self.remove_node_by_value(value)).collect()
  }

  /// Removes the values last updated earlier than `duration` before now and
  /// returns them from the least recent on. A value updated exactly
  /// `duration` before now stays, and a window that reaches before the clock's
  /// epoch keeps every value.
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
    let victims: Vec<A> = overall
      .by_recency
      .values()
      .take_while(|stamped| stamped.timestamp < earliest_kept)
      .map(|stamped| stamped.value.clone())
      .collect();
    victims.into_iter().filter_map(|value| self.remove_node_by_value(value)).collect()
  }

  /// Removes the values last updated later than `duration` before now and
  /// returns them from the most recent on. A value updated exactly `duration`
  /// before now stays.
  ///
  /// # Panics
  ///
  /// Panics if the list was created without a clock, as Pekko throws when
  /// overall recency is not enabled.
  pub fn remove_overall_most_recent_within(&mut self, duration: Duration) -> Vec<A> {
    let Some(overall) = &self.overall else {
      panic!("overall recency is not enabled");
    };
    let latest_kept = overall.clock.monotonic().checked_sub(duration);
    let victims: Vec<A> = overall
      .by_recency
      .values()
      .rev()
      .take_while(|stamped| latest_kept.is_none_or(|latest_kept| stamped.timestamp > latest_kept))
      .map(|stamped| stamped.value.clone())
      .collect();
    victims.into_iter().filter_map(|value| self.remove_node_by_value(value)).collect()
  }

  // Places a new value in its initial bucket and returns the bucket id and the
  // value's placement sequence within it.
  fn add_initial_frequency(&mut self, value: A) -> (u64, u64) {
    let priority = Priority::of(Accesses::FIRST, self.aging);
    let bucket = if self.aging.is_some() {
      match self.buckets.first() {
        | Some(first) if self.buckets.priority_of(first) < priority => {
          self.buckets.find_next_or_else_insert(first, priority)
        },
        | _ => self.buckets.first_or_else_prepend(priority),
      }
    } else {
      self.buckets.first_or_else_prepend(priority)
    };
    let bucket_sequence = self.append_to_bucket(value, bucket);
    (bucket, bucket_sequence)
  }

  // Moves the held value, not `value`, to the next bucket, so that an equal
  // value passed to `update` only serves as the key.
  fn increase_frequency(&mut self, value: &A, node: Node) {
    let accesses = node.accesses.next();
    let priority = Priority::of(accesses, self.aging);
    let bucket = if self.aging.is_some() {
      self.buckets.find_next_or_else_insert(node.bucket, priority)
    } else {
      self.buckets.next_or_else_insert(node.bucket, priority)
    };
    let held_value = self.remove_from_bucket(node.bucket, node.bucket_sequence);
    let bucket_sequence = self.append_to_bucket(held_value, bucket);
    let held = self.nodes.get_mut(value).expect("the value to move is held");
    held.accesses = accesses;
    held.bucket = bucket;
    held.bucket_sequence = bucket_sequence;
  }

  // Appends the value as the most recently placed one in the bucket and returns
  // its placement sequence.
  fn append_to_bucket(&mut self, value: A, bucket: u64) -> u64 {
    let sequence = self.next_sequence;
    self.next_sequence += 1;
    self.buckets.values_of_mut(bucket).insert(sequence, value);
    sequence
  }

  // Takes the value at the placement sequence out of the bucket, and removes
  // the bucket once it is empty.
  fn remove_from_bucket(&mut self, bucket: u64, sequence: u64) -> A {
    let value = self.buckets.values_of_mut(bucket).remove(&sequence).expect("the value sits at its placement sequence");
    if self.buckets.values_of(bucket).is_empty() {
      self.buckets.remove(bucket);
    }
    value
  }

  // Stamps the value as the most recent one overall. A value stamped before is
  // moved with the value it was stamped with, so the held value stays.
  fn stamp(&mut self, value: &A) {
    let Some(overall) = &mut self.overall else {
      return;
    };
    let sequence = self.next_sequence;
    self.next_sequence += 1;
    let timestamp = overall.clock.monotonic();
    let node = self.nodes.get_mut(value).expect("the value to stamp is held");
    let held_value = match node.overall_sequence {
      | Some(previous) => overall.by_recency.remove(&previous).expect("the value sits at its overall sequence").value,
      | None => value.clone(),
    };
    overall.by_recency.insert(sequence, Stamped { value: held_value, timestamp });
    node.overall_sequence = Some(sequence);
  }

  fn remove_node(&mut self, value: &A, node: Node) {
    self.remove_from_bucket(node.bucket, node.bucket_sequence);
    if let Some(overall) = &mut self.overall {
      if let Some(sequence) = node.overall_sequence {
        overall.by_recency.remove(&sequence);
      }
    }
    self.nodes.remove(value);
  }

  fn remove_node_by_value(&mut self, value: A) -> Option<A> {
    let node = self.nodes.get(&value).copied()?;
    self.remove_node(&value, node);
    Some(value)
  }

  fn remove_least_frequent_node(&mut self, value: A) -> A {
    if let Some(node) = self.nodes.get(&value).copied() {
      if let Some(age) = &mut self.aging {
        *age = Age::of_removed(self.buckets.priority_of(node.bucket));
      }
      self.remove_node(&value, node);
    }
    value
  }

  fn forward_sequence(&self) -> impl Iterator<Item = u64> + '_ {
    let mut current = self.buckets.first();
    core::iter::from_fn(move || {
      let bucket = current?;
      current = self.buckets.more_of(bucket);
      Some(bucket)
    })
  }

  fn backward_sequence(&self) -> impl Iterator<Item = u64> + '_ {
    let mut current = self.buckets.last();
    core::iter::from_fn(move || {
      let bucket = current?;
      current = self.buckets.less_of(bucket);
      Some(bucket)
    })
  }

  // The values to remove in the least-to-most frequent order, skipping any
  // value equal to `skip`, limited to `n`. Collected up front so that the age
  // recorded under dynamic aging follows the order of removal Pekko sees when
  // it walks the list while removing.
  fn forward_values(&self, n: usize, skip: Option<&A>) -> Vec<A> {
    let mut victims = Vec::new();
    for bucket in self.forward_sequence() {
      for value in self.buckets.values_of(bucket).values() {
        if skip.is_some_and(|skip| skip == value) {
          continue;
        }
        if victims.len() == n {
          return victims;
        }
        victims.push(value.clone());
      }
    }
    victims
  }

  fn backward_values(&self, n: usize, skip: Option<&A>) -> Vec<A> {
    let mut victims = Vec::new();
    for bucket in self.backward_sequence() {
      for value in self.buckets.values_of(bucket).values().rev() {
        if skip.is_some_and(|skip| skip == value) {
          continue;
        }
        if victims.len() == n {
          return victims;
        }
        victims.push(value.clone());
      }
    }
    victims
  }

  fn overall_least_recent_values(&self, n: usize) -> Vec<A> {
    let Some(overall) = &self.overall else {
      panic!("overall recency is not enabled");
    };
    overall.by_recency.values().take(n).map(|stamped| stamped.value.clone()).collect()
  }

  fn overall_most_recent_values(&self, n: usize) -> Vec<A> {
    let Some(overall) = &self.overall else {
      panic!("overall recency is not enabled");
    };
    overall.by_recency.values().rev().take(n).map(|stamped| stamped.value.clone()).collect()
  }
}
