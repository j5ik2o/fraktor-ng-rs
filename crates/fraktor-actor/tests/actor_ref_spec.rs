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
use std::{
  any::Any,
  sync::{
    atomic::{AtomicUsize, Ordering},
    mpsc, Arc,
  },
};

use fraktor_actor::kernel::{
  actor::{
    Actor, ActorContext, DeadLetter,
    Logging::{Error, LogEvent},
    Props, Terminated,
  },
  actor_path::ActorPath,
  actor_ref::ActorRef,
  event::EventClass,
  message::Message,
  supervision::{RestartLimit, RestartWindow, SupervisorStrategy},
};
use fraktor_testkit::{
  event_filter::{EventFilter, EventSinkRegistry},
  fraktorspec::FraktorSpec,
  test_kit::{ReceiveWhile, TestLatch, TestProbe},
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

// Original: `class InnerActor`, which answers `"innerself"` with its own
// reference, and every other message with the sender it saw.
struct InnerActor;

impl Actor for InnerActor {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    let self_ref = context.self_ref();
    if message == "innerself" {
      reply_to(context, self_ref);
    } else if let Some(outer) = message.downcast_ref::<ActorRef>() {
      // `case other => sender() ! other` for case 7's `a ? a`: the message is
      // the outer's handle, and the answer to the sender is that handle. The
      // port answers with the message rather than re-reading
      // `context.sender()`, because a forwarded `ActorRef` and the sender's
      // own entry cannot be told apart by value, and the reply is the same.
      let outer = outer.clone();
      reply_to(context, outer);
    } else {
      // `sender() ! other`, for a text. The test actor reads it as its queue
      // does not carry the `ask` handle the original's `await` would.
      if let Some(text) = message.downcast_ref::<&'static str>() {
        reply_to(context, *text);
      } else {
        let seen_sender = context.sender();
        reply_to(context, seen_sender);
      }
    }
  }
}

// The original has two outer actors, and the port keeps both shapes.
//
// Case 6 (original `:367-377`) is an anonymous actor whose constructor calls
// `system.actorOf`; it answers every message with the nested handle it holds,
// and the test reads that handle and checks it is not the outer's. Case 7
// (original `:379-388`) is `class OuterActor(val inner: ActorRef)`:
//
// ```scala
// def receive = {
//   case "self" => sender() ! self
//   case x      => inner.forward(x)
// }
// ```
//
// `"self"` never reaches the inner: the outer answers it with its own
// reference; everything else is forwarded, keeping the sender, and the inner
// answers `"innerself"` with its own handle and any other message with the
// sender it saw. In case 7 the port also sends `outer.clone()` (wrapped, see
// `ForwardedOuter`), which the outer forwards to the inner, and which comes
// back as the outer reference.
struct OuterActor {
  inner: ActorRef,
  answers_with_inner: bool,
}

impl Actor for OuterActor {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    if self.answers_with_inner {
      let inner = self.inner.clone();
      reply_to(context, inner);
      return;
    }
    // `outer.clone()` sent as the message: the outer forwards it to the inner,
    // keeping the sender, and the inner answers the sender with the outer.
    if let Some(forwarded) = message.downcast_ref::<ForwardedOuter>() {
      // Forward `outer.clone()` itself, keeping the test actor as the sender,
      // so the inner answers the test actor with the outer's handle.
      let outer = forwarded.0.clone();
      let sender = forwarded.1.clone();
      self.inner.tell(outer, sender);
      return;
    }
    if message == "self" {
      let own = context.self_ref();
      reply_to(context, own);
      return;
    }
    self.inner.forward(message, context);
  }
}

// Original case 9: an actor type that lives in a non-public module and is
// reached only through a public factory returning `Props` (design section
// 2.3). The module is private and re-exports nothing but `props`, so the type
// `PigDog` cannot be named from the test body, as in the original, where the
// class is not public. The factory is the only way to create it.
mod pigdog {
  use super::{reply_to, Actor, ActorContext, Message, Props};

  struct PigDog;

  impl Actor for PigDog {
    fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
      if let Some(text) = message.downcast_ref::<&'static str>() {
        reply_to(context, *text);
      }
    }
  }

  /// The public factory: the only way to reach the non-public `PigDog`. The
  /// module is private, so the type stays unnamed outside it; only this
  /// function and the `Props` it returns leave the module.
  pub(super) fn props() -> Props {
    Props::new(|| PigDog)
  }
}

/// Case 9's factory: the module is private and only this function names it.
fn pig_dog_props() -> Props {
  pigdog::props()
}

// The job a `WorkerActor` receives. Original: `case ReplyTo(replyTo) =>
// { work(); replyTo ! "complexReply" }`; round 1's `"work"` asks the worker to
// answer its sender instead.
#[derive(Clone)]
enum WorkerJob {
  Work,
  ReplyTo(ActorRef),
}

// Original: `class WorkerActor`: it answers `"workDone"` to its sender, or
// `"complexReply"` to the handle of a `ReplyTo`, after a dilated
// `Thread.sleep(1)`-sized pause, and then stops itself (`context.stop(self)`).
struct WorkerActor {
  delay: Duration,
}

impl Actor for WorkerActor {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    let Some(job) = message.downcast_ref::<WorkerJob>() else { return };
    std::thread::sleep(self.delay);
    let own = context.self_ref();
    match job {
      | WorkerJob::Work => {
        let target = context.sender();
        context.tell(&target, "workDone");
      },
      | WorkerJob::ReplyTo(reply_to) => {
        let target = reply_to.clone();
        context.tell(&target, "complexReply");
      },
    }
    context.stop(&own);
  }
}

// The copy of an answer that the sender actor passes to the test actor,
// together with the actor that sent the answer, which the test thread checks.
// The original has no such copy: its test thread never reads the answers.
struct SenderEcho {
  text: &'static str,
  from: ActorRef,
}

// Original case 8's `SenderActor`: it asks the reply actor for a reply and
// counts the `complexReply` and `simpleReply` answers that come back to it. The
// test actor sends every request, as the original's test body does (`clientRef !
// "complex"` and so on), so the latch cannot open on bookkeeping this actor did
// on its own. The answers reach the test actor as well, because the original
// never reads them there; the port keeps them in the test actor's queue so they
// are read side by side with the latch (see `supports_reply_via_sender`).
struct SenderActor {
  reply_actor: ActorRef,
  test_actor: ActorRef,
  latch: TestLatch,
}

impl Actor for SenderActor {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    let Some(text) = message.downcast_ref::<&'static str>() else { return };
    match *text {
      // `case "complex" => replyActor ! "complexRequest"` and its two siblings.
      | "complex" | "complex2" | "simple" => {
        let request = match *text {
          | "complex" => "complexRequest",
          | "complex2" => "complexRequest2",
          | _ => "simpleRequest",
        };
        let reply_actor = self.reply_actor.clone();
        reply_actor.tell(request, context.self_ref());
      },
      // `case "complexReply" | "simpleReply" => latch.countDown()`. The answer
      // is also passed on to the test actor, so the test thread can read which
      // actor sent it; the original does not need this, its latch is enough.
      | "complexReply" | "simpleReply" => {
        self.latch.count_down();
        let test_actor = self.test_actor.clone();
        let from = context.sender();
        context.tell(&test_actor, SenderEcho { text, from });
      },
      | _ => {},
    }
  }
}

// Original case 8's `ReplyActor`: `complexRequest` records the request's
// sender and starts a worker that answers this actor with `"workDone"`;
// `workDone` answers the recorded sender with `"complexReply"`. `complexRequest2`
// starts a worker that answers the request's sender directly with
// `"complexReply"`. `simpleRequest` answers the request's sender at once.
//
// The original starts a fresh `WorkerActor` under the reply actor for each
// request (`context.actorOf(Props[WorkerActor]())`); the port passes the
// worker's `Props` in its place, and each request runs a new instance.
struct RoutingReplyActor {
  worker: Props,
  reply_to: Option<ActorRef>,
}

impl Actor for RoutingReplyActor {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    let Some(text) = message.downcast_ref::<&'static str>() else { return };
    let request_sender = context.sender();
    match *text {
      | "complexRequest" => {
        self.reply_to = Some(request_sender);
        let worker = context.actor_of_with_strategy(self.worker.clone(), SupervisorStrategy::default_strategy());
        let own = context.self_ref();
        worker.tell(WorkerJob::Work, own);
      },
      | "complexRequest2" => {
        let worker = context.actor_of_with_strategy(self.worker.clone(), SupervisorStrategy::default_strategy());
        worker.tell(WorkerJob::ReplyTo(request_sender), context.self_ref());
      },
      // `case "workDone" => replyTo ! "complexReply"`. The reply actor names
      // itself as the sender of the answer, so the test actor can tell this
      // round's answer from round 2's.
      | "workDone" => {
        if let Some(reply_to) = self.reply_to.clone() {
          let own = context.self_ref();
          reply_to.tell("complexReply", own);
        }
      },
      // `case "simpleRequest" => sender() ! "simpleReply"`.
      | "simpleRequest" => context.tell(&request_sender, "simpleReply"),
      | _ => {},
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

// Original case 11: the child whose restart the hooks count. `pre_restart` and
// `post_restart` are counted apart, so one `Kill` must raise each count by
// exactly one; one shared latch would let two `post_restart` calls stand in for
// a `pre_restart` that never ran.
struct RestartCountingChild {
  pre_restarts: Arc<AtomicUsize>,
  post_restarts: Arc<AtomicUsize>,
}

impl Actor for RestartCountingChild {
  fn pre_restart(&mut self, _context: &mut ActorContext<'_>, _reason: &(dyn Any + Send)) {
    self.pre_restarts.fetch_add(1, Ordering::SeqCst);
  }

  fn post_restart(&mut self, _context: &mut ActorContext<'_>) {
    self.post_restarts.fetch_add(1, Ordering::SeqCst);
  }

  fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {}
}

// Original case 11's supervisor: the parent owns the child under
// `OneForOneStrategy(maxNrOfRetries = 2, within = 1.second)`.
struct OneForOneSupervisor {
  strategy: SupervisorStrategy,
  child: Option<ActorRef>,
  pre_restarts: Arc<AtomicUsize>,
  post_restarts: Arc<AtomicUsize>,
}

impl Actor for OneForOneSupervisor {
  fn supervisor_strategy(&self) -> SupervisorStrategy {
    self.strategy.clone()
  }

  fn pre_start(&mut self, context: &mut ActorContext<'_>) {
    let pre_restarts = Arc::clone(&self.pre_restarts);
    let post_restarts = Arc::clone(&self.post_restarts);
    let child =
      context.actor_of_with_strategy(restart_counting_child_props(pre_restarts, post_restarts), self.strategy.clone());
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

fn restart_counting_child_props(pre_restarts: Arc<AtomicUsize>, post_restarts: Arc<AtomicUsize>) -> Props {
  Props::new(move || RestartCountingChild {
    pre_restarts: Arc::clone(&pre_restarts),
    post_restarts: Arc::clone(&post_restarts),
  })
}

fn reply_actor_props() -> Props {
  Props::new(|| ReplyActor)
}

fn inner_actor_props() -> Props {
  Props::new(|| InnerActor)
}

/// Case 6's outer: it answers the nested handle it holds for any message.
fn nested_answering_outer_props(inner: ActorRef) -> Props {
  Props::new(move || OuterActor { inner: inner.clone(), answers_with_inner: true })
}

/// `a ? a` as the test body writes it: the outer's own handle, wrapped with the
/// sender so the outer can forward it. `Message::new` unwraps a bare
/// `ActorRef` payload, so the handle cannot be sent as an `ActorRef` directly.
#[derive(Clone, Debug)]
struct ForwardedOuter(ActorRef, ActorRef);

/// Case 7's `OuterActor`: `"self"` answers its own handle, the rest is forwarded.
fn forwarding_outer_props(inner: ActorRef) -> Props {
  Props::new(move || OuterActor { inner: inner.clone(), answers_with_inner: false })
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
// `inner_actor_props` and the two outer factories.
/// The actor the original's failing variants are made of; it never starts,
/// because the factory panics first.
struct FailingActor;

impl Actor for FailingActor {
  fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {}
}

/// One small case of case 1: the optional inner actor and the outer actor, each
/// failing or succeeding at creation.
///
/// The names are the original's pair: `failingInner` is the actor the outer is
/// built around, `failingOuter` is the actor holding it. A one-sided failure
/// therefore has two shapes: a healthy inner handed to a failing outer
/// (`new(false, true)`, small cases 2, 3, 5, 13) and a failing inner held by a
/// healthy outer (`new(true, false)`, small cases 4, 9, 11, 12).
struct SmallCase {
  inner_fails: bool,
  outer_fails: bool,
  /// The outer actor's failure text, when small case 13 names one.
  text: Option<&'static str>,
  /// Whether the outer is built around an inner actor at all. Small case 2's
  /// outer constructor throws before it creates one (design section 1.3.1,
  /// row 2); every other case creates both sides.
  inner_created: bool,
}

impl SmallCase {
  fn new(inner_fails: bool, outer_fails: bool) -> Self {
    Self { inner_fails, outer_fails, text: None, inner_created: true }
  }

  fn expecting(self, text: &'static str) -> Self {
    Self { text: Some(text), ..self }
  }

  fn without_inner(self) -> Self {
    Self { inner_created: false, ..self }
  }

  fn occurrences(&self) -> usize {
    usize::from(self.inner_fails) + usize::from(self.outer_fails)
  }
}

/// Runs one small case: creates the inner and the outer actor under the test
/// actor, as the original's `import system.actorOf` does (design section 1.3),
/// and checks the count of initialization failures per path, the report that
/// reached the caller, the stop of the failing side, the survival of the other
/// side, and that the name is free again under the same parent
/// (`contextStackMustBeEmpty()`).
fn run_initialization_case(spec: &mut FraktorSpec, case: SmallCase) {
  // Small case 13 names the root cause's text; the filter then matches the log
  // message exactly, as design section 1.3.1 requires. Every other case filters
  // by the cause name alone.
  let mut filter = EventFilter::typed("ActorInitializationException");
  if let Some(text) = case.text {
    filter = filter.with_message(text);
  }
  let sink = spec.system().event_sink().expect("the spec system has an event sink");
  let registry = sink.into_any().downcast::<EventSinkRegistry>().expect("the spec uses an EventSinkRegistry");
  let armed = registry.arm(filter);
  // The log count alone cannot tell one failing side from two failures of the
  // same side, so the test actor also reads the `Error` events themselves and
  // the count is taken per actor path below.
  let test_actor = spec.test_actor();
  spec.system().event_stream().subscribe(Some(test_actor), EventClass::of::<Error>()).expect("subscribe to Error");
  // Both sides exist in every case but small case 2: a one-sided failure still
  // leaves the other actor running, which is what design section 1.3.1 checks
  // (`FailingOuterActor` wraps a normal inner in small cases 3 and 5,
  // `OuterActor` wraps a failing inner in small case 4, and so on). Small case
  // 2's outer throws before it creates an inner, so only the outer is made
  // there (row 2).
  let makers = [("failingInner", case.inner_created && case.inner_fails), ("failingOuter", case.outer_fails)];
  let mut receivers = Vec::new();
  let mut created = Vec::new();
  let mut survivors = Vec::new();
  for (name, fails) in makers {
    if name == "failingInner" && !case.inner_created {
      continue;
    }
    let (actor, rx) = make_case_actor(spec, name, fails, case.text);
    if fails {
      receivers.push(rx.expect("the maker's receiver"));
      created.push(actor);
    } else {
      survivors.push(actor);
    }
  }
  spec.system_mut().run_until_idle();
  let matched = registry.matched(armed);
  registry.disarm(armed);
  assert_eq!(matched, case.occurrences(), "the failed starts log one ActorInitializationException each");
  // Per path: every actor that failed has exactly one initialization failure,
  // and an actor that did not fail has none. A total alone would pass when one
  // side failed twice and the other not at all, so the events are grouped by
  // the path they name. The `Error` events and the `Terminated` notices of the
  // failed actors do not arrive in a fixed order, so each wait takes only the
  // type it is about and puts the other type back for the other wait. The wait
  // has a limit.
  let mut failures_by_path: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
  let mut errors: Vec<Error> = Vec::new();
  while errors.len() < created.len() {
    let batch = spec.test_kit_mut().receive_while(ReceiveWhile::new().max(millis(5000)).idle(millis(100)), |message| {
      message.downcast_ref::<Error>().cloned()
    });
    if batch.is_empty() {
      break;
    }
    errors.extend(batch);
  }
  for event in errors {
    assert_eq!(event.cause_name(), Some("ActorInitializationException"), "the logged event is the failure");
    *failures_by_path.entry(event.log_source().to_string()).or_default() += 1;
  }
  let mut stopped: Vec<ActorRef> = Vec::new();
  while stopped.len() < created.len() {
    let batch = spec.test_kit_mut().receive_while(ReceiveWhile::new().max(millis(5000)).idle(millis(100)), |message| {
      message.downcast_ref::<Terminated>().map(|terminated| terminated.target().clone())
    });
    if batch.is_empty() {
      break;
    }
    stopped.extend(batch);
  }
  for actor in &created {
    let path = actor.path().to_string();
    assert_eq!(
      failures_by_path.get(&path).copied().unwrap_or_default(),
      1,
      "the failed actor at {path} logs exactly one initialization failure"
    );
  }
  for actor in &survivors {
    let path = actor.path().to_string();
    assert_eq!(
      failures_by_path.get(&path).copied().unwrap_or_default(),
      0,
      "the actor that did not fail at {path} logs no initialization failure"
    );
    assert!(!spec.system().is_actor_terminated(actor), "the side that did not fail keeps running");
  }
  assert_eq!(
    failures_by_path.len(),
    created.len(),
    "every initialization failure names a distinct failed path: {failures_by_path:?}"
  );
  for rx in receivers {
    let text = wrap(rx);
    if let Some(expected) = case.text {
      assert_eq!(text, expected, "the failure reached the caller with the original's text");
    } else {
      assert!(!text.is_empty(), "the failure reached the caller");
    }
  }
  // The supervisor stops the failed actors; their notices were collected above
  // together with the errors, as their order is not fixed.
  for actor in &created {
    assert!(stopped.contains(actor), "the supervisor stopped the failed actor");
  }
  // The surviving side has answered its part; stop it now, so the name check
  // below runs on a child table with nothing pending for it. That is what the
  // original's `contextStackMustBeEmpty()` reads: the stack of the creator
  // after both sides are done, not while one of them is still serving.
  for actor in &survivors {
    spec.system_mut().stop(actor);
  }
  spec.system_mut().run_until_idle();
  // `contextStackMustBeEmpty()`: the name is free again, and a normal actor of
  // the same name can be made under the *same parent*. The original creates
  // both the failed actor and its replacement with one `import system.actorOf`,
  // and recreating under `/user` would prove nothing about the namespace the
  // child actually lived in. The port creates both as top-level actors under
  // `/user`, with `system.actor_of_named` as design section 1.3 says, and
  // recreates with that same call.
  for actor in &created {
    let name = actor.path().name().to_string();
    let replacement = spec
      .system_mut()
      .actor_of_named(reply_actor_props(), name.as_str())
      .expect("the name of the failed actor is free again under /user");
    assert_eq!(
      replacement.path().parent(),
      actor.path().parent(),
      "the replacement sits under the same parent as the child that failed"
    );
    assert_eq!(
      replacement.path().parent(),
      spec.system().resolve_actor_ref(&replacement.path().parent()).path(),
      "both actors are children of the user guardian, as design section 1.3 says"
    );
    assert!(
      replacement.path().to_string().contains("/user/"),
      "the replacement sits under /user: {}",
      replacement.path()
    );
    assert_eq!(replacement.path().name(), name, "the replacement keeps the name under that parent");
    let reply = replacement.ask("hello").await_result(millis(1000));
    assert_eq!(reply.downcast_ref::<&'static str>().copied(), Some("hello"));
    spec.system_mut().stop(&replacement);
    spec.system_mut().run_until_idle();
  }
  // The test actor watches the survivors too, so their stops leave `Terminated`
  // notices in its queue; drain them so the next case starts with an empty one.
  for _ in 0..survivors.len() {
    let _: Terminated = spec.expect_msg_type();
  }
}

/// Starts one actor of a small case under `/user`, as the original's
/// `import system.actorOf` creates it (design section 1.3), watching it from
/// the test actor, and returns it with the maker's report receiver when it was
/// made to fail.
///
/// The actor is created through `system.actor_of_named` so that the failed
/// actor and its replacement (in `run_initialization_case`) share one parent,
/// `/user`. The test actor still watches it, so the stop of the failed side
/// reaches the test actor.
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
  let actor = spec.system_mut().actor_of_named(props, name).expect("the name is free under /user");
  spec.test_kit_mut().watch(&actor);
  (actor, fails.then_some(rx))
}

// Original: "not allow Actors to be created outside of an actorOf"
#[test]
fn does_not_allow_actors_to_be_created_outside_of_an_actor_of() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  // Small case 1 (`:150-152`) is not built (decision 4).
  // Small case 2 (`:156-166`): the outer constructor's `new Actor`. Its
  // `FailingOuterActor(message)` throws in the outer, so no inner is created
  // (design section 1.3.1, row 2).
  run_initialization_case(&mut spec, SmallCase::new(false, true).without_inner());
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
  let test_actor = spec.test_actor();
  // The original reads the path off the logged exception. The port reads it off
  // the `Error` event's own `cause_message()`, not off a filter that happens to
  // match: the cause text itself has to name the failed path.
  spec.system().event_stream().subscribe(Some(test_actor), EventClass::of::<Error>()).expect("subscribe to Error");
  let (tx, rx) = promise_pair();
  // `throw new IllegalStateException` (`:282`): no message, so the text that
  // reaches the caller is empty.
  let props = promise_intercept(tx, || -> FailingActor { panic!("") });
  let created = spec.system_mut().actor_of_named(props, "failingActor").expect("the name is free");
  // The factory runs when the system is driven, so the failure is raised here.
  spec.system_mut().run_until_idle();
  let text = wrap(rx);
  // The original's cause carries no message, so the failure reaching the caller
  // is empty.
  assert!(text.is_empty(), "expected the cause without a message, got {text:?}");
  // The event the parent logged for the failed child: its source is the child's
  // path, and its `cause_message()` carries that path as the original's
  // exception message does (`ActorInitializationException`, design section 1.3).
  let event: Error = spec.expect_msg_type();
  assert_eq!(event.log_source(), created.path().to_string(), "the event names the failed child");
  assert_eq!(event.cause_name(), Some("ActorInitializationException"), "the logged event is the failure");
  let cause = event.cause_message().expect("the initialization failure carries a cause message");
  // The original's `EventFilter(pattern = "/user/failingActor:")` matches the
  // exception message, which is `pekko://<system>/user/failingActor: exception
  // during creation, root cause message: [...]`. The port reads the same fact
  // off the event's own cause text: it starts with the failed actor's path and
  // the colon, as the original's message does.
  assert!(
    created.path().to_string().ends_with("/user/failingActor"),
    "case 2 names its actor under /user: {}",
    created.path()
  );
  let expected_prefix = format!("{}:", created.path());
  assert!(
    cause.starts_with(&expected_prefix),
    "the cause message itself starts with the failed path {expected_prefix:?}, got {cause:?}"
  );
  assert!(cause.contains("/user/failingActor:"), "the cause message itself names `/user/failingActor:`, got {cause:?}");
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
  let outer = spec.system_mut().actor_of(nested_answering_outer_props(nested.clone()));
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
  // Case 7's `OuterActor(system.actorOf(Props(new InnerActor)))`; the test body
  // creates the inner first, as in case 6, and hands it to the factory.
  let outer = spec.system_mut().actor_of(forwarding_outer_props(inner.clone()));
  let test_actor = spec.test_actor();
  // `val inner = Await.result(a ? "innerself", ...)`: the outer forwards
  // `"innerself"`, and the inner answers with its own handle, which is not the
  // outer's.
  outer.tell("innerself", test_actor.clone());
  let answered: ActorRef = spec.expect_msg_type();
  assert_eq!(answered, inner, "innerself answers with the handle the outer holds");
  assert_ne!(answered, outer, "innerself is not the outer's handle");
  // `Await.result(a ? a, ...) should ===(a)`: the test body sends `outer.clone()`
  // as its own message. `OuterActor` answers `"self"` with its own handle and
  // never forwards it, and a bare `ActorRef` would be forwarded again and again,
  // so the test body sends it wrapped, the outer forwards the handle to the
  // inner, and the inner answers the test actor with it. The answer is compared
  // with `outer`.
  outer.tell(ForwardedOuter(outer.clone(), test_actor.clone()), test_actor.clone());
  let seen_outer: ActorRef = spec.expect_msg_type();
  assert_eq!(seen_outer, outer, "the forwarded outer answers back as the outer the test body sent");
  // The test body also sends the outer's own handle as a bare `ActorRef`. It is
  // neither `"self"` nor a `ForwardedOuter`, so the outer forwards it to the
  // inner, and the inner answers with the handle it was sent, which is the
  // outer. A bare `ActorRef` is not unwrapped as a message, so this is the
  // direct form of `a ? a`.
  outer.tell(outer.clone(), test_actor.clone());
  let seen_direct: ActorRef = spec.expect_msg_type();
  assert_eq!(seen_direct, outer, "a bare outer handle is forwarded and answers back as the outer");
  // `Await.result(a ? "self", ...) should ===(a)`: the outer answers `"self"`
  // with its own handle and does not forward it, so the answer is the outer
  // and not the inner.
  outer.tell("self", test_actor.clone());
  let seen_self: ActorRef = spec.expect_msg_type();
  assert_eq!(seen_self, outer, "self answers with the outer's own handle");
  assert_ne!(seen_self, inner, "self does not answer with the inner's handle");
  // `Await.result(a ? "msg", ...) should ===("msg")`: `"msg"` is not `"self"`,
  // so it is forwarded to the inner, which answers the sender with the text.
  // The original's sender is its `ask` handle and the port's is the test actor,
  // so the test actor reads the text.
  outer.tell("msg", test_actor.clone());
  let forwarded = spec.expect_msg(&"msg");
  assert_eq!(forwarded.downcast_ref::<&'static str>().copied(), Some("msg"), "the text is forwarded back");
  spec.terminate();
}

// Original: "support reply via sender"
#[test]
fn supports_reply_via_sender() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let settings = spec.test_kit_settings();
  // The original's `TestLatch(4)`: each round opens it twice (the reply actor's
  // answer and the worker's), and `latch.reset()` closes it again between the
  // rounds. The test actor is read for every answer of both rounds, so the latch
  // cannot open on bookkeeping alone.
  let latch = TestLatch::new(4, settings);
  let test_actor = spec.test_actor();
  // The original's `Thread.sleep(1.second.dilated)`, scaled by the time factor.
  let delay = settings.dilated(millis(1));
  // The original's `Props[WorkerActor]`: the reply actor starts one worker per
  // request, and each worker stops itself once it has answered.
  let worker = Props::new(move || WorkerActor { delay });
  let reply_actor: ActorRef = spec.system_mut().actor_of({
    let worker = worker.clone();
    Props::new(move || RoutingReplyActor { worker: worker.clone(), reply_to: None })
  });
  let sender_actor: ActorRef = spec.system_mut().actor_of({
    let latch = latch.clone();
    let reply_actor = reply_actor.clone();
    let test_actor = test_actor.clone();
    Props::new(move || SenderActor {
      reply_actor: reply_actor.clone(),
      test_actor: test_actor.clone(),
      latch: latch.clone(),
    })
  });
  // Round 1: the original's `complex`, `simple`, `simple`, `simple`. Each request
  // goes through the sender actor to the reply actor.
  sender_actor.tell("complex", test_actor.clone());
  for _ in 0..3 {
    sender_actor.tell("simple", test_actor.clone());
  }
  latch.ready(spec.system(), TestLatch::DEFAULT_TIMEOUT);
  // Round 1's answers: the single `complexReply` travels worker → reply actor →
  // sender actor, and the reply actor sends `"workDone"` to itself, so the
  // answer names the reply actor, not the worker. The three `simpleReply` are
  // the reply actor's own answers.
  let round_one = spec.test_kit_mut().receive_n(4, TestLatch::DEFAULT_TIMEOUT);
  let mut texts = Vec::new();
  for message in &round_one {
    let echo = message.downcast_ref::<SenderEcho>().expect("round 1's answers are the sender actor's copies");
    let answer = echo.text;
    assert_eq!(message.sender(), &sender_actor, "the sender actor passes each answer on to the test actor");
    if answer == "complexReply" {
      assert_eq!(echo.from, reply_actor, "round 1's complexReply is sent by the reply actor");
    } else {
      assert_eq!(answer, "simpleReply");
      assert_eq!(echo.from, reply_actor, "round 1's simpleReply is sent by the reply actor");
    }
    texts.push(answer);
  }
  assert_eq!(texts.iter().filter(|text| **text == "complexReply").count(), 1, "round 1 answers complex once");
  assert_eq!(texts.iter().filter(|text| **text == "simpleReply").count(), 3, "round 1 answers simple three times");
  // Round 1 answers nothing else: the same upper bound `receive_n` is given,
  // with no further message in it.
  let upper_bound = settings.dilated(TestLatch::DEFAULT_TIMEOUT);
  spec.test_kit_mut().expect_no_message(upper_bound);
  // Round 2: the original's `latch.reset()`, `complex2`, `simple`, `simple`,
  // `simple`. The second round's first request is `complex2`, which the reply
  // actor gives to a worker that answers the request's sender directly.
  latch.reset();
  sender_actor.tell("complex2", test_actor.clone());
  for _ in 0..3 {
    sender_actor.tell("simple", test_actor.clone());
  }
  latch.ready(spec.system(), TestLatch::DEFAULT_TIMEOUT);
  // Round 2's answers: the `complexReply` of round 2 is sent by the worker
  // itself (`ReplyTo(sender())` names no reply actor), so its sender is a
  // worker under the reply actor and not the reply actor; the three
  // `simpleReply` are still the reply actor's.
  let round_two = spec.test_kit_mut().receive_n(4, TestLatch::DEFAULT_TIMEOUT);
  let mut texts = Vec::new();
  let mut direct_sender = None;
  for message in &round_two {
    let echo = message.downcast_ref::<SenderEcho>().expect("round 2's answers are the sender actor's copies");
    let answer = echo.text;
    assert_eq!(message.sender(), &sender_actor, "the sender actor passes each answer on to the test actor");
    if answer == "complexReply" {
      assert_eq!(echo.from.path().parent(), reply_actor.path(), "round 2's worker answers under the reply actor");
      assert_ne!(echo.from, reply_actor, "round 2's complexReply does not come from the reply actor");
      assert!(
        echo.from.path().name().starts_with('$'),
        "round 2's answer names a generated worker, not a fixed child: {}",
        echo.from.path()
      );
      direct_sender = Some(echo.from.clone());
    } else {
      assert_eq!(answer, "simpleReply");
      assert_eq!(echo.from, reply_actor, "round 2's simpleReply is sent by the reply actor");
    }
    texts.push(answer);
  }
  assert_eq!(texts.iter().filter(|text| **text == "complexReply").count(), 1, "round 2 answers complex2 once");
  assert_eq!(texts.iter().filter(|text| **text == "simpleReply").count(), 3, "round 2 answers simple three times");
  // Round 2 answers nothing else either, under the same upper bound as
  // `receive_n`.
  spec.test_kit_mut().expect_no_message(upper_bound);
  let direct_sender = direct_sender.expect("round 2 has one complexReply");
  // The workers stop themselves, as `context.stop(self)` does; one worker per
  // complex request, so the two workers leave the hierarchy once idle.
  spec.system_mut().run_until_idle();
  assert!(spec.system().is_actor_terminated(&direct_sender), "round 2's worker stopped itself after answering");
  spec.system_mut().stop(&reply_actor);
  spec.system_mut().stop(&sender_actor);
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
  let pre_restarts = Arc::new(AtomicUsize::new(0));
  let post_restarts = Arc::new(AtomicUsize::new(0));
  let strategy = SupervisorStrategy::one_for_one_with_window(RestartLimit::new(2), RestartWindow::Within(millis(1000)));
  let supervisor: ActorRef = spec.system_mut().actor_of({
    let pre_restarts = Arc::clone(&pre_restarts);
    let post_restarts = Arc::clone(&post_restarts);
    let strategy = strategy.clone();
    Props::new(move || OneForOneSupervisor {
      strategy: strategy.clone(),
      child: None,
      pre_restarts: Arc::clone(&pre_restarts),
      post_restarts: Arc::clone(&post_restarts),
    })
  });
  // The supervisor answers a `GetChild` with its child, as the original's
  // parent exposes the child it made in `pre_start`.
  let child = supervisor.ask(GetChild).await_result(millis(1000));
  let child = child.downcast_ref::<ActorRef>().cloned().expect("the supervisor answers with its child");
  // One `Kill` restarts the child once, and that one restart runs both hooks,
  // each exactly once. A second `Kill` would let a doubled `post_restart` stand
  // in for a `pre_restart` that never ran, so only one is sent.
  let filter = EventFilter::typed("ActorKilledException");
  let target = child.clone();
  let from = sender(&spec);
  EventFilter::intercept(&filter, spec.system_mut(), || {
    target.tell("Kill", from.clone());
  });
  spec
    .await_cond(|| pre_restarts.load(Ordering::SeqCst) == 1 && post_restarts.load(Ordering::SeqCst) == 1, millis(5000));
  assert_eq!(pre_restarts.load(Ordering::SeqCst), 1, "one `Kill` runs `pre_restart` exactly once");
  assert_eq!(post_restarts.load(Ordering::SeqCst), 1, "one `Kill` runs `post_restart` exactly once");
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
