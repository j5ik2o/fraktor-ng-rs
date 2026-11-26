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

// Ported from Pekko v2.0.0 (SHA 9fbd2283b1cf701c90a1223bf621942c191db61f):
//   actor-tests/src/test/scala/org/apache/pekko/util/ImmutableIntMapSpec.scala
//
// The original's ten cases under "ImmutableIntMap must" are kept as one test
// each, named after the case with an `immutable_int_map_` prefix, and with
// the original name as a comment. Every update and check runs in the
// original order.
//
// The original's `keysIterator` is the port's `keys()`. Its `toString` is
// the port's `Display`. Its `hashCode` value is `java.util.Arrays.hashCode`,
// which the port does not reproduce; the case only checks that equal maps
// hash equally, which the derived `Hash` satisfies.
//
// The random case seeds `scala.util.Random` with `System.nanoTime` in the
// original. The port uses a fixed-seed SplitMix64 instead, so a failure
// repeats; the failure message names the seed and the iteration. The
// operation mix is the original's `nextInt(3)`: 0 and 1 update, 2 removes.
// The values are the codes of printable characters, as in the original.

use std::collections::BTreeMap;

use fraktor_actor::kernel::immutable_int_map::ImmutableIntMap;

struct SplitMix64 {
  state: u64,
}

impl SplitMix64 {
  fn new(seed: u64) -> Self {
    Self { state: seed }
  }

  fn next_u64(&mut self) -> u64 {
    self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = self.state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
  }

  fn below(&mut self, bound: u64) -> u64 {
    self.next_u64() % bound
  }
}

fn values(map: &ImmutableIntMap) -> Vec<i32> {
  map.keys().map(|key| map.get(key)).collect()
}

#[test]
fn immutable_int_map_has_no_entries_when_empty() {
  // "have no entries when empty"
  let empty = ImmutableIntMap::empty();
  assert_eq!(empty.size(), 0);
  assert_eq!(empty.keys().collect::<Vec<_>>(), Vec::<i32>::new());
}

#[test]
fn immutable_int_map_adds_and_gets_entries() {
  // "add and get entries"
  let m1 = ImmutableIntMap::empty().updated(10, 10);
  assert_eq!(m1.keys().collect::<Vec<_>>(), vec![10]);
  assert_eq!(values(&m1), vec![10]);

  let m2 = m1.updated(20, 20);
  assert_eq!(m2.keys().collect::<Vec<_>>(), vec![10, 20]);
  assert_eq!(values(&m2), vec![10, 20]);

  let m3 = m1.updated(5, 5);
  assert_eq!(m3.keys().collect::<Vec<_>>(), vec![5, 10]);
  assert_eq!(values(&m3), vec![5, 10]);

  let m4 = m2.updated(5, 5);
  assert_eq!(m4.keys().collect::<Vec<_>>(), vec![5, 10, 20]);
  assert_eq!(values(&m4), vec![5, 10, 20]);

  let m5 = m4.updated(15, 15);
  assert_eq!(m5.keys().collect::<Vec<_>>(), vec![5, 10, 15, 20]);
  assert_eq!(values(&m5), vec![5, 10, 15, 20]);
}

#[test]
fn immutable_int_map_replaces_entries() {
  // "replace entries"
  let m1 = ImmutableIntMap::empty().updated(10, 10).updated(10, 11);
  assert_eq!(values(&m1), vec![11]);

  let m2 = m1.updated(20, 20).updated(30, 30).updated(20, 21).updated(30, 31);
  assert_eq!(values(&m2), vec![11, 21, 31]);
}

#[test]
fn immutable_int_map_updates_if_absent() {
  // "update if absent"
  let m1 = ImmutableIntMap::empty().updated(10, 10).updated(20, 11);
  assert_eq!(m1.update_if_absent(10, 15), ImmutableIntMap::empty().updated(10, 10).updated(20, 11));
  assert_eq!(m1.update_if_absent(30, 12), ImmutableIntMap::empty().updated(10, 10).updated(20, 11).updated(30, 12));
}

#[test]
fn immutable_int_map_formats_with_display() {
  // "have toString"
  assert_eq!(ImmutableIntMap::empty().to_string(), "ImmutableIntMap()");
  assert_eq!(ImmutableIntMap::empty().updated(10, 10).to_string(), "ImmutableIntMap(10 -> 10)");
  assert_eq!(
    ImmutableIntMap::empty().updated(10, 10).updated(20, 20).to_string(),
    "ImmutableIntMap(10 -> 10, 20 -> 20)"
  );
}

#[test]
fn immutable_int_map_has_equality_and_consistent_hash() {
  // "have equals and hashCode"
  use std::hash::{Hash, Hasher};

  fn hash_of(map: &ImmutableIntMap) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    map.hash(&mut hasher);
    hasher.finish()
  }

  assert_eq!(ImmutableIntMap::empty().updated(10, 10), ImmutableIntMap::empty().updated(10, 10));
  assert_eq!(hash_of(&ImmutableIntMap::empty().updated(10, 10)), hash_of(&ImmutableIntMap::empty().updated(10, 10)));

  let three = ImmutableIntMap::empty().updated(10, 10).updated(20, 20).updated(30, 30);
  assert_eq!(three, ImmutableIntMap::empty().updated(10, 10).updated(20, 20).updated(30, 30));
  assert_eq!(hash_of(&three), hash_of(&ImmutableIntMap::empty().updated(10, 10).updated(20, 20).updated(30, 30)));

  assert_ne!(ImmutableIntMap::empty().updated(10, 10).updated(20, 20), ImmutableIntMap::empty().updated(10, 10));
  assert_ne!(three, ImmutableIntMap::empty().updated(10, 10).updated(20, 20).updated(30, 31));
  assert_ne!(three, ImmutableIntMap::empty().updated(10, 10).updated(20, 20).updated(31, 30));

  assert_eq!(ImmutableIntMap::empty(), ImmutableIntMap::empty());
  assert_eq!(hash_of(&ImmutableIntMap::empty()), hash_of(&ImmutableIntMap::empty()));
}

#[test]
fn immutable_int_map_removes_entries() {
  // "remove entries"
  let m1 = ImmutableIntMap::empty().updated(10, 10).updated(20, 20).updated(30, 30);

  let m2 = m1.remove(10);
  assert_eq!(values(&m2), vec![20, 30]);

  let m3 = m1.remove(20);
  assert_eq!(values(&m3), vec![10, 30]);

  let m4 = m1.remove(30);
  assert_eq!(values(&m4), vec![10, 20]);

  assert_eq!(m1.remove(5), m1);
  assert_eq!(m1.remove(10).remove(20).remove(30), ImmutableIntMap::empty());
}

#[test]
fn immutable_int_map_get_returns_min_value_for_missing_keys() {
  // "get None when entry doesn't exist" (the original's `get` returns
  // `Int.MinValue` for a missing key; it does not return an `Option`)
  let m1 = ImmutableIntMap::empty().updated(10, 10).updated(20, 20).updated(30, 30);
  assert_eq!(m1.get(5), i32::MIN);
  assert_eq!(m1.get(15), i32::MIN);
  assert_eq!(m1.get(25), i32::MIN);
  assert_eq!(m1.get(35), i32::MIN);
}

#[test]
fn immutable_int_map_contains_keys() {
  // "contain keys"
  let m1 = ImmutableIntMap::empty().updated(10, 10).updated(20, 20).updated(30, 30);
  assert!(m1.contains(10));
  assert!(m1.contains(20));
  assert!(m1.contains(30));
  assert!(!m1.contains(5));
  assert!(!m1.contains(25));
}

#[test]
fn immutable_int_map_matches_the_model_for_random_operations() {
  // "have correct behavior for random operations"
  const SEED: u64 = 0x188_1000;
  let mut random = SplitMix64::new(SEED);
  let mut map = ImmutableIntMap::empty();
  let mut reference = BTreeMap::new();

  for iteration in 1..=1000 {
    let key = random.below(100) as i32;
    // `nextPrintableChar`: a printable character, stored as its code.
    let value = (random.below(0x5F) + 0x20) as i32;
    match random.below(3) {
      | 0 | 1 => {
        map = map.updated(key, value);
        reference.insert(key, value);
      },
      | _ => {
        map = map.remove(key);
        reference.remove(&key);
      },
    }
    let actual: BTreeMap<i32, i32> = map.iter().collect();
    assert_eq!(actual, reference, "seed={SEED}, iteration={iteration}");
  }
}
