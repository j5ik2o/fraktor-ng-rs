//! Unit tests for [`ByteString`](super::ByteString).

use alloc::{string::String, vec::Vec};

use super::ByteString;
use crate::kernel::sync::Arc;

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

fn shared(bytes: &[u8]) -> Arc<[u8]> {
  Arc::from(bytes)
}

fn base_of(array: &Arc<[u8]>) -> usize {
  Arc::as_ptr(array).cast::<u8>() as usize
}

// The start address and the length of each non-empty slice.
fn slices(string: &ByteString) -> Vec<(usize, usize)> {
  string.as_non_empty_byte_slices().map(|slice| (slice.as_ptr() as usize, slice.len())).collect()
}

#[test]
fn splitting_and_joining_again_gives_back_one_whole_array() {
  let string = ByteString::from_string("abcd");
  let base = slices(&string)[0].0;
  let (left, right) = string.split_at(2);
  assert_eq!(slices(&left), vec![(base, 2)]);
  assert_eq!(slices(&right), vec![(base + 2, 2)]);

  let joined = left.concat(&right);
  assert_eq!(slices(&joined), vec![(base, 4)]);
  assert!(joined.is_compact());
  assert_eq!(joined.as_byte_slices().count(), 1);
  assert!(joined.as_input_stream().mark_supported());
  assert_eq!(bytes(&joined), b"abcd");
}

#[test]
fn joining_adjacent_parts_of_one_array_gives_one_part() {
  let array = shared(&[0, 1, 2, 3, 4, 5]);
  let base = base_of(&array);
  let joined =
    ByteString::from_shared_range(array.clone(), 1, 2).concat(&ByteString::from_shared_range(array.clone(), 3, 2));
  assert_eq!(slices(&joined), vec![(base + 1, 4)]);
  assert!(!joined.is_compact());
  assert!(joined.as_input_stream().mark_supported());
  assert_eq!(bytes(&joined), vec![1, 2, 3, 4]);
}

#[test]
fn joining_parts_that_are_not_adjacent_keeps_both() {
  let array = shared(&[0, 1, 2, 3, 4, 5]);
  let base = base_of(&array);
  let joined =
    ByteString::from_shared_range(array.clone(), 0, 2).concat(&ByteString::from_shared_range(array.clone(), 3, 2));
  assert_eq!(slices(&joined), vec![(base, 2), (base + 3, 2)]);
  assert!(!joined.is_compact());
  assert!(!joined.as_input_stream().mark_supported());

  let backwards =
    ByteString::from_shared_range(array.clone(), 3, 2).concat(&ByteString::from_shared_range(array.clone(), 1, 2));
  assert_eq!(slices(&backwards), vec![(base + 3, 2), (base + 1, 2)]);
  assert!(!backwards.is_compact());
}

#[test]
fn joining_parts_of_different_arrays_keeps_both() {
  let first = shared(&[1, 2]);
  let second = shared(&[3, 4]);
  let joined = ByteString::from_shared(first.clone()).concat(&ByteString::from_shared(second.clone()));
  assert_eq!(slices(&joined), vec![(base_of(&first), 2), (base_of(&second), 2)]);
  assert!(!joined.is_compact());
  assert!(!joined.as_input_stream().mark_supported());
}

// Pekko's `ByteString2 ++ ByteString1` gives `ByteStrings` (ByteString.scala
// L1152-L1162) and `ByteString1 ++ ByteString2` gives `ByteStrings` (L670-L671):
// the touching ends of a value of several fragments are not joined, even when
// they are adjacent ranges of one array.
#[test]
fn the_ends_of_a_value_of_several_fragments_are_not_joined() {
  let array = shared(&[0, 1, 2, 3]);
  let other = shared(&[9, 9]);
  let base = base_of(&array);

  let left = ByteString::from_shared(other.clone()).concat(&ByteString::from_shared_range(array.clone(), 0, 2));
  let appended = left.concat(&ByteString::from_shared_range(array.clone(), 2, 2));
  assert_eq!(slices(&appended), vec![(base_of(&other), 2), (base, 2), (base + 2, 2)]);
  assert_eq!(appended.as_byte_slices().count(), 3);

  let right = ByteString::from_shared_range(array.clone(), 2, 2).concat(&ByteString::from_shared(other.clone()));
  let prepended = ByteString::from_shared_range(array.clone(), 0, 2).concat(&right);
  assert_eq!(slices(&prepended), vec![(base, 2), (base + 2, 2), (base_of(&other), 2)]);
  assert_eq!(prepended.as_byte_slices().count(), 3);
}

#[test]
fn from_shared_holds_the_array_itself() {
  let array = shared(&[1, 2, 3]);
  let string = ByteString::from_shared(array.clone());
  assert_eq!(slices(&string), vec![(base_of(&array), 3)]);
  assert!(string.is_compact());
  assert_eq!(bytes(&string), vec![1, 2, 3]);
}

#[test]
fn from_shared_of_an_empty_array_is_empty() {
  let string = ByteString::from_shared(shared(&[]));
  assert!(string.is_empty());
  assert!(string.is_compact());
  assert_eq!(string, ByteString::empty());
  assert_eq!(string.as_non_empty_byte_slices().count(), 0);
}

#[test]
fn from_shared_range_stops_at_the_end_of_the_array() {
  let array = shared(&[0, 1, 2, 3, 4, 5]);
  let base = base_of(&array);

  let whole = ByteString::from_shared_range(array.clone(), 0, 6);
  assert_eq!(slices(&whole), vec![(base, 6)]);
  assert!(whole.is_compact());

  let part = ByteString::from_shared_range(array.clone(), 1, 3);
  assert_eq!(slices(&part), vec![(base + 1, 3)]);
  assert!(!part.is_compact());

  let to_the_end = ByteString::from_shared_range(array.clone(), 4, usize::MAX);
  assert_eq!(slices(&to_the_end), vec![(base + 4, 2)]);
  assert!(!to_the_end.is_compact());

  let past_the_end = ByteString::from_shared_range(array.clone(), 2, 10);
  assert_eq!(slices(&past_the_end), vec![(base + 2, 4)]);

  for (offset, length) in [(10, 2), (6, 1), (3, 0), (usize::MAX, usize::MAX), (usize::MAX, 1)] {
    let empty = ByteString::from_shared_range(array.clone(), offset, length);
    assert!(empty.is_empty(), "offset={offset} length={length}");
    assert!(empty.is_compact(), "offset={offset} length={length}");
    assert_eq!(empty.as_non_empty_byte_slices().count(), 0, "offset={offset} length={length}");
  }
}

#[test]
fn a_part_of_an_array_becomes_compact_only_by_compact() {
  let array = shared(&[0, 1, 2, 3, 4, 5]);
  let base = base_of(&array);
  for part in [ByteString::from_shared(array.clone()).slice(1, 4), ByteString::from_shared_range(array.clone(), 1, 3)] {
    assert!(!part.is_compact());
    assert_eq!(slices(&part), vec![(base + 1, 3)]);
    let compacted = part.compact();
    assert!(compacted.is_compact());
    assert_eq!(bytes(&compacted), vec![1, 2, 3]);
    let (address, len) = slices(&compacted)[0];
    assert_eq!(len, 3);
    assert!(address < base || address >= base + 6, "compact did not copy into a new array");
  }
}

#[test]
fn compact_of_a_compact_value_shares_it() {
  let array = shared(&[7, 8, 9]);
  let compacted = ByteString::from_shared(array.clone()).compact();
  assert_eq!(slices(&compacted), vec![(base_of(&array), 3)]);
}

#[test]
fn take_of_the_whole_length_and_slice_of_everything_share_the_value() {
  let array = shared(&[1, 2, 3]);
  let other = shared(&[4]);
  let string = ByteString::from_shared(array.clone()).concat(&ByteString::from_shared(other.clone()));
  let expected = vec![(base_of(&array), 3), (base_of(&other), 1)];
  assert_eq!(slices(&string.take(4)), expected);
  assert_eq!(slices(&string.take(usize::MAX)), expected);
  assert_eq!(slices(&string.slice(0, 4)), expected);
  assert_eq!(slices(&string.slice(0, usize::MAX)), expected);
  assert_eq!(slices(&string.drop(0)), expected);
}

#[test]
#[should_panic(expected = "size=0 must be positive")]
fn grouped_of_zero_panics_on_an_empty_value_before_it_is_iterated() {
  let _groups = ByteString::empty().grouped(0);
}

#[test]
#[should_panic(expected = "size=0 must be positive")]
fn grouped_of_zero_panics_on_a_non_empty_value_before_it_is_iterated() {
  let _groups = ByteString::from_string("abc").grouped(0);
}

#[test]
fn grouped_of_the_largest_size_gives_one_group() {
  let string = ByteString::from_string("abc");
  let groups: Vec<ByteString> = string.grouped(usize::MAX).collect();
  assert_eq!(groups, vec![string]);
  assert_eq!(ByteString::empty().grouped(usize::MAX).count(), 0);
}

#[test]
fn the_cuts_clamp_the_largest_counts() {
  let string = ByteString::from_string("abc");
  assert_eq!(bytes(&string.take_right(usize::MAX)), b"abc");
  assert!(string.drop_right(usize::MAX).is_empty());
  assert!(string.drop(usize::MAX).is_empty());
  assert!(string.slice(usize::MAX, usize::MAX).is_empty());
  assert!(string.slice(2, 1).is_empty());
  assert!(string.slice(3, 3).is_empty());
  assert_eq!(bytes(&string.slice(1, usize::MAX)), b"bc");
  let (left, right) = string.split_at(usize::MAX);
  assert_eq!(bytes(&left), b"abc");
  assert!(right.is_empty());
}

#[test]
fn copy_to_array_writes_nothing_past_the_destination() {
  let string = ByteString::from_string("abc");
  let mut dest = [0u8; 3];
  assert_eq!(string.copy_to_array(&mut dest, 3, 1), 0);
  assert_eq!(string.copy_to_array(&mut dest, usize::MAX, 1), 0);
  assert_eq!(string.copy_to_array(&mut dest, 5, usize::MAX), 0);
  assert_eq!(dest, [0, 0, 0]);
  assert_eq!(string.copy_to_array(&mut dest, 1, usize::MAX), 2);
  assert_eq!(dest, [0, b'a', b'b']);
  assert_eq!(string.copy_to_array(&mut dest, 0, 1), 1);
  assert_eq!(dest, [b'a', b'a', b'b']);
}

#[test]
fn the_single_byte_reads_of_an_empty_value() {
  let empty = ByteString::empty();
  assert_eq!(empty.head(), None);
  assert_eq!(empty.last(), None);
  assert_eq!(empty.get(0), None);
  assert!(empty.tail().is_empty());
  assert!(empty.init().is_empty());
  assert_eq!(empty.len(), 0);
  assert!(empty.is_compact());
}

#[test]
fn get_of_the_length_is_none() {
  let string = ByteString::from_string("ab").concat(&ByteString::from_string("c"));
  assert_eq!(string.get(2), Some(b'c'));
  assert_eq!(string.get(3), None);
  assert_eq!(string.get(usize::MAX), None);
}

#[test]
fn the_slices_of_an_empty_value() {
  let empty = ByteString::empty();
  let all: Vec<&[u8]> = empty.as_byte_slices().collect();
  assert_eq!(all, vec![&[][..]]);
  assert_eq!(empty.as_non_empty_byte_slices().count(), 0);
}

#[test]
fn display_prints_signed_bytes_and_cuts_after_one_hundred() {
  assert_eq!(ByteString::empty().to_string(), "ByteString()");
  assert_eq!(ByteString::from_ints(&[0xFF]).to_string(), "ByteString(-1)");
  assert_eq!(ByteString::from_ints(&[1, 2]).to_string(), "ByteString(1, 2)");
  assert_eq!(ByteString::from_ints(&[127, 128, 0]).to_string(), "ByteString(127, -128, 0)");

  let hundred = ByteString::from_array(&[1; 100], 0, 100);
  let ones = vec!["1"; 100].join(", ");
  assert_eq!(hundred.to_string(), alloc::format!("ByteString({ones})"));

  let hundred_and_one = ByteString::from_array(&[1; 101], 0, 101);
  assert_eq!(hundred_and_one.to_string(), alloc::format!("ByteString({ones})... and [1] more"));

  let split = ByteString::from_array(&[2; 60], 0, 60).concat(&ByteString::from_array(&[2; 90], 0, 90));
  let twos = vec!["2"; 100].join(", ");
  assert_eq!(split.to_string(), alloc::format!("ByteString({twos})... and [50] more"));
}

#[test]
fn decode_string_replaces_malformed_utf8() {
  let malformed = ByteString::from_array(&[b'a', 0xE2, 0x82, b'b', 0xFF, 0xF0, 0x9F, 0x98, 0x80], 0, 9);
  assert_eq!(malformed.decode_string(), String::from("a\u{FFFD}b\u{FFFD}\u{1F600}"));
  let split_char = ByteString::from_string("\u{e9}").take(1).concat(&ByteString::from_string("\u{e9}").drop(1));
  assert_eq!(split_char.decode_string(), "\u{e9}");
}

fn base64(text: &[u8]) -> ByteString {
  ByteString::from_array(text, 0, text.len())
}

#[test]
fn encode_base64_matches_the_rfc_4648_vectors() {
  let vectors: [(&[u8], &[u8]); 10] = [
    (b"", b""),
    (b"f", b"Zg=="),
    (b"fo", b"Zm8="),
    (b"foo", b"Zm9v"),
    (b"foob", b"Zm9vYg=="),
    (b"fooba", b"Zm9vYmE="),
    (b"foobar", b"Zm9vYmFy"),
    (&[0xFB, 0xFF], b"+/8="),
    (&[0xFF, 0xEF], b"/+8="),
    (&[0x00], b"AA=="),
  ];
  for (plain, encoded) in vectors {
    let actual = base64(plain).encode_base64();
    assert_eq!(bytes(&actual), encoded, "{plain:?}");
    assert!(actual.is_compact(), "{plain:?}");
    assert_eq!(base64(encoded).decode_base64(), Some(base64(plain)), "{encoded:?}");
  }
}

#[test]
fn decode_base64_accepts_what_the_jdk_decoder_accepts() {
  let accepted: [(&[u8], &[u8]); 6] = [
    (b"", b""),
    (b"QQ", &[0x41]),
    (b"QUI", &[0x41, 0x42]),
    (b"AB==", &[0x00]),
    (b"ABB=", &[0x00, 0x10]),
    (b"Zm9vYg", b"foob"),
  ];
  for (encoded, plain) in accepted {
    let decoded = base64(encoded).decode_base64().unwrap_or_else(|| panic!("{encoded:?} is rejected"));
    assert_eq!(bytes(&decoded), plain, "{encoded:?}");
    assert!(decoded.is_compact(), "{encoded:?}");
  }
}

#[test]
fn decode_base64_rejects_what_the_jdk_decoder_rejects() {
  let rejected: [&[u8]; 20] = [
    b"A",
    b"AAAAA",
    b"QQ==\n",
    b"QQ ==",
    b" QQ==",
    b"QQ\r\n",
    b"QQ\t==",
    b"-_8=",
    b"QQ\0=",
    b"\xff\xff\xff\xff",
    b"QQ\x80=",
    b"====",
    b"A===",
    b"=AAA",
    b"A=AA",
    b"AA=",
    b"AA=A",
    b"QQ==A",
    b"QQ=A",
    b"Zm9v=",
  ];
  for encoded in rejected {
    assert_eq!(base64(encoded).decode_base64(), None, "{encoded:?}");
  }
}
