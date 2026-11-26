//! The hash a frequency sketch places values with.

use super::hasher_seal::HasherSeal;

/// Hashes the values a frequency sketch counts to a 32-bit integer.
///
/// Mirrors `FrequencySketch.Hasher` in `org.apache.pekko.util`.
/// [`FrequencySketch`](super::FrequencySketch) uses [`StringHasher`](super::StringHasher)
/// by default, and [`FastFrequencySketch`](super::FastFrequencySketch) uses
/// [`StringHashCode`](super::StringHashCode), the hash each original uses for strings.
///
/// The trait is sealed, as the original's `sealed trait Hasher` is: code
/// outside this crate cannot implement it, and a sketch counts only the value
/// types this crate provides a hasher for.
///
/// A hasher written outside this crate does not compile:
///
/// ```compile_fail
/// use fraktor_actor::kernel::frequency_sketch::FrequencySketchHasher;
///
/// struct ByLength;
///
/// impl FrequencySketchHasher<str> for ByLength {
///   fn hash(&self, value: &str) -> i32 {
///     value.len() as i32
///   }
/// }
/// ```
///
/// The hashers this crate provides are used the same way and compile, so the
/// example above fails only because the trait is sealed:
///
/// ```
/// use fraktor_actor::kernel::frequency_sketch::{FrequencySketchHasher, StringHashCode};
///
/// assert_eq!(StringHashCode.hash("a"), 97);
/// ```
pub trait FrequencySketchHasher<A: ?Sized>: HasherSeal {
  /// Returns the hash of `value`.
  fn hash(&self, value: &A) -> i32;
}
