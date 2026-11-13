//! Unit tests for [`TestProbe`](super::TestProbe).

use std::panic::catch_unwind;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::time::Duration;

use fraktor_actor::adaptor::ChannelBoundary;
use fraktor_actor::kernel::actor_system::ActorSystem;
use fraktor_actor::kernel::message::Message;

use super::TestProbe;

fn failure_text(failure: Box<dyn core::any::Any + Send>) -> String {
  match failure.downcast::<String>() {
    | Ok(text) => *text,
    | Err(failure) => failure.downcast::<&'static str>().map(|text| text.to_string()).unwrap_or_default(),
  }
}

#[test]
fn fish_for_message_fails_when_no_message_matches_in_time() {
  let mut system = ActorSystem::new("TestProbeTest", Arc::new(ChannelBoundary::new()));
  let mut probe = TestProbe::new(&mut system);
  probe.ref_handle().tell("other", probe.ref_handle());

  let failure = catch_unwind(AssertUnwindSafe(|| {
    probe.fish_for_message(Duration::from_millis(50), |message: &Message| message == *"wanted");
  }))
  .expect_err("fish_for_message returned although nothing matched");

  assert!(failure_text(failure).contains("during fishForMessage"));
}

#[test]
fn fish_for_specific_message_fails_when_no_message_matches_in_time() {
  let mut system = ActorSystem::new("TestProbeTest", Arc::new(ChannelBoundary::new()));
  let mut probe = TestProbe::new(&mut system);
  probe.ref_handle().tell("other", probe.ref_handle());

  let failure = catch_unwind(AssertUnwindSafe(|| {
    probe.fish_for_specific_message(Duration::from_millis(50), |_: &Message| None::<()>);
  }))
  .expect_err("fish_for_specific_message returned although nothing matched");

  assert!(failure_text(failure).contains("during fishForSpecificMessage"));
}

#[test]
fn fish_for_message_updates_last_message() {
  let mut system = ActorSystem::new("TestProbeTest", Arc::new(ChannelBoundary::new()));
  let mut probe = TestProbe::new(&mut system);
  let other = TestProbe::new(&mut system);
  let other_ref = other.ref_handle();
  probe.ref_handle().tell("skip", probe.ref_handle());
  probe.ref_handle().tell("wanted", other_ref.clone());

  let found = probe.fish_for_message(Duration::from_millis(200), |message: &Message| message == *"wanted");

  assert_eq!(found, *"wanted");
  assert_eq!(
    probe.last_message().sender(),
    &other_ref,
    "fish_for_message did not record the sender of the message it accepted"
  );
}
