//! Unit tests for [`ByteString`](super::ByteString).

use alloc::vec::Vec;

use super::ByteString;

fn bytes(string: &ByteString) -> Vec<u8> {
  string.iterator().collect()
}

#[test]
fn from_string_encodes_as_utf8() {
  assert_eq!(bytes(&ByteString::from_string("é")), "é".as_bytes());
}

#[test]
fn from_array_copies_the_requested_range() {
  assert_eq!(bytes(&ByteString::from_array(b"abcdef", 1, 3)), b"bcd");
}

#[test]
fn from_array_stops_at_the_end_of_the_array() {
  assert_eq!(bytes(&ByteString::from_array(b"abc", 1, 10)), b"bc");
  assert_eq!(bytes(&ByteString::from_array(b"abc", 5, 2)), b"");
  assert_eq!(bytes(&ByteString::from_array(b"abc", 3, 0)), b"");
}

#[test]
fn concat_keeps_the_left_bytes_before_the_right_bytes() {
  let left = ByteString::from_string("ab");
  let joined = left.concat(&ByteString::empty()).concat(&ByteString::from_string("cd"));
  assert_eq!(bytes(&joined), b"abcd");
  assert_eq!(bytes(&left), b"ab");
}

#[test]
fn from_ints_keeps_the_low_byte_of_each_value() {
  assert_eq!(bytes(&ByteString::from_ints(&[1, 2, 3])), vec![1, 2, 3]);
  assert_eq!(bytes(&ByteString::from_ints(&[256, -1])), vec![0, 0xff]);
  assert_eq!(bytes(&ByteString::from_ints(&[])), b"");
}

#[test]
fn byte_strings_with_the_same_bytes_are_equal_however_they_are_split() {
  let whole = ByteString::from_string("abc");
  let split = ByteString::from_string("a").concat(&ByteString::from_string("bc"));

  assert_eq!(whole, split);
  assert_ne!(whole, ByteString::from_string("abd"));
  assert_eq!(ByteString::from_ints(&[97, 98, 99]), whole);
}

#[test]
fn each_iterator_starts_from_the_first_byte() {
  let string = ByteString::from_string("xy");
  let mut first = string.iterator();
  assert_eq!(first.next(), Some(b'x'));
  assert_eq!(bytes(&string), b"xy");
}
