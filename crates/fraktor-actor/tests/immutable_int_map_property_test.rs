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

// Property-based tests of the immutable int map (the design of #188,
// docs/design/issue-188-specs.md §3). These generalize the original's
// random case: random operation sequences are run against both the map and
// a `BTreeMap` model, and the two are compared after every step.
//
// These are not ports of Pekko cases and are not counted in the ledger. The
// inputs come from a fixed-seed SplitMix64, so a failure repeats; every
// failure message names the seed and the step. The key draws include a
// narrow range (many collisions) and a wide range with the `i32`
// boundaries, and the values include `i32::MIN`, whose `get` result cannot
// be told apart from an absent key (the model comparison goes through
// `iter`, so the ambiguity does not hide a difference).

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

  fn pick(&mut self, items: &[i32]) -> i32 {
    items[(self.next_u64() % items.len() as u64) as usize]
  }
}

const BOUNDARY_KEYS: [i32; 8] = [i32::MIN, i32::MIN + 1, -100, -1, 0, 1, 100, i32::MAX];
const VALUES: [i32; 6] = [i32::MIN, -1, 0, 1, 42, i32::MAX];

#[test]
fn random_operations_match_the_btree_model() {
  for seed in 0..32_u64 {
    let mut random = SplitMix64::new(0x188_2000 + seed);
    let wide = seed % 2 == 1;
    let mut map = ImmutableIntMap::empty();
    let mut model: BTreeMap<i32, i32> = BTreeMap::new();

    for step in 0..500 {
      let key = if wide && random.below(3) == 0 { random.pick(&BOUNDARY_KEYS) } else { random.below(100) as i32 - 50 };
      // The receiver of the operation is unchanged (the design's M2): the
      // map that was the receiver is compared, after the call, against the
      // model as it was before the call.
      let model_before = model.clone();
      let receiver = std::mem::replace(&mut map, ImmutableIntMap::empty());
      map = match random.below(3) {
        | 0 => {
          let value = random.pick(&VALUES);
          model.insert(key, value);
          receiver.updated(key, value)
        },
        | 1 => {
          model.remove(&key);
          receiver.remove(key)
        },
        | _ => {
          let value = random.pick(&VALUES);
          model.entry(key).or_insert(value);
          receiver.update_if_absent(key, value)
        },
      };
      assert_eq!(
        receiver.iter().collect::<BTreeMap<_, _>>(),
        model_before,
        "seed={seed}, step={step}: receiver changed"
      );
      assert_eq!(map.size(), model.len(), "seed={seed}, step={step}: size");
      assert_eq!(map.iter().collect::<BTreeMap<_, _>>(), model, "seed={seed}, step={step}: contents");
      assert_eq!(
        map.keys().collect::<Vec<_>>(),
        model.keys().copied().collect::<Vec<_>>(),
        "seed={seed}, step={step}: keys"
      );
      for probe in [key, i32::MIN, 0, i32::MAX] {
        assert_eq!(map.contains(probe), model.contains_key(&probe), "seed={seed}, step={step}: contains({probe})");
        if let Some(expected) = model.get(&probe) {
          assert_eq!(map.get(probe), *expected, "seed={seed}, step={step}: get({probe})");
        } else {
          assert_eq!(map.get(probe), i32::MIN, "seed={seed}, step={step}: get({probe}) for an absent key");
        }
      }
    }
  }
}
