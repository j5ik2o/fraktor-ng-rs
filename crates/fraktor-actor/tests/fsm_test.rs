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

// Regression cases for `Fsm`.
//
// These are not ported from a Pekko test file and are not counted in the
// ledger. They pin parts of Pekko's `FSM` contract that the two ported cases
// (AbstractFSMActorTest and AbstractFSMWithStashActorTest) do not reach:
//
// - `stay().using(data)` replaces the data without telling the transition
//   handlers, and `goto` into the current state tells them.
// - A `goto` into a state with no state function keeps the machine running in
//   its current state: Pekko's `makeTransition` records the failure through
//   `terminate` but does not stop the actor.

use fraktor_actor::kernel::actor::{ActorContext, Fsm, FsmState, Props};
use fraktor_actor::kernel::actor_ref::ActorRef;
use fraktor_actor::kernel::message::Message;
use fraktor_testkit::fraktorspec::FraktorSpec;
use fraktor_testkit::test_kit::TestProbe;

const SPEC_SYSTEM_NAME: &str = "FsmTest";

// In `start`: `stay` adds one to the data, `same` reports the data and goes to
// `start` again, `nowhere` goes to a state with no state function, and `next`
// goes to `next`. Every transition is reported to the probe.
fn counting_fsm(probe: ActorRef) -> Fsm<&'static str, u32> {
  let mut fsm = Fsm::new();
  let notices = probe.clone();
  fsm.on_transition(move |context: &mut ActorContext<'_>, from: &&'static str, to: &&'static str| {
    notices.tell(format!("{from}->{to}"), context.self_ref());
  });
  fsm.start_with("start", 0);
  fsm.when("start", move |context: &mut ActorContext<'_>, _view: &fraktor_actor::kernel::actor::FsmView<&'static str>, event: &Message, data: &u32| {
    match *event.downcast_ref::<&'static str>()? {
      | "stay" => Some(FsmState::stay().using(data + 1)),
      | "same" => {
        probe.tell(format!("data={data}"), context.self_ref());
        Some(FsmState::goto("start"))
      },
      | "nowhere" => Some(FsmState::goto("nowhere")),
      | "next" => Some(FsmState::goto("next")),
      | _ => None,
    }
  });
  fsm.when("next", |_context: &mut ActorContext<'_>, _view: &fraktor_actor::kernel::actor::FsmView<&'static str>, _event: &Message, _data: &u32| None);
  fsm.initialize();
  fsm
}

fn expect_text(probe: &mut TestProbe, expected: &str) {
  let received: String = probe.expect_msg_type();
  assert_eq!(received, expected);
}

#[test]
fn stay_using_replaces_the_data_silently_and_goto_the_same_state_notifies() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let mut probe = TestProbe::new(spec.system_mut());
  let probe_ref = probe.ref_handle();
  let fsm = spec.system_mut().actor_of(Props::new(move || counting_fsm(probe_ref.clone())));
  expect_text(&mut probe, "start->start");
  let sender = spec.system().dead_letters();

  fsm.tell("stay", sender.clone());
  fsm.tell("same", sender);

  expect_text(&mut probe, "data=1");
  expect_text(&mut probe, "start->start");
  spec.terminate();
}

#[test]
fn a_goto_into_an_unknown_state_keeps_the_machine_running_in_its_state() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let mut probe = TestProbe::new(spec.system_mut());
  let probe_ref = probe.ref_handle();
  let fsm = spec.system_mut().actor_of(Props::new(move || counting_fsm(probe_ref.clone())));
  expect_text(&mut probe, "start->start");
  let sender = spec.system().dead_letters();

  fsm.tell("nowhere", sender.clone());
  fsm.tell("next", sender);

  expect_text(&mut probe, "start->next");
  spec.terminate();
}
