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

// Property and regression tests for `TestFsmRef` of issue #181
// (docs/design/issue-181-testfsmref.md, sections 4 to 6).
//
// These are not ported from a Pekko test file and are not counted in the
// ledger. P1 to P3 drive random operations, drawn from a fixed-seed
// SplitMix64, against the real machine and compare it with a plain model in
// this file. The fixed tests after them pin the invariants that need a set
// order of steps or a timer that really fires: the state timer is cancelled
// by every received message (I5), stale timer marks are dropped (I8), a
// single timer leaves the table once it fires (I9), a restarted machine
// starts over (I10), and the inputs of section 5.
//
// Every read waits first until the machine has processed a numbered no-op
// control message sent after everything else, so that a value that matches
// the model part-way through a sequence cannot pass for the final one.

use core::time::Duration;

use std::collections::BTreeSet;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::sync::Mutex;

use fraktor_actor::kernel::actor::{
  ActorContext, CurrentState, Fsm, FsmState, FsmView, StateTimeout, SubscribeTransitionCallBack, Transition,
};
use fraktor_actor::kernel::actor_ref::ActorRef;
use fraktor_actor::kernel::message::Message;
use fraktor_testkit::fraktorspec::FraktorSpec;
use fraktor_testkit::test_fsm_ref::TestFsmRef;
use fraktor_testkit::test_kit::TestProbe;

const SPEC_SYSTEM_NAME: &str = "TestFsmRefProperty";

const SEEDS: [u64; 5] = [1, 7, 42, 1_000, 99_991];

/// How long a test waits for the machine to process what it was sent.
const SETTLE_MAX: Duration = Duration::from_secs(3);

/// A delay no test outlives, for timers that must not fire.
const NEVER: Duration = Duration::from_secs(3_600);

struct SplitMix64(u64);

impl SplitMix64 {
  fn next(&mut self) -> u64 {
    self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = self.0;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
  }

  fn below(&mut self, bound: u64) -> u64 {
    self.next() % bound
  }

  fn pick<T: Clone>(&mut self, items: &[T]) -> T {
    items[self.below(items.len() as u64) as usize].clone()
  }
}

fn settle<S, D>(spec: &mut FraktorSpec, fsm: &TestFsmRef<S, D>)
where
  S: Clone + PartialEq + Send + Sync + 'static,
  D: Clone + Send + 'static,
{
  let sentinel = fsm.send_sentinel();
  spec.await_cond(|| fsm.processed_sequence() >= sentinel, SETTLE_MAX);
}

fn no_sender(spec: &FraktorSpec) -> ActorRef {
  spec.system().dead_letters()
}

#[derive(Clone, Copy, Debug)]
enum TimerKind {
  Single,
  FixedDelay,
  FixedRate,
}

const TIMER_KINDS: [TimerKind; 3] = [TimerKind::Single, TimerKind::FixedDelay, TimerKind::FixedRate];

fn start_timer<S, D>(fsm: &TestFsmRef<S, D>, kind: TimerKind, name: &str, message: Message, delay: Duration)
where
  S: Clone + PartialEq + Send + Sync + 'static,
  D: Clone + Send + 'static,
{
  match kind {
    | TimerKind::Single => fsm.start_single_timer(name, message, delay),
    | TimerKind::FixedDelay => fsm.start_timer_with_fixed_delay(name, message, delay),
    | TimerKind::FixedRate => fsm.start_timer_at_fixed_rate(name, message, delay),
  }
}

// ---------------------------------------------------------------------------
// P1: random events, `set_state`, and timer operations against a model.
// ---------------------------------------------------------------------------

/// The state with no state function: moving there keeps the current state.
const NOWHERE: u8 = 9;

const P1_STATES: [u8; 4] = [0, 1, 2, NOWHERE];

const P1_EVENTS: [&str; 6] = ["next", "bump", "stay", "self", "nowhere", "unknown"];

const P1_TIMER_NAMES: [&str; 3] = ["a", "b", ""];

type TransitionLog = Arc<Mutex<Vec<(u8, u8)>>>;

// States 0, 1, and 2 have a state function; `NOWHERE` has none. In each:
// `next` goes to the next of the three, `bump` stays with the data plus
// one, `stay` stays, `self` goes to the same state, `nowhere` goes to
// `NOWHERE`, and any other event is unhandled. Every reported transition
// is logged.
fn p1_machine(log: TransitionLog) -> Fsm<u8, u32> {
  let mut fsm = Fsm::new();
  fsm.on_transition(move |_context: &mut ActorContext<'_>, from: &u8, to: &u8| {
    log.lock().unwrap().push((*from, *to));
  });
  fsm.start_with(0, 0);
  for state in 0..3u8 {
    fsm.when(state, move |_context: &mut ActorContext<'_>, _view: &FsmView<u8>, event: &Message, data: &u32| {
      match *event.downcast_ref::<&'static str>()? {
        | "next" => Some(FsmState::goto((state + 1) % 3)),
        | "bump" => Some(FsmState::stay().using(data + 1)),
        | "stay" => Some(FsmState::stay()),
        | "self" => Some(FsmState::goto(state)),
        | "nowhere" => Some(FsmState::goto(NOWHERE)),
        | _ => None,
      }
    });
  }
  fsm
}

#[derive(Clone, Debug)]
enum P1Step {
  Event(&'static str),
  SetState { name: Option<u8>, data: Option<u32>, timeout: Option<Duration> },
  StartTimer(TimerKind, &'static str),
  CancelTimer(&'static str),
  Check,
}

/// The model of section 6, P1, which follows Pekko's `FSM`: every received
/// message cancels the state timer, `set_state` is a reported `goto`, a move
/// into a state with no state function changes nothing, and `stay().using`
/// replaces the data without a report.
#[derive(Default)]
struct P1Model {
  state: u8,
  data: u32,
  log: Vec<(u8, u8)>,
  state_timer: bool,
  timers: BTreeSet<&'static str>,
}

impl P1Model {
  fn has_state(state: u8) -> bool {
    state < 3
  }

  fn apply(&mut self, step: &P1Step) {
    match step {
      | P1Step::Event(event) => {
        self.state_timer = false;
        match *event {
          | "next" => {
            let next = (self.state + 1) % 3;
            self.log.push((self.state, next));
            self.state = next;
          },
          | "bump" => self.data += 1,
          | "self" => self.log.push((self.state, self.state)),
          | _ => {},
        }
      },
      | P1Step::SetState { name, data, timeout } => {
        self.state_timer = false;
        let target = name.unwrap_or(self.state);
        if Self::has_state(target) {
          self.log.push((self.state, target));
          self.state = target;
          self.data = data.unwrap_or(self.data);
          self.state_timer = timeout.is_some();
        }
      },
      | P1Step::StartTimer(_, name) => {
        self.timers.insert(name);
      },
      | P1Step::CancelTimer(name) => {
        self.timers.remove(name);
      },
      | P1Step::Check => {},
    }
  }
}

fn p1_steps(rng: &mut SplitMix64) -> Vec<P1Step> {
  (0..60)
    .map(|_| match rng.below(10) {
      | 0..=3 => P1Step::Event(rng.pick(&P1_EVENTS)),
      | 4..=5 => P1Step::SetState {
        name: if rng.below(3) == 0 { None } else { Some(rng.pick(&P1_STATES)) },
        data: if rng.below(3) == 0 { None } else { Some(rng.below(1_000) as u32) },
        timeout: if rng.below(2) == 0 { None } else { Some(NEVER) },
      },
      | 6 => P1Step::StartTimer(rng.pick(&TIMER_KINDS), rng.pick(&P1_TIMER_NAMES)),
      | 7 => P1Step::CancelTimer(rng.pick(&P1_TIMER_NAMES)),
      | _ => P1Step::Check,
    })
    .collect()
}

fn p1_assert_matches(fsm: &TestFsmRef<u8, u32>, log: &TransitionLog, model: &P1Model, seed: u64, at: usize) {
  let context = format!("seed {seed}, after step {at}");
  assert_eq!(fsm.state_name(), model.state, "state name, {context}");
  assert_eq!(fsm.state_data(), model.data, "state data, {context}");
  assert_eq!(*log.lock().unwrap(), model.log, "reported transitions, {context}");
  assert_eq!(fsm.is_state_timer_active(), model.state_timer, "state timer, {context}");
  for name in P1_TIMER_NAMES {
    assert_eq!(fsm.is_timer_active(name), model.timers.contains(name), "timer {name:?}, {context}");
  }
}

// P1 (I1, I2, I3, I5, I6): the snapshot after a random sequence equals the
// model that follows the same sequence.
#[test]
fn the_snapshot_follows_the_model_through_random_operations() {
  for seed in SEEDS {
    let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
    let log = TransitionLog::default();
    let machine_log = log.clone();
    let fsm = TestFsmRef::new(spec.system_mut(), move || p1_machine(machine_log.clone()));
    let mut model = P1Model::default();
    p1_assert_matches(&fsm, &log, &model, seed, 0);

    let steps = p1_steps(&mut SplitMix64(seed));
    for (index, step) in steps.iter().enumerate() {
      match step {
        | P1Step::Event(event) => fsm.tell(*event, no_sender(&spec)),
        | P1Step::SetState { name, data, timeout } => fsm.set_state(*name, *data, *timeout),
        | P1Step::StartTimer(kind, name) => start_timer(&fsm, *kind, name, Message::new("timer"), NEVER),
        | P1Step::CancelTimer(name) => fsm.cancel_timer(name),
        | P1Step::Check => {
          settle(&mut spec, &fsm);
          p1_assert_matches(&fsm, &log, &model, seed, index);
        },
      }
      model.apply(step);
    }
    settle(&mut spec, &fsm);
    p1_assert_matches(&fsm, &log, &model, seed, steps.len());
    spec.terminate();
  }
}

// ---------------------------------------------------------------------------
// P2: the timer table against a set of names.
// ---------------------------------------------------------------------------

fn idle_machine() -> Fsm<u8, ()> {
  let mut fsm = Fsm::new();
  fsm.start_with(0, ());
  fsm.when(0, |_context: &mut ActorContext<'_>, _view: &FsmView<u8>, _event: &Message, _data: &()| {
    Some(FsmState::stay())
  });
  fsm
}

fn p2_names() -> Vec<String> {
  vec![
    String::new(),
    "t".to_string(),
    "T".to_string(),
    "*".to_string(),
    "**".to_string(),
    "a*b".to_string(),
    "a?b".to_string(),
    "$x".to_string(),
    "/".to_string(),
    "n".repeat(512),
    format!("{}x", "n".repeat(512)),
    "名前".to_string(),
  ]
}

#[derive(Clone, Debug)]
enum P2Step {
  Start(TimerKind, String),
  Cancel(String),
  Check,
}

// P2 (I4): after any sequence of starts and cancels, a name is active
// exactly when the model's set holds it. The names include the empty
// string, wildcard characters, long strings, and names that are never
// started.
#[test]
fn the_timer_table_follows_a_set_of_names() {
  let names = p2_names();
  let started: Vec<String> = names[..names.len() - 2].to_vec();
  for seed in SEEDS {
    let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
    let fsm = TestFsmRef::new(spec.system_mut(), idle_machine);
    let mut rng = SplitMix64(seed);
    let mut model: BTreeSet<String> = BTreeSet::new();
    let steps: Vec<P2Step> = (0..80)
      .map(|_| match rng.below(9) {
        | 0..=3 => P2Step::Start(rng.pick(&TIMER_KINDS), rng.pick(&started)),
        | 4..=7 => P2Step::Cancel(rng.pick(&names)),
        | _ => P2Step::Check,
      })
      .collect();
    let check = |spec: &mut FraktorSpec, model: &BTreeSet<String>, at: usize| {
      settle(spec, &fsm);
      for name in &names {
        assert_eq!(fsm.is_timer_active(name), model.contains(name), "timer {name:?}, seed {seed}, after step {at}");
      }
    };
    for (index, step) in steps.iter().enumerate() {
      match step {
        | P2Step::Start(kind, name) => {
          start_timer(&fsm, *kind, name, Message::new("timer"), NEVER);
          model.insert(name.clone());
        },
        | P2Step::Cancel(name) => {
          fsm.cancel_timer(name);
          model.remove(name);
        },
        | P2Step::Check => check(&mut spec, &model, index),
      }
    }
    check(&mut spec, &model, steps.len());
    spec.terminate();
  }
}

// ---------------------------------------------------------------------------
// P3: any timeout value, including zero and the largest durations.
// ---------------------------------------------------------------------------

// Three states that keep the state on everything, the state timeout
// included, so a timer that fires does not move the machine.
fn steady_machine() -> Fsm<u8, String> {
  let mut fsm = Fsm::new();
  fsm.start_with(1, "start".to_string());
  for state in 1..=3u8 {
    fsm.when(state, |_context: &mut ActorContext<'_>, _view: &FsmView<u8>, _event: &Message, _data: &String| {
      Some(FsmState::stay())
    });
  }
  fsm
}

fn p3_timeouts(rng: &mut SplitMix64) -> Vec<Duration> {
  let mut timeouts = vec![
    Duration::ZERO,
    Duration::from_nanos(1),
    Duration::from_millis(1),
    Duration::from_millis(10),
    NEVER,
    Duration::from_secs(u64::MAX / 2),
    Duration::from_secs(u64::MAX),
    Duration::MAX - Duration::from_nanos(1),
    Duration::MAX,
  ];
  for _ in 0..8 {
    timeouts.push(Duration::new(rng.next(), (rng.next() % 1_000_000_000) as u32));
  }
  timeouts
}

// P3 (I2, section 5): `set_state` with any timeout neither panics nor
// restarts the machine, and leaves the state name and data it was given.
// Timers started with the largest delays do not break the machine either.
#[test]
fn any_timeout_leaves_the_state_it_was_given() {
  for seed in SEEDS {
    let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
    let fsm = TestFsmRef::new(spec.system_mut(), steady_machine);
    let mut rng = SplitMix64(seed);
    let mut last = (1, "start".to_string());
    for (index, timeout) in p3_timeouts(&mut rng).into_iter().enumerate() {
      let state = 2 + (index % 2) as u8;
      let data = format!("data-{index}");
      last = (state, data.clone());
      fsm.set_state(Some(state), Some(data.clone()), Some(timeout));
      settle(&mut spec, &fsm);
      assert_eq!(fsm.state_name(), state, "state name, seed {seed}, timeout {timeout:?}");
      assert_eq!(fsm.state_data(), data, "state data, seed {seed}, timeout {timeout:?}");
      assert!(!spec.system().is_actor_terminated(&fsm.actor_ref()), "seed {seed}, timeout {timeout:?}");
    }
    let large = [Duration::from_secs(u64::MAX), Duration::MAX];
    for (index, delay) in large.into_iter().enumerate() {
      let name = format!("large-{index}");
      fsm.start_single_timer(&name, Message::new("timer"), delay);
      fsm.start_timer_with_fixed_delay(&format!("{name}-delay"), Message::new("timer"), delay);
      fsm.start_timer_at_fixed_rate(&format!("{name}-rate"), Message::new("timer"), delay);
      settle(&mut spec, &fsm);
      assert!(fsm.is_timer_active(&name), "seed {seed}, delay {delay:?}");
      assert!(fsm.is_timer_active(&format!("{name}-delay")), "seed {seed}, delay {delay:?}");
      assert!(fsm.is_timer_active(&format!("{name}-rate")), "seed {seed}, delay {delay:?}");
    }
    assert_eq!((fsm.state_name(), fsm.state_data()), last, "seed {seed}");
    assert!(!spec.system().is_actor_terminated(&fsm.actor_ref()), "seed {seed}");
    spec.terminate();
  }
}

// ---------------------------------------------------------------------------
// Fixed tests.
// ---------------------------------------------------------------------------

type Received = Arc<Mutex<Vec<&'static str>>>;

// State 1 goes to 2 with "timeout" on the state timeout (or stays, when
// `move_on_timeout` is false), stays on `stay`, panics on `boom`, and
// records every other `&str` it receives, which is how a timer's message
// arrives. State 2 panics on `boom` and stays on everything else.
fn timeout_machine(received: Received, timeouts: Arc<AtomicUsize>, move_on_timeout: bool) -> Fsm<i32, String> {
  let mut fsm = Fsm::new();
  fsm.start_with(1, String::new());
  fsm.when(1, move |_context: &mut ActorContext<'_>, _view: &FsmView<i32>, event: &Message, _data: &String| {
    if event.is::<StateTimeout>() {
      timeouts.fetch_add(1, Ordering::SeqCst);
      return Some(if move_on_timeout { FsmState::goto(2).using("timeout".to_string()) } else { FsmState::stay() });
    }
    match *event.downcast_ref::<&'static str>()? {
      | "stay" => Some(FsmState::stay()),
      | "boom" => panic!("boom"),
      | other => {
        received.lock().unwrap().push(other);
        Some(FsmState::stay())
      },
    }
  });
  fsm.when(2, |_context: &mut ActorContext<'_>, _view: &FsmView<i32>, event: &Message, _data: &String| {
    match event.downcast_ref::<&'static str>() {
      | Some(&"boom") => panic!("boom"),
      | _ => Some(FsmState::stay()),
    }
  });
  fsm
}

struct Machine {
  spec: FraktorSpec,
  fsm: TestFsmRef<i32, String>,
  received: Received,
  timeouts: Arc<AtomicUsize>,
}

fn start_timeout_machine(move_on_timeout: bool) -> Machine {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let received = Received::default();
  let timeouts = Arc::new(AtomicUsize::new(0));
  let (machine_received, machine_timeouts) = (received.clone(), timeouts.clone());
  let fsm = TestFsmRef::new(spec.system_mut(), move || {
    timeout_machine(machine_received.clone(), machine_timeouts.clone(), move_on_timeout)
  });
  Machine { spec, fsm, received, timeouts }
}

// I5: a message the machine receives with `stay()` cancels the state timer,
// and none is armed again.
#[test]
fn a_message_received_with_stay_cancels_the_state_timer() {
  let Machine { mut spec, fsm, .. } = start_timeout_machine(true);
  fsm.set_state(None, None, Some(NEVER));
  settle(&mut spec, &fsm);
  assert!(fsm.is_state_timer_active());
  fsm.tell("stay", no_sender(&spec));
  settle(&mut spec, &fsm);
  assert!(!fsm.is_state_timer_active());
  assert_eq!(fsm.state_name(), 1);
  spec.terminate();
}

// I5: timer operations from outside the machine leave the state timer
// alone, as the original's proxies do not go through `receive`.
#[test]
fn timer_operations_leave_the_state_timer_alone() {
  let Machine { mut spec, fsm, .. } = start_timeout_machine(true);
  fsm.set_state(None, None, Some(NEVER));
  fsm.start_single_timer("t", Message::new("tick"), NEVER);
  fsm.cancel_timer("t");
  fsm.cancel_timer("absent");
  settle(&mut spec, &fsm);
  assert!(fsm.is_state_timer_active());
  spec.terminate();
}

// I5: once the state timeout fires, the state timer still reads as active
// until the next ordinary message, as in Pekko.
#[test]
fn the_state_timer_reads_active_after_it_fires_until_the_next_message() {
  let Machine { mut spec, fsm, timeouts, .. } = start_timeout_machine(false);
  fsm.set_state(None, None, Some(Duration::from_millis(20)));
  spec.await_cond(|| timeouts.load(Ordering::SeqCst) == 1, SETTLE_MAX);
  settle(&mut spec, &fsm);
  assert!(fsm.is_state_timer_active());
  fsm.tell("stay", no_sender(&spec));
  settle(&mut spec, &fsm);
  assert!(!fsm.is_state_timer_active());
  assert_eq!(timeouts.load(Ordering::SeqCst), 1);
  spec.terminate();
}

// I8, section 3.5: a second `set_state` cancels the state timer of the
// first, so the first timeout never arrives.
#[test]
fn a_replaced_state_timer_does_not_fire() {
  let Machine { mut spec, fsm, timeouts, .. } = start_timeout_machine(true);
  fsm.set_state(None, None, Some(Duration::from_millis(50)));
  fsm.set_state(None, None, None);
  settle(&mut spec, &fsm);
  spec.test_kit_mut().expect_no_message(Duration::from_millis(300));
  settle(&mut spec, &fsm);
  assert_eq!(timeouts.load(Ordering::SeqCst), 0);
  assert_eq!(fsm.state_name(), 1);
  assert!(!fsm.is_state_timer_active());
  spec.terminate();
}

// I8: a message received before the state timeout fires cancels it.
#[test]
fn a_state_timer_cancelled_by_a_message_does_not_fire() {
  let Machine { mut spec, fsm, timeouts, .. } = start_timeout_machine(true);
  fsm.set_state(None, None, Some(Duration::from_millis(50)));
  fsm.tell("stay", no_sender(&spec));
  settle(&mut spec, &fsm);
  spec.test_kit_mut().expect_no_message(Duration::from_millis(300));
  settle(&mut spec, &fsm);
  assert_eq!(timeouts.load(Ordering::SeqCst), 0);
  assert_eq!(fsm.state_name(), 1);
  spec.terminate();
}

// I4, I8: a cancelled timer delivers nothing, and a timer started again
// under the same name replaces the earlier one.
#[test]
fn a_cancelled_or_replaced_timer_does_not_deliver() {
  let Machine { mut spec, fsm, received, .. } = start_timeout_machine(true);
  fsm.start_single_timer("cancelled", Message::new("cancelled"), Duration::from_millis(50));
  fsm.cancel_timer("cancelled");
  fsm.start_single_timer("replaced", Message::new("first"), Duration::from_millis(50));
  fsm.start_single_timer("replaced", Message::new("second"), Duration::from_millis(100));
  spec.await_cond(|| received.lock().unwrap().contains(&"second"), SETTLE_MAX);
  spec.test_kit_mut().expect_no_message(Duration::from_millis(200));
  settle(&mut spec, &fsm);
  assert_eq!(*received.lock().unwrap(), vec!["second"]);
  spec.terminate();
}

// I9: a single timer delivers its message once and leaves the table; a
// repeating timer stays in the table until it is cancelled, and delivers
// nothing after that.
#[test]
fn a_single_timer_leaves_the_table_after_it_fires_and_a_repeating_one_stays() {
  let Machine { mut spec, fsm, received, .. } = start_timeout_machine(true);
  fsm.start_single_timer("once", Message::new("once"), Duration::from_millis(20));
  spec.await_cond(|| received.lock().unwrap().contains(&"once"), SETTLE_MAX);
  settle(&mut spec, &fsm);
  assert!(!fsm.is_timer_active("once"));

  let count = |received: &Received, tag: &str| received.lock().unwrap().iter().filter(|seen| **seen == tag).count();
  fsm.start_timer_at_fixed_rate("rate", Message::new("rate"), Duration::from_millis(10));
  fsm.start_timer_with_fixed_delay("delay", Message::new("delay"), Duration::from_millis(10));
  spec.await_cond(|| count(&received, "rate") >= 3 && count(&received, "delay") >= 3, SETTLE_MAX);
  settle(&mut spec, &fsm);
  assert!(fsm.is_timer_active("rate"));
  assert!(fsm.is_timer_active("delay"));
  fsm.cancel_timer("rate");
  fsm.cancel_timer("delay");
  settle(&mut spec, &fsm);
  assert!(!fsm.is_timer_active("rate"));
  assert!(!fsm.is_timer_active("delay"));
  let (rate, delay) = (count(&received, "rate"), count(&received, "delay"));
  spec.test_kit_mut().expect_no_message(Duration::from_millis(100));
  settle(&mut spec, &fsm);
  assert_eq!(count(&received, "rate"), rate);
  assert_eq!(count(&received, "delay"), delay);
  assert_eq!(count(&received, "once"), 1);
  spec.terminate();
}

// Section 5: the empty string is a timer name like any other.
#[test]
fn the_empty_string_is_a_timer_name() {
  let Machine { mut spec, fsm, .. } = start_timeout_machine(true);
  fsm.start_timer_with_fixed_delay("", Message::new("timer"), NEVER);
  settle(&mut spec, &fsm);
  assert!(fsm.is_timer_active(""));
  assert!(!fsm.is_timer_active(" "));
  fsm.cancel_timer("");
  settle(&mut spec, &fsm);
  assert!(!fsm.is_timer_active(""));
  spec.terminate();
}

// Section 5: a zero state timeout fires at the next chance.
#[test]
fn a_zero_state_timeout_fires() {
  let Machine { mut spec, fsm, .. } = start_timeout_machine(true);
  fsm.set_state(None, None, Some(Duration::ZERO));
  spec.await_cond(|| fsm.state_name() == 2 && fsm.state_data() == "timeout", SETTLE_MAX);
  spec.terminate();
}

// I10, section 5: a machine restarted after a failure starts again from
// its initial state, with an empty timer table and no state timer, and the
// same `TestFsmRef` reads the new instance.
#[test]
fn a_restarted_machine_starts_over() {
  let Machine { mut spec, fsm, .. } = start_timeout_machine(true);
  fsm.set_state(Some(2), Some("moved".to_string()), Some(NEVER));
  fsm.start_timer_with_fixed_delay("t", Message::new("timer"), NEVER);
  settle(&mut spec, &fsm);
  assert_eq!(fsm.state_name(), 2);
  assert!(fsm.is_timer_active("t"));
  fsm.tell("boom", no_sender(&spec));
  settle(&mut spec, &fsm);
  assert_eq!(fsm.state_name(), 1);
  assert_eq!(fsm.state_data(), "");
  assert!(!fsm.is_timer_active("t"));
  assert!(!fsm.is_state_timer_active());
  fsm.set_state(Some(2), Some("again".to_string()), None);
  settle(&mut spec, &fsm);
  assert_eq!(fsm.state_name(), 2);
  assert_eq!(fsm.state_data(), "again");
  spec.terminate();
}

// I3, section 5: `set_state` into the current state is reported to a
// subscribed listener as a transition, as Pekko's `State` notifies by
// default.
#[test]
fn set_state_into_the_current_state_notifies_a_listener() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let mut probe = TestProbe::new(spec.system_mut());
  let fsm = TestFsmRef::new(spec.system_mut(), idle_machine);
  fsm.tell(SubscribeTransitionCallBack(probe.ref_handle()), no_sender(&spec));
  let current: CurrentState<u8> = probe.expect_msg_type();
  assert_eq!(current, CurrentState { fsm: fsm.actor_ref(), state: 0 });
  fsm.set_state(Some(0), None, None);
  let transition: Transition<u8> = probe.expect_msg_type();
  assert_eq!(transition, Transition { fsm: fsm.actor_ref(), from: 0, to: 0 });
  spec.terminate();
}
