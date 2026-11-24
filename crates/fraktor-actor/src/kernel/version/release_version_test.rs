//! Unit tests for [`ReleaseVersion`](super::ReleaseVersion).

use super::ReleaseVersion;

impl ReleaseVersion<'_> {
  fn major(&self) -> &str {
    self.major
  }

  fn minor(&self) -> &str {
    self.minor
  }

  fn patch(&self) -> &str {
    self.patch
  }

  fn milestone(&self) -> Option<(&str, &str)> {
    self.milestone
  }

  fn is_numeric_less_than_patch(&self, other: &Self) -> bool {
    super::digits_less(self.patch, other.patch)
  }
}

// The matcher is anchored at both ends, so a prefix or a suffix that the
// pattern does not describe makes the whole version unreadable.
#[test]
fn matches_the_release_pattern_only_on_the_whole_string() {
  assert!(ReleaseVersion::parse("2.5.6").is_some());
  assert!(ReleaseVersion::parse("2.5.6-M1").is_some());
  assert!(ReleaseVersion::parse("2.5.6-RC10").is_some());

  assert!(ReleaseVersion::parse("2.5.6 ").is_none());
  assert!(ReleaseVersion::parse(" 2.5.6").is_none());
  assert!(ReleaseVersion::parse("2.5").is_none());
  assert!(ReleaseVersion::parse("2.5-SNAPSHOT").is_none());
  assert!(ReleaseVersion::parse("v2.5.6").is_none());
  assert!(ReleaseVersion::parse("2.5.6-M1-extra").is_none());
}

// The qualifier letters are case sensitive.
#[test]
fn matches_only_uppercase_qualifier_letters() {
  assert!(ReleaseVersion::parse("2.5.6-rc1").is_none());
  assert!(ReleaseVersion::parse("2.5.6-m1").is_none());
  assert!(ReleaseVersion::parse("2.5.6-ALPHA1").is_none());
}

#[test]
fn keeps_each_number_as_its_original_text() {
  let release = ReleaseVersion::parse("02.05.006-M1").expect("the input matches");

  assert_eq!(release.major(), "02");
  assert_eq!(release.minor(), "05");
  assert_eq!(release.patch(), "006");
  assert_eq!(release.milestone(), Some(("M", "1")));
}

#[test]
fn reports_no_milestone_for_a_plain_release() {
  let release = ReleaseVersion::parse("2.5.6").expect("the input matches");

  assert_eq!(release.milestone(), None);
}

// Digit strings are compared by length and then lexicographically, so values
// that overflow a native integer still compare correctly.
#[test]
fn compares_digit_strings_beyond_the_native_integer_range() {
  let smaller = ReleaseVersion::parse("2.5.99999999999").expect("the input matches");
  let larger = ReleaseVersion::parse("2.5.100000000000").expect("the input matches");

  assert!(smaller.is_numeric_less_than_patch(&larger));
  assert!(!larger.is_numeric_less_than_patch(&smaller));

  let huge_smaller = ReleaseVersion::parse("2.5.99999999999999999999").expect("the input matches");
  let huge_larger = ReleaseVersion::parse("2.5.100000000000000000000").expect("the input matches");

  assert!(huge_smaller.is_numeric_less_than_patch(&huge_larger));
  assert!(!huge_larger.is_numeric_less_than_patch(&huge_smaller));
}

// Leading zeros do not change the value, so `07` and `7` are equal.
#[test]
fn treats_leading_zeros_as_equal() {
  let padded = ReleaseVersion::parse("2.5.07").expect("the input matches");
  let plain = ReleaseVersion::parse("2.5.7").expect("the input matches");

  assert!(!padded.is_numeric_less_than_patch(&plain));
  assert!(!plain.is_numeric_less_than_patch(&padded));
}

// A dash with no qualifier, or a qualifier that starts with a multi-byte
// character, does not match `-(M|RC)(\d+)`, so the version is unreadable
// instead of panicking on a byte split.
#[test]
fn a_dash_without_a_milestone_does_not_match() {
  assert_eq!(ReleaseVersion::parse("2.5.6-"), None);
  assert_eq!(ReleaseVersion::parse("2.5.6-é1"), None);
  assert_eq!(ReleaseVersion::parse("2.5.6-Mé"), None);
  assert_eq!(ReleaseVersion::parse("2.5.6-R"), None);
}
