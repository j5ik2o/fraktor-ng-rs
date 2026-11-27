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

// Property-based tests of `TestActorRef` (docs/design/issue-182-test-actor-ref.md
// section 6, P1 to P6).
//
// These are not ported from a Pekko test file and are not counted in the
// ledger. Each property draws its inputs from a fixed-seed SplitMix64, so a
// failure repeats, and every failure message names the seed and the case. They
// check the invariants of section 4 (I1 to I11) that the ported cases cannot
// reach on their own.
//
// Where a property cannot observe a value the design names, the reason is
// written next to it:
// - P1 checks the parent and name conditions (I1, I8, I9) through the parent
//   path the created actor reports, and through what the `in_context` child
//   reports, so the check does not depend on the system's name.
// - P3 cannot read `ActorHandle::processed_sequence` or `ActorHandle::generation`:
//   the reference keeps its handle private. The property reads the generation
//   from the actor's own snapshot, the way the ported cases read
//   `underlyingActor`, and checks the same claim: the read after a restart is
//   the new instance's starting state, and the generation rises.
// - P4 checks the kinds the reference reports (I2). An undefined name is
//   refused, but the dispatcher is verified inside the actor's start, whose
//   failure the guardian's restarting strategy retries: a refused start would
//   never settle, so the undefined case is pinned once in a fixed test rather
//   than driven by a `from_props` that would wait.
// - P6 cannot read the number of queued messages: no public entry reports a
//   mailbox's length. The property checks the order instead: after the direct
//   receive, the queued messages are echoed in the order they were sent, and
//   the actor handled exactly one more message than it was queued.

use core::any::Any;
use std::panic::{self, AssertUnwindSafe};
use std::sync::{Arc, Mutex};
use std::thread::ThreadId;
use std::time::Instant;

use fraktor_actor::kernel::actor::{Actor, ActorContext, DeadLetter, Props, ReceiveTimeoutSetting};
use fraktor_actor::kernel::actor_path::ActorPath;
use fraktor_actor::kernel::actor_ref::ActorRef;
use fraktor_actor::kernel::actor_system::ActorSystem;
use fraktor_actor::kernel::dispatch::{DispatcherConfig, DispatcherKind, DEFAULT_DISPATCHER_ID};
use fraktor_actor::kernel::event::EventClass;
use fraktor_actor::kernel::message::Message;
use fraktor_testkit::default_timeout::DefaultTimeout;
use fraktor_testkit::event_filter::EventFilter;
use fraktor_testkit::fraktorspec::FraktorSpec;
use fraktor_testkit::test_actor_ref::{TestActorRef, CALLING_THREAD_DISPATCHER_ID};

const SPEC_SYSTEM_NAME: &str = "TestActorRefPropertyTest";
const SEED: u64 = 0x0182_0000_C0FF_EE01;

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

// ---------------------------------------------------------------------------
// Helpers

fn user_guardian_path() -> ActorPath {
  ActorPath::from_string(&format!("pekko://{SPEC_SYSTEM_NAME}")).expect("the root path parses").with_child("user")
}

fn resolve(system: &ActorSystem, path: &ActorPath) -> ActorRef {
  system.resolve_actor_ref(path)
}

/// Original: `assertThread()`. A message handled off the test's thread is
/// recorded, so a case can check none was.
type OtherThread = Arc<Mutex<Option<ThreadId>>>;

fn record_off_test_thread(cell: &(ThreadId, OtherThread)) {
  let (test_thread, other_thread) = cell;
  let current = std::thread::current().id();
  if current != *test_thread {
    *other_thread.lock().unwrap() = Some(current);
  }
}

fn assert_thread(cell: &(ThreadId, OtherThread)) {
  let (test_thread, other_thread) = cell;
  let recorded = *other_thread.lock().unwrap();
  assert!(recorded.is_none() || recorded == Some(*test_thread), "a message was handled on another thread");
}

fn new_cell() -> (ThreadId, OtherThread) {
  (std::thread::current().id(), Arc::new(Mutex::new(None)))
}

/// An actor that handles nothing, for the supervisors and the dispatcher cases.
struct Idle;

impl Actor for Idle {
  fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {}
}

/// An actor that records, for the companion object's observation, the handle
/// it saw as itself in `pre_start` (section 1, note 9).
#[derive(Clone)]
struct SeenSelf {
  self_ref: ActorRef,
}

struct SeenActor {
  seen: Arc<Mutex<Option<SeenSelf>>>,
}

impl SeenActor {
  fn new(seen: &Arc<Mutex<Option<SeenSelf>>>) -> Self {
    Self { seen: seen.clone() }
  }
}

impl Actor for SeenActor {
  fn pre_start(&mut self, context: &mut ActorContext<'_>) {
    *self.seen.lock().unwrap() = Some(SeenSelf { self_ref: context.self_ref() });
  }

  fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {}

  fn underlying_snapshot(&self) -> Option<Box<dyn Any + Send + Sync>> {
    let seen = self.seen.lock().unwrap().clone();
    seen.map(|seen| Box::new(seen) as Box<dyn Any + Send + Sync>)
  }
}

// ---------------------------------------------------------------------------
// P1: creation below a supervisor, under a name (I1, I8, I9)

#[derive(Clone, Copy, Debug)]
enum ParentKind {
  Guardian,
  GuardianExplicit,
  StartedActor,
  TestActor,
}

fn random_parent(rng: &mut SplitMix64) -> ParentKind {
  rng.pick(&[
    ParentKind::Guardian,
    ParentKind::Guardian,
    ParentKind::GuardianExplicit,
    ParentKind::StartedActor,
    ParentKind::TestActor,
  ])
}

/// A name P1 draws: a reused name, a name that starts with `$`, a long name,
/// the empty name, or a fresh name.
///
/// `$` and the empty name are refused by `ActorPath::validate_path_element`;
/// the reused name is in use before the case runs. A wildcard is not drawn,
/// because `*` is one of the characters a path element may hold, so the
/// kernel accepts it and the design's "wildcard" is a legal name.
fn random_name(rng: &mut SplitMix64, reused: &str) -> String {
  match rng.below(6) {
    | 0 => reused.to_string(),
    | 1 => "$refused".to_string(),
    | 2 => "x".repeat(200),
    | 3 => format!("long{}", "y".repeat(150)),
    | 4 => String::new(),
    | _ => format!("n{}", rng.below(10_000)),
  }
}

/// Whether `name` is one the kernel accepts below the case's supervisor: the
/// empty name and a leading `$` are refused everywhere, and the reused name is
/// refused only below the guardian, where the case occupies it.
fn name_is_accepted(name: &str, reused: &str, parent: ParentKind) -> bool {
  let reused_here = matches!(parent, ParentKind::Guardian | ParentKind::GuardianExplicit) && name == reused;
  !name.is_empty() && !name.starts_with('$') && !reused_here
}

#[test]
fn p1_a_creation_lands_below_the_supervisor_under_the_name() {
  let mut rng = SplitMix64::new(SEED);
  for case in 0..40u64 {
    let parent_kind = random_parent(&mut rng);
    let reused = format!("reused{case}");
    let name = random_name(&mut rng, &reused);
    let accepted = name_is_accepted(&name, &reused, parent_kind);
    let context = format!("seed {SEED:#x} case {case} parent {parent_kind:?} name {name:?}");

    let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
    let guardian = resolve(spec.system(), &user_guardian_path());
    // The name is in use below the guardian before the case runs, so the reused
    // draw is refused there.
    let _occupant = spec.system_mut().actor_of_named(Props::new(|| Idle), &reused).expect("the first name is free");
    let (parent, parent_path) = match parent_kind {
      | ParentKind::Guardian | ParentKind::GuardianExplicit => (guardian.clone(), guardian.path().clone()),
      | ParentKind::StartedActor => {
        let parent = spec.system_mut().actor_of(Props::new(|| Idle));
        // A supervisor must have started: the actor starts in the run that
        // handles its `Create`, so the system is driven before the child.
        spec.system_mut().run_until_idle();
        (parent.clone(), parent.path().clone())
      },
      | ParentKind::TestActor => {
        let parent = spec.test_actor();
        // A supervisor must have started. The test actor starts on the run
        // that handles its first message, so a message is sent through it and
        // its reply is read before the child is made.
        parent.tell("start", parent.clone());
        spec.system_mut().run_until_idle();
        spec.expect_msg("start");
        (resolve(spec.system(), &parent.path().clone()), parent.path().clone())
      },
    };

    let seen = Arc::new(Mutex::new(None));
    // Omitting the supervisor and naming the `/user` guardian are the same
    // creation: both place the actor directly below the guardian.
    let created = if matches!(parent_kind, ParentKind::Guardian) {
      let seen = seen.clone();
      TestActorRef::with_name(spec.system_mut(), move || SeenActor::new(&seen), &name)
    } else {
      let seen = seen.clone();
      TestActorRef::with_supervisor_and_name(spec.system_mut(), move || SeenActor::new(&seen), &parent, &name)
    };

    match created {
      | Ok(reference) => {
        assert!(accepted, "{context}: a name the kernel should refuse was accepted");
        assert_eq!(reference.name(), name, "{context}: the actor was started under another name");
        assert_eq!(reference.actor_ref().path().parent(), parent_path, "{context}: the actor has another parent");
        assert_eq!(reference.supervisor(), parent, "{context}: the reported supervisor is not the one asked for");
        // I1: the reference of the parent path resolves to the supervisor.
        assert_eq!(resolve(spec.system(), &reference.actor_ref().path().parent()), parent, "{context}");
        let snapshot: SeenSelf = reference.underlying_snapshot();
        assert_eq!(snapshot.self_ref, reference.actor_ref(), "{context}: the actor saw another reference");
        assert_eq!(snapshot.self_ref.path().parent(), parent_path, "{context}: the actor saw another parent");
      },
      | Err(_) => {
        assert!(!accepted, "{context}: a name the kernel should accept was refused");
        // A refusal leaves the supervisor and its other children as they were.
        assert_eq!(resolve(spec.system(), &parent_path), parent, "{context}: a refusal changed the supervisor");
      },
    }
    spec.terminate();
  }
}

/// Builds a nested `TestActorRef` from the parent's `pre_start`, with the
/// supervisor and name the case draws, and answers the ask with it. This is
/// the `in_context` observation of I9.
struct NestedInContext {
  supervisor: Option<ActorRef>,
  name: Option<String>,
  nested: Option<ActorRef>,
}

impl Actor for NestedInContext {
  fn pre_start(&mut self, context: &mut ActorContext<'_>) {
    let supervisor = self.supervisor.clone();
    let name = self.name.clone();
    let nested = TestActorRef::in_context(context, Props::new(|| Idle), supervisor.as_ref(), name.as_deref())
      .expect("the nested actor starts");
    self.nested = Some(nested.actor_ref());
  }

  fn receive(&mut self, context: &mut ActorContext<'_>, _message: Message) {
    let sender = context.sender();
    match self.nested.as_ref() {
      | Some(nested) => context.tell(&sender, nested.clone()),
      | None => panic!("the nested actor was not built"),
    }
  }
}

#[test]
fn p1_in_context_builds_below_the_supervisor_it_is_given() {
  let mut rng = SplitMix64::new(SEED ^ 0x11);
  for case in 0..16u64 {
    let give_supervisor = rng.chance(50);
    let give_name = rng.chance(50);
    let context = format!("seed {SEED:#x} case {case} supervisor {give_supervisor} name {give_name}");

    let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
    let guardian = resolve(spec.system(), &user_guardian_path());
    let supervisor = if give_supervisor {
      let started = spec.system_mut().actor_of(Props::new(|| Idle));
      Some(resolve(spec.system(), &started.path().clone()))
    } else {
      None
    };
    let name = if give_name { Some(format!("inContext{case}")) } else { None };
    let outer = TestActorRef::from_props(
      spec.system_mut(),
      Props::new({
        let supervisor = supervisor.clone();
        let name = name.clone();
        move || NestedInContext { supervisor: supervisor.clone(), name: name.clone(), nested: None }
      }),
    );
    let reply = outer.actor_ref().ask("any").await_result(DefaultTimeout::timeout(spec.system()).duration());
    let nested = reply.downcast_ref::<ActorRef>().cloned().expect("the reply is an ActorRef");

    let expected_parent = supervisor.clone().unwrap_or_else(|| guardian.clone());
    assert_eq!(nested.path().parent(), expected_parent.path(), "{context}: the child has another parent");
    assert_eq!(resolve(spec.system(), &nested.path().parent()), expected_parent, "{context}");
    if let Some(name) = &name {
      assert_eq!(nested.path().name(), name, "{context}: the child has another name");
    }
    assert_ne!(nested, outer.actor_ref(), "{context}: the child is the outer actor");
    spec.terminate();
  }
}

// ---------------------------------------------------------------------------
// P2: a random message sequence against an actor that publishes its state
// (I4, I5)

/// Counts the strings it has seen and adds the numbers it has seen. `"add"`
/// switches it to adding, `"count"` back to counting; `"wait"` stashes the
/// message being handled and `"go"` returns the stashed messages to the front
/// of the mailbox, after the one being handled.
struct StateActor {
  count: i64,
  sum: i64,
  counting: bool,
}

impl StateActor {
  fn new() -> Self {
    Self { count: 0, sum: 0, counting: true }
  }
}

impl Actor for StateActor {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    if let Some(text) = message.downcast_ref::<&'static str>() {
      match *text {
        | "count" => {
          self.counting = true;
          return;
        },
        | "add" => {
          self.counting = false;
          return;
        },
        | "wait" => {
          context.stash();
          return;
        },
        | "go" => {
          context.unstash_all();
          return;
        },
        | _ => {
          if self.counting {
            self.count += 1;
          }
          return;
        },
      }
    }
    if let Some(value) = message.downcast_ref::<i64>() {
      self.sum += value;
    }
  }

  fn underlying_snapshot(&self) -> Option<Box<dyn Any + Send + Sync>> {
    Some(Box::new((self.count, self.sum)))
  }
}

#[derive(Clone, Copy, Debug)]
enum P2Step {
  Text,
  Number(i64),
  Add,
  Count,
  Wait,
  Go,
}

/// The model of a sequence: it runs the same queue the actor's mailbox runs.
/// A `"wait"` stashes the message being handled, and a `"go"` returns the
/// stashed messages to the front of the queue, before the ones still queued.
#[derive(Default)]
struct P2Model {
  count: i64,
  sum: i64,
  counting: bool,
  stashed: Vec<P2Step>,
  queued: std::collections::VecDeque<P2Step>,
}

impl P2Model {
  fn new() -> Self {
    Self { count: 0, sum: 0, counting: true, stashed: Vec::new(), queued: std::collections::VecDeque::new() }
  }

  fn handle(&mut self, step: P2Step) {
    match step {
      | P2Step::Text => {
        if self.counting {
          self.count += 1;
        }
      },
      | P2Step::Number(value) => self.sum += value,
      | P2Step::Add => self.counting = false,
      | P2Step::Count => self.counting = true,
      | P2Step::Wait => self.stashed.push(step),
      | P2Step::Go => {
        for stashed in self.stashed.drain(..).rev() {
          self.queued.push_front(stashed);
        }
      },
    }
  }

  /// Sends the next step, then handles everything the queue now holds: the
  /// actor drains its mailbox the same way before the case reads the state.
  fn send(&mut self, step: P2Step) {
    self.queued.push_back(step);
    while let Some(next) = self.queued.pop_front() {
      // `go` returns the stashed steps to the front, so the queue shrinks
      // and grows again in the same order the actor sees.
      self.handle(next);
    }
  }
}

fn random_p2_step(rng: &mut SplitMix64) -> P2Step {
  match rng.below(8) {
    | 0..=2 => P2Step::Text,
    | 3 | 4 => P2Step::Number(rng.below(21) as i64 - 10),
    | 5 => {
      if rng.chance(50) {
        P2Step::Add
      } else {
        P2Step::Count
      }
    },
    | 6 => P2Step::Wait,
    | _ => P2Step::Go,
  }
}

fn tell_step(reference: &TestActorRef, step: P2Step, sender: &ActorRef) {
  match step {
    | P2Step::Text => reference.tell("text", sender.clone()),
    | P2Step::Number(value) => reference.tell(value, sender.clone()),
    | P2Step::Add => reference.tell("add", sender.clone()),
    | P2Step::Count => reference.tell("count", sender.clone()),
    | P2Step::Wait => reference.tell("wait", sender.clone()),
    | P2Step::Go => reference.tell("go", sender.clone()),
  }
}

#[test]
fn p2_the_snapshot_matches_the_sequence_run_by_a_model() {
  let mut rng = SplitMix64::new(SEED ^ 0x2);
  for case in 0..20u64 {
    let sequence: Vec<P2Step> = (0..rng.below(12)).map(|_| random_p2_step(&mut rng)).collect();
    let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
    let sender = spec.test_actor();
    let reference = TestActorRef::from_props(spec.system_mut(), Props::new(StateActor::new));
    let mut model = P2Model::new();
    // I5 before the sequence: the probe waits for the start, so every message
    // the case sends is handled after it and none waits behind it.
    let _: (i64, i64) = reference.underlying_snapshot();
    for (step, event) in sequence.iter().enumerate() {
      tell_step(&reference, *event, &sender);
      model.send(*event);
      let snapshot: (i64, i64) = reference.underlying_snapshot();
      let expected = (model.count, model.sum);
      assert_eq!(expected, snapshot, "seed {SEED:#x} case {case} step {step} event {event:?}");
    }
    // Sequences whose state returns to an earlier value are part of the draw;
    // the fixed test below pins one so a returned state is read again.
    spec.terminate();
  }
}

// A sequence that goes away and comes back: the reads at the two ends agree,
// and the middle read sees the change.
#[test]
fn p2_a_sequence_that_returns_to_its_start() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let sender = spec.test_actor();
  let reference = TestActorRef::from_props(spec.system_mut(), Props::new(StateActor::new));
  let start: (i64, i64) = reference.underlying_snapshot();
  reference.tell(7i64, sender.clone());
  let middle: (i64, i64) = reference.underlying_snapshot();
  reference.tell(-7i64, sender);
  let end: (i64, i64) = reference.underlying_snapshot();
  assert_eq!(start, end, "a sequence that goes away and comes back returns the starting state");
  assert_ne!(start, middle, "the middle read sees the number added");
  spec.terminate();
}

// ---------------------------------------------------------------------------
// P3: a restart leaves the reads on the new instance, and the generation rises
// (I6, I4)

#[derive(Clone, Copy, Debug)]
struct RestartSnapshot {
  counter: i64,
  generation: u64,
}

/// Counts the numbers it has seen and reports, with the count, how many times
/// it has been restarted. `"boom"` fails the behaviour, which the guardian
/// restarts (Pekko's default strategy).
struct RestartingActor {
  counter: i64,
  generation: u64,
}

impl RestartingActor {
  fn new() -> Self {
    Self { counter: 0, generation: 0 }
  }
}

impl Actor for RestartingActor {
  fn receive(&mut self, _context: &mut ActorContext<'_>, message: Message) {
    if message.downcast_ref::<&'static str>() == Some(&"boom") {
      panic!("expected");
    }
    if let Some(value) = message.downcast_ref::<i64>() {
      self.counter += value;
    }
  }

  fn post_restart(&mut self, _context: &mut ActorContext<'_>) {
    self.generation += 1;
  }

  fn underlying_snapshot(&self) -> Option<Box<dyn Any + Send + Sync>> {
    Some(Box::new(RestartSnapshot { counter: self.counter, generation: self.generation }))
  }
}

fn run_all(system: &mut ActorSystem) {
  for _ in 0..4096 {
    if !system.run_one() {
      return;
    }
  }
}

/// Fails the actor so the guardian restarts it, and checks that the read after
/// the restart is the new instance's start and that its generation rose.
///
/// The guardian's default one-for-one strategy restarts a failed child a
/// limited number of times (Pekko's `maxNrOfRetries = 1`), so a longer run
/// would stop the actor; the property stays inside that limit. A `Kill` is
/// not used here: `Kill` is stopped by that strategy's decider, so it would
/// stop the actor instead of restarting it.
fn restart_once(spec: &mut FraktorSpec, reference: &TestActorRef) -> RestartSnapshot {
  let sender = spec.test_actor();
  reference.tell(5i64, sender.clone());
  let before: RestartSnapshot = reference.underlying_snapshot();
  EventFilter::typed("RuntimeException").with_message("expected").intercept_any(spec.system_mut(), || {
    reference.tell("boom", sender);
  });
  run_all(spec.system_mut());
  let after: RestartSnapshot = reference.underlying_snapshot();
  assert_eq!(after.generation, before.generation + 1, "the generation did not rise");
  assert_eq!(after.counter, 0, "the read is not the new instance's start");
  after
}

#[test]
fn p3_a_restart_leaves_the_reads_on_the_new_instance() {
  let mut rng = SplitMix64::new(SEED ^ 0x3);
  for case in 0..10u64 {
    let before_restart = rng.below(3) + 1;
    let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
    let sender = spec.test_actor();
    let reference = TestActorRef::from_props(spec.system_mut(), Props::new(RestartingActor::new));
    // Numbers arrive before the restart, so a read that showed the old
    // instance would be visible if the restart kept it.
    for _ in 0..before_restart {
      reference.tell(1i64, sender.clone());
      run_all(spec.system_mut());
    }
    let snapshot = restart_once(&mut spec, &reference);
    assert_eq!(snapshot.counter, 0, "seed {SEED:#x} case {case}: the old instance's numbers survive the restart");
    spec.terminate();
  }
}

#[test]
fn p3_the_generation_rises_at_every_restart() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let reference = TestActorRef::from_props(spec.system_mut(), Props::new(RestartingActor::new));
  run_all(spec.system_mut());
  let snapshot = restart_once(&mut spec, &reference);
  assert_eq!(snapshot.generation, 1, "the first restart leaves the first generation");
  spec.terminate();
}

// ---------------------------------------------------------------------------
// P4: the kind of dispatcher a recipe names (I2)

#[test]
fn p4_the_dispatcher_kind_follows_the_recipe() {
  let mut rng = SplitMix64::new(SEED ^ 0x4);
  for case in 0..40u64 {
    let choice = rng.below(5);
    let mut spec = FraktorSpec::with_dispatchers(
      SPEC_SYSTEM_NAME,
      vec![DispatcherConfig { id: "pinned".to_string(), kind: DispatcherKind::Pinned }],
    );
    let (dispatcher, expected) = match choice {
      | 0 => (None, Some(DispatcherKind::CallingThread)),
      | 1 => (Some(DEFAULT_DISPATCHER_ID.to_string()), Some(DispatcherKind::Default)),
      | 2 => (Some(CALLING_THREAD_DISPATCHER_ID.to_string()), Some(DispatcherKind::CallingThread)),
      | 3 => (Some("pinned".to_string()), Some(DispatcherKind::Pinned)),
      | _ => (Some("nowhere".to_string()), None),
    };
    let context = format!("seed {SEED:#x} case {case} dispatcher {dispatcher:?}");
    match expected {
      | Some(kind) => {
        let mut props = Props::new(|| Idle);
        if let Some(name) = &dispatcher {
          props = props.with_dispatcher(name);
        }
        let reference = TestActorRef::from_props(spec.system_mut(), props);
        assert_eq!(reference.dispatcher_kind(), Some(kind), "{context}");
      },
      | None => {
        // The undefined name is the fixed test below: the kind is `None`, and
        // the creation is refused. It is not driven through `from_props` here
        // because a refused creation's start may be retried and not settle.
        let name = dispatcher.expect("the undefined case names a dispatcher");
        assert_eq!(name, "nowhere", "{context}");
      },
    }
    spec.terminate();
  }
}

#[test]
fn p4_an_undefined_dispatcher_is_refused() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let props = Props::new(|| Idle).with_dispatcher("nowhere");
  let result = std::panic::catch_unwind(AssertUnwindSafe(|| TestActorRef::from_props(spec.system_mut(), props)));
  match result {
    | Ok(reference) => {
      // An undefined name resolves to no kind, so the reference reports none
      // whatever the start does with the refusal.
      assert_eq!(reference.dispatcher_kind(), None, "an undefined dispatcher must not resolve to a kind");
    },
    | Err(payload) => {
      let message = if let Some(text) = payload.downcast_ref::<String>() {
        text.clone()
      } else if let Some(text) = payload.downcast_ref::<&'static str>() {
        (*text).to_string()
      } else {
        String::from("<non-string panic>")
      };
      assert!(message.contains("nowhere"), "the refusal must name the dispatcher: {message}");
    },
  }
  spec.terminate();
}

// ---------------------------------------------------------------------------
// P5: a read after the stop ends at once, and the stop leaves no message for
// the stopped actor in dead letters (I7)

struct DeadLetterSink {
  dead: Arc<Mutex<Vec<Message>>>,
}

impl Actor for DeadLetterSink {
  fn receive(&mut self, _context: &mut ActorContext<'_>, message: Message) {
    if let Some(letter) = message.downcast_ref::<DeadLetter>() {
      self.dead.lock().unwrap().push(letter.message().clone());
    }
  }
}

fn panic_message(payload: &Box<dyn Any + Send>) -> String {
  if let Some(text) = payload.downcast_ref::<String>() {
    return text.clone();
  }
  if let Some(text) = payload.downcast_ref::<&'static str>() {
    return (*text).to_string();
  }
  String::from("<non-string panic>")
}

#[test]
fn p5_a_read_after_the_stop_ends_at_once() {
  let mut rng = SplitMix64::new(SEED ^ 0x5);
  for case in 0..10u64 {
    let stop_with = rng.pick(&["PoisonPill", "Kill"]);
    let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
    let sender = spec.test_actor();
    let dead = Arc::new(Mutex::new(Vec::new()));
    let sink = dead.clone();
    let subscriber = spec.system_mut().actor_of(Props::new(move || DeadLetterSink { dead: sink.clone() }));
    spec
      .system()
      .event_stream()
      .subscribe(Some(subscriber), EventClass::of::<DeadLetter>())
      .expect("the subscriber is not null");
    let reference = TestActorRef::from_props(spec.system_mut(), Props::new(|| Idle));
    let target = reference.actor_ref();
    let filter = EventFilter::typed("ActorKilledException");
    filter.intercept_any(spec.system_mut(), || reference.tell(stop_with, sender.clone()));
    spec.system_mut().run_until_idle();
    assert!(reference.is_terminated(), "seed {SEED:#x} case {case}: the actor did not stop");
    let started = Instant::now();
    let error = panic::catch_unwind(AssertUnwindSafe(|| {
      let _: i64 = reference.underlying_snapshot();
    }))
    .expect_err("reading a stopped actor panics");
    assert!(
      panic_message(&error).contains("underlying actor is terminated"),
      "seed {SEED:#x} case {case}: {}",
      panic_message(&error)
    );
    let limit = DefaultTimeout::timeout(spec.system()).duration();
    assert!(started.elapsed() < limit, "seed {SEED:#x} case {case}: the read waited for the timeout");
    // The stop leaves nothing addressed to the actor in dead letters. A read
    // that sent a probe after the stop would leave one there, so this is the
    // check that the read stopped before probing.
    let to_target = dead
      .lock()
      .unwrap()
      .iter()
      .filter(|letter| letter.downcast_ref::<DeadLetter>().is_some_and(|l| l.recipient() == &target))
      .count();
    assert_eq!(to_target, 0, "seed {SEED:#x} case {case}: a message for the stopped actor reached dead letters");
    spec.terminate();
  }
}

// ---------------------------------------------------------------------------
// P6: a direct receive leaves the mailbox in order (I11)

/// Echoes every number it handles to `echo_to`, and stops itself on "stop".
struct EchoActor {
  echo_to: ActorRef,
  cell: (ThreadId, OtherThread),
  handled: Arc<Mutex<i64>>,
}

impl Actor for EchoActor {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    record_off_test_thread(&self.cell);
    if message.downcast_ref::<&'static str>() == Some(&"stop") {
      let me = context.self_ref();
      context.stop(&me);
      return;
    }
    if let Some(value) = message.downcast_ref::<i64>() {
      *self.handled.lock().unwrap() += 1;
      context.tell(&self.echo_to.clone(), *value);
    }
  }

  fn underlying_snapshot(&self) -> Option<Box<dyn Any + Send + Sync>> {
    Some(Box::new(*self.handled.lock().unwrap()))
  }
}

#[test]
fn p6_a_direct_receive_keeps_the_queued_order() {
  let mut rng = SplitMix64::new(SEED ^ 0x6);
  for case in 0..20u64 {
    let queued: Vec<i64> = (0..rng.below(6)).map(|_| rng.below(100) as i64).collect();
    let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
    let echo_to = spec.test_actor();
    let cell = new_cell();
    let handled = Arc::new(Mutex::new(0i64));
    let reference = TestActorRef::from_props(
      spec.system_mut(),
      Props::new({
        let echo_to = echo_to.clone();
        let cell = cell.clone();
        let handled = handled.clone();
        move || EchoActor { echo_to: echo_to.clone(), cell: cell.clone(), handled: handled.clone() }
      }),
    );
    let sender = spec.test_actor();
    // A snapshot read before the direct receive: its probe waits for the
    // start, so the queue is drained and the messages below are handled after
    // it.
    let handled_before: i64 = reference.underlying_snapshot();
    for value in &queued {
      reference.tell(*value, sender.clone());
    }
    // The direct receive runs the behaviour once, outside the mailbox. Its
    // value is never one a queued message echoes, so the echoes are exactly
    // the queued values.
    reference.receive(-1i64);
    let mut observed: Vec<i64> = Vec::new();
    // The direct receive echoes `-1` too, so that reply is taken out of the
    // observation; the rest must arrive in the order the queue was sent.
    let mut seen_direct = false;
    for _ in 0..(queued.len() + 1) {
      let reply = spec.expect_msg_type::<i64>();
      if reply == -1 && !seen_direct {
        seen_direct = true;
      } else {
        observed.push(reply);
      }
    }
    assert!(seen_direct, "seed {SEED:#x} case {case}: the direct receive echoed no reply");
    let sorted_sent = {
      let mut sent = queued.clone();
      sent.sort_unstable();
      sent
    };
    let mut sorted_got = observed.clone();
    sorted_got.sort_unstable();
    assert_eq!(sorted_got, sorted_sent, "seed {SEED:#x} case {case}: a queued message was lost or repeated");
    // The design's claim: the queued messages keep their order.
    assert_eq!(observed, queued, "seed {SEED:#x} case {case}: the queued messages changed order");
    let handled_after: i64 = reference.underlying_snapshot();
    // The direct receive handles one message of its own; every queued message
    // is handled once more, so the total is one more than the queue.
    assert_eq!(
      handled_after - handled_before,
      queued.len() as i64 + 1,
      "seed {SEED:#x} case {case}: the queued messages were not handled once each"
    );
    assert_thread(&cell);
    spec.terminate();
  }
}

#[test]
fn p6_a_direct_receive_that_stops_the_actor_ends_it() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let echo_to = spec.test_actor();
  let cell = new_cell();
  let handled = Arc::new(Mutex::new(0i64));
  let reference = TestActorRef::from_props(
    spec.system_mut(),
    Props::new({
      let echo_to = echo_to.clone();
      let cell = cell.clone();
      move || EchoActor { echo_to: echo_to.clone(), cell: cell.clone(), handled: handled.clone() }
    }),
  );
  // The actor is started before the direct receive, so the stop below is the
  // one the direct receive itself queues and handles, not the start's.
  spec.system_mut().run_until_idle();
  assert!(!reference.is_terminated(), "the actor is running before the direct receive");
  reference.receive("stop");
  // The calling-thread kind handles the stop the receive queued before it
  // returns, so the actor is terminated at once.
  assert!(reference.is_terminated(), "a direct receive that stops the actor leaves it terminated");
  spec.terminate();
}

// ---------------------------------------------------------------------------
// I3, which P1 of the design carries in its classification draw: the probe a
// read sends is not outside the receive-timeout classification, so a read
// re-arms a receive timeout as Pekko's `InternalGetActor` does.

#[test]
fn i3_the_probe_is_not_outside_the_receive_timeout_classification() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let reference = TestActorRef::from_props(spec.system_mut(), Props::new(StateActor::new));
  let probe = Message::new("probe");
  assert!(
    !spec.system().is_not_influence_receive_timeout(&probe),
    "a message that is not classified must influence the receive timeout"
  );
  assert_eq!(reference.receive_timeout(), ReceiveTimeoutSetting::Undefined, "a fresh actor has no receive timeout");
  spec.terminate();
}
