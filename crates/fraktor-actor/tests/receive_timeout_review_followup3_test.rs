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

// Regression test for the receive timeout of ReceiveTimeoutSpec (#200):
// a parent that is stopped from outside waits for its children with its
// mailbox suspended, and a child-terminated hook that runs in that wait
// must not arm a timeout on the stopping parent.

use core::time::Duration;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use fraktor_actor::kernel::actor::{Actor, ActorContext, DeadLetter, Props, ReceiveTimeout, ReceiveTimeoutSetting};
use fraktor_actor::kernel::event::EventClass;
use fraktor_actor::kernel::message::Message;
use fraktor_actor::kernel::supervision::SupervisorStrategy;
use fraktor_actor::kernel::Clock;
use fraktor_testkit::fraktorspec::FraktorSpec;
use fraktor_testkit::test_kit::TestProbe;

const SYSTEM_NAME: &str = "ReceiveTimeoutReviewFollowup3";

// A hand-driven monotonic clock, in nanoseconds. It never advances unless
// the test advances it.
#[derive(Clone)]
struct ControlledClock(Arc<AtomicU64>);

impl ControlledClock {
  fn new(start: Duration) -> Self {
    Self(Arc::new(AtomicU64::new(start.as_nanos() as u64)))
  }
}

impl Clock for ControlledClock {
  fn monotonic(&self) -> Duration {
    Duration::from_nanos(self.0.load(Ordering::SeqCst))
  }
}

type Deliveries = Arc<Mutex<Vec<Duration>>>;

struct Child;

impl Actor for Child {
  fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {}
}

// A parent that starts one child when it starts. Its child-terminated
// hook sets a zero timeout on the parent itself.
struct HookSetsZeroParent {
  deliveries: Deliveries,
}

impl Actor for HookSetsZeroParent {
  fn pre_start(&mut self, context: &mut ActorContext<'_>) {
    let _child = context.actor_of(Props::new(|| Child), "child").expect("the child starts");
  }

  fn receive(&mut self, _context: &mut ActorContext<'_>, message: Message) {
    if let Some(timeout) = message.downcast_ref::<ReceiveTimeout>() {
      self.deliveries.lock().unwrap().push(timeout.timeout());
    }
  }

  fn supervisor_strategy(&self) -> SupervisorStrategy {
    SupervisorStrategy::one_for_one(3).with_child_terminated(|context, _child, _children| {
      context.set_receive_timeout(ReceiveTimeoutSetting::Finite(Duration::ZERO));
    })
  }
}

// The parent is stopped from outside: it waits for its child, the hook
// runs in that wait and sets a zero timeout, and the stop finishes. The
// timeout the hook set must neither be delivered to the parent nor be
// routed to dead letters when the closing mailbox is drained.
#[test]
fn hook_timeout_set_while_parent_stops_does_not_reach_dead_letters() {
  let mut spec = FraktorSpec::new(SYSTEM_NAME);
  let clock = ControlledClock::new(Duration::from_secs(10));
  spec.system().set_clock(Arc::new(clock));
  let mut dead_listener = TestProbe::new(spec.system_mut());
  spec
    .system()
    .event_stream()
    .subscribe(Some(dead_listener.ref_handle()), EventClass::of::<DeadLetter>())
    .expect("the probe is not null");
  let deliveries: Deliveries = Arc::new(Mutex::new(Vec::new()));
  let parent = spec.system_mut().actor_of(Props::new({
    let deliveries = Arc::clone(&deliveries);
    move || HookSetsZeroParent { deliveries: Arc::clone(&deliveries) }
  }));
  spec.system_mut().run_until_idle();
  spec.system_mut().stop(&parent);
  spec.system_mut().run_until_idle();
  assert!(spec.system().is_actor_terminated(&parent), "the parent stopped");
  assert!(deliveries.lock().unwrap().is_empty(), "no timeout was delivered to the stopping parent");
  dead_listener.expect_no_message(Duration::from_millis(300));
  spec.terminate();
}
