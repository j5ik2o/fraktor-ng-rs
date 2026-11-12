//! Unit tests for [`TestKit`](super::TestKit).

use std::sync::Arc;

use fraktor_actor::adaptor::ChannelBoundary;
use fraktor_actor::kernel::actor_system::ActorSystem;

use super::TestKit;

fn test_kit() -> TestKit {
  TestKit::new(ActorSystem::new("TestKitSpec", Arc::new(ChannelBoundary::new())))
}

#[test]
fn expect_msg_returns_the_expected_message() {
  let mut kit = test_kit();
  let test_actor = kit.test_actor();
  test_actor.tell("hello", test_actor.clone());

  let received = kit.expect_msg("hello");

  assert_eq!(received, *"hello");
}

#[test]
#[should_panic(expected = "expected")]
fn expect_msg_fails_on_a_different_value() {
  let mut kit = test_kit();
  let test_actor = kit.test_actor();
  test_actor.tell("goodbye", test_actor.clone());

  kit.expect_msg("hello");
}

#[test]
#[should_panic(expected = "timeout")]
fn expect_msg_times_out_when_nothing_arrives() {
  let mut kit = test_kit();

  kit.expect_msg("hello");
}
