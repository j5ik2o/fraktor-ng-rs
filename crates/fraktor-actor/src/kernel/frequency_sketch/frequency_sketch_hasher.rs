//! The hash a frequency sketch places values with.

use super::hasher_seal::HasherSeal;

/// Hashes the values a frequency sketch counts to a 32-bit integer.
///
/// Mirrors `FrequencySketch.Hasher` in `org.apache.pekko.util`.
/// [`FrequencySketch`](super::FrequencySketch) uses [`StringHasher`](super::StringHasher)
/// by default, and [`FastFrequencySketch`](super::FastFrequencySketch) uses
/// [`StringHashCode`](super::StringHashCode), the hash each original uses for strings.
///
/// The trait is sealed, as the original's `sealed trait Hasher` is, for each
/// pair of a hasher and a value type: code outside this crate can neither
/// write a new hasher nor give a hasher of this crate another value type, and
/// a sketch counts only the value types this crate provides a hasher for.
///
/// A sketch with a hasher of this crate compiles:
///
/// ```
/// use fraktor_actor::kernel::frequency_sketch::{
///   FastFrequencySketch, FastFrequencySketchSettings, FrequencySketchHasher, StringHashCode,
/// };
///
/// struct ByLength;
///
/// impl ByLength {
///   fn hash(&self, value: &str) -> i32 {
///     value.len() as i32
///   }
/// }
///
/// assert_eq!(ByLength.hash("foo"), 3);
/// assert_eq!(StringHashCode.hash("a"), 97);
/// let mut sketch =
///   FastFrequencySketch::<str, StringHashCode>::with_hasher(4, FastFrequencySketchSettings::default(), StringHashCode)
///     .expect("the default settings are valid");
/// sketch.increment("foo");
/// assert_eq!(sketch.frequency("foo"), 1);
/// ```
///
/// The same code with `ByLength` as a hasher of `str` does not compile,
/// because the trait is sealed:
///
/// ```compile_fail,E0277
/// use fraktor_actor::kernel::frequency_sketch::{
///   FastFrequencySketch, FastFrequencySketchSettings, FrequencySketchHasher, StringHashCode,
/// };
///
/// struct ByLength;
///
/// impl FrequencySketchHasher<str> for ByLength {
///   fn hash(&self, value: &str) -> i32 {
///     value.len() as i32
///   }
/// }
///
/// assert_eq!(ByLength.hash("foo"), 3);
/// assert_eq!(StringHashCode.hash("a"), 97);
/// let mut sketch = FastFrequencySketch::<str, ByLength>::with_hasher(4, FastFrequencySketchSettings::default(), ByLength)
///   .expect("the default settings are valid");
/// sketch.increment("foo");
/// assert_eq!(sketch.frequency("foo"), 1);
/// ```
///
/// A value type outside this crate, counted by its string with a hasher of
/// this crate, compiles:
///
/// ```
/// use fraktor_actor::kernel::frequency_sketch::{
///   FastFrequencySketch, FastFrequencySketchSettings, FrequencySketchHasher, StringHashCode,
/// };
///
/// struct Key(&'static str);
///
/// assert_eq!(StringHashCode.hash(Key("a").0), 97);
/// let mut sketch =
///   FastFrequencySketch::<str, StringHashCode>::with_hasher(4, FastFrequencySketchSettings::default(), StringHashCode)
///     .expect("the default settings are valid");
/// sketch.increment(Key("foo").0);
/// assert_eq!(sketch.frequency(Key("foo").0), 1);
/// ```
///
/// The same code that gives `StringHashCode` the value type `Key` does not
/// compile, because the seal is for `StringHashCode` and `str` only:
///
/// ```compile_fail,E0277
/// use fraktor_actor::kernel::frequency_sketch::{
///   FastFrequencySketch, FastFrequencySketchSettings, FrequencySketchHasher, StringHashCode,
/// };
///
/// struct Key(&'static str);
///
/// impl FrequencySketchHasher<Key> for StringHashCode {
///   fn hash(&self, value: &Key) -> i32 {
///     <Self as FrequencySketchHasher<str>>::hash(self, value.0)
///   }
/// }
///
/// assert_eq!(<StringHashCode as FrequencySketchHasher<Key>>::hash(&StringHashCode, &Key("a")), 97);
/// let mut sketch =
///   FastFrequencySketch::<Key, StringHashCode>::with_hasher(4, FastFrequencySketchSettings::default(), StringHashCode)
///     .expect("the default settings are valid");
/// sketch.increment(&Key("foo"));
/// assert_eq!(sketch.frequency(&Key("foo")), 1);
/// ```
///
/// `compile_fail,E0277` names the error for the reader only: stable rustdoc
/// does not check the code. Each pair of examples differs only in the sealed
/// implementation, so the failing one fails for the seal.
pub trait FrequencySketchHasher<A: ?Sized>: HasherSeal<A> {
  /// Returns the hash of `value`.
  fn hash(&self, value: &A) -> i32;
}
