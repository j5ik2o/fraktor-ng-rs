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

// Reproduction for review thread C3:
//   a subscription that another parent path still holds disappears when one
//   path is unsubscribed.
//
// The bot example (CodeRabbit event_stream_bus.rs:242) says: declare `Child` a
// subclass of both `Left` and `Right`; subscribe the same subscriber to both
// `Left` and `Right`; publish `Child` once; then unsubscribe from `Left`. The
// claim is that unsubscribing removes the subscriber from the descendant
// channels unconditionally, so `Child` stops reaching it even though `Right`
// still holds the subscription. Pekko's `removeValue` re-derives each changed
// key's values from all paths (`mergeChangesByKey(...).map { case (k, _) =>
// (k, findValues(k)) }`), so `Child` keeps the subscriber through `Right`.
//
// This test checks only that claim. It takes the first delivery off the queue
// before it unsubscribes, so the observation is about the publication after the
// removal and not about the earlier one.

use core::time::Duration;

use fraktor_actor::kernel::event::{EventClass, EventStream};
use fraktor_testkit::fraktorspec::FraktorSpec;
use fraktor_testkit::test_kit::TestProbe;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Left;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Right;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Child;

fn wait() -> Duration {
  Duration::from_secs(1)
}

#[test]
fn g0f_review_c3_subscription_held_by_another_parent_path_survives() {
  let mut spec = FraktorSpec::new("G0fReviewC3");
  let bus = EventStream::new(spec.system().downgrade(), false);
  bus.declare_subclass::<Child, Left>();
  bus.declare_subclass::<Child, Right>();
  let mut probe = TestProbe::new(spec.system_mut());

  // The same subscriber holds both parent subscriptions.
  let subscriber = probe.ref_handle();
  assert!(bus.subscribe(Some(subscriber.clone()), EventClass::of::<Left>()).expect("the probe is not null"));
  assert!(bus.subscribe(Some(subscriber.clone()), EventClass::of::<Right>()).expect("the probe is not null"));

  // Publish the child once, so its channel exists with the subscriber on it,
  // and take that first delivery off the queue.
  bus.publish(Child);
  let first: Child = probe.fish_for_specific_message(wait(), |message| message.downcast_ref::<Child>().copied());
  assert_eq!(first, Child);

  // Remove only the left path; the right path still holds the subscription, so
  // the child must still reach the subscriber.
  assert!(bus.unsubscribe(Some(subscriber), EventClass::of::<Left>()).expect("the probe is not null"));
  bus.publish(Child);

  let second: Child = probe.fish_for_specific_message(wait(), |message| message.downcast_ref::<Child>().copied());
  assert_eq!(second, Child);
  spec.terminate();
}
