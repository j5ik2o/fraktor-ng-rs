//! Unit tests for [`DomainName`](super::DomainName).

use alloc::{string::String, vec, vec::Vec};

use super::DomainName;
use crate::kernel::io::dns::{WireFormatError, WireReader};

fn label(s: &str) -> Vec<u8> {
  let mut bytes = vec![u8::try_from(s.len()).unwrap()];
  bytes.extend_from_slice(s.as_bytes());
  bytes
}

fn pointer(offset: usize) -> Vec<u8> {
  vec![u8::try_from(0xC0 | (offset >> 8)).unwrap(), u8::try_from(offset & 0xFF).unwrap()]
}

fn parse(bytes: &[u8]) -> Result<DomainName, WireFormatError> {
  DomainName::parse(&mut WireReader::new(bytes))
}

#[test]
fn a_zero_length_byte_alone_is_the_empty_root_name() {
  assert_eq!(parse(&[0]).unwrap().as_str(), "");
}

#[test]
fn the_reader_moves_past_the_terminating_zero_byte() {
  let bytes = [label("www"), vec![0], vec![0xAB]].concat();
  let mut reader = WireReader::new(&bytes);
  DomainName::parse(&mut reader).unwrap();
  assert_eq!(reader.position(), 5);
}

// The original's `it` stops after the first pointer; the rest of the name is
// read through a fresh iterator.
#[test]
fn the_reader_moves_past_the_first_compression_pointer_only() {
  let bytes = [label("example"), vec![0], label("www"), pointer(0), vec![0xAB]].concat();
  let mut reader = WireReader::starting_at(&bytes, 9);
  assert_eq!(DomainName::parse(&mut reader).unwrap().as_str(), "www.example");
  assert_eq!(reader.position(), 15);
}

#[test]
fn a_pointer_may_be_the_whole_name() {
  let bytes = [label("example"), vec![0], pointer(0)].concat();
  assert_eq!(DomainName::parse(&mut WireReader::starting_at(&bytes, 9)).unwrap().as_str(), "example");
}

#[test]
fn a_pointer_offset_uses_the_low_six_bits_of_its_first_byte() {
  let mut bytes = vec![0_u8; 0x0103];
  bytes[0x0100..0x0103].copy_from_slice(&[1, b'a', 0]);
  bytes.extend_from_slice(&pointer(0x0100));
  assert_eq!(DomainName::parse(&mut WireReader::starting_at(&bytes, 0x0103)).unwrap().as_str(), "a");
}

// Offset 0 ends the name; each pointer at 1, 3, 5, ... points at the one
// before it, so starting at the last pointer follows every pointer.
fn pointer_chain(pointers: usize) -> (Vec<u8>, usize) {
  let mut bytes = vec![0_u8];
  let mut previous = 0;
  for _ in 0..pointers {
    let at = bytes.len();
    bytes.extend_from_slice(&pointer(previous));
    previous = at;
  }
  (bytes, previous)
}

#[test]
fn sixteen_compression_pointers_are_followed() {
  let (bytes, start) = pointer_chain(16);
  assert_eq!(DomainName::parse(&mut WireReader::starting_at(&bytes, start)).unwrap().as_str(), "");
}

#[test]
fn a_seventeenth_compression_pointer_is_rejected() {
  let (bytes, start) = pointer_chain(17);
  assert_eq!(
    DomainName::parse(&mut WireReader::starting_at(&bytes, start)),
    Err(WireFormatError::TooManyCompressionPointers)
  );
}

#[test]
fn the_0x80_label_type_is_reserved_too() {
  assert_eq!(parse(&[0x80, b'a', 0]), Err(WireFormatError::ReservedLabelType(2)));
}

#[test]
fn a_label_that_runs_past_the_end_is_truncated() {
  assert_eq!(parse(&[5, b'a', b'b']), Err(WireFormatError::Truncated));
}

#[test]
fn a_pointer_without_its_second_byte_is_truncated() {
  assert_eq!(parse(&[0xC0]), Err(WireFormatError::Truncated));
}

#[test]
fn a_pointer_past_the_end_of_the_message_is_truncated() {
  assert_eq!(parse(&pointer(40)), Err(WireFormatError::Truncated));
}

fn name_of_labels(lengths: &[usize]) -> Vec<u8> {
  let mut bytes: Vec<u8> = lengths.iter().flat_map(|&length| label(&"x".repeat(length))).collect();
  bytes.push(0);
  bytes
}

#[test]
fn a_name_of_255_characters_is_accepted() {
  let name = parse(&name_of_labels(&[63, 63, 63, 63])).unwrap();
  assert_eq!(name.as_str().chars().count(), 255);
}

#[test]
fn a_name_of_256_characters_is_rejected() {
  assert_eq!(parse(&name_of_labels(&[63, 63, 63, 62, 1])), Err(WireFormatError::NameTooLong));
}

// Scala's `Byte.toChar` sign-extends: byte `0xE9` is the character `U+FFE9`.
#[test]
fn a_byte_from_0x80_up_becomes_a_sign_extended_character() {
  let name = parse(&[3, b'a', 0xE9, 0x80, 0]).unwrap();
  assert_eq!(name.as_str(), "a\u{FFE9}\u{FF80}");
}

#[test]
fn the_name_displays_as_its_text() {
  let name = parse(&[label("www"), label("example"), vec![0]].concat()).unwrap();
  assert_eq!(alloc::format!("{name}"), String::from("www.example"));
}
