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

//! Unit tests for [`TokenBucket`](super::TokenBucket).
//!
//! These are not ports of Pekko cases. They cover what the integration tests
//! in `tests/token_bucket_spec.rs` cannot see: the private state behind the
//! invariants T1, T2, T3 and T9 of the design
//! (docs/design/issue-188-specs.md §3), the construction and cost errors,
//! and the saturating-add difference from the original (design F1). The
//! property check here is the design's property 2, moved next to the value
//! by the design re-review (N1) because it reads the private fields.

use core::{
  sync::atomic::{AtomicU64, Ordering},
  time::Duration,
};

use super::{TokenBucket, TokenBucketError};
use crate::kernel::Clock;

// A manual clock that can be cloned, so the test keeps a handle to move the
// time while the bucket holds the other clone (design re-review N2). The
// nanoseconds are kept as the bits of an `i64`, so negative times and times
// that wrap over the `i64` range are representable.
#[derive(Clone)]
struct ManualClock(alloc::sync::Arc<AtomicU64>);

impl ManualClock {
  fn new(time: i64) -> Self {
    Self(alloc::sync::Arc::new(AtomicU64::new(time as u64)))
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
fn a_negative_capacity_is_rejected() {
  let clock = ManualClock::new(0);
  assert_eq!(TokenBucket::new(-1, 1, clock).err(), Some(TokenBucketError::NegativeCapacity { capacity: -1 }));
}

#[test]
fn a_non_positive_period_is_rejected() {
  let clock = ManualClock::new(0);
  assert_eq!(
    TokenBucket::new(10, 0, clock.clone()).err(),
    Some(TokenBucketError::NonPositivePeriod { nanos_between_tokens: 0 })
  );
  assert_eq!(
    TokenBucket::new(10, -3, clock).err(),
    Some(TokenBucketError::NonPositivePeriod { nanos_between_tokens: -3 })
  );
}

#[test]
fn a_negative_cost_is_rejected_and_keeps_the_state() {
  let clock = ManualClock::new(0);
  let mut bucket = TokenBucket::new(10, 1, clock).unwrap();
  bucket.init();
  assert_eq!(bucket.offer(-1), Err(TokenBucketError::NegativeCost { cost: -1 }));
  assert_eq!(bucket.available_tokens, 10);
  assert_eq!(bucket.last_update, 0);
}

#[test]
fn the_available_tokens_saturate_instead_of_wrapping() {
  // The input of design F1: with the original's wrapping add, the available
  // tokens would become `i64::MIN + 8` and then a huge positive value above
  // the capacity; with the plain add, debug builds panic.
  let clock = ManualClock::new(0);
  let mut bucket = TokenBucket::new(10, 1, clock.clone()).unwrap();
  bucket.init();
  assert_eq!(bucket.offer(1), Ok(0));
  clock.set(i64::MAX);
  assert_eq!(bucket.offer(0), Ok(0));
  assert_eq!(bucket.available_tokens, 10);
}

#[test]
fn the_state_stays_within_the_invariants_for_random_offers() {
  // Property 2 of the design: after every offer, T1 (the available tokens
  // stay within `0..=capacity`), T2 (`last_update` stays on the arrival grid
  // anchored at the init time), T3 (the delay is never negative) and T9 (a
  // positive delay is the earliest one) hold. The time jumps include jumps
  // to near `i64::MAX`, so the saturating add of F1 is exercised. A single
  // jump stays below `i64::MAX / 2`: an elapsed time of `2^63` or more is
  // indistinguishable from a negative one in wrapping arithmetic, which is
  // the original's documented overflow, outside the invariants.
  const STARTS: [i64; 4] = [0, -100, i64::MIN, i64::MAX - 5];
  for seed in 0..64_u64 {
    let mut random = SplitMix64::new(0x188_0000 + seed);
    let capacity = random.below(20);
    let period = random.below(9) + 1;
    let start = random.pick(&STARTS);
    let clock = ManualClock::new(start);
    let mut bucket = TokenBucket::new(capacity, period, clock.clone()).unwrap();
    bucket.init();
    let mut now = start;
    // The same timeline as `now`, but never wrapped: the grid check below
    // reads `last_update` on this timeline.
    let mut unwrapped_now = start as i128;
    for step in 0..200 {
      // The caller contract (T8): after a delayed offer, the next offer is
      // not made before the delay has elapsed, so the time is first moved
      // up to the grid point the last delayed offer reserved.
      let ahead = bucket.last_update.wrapping_sub(now);
      if (1..i64::MAX / 2).contains(&ahead) {
        now = bucket.last_update;
        unwrapped_now += ahead as i128;
        clock.set(now);
      }
      let jump = random.pick(&[0, 1, period, period * 3, 1_000, i64::MAX / 2]);
      now = now.wrapping_add(jump);
      unwrapped_now += jump as i128;
      clock.set(now);
      let cost = random.below(capacity * 2 + 5);
      let available_before = bucket.available_tokens;
      let last_update_before = bucket.last_update;
      let delay = bucket.offer(cost).unwrap_or_else(|error| {
        panic!("seed={seed}, step={step}: offer({cost}) failed with {error}");
      });
      assert!(
        (0..=capacity).contains(&bucket.available_tokens),
        "seed={seed}, step={step}: T1 broken, available={} capacity={capacity}",
        bucket.available_tokens
      );
      // The grid is anchored at the init time. `last_update` is read on
      // the unwrapped timeline: shift it by the multiple of 2^64 that
      // brings it next to the unwrapped current time, then the offset
      // from the start must be a multiple of the period.
      let wrap = 1_i128 << 64;
      let last = bucket.last_update as i128;
      let turns = (unwrapped_now - last + wrap / 2).div_euclid(wrap);
      let offset = last + turns * wrap - start as i128;
      assert_eq!(offset.rem_euclid(period as i128), 0, "seed={seed}, step={step}: T2 broken, last_update off the grid");
      assert!(delay >= 0, "seed={seed}, step={step}: T3 broken, delay={delay}");
      if delay > 0 {
        // Recompute how many tokens had arrived by `now`, the plain way.
        let elapsed = now.wrapping_sub(last_update_before);
        let arrived = if elapsed >= period { elapsed / period } else { 0 };
        let available_after_arrival = available_before.saturating_add(arrived).min(capacity);
        let remaining = cost as i128 - available_after_arrival as i128;
        assert!(remaining > 0, "seed={seed}, step={step}: positive delay without a shortage");
        assert!(
          (remaining - 1) * (period as i128) < (delay as i128) && (delay as i128) <= remaining * (period as i128),
          "seed={seed}, step={step}: T9 broken, delay={delay} remaining={remaining} period={period}"
        );
      }
    }
  }
}
