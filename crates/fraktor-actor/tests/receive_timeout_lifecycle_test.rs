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

// The receive timeout around the life cycle of an actor: its sender, a
// child's stop and the parent's child-terminated hook, a restart, a stop the
// actor asks for itself, `Kill`, and a `Terminated` that arrives after the
// watch was removed. The cases are outside ReceiveTimeoutSpec and are not
// counted in the ledger. Each one states what Pekko v2.0.0 does, after:
//
// - `actor/dungeon/ReceiveTimeout.scala`: `setReceiveTimeout` changes the
//   setting and the version only; the scheduled task is touched by
//   `cancelReceiveTimeoutIfNeeded` and `checkReceiveTimeout`, and the task is
//   `scheduleOnce(timeout, self, ReceiveTimeout(timeout))`, with no sender.
// - `ActorCell.invoke` (lines 568-585): those two calls run before and after
//   every message, an auto-received one (`Kill`, `Terminated`) included.
// - `ActorCell.systemInvoke` (line 531) and `FaultHandling.handleChildTerminated`
//   (lines 327-351): a child's stop reaches the parent as the system message
//   `DeathWatchNotification`, so the parent's `SupervisorStrategy
//   .handleChildTerminated` runs outside `invoke` and never touches the
//   timeout.
// - `FaultHandling.terminate` (lines 180-182): the timeout is cancelled when
//   the `Terminate` is handled, not when the stop is asked for.
// - `LightArrayRevolverScheduler.schedule` (lines 223-227): a zero delay runs
//   the task at once and returns `NotCancellable`.
//
// The clock is driven by hand. A task whose deadline has come fires at the
// next point the system is driven, which is where Pekko's scheduler would
// have sent the message under the same clock.

use core::time::Duration;

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use fraktor_actor::kernel::actor::{
  Actor, ActorContext, DeadLetter, NotInfluenceReceiveTimeout, Props, ReceiveTimeout, ReceiveTimeoutSetting,
};
use fraktor_actor::kernel::actor_ref::ActorRef;
use fraktor_actor::kernel::event::EventClass;
use fraktor_actor::kernel::message::Message;
use fraktor_actor::kernel::supervision::{Directive, SupervisorStrategy};
use fraktor_actor::kernel::Clock;
use fraktor_testkit::fraktorspec::FraktorSpec;
use fraktor_testkit::test_kit::TestProbe;

const SYSTEM_NAME: &str = "ReceiveTimeoutLifecycleTest";

// A hand-driven monotonic clock, in nanoseconds. It never advances unless
// the test, or an actor under test, advances it.
#[derive(Clone)]
struct ControlledClock(Arc<AtomicU64>);

impl ControlledClock {
  fn new(start: Duration) -> Self {
    Self(Arc::new(AtomicU64::new(start.as_nanos() as u64)))
  }

  fn advance(&self, by: Duration) {
    self.0.fetch_add(by.as_nanos() as u64, Ordering::SeqCst);
  }
}

impl Clock for ControlledClock {
  fn monotonic(&self) -> Duration {
    Duration::from_nanos(self.0.load(Ordering::SeqCst))
  }
}

type Deliveries = Arc<Mutex<Vec<Duration>>>;
type Slot = Arc<Mutex<Option<ActorRef>>>;

fn millis(value: u64) -> Duration {
  Duration::from_millis(value)
}

fn finite(value: Duration) -> ReceiveTimeoutSetting {
  ReceiveTimeoutSetting::Finite(value)
}

fn new_deliveries() -> Deliveries {
  Arc::new(Mutex::new(Vec::new()))
}

fn new_slot() -> Slot {
  Arc::new(Mutex::new(None))
}

fn delivered(deliveries: &Deliveries) -> Vec<Duration> {
  deliveries.lock().unwrap().clone()
}

// A system whose clock reads 10s and moves only by hand.
fn spec_with_clock() -> (FraktorSpec, ControlledClock) {
  let spec = FraktorSpec::new(SYSTEM_NAME);
  let clock = ControlledClock::new(Duration::from_secs(10));
  spec.system().set_clock(Arc::new(clock.clone()));
  (spec, clock)
}

fn dead_letter_listener(spec: &mut FraktorSpec) -> TestProbe {
  let listener = TestProbe::new(spec.system_mut());
  spec
    .system()
    .event_stream()
    .subscribe(Some(listener.ref_handle()), EventClass::of::<DeadLetter>())
    .expect("the probe is not null");
  listener
}

fn record(deliveries: &Deliveries, message: &Message) -> bool {
  match message.downcast_ref::<ReceiveTimeout>() {
    | Some(timeout) => {
      deliveries.lock().unwrap().push(timeout.timeout());
      true
    },
    | None => false,
  }
}

struct Empty;

impl Actor for Empty {
  fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {}
}

fn empty_props() -> Props {
  Props::new(|| Empty)
}

// A child that stops itself on "die".
struct SelfStoppingChild;

impl Actor for SelfStoppingChild {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    if message.downcast_ref::<&str>() == Some(&"die") {
      let me = context.self_ref();
      context.stop(&me);
    }
  }
}

// A child whose `post_stop` moves the clock by `by`: time passes while
// its parent waits for it to stop.
struct SlowStoppingChild {
  clock: ControlledClock,
  by: Duration,
}

impl Actor for SlowStoppingChild {
  fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {}

  fn post_stop(&mut self) {
    self.clock.advance(self.by);
  }
}

fn slow_stopping_props(clock: &ControlledClock, by: Duration) -> Props {
  let clock = clock.clone();
  Props::new(move || SlowStoppingChild { clock: clock.clone(), by })
}

// ---------------------------------------------------------------------------
// The sender of `ReceiveTimeout`

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Reply;

// An actor that answers the sender of every `ReceiveTimeout`, as an echo
// that replies to whatever it receives would.
struct ReplyingToSender {
  senders: Arc<Mutex<Vec<ActorRef>>>,
  replies: Arc<AtomicUsize>,
}

impl Actor for ReplyingToSender {
  fn pre_start(&mut self, context: &mut ActorContext<'_>) {
    context.set_receive_timeout(finite(Duration::from_secs(1)));
  }

  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    if message.downcast_ref::<ReceiveTimeout>().is_some() {
      let sender = context.sender();
      self.senders.lock().unwrap().push(sender.clone());
      context.tell(&sender, Reply);
      return;
    }
    if message.downcast_ref::<Reply>().is_some() {
      self.replies.fetch_add(1, Ordering::SeqCst);
    }
  }
}

// Pekko sends `ReceiveTimeout` with `Actor.noSender` (the implicit sender
// of `Scheduler.scheduleOnce`, `Scheduler.scala` lines 355-359), so
// `sender()` is `deadLetters` and a reply to it is a dead letter. It never
// comes back to the actor, where it would reset the timeout.
#[test]
fn receive_timeout_has_no_sender_and_a_reply_to_it_is_a_dead_letter() {
  let (mut spec, clock) = spec_with_clock();
  let mut dead_listener = dead_letter_listener(&mut spec);
  let senders = Arc::new(Mutex::new(Vec::new()));
  let replies = Arc::new(AtomicUsize::new(0));
  let actor = spec.system_mut().actor_of(Props::new({
    let senders = Arc::clone(&senders);
    let replies = Arc::clone(&replies);
    move || ReplyingToSender { senders: Arc::clone(&senders), replies: Arc::clone(&replies) }
  }));
  spec.system_mut().run_until_idle();
  clock.advance(Duration::from_secs(1));
  spec.system_mut().run_until_idle();
  let dead_letters = spec.system().dead_letters();
  let senders = senders.lock().unwrap().clone();
  assert_eq!(senders.len(), 1, "one ReceiveTimeout was handled");
  assert!(senders[0] != actor, "the sender of ReceiveTimeout must not be the actor itself");
  assert!(senders[0] == dead_letters, "the sender of ReceiveTimeout is dead letters (no sender)");
  assert_eq!(replies.load(Ordering::SeqCst), 0, "the reply must not come back to the actor");
  let dead = dead_listener.expect_msg_type::<DeadLetter>();
  assert_eq!(dead.message().downcast_ref::<Reply>(), Some(&Reply), "the reply is the dead letter");
  assert!(*dead.sender() == actor, "the actor sent the reply");
  dead_listener.expect_no_message(millis(300));
  spec.terminate();
}

// ---------------------------------------------------------------------------
// A child's stop and the parent's child-terminated hook

// A parent with a one-second timeout and one child that stops itself on
// "die". With `hook`, its strategy has a child-terminated hook that does
// nothing.
struct ParentOfStoppingChild {
  hook: bool,
  child_slot: Slot,
  deliveries: Deliveries,
}

impl Actor for ParentOfStoppingChild {
  fn pre_start(&mut self, context: &mut ActorContext<'_>) {
    context.set_receive_timeout(finite(Duration::from_secs(1)));
    let child = context.actor_of(Props::new(|| SelfStoppingChild), "child").expect("the child starts");
    *self.child_slot.lock().unwrap() = Some(child);
  }

  fn receive(&mut self, _context: &mut ActorContext<'_>, message: Message) {
    record(&self.deliveries, &message);
  }

  fn supervisor_strategy(&self) -> SupervisorStrategy {
    if self.hook {
      SupervisorStrategy::one_for_one(3).with_child_terminated(|_context, _child, _children| {})
    } else {
      SupervisorStrategy::one_for_one(3)
    }
  }
}

// The parent starts at 10s (deadline 11s), its child stops itself at
// 10.9s, and the parent must get its `ReceiveTimeout` at 11s all the same:
// the child's stop is a system message to the parent, which does not
// influence the timeout.
fn child_stop_keeps_the_parent_deadline(hook: bool) {
  let (mut spec, clock) = spec_with_clock();
  let child_slot = new_slot();
  let deliveries = new_deliveries();
  let parent = spec.system_mut().actor_of(Props::new({
    let child_slot = Arc::clone(&child_slot);
    let deliveries = Arc::clone(&deliveries);
    move || ParentOfStoppingChild { hook, child_slot: Arc::clone(&child_slot), deliveries: Arc::clone(&deliveries) }
  }));
  spec.system_mut().run_until_idle();
  clock.advance(millis(900));
  spec.system_mut().run_until_idle();
  let child = child_slot.lock().unwrap().clone().expect("the child was reported");
  let no_sender = spec.system().dead_letters();
  child.tell("die", no_sender);
  spec.system_mut().run_until_idle();
  assert!(spec.system().is_actor_terminated(&child), "the child stopped");
  assert!(delivered(&deliveries).is_empty(), "the deadline at 11s has not come at 10.9s");
  clock.advance(millis(200));
  spec.system_mut().run_until_idle();
  assert_eq!(
    delivered(&deliveries),
    vec![Duration::from_secs(1)],
    "the child's stop at 10.9s must not move the parent's deadline at 11s"
  );
  assert!(!spec.system().is_actor_terminated(&parent), "the parent is still alive");
  spec.terminate();
}

#[test]
fn child_stop_keeps_the_parent_deadline_with_a_child_terminated_hook() {
  child_stop_keeps_the_parent_deadline(true);
}

#[test]
fn child_stop_keeps_the_parent_deadline_without_a_child_terminated_hook() {
  child_stop_keeps_the_parent_deadline(false);
}

// A parent that applies `initial` when it starts and stops a new child on
// "engage". Its child-terminated hook applies `hook_setting`. Any other
// `&str` is an ordinary message that influences the timeout.
struct HookParent {
  initial: Option<ReceiveTimeoutSetting>,
  hook_setting: ReceiveTimeoutSetting,
  deliveries: Deliveries,
}

impl Actor for HookParent {
  fn pre_start(&mut self, context: &mut ActorContext<'_>) {
    if let Some(setting) = self.initial {
      context.set_receive_timeout(setting);
    }
  }

  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    if record(&self.deliveries, &message) {
      return;
    }
    if message.downcast_ref::<&str>() == Some(&"engage") {
      let child = context.actor_of(empty_props(), "child").expect("the child starts");
      context.stop(&child);
    }
  }

  fn supervisor_strategy(&self) -> SupervisorStrategy {
    let hook_setting = self.hook_setting;
    SupervisorStrategy::one_for_one(3).with_child_terminated(move |context, _child, _children| {
      context.set_receive_timeout(hook_setting);
    })
  }
}

// Starts a `HookParent` at 10s and has it handle "engage" at 10s, so its
// child's stop runs the hook at 10s.
fn engaged_hook_parent(
  initial: Option<ReceiveTimeoutSetting>,
  hook_setting: ReceiveTimeoutSetting,
) -> (FraktorSpec, ControlledClock, ActorRef, Deliveries) {
  let (mut spec, clock) = spec_with_clock();
  let deliveries = new_deliveries();
  let parent = spec.system_mut().actor_of(Props::new({
    let deliveries = Arc::clone(&deliveries);
    move || HookParent { initial, hook_setting, deliveries: Arc::clone(&deliveries) }
  }));
  let no_sender = spec.system().dead_letters();
  parent.tell("engage", no_sender);
  spec.system_mut().run_until_idle();
  (spec, clock, parent, deliveries)
}

// The hook shortens the timeout from 1s to 100ms. `setReceiveTimeout`
// leaves the task scheduled at 11s, and nothing checks the timeout until
// the next message: the 1s deadline fires, and the `ReceiveTimeout(1s)`
// it delivers is the message whose check schedules the 100ms one.
#[test]
fn timeout_changed_in_a_child_terminated_hook_takes_effect_after_the_next_message() {
  let (mut spec, clock, _parent, deliveries) =
    engaged_hook_parent(Some(finite(Duration::from_secs(1))), finite(millis(100)));
  clock.advance(millis(200));
  spec.system_mut().run_until_idle();
  assert!(delivered(&deliveries).is_empty(), "at 10.2s the deadline scheduled for 11s has not come");
  clock.advance(millis(800));
  spec.system_mut().run_until_idle();
  assert_eq!(
    delivered(&deliveries),
    vec![Duration::from_secs(1)],
    "the deadline scheduled before the hook fires at 11s"
  );
  clock.advance(millis(99));
  spec.system_mut().run_until_idle();
  assert_eq!(delivered(&deliveries), vec![Duration::from_secs(1)], "the 100ms setting counts from 11s");
  clock.advance(millis(1));
  spec.system_mut().run_until_idle();
  assert_eq!(
    delivered(&deliveries),
    vec![Duration::from_secs(1), millis(100)],
    "the hook's 100ms setting fires 100ms after the ReceiveTimeout(1s) was handled"
  );
  spec.terminate();
}

// The hook turns the timeout off. The task scheduled at 11s is not
// cancelled by `setReceiveTimeout`, so it fires and its `ReceiveTimeout`
// is delivered; the check after that message cancels the timeout.
#[test]
fn timeout_turned_off_in_a_child_terminated_hook_still_fires_the_scheduled_deadline() {
  let (mut spec, clock, _parent, deliveries) =
    engaged_hook_parent(Some(finite(Duration::from_secs(1))), ReceiveTimeoutSetting::Undefined);
  clock.advance(Duration::from_secs(1));
  spec.system_mut().run_until_idle();
  assert_eq!(
    delivered(&deliveries),
    vec![Duration::from_secs(1)],
    "the deadline scheduled before the hook turned the timeout off fires"
  );
  clock.advance(Duration::from_secs(2));
  spec.system_mut().run_until_idle();
  assert_eq!(delivered(&deliveries), vec![Duration::from_secs(1)], "the timeout is off after that message");
  spec.terminate();
}

// A parent without a timeout sets one in the hook. Nothing schedules it
// until the next message: Pekko's `checkReceiveTimeout` runs only in
// `invoke`, `create`, and `finishRecreate`.
#[test]
fn timeout_set_in_a_child_terminated_hook_is_scheduled_by_the_next_message() {
  let (mut spec, clock, parent, deliveries) = engaged_hook_parent(None, finite(millis(100)));
  clock.advance(millis(200));
  spec.system_mut().run_until_idle();
  assert!(delivered(&deliveries).is_empty(), "the hook's setting is not scheduled before a message comes");
  let no_sender = spec.system().dead_letters();
  parent.tell("poke", no_sender);
  spec.system_mut().run_until_idle();
  clock.advance(millis(99));
  spec.system_mut().run_until_idle();
  assert!(delivered(&deliveries).is_empty(), "the 100ms counts from the message at 10.2s");
  clock.advance(millis(1));
  spec.system_mut().run_until_idle();
  assert_eq!(delivered(&deliveries), vec![millis(100)], "the message at 10.2s scheduled the 100ms setting");
  spec.terminate();
}

// A parent whose child-terminated hook sets a zero timeout and stops the
// parent itself. The hook checks nothing, and the `Terminate` it queued
// cancels the timeout before any message is handled, so no
// `ReceiveTimeout` is fired, delivered, or routed to dead letters.
struct SelfStoppingHookParent {
  deliveries: Deliveries,
}

impl Actor for SelfStoppingHookParent {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    if record(&self.deliveries, &message) {
      return;
    }
    if message.downcast_ref::<&str>() == Some(&"engage") {
      let child = context.actor_of(empty_props(), "child").expect("the child starts");
      context.stop(&child);
    }
  }

  fn supervisor_strategy(&self) -> SupervisorStrategy {
    SupervisorStrategy::one_for_one(3).with_child_terminated(|context, _child, _children| {
      context.set_receive_timeout(finite(Duration::ZERO));
      let me = context.self_ref();
      context.stop(&me);
    })
  }
}

#[test]
fn child_terminated_hook_that_stops_its_actor_fires_no_timeout() {
  let (mut spec, _clock) = spec_with_clock();
  let mut dead_listener = dead_letter_listener(&mut spec);
  let deliveries = new_deliveries();
  let parent = spec.system_mut().actor_of(Props::new({
    let deliveries = Arc::clone(&deliveries);
    move || SelfStoppingHookParent { deliveries: Arc::clone(&deliveries) }
  }));
  let no_sender = spec.system().dead_letters();
  parent.tell("engage", no_sender);
  spec.system_mut().run_until_idle();
  assert!(spec.system().is_actor_terminated(&parent), "the hook's stop took effect");
  assert!(delivered(&deliveries).is_empty(), "no timeout was delivered");
  dead_listener.expect_no_message(millis(300));
  spec.terminate();
}

// A parent that starts one child when it starts. Its child-terminated
// hook sets a zero timeout on the parent itself.
struct HookSetsZeroParent {
  deliveries: Deliveries,
}

impl Actor for HookSetsZeroParent {
  fn pre_start(&mut self, context: &mut ActorContext<'_>) {
    let _child = context.actor_of(empty_props(), "child").expect("the child starts");
  }

  fn receive(&mut self, _context: &mut ActorContext<'_>, message: Message) {
    record(&self.deliveries, &message);
  }

  fn supervisor_strategy(&self) -> SupervisorStrategy {
    SupervisorStrategy::one_for_one(3).with_child_terminated(|context, _child, _children| {
      context.set_receive_timeout(finite(Duration::ZERO));
    })
  }
}

// The parent is stopped from outside: `terminate` has cancelled its
// timeout, it waits for its child, and the hook runs in that wait and sets
// a zero timeout. The hook checks nothing, and no message is handled
// before the stop finishes, so the setting is never scheduled: nothing is
// delivered to the parent or routed to dead letters.
#[test]
fn timeout_set_in_a_child_terminated_hook_while_the_parent_stops_fires_nothing() {
  let (mut spec, _clock) = spec_with_clock();
  let mut dead_listener = dead_letter_listener(&mut spec);
  let deliveries = new_deliveries();
  let parent = spec.system_mut().actor_of(Props::new({
    let deliveries = Arc::clone(&deliveries);
    move || HookSetsZeroParent { deliveries: Arc::clone(&deliveries) }
  }));
  spec.system_mut().run_until_idle();
  spec.system_mut().stop(&parent);
  spec.system_mut().run_until_idle();
  assert!(spec.system().is_actor_terminated(&parent), "the parent stopped");
  assert!(delivered(&deliveries).is_empty(), "no timeout was delivered to the stopping parent");
  dead_listener.expect_no_message(millis(300));
  spec.terminate();
}

// ---------------------------------------------------------------------------
// A restart

// A parent that sets a 100ms timeout when it starts, and starts one child
// whose stop takes 200ms on its first creation only. On "boom" it fails;
// its `pre_restart` turns the timeout off and stops the child.
struct RestartingParent {
  deliveries: Deliveries,
  child_started: Arc<AtomicBool>,
  child_slot: Slot,
  clock: ControlledClock,
}

impl Actor for RestartingParent {
  fn pre_start(&mut self, context: &mut ActorContext<'_>) {
    context.set_receive_timeout(finite(millis(100)));
    if !self.child_started.swap(true, Ordering::SeqCst) {
      let child = context.actor_of(slow_stopping_props(&self.clock, millis(200)), "slow").expect("the child starts");
      *self.child_slot.lock().unwrap() = Some(child);
    }
  }

  fn pre_restart(&mut self, context: &mut ActorContext<'_>, _reason: &(dyn core::any::Any + Send)) {
    context.set_receive_timeout(ReceiveTimeoutSetting::Undefined);
    if let Some(child) = self.child_slot.lock().unwrap().clone() {
      context.stop(&child);
    }
  }

  fn receive(&mut self, _context: &mut ActorContext<'_>, message: Message) {
    if record(&self.deliveries, &message) {
      return;
    }
    if message.downcast_ref::<&str>() == Some(&"boom") {
      panic!("boom");
    }
  }
}

// The failing message at 10s leaves the task scheduled at 10.1s. The
// `pre_restart` that turns the timeout off does not cancel it: in Pekko
// `setReceiveTimeout` only writes the setting and the version
// (`ReceiveTimeout.scala` lines 39-47), and `pre_restart` runs inside
// `faultRecreate`, from `systemInvoke` (`ActorCell.scala` lines 490-555),
// which checks nothing. The next check for this actor is at 10.2s, in
// `finishRecreate` (`FaultHandling.scala` line 289).
//
// Whether 10.1s comes before that check is a race in Pekko, and nothing
// guarantees which side runs first. If the timer thread
// (`LightArrayRevolverScheduler.scala` lines 340-356) extracts the task and
// sends it (lines 403-411) first, the deadline is delivered; if the check
// runs first, the reschedule below replaces the old deadline and it never
// arrives. The test's layout, a failure at 10.0s, a deadline at 10.1s and a
// restart that ends at 10.2s, gives the timer thread a 100ms margin (a tick
// is about 10ms, and `LightArrayRevolverScheduler.scala` lines 181-186
// round a delay up to one), so an ordinary scheduler is likely to run the
// timer thread first. That is an expectation, not a guarantee: a margin
// under one tick still delivers if the task is extracted in it, and a small
// margin can go either way.
//
// The `pre_start` of the restarted actor sets the 100ms again (Pekko's
// `postRestart` calls `preStart`: `Actor.scala` lines 645-647), so at the
// check the setting is not `Undefined` but `Finite(100ms)`: the check
// cancels nothing and reschedules the timeout for 10.3s (`reschedule =
// true` at `FaultHandling.scala` line 289, `ReceiveTimeout.scala` lines
// 56-72). That new deadline is past the 10.2s point the test reads, so the
// one message in `deliveries` is the old 10.1s one.
//
// The `ReceiveTimeout(100ms)` fired while the restart waited for the child.
// The failure suspends the mailbox (`FaultHandling.scala` line 218) and
// only `finishRecreate` resumes it (line 283), but enqueue does not look at
// the status (`Mailbox.scala` line 106), so it is accepted: the message
// waits in the suspended mailbox and is handled after the resume, in the
// same run. Cancelling a task after it fired cannot take the message back
// either: the scheduler's `cancel` fails once the task is extracted
// (`LightArrayRevolverScheduler.scala` lines 418-425), and
// `cancelReceiveTimeoutTask` only stops the `Cancellable`
// (`ReceiveTimeout.scala` lines 95-102).
//
// The port has no scheduler thread of its own: a deadline that has come
// fires at a drive point (`run_until_idle`, `finish_run`,
// `await_envelope`), which sends the `ReceiveTimeout` into the mailbox. The
// hand-driven clock makes the drive points, not a thread race, decide the
// order: the child's stop and its `finish_run` come before the parent's
// `ChildTerminated` run, and the deadline fires before the restart ends.
// That order, not Pekko's race, is what the test pins.
#[test]
fn timeout_turned_off_in_pre_restart_still_delivers_a_deadline_that_came_during_the_restart() {
  let (mut spec, clock) = spec_with_clock();
  let deliveries = new_deliveries();
  let parent = spec.system_mut().actor_of(Props::new({
    let deliveries = Arc::clone(&deliveries);
    let clock = clock.clone();
    let child_started = Arc::new(AtomicBool::new(false));
    let child_slot = new_slot();
    move || RestartingParent {
      deliveries: Arc::clone(&deliveries),
      child_started: Arc::clone(&child_started),
      child_slot: Arc::clone(&child_slot),
      clock: clock.clone(),
    }
  }));
  spec.system_mut().run_until_idle();
  let no_sender = spec.system().dead_letters();
  parent.tell("boom", no_sender);
  spec.system_mut().run_until_idle();
  assert!(!spec.system().is_actor_terminated(&parent), "the parent restarted");
  assert_eq!(
    delivered(&deliveries),
    vec![millis(100)],
    "the deadline that came while the restart waited for the child is delivered to the restarted actor"
  );
  spec.terminate();
}

// ---------------------------------------------------------------------------
// A stop the actor asks for itself

// An actor that, on "go", stops itself and sets `setting` in the same
// handling: the setting first when `set_first`, the stop first otherwise.
// With `slow_child`, it has a child whose stop takes one second, so the
// stop waits a second for it.
struct SelfStopWithTimeout {
  setting: ReceiveTimeoutSetting,
  set_first: bool,
  slow_child: Option<ControlledClock>,
  deliveries: Deliveries,
}

impl Actor for SelfStopWithTimeout {
  fn pre_start(&mut self, context: &mut ActorContext<'_>) {
    if let Some(clock) = &self.slow_child {
      let _child =
        context.actor_of(slow_stopping_props(clock, Duration::from_secs(1)), "slow").expect("the child starts");
    }
  }

  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    if record(&self.deliveries, &message) {
      return;
    }
    if message.downcast_ref::<&str>() == Some(&"go") {
      let me = context.self_ref();
      if self.set_first {
        context.set_receive_timeout(self.setting);
        context.stop(&me);
      } else {
        context.stop(&me);
        context.set_receive_timeout(self.setting);
      }
    }
  }
}

// The check after "go" schedules the 100ms timeout, and handling the
// `Terminate` cancels it (`FaultHandling.terminate`). The deadline comes
// while the stop waits for the child, but nothing is left to fire, so no
// `ReceiveTimeout` reaches the closing mailbox or dead letters.
#[test]
fn timeout_set_after_a_self_stop_is_cancelled_when_the_stop_is_handled() {
  let (mut spec, clock) = spec_with_clock();
  let mut dead_listener = dead_letter_listener(&mut spec);
  let deliveries = new_deliveries();
  let actor = spec.system_mut().actor_of(Props::new({
    let deliveries = Arc::clone(&deliveries);
    let clock = clock.clone();
    move || SelfStopWithTimeout {
      setting: finite(millis(100)),
      set_first: false,
      slow_child: Some(clock.clone()),
      deliveries: Arc::clone(&deliveries),
    }
  }));
  spec.system_mut().run_until_idle();
  let no_sender = spec.system().dead_letters();
  actor.tell("go", no_sender);
  spec.system_mut().run_until_idle();
  assert!(spec.system().is_actor_terminated(&actor), "the actor stopped");
  assert!(delivered(&deliveries).is_empty(), "no timeout was delivered");
  dead_listener.expect_no_message(millis(300));
  spec.terminate();
}

// A zero timeout is sent at once and cannot be cancelled: the scheduler
// runs a task of zero delay immediately and returns `NotCancellable`. The
// `ReceiveTimeout(0)` reaches the mailbox the `Terminate` closes and goes
// to dead letters, exactly once, without a sender, whichever of the two
// calls the handling makes first: the check after the handling schedules
// it either way, and the `Terminate` is handled after that.
fn zero_timeout_with_a_self_stop_goes_to_dead_letters_once(set_first: bool) {
  let (mut spec, _clock) = spec_with_clock();
  let mut dead_listener = dead_letter_listener(&mut spec);
  let deliveries = new_deliveries();
  let actor = spec.system_mut().actor_of(Props::new({
    let deliveries = Arc::clone(&deliveries);
    move || SelfStopWithTimeout {
      setting: finite(Duration::ZERO),
      set_first,
      slow_child: None,
      deliveries: Arc::clone(&deliveries),
    }
  }));
  spec.system_mut().run_until_idle();
  let no_sender = spec.system().dead_letters();
  actor.tell("go", no_sender);
  spec.system_mut().run_until_idle();
  assert!(spec.system().is_actor_terminated(&actor), "the actor stopped");
  assert!(delivered(&deliveries).is_empty(), "the stopping actor does not handle the ReceiveTimeout");
  let dead = dead_listener.expect_msg_type::<DeadLetter>();
  assert_eq!(
    dead.message().downcast_ref::<ReceiveTimeout>().map(ReceiveTimeout::timeout),
    Some(Duration::ZERO),
    "the dead letter is the ReceiveTimeout(0)"
  );
  assert!(*dead.recipient() == actor, "it was addressed to the stopped actor");
  assert!(*dead.sender() == spec.system().dead_letters(), "it has no sender");
  dead_listener.expect_no_message(millis(300));
  spec.terminate();
}

#[test]
fn zero_timeout_set_before_a_self_stop_goes_to_dead_letters_once() {
  zero_timeout_with_a_self_stop_goes_to_dead_letters_once(true);
}

#[test]
fn zero_timeout_set_after_a_self_stop_goes_to_dead_letters_once() {
  zero_timeout_with_a_self_stop_goes_to_dead_letters_once(false);
}

// An actor with a one-second timeout that stops itself on "go" and reads
// its timeout right after.
struct ReadsTimeoutAfterStop {
  read: Arc<Mutex<Option<ReceiveTimeoutSetting>>>,
}

impl Actor for ReadsTimeoutAfterStop {
  fn pre_start(&mut self, context: &mut ActorContext<'_>) {
    context.set_receive_timeout(finite(Duration::from_secs(1)));
  }

  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    if message.downcast_ref::<&str>() == Some(&"go") {
      let me = context.self_ref();
      context.stop(&me);
      *self.read.lock().unwrap() = Some(context.receive_timeout());
    }
  }
}

// `context.stop(self)` only queues the `Terminate`; the setting is
// cancelled when it is handled, so the handling still reads 1s.
#[test]
fn receive_timeout_keeps_its_setting_until_the_stop_is_handled() {
  let (mut spec, _clock) = spec_with_clock();
  let read = Arc::new(Mutex::new(None));
  let actor = spec.system_mut().actor_of(Props::new({
    let read = Arc::clone(&read);
    move || ReadsTimeoutAfterStop { read: Arc::clone(&read) }
  }));
  spec.system_mut().run_until_idle();
  let no_sender = spec.system().dead_letters();
  actor.tell("go", no_sender);
  spec.system_mut().run_until_idle();
  assert!(spec.system().is_actor_terminated(&actor), "the actor stopped");
  assert_eq!(
    *read.lock().unwrap(),
    Some(finite(Duration::from_secs(1))),
    "the setting is still 1s in the handling that asked for the stop"
  );
  spec.terminate();
}

// A child with a 100ms timeout whose own stop takes 200ms.
struct TimedSlowChild {
  clock: ControlledClock,
  deliveries: Deliveries,
}

impl Actor for TimedSlowChild {
  fn pre_start(&mut self, context: &mut ActorContext<'_>) {
    context.set_receive_timeout(finite(millis(100)));
  }

  fn receive(&mut self, _context: &mut ActorContext<'_>, message: Message) {
    record(&self.deliveries, &message);
  }

  fn post_stop(&mut self) {
    self.clock.advance(millis(200));
  }
}

// The parent of `TimedSlowChild`, with a child-terminated hook that does
// nothing.
struct TimedChildHolder {
  clock: ControlledClock,
  child_slot: Slot,
  child_deliveries: Deliveries,
}

impl Actor for TimedChildHolder {
  fn pre_start(&mut self, context: &mut ActorContext<'_>) {
    let clock = self.clock.clone();
    let deliveries = Arc::clone(&self.child_deliveries);
    let child = context
      .actor_of(
        Props::new(move || TimedSlowChild { clock: clock.clone(), deliveries: Arc::clone(&deliveries) }),
        "timed-child",
      )
      .expect("the child starts");
    *self.child_slot.lock().unwrap() = Some(child);
  }

  fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {}

  fn supervisor_strategy(&self) -> SupervisorStrategy {
    SupervisorStrategy::one_for_one(3).with_child_terminated(|_context, _child, _children| {})
  }
}

// The child is stopped at 10s, before its deadline at 10.1s, and its
// `post_stop` moves the clock to 10.2s. `terminate` cancelled the timeout
// before `postStop` ran, so the deadline that comes during the stop fires
// nothing: not to the child, and not to dead letters when the parent's
// hook runs.
#[test]
fn deadline_that_comes_while_the_actor_stops_fires_nothing() {
  let (mut spec, clock) = spec_with_clock();
  let mut dead_listener = dead_letter_listener(&mut spec);
  let child_slot = new_slot();
  let child_deliveries = new_deliveries();
  let _parent = spec.system_mut().actor_of(Props::new({
    let clock = clock.clone();
    let child_slot = Arc::clone(&child_slot);
    let child_deliveries = Arc::clone(&child_deliveries);
    move || TimedChildHolder {
      clock: clock.clone(),
      child_slot: Arc::clone(&child_slot),
      child_deliveries: Arc::clone(&child_deliveries),
    }
  }));
  spec.system_mut().run_until_idle();
  let child = child_slot.lock().unwrap().clone().expect("the child was reported");
  spec.system_mut().stop(&child);
  spec.system_mut().run_until_idle();
  assert!(spec.system().is_actor_terminated(&child), "the child stopped");
  assert_eq!(clock.monotonic(), Duration::from_millis(10_200), "the child's stop moved the clock past its deadline");
  assert!(delivered(&child_deliveries).is_empty(), "the deadline was not delivered to the child");
  dead_listener.expect_no_message(millis(300));
  spec.terminate();
}

// ---------------------------------------------------------------------------
// `Kill`, and a `Terminated` after the watch was removed

// A child with a one-second timeout. On its first start it starts a
// grandchild whose stop takes 700ms, so a restart of this child waits 700ms
// for the grandchild to go (the default `pre_restart` stops the children).
struct KilledChild {
  clock: ControlledClock,
  grandchild_started: Arc<AtomicBool>,
  deliveries: Deliveries,
}

impl Actor for KilledChild {
  fn pre_start(&mut self, context: &mut ActorContext<'_>) {
    context.set_receive_timeout(finite(Duration::from_secs(1)));
    if !self.grandchild_started.swap(true, Ordering::SeqCst) {
      let _grandchild =
        context.actor_of(slow_stopping_props(&self.clock, millis(700)), "slow").expect("the grandchild starts");
    }
  }

  fn receive(&mut self, _context: &mut ActorContext<'_>, message: Message) {
    record(&self.deliveries, &message);
  }
}

// A parent that starts a `KilledChild` and restarts it on every failure,
// `Kill` included (the default decider stops a killed child).
struct RestartingOnKillParent {
  clock: ControlledClock,
  child_slot: Slot,
  child_deliveries: Deliveries,
}

impl Actor for RestartingOnKillParent {
  fn pre_start(&mut self, context: &mut ActorContext<'_>) {
    let clock = self.clock.clone();
    let deliveries = Arc::clone(&self.child_deliveries);
    let grandchild_started = Arc::new(AtomicBool::new(false));
    let child = context
      .actor_of(
        Props::new(move || KilledChild {
          clock: clock.clone(),
          grandchild_started: Arc::clone(&grandchild_started),
          deliveries: Arc::clone(&deliveries),
        }),
        "killed-child",
      )
      .expect("the child starts");
    *self.child_slot.lock().unwrap() = Some(child);
  }

  fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {}

  fn supervisor_strategy(&self) -> SupervisorStrategy {
    SupervisorStrategy::one_for_one(3).with_decider(|_child, _failure| Directive::Restart)
  }
}

// `Kill` is an auto-received message that influences the timeout: `invoke`
// cancels the task before it and, in its `finally`, schedules it again
// after the `ActorKilledException`, so at 10.5s the deadline moves from 11s
// to 11.5s. The restart then waits for the grandchild until 11.2s, before
// that deadline, and `finishRecreate` schedules the timeout from 11.2s.
// Nothing comes due in the wait, and the next `ReceiveTimeout` is at 12.2s.
// (The port has no `Resume` directive, where the moved deadline would show
// without a wait.)
#[test]
fn kill_moves_the_deadline_before_the_restart_it_causes() {
  let (mut spec, clock) = spec_with_clock();
  let child_slot = new_slot();
  let child_deliveries = new_deliveries();
  let _parent = spec.system_mut().actor_of(Props::new({
    let clock = clock.clone();
    let child_slot = Arc::clone(&child_slot);
    let child_deliveries = Arc::clone(&child_deliveries);
    move || RestartingOnKillParent {
      clock: clock.clone(),
      child_slot: Arc::clone(&child_slot),
      child_deliveries: Arc::clone(&child_deliveries),
    }
  }));
  spec.system_mut().run_until_idle();
  let child = child_slot.lock().unwrap().clone().expect("the child was reported");
  clock.advance(millis(500));
  let no_sender = spec.system().dead_letters();
  child.tell("Kill", no_sender);
  spec.system_mut().run_until_idle();
  assert!(!spec.system().is_actor_terminated(&child), "the parent restarted the child");
  assert_eq!(clock.monotonic(), Duration::from_millis(11_200), "the restart waited for the grandchild until 11.2s");
  assert!(
    delivered(&child_deliveries).is_empty(),
    "the deadline the Kill moved to 11.5s did not come while the restart waited until 11.2s"
  );
  clock.advance(millis(999));
  spec.system_mut().run_until_idle();
  assert!(delivered(&child_deliveries).is_empty(), "the restart scheduled the timeout from 11.2s");
  clock.advance(millis(1));
  spec.system_mut().run_until_idle();
  assert_eq!(delivered(&child_deliveries), vec![Duration::from_secs(1)], "the timeout fires at 12.2s");
  spec.terminate();
}

// A message that does not influence the timeout, on which the watcher
// removes its watch.
#[derive(Clone, Copy, Debug)]
struct Unwatch;

impl NotInfluenceReceiveTimeout for Unwatch {}

// An actor whose `post_stop` tells the watcher `Unwatch`, so the watcher
// has the `Unwatch` in its mailbox before the `Terminated` of this actor.
struct UnwatchedOnStop {
  watcher_slot: Slot,
}

impl Actor for UnwatchedOnStop {
  fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {}

  fn post_stop_with_context(&mut self, context: &mut ActorContext<'_>) {
    if let Some(watcher) = self.watcher_slot.lock().unwrap().clone() {
      context.tell(&watcher, Unwatch);
    }
  }
}

// A watcher with a one-second timeout. It watches `target` when it starts
// and removes the watch on `Unwatch`.
struct Watcher {
  target: ActorRef,
  terminated: Arc<AtomicUsize>,
  deliveries: Deliveries,
}

impl Actor for Watcher {
  fn pre_start(&mut self, context: &mut ActorContext<'_>) {
    context.set_receive_timeout(finite(Duration::from_secs(1)));
    let target = self.target.clone();
    context.watch(&target);
  }

  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    if record(&self.deliveries, &message) {
      return;
    }
    if message.downcast_ref::<Unwatch>().is_some() {
      let target = self.target.clone();
      context.unwatch(&target);
      return;
    }
    self.terminated.fetch_add(1, Ordering::SeqCst);
  }
}

// The watched actor stops at 10.5s. Its `Terminated` is already in the
// watcher's mailbox when the watcher removes the watch, so the watcher's
// `receive` never sees it (`DeathWatch.receivedTerminated` finds nothing
// queued). It is still an auto-received message that `invoke` handles,
// and it influences the timeout: the deadline counts from 10.5s.
#[test]
fn terminated_handled_after_the_watch_was_removed_restarts_the_timeout() {
  let (mut spec, clock) = spec_with_clock();
  spec.system().register_not_influence_receive_timeout::<Unwatch>();
  let watcher_slot = new_slot();
  let target = spec.system_mut().actor_of(Props::new({
    let watcher_slot = Arc::clone(&watcher_slot);
    move || UnwatchedOnStop { watcher_slot: Arc::clone(&watcher_slot) }
  }));
  let terminated = Arc::new(AtomicUsize::new(0));
  let deliveries = new_deliveries();
  let watcher = spec.system_mut().actor_of(Props::new({
    let target = target.clone();
    let terminated = Arc::clone(&terminated);
    let deliveries = Arc::clone(&deliveries);
    move || Watcher { target: target.clone(), terminated: Arc::clone(&terminated), deliveries: Arc::clone(&deliveries) }
  }));
  *watcher_slot.lock().unwrap() = Some(watcher);
  spec.system_mut().run_until_idle();
  clock.advance(millis(500));
  spec.system_mut().stop(&target);
  spec.system_mut().run_until_idle();
  assert!(spec.system().is_actor_terminated(&target), "the watched actor stopped");
  assert_eq!(terminated.load(Ordering::SeqCst), 0, "the removed watch hides the Terminated from receive");
  clock.advance(millis(700));
  spec.system_mut().run_until_idle();
  assert!(delivered(&deliveries).is_empty(), "at 11.2s the deadline counted from the Terminated at 10.5s has not come");
  clock.advance(millis(300));
  spec.system_mut().run_until_idle();
  assert_eq!(delivered(&deliveries), vec![Duration::from_secs(1)], "the timeout fires at 11.5s");
  spec.terminate();
}

// An actor whose `receive` records the `ReceiveTimeout` it gets.
struct PoisonPillTarget {
  deliveries: Deliveries,
}

impl Actor for PoisonPillTarget {
  fn pre_start(&mut self, context: &mut ActorContext<'_>) {
    context.set_receive_timeout(finite(Duration::ZERO));
  }

  fn receive(&mut self, _context: &mut ActorContext<'_>, message: Message) {
    record(&self.deliveries, &message);
  }
}

// `PoisonPill` is an auto-received message that influences the timeout, so
// `ActorCell.invoke` (lines 568-585) runs its three calls around it like any
// other: `cancelReceiveTimeoutIfNeeded`, `autoReceiveMessage` (which puts the
// `Terminate` on the system queue) and, in the `finally`,
// `checkReceiveTimeout`. A zero timeout is sent at once and cannot be
// cancelled (`LightArrayRevolverScheduler.schedule`, lines 223-227), so the
// `ReceiveTimeout(0)` is addressed to the actor that is stopping and reaches
// it only after `FaultHandling.terminate` has closed the mailbox: it goes to
// dead letters, exactly once. The `PoisonPill` waits in the new mailbox
// before the actor's first run, so it is the first message `invoke` handles
// and `receive` never sees it; no run is left to loop on the timeout the
// check schedules again.
#[test]
fn poison_pill_fires_the_due_receive_timeout_into_dead_letters() {
  let (mut spec, _clock) = spec_with_clock();
  let mut dead_listener = dead_letter_listener(&mut spec);
  let deliveries = new_deliveries();
  let actor = spec.system_mut().actor_of(Props::new({
    let deliveries = Arc::clone(&deliveries);
    move || PoisonPillTarget { deliveries: Arc::clone(&deliveries) }
  }));
  let no_sender = spec.system().dead_letters();
  actor.tell("PoisonPill", no_sender);
  spec.system_mut().run_until_idle();
  assert!(spec.system().is_actor_terminated(&actor), "the PoisonPill stopped the actor");
  let dead = dead_listener.expect_msg_type::<DeadLetter>();
  assert_eq!(
    dead.message().downcast_ref::<ReceiveTimeout>().map(ReceiveTimeout::timeout),
    Some(Duration::ZERO),
    "the dead letter is the ReceiveTimeout(0) the check after the PoisonPill scheduled"
  );
  assert!(*dead.recipient() == actor, "it was addressed to the stopping actor");
  assert!(*dead.sender() == spec.system().dead_letters(), "it has no sender");
  dead_listener.expect_no_message(millis(300));
  assert!(
    delivered(&deliveries).is_empty(),
    "the actor handed neither the PoisonPill nor the ReceiveTimeout to receive"
  );
  spec.terminate();
}
