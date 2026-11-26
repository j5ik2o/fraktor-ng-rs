// Ported from Pekko v2.0.0 (SHA 9fbd2283b1cf701c90a1223bf621942c191db61f):
//   actor-tests/src/test/scala/org/apache/pekko/util/MessageBufferSpec.scala
//
// The original's nine cases (seven under "A MessageBuffer must", two under
// "A MessageBufferMap must") are kept as one test each, named after the case,
// with the original name as a comment. The original buffers (Any, ActorRef)
// pairs and builds its senders from strings through an implicit conversion to
// a dummy ActorRef whose toString is the id; the port is generic over the
// message and the sender, and the tests use &str for both. The original's
// `head()` returns (null, null) on an empty buffer; the port returns an
// Option, and the original only calls `head()` on non-empty buffers.

use fraktor_actor::kernel::message_buffer::{MessageBuffer, MessageBufferMap};

type Buffer = MessageBuffer<&'static str, &'static str>;
type BufferMap = MessageBufferMap<&'static str, &'static str, &'static str>;

fn empty_buffer() -> Buffer {
  MessageBuffer::empty()
}

fn contents(buffer: &Buffer) -> String {
  let mut out = String::new();
  buffer.for_each(|message, sender| out.push_str(&format!("{message}->{sender}:")));
  out
}

// "answer empty correctly"
#[test]
fn answer_empty_correctly() {
  let mut buffer = empty_buffer();
  assert!(buffer.is_empty());
  buffer.append("m1", "s1");
  assert!(!buffer.is_empty());
}

// "append and drop"
#[test]
fn append_and_drop() {
  let mut buffer = empty_buffer();
  assert_eq!(buffer.size(), 0);
  buffer.append("m1", "s1");
  assert_eq!(buffer.size(), 1);
  buffer.append("m2", "s2");
  assert_eq!(buffer.size(), 2);
  let (m1, s1) = buffer.head().expect("the buffer holds two entries");
  assert_eq!(buffer.size(), 2);
  buffer.drop_head();
  assert_eq!(buffer.size(), 1);
  assert_eq!(m1, &"m1");
  assert_eq!(s1, &"s1");
  let (m2, s2) = buffer.head().expect("the buffer holds one entry");
  buffer.drop_head();
  assert_eq!(buffer.size(), 0);
  buffer.drop_head();
  assert_eq!(buffer.size(), 0);
  assert_eq!(m2, &"m2");
  assert_eq!(s2, &"s2");
}

// "process elements in the right order"
#[test]
fn process_elements_in_the_right_order() {
  let mut buffer = empty_buffer();
  buffer.append("m1", "s1");
  buffer.append("m2", "s2");
  buffer.append("m3", "s3");
  assert_eq!(contents(&buffer), "m1->s1:m2->s2:m3->s3:");
  buffer.drop_head();
  assert_eq!(contents(&buffer), "m2->s2:m3->s3:");
}

// "filterNot of first"
#[test]
fn filter_not_of_first() {
  let mut buffer = empty_buffer();
  buffer.append("m1", "s1");
  buffer.append("m2", "s2");
  buffer.filter_not(|message, _| *message == "m1");
  assert_eq!(buffer.head().map(|(message, _)| *message), Some("m2"));
  assert_eq!(buffer.size(), 1);
  buffer.filter_not(|message, _| *message == "m2");
  assert!(buffer.is_empty());
}

// "filterNot of last"
#[test]
fn filter_not_of_last() {
  let mut buffer = empty_buffer();
  buffer.append("m1", "s1");
  buffer.append("m2", "s2");
  buffer.filter_not(|message, _| *message == "m2");
  assert_eq!(buffer.head().map(|(message, _)| *message), Some("m1"));
  assert_eq!(buffer.size(), 1);
  buffer.filter_not(|message, _| *message == "m1");
  assert!(buffer.is_empty());
}

// "filterNot of single entry"
#[test]
fn filter_not_of_single_entry() {
  let mut buffer = empty_buffer();
  for (message, sender) in [("m1", "s1"), ("m2", "s2"), ("m3", "s3"), ("m4", "s4"), ("m5", "s5"), ("m6", "s6")] {
    buffer.append(message, sender);
  }
  assert_eq!(buffer.head().map(|(message, _)| *message), Some("m1"));
  buffer.filter_not(|message, _| *message == "m1");
  assert_eq!(buffer.head().map(|(message, _)| *message), Some("m2"));
  assert_eq!(buffer.size(), 5);
  buffer.filter_not(|message, _| *message == "m3");
  assert_eq!(buffer.size(), 4);
  buffer.filter_not(|message, _| *message == "m4");
  assert_eq!(buffer.size(), 3);
  buffer.drop_head();
  assert_eq!(buffer.head().map(|(message, _)| *message), Some("m5"));
  assert_eq!(buffer.size(), 2);
  buffer.filter_not(|message, _| *message == "m6");
  assert_eq!(buffer.size(), 1);
  buffer.append("m7", "s7");
  assert_eq!(buffer.head().map(|(message, _)| *message), Some("m5"));
  assert_eq!(buffer.size(), 2);
  buffer.filter_not(|message, _| *message == "m5");
  assert_eq!(buffer.size(), 1);
  buffer.filter_not(|message, _| *message == "m7");
  assert_eq!(buffer.size(), 0);
  assert!(buffer.is_empty());
}

// "filterNot of several entries"
#[test]
fn filter_not_of_several_entries() {
  let mut buffer = empty_buffer();
  buffer.append("m1", "s1");
  buffer.append("m2", "s2");
  buffer.append("m3", "s3");
  buffer.append("m4", "s4");
  buffer.append("m5", "s5");
  buffer.append("m6", "s6");
  buffer.filter_not(|message, _| *message == "m1" || *message == "m2");
  assert_eq!(buffer.head().map(|(message, _)| *message), Some("m3"));
  assert_eq!(buffer.size(), 4);
  buffer.filter_not(|message, _| *message == "m4" || *message == "m5");
  assert_eq!(buffer.size(), 2);
  buffer.drop_head();
  assert_eq!(buffer.head().map(|(message, _)| *message), Some("m6"));
  assert_eq!(buffer.size(), 1);
}

// "support contains, add, append and remove"
#[test]
fn message_buffer_map_supports_contains_add_append_and_remove() {
  let mut map: BufferMap = MessageBufferMap::new();
  assert!(!map.contains(&"id1"));
  assert!(map.get(&"id1").map_or(true, |buffer| buffer.is_empty()));
  assert_eq!(map.total_size(), 0);
  map.add("id1");
  assert!(map.contains(&"id1"));
  assert!(map.get(&"id1").map_or(true, |buffer| buffer.is_empty()));
  assert_eq!(map.total_size(), 0);
  map.append("id1", "m1", "s1");
  assert!(map.contains(&"id1"));
  assert!(map.get(&"id1").map_or(false, |buffer| !buffer.is_empty()));
  assert_eq!(map.total_size(), 1);
  map.remove(&"id1");
  assert!(!map.contains(&"id1"));
  assert!(map.get(&"id1").map_or(true, |buffer| buffer.is_empty()));
  assert_eq!(map.total_size(), 0);
}

// "handle multiple message buffers"
#[test]
fn message_buffer_map_handles_multiple_message_buffers() {
  let mut map: BufferMap = MessageBufferMap::new();
  map.append("id1", "m11", "s11");
  map.append("id1", "m12", "s12");
  map.append("id2", "m21", "s21");
  map.append("id2", "m22", "s22");
  assert_eq!(map.total_size(), 4);
  let mut out = String::new();
  if let Some(buffer) = map.get(&"id1") {
    buffer.for_each(|message, sender| out.push_str(&format!("id1->{message}->{sender}:")));
  }
  if let Some(buffer) = map.get(&"id2") {
    buffer.for_each(|message, sender| out.push_str(&format!("id2->{message}->{sender}:")));
  }
  assert_eq!(out, "id1->m11->s11:id1->m12->s12:id2->m21->s21:id2->m22->s22:");
}
