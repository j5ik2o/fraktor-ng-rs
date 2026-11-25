//! The hash a frequency sketch places values with.

/// Hashes the values a frequency sketch counts to a 32-bit integer.
///
/// Mirrors `FrequencySketch.Hasher` in `org.apache.pekko.util`.
/// [`FrequencySketch`](super::FrequencySketch) uses [`StringHasher`](super::StringHasher)
/// by default, and [`FastFrequencySketch`](super::FastFrequencySketch) uses
/// [`StringHashCode`](super::StringHashCode), the hash each original uses for strings.
pub trait FrequencySketchHasher<A: ?Sized> {
  /// Returns the hash of `value`.
  fn hash(&self, value: &A) -> i32;
}
