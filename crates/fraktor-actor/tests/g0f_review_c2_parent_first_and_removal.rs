// Regression for the two Codex follow-ups on `propagate_subclass_link`:
//   A. descendant channels are updated in insertion order, not parent-first,
//      so a deep descendant created before its parent reads the parent's
//      subscribers before the parent gained them and never receives them.
//   B. adding an unrelated parent link re-derives a child from every direct
//      parent, so a subscriber explicitly removed from the child comes back
//      through an older link that the new declaration did not touch.
//
// Pekko's `SubclassifiedIndex.innerAddKey` integrates the new key under its
// most specific known super-class and `addValue` recurses into the sub-keys;
// `removeValue` re-derives each changed key from the remaining paths, so a
// removal sticks unless a path that still holds the subscriber contributes it.

use core::time::Duration;

use fraktor_actor::kernel::event::{EventClass, EventStream};
use fraktor_testkit::fraktorspec::FraktorSpec;
use fraktor_testkit::test_kit::TestProbe;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Parent;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Child;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Middle;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Leaf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Other;

fn wait() -> Duration {
  Duration::from_secs(1)
}

#[test]
fn g0f_review_c2_late_hierarchy_reaches_deep_descendant_in_parent_first_order() {
  let mut spec = FraktorSpec::new("G0fReviewC2ParentFirst");
  let bus = EventStream::new(spec.system().downgrade(), false);
  bus.declare_subclass::<Leaf, Middle>();
  bus.declare_subclass::<Middle, Child>();
  let mut probe = TestProbe::new(spec.system_mut());

  // Create the channels with Leaf before Middle, so insertion order puts the
  // deep descendant ahead of its parent.
  bus.publish(Leaf);
  bus.publish(Middle);
  bus.publish(Child);
  probe.expect_no_message(wait());

  let subscriber = probe.ref_handle();
  assert!(bus.subscribe(Some(subscriber.clone()), EventClass::of::<Parent>()).expect("the probe is not null"));

  // The new link adds the subscribers to Child and directly to Middle and
  // Leaf as well, even though Leaf's channel was created before Middle's.
  bus.declare_subclass::<Child, Parent>();

  bus.publish(Leaf);
  let received: Leaf = probe.fish_for_specific_message(wait(), |message| message.downcast_ref::<Leaf>().copied());
  assert_eq!(received, Leaf);

  assert!(bus.unsubscribe(Some(subscriber), EventClass::of::<Parent>()).expect("the probe is not null"));
  spec.terminate();
}

#[test]
fn g0f_review_c2_unrelated_parent_link_does_not_revive_removal() {
  let mut spec = FraktorSpec::new("G0fReviewC2RemovalSticks");
  let bus = EventStream::new(spec.system().downgrade(), false);
  bus.declare_subclass::<Child, Parent>();
  let mut probe = TestProbe::new(spec.system_mut());

  let subscriber = probe.ref_handle();
  assert!(bus.subscribe(Some(subscriber.clone()), EventClass::of::<Parent>()).expect("the probe is not null"));

  // The probe reaches Child through Parent, then is removed from Child.
  bus.publish(Child);
  let received: Child = probe.fish_for_specific_message(wait(), |message| message.downcast_ref::<Child>().copied());
  assert_eq!(received, Child);
  assert!(bus.unsubscribe(Some(subscriber.clone()), EventClass::of::<Child>()).expect("the probe is not null"));
  bus.publish(Child);
  probe.expect_no_message(wait());

  // Adding an unrelated link that contributes no subscribers must not bring
  // the removed subscription back through the older Parent link.
  bus.declare_subclass::<Child, Other>();

  bus.publish(Child);
  probe.expect_no_message(wait());

  assert!(bus.unsubscribe(Some(subscriber), EventClass::of::<Parent>()).expect("the probe is not null"));
  spec.terminate();
}
