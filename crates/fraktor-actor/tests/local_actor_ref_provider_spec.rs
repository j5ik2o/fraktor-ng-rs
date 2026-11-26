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
//   actor-tests/src/test/scala/org/apache/pekko/actor/LocalActorRefProviderSpec.scala
//
// Differences from the original, recorded as the design of issue #194
// (docs/design/issue-194-specs.md) decides:
// - The concurrent case ("only create one instance of an actor with a
//   specific address in a concurrent environment") is not in this file.
//   The marshal decided on 2026-11-26 that it waits for stage 5 of #234
//   (issue #256) and is ported under issue #265. The ledger keeps this
//   node at 8 total / 7 ported / 7 passing, unported.
// - Case 1 resolves from the test side, not from inside `receive`: the
//   system's executor sits behind a `RefCell`, so resolving while a
//   delivery borrows it would panic.
// - Case 5 is checked by shape: a stopped cell is removed from the
//   executor's table and dropped, so there is no cell whose `Props`
//   could be read afterwards. The test holds a `Weak` to what the
//   factory captured and checks that it can no longer be upgraded.
// - Case 7 reads the second creation's `Result` from the actor instead
//   of intercepting a thrown exception, because the port's `actor_of`
//   returns the refusal.
// - The name `null` sub-case of case 8 is not built: Rust's `&str`
//   cannot hold null, so the type refuses it. Whether to record this
//   as not built is waiting for the owner's decision; nothing in this
//   file or in the ledger claims it was approved.
// - The warning text differs from Pekko's: this port writes
//   `unhandled message from {path}: {message:?}` where Pekko writes
//   `unhandled message from Actor[{path}]: {message}`.

use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::sync::Weak;

use fraktor_actor::kernel::actor::{Actor, ActorContext, Props};
use fraktor_actor::kernel::actor_path::ActorPath;
use fraktor_actor::kernel::actor_ref::ActorRef;
use fraktor_actor::kernel::message::Message;
use fraktor_testkit::event_filter::EventFilter;
use fraktor_testkit::fraktorspec::FraktorSpec;
use fraktor_testkit::test_kit::TestProbe;

const SPEC_SYSTEM_NAME: &str = "LocalActorRefProviderSpec";

fn sender(spec: &FraktorSpec) -> ActorRef {
  spec.system().dead_letters()
}

struct ChildMaker {
  child_name: String,
  reply_to: ActorRef,
}

impl Actor for ChildMaker {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    if message.downcast_ref::<&'static str>() == Some(&"make") {
      let child = context.actor_of(Props::empty(), &self.child_name).expect("the child is created");
      let reply_to = self.reply_to.clone();
      reply_to.tell(child, context.self_ref());
    }
  }
}

// Original: "find child actor with URL encoded name"
#[test]
fn finds_a_child_actor_with_a_url_encoded_name() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let mut probe = TestProbe::new(spec.system_mut());
  let probe_ref = probe.ref_handle();
  let name = "pekko%3A%2F%2FClusterSystem%40127.0.0.1%3A7355";
  let maker = spec
    .system_mut()
    .actor_of(Props::new(move || ChildMaker { child_name: name.to_string(), reply_to: probe_ref.clone() }));

  maker.tell("make", sender(&spec));
  let child: ActorRef = probe.expect_msg_type();

  let resolved = spec.system().resolve_actor_ref(&child.path());
  assert_eq!(resolved, child);
  assert!(!spec.system().is_actor_terminated(&resolved));
  assert_eq!(resolved.path().name(), name);
  spec.terminate();
}

fn guardian_reports_unhandled(spec: &mut FraktorSpec, elements: &[&str]) {
  let dead_letters = spec.system().dead_letters();
  let mut path = ActorPath::from_string(&format!("pekko://{SPEC_SYSTEM_NAME}")).expect("the root path parses");
  for element in elements {
    path = path.with_child(element);
  }
  let filter = EventFilter::warning().with_message(&format!("unhandled message from {}", dead_letters.path()));
  let selection = spec.system().actor_selection(&path);
  let from = dead_letters.clone();
  EventFilter::intercept(&filter, spec.system_mut(), move || {
    selection.tell("hello", from);
  });
}

// Original: "not handle messages other than those it will act upon" (root)
#[test]
fn root_guardian_reports_messages_it_does_not_handle_as_unhandled() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  guardian_reports_unhandled(&mut spec, &[]);
  spec.terminate();
}

// Original: "not handle messages other than those it will act upon" (user)
#[test]
fn user_guardian_reports_messages_it_does_not_handle_as_unhandled() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  guardian_reports_unhandled(&mut spec, &["user"]);
  spec.terminate();
}

// Original: "not handle messages other than those it will act upon" (system)
#[test]
fn system_guardian_reports_messages_it_does_not_handle_as_unhandled() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  guardian_reports_unhandled(&mut spec, &["system"]);
  spec.terminate();
}

// Original: "not retain its original Props when terminated"
#[test]
fn does_not_retain_the_original_props_when_terminated() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let captured = Arc::new(AtomicBool::new(true));
  let weak: Weak<AtomicBool> = Arc::downgrade(&captured);
  let in_factory = captured.clone();
  let child = spec.system_mut().actor_of(Props::new(move || {
    let _held = in_factory.clone();
    struct HoldingActor {
      _held: Arc<AtomicBool>,
    }
    impl Actor for HoldingActor {
      fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {}
    }
    HoldingActor { _held }
  }));
  // The factory has run and the actor holds the value.
  assert!(weak.upgrade().is_some());
  drop(captured);

  spec.system_mut().stop(&child);
  // The port of `awaitAssert`: drive the system until the value is
  // released, within the same three seconds the original allows.
  let mut released = false;
  for _ in 0..300 {
    spec.system_mut().run_until_idle();
    if weak.upgrade().is_none() {
      released = true;
      break;
    }
    std::thread::sleep(core::time::Duration::from_millis(10));
  }
  assert!(released, "the stopped actor still holds its original Props' value");
  spec.terminate();
}

struct DuplicateMaker {
  reply_to: ActorRef,
}

impl Actor for DuplicateMaker {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    if message.downcast_ref::<&'static str>() == Some(&"go") {
      let first = context.actor_of(Props::empty(), "duplicate");
      let second = context.actor_of(Props::empty(), "duplicate");
      let reply_to = self.reply_to.clone();
      let report = match (first, second) {
        | (Ok(child), Err(error)) => format!("one|{}|{}", child.path(), error),
        | (Ok(_), Ok(_)) => "two".to_string(),
        | (Err(error), _) => format!("first-failed:{error}"),
      };
      reply_to.tell(report, context.self_ref());
    }
  }
}

// Original: "only create one instance of an actor from within the same
// message invocation"
#[test]
fn creates_only_one_instance_from_within_the_same_message_invocation() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let mut probe = TestProbe::new(spec.system_mut());
  let probe_ref = probe.ref_handle();
  let maker = spec.system_mut().actor_of(Props::new(move || DuplicateMaker { reply_to: probe_ref.clone() }));

  maker.tell("go", sender(&spec));
  let report: String = probe.expect_msg_type();
  assert!(report.starts_with("one|"), "unexpected report: {report}");
  assert!(report.contains("Actor name [duplicate] is not unique!"), "unexpected report: {report}");

  // Only one child remains: resolving its path returns the first child.
  let child_path_text = report.split('|').nth(1).expect("the report carries the child path").to_string();
  let child_path = ActorPath::from_string(&child_path_text).expect("the child path parses");
  let resolved = spec.system().resolve_actor_ref(&child_path);
  assert_eq!(resolved.path(), child_path);
  assert!(!spec.system().is_actor_terminated(&resolved));
  spec.terminate();
}

// Original: "throw suitable exceptions for malformed actor names"
#[test]
fn rejects_malformed_actor_names_with_suitable_messages() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let system = spec.system_mut();

  let empty = system.actor_of_named(Props::empty(), "").expect_err("an empty name is refused");
  assert!(empty.to_string().contains("actor name must not be empty"), "unexpected message: {empty}");

  // Each entry: the name, and the fragments the single validation
  // message of `ActorPath::validate_path_element` must carry.
  let cases: Vec<(&str, Vec<&str>)> = vec![
    ("$hallo", vec!["illegal character [$] at position: 0", "not start with `$`"]),
    ("$hello", vec!["illegal character [$] at position: 0"]),
    ("a%", vec!["illegal character [%] at position: 1"]),
    ("%3", vec!["illegal character [%] at position: 0"]),
    ("%xx", vec!["illegal character [%] at position: 0"]),
    ("%0G", vec!["illegal character [%] at position: 0"]),
    ("%gg", vec!["illegal character [%] at position: 0"]),
    ("%", vec!["illegal character [%] at position: 0"]),
    ("%1t", vec!["illegal character [%] at position: 0"]),
    ("a?", vec!["illegal character [?] at position: 1"]),
    ("üß", vec!["illegal character [ü] at position: 0", "include only ASCII"]),
  ];
  for (name, fragments) in cases {
    let error = system.actor_of_named(Props::empty(), name).expect_err("a malformed name is refused");
    let text = error.to_string();
    for fragment in fragments {
      assert!(text.contains(fragment), "name {name:?}: message {text:?} does not contain {fragment:?}");
    }
  }

  // A well-formed name is accepted, and taking it twice is refused.
  let created = system.actor_of_named(Props::empty(), "valid-name").expect("a valid name is accepted");
  assert_eq!(created.path().name(), "valid-name");
  let taken = system.actor_of_named(Props::empty(), "valid-name").expect_err("a taken name is refused");
  assert!(taken.to_string().contains("Actor name [valid-name] is not unique!"), "unexpected message: {taken}");
  spec.terminate();
}
