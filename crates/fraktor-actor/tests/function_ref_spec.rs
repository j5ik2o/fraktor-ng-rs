// SPDX-License-Identifier: Apache-2.0
//
// Licensed to the Apache Software Foundation (ASF) under one or more
// license agreements; and to You under the Apache License, version 2.0:
//
//   https://www.apache.org/licenses/LICENSE-2.0
//
// This file is part of the Apache Pekko project, which was derived from Akka.
//
// Copyright (C) 2016-2022 Lightbend Inc. <https://www.lightbend.com>
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.

// Ported from Pekko v2.0.0 (SHA 9fbd2283b1cf701c90a1223bf621942c191db61f):
//   actor-tests/src/test/scala/org/apache/pekko/actor/FunctionRefSpec.scala
//
// The original's `commonTests` define four cases that run twice, once for
// a function ref created by a toplevel actor and once for one created by
// a non-toplevel actor; the port keeps the eight executions as eight
// tests. The ninth case, "not be found" for a ref that was never
// registered, is excluded with the owner's approval (design
// issue-192-specs.md section 5, E3): the original checks, through a
// serialization round trip, that an unregistered FunctionRef resolved
// from a path does not come back as a FunctionRef, and this port has
// neither path resolution nor message serialization. The ledger
// records the exclusion with its approver and date. The tests after
// the eight ported cases are regression tests from the review of
// this port, not ported cases.

use std::sync::Arc;
use std::time::Duration;

use fraktor_actor::adaptor::{ChannelBoundary, UnwindCatcher};
use fraktor_actor::kernel::actor::{Actor, ActorContext, DeadLetter, FunctionRef, Props, Terminated};
use fraktor_actor::kernel::actor_ref::ActorRef;
use fraktor_actor::kernel::actor_system::ActorSystem;
use fraktor_actor::kernel::event::EventClass;
use fraktor_actor::kernel::message::Message;
use fraktor_testkit::test_kit::TestKit;

// Original: `GetForwarder(replyTo)`.
#[derive(Clone, Debug)]
struct GetForwarder(ActorRef);

// Original: `DropForwarder(ref)`.
#[derive(Clone, Debug)]
struct DropForwarder(FunctionRef);

// Original: `Forwarded(msg, sender)`.
#[derive(Clone, Debug)]
struct Forwarded(Message, ActorRef);

// Original: `Super`. Answers `GetForwarder` by registering a forwarding
// function ref on its own cell and replying with it; answers
// `DropForwarder` by removing the ref.
struct Super;

impl Actor for Super {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    if let Some(get) = message.downcast_ref::<GetForwarder>() {
      let reply_to = get.0.clone();
      let target = reply_to.clone();
      let fref = context.add_function_ref(move |sender, msg| {
        target.tell(Forwarded(msg, sender.clone()), sender.clone());
      });
      context.tell(&reply_to, fref);
    } else if let Some(drop) = message.downcast_ref::<DropForwarder>() {
      context.remove_function_ref(&drop.0);
    }
  }
}

// Original: `SupSuper`. Holds a `Super` child and forwards everything.
struct SupSuper {
  child: Option<ActorRef>,
}

impl Actor for SupSuper {
  fn pre_start(&mut self, context: &mut ActorContext<'_>) {
    let child = context.actor_of(Props::new(|| Super), "super").expect("the child name is free");
    self.child = Some(child);
  }

  fn receive(&mut self, _context: &mut ActorContext<'_>, message: Message) {
    let child = self.child.clone().expect("the child was started in pre_start");
    child.forward(message, _context);
  }
}

fn kit() -> TestKit {
  let system =
    ActorSystem::with_failure_catcher("FunctionRefSpec", Arc::new(ChannelBoundary::new()), Arc::new(UnwindCatcher));
  TestKit::new(system)
}

fn toplevel(kit: &mut TestKit) -> ActorRef {
  kit.system_mut().actor_of(Props::new(|| Super))
}

fn non_toplevel(kit: &mut TestKit) -> ActorRef {
  kit.system_mut().actor_of(Props::new(|| SupSuper { child: None }))
}

fn get_forwarder(kit: &mut TestKit, s: &ActorRef) -> FunctionRef {
  let test_actor = kit.test_actor();
  s.tell(GetForwarder(test_actor.clone()), test_actor);
  kit.expect_msg_type::<FunctionRef>()
}

fn expect_forwarded_text(kit: &mut TestKit, expected: &'static str, sender: &ActorRef) {
  let forwarded = kit.expect_msg_type::<Forwarded>();
  assert_eq!(forwarded.0.downcast_ref::<&'static str>().copied(), Some(expected));
  assert_eq!(&forwarded.1, sender);
}

fn expect_terminated(kit: &mut TestKit, target: &FunctionRef) {
  let notice: Terminated = kit.expect_msg_terminated(Duration::from_secs(3));
  assert_eq!(notice.target(), &target.as_actor_ref());
}

// Original: "forward messages" (toplevel)
#[test]
fn function_ref_created_by_a_toplevel_actor_forwards_messages() {
  let mut kit = kit();
  let s = toplevel(&mut kit);
  let f = get_forwarder(&mut kit, &s);
  let test_actor = kit.test_actor();
  f.tell("hello", test_actor.clone());
  expect_forwarded_text(&mut kit, "hello", &test_actor);
}

// Original: "be watchable" (toplevel)
#[test]
fn function_ref_created_by_a_toplevel_actor_is_watchable() {
  let mut kit = kit();
  let s = toplevel(&mut kit);
  let f = get_forwarder(&mut kit, &s);
  kit.watch(&f.as_actor_ref());
  let test_actor = kit.test_actor();
  s.tell(DropForwarder(f.clone()), test_actor);
  expect_terminated(&mut kit, &f);
}

// Original: "be able to watch" (toplevel)
#[test]
fn function_ref_created_by_a_toplevel_actor_can_watch() {
  let mut kit = kit();
  let s = toplevel(&mut kit);
  let forwarder = get_forwarder(&mut kit, &s);
  let f = get_forwarder(&mut kit, &s);
  kit.system_mut().function_ref_watch(&forwarder, &f.as_actor_ref());
  assert!(kit.system().function_ref_is_watching(&forwarder, &f.as_actor_ref()));
  let test_actor = kit.test_actor();
  s.tell(DropForwarder(f.clone()), test_actor.clone());
  let forwarded = kit.expect_msg_type::<Forwarded>();
  let notice = forwarded.0.downcast_ref::<Terminated>().expect("the forwarded message is a Terminated");
  assert_eq!(notice.target(), &f.as_actor_ref());
  assert!(notice.existence_confirmed());
  assert!(!notice.address_terminated());
  assert_eq!(forwarded.1, f.as_actor_ref());
  // Upon receiving the Terminated message, unwatch() must be called,
  // which is different from an ordinary actor.
  assert!(kit.system().function_ref_is_watching(&forwarder, &f.as_actor_ref()));
  kit.system_mut().function_ref_unwatch(&forwarder, &f.as_actor_ref());
  assert!(!kit.system().function_ref_is_watching(&forwarder, &f.as_actor_ref()));
}

// Original: "terminate when their parent terminates" (toplevel)
#[test]
fn function_ref_created_by_a_toplevel_actor_terminates_when_its_parent_terminates() {
  let mut kit = kit();
  let s = toplevel(&mut kit);
  let forwarder = get_forwarder(&mut kit, &s);
  kit.watch(&forwarder.as_actor_ref());
  let test_actor = kit.test_actor();
  s.tell("PoisonPill", test_actor);
  expect_terminated(&mut kit, &forwarder);
}

// Original: "forward messages" (non-toplevel)
#[test]
fn function_ref_created_by_a_non_toplevel_actor_forwards_messages() {
  let mut kit = kit();
  let s = non_toplevel(&mut kit);
  let f = get_forwarder(&mut kit, &s);
  let test_actor = kit.test_actor();
  f.tell("hello", test_actor.clone());
  expect_forwarded_text(&mut kit, "hello", &test_actor);
}

// Original: "be watchable" (non-toplevel)
#[test]
fn function_ref_created_by_a_non_toplevel_actor_is_watchable() {
  let mut kit = kit();
  let s = non_toplevel(&mut kit);
  let f = get_forwarder(&mut kit, &s);
  kit.watch(&f.as_actor_ref());
  let test_actor = kit.test_actor();
  s.tell(DropForwarder(f.clone()), test_actor);
  expect_terminated(&mut kit, &f);
}

// Original: "be able to watch" (non-toplevel)
#[test]
fn function_ref_created_by_a_non_toplevel_actor_can_watch() {
  let mut kit = kit();
  let s = non_toplevel(&mut kit);
  let forwarder = get_forwarder(&mut kit, &s);
  let f = get_forwarder(&mut kit, &s);
  kit.system_mut().function_ref_watch(&forwarder, &f.as_actor_ref());
  assert!(kit.system().function_ref_is_watching(&forwarder, &f.as_actor_ref()));
  let test_actor = kit.test_actor();
  s.tell(DropForwarder(f.clone()), test_actor.clone());
  let forwarded = kit.expect_msg_type::<Forwarded>();
  let notice = forwarded.0.downcast_ref::<Terminated>().expect("the forwarded message is a Terminated");
  assert_eq!(notice.target(), &f.as_actor_ref());
  assert!(notice.existence_confirmed());
  assert!(!notice.address_terminated());
  assert_eq!(forwarded.1, f.as_actor_ref());
  assert!(kit.system().function_ref_is_watching(&forwarder, &f.as_actor_ref()));
  kit.system_mut().function_ref_unwatch(&forwarder, &f.as_actor_ref());
  assert!(!kit.system().function_ref_is_watching(&forwarder, &f.as_actor_ref()));
}

// Original: "terminate when their parent terminates" (non-toplevel)
#[test]
fn function_ref_created_by_a_non_toplevel_actor_terminates_when_its_parent_terminates() {
  let mut kit = kit();
  let s = non_toplevel(&mut kit);
  let forwarder = get_forwarder(&mut kit, &s);
  kit.watch(&forwarder.as_actor_ref());
  let test_actor = kit.test_actor();
  s.tell("PoisonPill", test_actor);
  expect_terminated(&mut kit, &forwarder);
}

// Regression (review of #192): a function ref created from a
// post_stop hook ends with its parent instead of staying live.
struct PostStopCreator {
  probe: ActorRef,
  created: std::sync::Arc<std::sync::Mutex<Option<FunctionRef>>>,
}

impl Actor for PostStopCreator {
  fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {}

  fn post_stop_with_context(&mut self, context: &mut ActorContext<'_>) {
    let probe = self.probe.clone();
    let target = probe.clone();
    let fref = context.add_function_ref(move |sender, msg| {
      target.tell(Forwarded(msg, sender.clone()), sender.clone());
    });
    *self.created.lock().expect("created slot") = Some(fref.clone());
    context.tell(&probe, fref);
  }
}

#[test]
fn function_ref_created_in_post_stop_ends_with_its_parent() {
  let mut kit = kit();
  let test_actor = kit.test_actor();
  kit.system().event_stream().subscribe(Some(test_actor.clone()), EventClass::of::<DeadLetter>()).expect("subscribe");
  let created = std::sync::Arc::new(std::sync::Mutex::new(None));
  let actor = kit.system_mut().actor_of(Props::new({
    let probe = test_actor.clone();
    let created = created.clone();
    move || PostStopCreator { probe: probe.clone(), created: created.clone() }
  }));
  actor.tell("PoisonPill", test_actor.clone());
  let fref = kit.expect_msg_type::<FunctionRef>();
  assert!(created.lock().expect("created slot").is_some(), "the hook recorded the ref it created");
  fref.tell("hello", test_actor.clone());
  let dead = kit.expect_msg_type::<DeadLetter>();
  assert_eq!(dead.message().downcast_ref::<&'static str>().copied(), Some("hello"));
  assert!(dead.recipient() == &fref.as_actor_ref());
  kit.expect_no_message(Duration::from_millis(300));
}

// Regression (review of #192): registering the same pair through
// both watch tables delivers the notice once, as the original's
// single DeathWatch set does.
#[test]
fn function_ref_watched_through_both_tables_is_told_once() {
  let mut kit = kit();
  let s = toplevel(&mut kit);
  let forwarder = get_forwarder(&mut kit, &s);
  let target = get_forwarder(&mut kit, &s);
  kit.system_mut().watch(&forwarder.as_actor_ref(), &target.as_actor_ref());
  kit.system_mut().function_ref_watch(&forwarder, &target.as_actor_ref());
  let test_actor = kit.test_actor();
  s.tell(DropForwarder(target.clone()), test_actor);
  let forwarded = kit.expect_msg_type::<Forwarded>();
  let notice = forwarded.0.downcast_ref::<Terminated>().expect("the forwarded message is a Terminated");
  assert_eq!(notice.target(), &target.as_actor_ref());
  kit.expect_no_message(Duration::from_millis(300));
}

// Regression (review of #192): a removed function ref subscribed to
// dead letters must not receive its own dead letter again and again.
#[test]
fn removed_function_ref_subscribed_to_dead_letters_does_not_loop() {
  let mut kit = kit();
  let s = toplevel(&mut kit);
  let f = get_forwarder(&mut kit, &s);
  let test_actor = kit.test_actor();
  kit.system().event_stream().subscribe(Some(f.as_actor_ref()), EventClass::of::<DeadLetter>()).expect("subscribe");
  kit.system().event_stream().subscribe(Some(test_actor.clone()), EventClass::of::<DeadLetter>()).expect("subscribe");
  kit.watch(&f.as_actor_ref());
  s.tell(DropForwarder(f.clone()), test_actor.clone());
  expect_terminated(&mut kit, &f);
  f.tell("hello", test_actor.clone());
  let dead = kit.expect_msg_type::<DeadLetter>();
  assert_eq!(dead.message().downcast_ref::<&'static str>().copied(), Some("hello"));
  assert!(dead.recipient() == &f.as_actor_ref());
  kit.expect_no_message(Duration::from_millis(500));
}
