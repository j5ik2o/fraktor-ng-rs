//! Unit tests for [`FraktorVersion`](super::FraktorVersion).

use super::FraktorVersion;

fn require(library_name: &str, required_version: &str, current_version: &str) -> bool {
  FraktorVersion::require(library_name, required_version, current_version).is_ok()
}

#[test]
fn keeps_the_original_cases_for_an_identical_version() {
  assert!(require("x", "nonsense", "nonsense"));
}

// The major is compared as text, so `02` and `2` differ even though they are
// the same number.
#[test]
fn rejects_a_textually_different_major() {
  assert!(!require("x", "2.5.6", "02.5.6"));
}

// When the minor differs, only the minor numbers are compared and the patch is
// ignored, so `2.05.9` against `2.5.1` is accepted.
#[test]
fn compares_only_minors_when_the_minor_text_differs() {
  assert!(require("x", "2.05.9", "2.5.1"));
}

// Leading zeros are stripped before the numeric comparison, so `07` equals `7`.
#[test]
fn compares_patch_as_a_number_ignoring_leading_zeros() {
  assert!(require("x", "2.5.07", "2.5.7"));
}

#[test]
fn accepts_a_release_candidate_requirement_against_the_release() {
  assert!(require("x", "2.5.6-RC1", "2.5.6"));
}

#[test]
fn accepts_a_milestone_requirement_against_a_release_candidate() {
  assert!(require("x", "2.5.6-M1", "2.5.6-RC1"));
}

// The qualifier letters are case sensitive, so `rc1` is not recognized and the
// pair falls back to the "unknown, comply" rule.
#[test]
fn complies_when_a_lowercase_qualifier_does_not_match() {
  assert!(require("x", "2.5.6-rc1", "2.5.6"));
}

// A trailing space breaks the whole-string match, so the pair is unknown.
#[test]
fn complies_when_the_pattern_does_not_match_the_whole_string() {
  assert!(require("x", "2.5.6", "2.5.6 "));
}

// The numbers are compared as digit strings, so none of these fit a native
// integer and none may overflow. The smaller one is still numerically less
// even though it has fewer digits, and the larger one has twenty-one digits.
#[test]
fn compares_large_numbers_without_overflow() {
  assert!(require("x", "2.5.99999999999", "2.5.100000000000"));
  assert!(!require("x", "2.5.100000000000", "2.5.99999999999"));
  assert!(require("x", "2.5.99999999999999999999", "2.5.100000000000000000000"));
  assert!(!require("x", "2.5.100000000000000000000", "2.5.99999999999999999999"));
}

#[test]
fn rejects_a_requirement_that_is_a_newer_patch() {
  assert!(!require("x", "2.5.6", "2.5.5"));
}

#[test]
fn rejects_a_requirement_that_is_a_newer_milestone() {
  assert!(!require("x", "2.5.7-M10", "2.5.7-M9"));
}

#[test]
fn carries_the_arguments_and_the_original_message_on_failure() {
  let error =
    FraktorVersion::require("PekkoVersionSpec", "2.5.6", "2.5.6-RC1").expect_err("the release is below the RC");

  assert_eq!(error.library_name(), "PekkoVersionSpec");
  assert_eq!(error.required_version(), "2.5.6");
  assert_eq!(error.current_version(), "2.5.6-RC1");
  assert_eq!(
    error.to_string(),
    "Current version of Fraktor is [2.5.6-RC1], but PekkoVersionSpec requires version [2.5.6]"
  );
}
