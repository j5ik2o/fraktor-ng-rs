//! The seeded MurmurHash3 string hash of the configurable sketch.

use super::{hasher_seal::HasherSeal, murmur3::Murmur3, FrequencySketchHasher};

#[cfg(test)]
#[path = "string_hasher_test.rs"]
mod tests;

/// Hashes a string with the seeded 32-bit MurmurHash3 over its UTF-16 code
/// units, two units per block.
///
/// Mirrors `FrequencySketch.Hasher.StringHasher` in `org.apache.pekko.util`,
/// which calls `scala.util.hashing.MurmurHash3.stringHash`. Strings hash to
/// the same values as on the JVM. [`Default`] uses the original's default
/// seed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StringHasher {
  seed: i32,
}

impl StringHasher {
  /// The seed of the original's implicit string hasher.
  pub const DEFAULT_SEED: i32 = 135_283_237;

  /// Creates a hasher with the given seed.
  #[must_use]
  pub const fn new(seed: i32) -> Self {
    Self { seed }
  }
}

impl Default for StringHasher {
  fn default() -> Self {
    Self::new(Self::DEFAULT_SEED)
  }
}

impl HasherSeal<str> for StringHasher {}

impl FrequencySketchHasher<str> for StringHasher {
  fn hash(&self, value: &str) -> i32 {
    let mut state = Murmur3::new(self.seed);
    let mut pending: Option<u16> = None;
    let mut length: i32 = 0;
    for unit in value.encode_utf16() {
      length = length.wrapping_add(1);
      pending = match pending {
        | Some(high) => {
          state = state.mix((i32::from(high) << 16).wrapping_add(i32::from(unit)));
          None
        },
        | None => Some(unit),
      };
    }
    if let Some(last) = pending {
      state = state.mix_last(i32::from(last));
    }
    state.finalize(length)
  }
}
