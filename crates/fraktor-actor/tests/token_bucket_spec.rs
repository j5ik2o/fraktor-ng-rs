// SPDX-License-Identifier: Apache-2.0
//
// Licensed to the Apache Software Foundation (ASF) under one or more
// license agreements; and to You under the Apache License, version 2.0:
//
//   https://www.apache.org/licenses/LICENSE-2.0
//
// This file is part of the Apache Pekko project, which was derived from Akka.
//
// Copyright (C) 2015-2022 Lightbend Inc. <https://www.lightbend.com>
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.

// Ported from Pekko v2.0.0 (SHA 9fbd2283b1cf701c90a1223bf621942c191db61f):
//   actor-tests/src/test/scala/org/apache/pekko/util/TokenBucketSpec.scala
//
// The original's twelve cases under "A Token Bucket must" are kept as one
// test each, named after the case with a `token_bucket_` prefix, and with
// the original name as a comment. Every offer and check runs in the
// original order.
//
// The original extends `PekkoSpec`, but no case uses a TestKit feature; the
// only fixture is the `TestBucket`, a bucket whose `currentTime` the test
// sets by hand. The port's `TestBucket` holds a `TokenBucket` over a
// manual clock that implements the kernel's `Clock`. The clock is cloneable
// (design re-review N2): one clone goes into the bucket, the other stays
// with the test to move the time. Times are kept as the bits of an `i64`,
// so negative times and times that wrap over are set exactly as the
// original sets them, and the time arithmetic in the test wraps, as the
// original's `Long` arithmetic does.
//
// The original's `require` and cost check throw `IllegalArgumentException`.
// The port's constructor and `offer` return `Err(TokenBucketError::..)`
// instead; no case here passes an invalid input, so the cases unwrap.

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

  fn advance(&self, delta: i64) {
    self.0.fetch_add(delta as u64, Ordering::SeqCst);
  }
}

impl Clock for ManualClock {
  fn monotonic(&self) -> Duration {
    Duration::from_nanos(self.0.load(Ordering::SeqCst))
  }
}

struct TestBucket {
  clock: ManualClock,
  bucket: TokenBucket<ManualClock>,
}

impl TestBucket {
  fn new(capacity: i64, period: i64) -> Self {
    let clock = ManualClock::new(0);
    let bucket = TokenBucket::new(capacity, period, clock.clone()).expect("the case's parameters are valid");
    Self { clock, bucket }
  }

  fn init(&mut self) {
    self.bucket.init();
  }

  fn offer(&mut self, cost: i64) -> i64 {
    self.bucket.offer(cost).expect("the case's costs are valid")
  }

  fn set_time(&self, time: i64) {
    self.clock.set(time);
  }

  fn advance_time(&self, delta: i64) {
    self.clock.advance(delta);
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
}

#[test]
fn token_bucket_starts_full() {
  // "start full"
  let mut bucket = TestBucket::new(10, 1);
  bucket.init();

  assert_eq!(bucket.offer(1), 0);
  assert_eq!(bucket.offer(1), 0);
  assert_eq!(bucket.offer(1), 0);
  assert_eq!(bucket.offer(7), 0);

  assert_eq!(bucket.offer(3), 3);
}

#[test]
fn token_bucket_calculates_with_different_rates_and_capacities() {
  // "calculate correctly with different rates and capacities"
  let mut bucket_rate2 = TestBucket::new(10, 2);
  bucket_rate2.init();

  assert_eq!(bucket_rate2.offer(5), 0);
  assert_eq!(bucket_rate2.offer(5), 0);
  assert_eq!(bucket_rate2.offer(5), 10);

  let mut bucket_rate3 = TestBucket::new(8, 3);
  bucket_rate3.init();
  assert_eq!(bucket_rate3.offer(5), 0);
  assert_eq!(bucket_rate3.offer(5), 6);

  bucket_rate3.set_time(6);
  assert_eq!(bucket_rate3.offer(3), 9);
}

#[test]
fn token_bucket_allows_cost_larger_than_capacity() {
  // "allow sending elements larger than capacity"
  let mut bucket = TestBucket::new(10, 2);
  bucket.init();

  assert_eq!(bucket.offer(5), 0);
  assert_eq!(bucket.offer(20), 30);

  bucket.set_time(30);
  assert_eq!(bucket.offer(1), 2);

  bucket.set_time(34);
  assert_eq!(bucket.offer(1), 0);
  assert_eq!(bucket.offer(1), 2);
}

#[test]
fn token_bucket_works_with_zero_capacity() {
  // "work with zero capacity"
  let mut bucket = TestBucket::new(0, 2);
  bucket.init();

  assert_eq!(bucket.offer(10), 20);

  bucket.set_time(40);
  assert_eq!(bucket.offer(10), 20);
}

#[test]
fn token_bucket_does_not_delay_when_rate_exceeds_production() {
  // "not delay if rate is higher than production"
  let mut bucket = TestBucket::new(1, 10);
  bucket.init();

  for time in (0..=100).step_by(10) {
    bucket.set_time(time);
    assert_eq!(bucket.offer(1), 0);
  }
}

#[test]
fn token_bucket_maintains_maximum_capacity() {
  // "maintain maximum capacity"
  let mut bucket = TestBucket::new(10, 1);
  bucket.init();
  assert_eq!(bucket.offer(10), 0);

  bucket.set_time(100_000);
  assert_eq!(bucket.offer(20), 10);
}

#[test]
fn token_bucket_works_with_negative_current_time() {
  // "work if currentTime is negative"
  let mut bucket = TestBucket::new(10, 1);
  bucket.set_time(-100); // Must be set before init()!
  bucket.init();

  assert_eq!(bucket.offer(5), 0);
  assert_eq!(bucket.offer(10), 5);

  bucket.advance_time(10);

  assert_eq!(bucket.offer(5), 0);
}

#[test]
fn token_bucket_works_when_current_time_wraps_over() {
  // "work if currentTime wraps over"
  let mut bucket = TestBucket::new(10, 1);
  bucket.set_time(i64::MAX - 5); // Must be set before init()!
  bucket.init();

  assert_eq!(bucket.offer(5), 0);
  assert_eq!(bucket.offer(10), 5);

  bucket.advance_time(10);

  assert_eq!(bucket.offer(5), 0);
}

#[test]
fn token_bucket_keeps_equal_time_between_token_renewal_intervals() {
  // "(attempt to) maintain equal time between token renewal intervals"
  let mut bucket = TestBucket::new(5, 3);
  bucket.init();

  assert_eq!(bucket.offer(10), 15);

  bucket.set_time(16);
  // At this point there is no token in the bucket (we consumed it at T15)
  // but the next token will arrive at T18! A naive calculation would
  // consider that there is 1 token needed hence we need to wait 3 units,
  // but in fact we only need to wait 2 units, otherwise we shift the whole
  // token arrival sequence resulting in lower rates.
  assert_eq!(bucket.offer(1), 2);

  bucket.set_time(19);
  // At 18 bucket is empty, and so is at 19. For a cost of 2 we need to wait
  // until T24 which is 5 units.
  assert_eq!(bucket.offer(2), 5);

  // Another case
  let mut bucket2 = TestBucket::new(10, 3);
  bucket2.init();

  bucket2.set_time(4);
  assert_eq!(bucket2.offer(6), 0);

  // 4 tokens remain and new tokens arrive at T6 and T9 so here we have 6
  // tokens remaining. We need 1 more, which will arrive at T12.
  bucket2.set_time(10);
  assert_eq!(bucket2.offer(7), 2);
}

#[test]
fn token_bucket_works_with_zero_cost() {
  // "work with cost of zero"
  let mut bucket = TestBucket::new(10, 1);
  bucket.init();

  // Can be called any number of times
  bucket.offer(0);
  bucket.offer(0);
  bucket.offer(0);

  assert_eq!(bucket.offer(10), 0);

  // Bucket is empty now. Still can be called any number of times.
  bucket.offer(0);
  bucket.offer(0);
  bucket.offer(0);
}

#[test]
fn token_bucket_works_with_very_slow_rates() {
  // "work with very slow rates"
  let t = i64::MAX >> 10;
  let mut bucket = TestBucket::new(10, t);
  bucket.init();

  assert_eq!(bucket.offer(20), 10 * t);
  bucket.advance_time(10 * t);

  // Collect 5 tokens
  bucket.advance_time(5 * t);

  assert_eq!(bucket.offer(4), 0);
  assert_eq!(bucket.offer(2), t);
}

#[test]
fn token_bucket_matches_the_ideal_bucket_with_perfect_timing() {
  // "behave exactly as the ideal (naive) token bucket if offer is called
  // with perfect timing"
  //
  // The original draws from an unseeded `scala.util.Random`; the port uses
  // a fixed-seed SplitMix64, so a failure repeats, and the failure message
  // names the combination. The update of the remaining steps until the
  // next element copies the original's formula literally,
  // `untilNextElement = time + Random.nextInt(arrivalPeriod)`, including
  // that it adds the absolute time to a relative count (design F6).
  const SEED: u64 = 0x188_1200;
  let mut random = SplitMix64::new(SEED);

  for capacity in [0_i64, 1, 5, 10] {
    for period in [1_i64, 3, 5] {
      for arrival_period in [1_i64, 3, 5] {
        for start_time in [i64::MIN, -1, 0, i64::MAX] {
          for max_cost in [1_i64, 5, 10] {
            let context = format!(
              "seed={SEED}, capacity={capacity}, period={period}, arrivalPeriod={arrival_period}, \
               startTime={start_time}, maxCost={max_cost}"
            );
            let mut bucket = TestBucket::new(capacity, period);
            bucket.set_time(start_time);
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
                // The actual emit time should equal what the optimized
                // token bucket calculates.
                assert_eq!(time, next_emit, "{context}");
                until_next_element = time + random.below(arrival_period);
                delaying = false;
              }

              if until_next_element == 0 {
                // Allow cost of zero.
                let cost = random.below(max_cost + 1);
                ideal_bucket -= cost; // This can go negative.
                bucket.set_time(start_time.wrapping_add(time));
                let delay = bucket.offer(cost);
                next_emit = time + delay;
                if delay == 0 {
                  assert!(ideal_bucket >= 0, "{context}");
                  until_next_element = time + random.below(arrival_period);
                } else {
                  delaying = true;
                }
              }

              until_next_tick -= 1;
              until_next_element -= 1;
            }
          }
        }
      }
    }
  }
}
