//! Unit tests for [`ByteStringInputStream`](super::ByteStringInputStream).

use alloc::vec::Vec;

use super::super::{ByteString, EndOfStreamError, IndexOutOfBoundsError, MarkNotSupportedError};

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

#[test]
fn reset_without_a_mark_goes_back_to_the_start() {
  let mut input = ByteString::from_string("ab").as_input_stream();
  assert_eq!(input.read(), Some(b'a'));
  assert_eq!(input.read(), Some(b'b'));
  assert_eq!(input.reset(), Ok(()));
  assert_eq!(input.read(), Some(b'a'));
}

#[test]
fn a_mark_survives_reading_past_the_read_ahead_limit_and_the_end() {
  let mut input = ByteString::from_string("abc").as_input_stream();
  assert_eq!(input.read(), Some(b'a'));
  input.mark(0);
  let mut buffer = [0_u8; 8];
  assert_eq!(input.read_into(&mut buffer), Some(2));
  assert_eq!(input.read(), None);
  assert_eq!(input.reset(), Ok(()));
  assert_eq!(input.read(), Some(b'b'));
  assert_eq!(input.reset(), Ok(()));
  assert_eq!(input.read_into(&mut buffer), Some(2));
  assert_eq!(&buffer[..2], b"bc");
}

#[test]
fn a_later_mark_replaces_the_earlier_one() {
  let mut input = ByteString::from_string("abc").as_input_stream();
  input.mark(1);
  assert_eq!(input.read(), Some(b'a'));
  input.mark(1);
  assert_eq!(input.read(), Some(b'b'));
  assert_eq!(input.reset(), Ok(()));
  assert_eq!(input.read(), Some(b'b'));
}

#[test]
fn reset_goes_back_to_the_mark_after_skipping_past_the_end() {
  let mut input = ByteString::from_string("abc").as_input_stream();
  assert_eq!(input.skip(1), 1);
  input.mark(1);
  assert_eq!(input.skip(50), 2);
  assert_eq!(input.reset(), Ok(()));
  assert_eq!(input.read(), Some(b'b'));
}

#[test]
fn a_sliced_byte_string_supports_marks_from_its_own_start() {
  let mut input = ByteString::from_array(b"abcdef", 1, 3).as_input_stream();
  assert!(input.mark_supported());
  assert_eq!(input.skip(2), 2);
  assert_eq!(input.reset(), Ok(()));
  assert_eq!(input.read(), Some(b'b'));
}

#[test]
fn an_empty_stream_skips_nothing_and_supports_marks() {
  let mut input = ByteString::empty().as_input_stream();
  assert!(input.mark_supported());
  assert_eq!(input.skip(1), 0);
  input.mark(1);
  assert_eq!(input.reset(), Ok(()));
  assert_eq!(input.read(), None);
}

#[test]
fn skip_returns_what_is_left_then_zero_after_the_end() {
  let mut input = ByteString::from_string("abc").as_input_stream();
  assert_eq!(input.skip(0), 0);
  assert_eq!(input.skip(usize::MAX), 3);
  assert_eq!(input.skip(0), 0);
  assert_eq!(input.skip(1), 0);
  assert_eq!(input.read(), None);
  assert_eq!(input.read_into(&mut [0]), None);
  assert_eq!(input.read_into(&mut []), Some(0));
}

#[test]
fn concatenating_with_an_empty_byte_string_keeps_mark_support() {
  let abc = ByteString::from_string("abc");
  assert!(abc.concat(&ByteString::empty()).as_input_stream().mark_supported());
  assert!(ByteString::empty().concat(&abc).as_input_stream().mark_supported());
  assert!(ByteString::empty().concat(&ByteString::empty()).as_input_stream().mark_supported());
}

#[test]
fn a_concatenation_does_not_support_marks() {
  let mut input = ByteString::from_string("a").concat(&ByteString::from_string("bc")).as_input_stream();
  assert!(!input.mark_supported());
  assert_eq!(input.read(), Some(b'a'));
  input.mark(1);
  assert_eq!(input.read(), Some(b'b'));
  assert_eq!(input.reset(), Err(MarkNotSupportedError));
  assert_eq!(input.read(), Some(b'c'));
  assert_eq!(input.read(), None);
}

#[test]
fn a_concatenation_skips_across_its_parts_and_stops_at_the_end() {
  let mut input = ByteString::from_string("ab").concat(&ByteString::from_string("cde")).as_input_stream();
  assert_eq!(input.skip(3), 3);
  assert_eq!(input.read(), Some(b'd'));
  assert_eq!(input.skip(usize::MAX), 1);
  assert_eq!(input.skip(1), 0);
  assert_eq!(input.read(), None);
}

#[test]
fn a_built_byte_string_supports_marks_by_how_many_arrays_it_holds() {
  // Pekko's builder keeps each added array as its own part; this builder
  // gathers added bytes into one array, so the result supports marks here and
  // not in Pekko (issue #148).
  let mut one_array = ByteString::new_builder();
  one_array.add_all([1_u8, 2]).add_all([3_u8]);
  assert!(one_array.result().as_input_stream().mark_supported());

  let mut several_arrays = ByteString::new_builder();
  several_arrays.append(&ByteString::from_string("a")).append(&ByteString::from_string("bc"));
  assert!(!several_arrays.result().as_input_stream().mark_supported());
}

#[test]
fn separately_added_arrays_build_one_array_whose_stream_resets_to_the_mark() {
  // An intended difference from Pekko v1.7.1 (issue #148): Pekko's builder
  // keeps `++= Array[Byte](1, 2)` and `++= Array[Byte](3)` as two parts, so
  // its result is a `ByteStrings` whose stream does not support marks. This
  // builder gathers them into one array, so the stream marks and resets as a
  // single array's does.
  let mut builder = ByteString::new_builder();
  builder.add_all([1_u8, 2]).add_all([3_u8]);
  let built = builder.result();
  assert_eq!(built.arrays().len(), 1);

  let mut input = built.as_input_stream();
  assert!(input.mark_supported());
  assert_eq!(input.read(), Some(1));
  input.mark(2);
  assert_eq!(input.read(), Some(2));
  assert_eq!(input.read(), Some(3));
  assert_eq!(input.read(), None);
  assert_eq!(input.reset(), Ok(()));
  assert_eq!(input.read(), Some(2));
  assert_eq!(input.read(), Some(3));
  assert_eq!(input.read(), None);
}

fn concatenated() -> super::ByteStringInputStream {
  ByteString::from_string("ab").concat(&ByteString::from_string("cde")).as_input_stream()
}

#[test]
fn available_counts_the_bytes_left_in_every_array() {
  let mut input = concatenated();
  assert_eq!(input.available(), 5);
  assert_eq!(input.read(), Some(b'a'));
  assert_eq!(input.read(), Some(b'b'));
  assert_eq!(input.available(), 3);
  assert_eq!(input.skip(3), 3);
  assert_eq!(input.available(), 0);
}

#[test]
fn bulk_reads_cross_concatenated_arrays() {
  let mut input = concatenated();
  assert_eq!(input.read(), Some(b'a'));
  assert_eq!(input.read_n_bytes(2), b"bc".to_vec());
  assert_eq!(input.read_all_bytes(), b"de".to_vec());
  assert_eq!(input.read_all_bytes(), Vec::<u8>::new());

  let mut input = concatenated();
  let mut buffer = [0_u8; 6];
  assert_eq!(input.read_n_bytes_into(&mut buffer, 1, 4), Ok(4));
  assert_eq!(&buffer[1..5], b"abcd");
  assert_eq!(input.read_range(&mut buffer, 0, 6), Ok(Some(1)));
  assert_eq!(buffer[0], b'e');
  let mut out = Vec::new();
  assert_eq!(input.transfer_to(&mut out), 0);
}

#[test]
fn skip_n_bytes_crosses_arrays_and_stops_at_the_end_when_short() {
  let mut input = concatenated();
  assert_eq!(input.skip_n_bytes(3), Ok(()));
  assert_eq!(input.read(), Some(b'd'));
  assert_eq!(input.skip_n_bytes(2), Err(EndOfStreamError));
  assert_eq!(input.available(), 0);
}

#[test]
fn a_range_outside_the_buffer_is_refused_without_moving_the_stream() {
  let mut input = concatenated();
  let mut buffer = [0_u8; 2];
  assert_eq!(input.read_range(&mut buffer, 3, 0), Err(IndexOutOfBoundsError));
  assert_eq!(input.read_range(&mut buffer, 1, usize::MAX), Err(IndexOutOfBoundsError));
  assert_eq!(input.read_n_bytes_into(&mut buffer, 0, 3), Err(IndexOutOfBoundsError));
  assert_eq!(input.available(), 5);
}
