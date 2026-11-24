//! Unit tests for [`MessageFlags`](super::MessageFlags).

use super::MessageFlags;

#[test]
fn a_truncated_recursive_answer_has_its_bits_set() {
  let flags = MessageFlags::new(0x8380);
  assert_eq!(flags.bits(), 0x8380);
  assert!(flags.is_answer());
  assert!(!flags.is_query());
  assert!(flags.is_truncated());
  assert!(flags.is_recursion_desired());
  assert!(flags.is_recursion_available());
  assert!(!flags.is_authoritative_answer());
}

#[test]
fn a_recursive_query_has_only_the_rd_bit() {
  let flags = MessageFlags::new(0x0100);
  assert!(flags.is_query());
  assert!(!flags.is_answer());
  assert!(!flags.is_truncated());
  assert!(flags.is_recursion_desired());
  assert!(!flags.is_recursion_available());
}

#[test]
fn the_aa_bit_is_bit_ten() {
  assert!(MessageFlags::new(1 << 10).is_authoritative_answer());
  assert!(!MessageFlags::new(1 << 9).is_authoritative_answer());
}
