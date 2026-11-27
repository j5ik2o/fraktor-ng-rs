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
//   actor-tests/src/test/scala/org/apache/pekko/actor/FSMTimingSpec.scala
//   (Pekko v2.0.0)
// Original test names are kept as comments next to the ported cases.
//
// Design: docs/design/issue-198-fsm-timing-actor-ref.md. Eight of the ten
// cases are ported here. "correctly cancel a named timer" (`:102-113`,
// `:237-251`) and "not get confused between named and state timers"
// (`:115-129`, `:262-277`) wait on `ActorCell.mailbox.hasMessages`, which
// arrives with issue #234 stage 6 (#257), and are not in this file (design
// sections 1.2 and 1.4).
//
// The original shares one `StateMachine` (and, for case 3, one
// `StoppingActor`) across the ten cases of the suite. Rust runs tests in
// parallel, so every case starts its own `FraktorSpec` and its own machine
// (design section 1.2). `start_fsm` plays the original's `beforeEach`
// (`FSMTimingSpec.scala:26-31`): subscribe, wait for the `CurrentState`, and
// ignore the transition out of the starting state.
//
// No machine calls `initialize()`, as neither `StateMachine` (`:198-290`) nor
// `StoppingActor` (`:292-300`) does (design section 3.1(a)). `initialState` is
// `goto(Initial)` in the original; `startWith(Initial, 0)` arms no timer for
// `Initial`, and the first state timer is armed by the `make_transition` of the
// first handled event.
//
// The delays of `StateMachine` are the original's, scaled by
// `TestKitSettings::dilated`; `StoppingActor`'s 200 milliseconds are not
// scaled, as in the original. `expectNoMessage(remainingOrDefault)` inside a
// `within` is `spec.test_kit_mut().expect_no_message(spec.remaining())`, and
// the argument-less `expectNoMessage()` waits the dilated
// `expect-no-message-default` (100 milliseconds), as the original does
// (design section 1.2).
//
// The original keeps the timer a state function arms in a field of
// `StateMachine` and starts it on the first event of the state (`:205-227`,
// `:243`). The port arms the named timer from the state function that handles
// the event naming the state, which is the same event and the same turn, and
// lets `make_transition` arm the state timer afterwards.

use core::{fmt, time::Duration};

use fraktor_actor::kernel::{
  actor::{
    ActorContext, CurrentState, DeadLetter, Fsm, FsmState, FsmTimeout, FsmView, Props, StateTimeout,
    SubscribeTransitionCallBack, Transition,
  },
  actor_ref::ActorRef,
  event::EventClass,
  message::Message,
};
use fraktor_testkit::{
  event_filter::EventFilter,
  fraktorspec::FraktorSpec,
  test_kit::{ReceiveWhile, TestProbe},
  test_kit_extension::TestKitSettings,
};

const SPEC_SYSTEM_NAME: &str = "FSMTimingSpec";

// Original: `sealed trait State` with the states `StateMachine` uses.
#[derive(Clone, Debug, PartialEq, Eq)]
enum State {
  Initial,
  TestStateTimeout,
  TestSingleTimer,
  TestSingleTimerResubmit,
  TestRepeatedTimer,
  TestUnhandled,
}

// The original's states extend `FSM.State`, whose `toString` prints the name.
// The port carries the state name apart from the next state, and the warning
// text is compared to the character, so the display prints the name unquoted
// (design section 3.2(a)).
impl fmt::Display for State {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      | Self::Initial => formatter.write_str("Initial"),
      | Self::TestStateTimeout => formatter.write_str("TestStateTimeout"),
      | Self::TestSingleTimer => formatter.write_str("TestSingleTimer"),
      | Self::TestSingleTimerResubmit => formatter.write_str("TestSingleTimerResubmit"),
      | Self::TestRepeatedTimer => formatter.write_str("TestRepeatedTimer"),
      | Self::TestUnhandled => formatter.write_str("TestUnhandled"),
    }
  }
}

// Original: `case object Tick`.
#[derive(Clone, Copy, Debug)]
struct Tick;

// Original `Tick.toString` is `Tick`; the warning is matched to the character.
impl fmt::Display for Tick {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str("Tick")
  }
}

// Original: `case object Tock`.
#[derive(Clone, Copy, Debug)]
struct Tock;

// Original: `case class Unhandled(msg: AnyRef)`, whose `toString` is
// `Unhandled(test)`, without quotes.
#[derive(Clone, Copy, Debug)]
struct Unhandled(&'static str);

impl fmt::Display for Unhandled {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(formatter, "Unhandled({})", self.0)
  }
}

// Original: `case object SetHandler`.
#[derive(Clone, Copy, Debug)]
struct SetHandler;

// Original: `case object Cancel`.
#[derive(Clone, Copy, Debug)]
struct Cancel;

fn millis(value: u64) -> Duration {
  Duration::from_millis(value)
}

fn dilated(settings: TestKitSettings, duration: Duration) -> Duration {
  settings.dilated(duration)
}

// The original's argument-less `expectNoMessage()` waits
// `ExpectNoMessageDefaultTimeout.dilated` (`TestKit.scala:733-734`), not the
// rest of the scope.
fn expect_no_message_by_default(spec: &mut FraktorSpec) {
  let settings = spec.test_kit_settings();
  let max = settings.dilated(settings.expect_no_message_default_timeout());
  spec.test_kit_mut().expect_no_message(max);
}

fn sender(spec: &FraktorSpec) -> ActorRef {
  spec.system().dead_letters()
}

// Original's `whenUnhandled { case Event(Tick, _) => stay() }` for
// `TestUnhandled` (`FSMTimingSpec.scala:134-140`).
fn custom_unhandled(
  _context: &mut ActorContext<'_>,
  _view: &FsmView<State>,
  message: &Message,
  data: &i32,
) -> Option<FsmState<State, i32>> {
  if message.is::<Tick>() { Some(FsmState::stay().using(*data)) } else { None }
}

// Original: `class StateMachine extends FSM[State, Int]` (`FSMTimingSpec.scala:198-290`).
fn state_machine(settings: TestKitSettings) -> Fsm<State, i32> {
  let state_timeout = dilated(settings, millis(800));
  let single_timer = dilated(settings, millis(500));
  let resubmit_tick = dilated(settings, millis(500));
  let resubmit_tock = dilated(settings, millis(500));
  let repeated_timer = dilated(settings, millis(100));

  let mut machine = Fsm::new();
  machine.with_state_display();
  machine.start_with(State::Initial, 0);
  // `:198-210`
  machine.when_with_state_timeout(State::Initial, None, move |_context, _view, message, data| {
    if message == "TestStateTimeout" {
      Some(FsmState::goto(State::TestStateTimeout).using(*data))
    } else if message == "TestSingleTimer" {
      Some(FsmState::goto(State::TestSingleTimer).using(*data))
    } else if message == "TestSingleTimerResubmit" {
      Some(FsmState::goto(State::TestSingleTimerResubmit).using(*data))
    } else if message == "TestRepeatedTimer" {
      Some(FsmState::goto(State::TestRepeatedTimer).using(*data))
    } else if message == "TestUnhandled" {
      Some(FsmState::goto(State::TestUnhandled).using(*data))
    } else {
      Some(FsmState::stay())
    }
  });
  // `:212-219`
  machine.when_with_state_timeout(
    State::TestStateTimeout,
    Some(state_timeout),
    move |_context, _view, message, data| {
      if message.downcast_ref::<StateTimeout>().is_some() || message.is::<Cancel>() {
        Some(FsmState::goto(State::Initial).using(*data))
      } else {
        Some(FsmState::stay())
      }
    },
  );
  // `:221-223`
  machine.when(State::TestSingleTimer, move |_context, view, message, data| {
    if message == "TestSingleTimer" {
      view.start_single_timer("tester", Message::new(Tick), single_timer);
      Some(FsmState::stay().using(*data))
    } else if message.is::<Tick>() {
      Some(FsmState::goto(State::Initial).using(*data))
    } else {
      None
    }
  });
  // `:225-229`, `:241-245`
  machine.when(State::TestSingleTimerResubmit, move |_context, view, message, data| {
    if message == "TestSingleTimerResubmit" {
      view.start_single_timer("blah", Message::new(Tick), resubmit_tick);
      Some(FsmState::stay().using(*data))
    } else if message.is::<Tick>() {
      // The fired `Tick` arrives with `sender() == self` in the original;
      // either way the same name is started again, here for `Tock`.
      view.start_single_timer("blah", Message::new(Tock), resubmit_tock);
      Some(FsmState::stay().using(*data))
    } else if message.is::<Tock>() {
      Some(FsmState::goto(State::Initial).using(*data))
    } else {
      None
    }
  });
  // `:253-260`
  machine.when(State::TestRepeatedTimer, move |_context, view, message, data| {
    if message == "TestRepeatedTimer" {
      view.start_timer_with_fixed_delay("tester", Message::new(Tick), repeated_timer);
      Some(FsmState::stay().using(*data))
    } else if message.is::<Tick>() && *data < 5 {
      Some(FsmState::stay().using(*data + 1))
    } else if *data >= 5 {
      view.cancel_timer("tester");
      Some(FsmState::goto(State::Initial).using(*data))
    } else {
      None
    }
  });
  // `:132-141`: the unhandled state. `Tick` before `SetHandler` is
  // unhandled; `SetHandler` arms a custom `whenUnhandled` that answers `Tick`
  // with `stay()`, so the next `Tick` is handled and `Unhandled("test")` is
  // not; `Cancel` leaves the state and clears the custom handler.
  machine.when(State::TestUnhandled, move |_context, _view, message, data| {
    if message.is::<SetHandler>() {
      Some(FsmState::stay().with_unhandled(custom_unhandled).using(*data))
    } else if message.is::<Cancel>() {
      Some(FsmState::goto(State::Initial).clear_unhandled().using(*data))
    } else {
      None
    }
  });
  machine
}

// Original: `class StoppingActor extends FSM[State, Int]` (`FSMTimingSpec.scala:292-300`).
// It has one state, `Initial`, with a 200 millisecond timeout (undilated, as in
// the original), and never calls `initialize()`: the first event arms the
// state timer and the stop takes it away before it fires.
fn stopping_actor() -> Fsm<State, i32> {
  let timeout = millis(200);
  let mut machine = Fsm::new();
  machine.with_state_display();
  machine.start_with(State::Initial, 0);
  machine.when_with_state_timeout(State::Initial, Some(timeout), move |context, _view, _message, data| {
    context.stop(&context.self_ref());
    Some(FsmState::stay().using(*data))
  });
  machine
}

/// Starts the `StateMachine` of the original with `settings` as its delays.
fn machine_props(settings: TestKitSettings) -> Props {
  Props::new(move || state_machine(settings))
}

fn stopping_props() -> Props {
  Props::new(stopping_actor)
}

/// Starts the original's `StateMachine` and plays its `beforeEach`: subscribes,
/// waits for the `CurrentState`, and lets the transition out of `Initial`
/// through untouched.
fn start_fsm(spec: &mut FraktorSpec) -> ActorRef {
  let settings = spec.test_kit_settings();
  let fsm = spec.system_mut().actor_of(machine_props(settings));
  let test_actor = spec.test_actor();
  fsm.tell(SubscribeTransitionCallBack(test_actor.clone()), test_actor.clone());
  let current: CurrentState<State> = spec.expect_msg_type();
  assert_eq!(current.state, State::Initial, "the machine starts in Initial");
  assert_eq!(current.fsm, fsm, "the current state names the machine");
  let initial_machine = fsm.clone();
  spec.test_kit_mut().ignore_msg(move |message| {
    matches!(
      message.downcast_ref::<Transition<State>>(),
      Some(transition) if transition.fsm == initial_machine && transition.from == State::Initial
    )
  });
  fsm
}

// "receive StateTimeout" (case 1, `:32-40`)
#[test]
fn receives_a_state_timeout() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let fsm = start_fsm(&mut spec);
  let test_actor = spec.test_actor();
  spec.within(millis(2000), |spec| {
    fsm.tell("TestStateTimeout", test_actor.clone());
    spec.within_range(millis(500), millis(2000), |spec| {
      let expected = Transition { fsm: fsm.clone(), from: State::TestStateTimeout, to: State::Initial };
      let received: Transition<State> = spec.expect_msg_type();
      assert_eq!(received, expected);
    });
    let remaining = spec.remaining();
    spec.test_kit_mut().expect_no_message(remaining);
  });
  spec.terminate();
}

// "cancel a StateTimeout" (case 2, `:42-50`)
#[test]
fn cancels_a_state_timeout() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let fsm = start_fsm(&mut spec);
  let test_actor = spec.test_actor();
  fsm.tell("TestStateTimeout", test_actor.clone());
  spec.within(millis(1000), |spec| {
    fsm.tell(Cancel, test_actor.clone());
    // The original receives `Cancel` first; the machine answers it with the
    // transition, which carries `Cancel` nowhere, so only the transition
    // arrives.
    let expected = Transition { fsm: fsm.clone(), from: State::TestStateTimeout, to: State::Initial };
    let received: Transition<State> = spec.expect_msg_type();
    assert_eq!(received, expected);
    let remaining = spec.remaining();
    spec.test_kit_mut().expect_no_message(remaining);
  });
  spec.terminate();
}

// "cancel a StateTimeout when actor is stopped" (case 3, `:52-59`)
#[test]
fn cancels_a_state_timeout_when_the_actor_is_stopped() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let mut probe = TestProbe::new(spec.system_mut());
  spec
    .system()
    .event_stream()
    .subscribe(Some(probe.ref_handle()), EventClass::of::<DeadLetter>())
    .expect("subscribe to DeadLetter");
  let stopping = spec.system_mut().actor_of(stopping_props());
  probe.watch(&stopping);
  let dead_letters = sender(&spec);
  stopping.tell("die", dead_letters);
  probe.expect_msg_terminated_within(millis(2000));
  // The original waits 400 milliseconds, undilated, for no `DeadLetter`.
  probe.expect_no_message(millis(400));
  spec.terminate();
}

// "allow StateTimeout override" (case 4, `:61-71`)
#[test]
fn allows_a_state_timeout_override() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let settings = spec.test_kit_settings();
  let fsm = spec.system_mut().actor_of(Props::new(move || override_state_machine(settings)));
  let test_actor = spec.test_actor();
  fsm.tell(SubscribeTransitionCallBack(test_actor.clone()), test_actor.clone());
  let current: CurrentState<State> = spec.expect_msg_type();
  assert_eq!(current.state, State::Initial);
  let initial_machine = fsm.clone();
  spec.test_kit_mut().ignore_msg(move |message| {
    matches!(
      message.downcast_ref::<Transition<State>>(),
      Some(transition) if transition.fsm == initial_machine && transition.from == State::Initial
    )
  });
  spec.within(millis(400), |spec| {
    fsm.tell("TestStateTimeout", test_actor.clone());
    // `forMax(Duration.Inf)` disables the state's own 800 milliseconds, so the
    // default no-message length passes quietly.
    expect_no_message_by_default(spec);
  });
  spec.within(millis(1000), |spec| {
    fsm.tell(Cancel, test_actor.clone());
    let expected = Transition { fsm: fsm.clone(), from: State::TestStateTimeout, to: State::Initial };
    let received: Transition<State> = spec.expect_msg_type();
    assert_eq!(received, expected);
  });
  spec.terminate();
}

// The machine of case 4 (`:211-214`): `Initial` moves to `TestStateTimeout`
// with `for_max(FsmTimeout::Infinite)`, so the state's own timeout is not
// armed.
fn override_state_machine(settings: TestKitSettings) -> Fsm<State, i32> {
  let state_timeout = dilated(settings, millis(800));
  let mut machine = Fsm::new();
  machine.with_state_display();
  machine.start_with(State::Initial, 0);
  machine.when_with_state_timeout(State::Initial, None, move |_context, _view, message, data| {
    if message == "TestStateTimeout" {
      Some(FsmState::goto(State::TestStateTimeout).for_max(FsmTimeout::Infinite).using(*data))
    } else {
      Some(FsmState::stay())
    }
  });
  machine.when_with_state_timeout(
    State::TestStateTimeout,
    Some(state_timeout),
    move |_context, _view, message, data| {
      if message.is::<Cancel>() || message.downcast_ref::<StateTimeout>().is_some() {
        Some(FsmState::goto(State::Initial).using(*data))
      } else {
        Some(FsmState::stay())
      }
    },
  );
  machine
}

// "receive single-shot timer" (case 5, `:73-83`)
#[test]
fn receives_a_single_shot_timer() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let fsm = start_fsm(&mut spec);
  let test_actor = spec.test_actor();
  spec.within(millis(2000), |spec| {
    fsm.tell("TestSingleTimer", test_actor.clone());
    spec.within_range(millis(500), millis(1000), |spec| {
      let _tick: Tick = spec.expect_msg_type();
      let expected = Transition { fsm: fsm.clone(), from: State::TestSingleTimer, to: State::Initial };
      let received: Transition<State> = spec.expect_msg_type();
      assert_eq!(received, expected);
    });
    expect_no_message_by_default(spec);
  });
  spec.terminate();
}

// "resubmit single-shot timer" (case 6, `:85-100`)
#[test]
fn resubmits_a_single_shot_timer() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let fsm = start_fsm(&mut spec);
  let test_actor = spec.test_actor();
  spec.within(millis(2500), |spec| {
    fsm.tell("TestSingleTimerResubmit", test_actor.clone());
    spec.within_range(millis(500), millis(1000), |spec| {
      let _tick: Tick = spec.expect_msg_type();
    });
    spec.within(millis(1000), |spec| {
      let _tock: Tock = spec.expect_msg_type();
      let expected = Transition { fsm: fsm.clone(), from: State::TestSingleTimerResubmit, to: State::Initial };
      let received: Transition<State> = spec.expect_msg_type();
      assert_eq!(received, expected);
    });
    expect_no_message_by_default(spec);
  });
  spec.terminate();
}

// "receive and cancel a repeated timer" (case 9, `:131-141`)
#[test]
fn receives_and_cancels_a_repeated_timer() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let fsm = start_fsm(&mut spec);
  let test_actor = spec.test_actor();
  fsm.tell("TestRepeatedTimer", test_actor.clone());
  let ticks: Vec<Tick> =
    spec.receive_while(ReceiveWhile::new().max(millis(2000)), |message| message.downcast_ref::<Tick>().copied());
  assert_eq!(ticks.len(), 5, "the repeated timer fired five times, as data reached 5");
  let expected = Transition { fsm: fsm.clone(), from: State::TestRepeatedTimer, to: State::Initial };
  spec.within(millis(500), |spec| {
    let received: Transition<State> = spec.expect_msg_type();
    assert_eq!(received, expected);
  });
  spec.terminate();
}

// "notify unhandled messages" (case 10, `:143-160`)
#[test]
fn notifies_unhandled_messages() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let fsm = start_fsm(&mut spec);
  let path = fsm.path().to_string();
  let filters = [
    EventFilter::warning()
      .with_message("unhandled event Tick in state TestUnhandled")
      .with_source(&path)
      .with_occurrences(1),
    EventFilter::warning()
      .with_message("unhandled event Unhandled(test) in state TestUnhandled")
      .with_source(&path)
      .with_occurrences(1),
  ];
  let test_actor = spec.test_actor();
  spec.filter_events(&filters, |spec| {
    // `:149`, outside the `within`.
    fsm.tell("TestUnhandled", test_actor.clone());
    spec.within(millis(3000), |spec| {
      fsm.tell(Message::with_display(Tick), test_actor.clone());
      fsm.tell(SetHandler, test_actor.clone());
      fsm.tell(Message::with_display(Tick), test_actor.clone());
      fsm.tell(Message::with_display(Unhandled("test")), test_actor.clone());
      fsm.tell(Cancel, test_actor.clone());
      let expected = Transition { fsm: fsm.clone(), from: State::TestUnhandled, to: State::Initial };
      let received: Transition<State> = spec.expect_msg_type();
      assert_eq!(received, expected);
    });
  });
  spec.terminate();
}
