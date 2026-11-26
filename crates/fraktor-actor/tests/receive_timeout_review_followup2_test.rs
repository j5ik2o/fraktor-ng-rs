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
// second follow-up: a clock switch must keep the remaining time of a
// scheduled timeout, a setting changed to `Undefined` must leave no
// scheduled firing behind, losing the clock after a timeout was set must
// not panic creation, a stopping actor must not fire its timeout into
// dead letters, and a child-terminated hook that stops its own actor must
// not re-arm its timeout.

use core::time::Duration;

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use fraktor_actor::adaptor::{ChannelBoundary, UnwindCatcher};
use fraktor_actor::kernel::actor::{
  Actor, ActorContext, DeadLetter, Identify, Props, ReceiveTimeout, ReceiveTimeoutSetting,
};
use fraktor_actor::kernel::actor_ref::ActorRef;
use fraktor_actor::kernel::actor_system::ActorSystem;
use fraktor_actor::kernel::event::EventClass;
use fraktor_actor::kernel::executor::ShardBoundary;
use fraktor_actor::kernel::message::{Envelope, Message};
use fraktor_actor::kernel::supervision::SupervisorStrategy;
use fraktor_actor::kernel::Clock;
use fraktor_testkit::fraktorspec::FraktorSpec;
use fraktor_testkit::test_kit::TestProbe;

const SYSTEM_NAME: &str = "ReceiveTimeoutReviewFollowup2";

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

type Deliveries = Arc<Mutex<Vec<Duration>>>;

struct Empty;

impl Actor for Empty {
  fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {}
}

fn empty_props() -> Props {
  Props::new(|| Empty)
}

// An actor that records the `ReceiveTimeout` messages it receives and
// sets a one-second timeout when it starts.
struct Recorder {
  deliveries: Deliveries,
}

impl Actor for Recorder {
  fn pre_start(&mut self, context: &mut ActorContext<'_>) {
    context.set_receive_timeout(ReceiveTimeoutSetting::Finite(Duration::from_secs(1)));
  }

  fn receive(&mut self, _context: &mut ActorContext<'_>, message: Message) {
    if let Some(timeout) = message.downcast_ref::<ReceiveTimeout>() {
      self.deliveries.lock().unwrap().push(timeout.timeout());
    }
  }
}

// Switching the clock must not change how much inactivity time is left:
// a deadline measured on the old clock keeps its remaining time on the
// new clock, instead of being read as a point on the new clock.
#[test]
fn clock_switch_keeps_the_remaining_time_of_a_scheduled_timeout() {
  let mut spec = FraktorSpec::new(SYSTEM_NAME);
  let first = ControlledClock::new(Duration::from_secs(10));
  spec.system().set_clock(Arc::new(first));
  let deliveries: Deliveries = Arc::new(Mutex::new(Vec::new()));
  let _actor = spec.system_mut().actor_of(Props::new({
    let deliveries = Arc::clone(&deliveries);
    move || Recorder { deliveries: Arc::clone(&deliveries) }
  }));
  // The deadline is 11s on the first clock. The second clock already
  // reads 100s, far past that point as a bare reading.
  let second = ControlledClock::new(Duration::from_secs(100));
  spec.system().set_clock(Arc::new(second.clone()));
  spec.system_mut().run_until_idle();
  assert!(deliveries.lock().unwrap().is_empty(), "one second has not passed on the new clock yet");
  second.advance(Duration::from_secs(1));
  spec.system_mut().run_until_idle();
  assert_eq!(deliveries.lock().unwrap().as_slice(), &[Duration::from_secs(1)], "the timeout fires after its second");
  spec.terminate();
}

// A parent that sets a 100ms timeout and starts a child on its first
// creation. On `Identify` (which does not influence the timeout) it
// fails; its `pre_restart` cancels the timeout and stops the child.
// The restart carries the child's stop out at once; the child has a
// grandchild and a child-terminated hook that advances the clock past
// the parent's deadline, so the hook's end runs the executor's firing
// step while the parent's cancelled setting is already in place.
struct FailingParent {
  deliveries: Deliveries,
  child_started: Arc<AtomicBool>,
  child_slot: Arc<Mutex<Option<ActorRef>>>,
  clock: ControlledClock,
}

impl Actor for FailingParent {
  fn pre_start(&mut self, context: &mut ActorContext<'_>) {
    context.set_receive_timeout(ReceiveTimeoutSetting::Finite(Duration::from_millis(100)));
    if !self.child_started.swap(true, Ordering::SeqCst) {
      let child = context.actor_of(middle_props(self.clock.clone()), "middle").expect("the child starts");
      *self.child_slot.lock().unwrap() = Some(child);
    }
  }

  fn pre_restart(&mut self, context: &mut ActorContext<'_>, _reason: &(dyn core::any::Any + Send)) {
    context.set_receive_timeout(ReceiveTimeoutSetting::Undefined);
    if let Some(child) = self.child_slot.lock().unwrap().clone() {
      context.stop(&child);
    }
  }

  fn receive(&mut self, _context: &mut ActorContext<'_>, message: Message) {
    if let Some(timeout) = message.downcast_ref::<ReceiveTimeout>() {
      self.deliveries.lock().unwrap().push(timeout.timeout());
      return;
    }
    if message.downcast_ref::<Identify>().is_some() {
      panic!("boom");
    }
  }
}

// The middle child: its hook advances the clock past the parent's
// deadline, so the parent's old firing comes due inside the cascade
// that the hook's end then runs the firing step of.
struct Middle {
  clock: ControlledClock,
}

impl Actor for Middle {
  fn pre_start(&mut self, context: &mut ActorContext<'_>) {
    let _grandchild = context.actor_of(empty_props(), "grandchild").expect("the grandchild starts");
  }

  fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {}

  fn supervisor_strategy(&self) -> SupervisorStrategy {
    let clock = self.clock.clone();
    SupervisorStrategy::one_for_one(3).with_child_terminated(move |_context, _child, _children| {
      clock.advance(Duration::from_millis(200));
    })
  }
}

fn middle_props(clock: ControlledClock) -> Props {
  Props::new(move || Middle { clock: clock.clone() })
}

#[test]
fn setting_undefined_leaves_no_scheduled_firing_behind() {
  let mut spec = FraktorSpec::new(SYSTEM_NAME);
  let clock = ControlledClock::new(Duration::from_secs(10));
  spec.system().set_clock(Arc::new(clock.clone()));
  let deliveries: Deliveries = Arc::new(Mutex::new(Vec::new()));
  let parent = spec.system_mut().actor_of(Props::new({
    let deliveries = Arc::clone(&deliveries);
    let clock = clock.clone();
    move || FailingParent {
      deliveries: Arc::clone(&deliveries),
      child_started: Arc::new(AtomicBool::new(false)),
      child_slot: Arc::new(Mutex::new(None)),
      clock: clock.clone(),
    }
  }));
  let no_sender = spec.system().dead_letters();
  parent.tell(Identify::new(Option::<()>::None), no_sender);
  spec.system_mut().run_until_idle();
  assert!(
    deliveries.lock().unwrap().is_empty(),
    "the firing scheduled before the setting changed to Undefined must not be delivered"
  );
  spec.terminate();
}

// A boundary whose monotonic time can be switched off after it was read.
struct FadingBoundary {
  transport: ChannelBoundary,
  clock_on: Arc<AtomicBool>,
}

impl ShardBoundary for FadingBoundary {
  fn monotonic_time(&self) -> Option<Duration> {
    if self.clock_on.load(Ordering::SeqCst) {
      Some(Duration::from_secs(10))
    } else {
      None
    }
  }

  fn post(&self, envelope: Envelope) {
    self.transport.post(envelope);
  }

  fn poll(&self) -> Option<Envelope> {
    self.transport.poll()
  }

  fn poll_within(&self, max: Duration) -> Option<Envelope> {
    self.transport.poll_within(max)
  }

  fn has_pending(&self) -> bool {
    self.transport.has_pending()
  }

  fn close(&self) {
    self.transport.close();
  }
}

// An actor that sets a finite timeout in `pre_start` and then switches
// the boundary clock off, before creation completes and the executor
// checks the timeout it just set.
struct ClockFading {
  clock_on: Arc<AtomicBool>,
}

impl Actor for ClockFading {
  fn pre_start(&mut self, context: &mut ActorContext<'_>) {
    context.set_receive_timeout(ReceiveTimeoutSetting::Finite(Duration::from_secs(1)));
    self.clock_on.store(false, Ordering::SeqCst);
  }

  fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {}
}

#[test]
fn creation_completes_when_the_clock_disappears_after_the_timeout_is_set() {
  let clock_on = Arc::new(AtomicBool::new(true));
  let boundary = Arc::new(FadingBoundary { transport: ChannelBoundary::new(), clock_on: Arc::clone(&clock_on) });
  let mut system = ActorSystem::with_failure_catcher(SYSTEM_NAME, boundary, Arc::new(UnwindCatcher));
  fraktor_actor::adaptor::attach_wait_driver(&system);
  let actor = system.actor_of(Props::new({
    let clock_on = Arc::clone(&clock_on);
    move || ClockFading { clock_on: Arc::clone(&clock_on) }
  }));
  system.run_until_idle();
  assert!(!system.is_actor_terminated(&actor), "creation completes without a clock for the completion check");
  system.terminate();
}

// A child with a 100ms timeout, stopped from outside while its
// timeout is due. Its timeout state is cleared only at the end of the
// stop, after the parent's child-terminated hook has run; the hook's
// end runs the executor's firing step, which finds the child's entry
// still present and sends the timeout to the terminated child.
struct TimedChild {
  deliveries: Deliveries,
}

impl Actor for TimedChild {
  fn pre_start(&mut self, context: &mut ActorContext<'_>) {
    context.set_receive_timeout(ReceiveTimeoutSetting::Finite(Duration::from_millis(100)));
  }

  fn receive(&mut self, _context: &mut ActorContext<'_>, message: Message) {
    if let Some(timeout) = message.downcast_ref::<ReceiveTimeout>() {
      self.deliveries.lock().unwrap().push(timeout.timeout());
    }
  }
}

struct ChildHolder {
  child_slot: Arc<Mutex<Option<ActorRef>>>,
  child_deliveries: Deliveries,
}

impl Actor for ChildHolder {
  fn pre_start(&mut self, context: &mut ActorContext<'_>) {
    let child = context
      .actor_of(
        Props::new({
          let deliveries = Arc::clone(&self.child_deliveries);
          move || TimedChild { deliveries: Arc::clone(&deliveries) }
        }),
        "timed-child",
      )
      .expect("the child starts");
    *self.child_slot.lock().unwrap() = Some(child);
  }

  fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {}

  fn supervisor_strategy(&self) -> SupervisorStrategy {
    SupervisorStrategy::one_for_one(3).with_child_terminated(|_context, _child, _children| {})
  }
}

#[test]
fn stopping_an_actor_does_not_fire_its_due_timeout_into_dead_letters() {
  let mut spec = FraktorSpec::new(SYSTEM_NAME);
  let clock = ControlledClock::new(Duration::from_secs(10));
  spec.system().set_clock(Arc::new(clock.clone()));
  let mut dead_listener = TestProbe::new(spec.system_mut());
  spec
    .system()
    .event_stream()
    .subscribe(Some(dead_listener.ref_handle()), EventClass::of::<DeadLetter>())
    .expect("the probe is not null");
  let child_slot: Arc<Mutex<Option<ActorRef>>> = Arc::new(Mutex::new(None));
  let child_deliveries: Deliveries = Arc::new(Mutex::new(Vec::new()));
  let _parent = spec.system_mut().actor_of(Props::new({
    let child_slot = Arc::clone(&child_slot);
    let child_deliveries = Arc::clone(&child_deliveries);
    move || ChildHolder { child_slot: Arc::clone(&child_slot), child_deliveries: Arc::clone(&child_deliveries) }
  }));
  // Creation is asynchronous since stage 4a (#234, section 11.12): the
  // parent's `pre_start`, which starts and reports the child, runs when
  // the dispatcher turns, not inside `actor_of`.
  spec.system_mut().run_until_idle();
  let child = child_slot.lock().unwrap().clone().expect("the child was reported");
  clock.advance(Duration::from_millis(200));
  spec.system_mut().stop(&child);
  spec.system_mut().run_until_idle();
  assert!(spec.system().is_actor_terminated(&child), "the child stopped");
  assert!(child_deliveries.lock().unwrap().is_empty(), "the due timeout was not delivered to the child");
  dead_listener.expect_no_message(Duration::from_millis(300));
  spec.terminate();
}

// A parent whose child-terminated hook sets a zero timeout and stops
// the parent itself: the pending stop wins, as it does for a message
// handling, and no timeout is armed or fired on the way out.
struct SelfStoppingHookParent {
  deliveries: Deliveries,
}

impl Actor for SelfStoppingHookParent {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    if let Some(timeout) = message.downcast_ref::<ReceiveTimeout>() {
      self.deliveries.lock().unwrap().push(timeout.timeout());
      return;
    }
    if message.downcast_ref::<&str>() == Some(&"engage") {
      let child = context.actor_of(empty_props(), "child").expect("the child starts");
      context.stop(&child);
    }
  }

  fn supervisor_strategy(&self) -> SupervisorStrategy {
    SupervisorStrategy::one_for_one(3).with_child_terminated(|context, _child, _children| {
      context.set_receive_timeout(ReceiveTimeoutSetting::Finite(Duration::ZERO));
      let me = context.self_ref();
      context.stop(&me);
    })
  }
}

#[test]
fn child_terminated_hook_that_stops_its_actor_does_not_arm_a_timeout() {
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
    move || SelfStoppingHookParent { deliveries: Arc::clone(&deliveries) }
  }));
  let no_sender = spec.system().dead_letters();
  parent.tell("engage", no_sender);
  spec.system_mut().run_until_idle();
  assert!(spec.system().is_actor_terminated(&parent), "the hook's stop took effect");
  assert!(deliveries.lock().unwrap().is_empty(), "no timeout was delivered");
  dead_listener.expect_no_message(Duration::from_millis(300));
  spec.terminate();
}
