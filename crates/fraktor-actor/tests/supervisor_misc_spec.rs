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

// Checked against Pekko v2.0.0 (SHA 9fbd2283b1cf701c90a1223bf621942c191db61f): the upstream test
// file changes only by 870d4499cc #2235; the cases and their inputs are unchanged.
// The record is docs/baseline/pekko-v2.0.0-migration/batch-3.md.
//
// Ported from Pekko v1.7.1:
//   actor-tests/src/test/scala/org/apache/pekko/actor/SupervisorMiscSpec.scala
//
// Original test names are kept as comments next to each ported case.
//
// The original builds each actor with `Props(new Actor { ... })` and relies on
// the production `ActorContext` to start named children, watch them, stop them,
// and let a parent override `supervisorStrategy` and its
// `handleChildTerminated` hook. Every case below calls those production
// operations; none of them reproduces the supervision semantics inside the
// test. Until the production `ActorContext::actor_of`, `watch`, `stop`, the
// `InvalidActorNameException` type, the `Props::with_dispatcher` name record,
// and the per-parent `supervisor_strategy` hook exist, the file does not
// compile. That compile error is the intended red for this node.
//
// The original config declares `pinned-dispatcher` (a `PinnedDispatcher`) and
// `test-dispatcher`. The goal document rules out interpreting HOCON here: the
// port records the dispatcher name on `Props` and does not model the thread
// pool. Case 1 therefore checks that a worker created under each dispatcher
// restarts and answers, which is what the original asserts, and not the thread
// of a dispatcher.
//
// The original uses a `CountDownLatch(4)` to count `postRestart` calls across
// the four workers. Pekko's dispatchers run those on other threads, so the
// original awaits the latch. The port drives one shard on the test thread, so
// the four restarts have all happened by the time the queue is drained; the
// counter is a test-side helper of the same shape as the one G0d used for a
// latch.

use core::sync::atomic::{AtomicUsize, Ordering};
use core::time::Duration;

use std::sync::Arc;

use fraktor_actor::kernel::actor::{Actor, ActorContext, Props};
use fraktor_actor::kernel::actor_path::ActorPath;
use fraktor_actor::kernel::actor_ref::ActorRef;
use fraktor_actor::kernel::actor_system::ActorSystem;
use fraktor_actor::kernel::message::Message;
use fraktor_actor::kernel::supervision::{Directive, SupervisorStrategy};
use fraktor_testkit::event_filter::EventFilter;
use fraktor_testkit::fraktorspec::FraktorSpec;
use fraktor_testkit::implicit_sender::ImplicitSender;

const SPEC_SYSTEM_NAME: &str = "SupervisorMiscSpec";

const PINNED_DISPATCHER: &str = "pinned-dispatcher";

const TEST_DISPATCHER: &str = "test-dispatcher";

fn default_timeout() -> Duration {
  Duration::from_secs(3)
}

fn text_of(message: &Message) -> Option<&'static str> {
  message.downcast_ref::<&'static str>().copied()
}

// Original: `Props.empty`
//
// The child of the supervision cases has no behaviour of its own; it exists so
// the parent can create it, watch it, and stop it under a name.
struct EmptyActor;

impl Actor for EmptyActor {
  fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {}
}

fn empty_props() -> Props {
  Props::new(|| EmptyActor)
}

// Original: the `Supervisor` helper of `actor-tests/.../Supervisor.scala`, which
// answers a `Props` with a child it creates under its own strategy.
//
// Its `preRestart` is empty, so the children survive a restart of the helper,
// as the original comment notes. The strategy it reports through
// `supervisor_strategy` is the one its children are created under.
struct SupervisorActor {
  strategy: SupervisorStrategy,
  created: u32,
}

impl Actor for SupervisorActor {
  // Original: `case x: Props => sender() ! context.actorOf(x)`
  //
  // The original asks for a child with no name, so each child gets its own
  // generated name. The port's `actor_of` takes a name, so the supervisor
  // hands out a distinct one per request.
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    let Some(props) = message.downcast_ref::<Props>() else {
      return;
    };
    let name = format!("worker-{}", self.created);
    self.created += 1;
    let child = context.actor_of(props.clone(), &name);
    let child = match child {
      | Ok(child) => child,
      | Err(error) => panic!("the supervisor could not start its child: {error}"),
    };
    context.sender().tell(child, context.self_ref());
  }

  fn supervisor_strategy(&self) -> SupervisorStrategy {
    self.strategy.clone()
  }

  fn pre_restart(&mut self, _context: &mut ActorContext<'_>, _reason: &(dyn core::any::Any + Send)) {}
}

// Original: the anonymous `Actor` of case 1. Its `postRestart` counts down the
// latch, it answers "status" with "OK", and it stops itself for any other
// message. The alternative message never arrives because `Kill` is handled by
// the cell before `receive`, as it is in the original.
struct RestartCountingWorker {
  restarts: Arc<AtomicUsize>,
}

impl Actor for RestartCountingWorker {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    match text_of(&message) {
      | Some("status") => context.sender().tell("OK", context.self_ref()),
      | _ => {
        let self_ref = context.self_ref();
        context.stop(&self_ref);
      },
    }
  }

  // Original: `override def postRestart(cause: Throwable): Unit = { countDownLatch.countDown() }`
  fn post_restart(&mut self, _context: &mut ActorContext<'_>) {
    self.restarts.fetch_add(1, Ordering::SeqCst);
  }
}

// Case 1 worker factory: the worker counts restarts into a shared counter and
// is started under a dispatcher whose name the recipe records.
fn counting_worker_props(restarts: Arc<AtomicUsize>, dispatcher: &str) -> Props {
  let factory = move || RestartCountingWorker { restarts: Arc::clone(&restarts) };
  Props::new(factory).with_dispatcher(dispatcher)
}

// Original: the anonymous `Actor` of case 2. Its constructor creates a named
// child, it throws on any `Exception`, and its `preStart` tells the test actor
// "preStart".
struct NamedChildInConstructor {
  test_actor: ActorRef,
}

impl Actor for NamedChildInConstructor {
  fn receive(&mut self, _context: &mut ActorContext<'_>, message: Message) {
    if let Some(reason) = message.downcast_ref::<TestException>() {
      panic!("{}", reason.message);
    }
  }

  // Original: `override def preStart(): Unit = testActor ! "preStart"`
  fn pre_start(&mut self, context: &mut ActorContext<'_>) {
    context.actor_of(empty_props(), "bob").expect("the constructor must create bob before reporting preStart");
    self.test_actor.tell("preStart", self.test_actor.clone());
  }
}

// Original: `new Exception(message)`. The port cannot recover a thrown value, so
// the payload carries the message the test wants to see in the log event.
#[derive(Clone)]
struct TestException {
  message: &'static str,
}

// Original: the anonymous `Actor` of case 3. It watches a named child and, when
// the child terminates, creates a new child under the same name and reports
// whether the new child is a fresh instance at the same path. On "engage" it
// stops the child.
struct RecreateChildWhenTerminated {
  test_actor: ActorRef,
  kid: Option<ActorRef>,
}

impl Actor for RecreateChildWhenTerminated {
  // Original: `val kid = context.watch(context.actorOf(Props.empty, "foo"))`
  fn pre_start(&mut self, context: &mut ActorContext<'_>) {
    let kid = match context.actor_of(empty_props(), "foo") {
      | Ok(kid) => kid,
      | Err(error) => panic!("the parent could not start its kid: {error}"),
    };
    self.kid = Some(context.watch(&kid));
  }

  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    if text_of(&message) == Some("engage") {
      let kid = self.kid.clone().expect("the kid was created in pre_start");
      context.stop(&kid);
      return;
    }
    if let Some(terminated) = message.downcast_ref::<fraktor_actor::kernel::actor::Terminated>() {
      let kid = self.kid.clone().expect("the kid was created in pre_start");
      if terminated.target() != &kid {
        return;
      }
      let created = context.actor_of(empty_props(), "foo");
      let result = match created {
        | Err(error) => format!("{error}"),
        | Ok(new_kid) => classify_recreated_kid(context, &kid, &new_kid),
      };
      self.test_actor.tell(result, self.test_actor.clone());
    }
  }
}

// Original: the `if`/`else` chain of case 3, kept as its own function so the
// observable strings stay exactly as the original writes them. `context`
// answers whether a handle is terminated, mirroring `ActorRef.isTerminated`.
fn classify_recreated_kid(context: &ActorContext<'_>, kid: &ActorRef, new_kid: &ActorRef) -> String {
  if new_kid == kid {
    String::from("Failure: context.actorOf returned the same instance!")
  } else if !context.is_terminated(kid) {
    String::from("Kid is zombie")
  } else if context.is_terminated(new_kid) {
    String::from("newKid was stillborn")
  } else if kid.path() != new_kid.path() {
    String::from("The kids do not share the same path")
  } else {
    String::from("green")
  }
}

// Original: the anonymous `Actor` of case 4. It creates a child named "foo",
// stops it, and immediately creates another "foo". Pekko raises
// `InvalidActorNameException` because the first child is still alive when the
// second is asked for; the actor reports "green" on that exception and "red"
// when the second creation succeeds.
struct RejectDuplicateChildName {
  test_actor: ActorRef,
}

impl Actor for RejectDuplicateChildName {
  // Original: `case "engage" => try { ... } catch { case _: InvalidActorNameException => testActor ! "green" }`
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    if text_of(&message) != Some("engage") {
      return;
    }
    let kid = match context.actor_of(empty_props(), "foo") {
      | Ok(kid) => kid,
      | Err(error) => panic!("the first child name was refused: {error}"),
    };
    context.stop(&kid);
    let result = match context.actor_of(empty_props(), "foo") {
      | Err(error) if error.is::<fraktor_actor::kernel::actor::InvalidActorNameException>() => String::from("green"),
      | Err(error) => format!("unexpected error: {error}"),
      | Ok(_) => String::from("red"),
    };
    self.test_actor.tell(result, self.test_actor.clone());
  }
}

// Original: the anonymous `Actor` of case 5. Its `supervisorStrategy` overrides
// `handleChildTerminated` to create a new child under the terminated child's
// name, reporting whether the new child is a fresh instance at the same path.
// On "engage" it stops a child named "Robert".
struct RecreateChildInFaultHandling {
  test_actor: ActorRef,
}

impl Actor for RecreateChildInFaultHandling {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    if text_of(&message) != Some("engage") {
      return;
    }
    let child = match context.actor_of(empty_props(), "Robert") {
      | Ok(child) => child,
      | Err(error) => panic!("the parent could not start Robert: {error}"),
    };
    context.stop(&child);
  }

  // Original: `override val supervisorStrategy = new OneForOneStrategy()(...) {
  //   override def handleChildTerminated(context, child, children): Unit = {
  //     val newKid = context.actorOf(Props.empty, child.path.name)
  //     testActor ! { if ((newKid ne child) && newKid.path == child.path) "green" else "red" }
  //   }
  // }`
  fn supervisor_strategy(&self) -> SupervisorStrategy {
    let test_actor = self.test_actor.clone();
    SupervisorStrategy::one_for_one(3).with_child_terminated(move |context, child, _children| {
      let name = String::from(child.path().name());
      match context.actor_of(empty_props(), &name) {
        | Err(error) => test_actor.tell(format!("{error}"), test_actor.clone()),
        | Ok(new_kid) => {
          let result = if new_kid != *child && new_kid.path() == child.path() { "green" } else { "red" };
          test_actor.tell(result, test_actor.clone());
        },
      }
    })
  }
}

// Original: the anonymous `Actor` of case 6. Its `supervisorStrategy`'s decider
// receives the failing child as `sender()` and stops the child, reporting the
// child to the test actor. On "doit" it creates a named child "child" and sends
// it `Kill`.
struct FailingChildAsSender {
  test_actor: ActorRef,
}

impl Actor for FailingChildAsSender {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    if text_of(&message) != Some("doit") {
      return;
    }
    let child = match context.actor_of(empty_props(), "child") {
      | Ok(child) => child,
      | Err(error) => panic!("the parent could not start child: {error}"),
    };
    child.tell("Kill", context.self_ref());
  }

  // Original: `override val supervisorStrategy = OneForOneStrategy() {
  //   case _: Exception => testActor ! sender(); SupervisorStrategy.Stop
  // }`
  fn supervisor_strategy(&self) -> SupervisorStrategy {
    let test_actor = self.test_actor.clone();
    SupervisorStrategy::one_for_one(3).with_decider(move |child, _failure| {
      test_actor.tell(child.clone(), test_actor.clone());
      Directive::Stop
    })
  }
}

struct SupervisorMiscSpecCase {
  fraktor_spec: FraktorSpec,
}

impl SupervisorMiscSpecCase {
  fn new() -> Self {
    Self { fraktor_spec: FraktorSpec::new(SPEC_SYSTEM_NAME) }
  }

  fn system(&mut self) -> &mut ActorSystem {
    self.fraktor_spec.system_mut()
  }

  fn test_actor(&self) -> ActorRef {
    self.fraktor_spec.test_actor()
  }

  fn self_actor_ref(&self) -> ActorRef {
    ImplicitSender::self_actor_ref(&self.fraktor_spec)
  }

  fn actor_of(&mut self, props: Props) -> ActorRef {
    self.system().actor_of(props)
  }

  fn tell(&mut self, target: &ActorRef, message: &'static str) {
    let sender = self.self_actor_ref();
    target.tell(message, sender);
  }

  fn expect_str(&mut self, expected: &str) {
    let found: String = self.fraktor_spec.expect_msg_type();
    assert_eq!(found, expected, "expected {expected:?}, found {found:?}");
  }

  // Original: `Await.result((supervisor ? props).mapTo[ActorRef], timeout.duration)`
  fn child_under(&mut self, supervisor: &ActorRef, props: Props) -> ActorRef {
    let reply = supervisor.ask(props).await_result(default_timeout());
    reply.downcast_ref::<ActorRef>().cloned().expect("a child handle from the supervisor")
  }

  // Original: `filterEvents(EventFilter[Exception]("Kill")) { ... }`
  //
  // Each `Kill` publishes one `ActorKilledException` whose message is "Kill".
  // `filterEvents` only mutes the events; the port's `intercept` also asserts
  // the count, so one interception wraps each Kill, as the actor life cycle
  // cases do. Four Kills are four interceptions.
  fn kill(&mut self, target: &ActorRef) {
    let filter = EventFilter::typed("ActorKilledException").with_message("Kill");
    let sender = self.self_actor_ref();
    let system = self.fraktor_spec.system_mut();
    let target = target.clone();
    EventFilter::intercept(&filter, system, move || {
      target.tell("Kill", sender);
    });
  }
}

impl Drop for SupervisorMiscSpecCase {
  fn drop(&mut self) {
    self.fraktor_spec.terminate();
  }
}

// "A Supervisor" must { "restart a crashing actor and its dispatcher for any dispatcher" in { ... } }
#[test]
fn restart_a_crashing_actor_and_its_dispatcher_for_any_dispatcher() {
  let mut case = SupervisorMiscSpecCase::new();

  let supervisor = {
    let strategy = SupervisorStrategy::one_for_one(3);
    case.actor_of(Props::new(move || SupervisorActor { strategy: strategy.clone(), created: 0 }))
  };

  let restarts = Arc::new(AtomicUsize::new(0));

  let actor1 = case.child_under(&supervisor, counting_worker_props(Arc::clone(&restarts), PINNED_DISPATCHER));
  let actor2 = case.child_under(&supervisor, counting_worker_props(Arc::clone(&restarts), PINNED_DISPATCHER));
  let actor3 = case.child_under(&supervisor, counting_worker_props(Arc::clone(&restarts), TEST_DISPATCHER));
  let actor4 = case.child_under(&supervisor, counting_worker_props(Arc::clone(&restarts), PINNED_DISPATCHER));

  case.kill(&actor1);
  case.kill(&actor2);
  case.kill(&actor3);
  case.kill(&actor4);

  case.system().run_until_idle();
  assert_eq!(restarts.load(Ordering::SeqCst), 4, "each of the four workers restarts once");

  // Original: `case (id, Await.result(f)) should === ((id, "OK"))` — a value
  // comparison, so the reply is compared as the string it carries.
  for (id, actor) in [("actor1", &actor1), ("actor2", &actor2), ("actor3", &actor3), ("actor4", &actor4)] {
    actor.tell("status", case.self_actor_ref());
    let reply = case.fraktor_spec.expect_msg_within(default_timeout(), "OK");
    assert_eq!(reply, "OK", "{id} must answer its status with OK");
  }
}

// "A Supervisor" must { "be able to create named children in its constructor" in { ... } }
#[test]
fn be_able_to_create_named_children_in_its_constructor() {
  let mut case = SupervisorMiscSpecCase::new();
  let test_actor = case.test_actor();
  let actor = case.actor_of(Props::new(move || NamedChildInConstructor { test_actor: test_actor.clone() }));

  let message = "weird message";
  let filter = EventFilter::typed("RuntimeException").with_message(message);
  let target = actor.clone();
  let sender = case.self_actor_ref();
  let system = case.fraktor_spec.system_mut();
  EventFilter::intercept(&filter, system, move || {
    target.tell(TestException { message }, sender);
  });

  case.fraktor_spec.expect_msg_within(default_timeout(), "preStart");
  case.fraktor_spec.expect_msg_within(default_timeout(), "preStart");

  // Original: `a.isTerminated should ===(false)` — the restarted actor is alive.
  assert!(!case.system().is_actor_terminated(&actor), "the actor restarted by the exception must still be alive");
}

// Occupying bob first exercises the real refusal in the named-child hook.
struct StartsNamedChildWithOccupiedName {
  actor: NamedChildInConstructor,
}

impl Actor for StartsNamedChildWithOccupiedName {
  fn pre_start(&mut self, context: &mut ActorContext<'_>) {
    context.actor_of(empty_props(), "bob").expect("the fixture must occupy bob first");
    self.actor.pre_start(context);
  }

  fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {}
}

#[test]
fn a_failed_named_child_creation_does_not_report_pre_start_success() {
  let mut case = SupervisorMiscSpecCase::new();
  let notifications = Arc::new(AtomicUsize::new(0));
  let observer = case.system().function_ref("notifications", {
    let notifications = Arc::clone(&notifications);
    move |_sender: &ActorRef, message: Message| {
      if text_of(&message) == Some("preStart") {
        notifications.fetch_add(1, Ordering::SeqCst);
      }
    }
  });
  let actor = case.actor_of(Props::new(move || StartsNamedChildWithOccupiedName {
    actor: NamedChildInConstructor { test_actor: observer.clone() },
  }));
  case.system().run_until_idle();

  assert_eq!(notifications.load(Ordering::SeqCst), 0, "a refused bob creation was reported as successful preStart");
  assert!(case.system().is_actor_terminated(&actor), "the failed creation must fail actor initialization");
}

// "A Supervisor" must { "be able to recreate child when old child is Terminated" in { ... } }
#[test]
fn be_able_to_recreate_child_when_old_child_is_terminated() {
  let mut case = SupervisorMiscSpecCase::new();
  let test_actor = case.test_actor();
  let parent =
    case.actor_of(Props::new(move || RecreateChildWhenTerminated { test_actor: test_actor.clone(), kid: None }));

  case.tell(&parent, "engage");
  case.system().run_until_idle();
  case.expect_str("green");
}

// "A Supervisor" must { "not be able to recreate child when old child is alive" in { ... } }
#[test]
fn not_be_able_to_recreate_child_when_old_child_is_alive() {
  let mut case = SupervisorMiscSpecCase::new();
  let test_actor = case.test_actor();
  let parent = case.actor_of(Props::new(move || RejectDuplicateChildName { test_actor: test_actor.clone() }));

  case.tell(&parent, "engage");
  case.system().run_until_idle();
  case.expect_str("green");
}

// "A Supervisor" must { "be able to create a similar kid in the fault handling strategy" in { ... } }
#[test]
fn be_able_to_create_a_similar_kid_in_the_fault_handling_strategy() {
  let mut case = SupervisorMiscSpecCase::new();
  let test_actor = case.test_actor();
  let parent = case.actor_of(Props::new(move || RecreateChildInFaultHandling { test_actor: test_actor.clone() }));

  case.tell(&parent, "engage");
  case.system().run_until_idle();
  // Original: `expectMsg("green")` — a value comparison, not a typed read.
  case.fraktor_spec.expect_msg_within(default_timeout(), "green");

  // Original: `EventFilter[IllegalStateException]("handleChildTerminated
  // failed", occurrences = 1).intercept { system.stop(parent) }`
  //
  // Stopping the parent stops its children, so the hook runs once more, now
  // that the parent is shutting down. Pekko wraps the refused child creation as
  // `IllegalStateException("handleChildTerminated failed")` and logs it once.
  // `intercept` lends the system to the body, so the body stops the parent
  // through the non-owning handle of the same system.
  let filter = EventFilter::typed("IllegalStateException").with_message("handleChildTerminated failed");
  let downgraded = case.fraktor_spec.system().downgrade();
  let system = case.fraktor_spec.system_mut();
  let target = parent.clone();
  EventFilter::intercept(&filter, system, move || {
    downgraded.stop(&target);
    downgraded.run_until_idle();
  });
}

// "A Supervisor" must { "have access to the failing child's reference in supervisorStrategy" in { ... } }
#[test]
fn have_access_to_the_failing_child_s_reference_in_supervisor_strategy() {
  let mut case = SupervisorMiscSpecCase::new();
  let test_actor = case.test_actor();
  let parent = case.actor_of(Props::new(move || FailingChildAsSender { test_actor: test_actor.clone() }));

  let filter = EventFilter::typed("ActorKilledException");
  let sender = case.self_actor_ref();
  let system = case.fraktor_spec.system_mut();
  let target = parent.clone();
  EventFilter::intercept(&filter, system, move || {
    target.tell("doit", sender);
  });

  let failing_child: ActorRef = case.fraktor_spec.expect_msg_type();
  let path: ActorPath = failing_child.path();
  assert_eq!(path.parent(), parent.path());
  assert_eq!(path.name(), "child");
}
