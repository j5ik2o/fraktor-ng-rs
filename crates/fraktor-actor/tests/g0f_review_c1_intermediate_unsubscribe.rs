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

// Reproduction for review thread C1:
//   after unsubscribing an intermediate channel, a not-yet-created child channel
//   revives the subscription.
//
// The bot example (Devin event_stream_bus.rs:244 / Cursor :194 / CodeRabbit
// :185) says: declare `Grandchild -> Child -> Parent`, subscribe to `Parent`,
// unsubscribe from `Child`, then publish `Grandchild` for the first time. The
// claim is that creating the grandchild's channel inherits from every ancestor
// still holding the subscriber, so the grandchild is delivered to the
// subscriber that was removed from `Child`. Pekko's `SubclassifiedIndex` places
// a new subclass under the most specific known super-class and copies only its
// subscribers, so the removed subscription does not come back.
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Grandchild;

fn wait() -> Duration {
  Duration::from_millis(300)
}

#[test]
fn g0f_review_c1_unsubscribed_intermediate_channel_is_not_revived() {
  let mut spec = FraktorSpec::new("G0fReviewC1");
  let bus = EventStream::new(spec.system().downgrade(), false);
  bus.declare_subclass::<Child, Parent>();
  bus.declare_subclass::<Grandchild, Child>();
  let mut probe = TestProbe::new(spec.system_mut());

  // Subscribe to the parent, then remove the intermediate channel from it.
  assert!(bus.subscribe(Some(probe.ref_handle()), EventClass::of::<Parent>()).expect("the probe is not null"));
  assert!(bus.unsubscribe(Some(probe.ref_handle()), EventClass::of::<Child>()).expect("the probe is not null"));

  // The grandchild's channel is created on this first publish. It must inherit
  // from `Child` (the most specific known super-class), not from `Parent`, so
  // the removed subscription does not come back.
  bus.publish(Grandchild);

  probe.expect_no_message(wait());
  spec.terminate();
}
