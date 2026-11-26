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

// Property-based tests of the token bucket (the design of #188,
// docs/design/issue-188-specs.md §3, property 1). The bucket is compared
// against a naive ideal bucket — one token per tick, capped at the
// capacity — over random combinations of capacity, period, start time and
// arrival pattern, following the shape of the original's last case but
// drawing the parameters instead of enumerating them. The relative
// arrival formula used here differs from the original case's literal one,
// as the design records (F6).
//
// The design's property 2 (the invariants T1, T2, T3 and T9) reads the
// bucket's private state, so it is a unit test next to the value:
// src/kernel/token_bucket/token_bucket_value_test.rs (design re-review
// N1). These tests here are not ports of Pekko cases and are not counted
// in the ledger. The inputs come from a fixed-seed SplitMix64, so a
// failure repeats; every failure message names the seed.

use std::{
  sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
  },
  time::Duration,
};

use fraktor_actor::kernel::{token_bucket::TokenBucket, Clock};

#[derive(Clone)]
struct ManualClock(Arc<AtomicU64>);

impl ManualClock {
  fn new(time: i64) -> Self {
    Self(Arc::new(AtomicU64::new(time as u64)))
  }

  fn set(&self, time: i64) {
    self.0.store(time as u64, Ordering::SeqCst);
  }
}

impl Clock for ManualClock {
  fn monotonic(&self) -> Duration {
    Duration::from_nanos(self.0.load(Ordering::SeqCst))
  }
}

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

  fn below(&mut self, bound: i64) -> i64 {
    (self.next_u64() % bound as u64) as i64
  }

  fn pick(&mut self, items: &[i64]) -> i64 {
    items[(self.next_u64() % items.len() as u64) as usize]
  }
}

#[test]
fn random_parameters_match_the_ideal_bucket() {
  const STARTS: [i64; 5] = [i64::MIN, -100, -1, 0, i64::MAX - 10_000];
  for seed in 0..48_u64 {
    let mut random = SplitMix64::new(0x188_3000 + seed);
    let capacity = random.pick(&[0, 1, 3, 5, 10, 25]);
    let period = random.pick(&[1, 2, 3, 5, 7]);
    let arrival_period = random.pick(&[1, 3, 5]);
    let max_cost = random.pick(&[1, 5, 10]);
    let start_time = random.pick(&STARTS);
    let context = format!(
      "seed={seed}, capacity={capacity}, period={period}, arrivalPeriod={arrival_period}, \
       startTime={start_time}, maxCost={max_cost}"
    );

    let clock = ManualClock::new(start_time);
    let mut bucket = TokenBucket::new(capacity, period, clock.clone()).expect("the drawn parameters are valid");
    bucket.init();

    let mut ideal_bucket = capacity;
    let mut until_next_tick = period;
    let mut until_next_element = random.below(arrival_period) + 1;
    let mut next_emit = 0_i64;
    let mut delaying = false;

    for time in 0..=1000_i64 {
      if until_next_tick == 0 {
        until_next_tick = period;
        ideal_bucket = (ideal_bucket + 1).min(capacity);
      }

      if delaying && ideal_bucket == 0 {
        assert_eq!(time, next_emit, "{context}");
        until_next_element = random.below(arrival_period) + 1;
        delaying = false;
      }

      if until_next_element == 0 {
        let cost = random.below(max_cost + 1);
        ideal_bucket -= cost;
        clock.set(start_time.wrapping_add(time));
        let delay = bucket.offer(cost).expect("the drawn costs are valid");
        next_emit = time + delay;
        if delay == 0 {
          assert!(ideal_bucket >= 0, "{context}");
          until_next_element = random.below(arrival_period) + 1;
        } else {
          delaying = true;
        }
      }

      until_next_tick -= 1;
      until_next_element -= 1;
    }
  }
}
