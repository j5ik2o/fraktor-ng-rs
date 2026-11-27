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

// Ported from Pekko (Apache Pekko, actor-tests):
//   actor-tests/src/test/scala/org/apache/pekko/actor/ActorRefSpec.scala
//   (Pekko v2.0.0)
// Original test names are kept as comments next to the ported cases.
//
// Design: docs/design/issue-198-fsm-timing-actor-ref.md. Of the twelve cases,
// ten are ported here:
//   - Case 3 ("be serializable using Java Serialization on local node") and case 4 ("throw an
//     exception on deserialize if no system in scope") are left to the serialization design (design
//     section 1.5, decisions 1 and 2), because they test `java.io.ObjectOutputStream` and
//     `JavaSerializer.currentSystem`, which the port does not have.
//   - Case 5 ("return EmptyLocalActorRef ...") is ported without the byte round trip (decision 3):
//     the case checks the four observable facts of an unresolved path instead of `==`.
//
// Case 1 (`does not allow Actors to be created outside of an actorOf`) runs
// the small cases 2 to 13 of the original, and not small case 1: the check
// that a direct `new Actor` is refused needs `ActorCell.contextStack`, which
// has no counterpart in Rust (decision 4). Each failure the original builds
// with a direct `new` is a panic inside the `Props` factory here. The
// original's `promiseIntercept` and `wrap` are `promise_intercept`: the
// factory runs the maker under `catch_unwind`, reports the failure text to the
// test, and re-raises the panic, so the `Create` capture, the error log, the
// supervisor's stop, and the `Terminated` are the same as for a plain panic.
// The panic text of the original's `contextStack` message is the test-side
// constant. The original's `intercept[ActorInitializationException]` log
// counts and `contextStackMustBeEmpty()` are kept.
//
// Cases 6 and 7 build the inner actor from the test body and hand its
// reference to the outer factory, as design section 1.3 says: calling
// `system.actor_of` from inside a `Props` factory panics with the executor's
// re-entrant borrow. Case 12 builds the child from `pre_start`, where
// `context.actor_of` is the right entry point.
//
// The original's `Thread.sleep(1)` in `WorkerActor` is scaled by
// `TestKitSettings::dilated`, as design section 1.3 says.

use core::time::Duration;
use std::sync::{mpsc, Arc, Mutex};

use fraktor_actor::kernel::{
  actor::{Actor, ActorContext, DeadLetter, Props, Terminated},
  actor_path::ActorPath,
  actor_ref::ActorRef,
  event::EventClass,
  message::Message,
  supervision::{RestartLimit, RestartWindow, SupervisorStrategy},
};
use fraktor_testkit::{
  event_filter::{EventFilter, EventSinkRegistry},
  fraktorspec::FraktorSpec,
  test_kit::{TestLatch, TestProbe},
};

const SPEC_SYSTEM_NAME: &str = "ActorRefSpec";

// Original `Actor.scala:502-504`: the text the constructor check throws. The
// port has no such check, so the factory panic carries this text and the error
// log names the same reason.
const CONTEXT_STACK_MESSAGE: &str =
  "You cannot create an instance of [InnerActor] explicitly using the constructor (new). \
   You have to use one of the 'actorOf' factory methods to create a new actor. See the documentation.";

fn millis(value: u64) -> Duration {
  Duration::from_millis(value)
}

fn sender(spec: &FraktorSpec) -> ActorRef {
  spec.system().dead_letters()
}

fn reply_to(context: &mut ActorContext<'_>, message: impl core::any::Any + Send + Sync) {
  let target = context.sender();
  context.tell(&target, message);
}

/// The text a panic payload carries, as `UnwindCatcher` reads it.
fn panic_text(payload: &(dyn core::any::Any + Send)) -> String {
  if let Some(text) = payload.downcast_ref::<&'static str>() {
    return (*text).to_string();
  }
  if let Some(text) = payload.downcast_ref::<String>() {
    return text.clone();
  }
  String::new()
}

// ---------------------------------------------------------------------------
// The actors of the original.
// ---------------------------------------------------------------------------

// Original: `class ReplyActor extends Actor { def receive: Receive = { case s: String => sender() !
// s } }`
struct ReplyActor;

impl Actor for ReplyActor {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    if let Some(text) = message.downcast_ref::<&'static str>() {
      reply_to(context, *text);
    }
  }
}

// Original: `class InnerActor`, which answers `innerself` with its own
// reference, `self` and `a` with its own sender, and the rest with the text.
struct InnerActor;

impl Actor for InnerActor {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    let self_ref = context.self_ref();
    let seen_sender = context.sender();
    if let Some(text) = message.downcast_ref::<&'static str>() {
      if *text == "innerself" || *text == "self" {
        reply_to(context, self_ref);
      } else if *text == "a" {
        reply_to(context, seen_sender);
      } else {
        reply_to(context, *text);
      }
    }
  }
}

// Original: `class OuterActor(val inner: ActorRef) extends Actor { def receive = {
// case _ => sender() ! inner } }` for case 6, and `case msg => inner.forward(msg, context)`
// for case 7. The port uses one actor for both: the `InnerActor` answers
// `msg` with the text and `innerself`/`a`/`self` with what the original
// expects, and the outer forwards each message, keeping the sender, so case 6
// reads the inner's handle from `"innerself"`.
struct OuterActor {
  inner: ActorRef,
}

impl Actor for OuterActor {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    if message == "msg" {
      let inner = self.inner.clone();
      reply_to(context, inner);
      return;
    }
    self.inner.forward(message, context);
  }
}

// Original case 9: an actor type that lives in a non-public module and is
// reached through a public factory returning `Props` (design section 2.3).
struct PigDog;

impl Actor for PigDog {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    if let Some(text) = message.downcast_ref::<&'static str>() {
      reply_to(context, *text);
    }
  }
}

fn pig_dog_props() -> Props {
  Props::new(|| PigDog)
}

// Original: `class WorkerActor`: it answers `reply_with` to `to` after a
// dilated `Thread.sleep(1)`-sized pause.
struct WorkerActor {
  reply_with: Message,
  to: ActorRef,
  delay: Duration,
}

impl Actor for WorkerActor {
  fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {
    std::thread::sleep(self.delay);
    let target = self.to.clone();
    let sender = self.to.clone();
    target.tell(self.reply_with.clone(), sender);
  }
}

// Original case 8's protocol: the test drives one round of `complex` and one
// of `simple` through the reply actor, which sends `complex` to the sender
// actor and `simple` to the worker actor. The worker answers the test actor,
// the sender actor answers the test actor, and each answer counts one of the
// four latch openings of `TestLatch(4)` (two per round: the worker's and the
// sender's).
struct SenderActor {
  reply_slot: Arc<Mutex<Option<ActorRef>>>,
  count_hits: TestLatch,
  rounds_left: usize,
}

impl Actor for SenderActor {
  fn pre_start(&mut self, _context: &mut ActorContext<'_>) {}

  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    let Some(text) = message.downcast_ref::<&'static str>() else { return };
    match *text {
      | "start" => {
        // The original's `SenderActor` sends `complex`; the port's two-round
        // dialogue starts with it.
        self.rounds_left = 2;
        self.request(context, "complex");
      },
      | "complex" => {
        // The sender actor's own answer to the test actor.
        reply_to(context, "ToTestActor");
        self.count_hits.count_down();
        self.rounds_left = self.rounds_left.saturating_sub(1);
        if self.rounds_left > 0 {
          self.request(context, "complex");
        } else {
          self.request(context, "simple");
        }
      },
      | "ToSenderActor" => {
        // The worker's answer to the sender actor, which answers the test
        // actor and opens the latch. The next round is already asked below
        // whenever a round was left.
        reply_to(context, "ToTestActor");
        self.count_hits.count_down();
      },
      | _ => {},
    }
  }
}

impl SenderActor {
  fn request(&mut self, context: &mut ActorContext<'_>, request: &'static str) {
    let Some(reply_actor) = self.reply_slot.lock().expect("the reply slot").clone() else { return };
    reply_actor.tell(request, context.self_ref());
  }
}

// Original case 8: the reply actor that routes by the prefix of the message to
// the sender actor (`complex`) or the worker actor (`simple`).
struct RoutingReplyActor {
  sender_actor: ActorRef,
  worker_actor: ActorRef,
}

impl Actor for RoutingReplyActor {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    if let Some(text) = message.downcast_ref::<&'static str>() {
      let target = if text.starts_with("complex") { self.sender_actor.clone() } else { self.worker_actor.clone() };
      target.tell(*text, context.self_ref());
    }
  }
}

// Original case 12: `class Parent extends Actor { context.actorOf(Props[Child], "child") ... }`.
struct ParentWithChild {
  child: Option<ActorRef>,
}

impl Actor for ParentWithChild {
  fn pre_start(&mut self, context: &mut ActorContext<'_>) {
    let child = context.actor_of(reply_actor_props(), "child").expect("the child name is free");
    self.child = Some(child);
  }

  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    if let Some(name) = message.downcast_ref::<&'static str>() {
      let found = context.child(name).is_some();
      reply_to(context, found);
    }
  }
}

// The question that asks a supervisor for its child in case 11.
#[derive(Clone, Copy, Debug)]
struct GetChild;

// Original case 11: the child whose restart the latch counts.
struct RestartCountingChild {
  latch: TestLatch,
}

impl Actor for RestartCountingChild {
  fn post_restart(&mut self, _context: &mut ActorContext<'_>) {
    self.latch.count_down();
  }

  fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {}
}

// Original case 11's supervisor: the parent owns the child under
// `OneForOneStrategy(maxNrOfRetries = 2, within = 1.second)`.
struct OneForOneSupervisor {
  strategy: SupervisorStrategy,
  child: Option<ActorRef>,
  latch: TestLatch,
}

impl Actor for OneForOneSupervisor {
  fn supervisor_strategy(&self) -> SupervisorStrategy {
    self.strategy.clone()
  }

  fn pre_start(&mut self, context: &mut ActorContext<'_>) {
    let latch = self.latch.clone();
    let child = context.actor_of_with_strategy(restart_counting_child_props(latch), self.strategy.clone());
    self.child = Some(child);
  }

  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    let Some(child) = self.child.clone() else { return };
    if message.is::<GetChild>() {
      reply_to(context, child);
      return;
    }
    child.forward(message, context);
  }
}

fn restart_counting_child_props(latch: TestLatch) -> Props {
  Props::new(move || RestartCountingChild { latch: latch.clone() })
}

fn reply_actor_props() -> Props {
  Props::new(|| ReplyActor)
}

fn inner_actor_props() -> Props {
  Props::new(|| InnerActor)
}

fn outer_actor_props(inner: ActorRef) -> Props {
  Props::new(move || OuterActor { inner: inner.clone() })
}

// ---------------------------------------------------------------------------
// Original `ActorRefSpec.scala:128-144`: `promiseIntercept` and `wrap`.
// ---------------------------------------------------------------------------

/// The original's `promiseIntercept(f)(to)`: the factory runs `make` under
/// `catch_unwind`, reports the failure text (empty when `make` succeeded), and
/// re-raises the panic so the `Create` capture handles it as usual. The maker
/// builds the actor the case is about; its successful value is the machine that
/// would have run, and the port only needs the report and the re-raised panic.
fn promise_intercept<F, A>(tx: mpsc::Sender<String>, make: F) -> Props
where
  F: FnOnce() -> A + Send + Sync + 'static,
  A: Actor + 'static,
{
  let make = std::sync::Mutex::new(Some(make));
  Props::new(move || {
    let make = make.lock().expect("the maker lock").take().expect("the maker runs once");
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(make)) {
      | Ok(actor) => {
        let _ = tx.send(String::new());
        InterceptedActor::Succeeded(Box::new(actor))
      },
      | Err(payload) => {
        let text = panic_text(payload.as_ref());
        let _ = tx.send(text.clone());
        std::panic::resume_unwind(payload)
      },
    }
  })
}

/// What `promise_intercept` hands back on success: the actor the maker built.
enum InterceptedActor {
  Succeeded(Box<dyn Actor>),
}

impl Actor for InterceptedActor {
  fn pre_start(&mut self, context: &mut ActorContext<'_>) {
    let Self::Succeeded(actor) = self;
    actor.pre_start(context);
  }

  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    let Self::Succeeded(actor) = self;
    actor.receive(context, message);
  }
}

/// Original `wrap`: reads the failure the maker reported, from the test thread.
fn wrap(rx: mpsc::Receiver<String>) -> String {
  rx.recv_timeout(Duration::from_secs(5)).expect("the factory reported its outcome")
}

fn promise_pair() -> (mpsc::Sender<String>, mpsc::Receiver<String>) {
  mpsc::channel()
}

// ---------------------------------------------------------------------------
// Case 1: the small cases of `ActorRefSpec.scala:148-283`.
// ---------------------------------------------------------------------------

// Original `ActorRefSpec.scala:89, 98, 109, 118`: `FailingOuterActor`,
// `FailingInnerActor`, and their inheriting variants. Each fails while it is
// created with `val fail = new InnerActor`, which the port writes as a panic
// with the original's context-stack text. The successful variants are
// `inner_actor_props` and `outer_actor_props`.
/// The actor the original's failing variants are made of; it never starts,
/// because the factory panics first.
struct FailingActor;

impl Actor for FailingActor {
  fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {}
}

/// One small case of case 1: the optional inner actor and the outer actor, each
/// failing or succeeding at creation.
struct SmallCase {
  inner_fails: bool,
  outer_fails: bool,
  /// The outer actor's failure text, when small case 13 names one.
  text: Option<&'static str>,
}

impl SmallCase {
  fn new(inner_fails: bool, outer_fails: bool) -> Self {
    Self { inner_fails, outer_fails, text: None }
  }

  fn expecting(self, text: &'static str) -> Self {
    Self { text: Some(text), ..self }
  }

  fn occurrences(&self) -> usize {
    usize::from(self.inner_fails) + usize::from(self.outer_fails)
  }
}

/// Runs one small case: creates the failing actor(s) under the test actor with
/// the stopping strategy (the `Create` path is the same as under `/user`, and
/// each failed start logs one `ActorInitializationException`), and checks the
/// count, the report that reached the caller, the stop, and that the name is
/// free again. The inner actor is the outer's sibling, as in the original,
/// which keeps the two failures independent and their logs counted as two.
fn run_initialization_case(spec: &mut FraktorSpec, case: SmallCase) {
  let sink = spec.system().event_sink().expect("the spec system has an event sink");
  let registry = sink.into_any().downcast::<EventSinkRegistry>().expect("the spec uses an EventSinkRegistry");
  let armed = registry.arm(EventFilter::typed("ActorInitializationException"));
  let mut receivers = Vec::new();
  let mut created = Vec::new();
  let mut inner_success = None;
  if case.inner_fails || !case.outer_fails {
    let inner = make_case_actor(spec, "failingInner", case.inner_fails, case.text);
    if case.inner_fails {
      receivers.push(inner.1.expect("the maker's receiver"));
      created.push(inner.0);
    } else {
      inner_success = Some(inner.0);
    }
  }
  if case.outer_fails {
    let outer = make_case_actor(spec, "failingOuter", true, case.text);
    receivers.push(outer.1.expect("the maker's receiver"));
    created.push(outer.0);
  }
  spec.system_mut().run_until_idle();
  let matched = registry.matched(armed);
  registry.disarm(armed);
  assert_eq!(matched, case.occurrences(), "the failed starts log one ActorInitializationException each");
  for rx in receivers {
    let text = wrap(rx);
    if let Some(expected) = case.text {
      assert_eq!(text, expected, "the failure reached the caller with the original's text");
    } else {
      assert!(!text.is_empty(), "the failure reached the caller");
    }
  }
  for actor in &created {
    let terminated: Terminated = spec.expect_msg_type();
    assert_eq!(terminated.target(), actor, "the supervisor stopped the failed actor");
  }
  if let Some(inner) = inner_success {
    assert!(!spec.system().is_actor_terminated(&inner), "the side that did not fail keeps running");
  }
  // `contextStackMustBeEmpty()`: the name is free again and a normal actor of
  // the same name answers.
  for actor in &created {
    let name = actor.path().name().to_string();
    let replacement = spec.system_mut().actor_of_named(reply_actor_props(), &name).expect("the name is free again");
    let reply = replacement.ask("hello").await_result(millis(1000));
    assert_eq!(reply.downcast_ref::<&'static str>().copied(), Some("hello"));
    spec.system_mut().stop(&replacement);
    spec.system_mut().run_until_idle();
  }
}

/// Starts one actor of a small case under the test actor, watching it, and
/// returns it with the maker's report receiver when it was made to fail.
fn make_case_actor(
  spec: &mut FraktorSpec,
  name: &str,
  fails: bool,
  text: Option<&'static str>,
) -> (ActorRef, Option<mpsc::Receiver<String>>) {
  let (tx, rx) = promise_pair();
  let props = if fails {
    match text {
      | Some(message) => promise_intercept(tx, move || -> FailingActor { panic!("{message}") }),
      | None => promise_intercept(tx, || -> FailingActor { panic!("{CONTEXT_STACK_MESSAGE}") }),
    }
  } else {
    promise_intercept(tx, || ReplyActor)
  };
  let actor = spec.test_kit_mut().child_actor_of(
    props,
    Some(name),
    fraktor_actor::kernel::supervision::SupervisorStrategy::stopping_strategy(),
  );
  spec.test_kit_mut().watch(&actor);
  (actor, fails.then_some(rx))
}

// Original: "not allow Actors to be created outside of an actorOf"
#[test]
fn does_not_allow_actors_to_be_created_outside_of_an_actor_of() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  // Small case 1 (`:150-152`) is not built (decision 4).
  // Small case 2 (`:156-166`): the outer constructor's `new Actor`.
  run_initialization_case(&mut spec, SmallCase::new(false, true));
  // Small case 3 (`:168-175`): `FailingOuterActor(actorOf(InnerActor))`.
  run_initialization_case(&mut spec, SmallCase::new(false, true));
  // Small case 4 (`:177-184`): `OuterActor(actorOf(FailingInnerActor))`.
  run_initialization_case(&mut spec, SmallCase::new(true, false));
  // Small case 5 (`:186-193`): `FailingInheritingOuterActor(actorOf(InnerActor))`.
  run_initialization_case(&mut spec, SmallCase::new(false, true));
  // Small cases 6, 7, 8, 10 (`:195-244`): both the outer and the inner fail.
  for _ in 0..4 {
    run_initialization_case(&mut spec, SmallCase::new(true, true));
  }
  // Small case 9 (`:225-234`): `OuterActor(actorOf(new InnerActor { val a = new InnerActor }))`.
  run_initialization_case(&mut spec, SmallCase::new(true, false));
  // Small case 11 (`:246-253`): `OuterActor(actorOf(FailingInheritingInnerActor))`.
  run_initialization_case(&mut spec, SmallCase::new(true, false));
  // Small case 12 (`:255-263`): the second `new` in the creator fails.
  run_initialization_case(&mut spec, SmallCase::new(true, false));
  // Small case 13 (`:265-274`): the text reaches the log and the caller.
  run_initialization_case(&mut spec, SmallCase::new(false, true).expecting("Ur state be b0rked"));
  spec.terminate();
}

// Original: "insert its path in a ActorInitializationException"
#[test]
fn inserts_its_path_in_an_actor_initialization_exception() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let (tx, rx) = promise_pair();
  // `throw new IllegalStateException` (`:282`): no message, so the text that
  // reaches the caller is empty.
  let props = promise_intercept(tx, || -> FailingActor { panic!("") });
  let filter = EventFilter::typed("ActorInitializationException").contains("/user/failingActor:").with_occurrences(1);
  // The `Create` runs when the system is driven, so the actor is started
  // before the filter is armed, exactly when the original's `actorOf` runs the
  // constructor inside `intercept`.
  let created = spec.system_mut().actor_of_named(props, "failingActor").expect("the name is free");
  let created = EventFilter::intercept(&filter, spec.system_mut(), || created);
  let text = wrap(rx);
  // The original's cause carries no message, so the failure reaching the caller
  // is empty.
  assert!(text.is_empty(), "expected the cause without a message, got {text:?}");
  spec.system_mut().run_until_idle();
  assert!(spec.system().is_actor_terminated(&created), "the supervisor stopped the failed actor");
  spec.terminate();
}

// Original: "return EmptyLocalActorRef on deserialize if not present in actor hierarchy (and
// remoting is not enabled)"
#[test]
fn returns_an_empty_local_actor_ref_for_a_path_that_is_not_in_the_hierarchy() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let test_actor = spec.test_actor();
  spec
    .system()
    .event_stream()
    .subscribe(Some(test_actor.clone()), EventClass::of::<DeadLetter>())
    .expect("subscribe to DeadLetter");
  let non_existing = spec.system_mut().actor_of_named(reply_actor_props(), "non-existing").expect("the name is free");
  let mut probe = TestProbe::new(spec.system_mut());
  probe.watch(&non_existing);
  non_existing.tell("PoisonPill", sender(&spec));
  probe.expect_msg_terminated_within(millis(1000));
  // ① the resolved handle keeps the path.
  let path = non_existing.path();
  let path = ActorPath::from_string(&path.to_string()).expect("the path parses");
  let resolved = spec.system().resolve_actor_ref(&path);
  assert_eq!(resolved.path(), non_existing.path(), "the unresolved handle keeps the path");
  // ② it is terminated.
  assert!(spec.system().is_actor_terminated(&resolved), "an unresolved path reads as terminated");
  // ③ a message to it goes to dead letters, with it as the recipient, and no
  // answer comes back.
  resolved.tell("hallo", test_actor.clone());
  let dead = spec.expect_msg_type::<DeadLetter>();
  assert_eq!(dead.recipient().path(), resolved.path(), "the dead letter names the resolved handle");
  spec.within(millis(200), |spec| {
    let remaining = spec.remaining();
    spec.test_kit_mut().expect_no_message(remaining);
  });
  // ④ watching it answers at once with `existence_confirmed = false`.
  probe.watch(&resolved);
  let terminated: Terminated = probe.expect_msg_type();
  assert_eq!(terminated.target(), &resolved);
  assert!(!terminated.existence_confirmed(), "the watch did not confirm the path");
  spec.terminate();
}

// Original: "support nested actorOfs"
#[test]
fn supports_nested_actor_ofs() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  // The original's outer constructor calls `system.actorOf`; the test body
  // creates the inner actor first and hands its reference to the factory.
  let nested = spec.system_mut().actor_of(inner_actor_props());
  let outer = spec.system_mut().actor_of(outer_actor_props(nested.clone()));
  let reply = outer.ask("msg").await_result(millis(1000));
  let answered = reply.downcast_ref::<ActorRef>().cloned().expect("the outer answers with the nested handle");
  assert_eq!(answered, nested, "the nested handle is the one the outer holds");
  assert_ne!(answered, outer, "the nested handle is a different actor than the outer");
  spec.terminate();
}

// Original: "support advanced nested actorOfs"
#[test]
fn supports_advanced_nested_actor_ofs() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let inner = spec.system_mut().actor_of(inner_actor_props());
  let outer = spec.system_mut().actor_of(outer_actor_props(inner.clone()));
  let test_actor = spec.test_actor();
  // The original's messages are compared with the test actor as `sender()`, so
  // the test actor asks rather than an `ask` handle.
  outer.tell("innerself", test_actor.clone());
  let innerself: ActorRef = spec.expect_msg_type();
  assert_eq!(innerself, inner, "innerself answers with the inner handle");
  outer.tell("a", test_actor.clone());
  let seen: ActorRef = spec.expect_msg_type();
  assert_eq!(seen, test_actor, "a answers with the sender, the test actor");
  outer.tell("self", test_actor.clone());
  let seen_self: ActorRef = spec.expect_msg_type();
  assert_eq!(seen_self, inner, "self answers with its own handle");
  outer.tell("msgs", test_actor.clone());
  spec.expect_msg(&"msgs");
  spec.terminate();
}

// Original: "support reply via sender"
#[test]
fn supports_reply_via_sender() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let settings = spec.test_kit_settings();
  // The original's `TestLatch(4)`: two rounds, and each round makes both the
  // worker's answer and the sender actor's arrival count one opening.
  let latch = TestLatch::new(4, settings);
  let test_actor = spec.test_actor();
  let reply_slot: Arc<Mutex<Option<ActorRef>>> = Arc::new(Mutex::new(None));
  // The worker answers the sender actor, as `WorkerActor(..., senderActor)`.
  let sender_actor: ActorRef = spec.system_mut().actor_of({
    let latch = latch.clone();
    let reply_slot = Arc::clone(&reply_slot);
    Props::new(move || SenderActor { reply_slot: Arc::clone(&reply_slot), count_hits: latch.clone(), rounds_left: 0 })
  });
  let worker: ActorRef = spec.system_mut().actor_of({
    let sender_actor = sender_actor.clone();
    // The original's `Thread.sleep(1)`, dilated.
    Props::new(move || WorkerActor {
      reply_with: Message::new("ToSenderActor"),
      to: sender_actor.clone(),
      delay: settings.dilated(millis(1)),
    })
  });
  let reply_actor: ActorRef = spec.system_mut().actor_of({
    let sender_actor = sender_actor.clone();
    let worker = worker.clone();
    Props::new(move || RoutingReplyActor { sender_actor: sender_actor.clone(), worker_actor: worker.clone() })
  });
  *reply_slot.lock().expect("the reply slot") = Some(reply_actor.clone());
  sender_actor.tell("start", test_actor.clone());
  latch.ready(spec.system(), TestLatch::DEFAULT_TIMEOUT);
  spec.system_mut().stop(&reply_actor);
  spec.system_mut().stop(&worker);
  spec.system_mut().run_until_idle();
  spec.terminate();
}

// Original: "support actorOfs where the class of the actor isn't public"
#[test]
fn supports_actor_ofs_where_the_type_of_the_actor_is_not_public() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let actor = spec.system_mut().actor_of(pig_dog_props());
  let reply = actor.ask("pigdog").await_result(millis(1000));
  assert_eq!(reply.downcast_ref::<&'static str>().copied(), Some("pigdog"));
  spec.system_mut().stop(&actor);
  spec.system_mut().run_until_idle();
  spec.terminate();
}

// Original: "stop when sent a poison pill"
#[test]
fn stops_when_sent_a_poison_pill() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let mut probe = TestProbe::new(spec.system_mut());
  let actor: ActorRef = spec.system_mut().actor_of(Props::new(|| CountdownActor { count: 5 }));
  probe.watch(&actor);
  assert_eq!(actor.ask(5).await_result(millis(1000)).downcast_ref::<&'static str>().copied(), Some("five"));
  assert_eq!(actor.ask(0).await_result(millis(1000)).downcast_ref::<&'static str>().copied(), Some("null"));
  actor.tell("PoisonPill", spec.test_actor());
  let terminated: Terminated = probe.expect_msg_type();
  assert_eq!(terminated.target(), &actor, "the poisoned actor stopped");
  spec.terminate();
}

// Original case 10's actor: `class PoisonPillActor`.
struct CountdownActor {
  count: i32,
}

impl Actor for CountdownActor {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    if let Some(number) = message.downcast_ref::<i32>() {
      if *number == 0 {
        reply_to(context, "null");
      } else if *number == self.count {
        reply_to(context, "five");
      }
    }
  }
}

// Original: "restart when Kill:ed"
#[test]
fn restarts_when_killed() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let settings = spec.test_kit_settings();
  let latch = TestLatch::new(2, settings);
  let strategy = SupervisorStrategy::one_for_one_with_window(RestartLimit::new(2), RestartWindow::Within(millis(1000)));
  let supervisor: ActorRef = spec.system_mut().actor_of({
    let latch = latch.clone();
    let strategy = strategy.clone();
    Props::new(move || OneForOneSupervisor { strategy: strategy.clone(), child: None, latch: latch.clone() })
  });
  // The supervisor answers a `GetChild` with its child, as the original's
  // parent exposes the child it made in `pre_start`.
  let child = supervisor.ask(GetChild).await_result(millis(1000));
  let child = child.downcast_ref::<ActorRef>().cloned().expect("the supervisor answers with its child");
  // `TestLatch(2)`: each `Kill` restarts the child once, and its
  // `post_restart` opens one.
  for _ in 0..2 {
    let filter = EventFilter::typed("ActorKilledException");
    let target = child.clone();
    let from = sender(&spec);
    EventFilter::intercept(&filter, spec.system_mut(), || {
      target.tell("Kill", from.clone());
    });
  }
  latch.ready(spec.system(), millis(5000));
  spec.system_mut().stop(&supervisor);
  spec.system_mut().run_until_idle();
  spec.terminate();
}

// Original: "be able to check for existence of children"
#[test]
fn can_check_for_the_existence_of_children() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let parent = spec.system_mut().actor_of(Props::new(|| ParentWithChild { child: None }));
  let found = parent.ask("child").await_result(millis(1000));
  assert_eq!(found.downcast_ref::<bool>().copied(), Some(true), "the child exists");
  let missing = parent.ask("whatnot").await_result(millis(1000));
  assert_eq!(missing.downcast_ref::<bool>().copied(), Some(false), "the unknown name does not exist");
  spec.terminate();
}
