// SPDX-License-Identifier: Apache-2.0
//
// Licensed to the Apache Software Foundation (ASF) under one or more
// license agreements; and to You under the Apache License, version 2.0:
//
//   https://www.apache.org/licenses/LICENSE-2.0
//
// This file is part of the Apache Pekko project, which was derived from Akka.
//
// Copyright (C) 2009-2022 Lightbend Inc. <https://www.lightbend.com>
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.

// Regression for the descendant path of a late-declared hierarchy:
//   a subclass link declared after a descendant channel already exists must
//   reach that descendant through its most specific known super-class.
//
// The child's channel is created by a first publish while no link is known, so
// it inherits nothing. `Middle` is declared a subclass of `A`, `Child` is
// published once (creating its channel empty), and a probe subscribes to `A`.
// Declaring `Child` a subclass of `Middle` afterwards must re-derive `Child`
// from its most specific known super-class: `Middle`, which inherits the probe
// from `A`. Publishing `Child` therefore reaches the probe on the channel that
// the earlier publish already created. The previous `propagate_subclass_link`
// did not reach that channel, so this test fails against it.
//
// The second test pairs that delivery with the negative case: the probe is
// removed from `Middle` itself, the channel `Child` inherits from, so the late
// declaration must leave `Child` undelivered. This pairs "late declaration
// reaches `Child`" with "a subscription absent from its direct parent does not
// reach it".
//
// Pekko's `SubclassifiedIndex.innerAddKey` integrates the new key under the
// most specific known super-class and returns the diff to apply to the cache;
// its `addValue` recurses into the sub-keys so the new ancestry reaches them.

use core::time::Duration;

use fraktor_actor::kernel::event::{EventClass, EventStream};
use fraktor_testkit::fraktorspec::FraktorSpec;
use fraktor_testkit::test_kit::TestProbe;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct A;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Middle;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Child;

fn wait() -> Duration {
  Duration::from_secs(1)
}

#[test]
fn g0f_review_c2_late_hierarchy_reaches_an_existing_descendant() {
  let mut spec = FraktorSpec::new("G0fReviewC2Descendant");
  let bus = EventStream::new(spec.system().downgrade(), false);
  bus.declare_subclass::<Middle, A>();
  let mut probe = TestProbe::new(spec.system_mut());

  // The child's channel exists before the hierarchy reaches it.
  bus.publish(Child);
  probe.expect_no_message(wait());

  // `Middle` inherits the probe from `A`, and `Child` must inherit it from
  // `Middle` when the link is declared after the child's channel exists.
  let subscriber = probe.ref_handle();
  assert!(bus.subscribe(Some(subscriber.clone()), EventClass::of::<A>()).expect("the probe is not null"));
  bus.declare_subclass::<Child, Middle>();

  bus.publish(Child);
  let received: Child = probe.fish_for_specific_message(wait(), |message| message.downcast_ref::<Child>().copied());
  assert_eq!(received, Child);

  assert!(bus.unsubscribe(Some(subscriber), EventClass::of::<A>()).expect("the probe is not null"));
  spec.terminate();
}

#[test]
fn g0f_review_c2_late_hierarchy_does_not_reach_through_a_removed_intermediate() {
  let mut spec = FraktorSpec::new("G0fReviewC2DescendantRemoved");
  let bus = EventStream::new(spec.system().downgrade(), false);
  bus.declare_subclass::<Middle, A>();
  let mut probe = TestProbe::new(spec.system_mut());

  // The child's channel exists before the hierarchy reaches it.
  bus.publish(Child);
  probe.expect_no_message(wait());

  // `A` holds the probe and `Middle` inherits it. Remove the probe from
  // `Middle`, the intermediate channel `Child` inherits from.
  let subscriber = probe.ref_handle();
  assert!(bus.subscribe(Some(subscriber.clone()), EventClass::of::<A>()).expect("the probe is not null"));
  assert!(bus.unsubscribe(Some(subscriber.clone()), EventClass::of::<Middle>()).expect("the probe is not null"));

  bus.declare_subclass::<Child, Middle>();

  bus.publish(Child);
  probe.expect_no_message(wait());

  assert!(bus.unsubscribe(Some(subscriber), EventClass::of::<A>()).expect("the probe is not null"));
  spec.terminate();
}
