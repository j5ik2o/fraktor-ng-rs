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

//! An immutable, sorted map specialized for `i32` keys and values.

use alloc::{format, vec::Vec};
use core::fmt;

/// An immutable map from `i32` keys to `i32` values.
///
/// Mirrors `org.apache.pekko.util.ImmutableIntMap`, the internal map Pekko
/// uses where a small map (fewer than about a thousand entries) must avoid
/// boxing its keys and values. Entries are kept as `[key, value]` pairs in a
/// single vector, sorted by key, and every update copies the vector
/// (copy-on-write, with no structural sharing), so a map is never changed
/// after it is built: `updated`, `update_if_absent` and `remove` each return
/// a new map and leave the receiver as it was.
///
/// Lookups are a binary search over the sorted keys. Because the original
/// reports a missing key with a plain `Int`, [`get`](Self::get) returns
/// `i32::MIN` for a key that is not present, so a stored value of `i32::MIN`
/// cannot be told apart from an absent key; use
/// [`contains`](Self::contains) when the difference matters.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct ImmutableIntMap {
  entries: Vec<[i32; 2]>,
}

impl ImmutableIntMap {
  /// Creates an empty map.
  pub fn empty() -> Self {
    Self { entries: Vec::new() }
  }

  /// Returns the number of entries in the map.
  pub fn size(&self) -> usize {
    self.entries.len()
  }

  /// Returns `true` when the map has no entries.
  pub fn is_empty(&self) -> bool {
    self.entries.is_empty()
  }

  /// Returns the value stored for `key`, or `i32::MIN` when the key is not
  /// present. Worst case `O(log n)`, allocation free.
  pub fn get(&self, key: i32) -> i32 {
    match self.entries.binary_search_by_key(&key, |entry| entry[0]) {
      | Ok(index) => self.entries[index][1],
      | Err(_) => i32::MIN,
    }
  }

  /// Returns `true` when `key` is present. Worst case `O(log n)`.
  pub fn contains(&self, key: i32) -> bool {
    self.entries.binary_search_by_key(&key, |entry| entry[0]).is_ok()
  }

  /// Returns a new map with `key` associated to `value`, replacing the old
  /// value when the key was already present. Worst case `O(n)`.
  pub fn updated(&self, key: i32, value: i32) -> Self {
    match self.entries.binary_search_by_key(&key, |entry| entry[0]) {
      | Ok(index) => {
        let mut entries = self.entries.clone();
        entries[index][1] = value;
        Self { entries }
      },
      | Err(index) => {
        let mut entries = self.entries.clone();
        entries.insert(index, [key, value]);
        Self { entries }
      },
    }
  }

  /// Returns a new map with `key` associated to `value` when the key is not
  /// yet present, and a map equal to this one when it is. Worst case `O(n)`.
  ///
  /// The original takes the value by name, so it is not evaluated when the
  /// key is present. This port takes it by value; the difference is only
  /// observable through side effects of computing the value.
  pub fn update_if_absent(&self, key: i32, value: i32) -> Self {
    match self.entries.binary_search_by_key(&key, |entry| entry[0]) {
      | Ok(_) => self.clone(),
      | Err(index) => {
        let mut entries = self.entries.clone();
        entries.insert(index, [key, value]);
        Self { entries }
      },
    }
  }

  /// Returns a new map without `key`, and a map equal to this one when the
  /// key was not present. Worst case `O(n)`.
  pub fn remove(&self, key: i32) -> Self {
    match self.entries.binary_search_by_key(&key, |entry| entry[0]) {
      | Ok(index) => {
        let mut entries = self.entries.clone();
        entries.remove(index);
        Self { entries }
      },
      | Err(_) => self.clone(),
    }
  }

  /// Returns the keys of the map, in ascending order.
  pub fn keys(&self) -> impl Iterator<Item = i32> + '_ {
    self.entries.iter().map(|entry| entry[0])
  }

  /// Returns the `(key, value)` pairs of the map, in ascending key order.
  pub fn iter(&self) -> impl Iterator<Item = (i32, i32)> + '_ {
    self.entries.iter().map(|entry| (entry[0], entry[1]))
  }
}

impl Default for ImmutableIntMap {
  fn default() -> Self {
    Self::empty()
  }
}

impl fmt::Display for ImmutableIntMap {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    let body = self.entries.iter().map(|entry| format!("{} -> {}", entry[0], entry[1])).collect::<Vec<_>>().join(", ");
    write!(formatter, "ImmutableIntMap({body})")
  }
}

impl fmt::Debug for ImmutableIntMap {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    fmt::Display::fmt(self, formatter)
  }
}
