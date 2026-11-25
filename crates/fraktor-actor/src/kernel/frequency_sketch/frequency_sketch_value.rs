//! A count-min sketch with a configurable depth and counter size.

use core::marker::PhantomData;

use super::{
  counter_bits::CounterBits, counter_matrix::CounterMatrix, murmur3::Murmur3, reset_size::ResetSize,
  sketch_width::SketchWidth, FrequencySketchError, FrequencySketchHasher, FrequencySketchSettings, StringHasher,
};

#[cfg(test)]
#[path = "frequency_sketch_value_test.rs"]
mod tests;

/// Estimates how often values were seen, for a TinyLFU cache admission
/// policy, with a configurable depth and counter size.
///
/// Mirrors `FrequencySketch` in `org.apache.pekko.util`. The sketch keeps
/// `depth` rows of counters, one hash function per row, and estimates a
/// value's frequency as the smallest of its counters. The row hashes are
/// `h1 + row * h2`, where `h1` is the value's hash and `h2` rehashes `h1` with
/// MurmurHash3. Counters stop at their maximum. After the reset size of
/// increments, every counter is halved, so old counts age out.
///
/// The sizes and counts are `i32` and wrap as the original's `Int` does, so a
/// value lands in the same counters and the estimates match the JVM for the
/// same hash. The default hasher, [`StringHasher`], hashes strings as the
/// original does.
pub struct FrequencySketch<A: ?Sized, H = StringHasher> {
  counters: CounterMatrix,
  hasher: H,
  values: PhantomData<fn(&A)>,
}

impl<A: ?Sized, H: FrequencySketchHasher<A> + Default> FrequencySketch<A, H> {
  /// Creates a sketch for a cache of `capacity` values, with the default
  /// hasher.
  ///
  /// The width is `settings.width_multiplier` times `capacity` rounded up to a
  /// power of two, and the reset size is `settings.reset_multiplier` times
  /// `capacity`, capped at `i32::MAX`.
  ///
  /// # Errors
  ///
  /// Returns [`FrequencySketchError`] when the width overflows `i32`, is not a
  /// positive power of two, when the counter size is not 2, 4, 8, 16, 32 or 64
  /// bits, or when the depth is negative.
  pub fn new(capacity: i32, settings: FrequencySketchSettings) -> Result<Self, FrequencySketchError> {
    Self::with_hasher(capacity, settings, H::default())
  }
}

impl<A: ?Sized, H: FrequencySketchHasher<A>> FrequencySketch<A, H> {
  /// Creates a sketch as [`FrequencySketch::new`] does, with the given
  /// hasher.
  ///
  /// # Errors
  ///
  /// Returns [`FrequencySketchError`] for the settings
  /// [`FrequencySketch::new`] rejects.
  pub fn with_hasher(
    capacity: i32,
    settings: FrequencySketchSettings,
    hasher: H,
  ) -> Result<Self, FrequencySketchError> {
    let width = SketchWidth::of(capacity, settings.width_multiplier)?;
    let reset_size = ResetSize::of(capacity, settings.reset_multiplier);
    let counter_bits = CounterBits::parse(settings.counter_bits)?;
    let depth =
      usize::try_from(settings.depth).map_err(|_| FrequencySketchError::NegativeDepth { depth: settings.depth })?;
    Ok(Self { counters: CounterMatrix::new(depth, width, counter_bits, reset_size), hasher, values: PhantomData })
  }

  /// Returns the number of increments that changed a counter, after the
  /// halving of the last reset.
  #[must_use]
  pub const fn size(&self) -> i32 {
    self.counters.size()
  }

  /// Returns the estimated frequency of `value`: the smallest of its
  /// counters, which is `i32::MAX` for a sketch without rows.
  #[must_use]
  pub fn frequency(&self, value: &A) -> i32 {
    let hash = self.hasher.hash(value);
    let rehash = Self::rehash(hash);
    (0..self.counters.depth())
      .fold(i32::MAX, |min_count, row| min_count.min(self.counters.counter(row, Self::row_hash(hash, rehash, row))))
  }

  /// Increments the counters of `value` that are below their maximum, and
  /// halves every counter when the size reaches the reset size.
  pub fn increment(&mut self, value: &A) {
    let hash = self.hasher.hash(value);
    let rehash = Self::rehash(hash);
    let mut updated = false;
    for row in 0..self.counters.depth() {
      updated |= self.counters.increment(row, Self::row_hash(hash, rehash, row));
    }
    if updated {
      self.counters.record_update();
    }
  }

  // The original's `MurmurHash3.finalizeHash(MurmurHash3.mixLast(hash, hash), 2)`.
  const fn rehash(hash: i32) -> i32 {
    Murmur3::new(hash).mix_last(hash).finalize(2)
  }

  // The original's `hash1 + row * hash2`, in wrapping `Int` arithmetic.
  const fn row_hash(hash: i32, rehash: i32, row: usize) -> i32 {
    hash.wrapping_add((row as i32).wrapping_mul(rehash))
  }
}
