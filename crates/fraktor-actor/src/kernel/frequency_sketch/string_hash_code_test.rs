use super::StringHashCode;
use crate::kernel::frequency_sketch::FrequencySketchHasher;

// The expected values are `String.hashCode` on the JVM.
#[test]
fn hashes_strings_as_the_jvm_string_hash_code() {
  assert_eq!(StringHashCode.hash("foo"), 101_574);
  assert_eq!(StringHashCode.hash("bar"), 97_299);
  assert_eq!(StringHashCode.hash(""), 0);
}
