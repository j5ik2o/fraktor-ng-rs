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

// Ported from Pekko v2.0.0:
//   testkit/src/test/scala/org/apache/pekko/testkit/TestFSMRefSpec.scala
//
// Differences from the original, recorded in the design of issue #181
// (docs/design/issue-181-testfsmref.md):
// - The original's actors mix `FSM` into an `Actor`; the port builds the
//   same machines as `Fsm` values in the factory that `TestFsmRef` takes,
//   as `TestFSMRef` takes the actor's constructor by name.
// - The original's `TestActorRef` runs on the CallingThreadDispatcher, so a
//   message or `setState` is handled before the next line runs. Here the
//   messages are queued, so each case sends the numbered no-op control
//   message after them and waits until the machine has processed it before
//   reading (section 1, note 1). The assertions are the original's.
// - `underlyingActor.supervisor` and `underlyingActor.name` are fields the
//   original's actor records from `context.parent` and
//   `context.self.path.name`. The port does not hand out the actor, so the
//   machine records `context.self_ref()` from inside its state function,
//   and the cases read the parent and the name from that path, from the
//   handle's own path, and from what `TestFsmRef` reports (section 3.3).
// - `null` state data is `()`.

use core::time::Duration;

use std::sync::Arc;
use std::sync::Mutex;

use fraktor_actor::kernel::actor::{Actor, ActorContext, Fsm, FsmState, FsmView, Props, StateTimeout};
use fraktor_actor::kernel::actor_path::ActorPath;
use fraktor_actor::kernel::actor_ref::ActorRef;
use fraktor_actor::kernel::message::Message;
use fraktor_testkit::fraktorspec::FraktorSpec;
use fraktor_testkit::test_fsm_ref::TestFsmRef;

const SPEC_SYSTEM_NAME: &str = "TestFSMRefSpec";

/// How long a case waits for the machine to process what it was sent.
const SETTLE_MAX: Duration = Duration::from_secs(3);

/// Waits until the machine has processed everything sent to it so far: the
/// mailbox is FIFO, so once the sentinel sent last is processed, every
/// message and control message sent before it is too.
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

// Original: the anonymous `FSM[Int, String]` of "allow access to state data".
fn state_data_fsm() -> Fsm<i32, String> {
  let mut fsm = Fsm::new();
  fsm.start_with(1, String::new());
  fsm.when(1, |_context: &mut ActorContext<'_>, _view: &FsmView<i32>, event: &Message, _data: &String| {
    if event.is::<StateTimeout>() {
      return Some(FsmState::goto(2).using("timeout".to_string()));
    }
    match *event.downcast_ref::<&'static str>()? {
      | "go" => Some(FsmState::goto(2).using("go".to_string())),
      | _ => None,
    }
  });
  fsm.when(2, |_context: &mut ActorContext<'_>, _view: &FsmView<i32>, event: &Message, _data: &String| {
    match *event.downcast_ref::<&'static str>()? {
      | "back" => Some(FsmState::goto(1).using("back".to_string())),
      | _ => None,
    }
  });
  fsm
}

// Original: the anonymous `FSM[Int, Null]` of "allow access to timers".
fn timer_fsm() -> Fsm<i32, ()> {
  let mut fsm = Fsm::new();
  fsm.start_with(1, ());
  fsm.when(1, |_context: &mut ActorContext<'_>, _view: &FsmView<i32>, _event: &Message, _data: &()| {
    Some(FsmState::stay())
  });
  fsm
}

/// What the machine saw as itself, recorded from inside its state function.
type SeenSelf = Arc<Mutex<Option<ActorRef>>>;

// Original: `TestFSMActor`, which records `context.parent` and
// `context.self.path.name`. The port records `context.self_ref()`, whose
// path holds both.
fn test_fsm_actor(seen: SeenSelf) -> Fsm<i32, ()> {
  let mut fsm = Fsm::new();
  fsm.start_with(1, ());
  fsm.when(1, move |context: &mut ActorContext<'_>, _view: &FsmView<i32>, _event: &Message, _data: &()| {
    *seen.lock().unwrap() = Some(context.self_ref());
    Some(FsmState::stay())
  });
  fsm
}

fn test_fsm_actor_factory(seen: &SeenSelf) -> impl Fn() -> Fsm<i32, ()> + Send + Sync + 'static {
  let seen = seen.clone();
  move || test_fsm_actor(seen.clone())
}

/// Asks the machine who it is and returns the handle it saw as itself.
fn self_seen_by_actor(spec: &mut FraktorSpec, fsm: &TestFsmRef<i32, ()>, seen: &SeenSelf) -> ActorRef {
  fsm.tell("who-am-i", no_sender(spec));
  settle(spec, fsm);
  seen.lock().unwrap().clone().expect("the machine handled the message")
}

// Original: `system.asInstanceOf[ActorSystemImpl].guardian`.
fn guardian(spec: &FraktorSpec) -> ActorRef {
  let path =
    ActorPath::from_string(&format!("pekko://{SPEC_SYSTEM_NAME}")).expect("the root path parses").with_child("user");
  spec.system().resolve_actor_ref(&path)
}

// Original: `system.actorOf(Props(new Actor { def receive = { case _ => } }))`.
struct Idle;

impl Actor for Idle {
  fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {}
}

/// Checks that the machine runs as a child of `supervisor` under `name`,
/// both as the test sees it and as the machine sees itself.
fn assert_started_under(
  spec: &mut FraktorSpec,
  fsm: &TestFsmRef<i32, ()>,
  seen: &SeenSelf,
  supervisor: &ActorRef,
  name: &str,
) {
  assert_eq!(fsm.supervisor(), *supervisor);
  assert_eq!(fsm.name(), name);
  let actor = fsm.actor_ref();
  assert_eq!(actor.path().parent(), supervisor.path());
  assert_eq!(actor.path().name(), name);
  let itself = self_seen_by_actor(spec, fsm, seen);
  assert_eq!(itself, actor);
  assert_eq!(itself.path().parent(), supervisor.path());
  assert_eq!(itself.path().name(), name);
}

// Original: "A TestFSMRef must allow access to state data"
#[test]
fn allows_access_to_state_data() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let fsm = TestFsmRef::with_name(spec.system_mut(), state_data_fsm, "test-fsm-ref-1").expect("the name is free");
  assert_eq!(fsm.state_name(), 1);
  assert_eq!(fsm.state_data(), "");
  fsm.tell("go", no_sender(&spec));
  settle(&mut spec, &fsm);
  assert_eq!(fsm.state_name(), 2);
  assert_eq!(fsm.state_data(), "go");
  fsm.set_state(Some(1), None, None);
  settle(&mut spec, &fsm);
  assert_eq!(fsm.state_name(), 1);
  assert_eq!(fsm.state_data(), "go");
  fsm.set_state(None, Some("buh".to_string()), None);
  settle(&mut spec, &fsm);
  assert_eq!(fsm.state_name(), 1);
  assert_eq!(fsm.state_data(), "buh");
  fsm.set_state(None, None, Some(Duration::from_millis(100)));
  spec.within_range(Duration::from_millis(80), Duration::from_millis(500), |spec| {
    let max = spec.remaining();
    spec.await_cond(|| fsm.state_name() == 2 && fsm.state_data() == "timeout", max);
  });
  spec.terminate();
}

// Original: "A TestFSMRef must allow access to timers"
#[test]
fn allows_access_to_timers() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let fsm = TestFsmRef::with_name(spec.system_mut(), timer_fsm, "test-fsm-ref-2").expect("the name is free");
  assert!(!fsm.is_timer_active("test"));
  fsm.start_timer_with_fixed_delay("test", Message::new(12), Duration::from_millis(10));
  settle(&mut spec, &fsm);
  assert!(fsm.is_timer_active("test"));
  fsm.cancel_timer("test");
  settle(&mut spec, &fsm);
  assert!(!fsm.is_timer_active("test"));
  spec.terminate();
}

// Original: "A TestFSMRef Companion Object must allow creation of a
// TestFSMRef with a default supervisor"
#[test]
fn allows_creation_with_a_default_supervisor() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let guardian = guardian(&spec);
  let seen = SeenSelf::default();
  let fsm = TestFsmRef::new(spec.system_mut(), test_fsm_actor_factory(&seen));
  let name = fsm.name().to_string();
  assert_started_under(&mut spec, &fsm, &seen, &guardian, &name);
  spec.terminate();
}

// Original: "A TestFSMRef Companion Object must allow creation of a
// TestFSMRef with a specified name"
#[test]
fn allows_creation_with_a_specified_name() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let guardian = guardian(&spec);
  let seen = SeenSelf::default();
  let fsm =
    TestFsmRef::with_name(spec.system_mut(), test_fsm_actor_factory(&seen), "fsmActor").expect("the name is free");
  assert_started_under(&mut spec, &fsm, &seen, &guardian, "fsmActor");
  spec.terminate();
}

// Original: "A TestFSMRef Companion Object must allow creation of a
// TestFSMRef with a specified supervisor"
#[test]
fn allows_creation_with_a_specified_supervisor() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let parent = spec.system_mut().actor_of(Props::new(|| Idle));
  let seen = SeenSelf::default();
  let fsm = TestFsmRef::with_supervisor(spec.system_mut(), test_fsm_actor_factory(&seen), &parent);
  let name = fsm.name().to_string();
  assert_started_under(&mut spec, &fsm, &seen, &parent, &name);
  spec.terminate();
}

// Original: "A TestFSMRef Companion Object must allow creation of a
// TestFSMRef with a specified supervisor and name"
#[test]
fn allows_creation_with_a_specified_supervisor_and_a_name() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let parent = spec.system_mut().actor_of(Props::new(|| Idle));
  let seen = SeenSelf::default();
  let fsm = TestFsmRef::with_supervisor_and_name(
    spec.system_mut(),
    test_fsm_actor_factory(&seen),
    &parent,
    "supervisedFsmActor",
  )
  .expect("the name is free");
  assert_started_under(&mut spec, &fsm, &seen, &parent, "supervisedFsmActor");
  spec.terminate();
}
