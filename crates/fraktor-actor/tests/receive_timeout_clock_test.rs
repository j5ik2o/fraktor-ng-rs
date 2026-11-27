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

// The clock that times the receive timeout of ReceiveTimeoutSpec (#200).
// Pekko's scheduler owns its clock; this port reads the clock installed with
// `ActorSystem::set_clock`, or the boundary's, and these cases cover what
// that adds: a clock switched while a timeout is scheduled, a clock the
// boundary stops reporting, and a wait for work on a clock driven by hand.
// The cases are outside ReceiveTimeoutSpec and are not counted in the ledger.

use core::time::Duration;

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;

use fraktor_actor::adaptor::{ChannelBoundary, UnwindCatcher};
use fraktor_actor::kernel::actor::{Actor, ActorContext, Props, ReceiveTimeout, ReceiveTimeoutSetting};
use fraktor_actor::kernel::actor_system::ActorSystem;
use fraktor_actor::kernel::dispatch::WakeSignal;
use fraktor_actor::kernel::executor::ShardBoundary;
use fraktor_actor::kernel::message::{Envelope, Message};
use fraktor_actor::kernel::Clock;
use fraktor_testkit::fraktorspec::FraktorSpec;

const SYSTEM_NAME: &str = "ReceiveTimeoutClockTest";

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

type Deliveries = Arc<Mutex<Vec<Duration>>>;

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
  spec.system_mut().run_until_idle();
  // The deadline is 11s on the first clock. The second clock already
  // reads 100s, far past that point as a bare reading.
  let second = ControlledClock::new(Duration::from_secs(100));
  spec.system().set_clock(Arc::new(second.clone()));
  spec.system_mut().run_until_idle();
  assert!(deliveries.lock().unwrap().is_empty(), "one second has not passed on the new clock yet");
  second.advance(Duration::from_millis(999));
  spec.system_mut().run_until_idle();
  assert!(deliveries.lock().unwrap().is_empty(), "the full second is kept, not shortened");
  second.advance(Duration::from_millis(1));
  spec.system_mut().run_until_idle();
  assert_eq!(deliveries.lock().unwrap().as_slice(), &[Duration::from_secs(1)], "the timeout fires after its second");
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

// Runs `wait` on its own thread and returns how long it took, or fails
// when it has not returned after `limit`. A wait that never returns must
// fail the test instead of hanging it, so the waiting thread is left
// unjoined.
fn bounded_wait<W>(limit: Duration, wait: W) -> (bool, Duration)
where
  W: FnOnce() -> bool + Send + 'static,
{
  let (sender, receiver) = mpsc::channel();
  thread::spawn(move || {
    let started = Instant::now();
    let arrived = wait();
    let _ = sender.send((arrived, started.elapsed()));
  });
  receiver.recv_timeout(limit).expect("the wait must end shortly after its bound, even on a hand-driven clock")
}

// A wait for work on an empty system whose clock never advances must still
// end when the real wait is over: the wait is bounded by `max`, not by the
// hand-driven clock.
#[test]
fn await_envelope_returns_when_hand_driven_clock_never_advances() {
  let (arrived, elapsed) = bounded_wait(Duration::from_secs(10), || {
    let mut spec = FraktorSpec::new(SYSTEM_NAME);
    spec.system().set_clock(Arc::new(ControlledClock::new(Duration::from_secs(10))));
    let arrived = spec.system_mut().await_envelope(Duration::from_millis(200));
    spec.terminate();
    arrived
  });
  assert!(!arrived, "no work came");
  assert!(elapsed < Duration::from_secs(5), "the wait must end shortly after its bound");
}

// A wake signal that is raised again every few milliseconds with no work
// behind it, as a signal raised for work that is already done is.
struct ChatteringSignal;

impl WakeSignal for ChatteringSignal {
  fn notify(&self) {}

  fn wait(&self, max: Duration) -> bool {
    thread::sleep(max.min(Duration::from_millis(5)));
    true
  }
}

// The standard boundary with a wake signal that keeps being raised.
struct ChatteringBoundary {
  transport: ChannelBoundary,
}

impl ShardBoundary for ChatteringBoundary {
  fn monotonic_time(&self) -> Option<Duration> {
    self.transport.monotonic_time()
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

  fn wake_signal(&self) -> Option<Arc<dyn WakeSignal>> {
    Some(Arc::new(ChatteringSignal))
  }

  fn close(&self) {
    self.transport.close();
  }
}

// With no receive timeout scheduled, a wait whose signal keeps being raised
// with no work behind it must still end about `max` later in real time.
// The hand-driven clock never reaches the end of the wait, so it cannot be
// what bounds it.
#[test]
fn await_envelope_with_stale_signals_returns_when_hand_driven_clock_never_advances() {
  let (arrived, elapsed) = bounded_wait(Duration::from_secs(10), || {
    let boundary = Arc::new(ChatteringBoundary { transport: ChannelBoundary::new() });
    let mut system = ActorSystem::with_failure_catcher(SYSTEM_NAME, boundary, Arc::new(UnwindCatcher));
    system.set_clock(Arc::new(ControlledClock::new(Duration::from_secs(10))));
    let arrived = system.await_envelope(Duration::from_millis(200));
    system.terminate();
    arrived
  });
  assert!(!arrived, "no work came");
  assert!(elapsed < Duration::from_secs(5), "the wait must end shortly after its bound");
}
