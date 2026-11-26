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

// Reproduction for issue #73:
//   a subclass link declared after the parent's channel already exists is not
//   reflected in that channel's subscribers.
//
// The child's channel already exists before the hierarchy is declared: a
// publish of `Child` creates it while no subclass link is known. Subscribe a
// subscriber to `Parent`, so the parent's channel records it. Then declare
// `Child` a subclass of `Parent`, after both channels already exist. Publishing
// `Child` must reach the parent's subscriber, because Pekko's
// `SubclassifiedIndex.addKey` / `innerAddKey` integrates the new child under the
// most specific known super-class and returns the diff to add to the cache, so
// the existing parent channel's values propagate into the already-existing
// child channel.
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

  // Let the child's channel exist first, while no subclass link is known.
  bus.publish(Child);
  probe.expect_no_message(wait());

  // The parent's channel exists with the subscriber on it.
  let subscriber = probe.ref_handle();
  assert!(bus.subscribe(Some(subscriber.clone()), EventClass::of::<Parent>()).expect("the probe is not null"));

  // Declare the hierarchy after both channels already exist.
  bus.declare_subclass::<Child, Parent>();

  // The late-declared hierarchy must reach the parent's subscriber through the
  // already-existing child channel.
  bus.publish(Child);
  let received: Child = probe.fish_for_specific_message(wait(), |message| message.downcast_ref::<Child>().copied());
  assert_eq!(received, Child);

  assert!(bus.unsubscribe(Some(subscriber), EventClass::of::<Parent>()).expect("the probe is not null"));
  spec.terminate();
}
