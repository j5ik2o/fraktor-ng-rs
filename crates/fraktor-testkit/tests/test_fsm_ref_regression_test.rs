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

//! Behaviour contracts of `TestFsmRef` that the ported spec does not
//! cover: a named timer must not invalidate an armed state timer, the
//! user guardian must be acceptable as an explicit supervisor, and a
//! machine that fails to start must not hand out a reference.

use core::time::Duration;

use fraktor_actor::kernel::actor::{ActorContext, Fsm, FsmState, FsmView, StateTimeout};
use fraktor_actor::kernel::actor_path::ActorPath;
use fraktor_actor::kernel::actor_ref::ActorRef;
use fraktor_actor::kernel::message::Message;
use fraktor_testkit::fraktorspec::FraktorSpec;
use fraktor_testkit::test_fsm_ref::TestFsmRef;

const SPEC_SYSTEM_NAME: &str = "TestFSMRefRegression";

/// How long a case waits for the machine to process what it was sent.
const SETTLE_MAX: Duration = Duration::from_secs(3);

/// Waits until the machine has processed everything sent to it so far.
fn settle<S, D>(spec: &mut FraktorSpec, fsm: &TestFsmRef<S, D>)
where
  S: Clone + PartialEq + Send + Sync + 'static,
  D: Clone + Send + 'static,
{
  let sentinel = fsm.send_sentinel();
  spec.await_cond(|| fsm.processed_sequence() >= sentinel, SETTLE_MAX);
}

fn guardian(spec: &FraktorSpec) -> ActorRef {
  let path =
    ActorPath::from_string(&format!("pekko://{SPEC_SYSTEM_NAME}")).expect("the root path parses").with_child("user");
  spec.system().resolve_actor_ref(&path)
}

fn timer_fsm() -> Fsm<i32, ()> {
  let mut fsm = Fsm::new();
  fsm.start_with(1, ());
  fsm.when(1, |_context: &mut ActorContext<'_>, _view: &FsmView<i32>, _event: &Message, _data: &()| {
    Some(FsmState::stay())
  });
  fsm
}

fn timeout_fsm() -> Fsm<i32, String> {
  let mut fsm = Fsm::new();
  fsm.start_with(1, String::new());
  fsm.when(1, |_context: &mut ActorContext<'_>, _view: &FsmView<i32>, event: &Message, _data: &String| {
    if event.is::<StateTimeout>() {
      return Some(FsmState::goto(2).using("timeout".to_string()));
    }
    Some(FsmState::stay())
  });
  fsm.when(2, |_context: &mut ActorContext<'_>, _view: &FsmView<i32>, _event: &Message, _data: &String| {
    Some(FsmState::stay())
  });
  fsm
}

fn failing_fsm() -> Fsm<i32, ()> {
  let mut fsm = Fsm::new();
  fsm.start_with(1, ());
  fsm.when(1, |_context: &mut ActorContext<'_>, _view: &FsmView<i32>, _event: &Message, _data: &()| {
    Some(FsmState::stay())
  });
  fsm.on_transition(|_context: &mut ActorContext<'_>, _from: &i32, _to: &i32| {
    panic!("the initial transition fails");
  });
  fsm.initialize();
  fsm
}

#[test]
fn state_timeout_still_fires_after_a_named_timer_starts() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let fsm =
    TestFsmRef::with_name(spec.system_mut(), timeout_fsm, "regression-state-timeout").expect("the name is free");
  fsm.set_state(None, None, Some(Duration::from_millis(150)));
  settle(&mut spec, &fsm);
  assert!(fsm.is_state_timer_active(), "the state timer is not armed");
  fsm.start_single_timer("named", Message::new("tick"), Duration::from_secs(60));
  settle(&mut spec, &fsm);
  assert!(fsm.is_state_timer_active(), "starting a named timer disarmed the state timer");
  spec.await_cond(|| fsm.state_name() == 2, SETTLE_MAX);
  assert_eq!(fsm.state_data(), "timeout");
  spec.terminate();
}

#[test]
fn creation_with_the_user_guardian_as_supervisor() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let guardian = guardian(&spec);
  let fsm = TestFsmRef::with_supervisor(spec.system_mut(), timer_fsm, &guardian);
  settle(&mut spec, &fsm);
  assert_eq!(fsm.state_name(), 1);
  assert_eq!(fsm.supervisor(), guardian);
  assert_eq!(fsm.actor_ref().path().parent(), guardian.path());
  spec.terminate();
}

#[test]
fn creation_fails_when_the_machine_fails_to_start() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
    let _reference = TestFsmRef::with_name(spec.system_mut(), failing_fsm, "regression-failing-start");
  }));
  assert!(outcome.is_err(), "a machine that fails to start handed out a reference");
  spec.terminate();
}
