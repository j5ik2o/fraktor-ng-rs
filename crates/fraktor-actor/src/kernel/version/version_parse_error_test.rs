//! Unit tests for [`VersionParseError`](super::VersionParseError).

use alloc::string::ToString;

use super::VersionParseError;

#[test]
fn tells_its_variants_apart() {
  assert_ne!(VersionParseError::Empty, VersionParseError::UnsupportedSegmentCount);
  assert_ne!(VersionParseError::Empty, VersionParseError::InvalidNumber);
  assert_ne!(VersionParseError::UnsupportedSegmentCount, VersionParseError::InvalidNumber);
}

#[test]
fn displays_a_message_for_each_variant() {
  assert!(!VersionParseError::Empty.to_string().is_empty());
  assert!(!VersionParseError::UnsupportedSegmentCount.to_string().is_empty());
  assert!(!VersionParseError::InvalidNumber.to_string().is_empty());
}
