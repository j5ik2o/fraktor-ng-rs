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

// Regression tests for the receive timeout of ReceiveTimeoutSpec (#200),
// covering paths the ported spec does not reach: a wait on a hand-driven
// clock, a setting changed from a child-terminated hook, and a stop that
// the actor asks for in the handling that sets a zero timeout.

use core::time::Duration;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use fraktor_actor::kernel::actor::{Actor, ActorContext, DeadLetter, Props, ReceiveTimeout, ReceiveTimeoutSetting};
use fraktor_actor::kernel::actor_ref::ActorRef;
use fraktor_actor::kernel::event::EventClass;
use fraktor_actor::kernel::message::Message;
use fraktor_actor::kernel::supervision::SupervisorStrategy;
use fraktor_actor::kernel::Clock;
use fraktor_testkit::fraktorspec::FraktorSpec;
use fraktor_testkit::test_kit::TestProbe;

const SYSTEM_NAME: &str = "ReceiveTimeoutReviewFollowup";

// A hand-driven monotonic clock, in nanoseconds. It never advances unless
// the test advances it.
#[derive(Clone)]
struct ControlledClock(Arc<AtomicU64>);

impl ControlledClock {
  fn new(start: Duration) -> Self {
    Self(Arc::new(AtomicU64::new(start.as_nanos() as u64)))
  }

  fn advance(&self, by: Duration) {
    self.0.fetch_add(by.as_nanos() as u64, Ordering::SeqCst);
  }
}

impl Clock for ControlledClock {
  fn monotonic(&self) -> Duration {
    Duration::from_nanos(self.0.load(Ordering::SeqCst))
  }
}

// A wait for work on an empty system whose clock never advances must still
// end when the real wait is over: the wait is bounded by `max`, not by the
// hand-driven clock.
#[test]
fn await_envelope_returns_when_hand_driven_clock_never_advances() {
  let mut spec = FraktorSpec::new(SYSTEM_NAME);
  let clock = ControlledClock::new(Duration::from_secs(10));
  spec.system().set_clock(Arc::new(clock));
  let started = Instant::now();
  let arrived = spec.system_mut().await_envelope(Duration::from_millis(200));
  assert!(!arrived, "no work came");
  assert!(started.elapsed() < Duration::from_secs(5), "the wait must end shortly after its bound");
  spec.terminate();
}

type Deliveries = Arc<Mutex<Vec<Duration>>>;

struct Child;

impl Actor for Child {
  fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {}
}

fn empty_child_props() -> Props {
  Props::new(|| Child)
}

// A parent that sets a one-second timeout when it starts and stops its
// child on "engage". Its child-terminated hook applies `hook_setting`.
struct HookParent {
  hook_setting: ReceiveTimeoutSetting,
  deliveries: Deliveries,
}

impl Actor for HookParent {
  fn pre_start(&mut self, context: &mut ActorContext<'_>) {
    context.set_receive_timeout(ReceiveTimeoutSetting::Finite(Duration::from_secs(1)));
  }

  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    if let Some(timeout) = message.downcast_ref::<ReceiveTimeout>() {
      self.deliveries.lock().unwrap().push(timeout.timeout());
      return;
    }
    if message.downcast_ref::<&str>() == Some(&"engage") {
      let child = context.actor_of(empty_child_props(), "child").expect("the child starts");
      context.stop(&child);
    }
  }

  fn supervisor_strategy(&self) -> SupervisorStrategy {
    let hook_setting = self.hook_setting;
    SupervisorStrategy::one_for_one(3).with_child_terminated(move |context, _child, _children| {
      context.set_receive_timeout(hook_setting);
    })
  }
}

fn hook_parent(hook_setting: ReceiveTimeoutSetting) -> (FraktorSpec, ControlledClock, ActorRef, Deliveries) {
  let mut spec = FraktorSpec::new(SYSTEM_NAME);
  let clock = ControlledClock::new(Duration::from_secs(10));
  spec.system().set_clock(Arc::new(clock.clone()));
  let deliveries: Deliveries = Arc::new(Mutex::new(Vec::new()));
  let parent = spec.system_mut().actor_of(Props::new({
    let deliveries = Arc::clone(&deliveries);
    move || HookParent { hook_setting, deliveries: Arc::clone(&deliveries) }
  }));
  (spec, clock, parent, deliveries)
}

// A setting the hook shortens must take effect at once: the new deadline
// is what fires, not the deadline the replaced setting had scheduled.
#[test]
fn child_terminated_hook_change_of_timeout_takes_effect() {
  let (mut spec, clock, parent, deliveries) = hook_parent(ReceiveTimeoutSetting::Finite(Duration::from_millis(100)));
  let no_sender = spec.system().dead_letters();
  parent.tell("engage", no_sender);
  spec.system_mut().run_until_idle();
  clock.advance(Duration::from_millis(200));
  spec.system_mut().run_until_idle();
  assert_eq!(
    deliveries.lock().unwrap().as_slice(),
    &[Duration::from_millis(100)],
    "the hook's 100ms setting fires after 200ms, not the replaced 1s setting"
  );
  spec.terminate();
}

// A setting the hook cancels must take effect at once: the deadline the
// cancelled setting had scheduled must not fire.
#[test]
fn child_terminated_hook_cancel_of_timeout_takes_effect() {
  let (mut spec, clock, parent, deliveries) = hook_parent(ReceiveTimeoutSetting::Undefined);
  let no_sender = spec.system().dead_letters();
  parent.tell("engage", no_sender);
  spec.system_mut().run_until_idle();
  clock.advance(Duration::from_secs(2));
  spec.system_mut().run_until_idle();
  assert!(deliveries.lock().unwrap().is_empty(), "the cancelled 1s deadline must not fire");
  spec.terminate();
}

// An actor that sets a zero timeout and stops itself in the same handling:
// the stop wins, and no `ReceiveTimeout` is fired into the closing mailbox
// to be routed to dead letters.
struct SelfStopping;

impl Actor for SelfStopping {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    if message.downcast_ref::<&str>() == Some(&"go") {
      context.set_receive_timeout(ReceiveTimeoutSetting::Finite(Duration::ZERO));
      let me = context.self_ref();
      context.stop(&me);
    }
  }
}

#[test]
fn self_stop_wins_over_zero_timeout_set_in_same_handling() {
  let mut spec = FraktorSpec::new(SYSTEM_NAME);
  let mut dead_listener = TestProbe::new(spec.system_mut());
  spec
    .system()
    .event_stream()
    .subscribe(Some(dead_listener.ref_handle()), EventClass::of::<DeadLetter>())
    .expect("the probe is not null");
  let actor = spec.system_mut().actor_of(Props::new(|| SelfStopping));
  let no_sender = spec.system().dead_letters();
  actor.tell("go", no_sender);
  spec.system_mut().run_until_idle();
  dead_listener.expect_no_message(Duration::from_millis(300));
  spec.terminate();
}
