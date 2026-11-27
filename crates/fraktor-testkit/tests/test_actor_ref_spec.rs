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
//   testkit/src/test/scala/org/apache/pekko/testkit/TestActorRefSpec.scala
// Original test names are kept as comments next to the ported cases.
//
// Design: docs/design/issue-182-test-actor-ref.md. Twenty of the original's
// twenty-three cases are ported here. Three cases are not in this file and
// are moved after stage 7b (#259), the CallingThreadDispatcher, without a
// weakened version:
// - "support reply via sender()" (case 3)
// - "restart when Kill:ed" (case 5)
// - "support futures" (case 6)
// Each of them checks that a message is handled before the next line runs,
// on the sender's thread; that needs the CallingThreadDispatcher.
//
// Differences from the original, recorded in the design:
// - The original shares `counter`, `thread`, and `otherthread` between cases
//   through the companion object, relying on ScalaTest running the cases one
//   at a time. `cargo test` runs the tests of a file in parallel, so each
//   test function builds its own `Arc<AtomicI32>` counter and
//   `Arc<Mutex<Option<ThreadId>>>` record, and takes
//   `std::thread::current()` at its start (section 1, "shared state").
// - The original builds nested actors inside the actor's constructor. The
//   port builds them from `pre_start`, because a Rust actor's factory has no
//   context: case 1 uses `TestActorRef::in_context`, case 2 uses
//   `ActorContext::actor_of` (section 1, note 1).
// - The original reads `underlyingActor` directly. The port does not hand
//   out the actor; a case reads a snapshot the actor publishes through
//   `Actor::underlying_snapshot`, taken by a probe that goes through the
//   mailbox (section 3.2).
// - The original looks at the dispatcher's runtime class (`getClass`); the
//   port looks at the `DispatcherKind` the executor resolved when the
//   actor was created (section 1, note 6).
// - The original's `a should not be null` and `nested should not be null`
//   hold by construction in Rust, where a reference cannot be null
//   (section 1, note 1, and section 4.1). The other assertions are the
//   original's.
// - `should be` on `ActorRef` is the port's `ActorRef` equality, which
//   compares the id and the generation as `LocalActorRef` compares the path
//   (section 1, note 9).

use core::any::Any;
use core::time::Duration;

use std::sync::{Arc, Mutex};
use std::thread::ThreadId;

use fraktor_actor::kernel::actor::{Actor, ActorContext, Props, ReceiveTimeout, ReceiveTimeoutSetting, Terminated};
use fraktor_actor::kernel::actor_path::ActorPath;
use fraktor_actor::kernel::actor_ref::ActorRef;
use fraktor_actor::kernel::dispatch::{DispatcherConfig, DispatcherKind};
use fraktor_actor::kernel::message::Message;
use fraktor_testkit::default_timeout::DefaultTimeout;
use fraktor_testkit::event_filter::EventFilter;
use fraktor_testkit::fraktorspec::FraktorSpec;
use fraktor_testkit::test_actor_ref::TestActorRef;
use fraktor_testkit::test_kit::TestProbe;

const SPEC_SYSTEM_NAME: &str = "TestActorRefSpec";

/// The name of the dispatcher case 11 registers, the original's `disp1`.
const DISP1_NAME: &str = "disp1";

/// Original: the shared `otherthread` of the companion object. The original's
/// `thread` is the `ThreadId` taken at the start of each test function,
/// which the actor is given when it is built.
type OtherThread = Arc<Mutex<Option<ThreadId>>>;

fn new_other_thread() -> OtherThread {
  Arc::new(Mutex::new(None))
}

/// Original: `trait TActor`, whose `receive` notes the thread that is not the
/// test's thread before it runs `receiveT`.
///
/// The original mixes the trait into the actors; the port puts the same step
/// in `TCell::record_thread`, which an actor that is a `TActor` calls at the
/// top of its `receive`.
#[derive(Clone)]
struct TCell {
  test_thread: ThreadId,
  other_thread: OtherThread,
}

impl TCell {
  fn new(test_thread: ThreadId) -> Self {
    Self { test_thread, other_thread: new_other_thread() }
  }

  fn child(&self) -> Self {
    Self { test_thread: self.test_thread, other_thread: self.other_thread.clone() }
  }

  fn record_thread(&self) {
    let current = std::thread::current().id();
    if current != self.test_thread {
      *self.other_thread.lock().unwrap() = Some(current);
    }
  }
}

/// Original: `assertThread()`, which checks that no other thread is recorded
/// or that the test's thread is.
fn assert_thread(other_thread: &OtherThread, test_thread: ThreadId) {
  let recorded = *other_thread.lock().unwrap();
  assert!(recorded.is_none() || recorded == Some(test_thread), "a message was handled on another thread");
}

/// Returns the payload of `message` when it is a `&'static str`.
fn string_of(message: &Message) -> Option<&'static str> {
  message.downcast_ref::<&'static str>().copied()
}

/// Original: `ReplyActor`.
///
/// The original builds its worker with `TestActorRef(Props[WorkerActor]())`
/// and hands the worker the actor's `sender()`. The port's worker factory goes
/// into an `Arc`, so it must be `'static`; the cell it needs is taken once
/// when the actor is built and moved into each factory.
struct ReplyActor {
  cell: TCell,
  /// The cell a worker built by this actor is given, taken once at build time.
  worker_cell: TCell,
  reply_to: Option<ActorRef>,
}

impl Actor for ReplyActor {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    self.cell.record_thread();
    match string_of(&message) {
      | Some("complexRequest") => {
        self.reply_to = Some(context.sender());
        let worker_cell = self.worker_cell.clone();
        let worker =
          TestActorRef::from_props(context.system(), Props::new(move || WorkerActor::new(worker_cell.clone())));
        let worker_ref = worker.actor_ref();
        worker.tell("work", context.self_ref());
        let _ = worker_ref;
      },
      | Some("complexRequest2") => {
        let worker_cell = self.worker_cell.clone();
        let worker =
          TestActorRef::from_props(context.system(), Props::new(move || WorkerActor::new(worker_cell.clone())));
        let worker_ref = worker.actor_ref();
        let sender = context.sender();
        worker.tell(sender, context.self_ref());
        let _ = worker_ref;
      },
      | Some("workDone") => {
        if let Some(reply_to) = self.reply_to.clone() {
          context.tell(&reply_to, "complexReply");
        }
      },
      | Some("simpleRequest") => {
        let sender = context.sender();
        context.tell(&sender, "simpleReply");
      },
      | _ => {},
    }
  }
}

/// What `WorkerActor` records for the companion object's cases: the handle it
/// saw as itself and its parent (section 1, note 9). The snapshot the cases
/// read is a copy of it.
#[derive(Clone)]
struct WorkerSnapshot {
  self_ref: ActorRef,
}

/// Original: `WorkerActor`, which answers work, stops itself, replies to a
/// `Promise` or an `ActorRef` with `"complexReply"`, and records
/// `context.parent` and `context.self.path.name`.
struct WorkerActor {
  cell: TCell,
  seen: Arc<Mutex<Option<WorkerSnapshot>>>,
}

impl WorkerActor {
  fn new(cell: TCell) -> Self {
    Self { cell, seen: Arc::new(Mutex::new(None)) }
  }
}

impl Actor for WorkerActor {
  fn pre_start(&mut self, context: &mut ActorContext<'_>) {
    *self.seen.lock().unwrap() = Some(WorkerSnapshot { self_ref: context.self_ref() });
  }

  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    self.cell.record_thread();
    match string_of(&message) {
      | Some("work") => {
        let sender = context.sender();
        context.tell(&sender, "workDone");
        let me = context.self_ref();
        context.stop(&me);
      },
      | _ => {
        if let Some(reply_to) = message.downcast_ref::<ActorRef>().cloned() {
          context.tell(&reply_to, "complexReply");
        }
      },
    }
  }

  fn underlying_snapshot(&self) -> Option<Box<dyn Any + Send + Sync>> {
    let seen = self.seen.lock().unwrap().clone();
    seen.map(|snapshot| Box::new(snapshot) as Box<dyn Any + Send + Sync>)
  }
}

/// Original: `ReceiveTimeoutActor`, which sets the receive timeout from its
/// constructor (the port sets it in `pre_start`) and forwards the timeout to
/// `target` before it stops.
struct ReceiveTimeoutActor {
  target: ActorRef,
}

impl Actor for ReceiveTimeoutActor {
  fn pre_start(&mut self, context: &mut ActorContext<'_>) {
    context.set_receive_timeout(ReceiveTimeoutSetting::Finite(Duration::from_secs(1)));
  }

  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    if message.is::<ReceiveTimeout>() {
      context.tell(&self.target.clone(), "timeout");
      let me = context.self_ref();
      context.stop(&me);
    }
  }
}

/// Original: `WrappedTerminated`. Forwarding a `Terminated` to a test actor
/// that does not watch the target is not possible, so it is wrapped.
#[derive(Clone)]
struct WrappedTerminated(Terminated);

/// Original: the anonymous actor of "stop when sent a poison pill", which
/// watches `a` and forwards `Terminated` and every other message to the test
/// actor.
#[derive(Clone)]
struct WatchAndForward {
  target: ActorRef,
  test_actor: ActorRef,
  cell: TCell,
}

impl Actor for WatchAndForward {
  fn pre_start(&mut self, context: &mut ActorContext<'_>) {
    context.watch(&self.target.clone());
  }

  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    self.cell.record_thread();
    if let Some(terminated) = message.downcast_ref::<Terminated>().cloned() {
      context.tell(&self.test_actor.clone(), WrappedTerminated(terminated));
    } else {
      context.tell(&self.test_actor.clone(), message);
    }
  }
}

/// Original: the anonymous actor of case 1, whose constructor builds the
/// nested `TestActorRef`. The port builds it in `pre_start`, where the
/// context exists, and answers the sender with it.
struct NestedInContext {
  /// The nested `TestActorRef` itself, since `in_context` hands one back.
  nested: Option<TestActorRef>,
}

impl Actor for NestedInContext {
  fn pre_start(&mut self, context: &mut ActorContext<'_>) {
    let nested = TestActorRef::in_context(context, Props::new(|| Idle), None, None).expect("the nested actor starts");
    self.nested = Some(nested);
  }

  fn receive(&mut self, context: &mut ActorContext<'_>, _message: Message) {
    let sender = context.sender();
    match self.nested.as_ref() {
      | Some(nested) => context.tell(&sender, nested.actor_ref()),
      | None => panic!("the nested actor was not built"),
    }
  }
}

/// Original: the anonymous actor of case 2, whose constructor calls
/// `context.actorOf`. The port calls `ActorContext::actor_of` in `pre_start`.
struct NestedInActorOf {
  nested: Option<ActorRef>,
}

impl Actor for NestedInActorOf {
  fn pre_start(&mut self, context: &mut ActorContext<'_>) {
    let nested = context.actor_of(Props::new(|| Idle), "nested").expect("the nested actor starts");
    self.nested = Some(nested);
  }

  fn receive(&mut self, context: &mut ActorContext<'_>, _message: Message) {
    let sender = context.sender();
    match self.nested.as_ref() {
      | Some(nested) => context.tell(&sender, nested.clone()),
      | None => panic!("the nested actor was not built"),
    }
  }
}

/// Original: `class TA extends TActor { var s: String; case x: String => s = x }`,
/// which the case reads as `underlyingActor.s`.
struct RecordsString {
  cell: TCell,
  s: String,
}

impl Actor for RecordsString {
  fn receive(&mut self, _context: &mut ActorContext<'_>, message: Message) {
    self.cell.record_thread();
    if let Some(text) = string_of(&message) {
      self.s = text.to_string();
    }
  }

  fn underlying_snapshot(&self) -> Option<Box<dyn Any + Send + Sync>> {
    Some(Box::new(self.s.clone()))
  }
}

/// Original: the anonymous actor of cases 14 and 15, which fails on `1` and
/// echoes every other message to its sender.
struct FailsOnOne;

impl Actor for FailsOnOne {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    match message.downcast_ref::<i32>() {
      | Some(&1) => panic!("expected"),
      | _ => {
        let sender = context.sender();
        context.tell(&sender, message);
      },
    }
  }
}

/// Original: `Props(new Actor { def receive = { case _ => } })` for the nested
/// actors.
struct Idle;

impl Actor for Idle {
  fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {}
}

fn guardian(spec: &FraktorSpec) -> ActorRef {
  let path =
    ActorPath::from_string(&format!("pekko://{SPEC_SYSTEM_NAME}")).expect("the root path parses").with_child("user");
  spec.system().resolve_actor_ref(&path)
}

fn default_timeout(spec: &FraktorSpec) -> Duration {
  DefaultTimeout::timeout(spec.system()).duration()
}

// Original: "A TestActorRef should be an ActorRef, hence it must support
// nested Actor creation when used with TestActorRef"
#[test]
fn supports_nested_actor_creation_with_test_actor_ref() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let a = TestActorRef::from_props(spec.system_mut(), Props::new(|| NestedInContext { nested: None }));
  let a_ref = a.actor_ref();
  let nested = a_ref
    .ask("any")
    .await_result(default_timeout(&spec))
    .downcast_ref::<ActorRef>()
    .cloned()
    .expect("the reply is an ActorRef");
  assert_ne!(a_ref, nested, "the nested actor is a different reference");
  spec.terminate();
}

// Original: "A TestActorRef should be an ActorRef, hence it must support
// nested Actor creation when used with ActorRef"
#[test]
fn supports_nested_actor_creation_with_actor_ref() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let a = TestActorRef::from_props(spec.system_mut(), Props::new(|| NestedInActorOf { nested: None }));
  let a_ref = a.actor_ref();
  let nested = a_ref
    .ask("any")
    .await_result(default_timeout(&spec))
    .downcast_ref::<ActorRef>()
    .cloned()
    .expect("the reply is an ActorRef");
  assert_ne!(a_ref, nested, "the nested actor is a different reference");
  spec.terminate();
}

// Original: "A TestActorRef should be an ActorRef, hence it must stop when
// sent a poison pill"
#[test]
fn stops_when_sent_a_poison_pill() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let test_thread = std::thread::current().id();
  let cell = TCell::new(test_thread);
  let filter = EventFilter::typed("ActorKilledException");
  let test_actor = spec.test_actor();
  let worker_cell = cell.child();
  let a = TestActorRef::from_props(spec.system_mut(), Props::new(move || WorkerActor::new(worker_cell.clone())));
  let target = a.actor_ref();
  let watcher = WatchAndForward { target: target.clone(), test_actor: test_actor.clone(), cell: cell.child() };
  spec.system_mut().actor_of(Props::new(move || watcher.clone()));
  filter.intercept_any(spec.system_mut(), || {
    a.tell("PoisonPill", test_actor);
  });
  spec.expect_msg_type::<WrappedTerminated>();
  assert!(a.is_terminated(), "the actor must be terminated after the poison pill");
  assert_thread(&cell.other_thread, test_thread);
  spec.terminate();
}

// Original: "A TestActorRef should be an ActorRef, hence it must support
// receive timeout"
#[test]
fn supports_receive_timeout() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let target = spec.test_actor();
  TestActorRef::from_props(spec.system_mut(), Props::new(move || ReceiveTimeoutActor { target: target.clone() }));
  spec.expect_msg("timeout");
  spec.terminate();
}

// Original: "A TestActorRef must allow access to internals"
#[test]
fn allows_access_to_internals() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let test_thread = std::thread::current().id();
  let cell = TCell::new(test_thread);
  let records_cell = cell.child();
  let ref_ = TestActorRef::from_props(
    spec.system_mut(),
    Props::new(move || RecordsString { cell: records_cell.clone(), s: String::new() }),
  );
  ref_.tell("hallo", spec.test_actor());
  let s: String = ref_.underlying_snapshot();
  assert_eq!(s, "hallo");
  spec.terminate();
}

// Original: "A TestActorRef must set receiveTimeout to None"
#[test]
fn sets_receive_timeout_to_none() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let test_thread = std::thread::current().id();
  let cell = TCell::new(test_thread);
  let worker_cell = cell.child();
  let a = TestActorRef::from_props(spec.system_mut(), Props::new(move || WorkerActor::new(worker_cell.clone())));
  assert_eq!(a.receive_timeout(), ReceiveTimeoutSetting::Undefined);
  spec.terminate();
}

// Original: "A TestActorRef must set CallingThreadDispatcher"
#[test]
fn sets_calling_thread_dispatcher() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let test_thread = std::thread::current().id();
  let cell = TCell::new(test_thread);
  let worker_cell = cell.child();
  let a = TestActorRef::from_props(spec.system_mut(), Props::new(move || WorkerActor::new(worker_cell.clone())));
  assert_eq!(a.dispatcher_kind(), Some(DispatcherKind::CallingThread));
  spec.terminate();
}

// Original: "A TestActorRef must allow override of dispatcher"
#[test]
fn allows_override_of_dispatcher() {
  let test_thread = std::thread::current().id();
  let cell = TCell::new(test_thread);
  let mut spec = FraktorSpec::with_dispatchers(
    SPEC_SYSTEM_NAME,
    vec![DispatcherConfig { id: DISP1_NAME.to_string(), kind: DispatcherKind::Default }],
  );
  let worker_cell = cell.child();
  let props = Props::new(move || WorkerActor::new(worker_cell.clone())).with_dispatcher(DISP1_NAME);
  let a = TestActorRef::from_props(spec.system_mut(), props);
  assert_eq!(a.dispatcher_kind(), Some(DispatcherKind::Default));
  spec.terminate();
}

// Original: "A TestActorRef must proxy receive for the underlying actor
// without sender()"
#[test]
fn proxies_receive_without_sender() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let test_thread = std::thread::current().id();
  let cell = TCell::new(test_thread);
  let worker_cell = cell.child();
  let ref_ = TestActorRef::from_props(spec.system_mut(), Props::new(move || WorkerActor::new(worker_cell.clone())));
  ref_.receive("work");
  assert!(ref_.is_terminated(), "the actor must be terminated after receiving its stop");
  spec.terminate();
}

// Original: "A TestActorRef must proxy receive for the underlying actor with
// sender()"
#[test]
fn proxies_receive_with_sender() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let test_thread = std::thread::current().id();
  let cell = TCell::new(test_thread);
  let test_actor = spec.test_actor();
  let worker_cell = cell.child();
  let ref_ = TestActorRef::from_props(spec.system_mut(), Props::new(move || WorkerActor::new(worker_cell.clone())));
  ref_.receive_with_sender("work", test_actor);
  assert!(ref_.is_terminated(), "the actor must be terminated after receiving its stop");
  spec.expect_msg("workDone");
  spec.terminate();
}

// Original: "A TestActorRef must not throw an exception when parent is passed
// in the apply"
#[test]
fn does_not_throw_when_parent_is_passed_in_the_apply() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let parent = TestProbe::new(spec.system_mut());
  let parent_ref = parent.ref_handle();
  let filter = EventFilter::typed("RuntimeException").with_message("expected");
  // `intercept` lends the system, so the child is built first and the body
  // sends it the failing message. The child's start and the failure it raises
  // both go through the mailbox, which `intercept` drives to idle after the
  // body, so the filter is armed when the failure is logged.
  let child = TestActorRef::with_supervisor_and_name(spec.system_mut(), || FailsOnOne, &parent_ref, "Child")
    .expect("the name is free");
  filter.intercept(spec.system_mut(), move || {
    child.tell(1, parent_ref.clone());
  });
  spec.terminate();
}

// Original: "A TestActorRef must not throw an exception when child is created
// through childActorOf"
#[test]
fn does_not_throw_when_child_is_created_through_child_actor_of() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let mut parent = TestProbe::new(spec.system_mut());
  let filter = EventFilter::typed("RuntimeException").with_message("expected");
  let child = parent.child_actor_of_named(Props::new(|| FailsOnOne), "Child");
  let parent_ref = parent.ref_handle();
  filter.intercept(spec.system_mut(), move || {
    child.tell(1, parent_ref.clone());
  });
  spec.terminate();
}

// Original: "A TestActorRef Companion Object must allow creation of a
// TestActorRef with a default supervisor"
#[test]
fn allows_creation_with_a_default_supervisor() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let test_thread = std::thread::current().id();
  let cell = TCell::new(test_thread);
  let guardian = guardian(&spec);
  let worker_cell = cell.child();
  let ref_ = TestActorRef::new(spec.system_mut(), move || WorkerActor::new(worker_cell.clone()));
  let snapshot: WorkerSnapshot = ref_.underlying_snapshot();
  let supervisor = spec.system().resolve_actor_ref(&snapshot.self_ref.path().parent());
  assert_eq!(supervisor, guardian, "a default supervisor is the guardian");
  spec.terminate();
}

// Original: "A TestActorRef Companion Object must allow creation of a
// TestActorRef with a default supervisor and specified name"
#[test]
fn allows_creation_with_a_default_supervisor_and_a_name() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let test_thread = std::thread::current().id();
  let cell = TCell::new(test_thread);
  let worker_cell = cell.child();
  let ref_ = TestActorRef::with_name(spec.system_mut(), move || WorkerActor::new(worker_cell.clone()), "specificActor")
    .expect("the name is free");
  let snapshot: WorkerSnapshot = ref_.underlying_snapshot();
  assert_eq!(snapshot.self_ref.path().name(), "specificActor");
  spec.terminate();
}

// Original: "A TestActorRef Companion Object must allow creation of a
// TestActorRef with a specified supervisor"
#[test]
fn allows_creation_with_a_specified_supervisor() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let test_thread = std::thread::current().id();
  let cell = TCell::new(test_thread);
  let reply_cell = cell.child();
  let parent = TestActorRef::new(spec.system_mut(), move || ReplyActor {
    cell: reply_cell.clone(),
    worker_cell: reply_cell.clone(),
    reply_to: None,
  });
  let parent_ref = parent.actor_ref();
  let worker_cell = cell.child();
  let ref_ =
    TestActorRef::with_supervisor(spec.system_mut(), move || WorkerActor::new(worker_cell.clone()), &parent_ref);
  let snapshot: WorkerSnapshot = ref_.underlying_snapshot();
  let supervisor = spec.system().resolve_actor_ref(&snapshot.self_ref.path().parent());
  assert_eq!(supervisor, parent_ref);
  spec.terminate();
}

// Original: "A TestActorRef Companion Object must allow creation of a
// TestActorRef with a specified supervisor and specified name"
#[test]
fn allows_creation_with_a_specified_supervisor_and_a_name() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let test_thread = std::thread::current().id();
  let cell = TCell::new(test_thread);
  let reply_cell = cell.child();
  let parent = TestActorRef::new(spec.system_mut(), move || ReplyActor {
    cell: reply_cell.clone(),
    worker_cell: reply_cell.clone(),
    reply_to: None,
  });
  let parent_ref = parent.actor_ref();
  let worker_cell = cell.child();
  let ref_ = TestActorRef::with_supervisor_and_name(
    spec.system_mut(),
    move || WorkerActor::new(worker_cell.clone()),
    &parent_ref,
    "specificSupervisedActor",
  )
  .expect("the name is free");
  let snapshot: WorkerSnapshot = ref_.underlying_snapshot();
  assert_eq!(snapshot.self_ref.path().name(), "specificSupervisedActor");
  let supervisor = spec.system().resolve_actor_ref(&snapshot.self_ref.path().parent());
  assert_eq!(supervisor, parent_ref);
  spec.terminate();
}

// Original: "A TestActorRef Companion Object must allow creation of a
// TestActorRef with a default supervisor with Props"
#[test]
fn allows_creation_with_a_default_supervisor_from_props() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let test_thread = std::thread::current().id();
  let cell = TCell::new(test_thread);
  let guardian = guardian(&spec);
  let worker_cell = cell.child();
  let props = Props::new(move || WorkerActor::new(worker_cell.clone()));
  let ref_ = TestActorRef::from_props(spec.system_mut(), props);
  let snapshot: WorkerSnapshot = ref_.underlying_snapshot();
  let supervisor = spec.system().resolve_actor_ref(&snapshot.self_ref.path().parent());
  assert_eq!(supervisor, guardian, "a default supervisor is the guardian");
  spec.terminate();
}

// Original: "A TestActorRef Companion Object must allow creation of a
// TestActorRef with a default supervisor and specified name with Props"
#[test]
fn allows_creation_with_a_default_supervisor_and_a_name_from_props() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let test_thread = std::thread::current().id();
  let cell = TCell::new(test_thread);
  let worker_cell = cell.child();
  let props = Props::new(move || WorkerActor::new(worker_cell.clone()));
  let ref_ =
    TestActorRef::from_props_with_name(spec.system_mut(), props, "specificPropsActor").expect("the name is free");
  let snapshot: WorkerSnapshot = ref_.underlying_snapshot();
  assert_eq!(snapshot.self_ref.path().name(), "specificPropsActor");
  spec.terminate();
}

// Original: "A TestActorRef Companion Object must allow creation of a
// TestActorRef with a specified supervisor with Props"
#[test]
fn allows_creation_with_a_specified_supervisor_from_props() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let test_thread = std::thread::current().id();
  let cell = TCell::new(test_thread);
  let reply_cell = cell.child();
  let parent = TestActorRef::new(spec.system_mut(), move || ReplyActor {
    cell: reply_cell.clone(),
    worker_cell: reply_cell.clone(),
    reply_to: None,
  });
  let parent_ref = parent.actor_ref();
  let worker_cell = cell.child();
  let props = Props::new(move || WorkerActor::new(worker_cell.clone()));
  let ref_ = TestActorRef::from_props_with_supervisor(spec.system_mut(), props, &parent_ref);
  let snapshot: WorkerSnapshot = ref_.underlying_snapshot();
  let supervisor = spec.system().resolve_actor_ref(&snapshot.self_ref.path().parent());
  assert_eq!(supervisor, parent_ref);
  spec.terminate();
}

// Original: "A TestActorRef Companion Object must allow creation of a
// TestActorRef with a specified supervisor and specified name with Props"
#[test]
fn allows_creation_with_a_specified_supervisor_and_a_name_from_props() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let test_thread = std::thread::current().id();
  let cell = TCell::new(test_thread);
  let reply_cell = cell.child();
  let parent = TestActorRef::new(spec.system_mut(), move || ReplyActor {
    cell: reply_cell.clone(),
    worker_cell: reply_cell.clone(),
    reply_to: None,
  });
  let parent_ref = parent.actor_ref();
  let worker_cell = cell.child();
  let props = Props::new(move || WorkerActor::new(worker_cell.clone()));
  let ref_ = TestActorRef::from_props_with_supervisor_and_name(
    spec.system_mut(),
    props,
    &parent_ref,
    "specificSupervisedPropsActor",
  )
  .expect("the name is free");
  let snapshot: WorkerSnapshot = ref_.underlying_snapshot();
  assert_eq!(snapshot.self_ref.path().name(), "specificSupervisedPropsActor");
  let supervisor = spec.system().resolve_actor_ref(&snapshot.self_ref.path().parent());
  assert_eq!(supervisor, parent_ref);
  spec.terminate();
}
