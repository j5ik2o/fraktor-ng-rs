// Ported from Pekko v1.7.1 (SHA 9a54d1294c3ac7821bbed3c940769c3d5027d1ff):
//   actor-tests/src/test/scala/org/apache/pekko/util/VersionSpec.scala
//
// The original's eight cases under `"Version" should` are kept as one test each,
// named after the case, with the original name as a comment. Every assertion from
// each case runs in the original order with the original inputs.
//
// `Version(str)` parses at construction and throws on failure; the port's
// `Version::parse(text) -> Result<Version, VersionParseError>` returns the outcome.
// The `version()` helper below unwraps a successful parse so that the original's
// comparisons read the same way, and the parse-failure cases compare the `Err`
// variant directly:
//   - `intercept[IllegalArgumentException](Version("1.2.3.1"))`
//     -> `Err(VersionParseError::UnsupportedSegmentCount)`
//   - `intercept[NumberFormatException](...)`
//     -> `Err(VersionParseError::InvalidNumber)`
// `a should ===(b)` / `a should !==(b)` become `assert_eq!` / `assert_ne!`, and
// `a should be > b` / `a should be < b` become `assert!(a > b)` / `assert!(a < b)`.
//
// The product's `Version` and `VersionParseError` live at
// `fraktor_actor::kernel::version`. Until that module exists the file does not
// compile: that compile error is the intended red for this node.

use fraktor_actor::kernel::version::{Version, VersionParseError};

fn version(text: &str) -> Version {
  Version::parse(text).expect("the original constructs this version at parse time")
}

// "compare 3 digit version"
#[test]
fn compare_3_digit_version() {
  assert_eq!(version("1.2.3"), version("1.2.3"));
  assert_ne!(version("1.2.3"), version("1.2.4"));
  assert!(version("1.2.4") > version("1.2.3"));
  assert!(version("3.2.1") > version("1.2.3"));
  assert!(version("3.2.1") < version("3.3.1"));
  assert!(version("3.2.0") < version("3.2.1"));
}

// "not support more than 3 digits version"
#[test]
fn not_support_more_than_3_digits_version() {
  assert_eq!(Version::parse("1.2.3.1"), Err(VersionParseError::UnsupportedSegmentCount));
}

// "compare 2 digit version"
#[test]
fn compare_2_digit_version() {
  assert_eq!(version("1.2"), version("1.2"));
  assert_eq!(version("1.2"), version("1.2.0"));
  assert_ne!(version("1.2"), version("1.3"));
  assert!(version("1.2.1") > version("1.2"));
  assert!(version("2.4") > version("2.3"));
  assert!(version("3.2") < version("3.2.7"));
}

// "compare single digit version"
#[test]
fn compare_single_digit_version() {
  assert_eq!(version("1"), version("1"));
  assert_eq!(version("1"), version("1.0"));
  assert_eq!(version("1"), version("1.0.0"));
  assert_ne!(version("1"), version("2"));
  assert!(version("3") > version("2"));

  assert!(version("2b") > version("2a"));
  assert!(version("2020-09-07") > version("2020-08-30"));
}

// "compare extra"
#[test]
fn compare_extra() {
  assert_eq!(version("1.2.3-M1"), version("1.2.3-M1"));
  assert_eq!(version("1.2-M1"), version("1.2-M1"));
  assert_eq!(version("1.2.0-M1"), version("1.2-M1"));
  assert_ne!(version("1.2.3-M1"), version("1.2.3-M2"));
  assert!(version("1.2-M1") < version("1.2.0"));
  assert!(version("1.2.0-M1") < version("1.2.0"));
  assert!(version("1.2.3-M2") > version("1.2.3-M1"));
}

// "require digits"
#[test]
fn require_digits() {
  assert_eq!(Version::parse("1.x.3"), Err(VersionParseError::InvalidNumber));
  assert_eq!(Version::parse("1.2x.3"), Err(VersionParseError::InvalidNumber));
  assert_eq!(Version::parse("1.2.x"), Err(VersionParseError::InvalidNumber));
  assert_eq!(Version::parse("1.2.3x"), Err(VersionParseError::InvalidNumber));

  assert_eq!(Version::parse("x.3"), Err(VersionParseError::InvalidNumber));
  assert_eq!(Version::parse("1.x"), Err(VersionParseError::InvalidNumber));
  assert_eq!(Version::parse("1.2x"), Err(VersionParseError::InvalidNumber));
}

// "compare dynver format"
#[test]
fn compare_dynver_format() {
  // dynver format
  assert!(version("1.0.10+3-1234abcd") < version("1.0.11"));
  assert!(version("1.0.10+3-1234abcd") < version("1.0.10+10-1234abcd"));
  assert!(version("1.2+3-1234abcd") < version("1.2+10-1234abcd"));
  assert!(version("1.0.0+3-1234abcd+20140707-1030") < version("1.0.0+3-1234abcd+20140707-1130"));
  assert!(version("0.0.0+3-2234abcd") < version("0.0.0+4-1234abcd"));
  assert!(version("HEAD+20140707-1030") < version("HEAD+20140707-1130"));

  assert!(version("1.0.10-3-1234abcd") < version("1.0.10-10-1234abcd"));
  assert!(version("1.0.0-3-1234abcd+20140707-1030") < version("1.0.0-3-1234abcd+20140707-1130"));

  // not real dynver, but should still work
  assert!(version("1.0.10+3a-1234abcd") < version("1.0.10+3b-1234abcd"));
}

// "compare extra without digits"
#[test]
fn compare_extra_without_digits() {
  assert_eq!(version("foo"), version("foo"));
  assert_ne!(version("foo"), version("bar"));
  assert!(version("foo") < version("1.2.3"));
  assert!(version("foo") > version("bar"));

  assert_ne!(version("1-foo"), version("01-foo"));
  assert!(version("1-foo") > version("02-foo"));
}
