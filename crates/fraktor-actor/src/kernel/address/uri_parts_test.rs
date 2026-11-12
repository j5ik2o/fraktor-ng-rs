//! Unit tests for the RFC 2396 character rules of [`UriParts`](super::UriParts).

use super::RelativeUriParts;
use super::UriParts;

#[test]
fn accepts_pchar_mark_characters_in_the_path() {
  let parts = UriParts::parse("pekko://sys/a!b").unwrap();
  assert_eq!(parts.raw_path(), "/a!b");

  let parts = UriParts::parse("pekko://sys/a'b(c)").unwrap();
  assert_eq!(parts.raw_path(), "/a'b(c)");

  let parts = UriParts::parse("pekko://sys/a*b~d").unwrap();
  assert_eq!(parts.raw_path(), "/a*b~d");
}

#[test]
fn accepts_percent_escapes_in_the_path_without_decoding() {
  let parts = UriParts::parse("pekko://sys/a%3Ab").unwrap();
  assert_eq!(parts.raw_path(), "/a%3Ab");
}

#[test]
fn rejects_incomplete_or_invalid_percent_escapes_in_the_path() {
  assert!(UriParts::parse("pekko://sys/a%3").is_none());
  assert!(UriParts::parse("pekko://sys/a%zz").is_none());
}

#[test]
fn accepts_mark_characters_in_the_user_info() {
  let parts = UriParts::parse("pekko://a!b'c@host:1").unwrap();
  assert_eq!(parts.user_info(), Some("a!b'c"));
}

#[test]
fn relative_parts_reject_absolute_references() {
  assert!(RelativeUriParts::parse("a:b").is_none());
}

#[test]
fn relative_parts_drop_the_query_and_keep_the_raw_path() {
  let parts = RelativeUriParts::parse("foo?x=1").unwrap();
  assert_eq!(parts.raw_path(), "foo");
  assert_eq!(parts.raw_fragment(), None);
}

#[test]
fn relative_parts_keep_an_empty_fragment() {
  let parts = RelativeUriParts::parse("foo#").unwrap();
  assert_eq!(parts.raw_fragment(), Some(""));
}

#[test]
fn relative_parts_reject_invalid_characters() {
  assert!(RelativeUriParts::parse("a b").is_none());
  assert!(RelativeUriParts::parse("a%zz").is_none());
}

#[test]
fn relative_parts_keep_leading_empty_elements() {
  let parts = RelativeUriParts::parse("/user/a").unwrap();
  assert_eq!(parts.raw_path(), "/user/a");
}
