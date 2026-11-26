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
//   actor-tests/src/test/scala/org/apache/pekko/actor/FSMTransitionSpec.scala
//
// Differences from the original, recorded as the design of issue #194
// (docs/design/issue-194-specs.md) decides:
// - The original's actors subclass `FSM`; the port builds the same machines
//   as `Fsm` values inside the `Props` factory, which is where the original's
//   constructor calls (`startWith`, `when`, `onTransition`, `initialize`)
//   land.
// - Case 8 ("not leak memory in nextState") is checked by shape: the next
//   data exists only as the argument of `on_transition_with_data`, and the
//   test counts the strong references of the `Arc` it handed over instead of
//   calling a `nextStateData` that throws outside a transition. No public
//   API panics for this.
// - The warning text for an unhandled message differs from Pekko's
//   (`unhandled message from {path}: {message:?}` in this port); no case
//   here reads it.

use std::sync::atomic::AtomicI32;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::sync::Mutex;

use fraktor_actor::kernel::actor::{
  Actor, ActorContext, CurrentState, Fsm, FsmState, FsmView, Props, SubscribeTransitionCallBack, Transition,
};
use fraktor_actor::kernel::actor_ref::ActorRef;
use fraktor_actor::kernel::message::Message;
use fraktor_testkit::fraktorspec::FraktorSpec;
use fraktor_testkit::test_kit::TestProbe;

const SPEC_SYSTEM_NAME: &str = "FSMTransitionSpec";

fn sender(spec: &FraktorSpec) -> ActorRef {
  spec.system().dead_letters()
}

// Original: `SendAnyTransitionFSM`, whose transitions all reach the test
// actor. States and data are both `Int` in the original.
fn send_any_transition_fsm(probe: ActorRef) -> Fsm<i32, i32> {
  let mut fsm = Fsm::new();
  let notices = probe.clone();
  fsm.on_transition(move |context: &mut ActorContext<'_>, from: &i32, to: &i32| {
    notices.tell((*from, *to), context.self_ref());
  });
  fsm.start_with(0, 0);
  fsm.when(0, |_context: &mut ActorContext<'_>, _view: &FsmView<i32>, event: &Message, _data: &i32| {
    match *event.downcast_ref::<&'static str>()? {
      | "stay" => Some(FsmState::stay()),
      | "goto-self" => Some(FsmState::goto(0)),
      | _ => None,
    }
  });
  fsm.when(1, |_context: &mut ActorContext<'_>, _view: &FsmView<i32>, _event: &Message, _data: &i32| {
    Some(FsmState::stay())
  });
  fsm.initialize();
  fsm
}

// Original: `MyFSM`, which toggles between 0 and 1 on "tick" and answers
// the size of its listener set from `whenUnhandled`.
fn my_fsm(listener_count: Arc<AtomicI32>) -> Fsm<i32, i32> {
  let mut fsm = Fsm::new();
  fsm.start_with(0, 0);
  fsm.when(0, |_context: &mut ActorContext<'_>, _view: &FsmView<i32>, event: &Message, _data: &i32| {
    match *event.downcast_ref::<&'static str>()? {
      | "tick" => Some(FsmState::goto(1)),
      | _ => None,
    }
  });
  fsm.when(1, |_context: &mut ActorContext<'_>, _view: &FsmView<i32>, event: &Message, _data: &i32| {
    match *event.downcast_ref::<&'static str>()? {
      | "tick" => Some(FsmState::goto(0)),
      | "goto-self" => Some(FsmState::goto(1)),
      | _ => None,
    }
  });
  fsm.when_unhandled(move |_context: &mut ActorContext<'_>, view: &FsmView<i32>, _event: &Message, _data: &i32| {
    let count = view.listener_count() as i32;
    listener_count.store(count, Ordering::SeqCst);
    Some(FsmState::stay().replying(count))
  });
  fsm.initialize();
  fsm
}

// Original: `OtherFSM`, which reports the previous and next state data
// from inside `onTransition`.
fn other_fsm(probe: ActorRef) -> Fsm<i32, i32> {
  let mut fsm = Fsm::new();
  let notices = probe.clone();
  fsm.on_transition_with_data(
    move |context: &mut ActorContext<'_>, _from: &i32, _to: &i32, previous: &i32, next: &i32| {
      notices.tell((*previous, *next), context.self_ref());
    },
  );
  fsm.start_with(0, 0);
  fsm.when(0, |_context: &mut ActorContext<'_>, _view: &FsmView<i32>, event: &Message, _data: &i32| {
    match *event.downcast_ref::<&'static str>()? {
      | "tick" => Some(FsmState::goto(1).using(1)),
      | _ => None,
    }
  });
  fsm.when(1, |_context: &mut ActorContext<'_>, _view: &FsmView<i32>, _event: &Message, _data: &i32| {
    Some(FsmState::stay())
  });
  fsm
}

struct Forwarder {
  target: ActorRef,
}

impl Actor for Forwarder {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    let sender = context.sender();
    self.target.tell(message, sender);
  }
}

// Original: "not trigger onTransition for stay"
#[test]
fn does_not_trigger_on_transition_for_stay() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let mut probe = TestProbe::new(spec.system_mut());
  let probe_ref = probe.ref_handle();
  let fsm = spec.system_mut().actor_of(Props::new(move || send_any_transition_fsm(probe_ref.clone())));
  // `initialize` reports the move into the initial state.
  let initial: (i32, i32) = probe.expect_msg_type();
  assert_eq!(initial, (0, 0));

  fsm.tell("stay", sender(&spec));
  probe.expect_no_message(core::time::Duration::from_millis(100));

  fsm.tell("goto-self", sender(&spec));
  let again: (i32, i32) = probe.expect_msg_type();
  assert_eq!(again, (0, 0));
  spec.terminate();
}

// Original: "notify listeners"
#[test]
fn notifies_transition_listeners() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let mut probe = TestProbe::new(spec.system_mut());
  let probe_ref = probe.ref_handle();
  let counter = Arc::new(AtomicI32::new(-1));
  let shared = counter.clone();
  let fsm = spec.system_mut().actor_of(Props::new(move || my_fsm(shared.clone())));

  fsm.tell(SubscribeTransitionCallBack(probe_ref.clone()), sender(&spec));
  let current: CurrentState<i32> = probe.expect_msg_type();
  assert_eq!(current, CurrentState { fsm: fsm.clone(), state: 0 });

  fsm.tell("tick", sender(&spec));
  let first: Transition<i32> = probe.expect_msg_type();
  assert_eq!(first, Transition { fsm: fsm.clone(), from: 0, to: 1 });

  fsm.tell("tick", sender(&spec));
  let second: Transition<i32> = probe.expect_msg_type();
  assert_eq!(second, Transition { fsm: fsm.clone(), from: 1, to: 0 });
  spec.terminate();
}

// Original: "not fail when listener goes away"
#[test]
fn does_not_fail_when_a_listener_goes_away() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let mut probe = TestProbe::new(spec.system_mut());
  let probe_ref = probe.ref_handle();
  let counter = Arc::new(AtomicI32::new(-1));
  let shared = counter.clone();
  let fsm = spec.system_mut().actor_of(Props::new(move || my_fsm(shared.clone())));
  let forward_target = probe_ref.clone();
  let listener = spec.system_mut().actor_of(Props::new(move || Forwarder { target: forward_target.clone() }));

  fsm.tell(SubscribeTransitionCallBack(listener.clone()), sender(&spec));
  let current: CurrentState<i32> = probe.expect_msg_type();
  assert_eq!(current.state, 0);

  spec.system_mut().stop(&listener);
  fsm.tell("tick", sender(&spec));
  fsm.tell("tick", sender(&spec));
  probe.expect_no_message(core::time::Duration::from_millis(100));

  // The machine kept running and toggled twice, so it is back in state 0.
  fsm.tell(SubscribeTransitionCallBack(probe_ref.clone()), sender(&spec));
  let after: CurrentState<i32> = probe.expect_msg_type();
  assert_eq!(after.state, 0);
  spec.terminate();
}

// Original: "remove stopped listeners"
#[test]
fn removes_stopped_listeners() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let mut probe = TestProbe::new(spec.system_mut());
  let probe_ref = probe.ref_handle();
  let counter = Arc::new(AtomicI32::new(-1));
  let shared = counter.clone();
  let fsm = spec.system_mut().actor_of(Props::new(move || my_fsm(shared.clone())));
  let forward_target = probe_ref.clone();
  let listener = spec.system_mut().actor_of(Props::new(move || Forwarder { target: forward_target.clone() }));

  fsm.tell(SubscribeTransitionCallBack(listener.clone()), sender(&spec));
  let _current: CurrentState<i32> = probe.expect_msg_type();

  spec.system_mut().stop(&listener);
  let from = sender(&spec);
  // The port of `awaitAssert`: ask again until the machine answers 0.
  let mut answered_zero = false;
  for _ in 0..300 {
    fsm.tell("listener-count", from.clone());
    spec.system_mut().run_until_idle();
    if counter.load(Ordering::SeqCst) == 0 {
      answered_zero = true;
      break;
    }
    std::thread::sleep(core::time::Duration::from_millis(10));
  }
  assert!(answered_zero, "the stopped listener is still in the set");
  spec.terminate();
}

// Original: "make previous and next state data available in onTransition"
#[test]
fn makes_previous_and_next_state_data_available_in_on_transition() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let mut probe = TestProbe::new(spec.system_mut());
  let probe_ref = probe.ref_handle();
  let fsm = spec.system_mut().actor_of(Props::new(move || other_fsm(probe_ref.clone())));

  fsm.tell("tick", sender(&spec));
  let pair: (i32, i32) = probe.expect_msg_type();
  assert_eq!(pair, (0, 1));
  spec.terminate();
}

// Original: "trigger transition event when goto() the same state"
#[test]
fn triggers_a_transition_event_when_goto_targets_the_same_state() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let mut probe = TestProbe::new(spec.system_mut());
  let probe_ref = probe.ref_handle();
  let counter = Arc::new(AtomicI32::new(-1));
  let shared = counter.clone();
  let fsm = spec.system_mut().actor_of(Props::new(move || my_fsm(shared.clone())));

  fsm.tell(SubscribeTransitionCallBack(probe_ref.clone()), sender(&spec));
  let _current: CurrentState<i32> = probe.expect_msg_type();
  fsm.tell("tick", sender(&spec));
  let _first: Transition<i32> = probe.expect_msg_type();

  fsm.tell("goto-self", sender(&spec));
  let same: Transition<i32> = probe.expect_msg_type();
  assert_eq!(same, Transition { fsm: fsm.clone(), from: 1, to: 1 });
  spec.terminate();
}

// Original: "not trigger transition event on stay()"
#[test]
fn does_not_trigger_a_transition_event_on_stay() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let mut probe = TestProbe::new(spec.system_mut());
  let probe_ref = probe.ref_handle();
  // MyFSM answers an unknown event from `when_unhandled` with a stay,
  // which must not produce a transition event either.
  let counter = Arc::new(AtomicI32::new(-1));
  let shared = counter.clone();
  let fsm = spec.system_mut().actor_of(Props::new(move || my_fsm(shared.clone())));

  fsm.tell(SubscribeTransitionCallBack(probe_ref.clone()), sender(&spec));
  let _current: CurrentState<i32> = probe.expect_msg_type();
  fsm.tell("stay-please", sender(&spec));
  probe.expect_no_message(core::time::Duration::from_millis(100));
  spec.terminate();
}

// Original: "not leak memory in nextState"
#[test]
fn does_not_leak_next_state_data_outside_on_transition() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let mut probe = TestProbe::new(spec.system_mut());
  let probe_ref = probe.ref_handle();
  let slot: Arc<Mutex<Option<Arc<i32>>>> = Arc::new(Mutex::new(None));
  let slot_in_fsm = slot.clone();
  let fsm = spec.system_mut().actor_of(Props::new(move || {
    let mut fsm: Fsm<i32, Arc<i32>> = Fsm::new();
    let notices = probe_ref.clone();
    fsm.on_transition_with_data(
      move |context: &mut ActorContext<'_>, _from: &i32, _to: &i32, previous: &Arc<i32>, next: &Arc<i32>| {
        notices.tell((**previous, **next), context.self_ref());
      },
    );
    fsm.start_with(0, Arc::new(0));
    let slot_in_state = slot_in_fsm.clone();
    fsm.when(0, move |_context: &mut ActorContext<'_>, _view: &FsmView<i32>, event: &Message, _data: &Arc<i32>| {
      match *event.downcast_ref::<&'static str>()? {
        | "go" => {
          let next = Arc::new(1);
          *slot_in_state.lock().unwrap() = Some(next.clone());
          Some(FsmState::goto(1).using(next))
        },
        | _ => None,
      }
    });
    fsm.when(1, |_context: &mut ActorContext<'_>, _view: &FsmView<i32>, _event: &Message, _data: &Arc<i32>| {
      Some(FsmState::stay())
    });
    fsm
  }));

  fsm.tell("go", sender(&spec));
  let pair: (i32, i32) = probe.expect_msg_type();
  assert_eq!(pair, (0, 1));

  spec.system_mut().run_until_idle();
  let next = slot.lock().unwrap().clone().expect("the state function recorded the next data");
  // Only the slot, this test's own inspection clone, and the machine's
  // current data hold the value: the transition did not leave another
  // reference behind.
  assert_eq!(Arc::strong_count(&next), 3);
  spec.terminate();
}
