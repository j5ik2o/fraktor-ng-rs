use super::StringHasher;
use crate::kernel::frequency_sketch::FrequencySketchHasher;

// The expected values are `MurmurHash3.stringHash(value, 135283237)` on Scala 2.13.18.
#[test]
fn hashes_strings_as_the_original() {
  let hasher = StringHasher::default();
  assert_eq!(hasher.hash("foo"), 537_187_922);
  assert_eq!(hasher.hash("bar"), -185_253_344);
  assert_eq!(hasher.hash("baz"), -478_924_459);
  assert_eq!(hasher.hash(""), -1_699_863_197);
  assert_eq!(hasher.hash("a"), 329_321_386);
}

#[test]
fn a_different_seed_gives_a_different_hash() {
  assert_ne!(StringHasher::new(1).hash("foo"), StringHasher::default().hash("foo"));
}
