//! The polynomial string hash of the fixed-shape sketch.

use super::{hasher_seal::HasherSeal, FrequencySketchHasher};

#[cfg(test)]
#[path = "string_hash_code_test.rs"]
mod tests;

/// Hashes a string as `s[0]*31^(n-1) + ... + s[n-1]` over its UTF-16 code
/// units, wrapping in 32 bits.
///
/// `FastFrequencySketch` in `org.apache.pekko.util` hashes values with their
/// `hashCode`, which for a string is this hash, so strings land in the same
/// counters as on the JVM.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StringHashCode;

impl HasherSeal for StringHashCode {}

impl FrequencySketchHasher<str> for StringHashCode {
  fn hash(&self, value: &str) -> i32 {
    value.encode_utf16().fold(0_i32, |hash, unit| hash.wrapping_mul(31).wrapping_add(i32::from(unit)))
  }
}
