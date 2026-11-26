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
// Copyright 2015 Ben Manes. All Rights Reserved.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.

//! A count-min sketch with four rows of four-bit counters.

use core::marker::PhantomData;

use super::{
  counter_bits::CounterBits, counter_matrix::CounterMatrix, reset_size::ResetSize, sketch_depth::SketchDepth,
  sketch_width::SketchWidth, FastFrequencySketchSettings, FrequencySketchError, FrequencySketchHasher, StringHashCode,
};

#[cfg(test)]
#[path = "fast_frequency_sketch_test.rs"]
mod tests;

// One seed per row, large primes between 2^63 and 2^64.
const SEEDS: [i64; SketchDepth::FOUR.rows()] = [
  0xC3A5_C85C_97CB_3127_u64 as i64,
  0xB492_B66F_BE98_F273_u64 as i64,
  0x9AE1_6A3B_2F90_404F_u64 as i64,
  0xCBF2_9CE4_8422_2325_u64 as i64,
];

/// Estimates how often values were seen, for a TinyLFU cache admission
/// policy, with four rows of four-bit counters.
///
/// Mirrors `FastFrequencySketch` in `org.apache.pekko.util`, which fixes the
/// shape of [`FrequencySketch`](super::FrequencySketch) so it can use
/// constants, and places values with its own rehash and one seed per row. A
/// value's frequency is the smallest of its four counters, each counter stops
/// at fifteen, and every counter is halved after the reset size of
/// increments.
///
/// The original hashes values with their `hashCode`. The default hasher,
/// [`StringHashCode`], is that hash for strings, so values land in the same
/// counters as on the JVM.
pub struct FastFrequencySketch<A: ?Sized, H = StringHashCode> {
  counters: CounterMatrix,
  index_mask: i32,
  hasher: H,
  values: PhantomData<fn(&A)>,
}

impl<A: ?Sized, H: FrequencySketchHasher<A> + Default> FastFrequencySketch<A, H> {
  /// Creates a sketch for a cache of `capacity` values, with the default
  /// hasher.
  ///
  /// The width is `settings.width_multiplier` times `capacity` rounded up to a
  /// power of two, and the reset size is `settings.reset_multiplier` times
  /// `capacity`, capped at `i32::MAX`.
  ///
  /// A capacity of zero or below is accepted, as the original accepts it: it
  /// rounds up to one, so the width is `settings.width_multiplier`. The reset
  /// size is the product truncated to `i32` as the original truncates it
  /// (saturated to `i64`, capped at `i32::MAX`, then its low 32 bits), and the
  /// reset is enabled only when that truncated size is positive. A capacity of
  /// zero gives zero, which disables the reset. A negative capacity can give a
  /// positive size: with a reset multiplier of `-2.0` the product is `2`, and
  /// with `4_294_967_295.0` the product is `-4_294_967_295`, whose low 32 bits
  /// are `1`, so the reset runs on every update. `i32::MIN` alone rounds up to
  /// `i32::MIN`, so with a positive width multiplier it gives
  /// [`FrequencySketchError::WidthNotPositive`].
  ///
  /// # Errors
  ///
  /// Returns [`FrequencySketchError`] when the width overflows `i32` or is not
  /// a positive power of two.
  pub fn new(capacity: i32, settings: FastFrequencySketchSettings) -> Result<Self, FrequencySketchError> {
    Self::with_hasher(capacity, settings, H::default())
  }
}

impl<A: ?Sized, H: FrequencySketchHasher<A>> FastFrequencySketch<A, H> {
  /// Creates a sketch as [`FastFrequencySketch::new`] does, with the given
  /// hasher.
  ///
  /// # Errors
  ///
  /// Returns [`FrequencySketchError`] for the settings
  /// [`FastFrequencySketch::new`] rejects.
  pub fn with_hasher(
    capacity: i32,
    settings: FastFrequencySketchSettings,
    hasher: H,
  ) -> Result<Self, FrequencySketchError> {
    let width = SketchWidth::of(capacity, settings.width_multiplier)?;
    let reset_size = ResetSize::of(capacity, settings.reset_multiplier);
    Ok(Self {
      counters: CounterMatrix::new(SketchDepth::FOUR, width, CounterBits::FOUR, reset_size),
      index_mask: width.index_mask(),
      hasher,
      values: PhantomData,
    })
  }

  /// Returns the number of increments that changed a counter, after the
  /// halving of the last reset.
  #[must_use]
  pub const fn size(&self) -> i32 {
    self.counters.size()
  }

  /// Returns the estimated frequency of `value`: the smallest of its four
  /// counters.
  #[must_use]
  pub fn frequency(&self, value: &A) -> i32 {
    let hash = Self::rehash(self.hasher.hash(value));
    SEEDS
      .iter()
      .enumerate()
      .map(|(row, seed)| self.counters.counter(row, self.index(hash, *seed)))
      .min()
      .unwrap_or(i32::MAX)
  }

  /// Increments the counters of `value` that are below fifteen, and halves
  /// every counter when the size reaches the reset size.
  pub fn increment(&mut self, value: &A) {
    let hash = Self::rehash(self.hasher.hash(value));
    let mut updated = false;
    for (row, seed) in SEEDS.iter().enumerate() {
      let index = self.index(hash, *seed);
      updated |= self.counters.increment(row, index);
    }
    if updated {
      self.counters.record_update();
    }
  }

  // A low-bias hash function found by Hash Function Prospector
  // https://github.com/skeeto/hash-prospector
  // hash-prospector has been placed in public domain - https://github.com/skeeto/hash-prospector/blob/master/UNLICENSE
  const fn rehash(hash: i32) -> i32 {
    let mut x = hash;
    x = (((x as u32) >> 15) as i32 ^ x).wrapping_mul(0xD168_AAAD_u32 as i32);
    x = (((x as u32) >> 15) as i32 ^ x).wrapping_mul(0xAF72_3597_u32 as i32);
    ((x as u32) >> 15) as i32 ^ x
  }

  // The original's `index`: `(hash + seed) * seed` in wrapping `Long`
  // arithmetic, folded to 32 bits and masked to the width.
  const fn index(&self, hash: i32, seed: i64) -> i32 {
    let x = (hash as i64).wrapping_add(seed).wrapping_mul(seed);
    (x.wrapping_add(((x as u64) >> 32) as i64) as i32) & self.index_mask
  }
}
