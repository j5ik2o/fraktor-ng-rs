//! Unit tests for [`ByteIterator`](super::ByteIterator).

use alloc::{sync::Arc, vec, vec::Vec};

use super::ByteIterator;

fn over(arrays: &[&[u8]]) -> ByteIterator {
  ByteIterator::over_arrays(arrays.iter().map(|array| Arc::from(*array)).collect())
}

#[test]
fn a_byte_that_is_absent_is_not_found() {
  assert_eq!(ByteIterator::from_array(&[1, 2, 3]).index_of(9), None);
  assert_eq!(ByteIterator::from_array(&[]).index_of(0), None);
}

#[test]
fn a_byte_before_the_start_position_is_not_found() {
  assert_eq!(ByteIterator::from_array(&[7, 1, 1]).index_of_from(7, 1), None);
}

#[test]
fn a_start_position_past_the_end_finds_nothing() {
  assert_eq!(ByteIterator::from_array(&[1, 2]).index_of_from(1, 5), None);
}

#[test]
fn a_search_consumes_the_bytes_up_to_the_match() {
  let mut iterator = ByteIterator::from_array(&[1, 2, 3, 2]);
  assert_eq!(iterator.index_of(2), Some(1));
  assert_eq!(iterator.collect::<Vec<_>>(), vec![3, 2]);
}

#[test]
fn the_index_counts_across_arrays() {
  assert_eq!(over(&[&[1, 2], &[], &[3, 4]]).index_of(4), Some(3));
  assert_eq!(over(&[&[5], &[5, 5]]).index_of_from(5, 2), Some(2));
}

#[test]
fn iteration_yields_every_array_in_order_and_skips_empty_ones() {
  assert_eq!(over(&[&[], &[1], &[], &[2, 3], &[]]).collect::<Vec<_>>(), vec![1, 2, 3]);
}

#[test]
fn copying_fills_the_buffer_across_arrays() {
  let mut iterator = over(&[&[1, 2], &[3], &[4, 5]]);
  let mut buffer = [0_u8; 4];
  assert_eq!(iterator.copy_into(&mut buffer), 4);
  assert_eq!(buffer, [1, 2, 3, 4]);
  assert!(iterator.has_next());
  assert_eq!(iterator.copy_into(&mut buffer), 1);
  assert_eq!(buffer[0], 5);
  assert!(!iterator.has_next());
  assert_eq!(iterator.copy_into(&mut buffer), 0);
}

#[test]
fn skipping_moves_across_arrays_and_stops_at_the_end() {
  let mut iterator = over(&[b"ab", b"", b"cde"]);
  assert_eq!(iterator.skip_bytes(3), 3);
  assert_eq!(iterator.next(), Some(b'd'));
  assert_eq!(iterator.skip_bytes(5), 1);
  assert!(!iterator.has_next());
  assert_eq!(iterator.skip_bytes(1), 0);
}
