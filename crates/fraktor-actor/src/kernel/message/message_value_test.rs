//! Unit tests for [`Message`](super::Message).

use super::Message;

#[test]
fn a_static_string_equals_the_same_text() {
  let message = Message::new("hello world");

  assert_eq!(message, *"hello world");
}

#[test]
fn a_static_string_differs_from_other_text() {
  let message = Message::new("hello world");

  assert_ne!(message, *"goodbye");
}

#[test]
fn a_non_string_payload_never_equals_text() {
  let message = Message::new(42_u32);

  assert_ne!(message, *"42");
}

#[test]
fn downcast_returns_the_payload_of_the_matching_type() {
  let message = Message::new(42_u32);

  assert_eq!(message.downcast_ref::<u32>(), Some(&42));
  assert_eq!(message.downcast_ref::<i64>(), None);
}

#[test]
fn debug_shows_the_text_of_a_static_string() {
  let message = Message::new("hello world");

  assert_eq!(format!("{message:?}"), "\"hello world\"");
}
