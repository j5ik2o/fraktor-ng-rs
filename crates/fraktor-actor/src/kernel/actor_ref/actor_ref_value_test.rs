//! Tests for the actor identity primitives and [`ActorRef`](super::ActorRef).

use alloc::sync::Arc;

use crate::adaptor::ChannelBoundary;
use crate::kernel::actor_path::ActorPath;

use super::ActorId;
use super::ActorRef;
use super::Generation;

fn path() -> Arc<ActorPath> {
  Arc::new(ActorPath::from_string("pekko://test/user/target").expect("a valid path"))
}

fn handle(id: u64, generation: u32) -> ActorRef {
  ActorRef::new(ActorId::new(id), Generation::new(generation), path(), Arc::new(ChannelBoundary::new()))
}

#[test]
fn actor_id_keeps_a_counter_value() {
  assert_eq!(ActorId::new(7).value(), 7);
}

#[test]
fn generation_keeps_a_counter_value() {
  assert_eq!(Generation::new(2).value(), 2);
}

#[test]
fn a_zero_id_cannot_be_confused_with_a_fresh_handle() {
  let first = handle(0, 0);
  let second = handle(1, 0);

  assert_ne!(first, second, "handles with distinct identifiers compared equal");
}

#[test]
fn handles_with_the_same_id_and_generation_compare_equal() {
  assert_eq!(handle(3, 1), handle(3, 1));
}

#[test]
fn handles_with_the_same_id_but_a_different_generation_compare_unequal() {
  assert_ne!(handle(3, 1), handle(3, 2), "the generation did not take part in comparing handles");
}

#[test]
fn a_cloned_handle_equals_the_original() {
  let original = handle(4, 0);

  assert_eq!(original.clone(), original);
}

#[test]
fn the_display_form_of_a_handle_is_unchanged() {
  assert_eq!(format!("{:?}", handle(5, 0)), "ActorRef(5, generation 0)");
}
