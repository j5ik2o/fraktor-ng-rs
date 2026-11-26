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
//   actor-tests/src/test/scala/org/apache/pekko/actor/ReceiveTimeoutSpec.scala
//   (Pekko v2.0.0)
// Original test names are kept as comments next to the ported cases.
//
// Design: docs/design/issue-200-receive-timeout.md. Fourteen of the fifteen
// cases are ported. The case "get timeout while receiving
// NotInfluenceReceiveTimeout messages scheduled with Timers" waits for the
// port of Pekko's `Timers` (#257) and is not in this file.
//
// The original calls `context.setReceiveTimeout` in the actor's constructor;
// the port calls it from `Actor::pre_start`, which runs right after the
// actor is created. The original's `TransparentTick extends
// NotInfluenceReceiveTimeout` is registered with the system
// (`register_not_influence_receive_timeout`), because Rust cannot test trait
// membership at run time. `Duration.Undefined`, `Duration.Inf`, and a finite
// duration are the three values of `ReceiveTimeoutSetting`.
//
// `Await.ready(latch, max)` is `TestLatch::ready`, which drives the system
// while it waits. `intercept[TimeoutException] { Await.ready(latch, max) }`
// is `assert_latch_stays_closed`, which expects `ready` to fail with its
// timeout. The durations are the original's; the time factor scales only
// the waits, never the receive timeout, as in Pekko.

use core::time::Duration;

use std::panic::{self, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;

use fraktor_actor::kernel::actor::{
  Actor, ActorContext, DeadLetter, Identify, NotInfluenceReceiveTimeout, Props, ReceiveTimeout, ReceiveTimeoutSetting,
};
use fraktor_actor::kernel::actor_ref::ActorRef;
use fraktor_actor::kernel::actor_system::ActorSystem;
use fraktor_actor::kernel::event::EventClass;
use fraktor_actor::kernel::message::Message;
use fraktor_actor::kernel::supervision::SupervisorStrategy;
use fraktor_testkit::event_filter::EventFilter;
use fraktor_testkit::fraktorspec::FraktorSpec;
use fraktor_testkit::test_kit::{TestLatch, TestProbe};

const SPEC_SYSTEM_NAME: &str = "ReceiveTimeoutSpec";

// Original: `case object Tick`
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Tick;

// Original: `case object TransparentTick extends NotInfluenceReceiveTimeout`
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TransparentTick;

impl NotInfluenceReceiveTimeout for TransparentTick {}

fn millis(value: u64) -> Duration {
  Duration::from_millis(value)
}

fn finite(value: Duration) -> ReceiveTimeoutSetting {
  ReceiveTimeoutSetting::Finite(value)
}

fn new_spec() -> FraktorSpec {
  let spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  spec.system().register_not_influence_receive_timeout::<TransparentTick>();
  spec
}

fn new_latch(spec: &FraktorSpec, count: usize) -> TestLatch {
  TestLatch::new(count, spec.test_kit_settings())
}

fn is_receive_timeout(message: &Message) -> bool {
  message.downcast_ref::<ReceiveTimeout>().is_some()
}

// `intercept[TimeoutException] { Await.ready(latch, max) }`: the wait must end
// with the latch's own timeout, not with any other failure.
fn assert_latch_stays_closed(latch: &TestLatch, system: &ActorSystem, max: Duration) {
  let outcome = panic::catch_unwind(AssertUnwindSafe(|| latch.ready(system, max)));
  let payload = match outcome {
    | Ok(()) => panic!("the latch opened within {max:?}; the original expects a TimeoutException"),
    | Err(payload) => payload,
  };
  let text = payload
    .downcast_ref::<String>()
    .map(String::as_str)
    .or_else(|| payload.downcast_ref::<&str>().copied())
    .unwrap_or_default();
  assert!(text.starts_with("Timeout of"), "the wait must end with the latch's timeout, but it ended with {text:?}");
}

// The original's anonymous `new Actor { ... }`: the constructor body is
// `start`, and the `receive` partial function is `handle`.
struct AnonymousActor<S, H> {
  start: S,
  handle: H,
}

impl<S, H> Actor for AnonymousActor<S, H>
where
  S: FnMut(&mut ActorContext<'_>) + Send,
  H: FnMut(&mut ActorContext<'_>, &Message) + Send,
{
  fn pre_start(&mut self, context: &mut ActorContext<'_>) {
    (self.start)(context);
  }

  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    (self.handle)(context, &message);
  }
}

fn anonymous<S, H>(start: S, handle: H) -> Props
where
  S: FnMut(&mut ActorContext<'_>) + Clone + Send + Sync + 'static,
  H: FnMut(&mut ActorContext<'_>, &Message) + Clone + Send + Sync + 'static,
{
  Props::new(move || AnonymousActor { start: start.clone(), handle: handle.clone() })
}

fn no_start(_context: &mut ActorContext<'_>) {}

fn set_in_start(setting: ReceiveTimeoutSetting) -> impl FnMut(&mut ActorContext<'_>) + Clone + Send + Sync + 'static {
  move |context: &mut ActorContext<'_>| context.set_receive_timeout(setting)
}

// "classify messages that do not influence the timeout"
#[test]
fn classifies_messages_that_do_not_influence_the_timeout() {
  let mut spec = new_spec();
  let system = spec.system();

  assert!(
    system.is_not_influence_receive_timeout(&Message::new(Identify::new(Option::<()>::None))),
    "Identify(None) does not influence the timeout"
  );
  assert!(
    system.is_not_influence_receive_timeout(&Message::new(TransparentTick)),
    "TransparentTick does not influence the timeout"
  );
  assert!(!system.is_not_influence_receive_timeout(&Message::new(Tick)), "Tick influences the timeout");
  spec.terminate();
}

// "get timeout"
#[test]
fn gets_timeout() {
  let mut spec = new_spec();
  let timeout_latch = new_latch(&spec, 1);
  let latch = timeout_latch.clone();

  let timeout_actor =
    spec.system_mut().actor_of(anonymous(set_in_start(finite(millis(500))), move |_context, message| {
      if is_receive_timeout(message) {
        latch.open();
      }
    }));

  timeout_latch.ready(spec.system(), TestLatch::DEFAULT_TIMEOUT);
  spec.system_mut().stop(&timeout_actor);
  spec.terminate();
}

// "carry the configured timeout duration"
#[test]
fn receive_timeout_carries_the_configured_duration() {
  let mut spec = new_spec();
  let mut probe = TestProbe::new(spec.system_mut());
  let probe_ref = probe.ref_handle();

  let timeout_actor =
    spec.system_mut().actor_of(anonymous(set_in_start(finite(millis(500))), move |context, message| {
      if let Some(timeout) = message.downcast_ref::<ReceiveTimeout>() {
        probe_ref.tell(*timeout, context.self_ref());
      }
    }));

  let msg = probe.expect_msg_type::<ReceiveTimeout>();
  // `msg.timeout should ===(500.milliseconds)`, and the Java API line
  // `msg.getTimeout should ===(java.time.Duration.ofMillis(500))`: the port has
  // no `java.time.Duration`, so both read the configured duration through
  // `timeout()` (design section 1, note 1; section 13-2).
  assert_eq!(msg.timeout(), millis(500), "the ReceiveTimeout carries the configured duration");
  spec.system_mut().stop(&timeout_actor);
  spec.terminate();
}

// "reschedule timeout after regular receive"
#[test]
fn reschedules_timeout_after_regular_receive() {
  let mut spec = new_spec();
  let timeout_latch = new_latch(&spec, 1);
  let message_count = 100;
  let processed_latch = new_latch(&spec, message_count);
  let latch = timeout_latch.clone();
  let processed = processed_latch.clone();

  let timeout_actor =
    spec.system_mut().actor_of(anonymous(set_in_start(finite(millis(1000))), move |_context, message| {
      if message.is::<Tick>() {
        processed.count_down();
      } else if is_receive_timeout(message) {
        latch.open();
      }
    }));

  for _ in 0..message_count {
    timeout_actor.tell(Tick, spec.system().dead_letters());
  }

  processed_latch.ready(spec.system(), TestLatch::DEFAULT_TIMEOUT);
  assert_latch_stays_closed(&timeout_latch, spec.system(), millis(500));
  timeout_latch.ready(spec.system(), TestLatch::DEFAULT_TIMEOUT);
  spec.system_mut().stop(&timeout_actor);
  spec.terminate();
}

// "be able to turn off timeout if desired"
#[test]
fn can_turn_off_timeout() {
  let mut spec = new_spec();
  let count = Arc::new(AtomicUsize::new(0));
  let timeout_latch = new_latch(&spec, 1);
  let latch = timeout_latch.clone();
  let counter = Arc::clone(&count);

  let timeout_actor =
    spec.system_mut().actor_of(anonymous(set_in_start(finite(millis(500))), move |context, message| {
      if message.is::<Tick>() {
        return;
      }
      if is_receive_timeout(message) {
        counter.fetch_add(1, Ordering::SeqCst);
        latch.open();
        context.set_receive_timeout(ReceiveTimeoutSetting::Undefined);
      }
    }));

  timeout_actor.tell(Tick, spec.system().dead_letters());

  timeout_latch.ready(spec.system(), TestLatch::DEFAULT_TIMEOUT);
  assert_eq!(count.load(Ordering::SeqCst), 1, "the actor received exactly one ReceiveTimeout");
  spec.system_mut().stop(&timeout_actor);
  spec.terminate();
}

// "not receive timeout message when not specified"
#[test]
fn does_not_receive_timeout_when_not_specified() {
  let mut spec = new_spec();
  let timeout_latch = new_latch(&spec, 1);
  let latch = timeout_latch.clone();

  let timeout_actor = spec.system_mut().actor_of(anonymous(no_start, move |_context, message| {
    if is_receive_timeout(message) {
      latch.open();
    }
  }));

  assert_latch_stays_closed(&timeout_latch, spec.system(), millis(1000));
  spec.system_mut().stop(&timeout_actor);
  spec.terminate();
}

// "get timeout while receiving NotInfluenceReceiveTimeout messages"
//
// The original's `system.scheduler.scheduleWithFixedDelay(100.millis,
// 100.millis)` sends `TransparentTick` and `Identify(None)` from the
// scheduler's thread for as long as the test waits. The port sends them
// from a std thread every 100 milliseconds until the latch has opened
// (design section 1, note 2): an implementation that let these messages
// reset the timer would never fire, and the latch would time out.
#[test]
fn gets_timeout_while_receiving_not_influence_messages() {
  let mut spec = new_spec();
  let timeout_latch = new_latch(&spec, 1);
  let latch = timeout_latch.clone();

  // The test actor ignores `Identify`: the port has no automatic
  // `ActorIdentity` reply yet, and the original does not look at it.
  let timeout_actor =
    spec.system_mut().actor_of(anonymous(set_in_start(finite(millis(1000))), move |_context, message| {
      if is_receive_timeout(message) {
        latch.open();
      }
    }));

  let cancelled = Arc::new(AtomicBool::new(false));
  let ticks = thread::spawn({
    let cancelled = Arc::clone(&cancelled);
    let target = timeout_actor.clone();
    let no_sender = spec.system().dead_letters();
    move || {
      while !cancelled.load(Ordering::SeqCst) {
        thread::sleep(millis(100));
        if cancelled.load(Ordering::SeqCst) {
          break;
        }
        target.tell(TransparentTick, no_sender.clone());
        target.tell(Identify::new(Option::<()>::None), no_sender.clone());
      }
    }
  });

  let outcome =
    panic::catch_unwind(AssertUnwindSafe(|| timeout_latch.ready(spec.system(), TestLatch::DEFAULT_TIMEOUT)));
  // `ticks.cancel()`
  cancelled.store(true, Ordering::SeqCst);
  ticks.join().expect("the tick thread does not panic");
  if let Err(payload) = outcome {
    panic::resume_unwind(payload);
  }
  spec.system_mut().stop(&timeout_actor);
  spec.terminate();
}

// "get timeout while receiving only NotInfluenceReceiveTimeout messages"
#[test]
fn gets_timeout_while_receiving_only_not_influence_messages() {
  let mut spec = new_spec();
  let timeout_latch = new_latch(&spec, 2);
  let latch = timeout_latch.clone();

  let timeout_actor =
    spec.system_mut().actor_of(anonymous(set_in_start(finite(millis(1000))), move |context, message| {
      if is_receive_timeout(message) {
        let self_ref = context.self_ref();
        context.tell(&self_ref, TransparentTick);
        latch.count_down();
      }
    }));

  timeout_latch.ready(spec.system(), TestLatch::DEFAULT_TIMEOUT);
  spec.system_mut().stop(&timeout_actor);
  spec.terminate();
}

// "be able to turn on timeout in NotInfluenceReceiveTimeout message handler"
#[test]
fn can_turn_on_timeout_in_not_influence_handler() {
  let mut spec = new_spec();
  let timeout_latch = new_latch(&spec, 1);
  let latch = timeout_latch.clone();

  let timeout_actor = spec.system_mut().actor_of(anonymous(no_start, move |context, message| {
    if message.is::<TransparentTick>() {
      context.set_receive_timeout(finite(millis(500)));
    } else if is_receive_timeout(message) {
      latch.open();
    }
  }));

  timeout_actor.tell(TransparentTick, spec.system().dead_letters());

  timeout_latch.ready(spec.system(), TestLatch::DEFAULT_TIMEOUT);
  spec.system_mut().stop(&timeout_actor);
  spec.terminate();
}

// "be able to turn off timeout in NotInfluenceReceiveTimeout message handler"
#[test]
fn can_turn_off_timeout_in_not_influence_handler() {
  let mut spec = new_spec();
  let timeout_latch = new_latch(&spec, 1);
  let latch = timeout_latch.clone();
  // Set once the `TransparentTick` handler has returned from
  // `set_receive_timeout`, so a latch that stays closed because the actor
  // failed is not taken for a timeout that was turned off.
  let handled = Arc::new(AtomicBool::new(false));
  let handler_done = Arc::clone(&handled);

  let timeout_actor =
    spec.system_mut().actor_of(anonymous(set_in_start(finite(millis(500))), move |context, message| {
      if message.is::<TransparentTick>() {
        context.set_receive_timeout(ReceiveTimeoutSetting::Infinite);
        handler_done.store(true, Ordering::SeqCst);
      } else if is_receive_timeout(message) {
        latch.open();
      }
    }));

  timeout_actor.tell(TransparentTick, spec.system().dead_letters());

  assert_latch_stays_closed(&timeout_latch, spec.system(), millis(1000));
  assert!(handled.load(Ordering::SeqCst), "the TransparentTick handler set Duration.Inf and returned");
  spec.system_mut().stop(&timeout_actor);
  spec.terminate();
}

// "remove / change existing timeout while handling a NotInfluenceReceiveTimeout message"
#[test]
fn removes_or_changes_timeout_while_handling_not_influence_message() {
  let mut spec = new_spec();
  let timeout_latch = new_latch(&spec, 1);
  let initial_timeout = millis(500);
  let latch = timeout_latch.clone();
  // Same guard as in `can_turn_off_timeout_in_not_influence_handler`.
  let handled = Arc::new(AtomicBool::new(false));
  let handler_done = Arc::clone(&handled);

  let timeout_actor =
    spec.system_mut().actor_of(anonymous(set_in_start(finite(initial_timeout)), move |context, message| {
      if message.is::<TransparentTick>() {
        context.set_receive_timeout(ReceiveTimeoutSetting::Undefined);
        handler_done.store(true, Ordering::SeqCst);
      } else if is_receive_timeout(message) {
        latch.open();
      }
    }));

  timeout_actor.tell(TransparentTick, spec.system().dead_letters());

  assert_latch_stays_closed(&timeout_latch, spec.system(), initial_timeout * 2);
  assert!(handled.load(Ordering::SeqCst), "the TransparentTick handler set Duration.Undefined and returned");
  spec.system_mut().stop(&timeout_actor);
  spec.terminate();
}

// "work correctly if the timeout is changed multiple times while handling a
// NotInfluenceReceiveTimeout message"
#[test]
fn changing_timeout_multiple_times_in_not_influence_handler_uses_the_last_setting() {
  let mut spec = new_spec();
  let mut probe = TestProbe::new(spec.system_mut());
  let probe_ref = probe.ref_handle();
  let initial_timeout = millis(500);

  // The original's `var count` in the actor; the port shares it so the test
  // can see that both ticks were handled.
  let count = Arc::new(AtomicUsize::new(0));
  let worked = Arc::clone(&count);
  let timeout_actor =
    spec.system_mut().actor_of(anonymous(set_in_start(finite(initial_timeout)), move |context, message| {
      if message.is::<TransparentTick>() {
        context.set_receive_timeout(finite(initial_timeout * 2));
        worked.fetch_add(1, Ordering::SeqCst); // do some work then
        context.set_receive_timeout(finite(initial_timeout));
      } else if let Some(timeout) = message.downcast_ref::<ReceiveTimeout>() {
        probe_ref.tell(*timeout, context.self_ref());
      }
    }));

  probe.expect_no_message(initial_timeout / 2);

  timeout_actor.tell(TransparentTick, spec.system().dead_letters());
  timeout_actor.tell(TransparentTick, spec.system().dead_letters());

  // not triggered by initialTimeout because it was changed by the ticks
  // wait shorter than last set value, but longer than initialTimeout
  probe.expect_no_message(initial_timeout / 2 + millis(50));
  // now should happen
  probe.expect_msg_type::<ReceiveTimeout>();
  assert_eq!(count.load(Ordering::SeqCst), 2, "both TransparentTick handlers ran to their end");
  spec.system_mut().stop(&timeout_actor);
  spec.terminate();
}

// Original: `class RestartingParent(probe: ActorRef)`. The child is created in
// the constructor with `context.actorOf(props)`, under the parent's default
// strategy, and every message is forwarded to it.
struct RestartingParent {
  probe: ActorRef,
  child: Option<ActorRef>,
}

impl Actor for RestartingParent {
  fn pre_start(&mut self, context: &mut ActorContext<'_>) {
    let restarting = Arc::new(AtomicBool::new(false));
    let probe = self.probe.clone();
    let child = context.actor_of_with_strategy(
      Props::new(move || RestartingChild { probe: probe.clone(), restarting: Arc::clone(&restarting) }),
      SupervisorStrategy::default_strategy(),
    );
    self.child = Some(child);
  }

  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    let child = self.child.clone().expect("the child is created in pre_start");
    child.forward(message, context);
  }
}

// Original: `class RestartingChild(probe: ActorRef, restarting: AtomicBoolean)`.
struct RestartingChild {
  probe: ActorRef,
  restarting: Arc<AtomicBool>,
}

impl Actor for RestartingChild {
  fn pre_start(&mut self, context: &mut ActorContext<'_>) {
    if self.restarting.load(Ordering::SeqCst) {
      self.probe.tell("restarting", context.self_ref());
      context.set_receive_timeout(finite(millis(500)));
    } else {
      self.probe.tell("starting", context.self_ref());
    }
  }

  fn post_stop_with_context(&mut self, context: &mut ActorContext<'_>) {
    self.probe.tell("stopping", context.self_ref());
  }

  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    if message == "crash" {
      self.restarting.store(true, Ordering::SeqCst);
      self.probe.tell("crashing", context.self_ref());
      // `throw TestException("boom bang")`
      panic!("boom bang");
    }
    if let Some(timeout) = message.downcast_ref::<ReceiveTimeout>() {
      self.probe.tell(*timeout, context.self_ref());
      return;
    }
    // `probe ! other`. The port forwards the message as it is; the sender,
    // which the original sets to the child, is not looked at.
    self.probe.forward(message, context);
  }
}

// "get the timeout when scheduled immediately on restart" (#28266 reproducer)
#[test]
fn gets_timeout_when_scheduled_immediately_on_restart() {
  let mut spec = new_spec();
  let mut probe = TestProbe::new(spec.system_mut());
  let probe_ref = probe.ref_handle();
  let parent =
    spec.system_mut().actor_of(Props::new(move || RestartingParent { probe: probe_ref.clone(), child: None }));
  probe.expect_msg(&"starting");

  let filter = EventFilter::error().with_message("boom bang");
  let target = parent.clone();
  let sender = spec.system().dead_letters();
  EventFilter::intercept(&filter, spec.system_mut(), move || {
    target.tell("crash", sender);
  });

  probe.expect_msg(&"crashing");
  probe.expect_msg(&"stopping");
  probe.expect_msg(&"restarting");
  probe.expect_msg_type::<ReceiveTimeout>();
  spec.terminate();
}

// Original: `class StoppingSelfActor(probe: ActorRef)`.
struct StoppingSelfActor {
  probe: ActorRef,
}

impl Actor for StoppingSelfActor {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    if message == "Stop" {
      context.set_receive_timeout(finite(millis(200)));
      let self_ref = context.self_ref();
      context.stop(&self_ref);
    }
  }

  fn post_stop_with_context(&mut self, context: &mut ActorContext<'_>) {
    self.probe.tell("stopping", context.self_ref());
  }
}

// "Will cancel receive timeout task if stopped"
#[test]
fn cancels_receive_timeout_task_when_actor_stops() {
  let mut spec = new_spec();
  let mut probe = TestProbe::new(spec.system_mut());
  spec
    .system()
    .event_stream()
    .subscribe(Some(probe.ref_handle()), EventClass::of::<DeadLetter>())
    .expect("the probe is not null");
  let probe_ref = probe.ref_handle();
  let actor = spec.system_mut().actor_of(Props::new(move || StoppingSelfActor { probe: probe_ref.clone() }));
  actor.tell("Stop", spec.system().dead_letters());
  probe.expect_msg(&"stopping");
  probe.expect_no_message(millis(200) * 2);
  // `context.stop(self)` in the original stops the actor; a restart after a
  // failure in the handler would also run `postStop` and send "stopping".
  assert!(spec.system().is_actor_terminated(&actor), "the actor stopped itself");
  spec.terminate();
}
