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

// Property-based tests of `core::time::Duration` arithmetic (the design of
// #185, docs/design/issue-185-util-specs.md §5, P1 to P4).
//
// These are not ports of Pekko cases and are not counted in the ledger. The
// inputs come from a fixed-seed SplitMix64, so a failure repeats; every
// failure message names the seed. The input mix is deliberate: boundaries
// (`ZERO`, `MAX`, nanoseconds of 999_999_999, values that carry over a
// second), small values and large values. A uniform draw of `u64` seconds
// would make `checked_mul` fail almost always and let the properties pass
// on almost no valid trials, so each property counts its valid trials and
// fails if they fall below a minimum set per property (design §5, N3):
// P1 requires half of its trials, P2 three quarters (its factors are drawn
// mostly from 0..=16), and P3 and P4 use every trial.

use core::time::Duration;

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

  fn nanos(&mut self) -> u32 {
    (self.next_u64() % 1_000_000_000) as u32
  }
}

fn boundary_duration(random: &mut SplitMix64) -> Duration {
  const BOUNDARIES: [Duration; 6] = [
    Duration::ZERO,
    Duration::MAX,
    Duration::new(0, 999_999_999),
    Duration::new(1, 999_999_999),
    Duration::from_nanos(1),
    Duration::new(u64::MAX, 999_999_999),
  ];
  BOUNDARIES[(random.next_u64() % BOUNDARIES.len() as u64) as usize]
}

fn small_duration(random: &mut SplitMix64) -> Duration {
  Duration::new(random.below(10_000), random.nanos())
}

fn medium_duration(random: &mut SplitMix64) -> Duration {
  Duration::new(random.below(1_000_000_000_000), random.nanos())
}

fn small_or_medium_duration(random: &mut SplitMix64) -> Duration {
  match random.below(2) {
    | 0 => small_duration(random),
    | _ => medium_duration(random),
  }
}

fn any_duration(random: &mut SplitMix64) -> Duration {
  match random.below(4) {
    | 0 => boundary_duration(random),
    | 1 => small_duration(random),
    | 2 => medium_duration(random),
    | _ => Duration::new(u64::MAX - random.below(1_000_000), random.nanos()),
  }
}

const TRIALS: usize = 4_000;

#[test]
fn addition_of_in_range_durations_is_commutative_and_subtraction_undoes_it() {
  let seed = 0x1851_u64;
  let mut random = SplitMix64::new(seed);
  let mut valid = 0_usize;
  for _ in 0..TRIALS {
    let a = small_or_medium_duration(&mut random);
    let b = small_or_medium_duration(&mut random);
    if let Some(sum) = a.checked_add(b) {
      valid += 1;
      assert_eq!(sum, b.checked_add(a).expect("the mirrored sum is in range"), "seed {seed:#x}: {a:?} + {b:?}");
      assert_eq!(sum - b, a, "seed {seed:#x}: ({a:?} + {b:?}) - {b:?}");
    }
  }
  assert!(valid * 2 >= TRIALS, "seed {seed:#x}: only {valid} of {TRIALS} trials were in range");
}

#[test]
fn scalar_multiplication_of_in_range_durations_divides_back() {
  let seed = 0x1852_u64;
  let mut random = SplitMix64::new(seed);
  let mut valid = 0_usize;
  for _ in 0..TRIALS {
    let d = small_or_medium_duration(&mut random);
    // Factors come mostly from a small range, so most trials stay in range.
    let n = match random.below(8) {
      | 0 => random.next_u64() as u32,
      | _ => random.below(17) as u32,
    };
    assert_eq!(d.checked_mul(0), Some(Duration::ZERO), "seed {seed:#x}: {d:?} * 0");
    if let Some(product) = d.checked_mul(n) {
      valid += 1;
      if n >= 1 {
        assert_eq!(product / n, d, "seed {seed:#x}: ({d:?} * {n}) / {n}");
      }
    }
  }
  assert!(valid * 4 >= TRIALS * 3, "seed {seed:#x}: only {valid} of {TRIALS} trials were in range");
}

#[test]
fn zero_is_an_identity_and_a_duration_divided_by_itself_is_one() {
  let seed = 0x1853_u64;
  let mut random = SplitMix64::new(seed);
  for _ in 0..TRIALS {
    let d = any_duration(&mut random);
    assert_eq!(d + Duration::ZERO, d, "seed {seed:#x}: {d:?} + ZERO");
    assert_eq!(d - Duration::ZERO, d, "seed {seed:#x}: {d:?} - ZERO");
    if !d.is_zero() {
      assert_eq!(d.div_duration_f64(d), 1.0, "seed {seed:#x}: {d:?} / itself");
      // The ratio of a doubling is only exact while the doubled total in
      // nanoseconds is at most 2^53 (design §3, I3).
      if d.as_nanos() * 2 <= (1_u128 << 53) {
        let doubled = d * 2;
        assert_eq!(doubled.div_duration_f64(d), 2.0, "seed {seed:#x}: (2 * {d:?}) / {d:?}");
      }
    }
  }
}

#[test]
fn inputs_outside_the_checked_range_keep_their_properties() {
  // P4 of the design: the inputs P1 to P3 set aside still have properties.
  assert!(Duration::ZERO.div_duration_f64(Duration::ZERO).is_nan());
  let seed = 0x1854_u64;
  let mut random = SplitMix64::new(seed);
  for _ in 0..TRIALS {
    let a = any_duration(&mut random);
    let b = any_duration(&mut random);
    assert_eq!(a.checked_add(b).is_none(), b > Duration::MAX - a, "seed {seed:#x}: checked_add({a:?}, {b:?})");
    // The same draws near the limit that do succeed are undone by the
    // inverse operation, which P1 and P2 do not generate.
    if let Some(sum) = a.checked_add(b) {
      assert_eq!(sum - b, a, "seed {seed:#x}: ({a:?} + {b:?}) - {b:?}");
    }
    let n = random.next_u64() as u32;
    if a.checked_mul(n).is_none() {
      assert_eq!(a.saturating_mul(n), Duration::MAX, "seed {seed:#x}: saturating_mul({a:?}, {n})");
    } else if n >= 1 {
      assert_eq!(a.checked_mul(n).expect("checked above") / n, a, "seed {seed:#x}: ({a:?} * {n}) / {n}");
    }
  }
}
