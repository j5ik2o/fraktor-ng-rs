//! Unit tests for [`ByteStringInputStream`](super::ByteStringInputStream).

use super::super::ByteString;

#[test]
fn single_byte_reads_cross_concatenated_arrays_in_order() {
  let mut input = ByteString::from_string("a").concat(&ByteString::from_string("bc")).as_input_stream();
  assert_eq!(input.read(), Some(b'a'));
  assert_eq!(input.read(), Some(b'b'));
  assert_eq!(input.read(), Some(b'c'));
  assert_eq!(input.read(), None);
  assert_eq!(input.read(), None);
}

#[test]
fn a_buffer_read_returns_what_fits_then_the_rest_then_the_end() {
  let mut input = ByteString::from_string("ab").concat(&ByteString::from_string("cde")).as_input_stream();
  let mut buffer = [0_u8; 3];
  assert_eq!(input.read_into(&mut buffer), Some(3));
  assert_eq!(&buffer, b"abc");
  assert_eq!(input.read_into(&mut buffer), Some(2));
  assert_eq!(&buffer[..2], b"de");
  assert_eq!(input.read_into(&mut buffer), None);
}

#[test]
fn an_empty_buffer_reads_zero_bytes_before_and_at_the_end() {
  let mut input = ByteString::from_string("a").as_input_stream();
  assert_eq!(input.read_into(&mut []), Some(0));
  assert_eq!(input.read(), Some(b'a'));
  assert_eq!(input.read_into(&mut []), Some(0));
  assert_eq!(input.read_into(&mut [0]), None);
}

#[test]
fn single_byte_and_buffer_reads_share_one_position() {
  let mut input = ByteString::from_string("xyz").as_input_stream();
  assert_eq!(input.read(), Some(b'x'));
  let mut buffer = [0_u8; 8];
  assert_eq!(input.read_into(&mut buffer), Some(2));
  assert_eq!(&buffer[..2], b"yz");
}
