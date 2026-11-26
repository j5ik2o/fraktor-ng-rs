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

// Property-based tests of the receive timeout (the design of #200,
// docs/design/issue-200-receive-timeout.md, section 6, properties P1 to P5).
//
// The receive timeout is driven through the public surface only: an actor
// sets its timeout through `ActorContext::set_receive_timeout`, the test
// moves a hand-driven clock installed with `ActorSystem::set_clock`, and the
// actor records every `ReceiveTimeout` it receives with the clock reading.
// The record is compared, after every step, with a reference model that
// copies Pekko v2.0.0's `dungeon.ReceiveTimeout` (`State(timeout, task,
// version)`, `cancelReceiveTimeoutIfNeeded`, `checkReceiveTimeoutIfNeeded`,
// `checkReceiveTimeout`, `rescheduleReceiveTimeout`,
// `cancelReceiveTimeoutTask`), the order `ActorCell.invoke` calls them in,
// `ActorCell.create` and `FaultHandling.finishRecreate`
// (`checkReceiveTimeout(reschedule = true)`), and `FaultHandling.terminate`
// (both cancels). The comparison is exact: the same `ReceiveTimeout`
// messages, at the same clock readings, with the same durations.
//
// The inputs come from a fixed-seed SplitMix64, so a failure repeats; every
// failure message names the seed and the step. The model's scheduler fires a
// task at the first drive of the system at or after its deadline, which is
// where Pekko's `scheduleOnce` would have enqueued the message under the same
// clock. Durations of zero are kept out of the model runs, because a zero
// timeout rearms on every message and `run_until_idle` would not return
// (design I8); property P5 drives them with `run_one`.
//
// These properties are outside the original suite and are not counted in
// the ledger.

use core::time::Duration;

use std::collections::VecDeque;
use std::panic::{self, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use fraktor_actor::kernel::actor::{
  Actor, ActorContext, Identify, NotInfluenceReceiveTimeout, Props, ReceiveTimeout, ReceiveTimeoutSetting,
};
use fraktor_actor::kernel::actor_ref::ActorRef;
use fraktor_actor::kernel::message::Message;
use fraktor_actor::kernel::Clock;
use fraktor_testkit::fraktorspec::FraktorSpec;

const SPEC_SYSTEM_NAME: &str = "ReceiveTimeoutPropertyTest";

// ---------------------------------------------------------------------------
// Inputs

struct SplitMix64 {
  state: u64,
}

impl SplitMix64 {
  fn new(seed: u64) -> Self {
    Self { state: seed }
  }

  fn next_u64(&mut self) -> u64 {
    self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = self.state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
  }

  fn below(&mut self, bound: u64) -> u64 {
    self.next_u64() % bound
  }

  fn chance(&mut self, percent: u64) -> bool {
    self.below(100) < percent
  }

  fn pick<T: Copy>(&mut self, items: &[T]) -> T {
    items[self.below(items.len() as u64) as usize]
  }
}

const FINITE_DURATIONS: [Duration; 7] = [
  Duration::from_millis(1),
  Duration::from_millis(2),
  Duration::from_millis(5),
  Duration::from_millis(10),
  Duration::from_millis(50),
  Duration::from_millis(100),
  Duration::from_secs(1),
];

fn random_setting(rng: &mut SplitMix64) -> ReceiveTimeoutSetting {
  match rng.below(6) {
    | 0 => ReceiveTimeoutSetting::Undefined,
    | 1 => ReceiveTimeoutSetting::Infinite,
    | _ => ReceiveTimeoutSetting::Finite(rng.pick(&FINITE_DURATIONS)),
  }
}

fn random_settings(rng: &mut SplitMix64, max: u64) -> Vec<ReceiveTimeoutSetting> {
  (0..rng.below(max + 1)).map(|_| random_setting(rng)).collect()
}

fn random_advance(rng: &mut SplitMix64) -> Duration {
  match rng.below(4) {
    | 0 => Duration::ZERO,
    | 1 => Duration::from_nanos(1 + rng.below(3)),
    | 2 => Duration::from_millis(rng.below(12)),
    | _ => Duration::from_millis(rng.below(1200)),
  }
}

// ---------------------------------------------------------------------------
// The system under test

// A hand-driven monotonic clock, in nanoseconds.
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

// A message that influences the timeout; its handler applies the settings in
// order.
#[derive(Clone, Debug)]
struct Influencing(Vec<ReceiveTimeoutSetting>);

// A message marked `NotInfluenceReceiveTimeout`; its handler applies the
// settings in order.
#[derive(Clone, Debug)]
struct Transparent(Vec<ReceiveTimeoutSetting>);

impl NotInfluenceReceiveTimeout for Transparent {}

// A message that influences the timeout; its handler applies the settings in
// order and then fails, so the default strategy restarts the actor.
#[derive(Clone, Debug)]
struct Crash(Vec<ReceiveTimeoutSetting>);

type Deliveries = Arc<Mutex<Vec<Delivery>>>;

// One `ReceiveTimeout` as the actor saw it: the clock reading when it was
// handled and the duration it carried.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Delivery {
  at: Duration,
  timeout: Duration,
}

struct RecordingActor {
  initial: Option<ReceiveTimeoutSetting>,
  started: Arc<AtomicBool>,
  clock: ControlledClock,
  deliveries: Deliveries,
}

impl Actor for RecordingActor {
  // The initial setting is applied on the first start only; a restart runs
  // `pre_start` again through the default `post_restart` and must find the
  // setting the cell kept.
  fn pre_start(&mut self, context: &mut ActorContext<'_>) {
    if !self.started.swap(true, Ordering::SeqCst) {
      if let Some(setting) = self.initial {
        context.set_receive_timeout(setting);
      }
    }
  }

  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    if let Some(timeout) = message.downcast_ref::<ReceiveTimeout>() {
      self.deliveries.lock().unwrap().push(Delivery { at: self.clock.monotonic(), timeout: timeout.timeout() });
      return;
    }
    if let Some(Influencing(settings)) = message.downcast_ref::<Influencing>() {
      apply(context, settings);
      return;
    }
    if let Some(Transparent(settings)) = message.downcast_ref::<Transparent>() {
      apply(context, settings);
      return;
    }
    if let Some(Crash(settings)) = message.downcast_ref::<Crash>() {
      apply(context, settings);
      panic!("crash requested by the property test");
    }
  }
}

fn apply(context: &mut ActorContext<'_>, settings: &[ReceiveTimeoutSetting]) {
  for setting in settings {
    context.set_receive_timeout(*setting);
  }
}

struct Harness {
  spec: FraktorSpec,
  clock: ControlledClock,
  actor: ActorRef,
  deliveries: Deliveries,
}

impl Harness {
  fn start(initial: Option<ReceiveTimeoutSetting>, clock_start: Duration) -> Self {
    let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
    spec.system().register_not_influence_receive_timeout::<Transparent>();
    let clock = ControlledClock::new(clock_start);
    spec.system().set_clock(Arc::new(clock.clone()));
    let deliveries: Deliveries = Arc::new(Mutex::new(Vec::new()));
    let started = Arc::new(AtomicBool::new(false));
    let actor = spec.system_mut().actor_of(Props::new({
      let clock = clock.clone();
      let deliveries = Arc::clone(&deliveries);
      move || RecordingActor {
        initial,
        started: Arc::clone(&started),
        clock: clock.clone(),
        deliveries: Arc::clone(&deliveries),
      }
    }));
    Self { spec, clock, actor, deliveries }
  }

  fn send<M: core::any::Any + Send + Sync>(&self, message: M) {
    self.actor.tell(message, self.spec.system().dead_letters());
  }

  fn run_until_idle(&mut self) {
    self.spec.system_mut().run_until_idle();
  }

  fn run_one(&mut self) -> bool {
    self.spec.system_mut().run_one()
  }

  fn stop(&mut self) {
    let actor = self.actor.clone();
    self.spec.system_mut().stop(&actor);
  }

  fn deliveries(&self) -> Vec<Delivery> {
    self.deliveries.lock().unwrap().clone()
  }

  fn finish(mut self) {
    self.spec.terminate();
  }
}

// ---------------------------------------------------------------------------
// The reference model: Pekko v2.0.0 `dungeon.ReceiveTimeout` and the parts of
// `ActorCell` and `FaultHandling` that call it.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Task {
  // `emptyCancellable`.
  Empty,
  // A `scheduleOnce(timeout, self, ReceiveTimeout(timeout))` not yet run.
  Scheduled { deadline: Duration, timeout: Duration },
  // A task that has run: it is not `emptyCancellable`, and its message is in
  // the mailbox.
  Fired,
}

#[derive(Clone, Copy, Debug)]
struct State {
  timeout: ReceiveTimeoutSetting,
  task: Task,
  version: u32,
}

#[derive(Clone, Debug)]
enum Envelope {
  Influencing(Vec<ReceiveTimeoutSetting>),
  Transparent(Vec<ReceiveTimeoutSetting>),
  Crash(Vec<ReceiveTimeoutSetting>),
  ReceiveTimeout(Duration),
}

impl Envelope {
  // `ReceiveTimeoutCompat.isNotInfluenceReceiveTimeout`.
  fn influences(&self) -> bool {
    !matches!(self, Envelope::Transparent(_))
  }
}

struct Model {
  data: Option<State>,
  now: Duration,
  mailbox: VecDeque<Envelope>,
  deliveries: Vec<Delivery>,
  stopped: bool,
}

impl Model {
  // `ActorCell.create`: the constructor (here `pre_start`) runs, then
  // `checkReceiveTimeout(reschedule = true)`.
  fn start(initial: Option<ReceiveTimeoutSetting>, now: Duration) -> Self {
    let mut model = Self { data: None, now, mailbox: VecDeque::new(), deliveries: Vec::new(), stopped: false };
    if let Some(setting) = initial {
      model.set_receive_timeout(setting);
    }
    model.check_receive_timeout(true);
    model
  }

  fn version(&self) -> u32 {
    self.data.map_or(0, |data| data.version)
  }

  fn set_receive_timeout(&mut self, timeout: ReceiveTimeoutSetting) {
    match self.data.as_mut() {
      | None => self.data = Some(State { timeout, task: Task::Empty, version: 1 }),
      | Some(data) => {
        data.timeout = timeout;
        data.version = data.version.wrapping_add(1);
      },
    }
  }

  fn check_receive_timeout(&mut self, reschedule: bool) {
    let Some(data) = self.data else {
      return;
    };
    match data.timeout {
      | ReceiveTimeoutSetting::Finite(timeout) => {
        if reschedule || data.task == Task::Empty {
          self.reschedule_receive_timeout(timeout);
        }
      },
      | _ => self.cancel_receive_timeout_task(),
    }
  }

  // A deadline past the clock's range schedules nothing (design section 5).
  fn reschedule_receive_timeout(&mut self, timeout: Duration) {
    let now = self.now;
    let data = self.data.as_mut().expect("rescheduling needs timeout data");
    data.task = match now.checked_add(timeout) {
      | Some(deadline) => Task::Scheduled { deadline, timeout },
      | None => Task::Empty,
    };
    data.version = data.version.wrapping_add(1);
  }

  fn cancel_receive_timeout_task(&mut self) {
    if let Some(data) = self.data.as_mut() {
      if data.task != Task::Empty {
        data.task = Task::Empty;
        data.version = data.version.wrapping_add(1);
      }
    }
  }

  fn cancel_receive_timeout_if_needed(&mut self, envelope: &Envelope) -> u32 {
    if self.data.is_some() && envelope.influences() {
      self.cancel_receive_timeout_task();
    }
    self.version()
  }

  fn check_receive_timeout_if_needed(&mut self, envelope: &Envelope, before: u32) {
    let changed = self.version() != before;
    if self.data.is_some() || changed {
      self.check_receive_timeout(envelope.influences() || changed);
    }
  }

  // `ActorCell.invoke`, with a failure handled by a restart:
  // `FaultHandling.finishRecreate` runs `postRestart` and then
  // `checkReceiveTimeout(reschedule = true)`.
  fn invoke(&mut self, envelope: Envelope) {
    let before = self.cancel_receive_timeout_if_needed(&envelope);
    let failed = match &envelope {
      | Envelope::ReceiveTimeout(timeout) => {
        self.deliveries.push(Delivery { at: self.now, timeout: *timeout });
        false
      },
      | Envelope::Influencing(settings) | Envelope::Transparent(settings) => {
        settings.iter().for_each(|setting| self.set_receive_timeout(*setting));
        false
      },
      | Envelope::Crash(settings) => {
        settings.iter().for_each(|setting| self.set_receive_timeout(*setting));
        true
      },
    };
    self.check_receive_timeout_if_needed(&envelope, before);
    if failed {
      self.check_receive_timeout(true);
    }
  }

  fn fire_due(&mut self) {
    if let Some(data) = self.data.as_mut() {
      if let Task::Scheduled { deadline, timeout } = data.task {
        if self.now >= deadline {
          data.task = Task::Fired;
          self.mailbox.push_back(Envelope::ReceiveTimeout(timeout));
        }
      }
    }
  }

  fn run_until_idle(&mut self) {
    if self.stopped {
      self.mailbox.clear();
      return;
    }
    loop {
      self.fire_due();
      match self.mailbox.pop_front() {
        | Some(envelope) => self.invoke(envelope),
        | None => break,
      }
    }
  }

  fn send(&mut self, envelope: Envelope) {
    if !self.stopped {
      self.mailbox.push_back(envelope);
    }
  }

  // `FaultHandling.terminate`: `cancelReceiveTimeout()` and
  // `cancelReceiveTimeoutTask()`.
  fn stop(&mut self) {
    if self.stopped {
      return;
    }
    self.set_receive_timeout(ReceiveTimeoutSetting::Undefined);
    self.cancel_receive_timeout_task();
    self.mailbox.clear();
    self.stopped = true;
  }

  fn scheduled_deadline(&self) -> Option<Duration> {
    match self.data.map(|data| data.task) {
      | Some(Task::Scheduled { deadline, .. }) if !self.stopped => Some(deadline),
      | _ => None,
    }
  }
}

// ---------------------------------------------------------------------------
// Steps

#[derive(Clone, Debug)]
enum Step {
  Advance(Duration),
  // Moves the clock first and sends before the system is driven, so a task
  // that comes due is fired behind the message.
  Send { advance_first: Duration, envelope: Envelope },
  // Moves the clock to one nanosecond before the scheduled deadline, then
  // to the deadline (P1).
  ProbeDeadline,
  Stop,
}

fn random_step(rng: &mut SplitMix64) -> Step {
  let roll = rng.below(100);
  if roll < 25 {
    return Step::Advance(random_advance(rng));
  }
  if roll < 35 {
    return Step::ProbeDeadline;
  }
  if roll < 37 {
    return Step::Stop;
  }
  let advance_first = if rng.chance(30) { random_advance(rng) } else { Duration::ZERO };
  let envelope = match rng.below(100) {
    | 0..=2 => Envelope::Crash(random_settings(rng, 2)),
    | 3..=45 => Envelope::Influencing(random_settings(rng, 3)),
    | _ => Envelope::Transparent(random_settings(rng, 3)),
  };
  Step::Send { advance_first, envelope }
}

fn real_message(envelope: &Envelope) -> Message {
  match envelope {
    | Envelope::Influencing(settings) => Message::new(Influencing(settings.clone())),
    | Envelope::Transparent(settings) => Message::new(Transparent(settings.clone())),
    | Envelope::Crash(settings) => Message::new(Crash(settings.clone())),
    | Envelope::ReceiveTimeout(_) => unreachable!("the test never sends a ReceiveTimeout itself"),
  }
}

fn advance_both(harness: &Harness, model: &mut Model, by: Duration) {
  harness.clock.advance(by);
  model.now += by;
}

fn drive_both(harness: &mut Harness, model: &mut Model) {
  harness.run_until_idle();
  model.run_until_idle();
}

fn assert_same(harness: &Harness, model: &Model, seed: u64, index: usize, step: &Step) {
  assert_eq!(
    harness.deliveries(),
    model.deliveries,
    "seed {seed:#x}, step {index} ({step:?}): the ReceiveTimeout messages differ from the reference model"
  );
}

fn run_step(harness: &mut Harness, model: &mut Model, step: &Step, seed: u64, index: usize) {
  match step {
    | Step::Advance(by) => {
      advance_both(harness, model, *by);
      drive_both(harness, model);
    },
    | Step::Send { advance_first, envelope } => {
      advance_both(harness, model, *advance_first);
      harness.actor.tell(real_message(envelope), harness.spec.system().dead_letters());
      model.send(envelope.clone());
      drive_both(harness, model);
    },
    | Step::ProbeDeadline => {
      let Some(deadline) = model.scheduled_deadline() else {
        return;
      };
      let just_before = deadline.saturating_sub(Duration::from_nanos(1));
      advance_both(harness, model, just_before.saturating_sub(model.now));
      drive_both(harness, model);
      assert_same(harness, model, seed, index, step);
      advance_both(harness, model, deadline.saturating_sub(model.now));
      drive_both(harness, model);
    },
    | Step::Stop => {
      harness.stop();
      harness.run_until_idle();
      model.stop();
    },
  }
  assert_same(harness, model, seed, index, step);
}

fn random_initial(rng: &mut SplitMix64) -> Option<ReceiveTimeoutSetting> {
  if rng.chance(20) {
    None
  } else {
    Some(random_setting(rng))
  }
}

// P1, P3, P4: over random step sequences, the actor receives exactly the
// `ReceiveTimeout` messages the reference model delivers, at the same clock
// readings and with the same durations. This covers that a deadline exists
// only while a finite timeout is scheduled and equals the scheduling time
// plus the duration (P1, probed one nanosecond before and at the deadline),
// that a fired `ReceiveTimeout` is delivered once and always, also after the
// setting changed or the actor restarted behind it (P3), and that the last of
// several settings in one handler is the one in effect (P4).
#[test]
fn receive_timeouts_match_the_reference_model() {
  for case in 0..96_u64 {
    let seed = 0x5EED_0200_0000_0000 ^ case;
    let mut rng = SplitMix64::new(seed);
    let clock_start = Duration::from_millis(rng.below(5_000));
    let initial = random_initial(&mut rng);
    let mut harness = Harness::start(initial, clock_start);
    harness.run_until_idle();
    let mut model = Model::start(initial, clock_start);
    let start = Step::Advance(Duration::ZERO);
    assert_same(&harness, &model, seed, 0, &start);
    for index in 1..=40 {
      let step = random_step(&mut rng);
      run_step(&mut harness, &mut model, &step, seed, index);
    }
    harness.finish();
  }
}

// P2 (with I7): a run of messages that do not influence the timeout, the
// registered type and the built-in `Identify`, with no setting change, never
// moves the deadline: the `ReceiveTimeout` arrives exactly at the time the
// first finite setting scheduled it.
#[test]
fn not_influence_messages_never_move_the_deadline() {
  for case in 0..64_u64 {
    let seed = 0x5EED_0200_0000_1000 ^ case;
    let mut rng = SplitMix64::new(seed);
    let timeout = rng.pick(&FINITE_DURATIONS[2..]);
    let clock_start = Duration::from_millis(rng.below(5_000));
    let mut harness = Harness::start(Some(ReceiveTimeoutSetting::Finite(timeout)), clock_start);
    harness.run_until_idle();
    let deadline = clock_start + timeout;
    let mut now = clock_start;
    loop {
      let left = deadline - now;
      let step = Duration::from_nanos(rng.below(left.as_nanos() as u64));
      harness.clock.advance(step);
      now += step;
      if rng.chance(50) {
        harness.send(Transparent(Vec::new()));
      } else {
        harness.send(Identify::new(rng.next_u64()));
      }
      harness.run_until_idle();
      assert!(
        harness.deliveries().is_empty(),
        "seed {seed:#x}: a ReceiveTimeout arrived at {now:?}, before the deadline {deadline:?}"
      );
      if rng.chance(15) {
        break;
      }
    }
    harness.clock.advance(deadline - now);
    harness.run_until_idle();
    assert_eq!(
      harness.deliveries(),
      vec![Delivery { at: deadline, timeout }],
      "seed {seed:#x}: the not-influencing messages moved the deadline"
    );
    harness.finish();
  }
}

// P3: one firing enqueues one `ReceiveTimeout`, however far the clock moved
// past the deadline, and a `ReceiveTimeout` that is already in the mailbox is
// delivered even when the message ahead of it turns the timeout off, changes
// it, or makes the actor restart.
#[test]
fn a_fired_receive_timeout_is_delivered_once_and_always() {
  for case in 0..64_u64 {
    let seed = 0x5EED_0200_0000_2000 ^ case;
    let mut rng = SplitMix64::new(seed);
    let timeout = rng.pick(&FINITE_DURATIONS);
    let clock_start = Duration::from_millis(rng.below(5_000));
    let mut harness = Harness::start(Some(ReceiveTimeoutSetting::Finite(timeout)), clock_start);
    harness.run_until_idle();

    let overshoot = timeout * (1 + rng.below(20) as u32);
    harness.clock.advance(overshoot);
    let ahead = match rng.below(5) {
      | 0 => Message::new(Transparent(vec![ReceiveTimeoutSetting::Undefined])),
      | 1 => Message::new(Transparent(vec![ReceiveTimeoutSetting::Infinite])),
      | 2 => Message::new(Influencing(vec![ReceiveTimeoutSetting::Undefined])),
      | 3 => Message::new(Transparent(vec![ReceiveTimeoutSetting::Finite(timeout * 7)])),
      | _ => Message::new(Crash(vec![ReceiveTimeoutSetting::Undefined])),
    };
    harness.actor.tell(ahead, harness.spec.system().dead_letters());
    harness.run_until_idle();

    let fired_at = clock_start + overshoot;
    assert_eq!(
      harness.deliveries(),
      vec![Delivery { at: fired_at, timeout }],
      "seed {seed:#x}: the fired ReceiveTimeout must be delivered exactly once"
    );
    harness.finish();
  }
}

// P4: however many times one handler sets the timeout, only the last setting
// is in effect, and at most one deadline exists.
#[test]
fn the_last_setting_in_one_handler_wins() {
  for case in 0..64_u64 {
    let seed = 0x5EED_0200_0000_3000 ^ case;
    let mut rng = SplitMix64::new(seed);
    let clock_start = Duration::from_millis(rng.below(5_000));
    let initial = random_initial(&mut rng);
    let mut harness = Harness::start(initial, clock_start);
    harness.run_until_idle();
    let settings: Vec<ReceiveTimeoutSetting> = (0..1 + rng.below(20)).map(|_| random_setting(&mut rng)).collect();
    let last = *settings.last().expect("at least one setting");
    if rng.chance(50) {
      harness.send(Transparent(settings));
    } else {
      harness.send(Influencing(settings));
    }
    harness.run_until_idle();
    let initial_deliveries = harness.deliveries();
    assert!(initial_deliveries.is_empty(), "seed {seed:#x}: no time has passed, so nothing fires");

    // Walk past every duration any setting could have scheduled.
    let horizon = Duration::from_secs(3);
    harness.clock.advance(horizon);
    harness.run_until_idle();
    let expected = match last {
      | ReceiveTimeoutSetting::Finite(timeout) => vec![Delivery { at: clock_start + horizon, timeout }],
      | ReceiveTimeoutSetting::Undefined | ReceiveTimeoutSetting::Infinite => Vec::new(),
    };
    assert_eq!(harness.deliveries(), expected, "seed {seed:#x}: only the last setting {last:?} is in effect");
    harness.finish();
  }
}

// P5 (with I8): a zero duration is finite and fires at the next drive of the
// system, and durations at the edge of the clock's range do not panic: a
// deadline past the range schedules nothing.
#[test]
fn edge_durations_do_not_panic() {
  let edges = [
    Duration::ZERO,
    Duration::from_nanos(1),
    Duration::MAX,
    Duration::MAX - Duration::from_nanos(1),
    Duration::from_secs(u64::MAX / 2),
  ];
  for (case, timeout) in edges.into_iter().enumerate() {
    let clock_start = Duration::from_secs(10);
    let outcome = panic::catch_unwind(AssertUnwindSafe(|| {
      let mut harness = Harness::start(None, clock_start);
      harness.run_until_idle();
      harness.send(Influencing(vec![ReceiveTimeoutSetting::Finite(timeout)]));
      assert!(harness.run_one(), "the setting message is delivered");
      if timeout == Duration::ZERO {
        // `run_until_idle` would not return: a zero timeout rearms after
        // every `ReceiveTimeout` (design I8).
        assert!(harness.run_one(), "a zero timeout fires at the next drive");
        assert_eq!(
          harness.deliveries(),
          vec![Delivery { at: clock_start, timeout }],
          "a zero timeout is delivered at once"
        );
        harness.send(Influencing(vec![ReceiveTimeoutSetting::Undefined]));
        harness.run_until_idle();
      } else if timeout == Duration::from_nanos(1) {
        harness.clock.advance(timeout);
        harness.run_until_idle();
        assert_eq!(harness.deliveries(), vec![Delivery { at: clock_start + timeout, timeout }]);
      } else {
        harness.run_until_idle();
        harness.clock.advance(Duration::from_secs(1_000_000));
        harness.run_until_idle();
        assert!(harness.deliveries().is_empty(), "a deadline at or past the clock's range never comes");
      }
      harness.finish();
    }));
    if let Err(payload) = outcome {
      let text = payload
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| payload.downcast_ref::<&str>().copied())
        .unwrap_or_default();
      panic!("case {case} ({timeout:?}) failed: {text}");
    }
  }
}
