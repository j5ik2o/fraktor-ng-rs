//! Unit tests for [`PreReleaseKind`](super::PreReleaseKind).

use super::PreReleaseKind;

#[test]
fn splits_the_kind_from_its_number() {
  assert_eq!(PreReleaseKind::split("RC10"), Some((PreReleaseKind::ReleaseCandidate, "10")));
  assert_eq!(PreReleaseKind::split("M9"), Some((PreReleaseKind::Milestone, "9")));
}

// The kind letters are case sensitive and nothing else is a kind, so these
// leave the qualifier unread.
#[test]
fn reads_no_kind_from_other_qualifiers() {
  assert_eq!(PreReleaseKind::split(""), None);
  assert_eq!(PreReleaseKind::split("R1"), None);
  assert_eq!(PreReleaseKind::split("rc1"), None);
  assert_eq!(PreReleaseKind::split("SNAPSHOT"), None);
  assert_eq!(PreReleaseKind::split("é1"), None);
}
