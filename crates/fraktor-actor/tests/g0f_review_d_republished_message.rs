// Reproduction for review thread D:
//   republishing an already-received `Message` does not reach the subscribers
//   of its payload type.
//
// The bot example (Devin event_stream_bus.rs:88) says: receive a `Message`
// whose payload is an event of a subscribed type, then pass that `Message`
// itself to `publish`. `Message::new` unwraps the nested `Message`, so the
// subscriber receives the payload value; the claim is that the recipients are
// chosen from `Message`'s own type instead of the payload's type, so a
// subscriber of the payload type never receives it.
//
// This test checks only that claim.

use core::time::Duration;

use fraktor_actor::kernel::event::{EventClass, EventStream};
use fraktor_actor::kernel::message::Message;
use fraktor_testkit::fraktorspec::FraktorSpec;
use fraktor_testkit::test_kit::TestProbe;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Event(i32);

fn wait() -> Duration {
  Duration::from_secs(1)
}

#[test]
fn g0f_review_d_republished_message_reaches_the_payload_type_subscriber() {
  let mut spec = FraktorSpec::new("G0fReviewD");
  let bus = EventStream::new(spec.system().downgrade(), false);
  let mut probe = TestProbe::new(spec.system_mut());

  assert!(bus.subscribe(Some(probe.ref_handle()), EventClass::of::<Event>()).expect("the probe is not null"));

  // An already-received message whose payload is an `Event`.
  let received = Message::new(Event(7));

  // Publishing the `Message` itself must reach the `Event` subscriber, because
  // `Message::new` unwraps it to the payload.
  bus.publish(received);

  let event: Event = probe.fish_for_specific_message(wait(), |message| message.downcast_ref::<Event>().copied());
  assert_eq!(event, Event(7));
  spec.terminate();
}
