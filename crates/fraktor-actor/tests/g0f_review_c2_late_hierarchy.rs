// Reproduction for review thread C2:
//   a hierarchy declared after a channel exists is not applied to that channel.
//
// The bot example says: publish an event of a child class once (so its channel
// is created), then declare that the child is a subclass of the parent, then
// subscribe to the parent and publish the child again. Pekko's
// `SubclassifiedIndex.innerAddKey` adds the key under the most specific known
// super-class and returns the diff to the cache, so a later declaration still
// reaches the already-created channel. The claim is that the port records the
// link only and leaves the existing channel untouched, so the parent's
// subscriber never receives the child.
//
// This test checks only that claim.

use core::time::Duration;

use fraktor_actor::kernel::event::{EventClass, EventStream};
use fraktor_testkit::fraktorspec::FraktorSpec;
use fraktor_testkit::test_kit::TestProbe;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Parent;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Child;

fn wait() -> Duration {
  Duration::from_secs(1)
}

#[test]
fn g0f_review_c2_late_hierarchy_reaches_an_existing_channel() {
  let mut spec = FraktorSpec::new("G0fReviewC2");
  let bus = EventStream::new(spec.system().downgrade(), false);
  let mut probe = TestProbe::new(spec.system_mut());

  // S subscribes to the parent first, so the parent's channel holds S.
  assert!(bus.subscribe(Some(probe.ref_handle()), EventClass::of::<Parent>()).expect("the probe is not null"));

  // Publish the child once, creating its channel from the hierarchy known now
  // (the link does not exist yet).
  bus.publish(Child);

  // Declare the hierarchy only now, after both channels exist.
  bus.declare_subclass::<Child, Parent>();

  // Publish the child again; S, subscribed to the parent, must see it through
  // the declared link.
  bus.publish(Child);

  let received: Child = probe.fish_for_specific_message(wait(), |message| message.downcast_ref::<Child>().copied());
  assert_eq!(received, Child);
  spec.terminate();
}
