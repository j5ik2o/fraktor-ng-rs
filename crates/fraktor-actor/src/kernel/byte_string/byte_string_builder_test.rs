//! Unit tests for [`ByteStringBuilder`](super::ByteStringBuilder).

use alloc::{vec, vec::Vec};

use super::{ByteString, ByteStringBuilder};

fn bytes(string: &ByteString) -> Vec<u8> {
  string.iterator().collect()
}

#[test]
fn an_empty_builder_results_in_the_empty_byte_string() {
  assert_eq!(ByteStringBuilder::default().result(), ByteString::empty());
}

#[test]
fn add_all_and_append_keep_the_order_they_are_called_in() {
  let builder = ByteString::new_builder()
    .add_all(vec![b'a', b'b'])
    .append(&ByteString::from_string("cd"))
    .add_all(vec![b'e'])
    .clone();
  assert_eq!(bytes(&builder.result()), b"abcde");
}

#[test]
fn a_result_can_be_followed_by_more_bytes() {
  let mut builder = ByteString::new_builder();
  builder.add_all(vec![b'a']);

  let first = builder.result();
  assert_eq!(bytes(&first), b"a");

  builder.add_all(vec![b'b']);
  builder.append(&ByteString::from_string("c"));

  assert_eq!(bytes(&first), b"a");
  assert_eq!(bytes(&builder.result()), b"abc");
}

#[test]
fn appending_an_empty_byte_string_changes_nothing() {
  let builder = ByteString::new_builder().add_all(vec![b'a']).append(&ByteString::empty()).clone();
  assert_eq!(bytes(&builder.result()), b"a");

  let empty = ByteString::new_builder().append(&ByteString::empty()).clone();
  assert_eq!(empty.result(), ByteString::empty());
}
