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

//! Unit tests for stage 3 of #234 (design, section 11.11.8): the count of
//! condition 1, the release of condition 4, the wait on the wake signal, the
//! work a run leaves, the stop of a running actor, and a panic that no
//! catcher catches.

use std::panic;
use std::string::String;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;
use std::time::Instant;
use std::vec::Vec;

use crate::adaptor::ChannelBoundary;
use crate::adaptor::UnwindCatcher;
use crate::kernel::actor::Actor;
use crate::kernel::actor::ActorContext;
use crate::kernel::actor::DeadLetter;
use crate::kernel::actor::Props;
use crate::kernel::actor_ref::ActorRef;
use crate::kernel::actor_system::ActorSystem;
use crate::kernel::event::EventClass;
use crate::kernel::executor::Executor;
use crate::kernel::message::Message;
use crate::kernel::sync::counts;
use crate::kernel::sync::counts::Counts;
use crate::kernel::sync::Arc;

fn system(name: &str) -> ActorSystem {
  ActorSystem::new(name, Arc::new(ChannelBoundary::new()))
}

fn with_catcher(name: &str) -> ActorSystem {
  ActorSystem::with_failure_catcher(name, Arc::new(ChannelBoundary::new()), Arc::new(UnwindCatcher))
}

type Log = Arc<Mutex<Vec<String>>>;

fn new_log() -> Log {
  Arc::new(Mutex::new(Vec::new()))
}

fn entries(log: &Log) -> Vec<String> {
  log.lock().unwrap().clone()
}

// Records every `&str` it receives.
struct Recorder {
  log: Log,
  label: &'static str,
}

impl Actor for Recorder {
  fn receive(&mut self, _context: &mut ActorContext<'_>, message: Message) {
    if let Some(text) = message.downcast_ref::<&'static str>() {
      self.log.lock().unwrap().push(std::format!("{}:{text}", self.label));
    }
  }
}

fn recorder(log: &Log, label: &'static str) -> Props {
  let log = Arc::clone(log);
  Props::new(move || Recorder { log: Arc::clone(&log), label })
}

// Collects the payloads of the dead letters the system publishes.
fn dead_letters_of(system: &mut ActorSystem) -> Log {
  let collected = new_log();
  let sink = Arc::clone(&collected);
  let collector = system.function_ref("dead-letter-collector", move |_: &ActorRef, message: Message| {
    if let Some(letter) = message.downcast_ref::<DeadLetter>() {
      if let Some(text) = letter.message().downcast_ref::<&'static str>() {
        sink.lock().unwrap().push(String::from(*text));
      }
    }
  });
  system.event_stream().subscribe(Some(collector), EventClass::of::<DeadLetter>()).expect("a collector");
  collected
}

// ----- condition 1: the Arc clones and atomic operations per message

const N: u64 = 1000;

struct Sink;

impl Actor for Sink {
  fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {}
}

fn report(row: &str, used: Counts, messages: u64) {
  std::eprintln!(
    "condition 1, {row}: {} Arc clones and {} atomic increments for {messages} messages \
     ({:.3} and {:.3} per message; paths {}, mailbox endpoints {}, boundary endpoints {}, runs {}, \
     upgrades {}; {:.3} locks per message)",
    used.arc_clones(),
    used.atomic_ops(),
    used.arc_clones() as f64 / messages as f64,
    used.atomic_ops() as f64 / messages as f64,
    used.paths,
    used.mailbox_endpoints,
    used.boundary_endpoints,
    used.runs,
    used.upgrades,
    used.locks as f64 / messages as f64,
  );
}

// S1: messages from outside to one actor, then `run_until_idle`. Only the
// first send wins the reservation.
#[test]
fn sends_that_do_not_win_the_reservation_clone_only_the_sender() {
  let mut system = system("ConditionOneS1");
  let target = system.actor_of(Props::new(|| Sink));
  let sender = system.dead_letters();
  let strong_before = target.mailbox_strong_count();

  let before = counts::snapshot();
  for _ in 0..N {
    target.tell(1_u32, sender.clone());
  }
  system.run_until_idle();
  let used = counts::snapshot().since(before);

  report("S1, from outside then run_until_idle", used, N);
  assert_eq!(used.arc_clones(), 2 * N + 1, "{used:?}");
  assert_eq!(used.atomic_ops(), 2 * N + 2, "{used:?}");
  assert_eq!(target.mailbox_strong_count(), strong_before, "a clone of the mailbox leaked");
}

// Sends to itself while it runs, so its own send never wins the reservation.
struct SelfSender {
  me: Arc<Mutex<Option<ActorRef>>>,
  left: u64,
}

impl Actor for SelfSender {
  fn receive(&mut self, context: &mut ActorContext<'_>, _message: Message) {
    if self.left > 0 {
      self.left -= 1;
      let me = self.me.lock().unwrap();
      context.tell(me.as_ref().expect("the handle is set before the first message"), 1_u32);
    }
  }
}

// S3: an actor that sends to itself, driven by `run_one` as the testkit does.
#[test]
fn a_send_to_itself_under_run_one_clones_only_the_sender() {
  let mut system = system("ConditionOneS3");
  let me = Arc::new(Mutex::new(None));
  let slot = Arc::clone(&me);
  let actor = system.actor_of(Props::new(move || SelfSender { me: Arc::clone(&slot), left: N }));
  *me.lock().unwrap() = Some(actor.clone());
  // Taken while no run waits: after the first message, one does.
  let strong_before = actor.mailbox_strong_count();
  actor.tell(0_u32, system.dead_letters());
  assert!(system.run_one(), "the first message");

  let before = counts::snapshot();
  let mut handled = 0;
  while system.run_one() {
    handled += 1;
  }
  let used = counts::snapshot().since(before);

  assert_eq!(handled, N);
  // The first send to itself happened in the first `run_one`, before the
  // count started.
  let sends = N - 1;
  report("S3, to itself under run_one", used, sends);
  assert_eq!(used.arc_clones(), 2 * sends, "{used:?}");
  assert_eq!(used.atomic_ops(), 2 * sends, "{used:?}");
  assert_eq!(actor.mailbox_strong_count(), strong_before, "a clone of the mailbox leaked");
  *me.lock().unwrap() = None;
}

// The testkit's way: one message from outside, then `run_one`, so every send
// finds the actor idle and wins the reservation.
#[test]
fn a_send_that_wins_the_reservation_clones_the_run_once() {
  let mut system = system("ConditionOneTestkit");
  let target = system.actor_of(Props::new(|| Sink));
  let sender = system.dead_letters();
  let strong_before = target.mailbox_strong_count();

  let before = counts::snapshot();
  for _ in 0..N {
    target.tell(1_u32, sender.clone());
    assert!(system.run_one());
  }
  let used = counts::snapshot().since(before);

  report("testkit, one from outside then run_one", used, N);
  assert_eq!(used.arc_clones(), 3 * N, "{used:?}");
  assert_eq!(used.atomic_ops(), 4 * N, "{used:?}");
  assert_eq!(target.mailbox_strong_count(), strong_before, "a clone of the mailbox leaked");
}

// Sends every message it receives back to its peer, until the count runs out.
struct Bouncer {
  peer: Arc<Mutex<Option<ActorRef>>>,
  left: Arc<AtomicUsize>,
}

impl Actor for Bouncer {
  fn receive(&mut self, context: &mut ActorContext<'_>, _message: Message) {
    if self.left.load(Ordering::SeqCst) > 0 {
      self.left.fetch_sub(1, Ordering::SeqCst);
      let peer = self.peer.lock().unwrap();
      context.tell(peer.as_ref().expect("the peer is set"), 1_u32);
    }
  }
}

// S4: two actors send to each other, so each send finds its peer idle.
#[test]
fn two_actors_that_answer_each_other_win_every_reservation() {
  let mut system = system("ConditionOneS4");
  let left = Arc::new(AtomicUsize::new(0));
  let to_b = Arc::new(Mutex::new(None));
  let to_a = Arc::new(Mutex::new(None));
  let (peer, count) = (Arc::clone(&to_b), Arc::clone(&left));
  let a = system.actor_of(Props::new(move || Bouncer { peer: Arc::clone(&peer), left: Arc::clone(&count) }));
  let (peer, count) = (Arc::clone(&to_a), Arc::clone(&left));
  let b = system.actor_of(Props::new(move || Bouncer { peer: Arc::clone(&peer), left: Arc::clone(&count) }));
  *to_b.lock().unwrap() = Some(b.clone());
  *to_a.lock().unwrap() = Some(a.clone());
  let strong_before = (a.mailbox_strong_count(), b.mailbox_strong_count());
  left.store(N as usize, Ordering::SeqCst);
  a.tell(0_u32, b.clone());
  system.run_one();

  let before = counts::snapshot();
  system.run_until_idle();
  let used = counts::snapshot().since(before);

  assert_eq!(left.load(Ordering::SeqCst), 0);
  // The first bounce was sent in `run_one`, before the count started.
  let bounces = N - 1;
  report("S4, two actors answering each other", used, bounces);
  assert_eq!(used.arc_clones(), 3 * bounces, "{used:?}");
  assert_eq!(used.atomic_ops(), 4 * bounces, "{used:?}");
  assert_eq!((a.mailbox_strong_count(), b.mailbox_strong_count()), strong_before, "a clone leaked");
  *to_a.lock().unwrap() = None;
  *to_b.lock().unwrap() = None;
}

// The refused path: messages to a stopped actor take its handle along to the
// executor, which needs the way back. Condition 1 is not met here (design of
// #234, section 11.11.7).
#[test]
fn a_send_to_a_stopped_actor_clones_four_and_walks_back_once() {
  let mut system = system("ConditionOneRefused");
  let target = system.actor_of(Props::new(|| Sink));
  let sender = system.dead_letters();
  system.stop(&target);
  system.run_until_idle();

  let before = counts::snapshot();
  for _ in 0..N {
    target.tell(1_u32, sender.clone());
  }
  let sent = counts::snapshot().since(before);
  system.run_until_idle();
  let routed = counts::snapshot().since(before);

  report("refused, to a stopped actor (the tell)", sent, N);
  report("refused, the tell and the routing to dead letters", routed, N);
  assert_eq!(sent.arc_clones(), 4 * N, "{sent:?}");
  assert_eq!(sent.atomic_ops(), 5 * N, "{sent:?}");
}

// ----- condition 4: the mailbox goes once the actor stops, or the system ends

#[test]
fn a_stopped_actor_releases_its_mailbox_once_its_handles_are_gone() {
  let mut system = system("ReleaseOnStop");
  let actor = system.actor_of(Props::new(|| Sink));
  let mailbox = actor.mailbox_weak();
  actor.tell(1_u32, actor.clone());
  actor.tell(2_u32, actor.clone());

  system.stop(&actor);
  drop(actor);
  system.run_until_idle();

  assert!(mailbox.upgrade().is_none(), "the stopped actor's mailbox is still held");
}

struct SetPeer(ActorRef);

// Holds the handle of its peer, so two of them hold each other's mailboxes.
struct PeerHolder {
  peer: Option<ActorRef>,
}

impl Actor for PeerHolder {
  fn receive(&mut self, _context: &mut ActorContext<'_>, message: Message) {
    if let Some(SetPeer(peer)) = message.downcast_ref::<SetPeer>() {
      self.peer = Some(peer.clone());
    }
  }
}

#[test]
fn terminating_releases_mailboxes_that_hold_each_other_and_queued_messages() {
  let mut system = system("ReleaseOnTerminate");
  let a = system.actor_of(Props::new(|| PeerHolder { peer: None }));
  let b = system.actor_of(Props::new(|| PeerHolder { peer: None }));
  a.tell(SetPeer(b.clone()), b.clone());
  b.tell(SetPeer(a.clone()), a.clone());
  system.run_until_idle();
  a.tell(1_u32, b.clone());
  b.tell(2_u32, a.clone());
  let mailboxes = [a.mailbox_weak(), b.mailbox_weak()];

  system.terminate();
  drop(a);
  drop(b);

  assert!(mailboxes.iter().all(|mailbox| mailbox.upgrade().is_none()), "a mailbox is still held after terminate");
}

// ----- the wake signal

#[test]
fn a_send_from_another_thread_ends_the_wait_at_once() {
  let mut system = system("WaitForASend");
  let actor = system.actor_of(Props::new(|| Sink));
  let sender = system.dead_letters();
  let started = Instant::now();
  let sending = thread::spawn(move || {
    thread::sleep(Duration::from_millis(50));
    actor.tell(1_u32, sender);
  });

  assert!(system.await_envelope(Duration::from_secs(5)));
  let waited = started.elapsed();
  sending.join().unwrap();
  std::eprintln!("await_envelope returned {waited:?} after a send 50ms in");
  assert!(waited < Duration::from_secs(2), "the wait slept past the send: {waited:?}");
  assert!(system.run_one());
}

#[test]
fn a_message_to_a_function_reference_from_another_thread_ends_the_wait() {
  let mut system = system("WaitForATestActor");
  let received = Arc::new(AtomicUsize::new(0));
  let counter = Arc::clone(&received);
  let probe = system.function_ref("probe", move |_: &ActorRef, _: Message| {
    counter.fetch_add(1, Ordering::SeqCst);
  });
  let sender = system.dead_letters();
  let started = Instant::now();
  let sending = thread::spawn(move || {
    thread::sleep(Duration::from_millis(50));
    probe.tell(1_u32, sender);
  });

  assert!(system.await_envelope(Duration::from_secs(5)));
  let waited = started.elapsed();
  sending.join().unwrap();
  assert!(waited < Duration::from_secs(2), "the wait slept past the message: {waited:?}");
  assert_eq!(received.load(Ordering::SeqCst), 1);
}

#[test]
fn a_signal_for_work_already_done_waits_until_the_deadline() {
  let mut system = system("WaitPastAStaleSignal");
  let actor = system.actor_of(Props::new(|| Sink));
  actor.tell(1_u32, system.dead_letters());
  system.run_until_idle();

  let started = Instant::now();
  assert!(!system.await_envelope(Duration::from_millis(100)), "no work came");
  assert!(started.elapsed() >= Duration::from_millis(100), "the wait ended before its deadline");
}

// ----- run_one

// Fails on "boom", which leaves a run that handles only the Resume.
struct FailsOnBoom {
  log: Log,
}

impl Actor for FailsOnBoom {
  fn receive(&mut self, _context: &mut ActorContext<'_>, message: Message) {
    if message == "boom" {
      self.log.lock().unwrap().push(String::from("boom"));
      panic!("boom");
    }
    if let Some(text) = message.downcast_ref::<&'static str>() {
      self.log.lock().unwrap().push(String::from(*text));
    }
  }

  fn post_restart(&mut self, _context: &mut ActorContext<'_>) {
    self.log.lock().unwrap().push(String::from("restarted"));
  }
}

fn fails_on_boom(log: &Log) -> Props {
  let log = Arc::clone(log);
  Props::new(move || FailsOnBoom { log: Arc::clone(&log) })
}

#[test]
fn run_one_goes_past_a_run_that_handles_only_system_messages() {
  let mut system = with_catcher("RunOnePastSystemMessages");
  let log = new_log();
  let failing = system.actor_of(fails_on_boom(&log));
  let other = system.actor_of(recorder(&log, "other"));
  failing.tell("boom", failing.clone());
  assert!(system.run_one(), "the failing message");
  other.tell("x", other.clone());

  // The failing actor's Resume waits first; its run hands no message on.
  assert!(system.run_one(), "run_one stopped at a run of system messages only");
  assert_eq!(entries(&log), ["boom", "restarted", "other:x"]);
  assert!(!system.run_one());
}

// ----- the work a run leaves (design of #234, section 11.11.2)

#[test]
fn a_failed_message_is_followed_by_the_restart_before_the_next_message() {
  let mut system = with_catcher("YieldOnFailure");
  let log = new_log();
  let actor = system.actor_of(fails_on_boom(&log));
  actor.tell("boom", actor.clone());
  actor.tell("next", actor.clone());

  system.run_until_idle();

  assert_eq!(entries(&log), ["boom", "restarted", "next"]);
}

#[test]
fn a_kill_ends_the_run_and_the_messages_behind_it_go_to_dead_letters() {
  let mut system = with_catcher("YieldOnKill");
  let dead = dead_letters_of(&mut system);
  let log = new_log();
  let actor = system.actor_of(recorder(&log, "a"));
  actor.tell("before", actor.clone());
  actor.tell("Kill", actor.clone());
  actor.tell("after", actor.clone());

  system.run_until_idle();

  assert_eq!(entries(&log), ["a:before"]);
  assert!(system.is_actor_terminated(&actor), "the default decider stops a killed actor");
  assert_eq!(entries(&dead), ["after"]);
}

#[test]
fn a_poison_pill_ends_the_run_and_the_messages_behind_it_go_to_dead_letters() {
  let mut system = with_catcher("YieldOnPoisonPill");
  let dead = dead_letters_of(&mut system);
  let log = new_log();
  let actor = system.actor_of(recorder(&log, "a"));
  actor.tell("before", actor.clone());
  actor.tell("PoisonPill", actor.clone());
  actor.tell("after", actor.clone());

  system.run_until_idle();

  assert_eq!(entries(&log), ["a:before"]);
  assert!(system.is_actor_terminated(&actor));
  assert_eq!(entries(&dead), ["after"]);
}

// Asks to stop after a reply, then fails the same message.
struct StopsThenFails {
  log: Log,
  reply_to: ActorRef,
}

impl Actor for StopsThenFails {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    if message == "boom" {
      context.stop_and_reply_after_termination(self.reply_to.clone(), "bye");
      panic!("boom");
    }
  }

  fn post_stop_with_context(&mut self, context: &mut ActorContext<'_>) {
    self.log.lock().unwrap().push(std::format!("post_stop from {}", context.sender().path().name()));
  }

  fn post_restart(&mut self, _context: &mut ActorContext<'_>) {
    self.log.lock().unwrap().push(String::from("restarted"));
  }
}

#[test]
fn a_stop_after_a_reply_wins_over_a_failure_of_the_same_message() {
  let mut system = with_catcher("YieldOnStopAndFailure");
  let log = new_log();
  let replies = new_log();
  let sink = Arc::clone(&replies);
  let reply_to = system.function_ref("reply-to", move |_: &ActorRef, message: Message| {
    if let Some(text) = message.downcast_ref::<&'static str>() {
      sink.lock().unwrap().push(String::from(*text));
    }
  });
  let failing_sender = system.function_ref("failing-sender", |_: &ActorRef, _: Message| {});
  let (actor_log, to) = (Arc::clone(&log), reply_to.clone());
  let actor = system.actor_of(Props::new(move || StopsThenFails { log: Arc::clone(&actor_log), reply_to: to.clone() }));
  actor.tell("boom", failing_sender);

  system.run_until_idle();

  assert_eq!(entries(&log), ["post_stop from failing-sender"], "the actor restarted, or its hook missed the sender");
  assert!(system.is_actor_terminated(&actor));
  assert_eq!(entries(&replies), ["bye"]);
}

// Stops `target` on "stop", and records whether it has stopped on "check".
struct StopsAnother {
  log: Log,
  target: ActorRef,
}

impl Actor for StopsAnother {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    if message == "stop" {
      context.stop(&self.target);
      self.log.lock().unwrap().push(String::from("x:stop"));
    } else if message == "check" {
      let stopped = context.is_terminated(&self.target);
      self.log.lock().unwrap().push(std::format!("x:check, stopped {stopped}"));
    }
  }
}

#[test]
fn a_stop_asked_for_in_receive_ends_the_run_and_takes_effect_before_the_next_message() {
  let mut system = system("YieldOnBoundaryWork");
  let log = new_log();
  let target = system.actor_of(Props::new(|| Sink));
  let (actor_log, to) = (Arc::clone(&log), target.clone());
  let x = system.actor_of(Props::new(move || StopsAnother { log: Arc::clone(&actor_log), target: to.clone() }));
  let other = system.actor_of(recorder(&log, "other"));
  x.tell("stop", x.clone());
  for _ in 0..4 {
    x.tell("check", x.clone());
  }
  other.tell("b", other.clone());

  system.run_until_idle();

  // The run of x yields after "stop", so the other actor's run comes
  // between; the remaining four go on in the run after it.
  assert_eq!(
    entries(&log),
    [
      "x:stop",
      "other:b",
      "x:check, stopped true",
      "x:check, stopped true",
      "x:check, stopped true",
      "x:check, stopped true"
    ]
  );
}

#[test]
fn messages_that_leave_nothing_behind_go_on_up_to_the_throughput() {
  let mut system = system("ContinueUpToTheThroughput");
  let log = new_log();
  let x = system.actor_of(recorder(&log, "x"));
  let other = system.actor_of(recorder(&log, "other"));
  for text in ["1", "2", "3", "4", "5", "6"] {
    x.tell(text, x.clone());
  }
  other.tell("b", other.clone());

  system.run_until_idle();

  assert_eq!(entries(&log), ["x:1", "x:2", "x:3", "x:4", "x:5", "other:b", "x:6"]);
}

// ----- a stop asked for while the actor runs

// Stops itself and `other` on its first message.
struct StopsBoth {
  log: Log,
  other: ActorRef,
}

impl Actor for StopsBoth {
  fn receive(&mut self, context: &mut ActorContext<'_>, _message: Message) {
    let me = context.self_ref();
    context.stop(&me);
    context.stop(&self.other);
    let still = (context.is_terminated(&me), context.is_terminated(&self.other));
    self.log.lock().unwrap().push(std::format!("terminated in receive: {still:?}"));
  }
}

#[test]
fn stops_asked_for_in_receive_happen_once_the_run_has_returned() {
  let mut system = system("StopsAfterTheRun");
  let log = new_log();
  let other = system.actor_of(Props::new(|| Sink));
  let (actor_log, to) = (Arc::clone(&log), other.clone());
  let actor = system.actor_of(Props::new(move || StopsBoth { log: Arc::clone(&actor_log), other: to.clone() }));
  actor.tell(1_u32, actor.clone());

  system.run_until_idle();

  assert_eq!(entries(&log), ["terminated in receive: (false, false)"]);
  assert!(system.is_actor_terminated(&actor));
  assert!(system.is_actor_terminated(&other));
}

// ----- a panic that no catcher catches (design of #234, section 11.11.2)

// Panics in `receive`, and counts the instances dropped.
struct PanicsInReceive {
  drops: Arc<AtomicUsize>,
}

impl Actor for PanicsInReceive {
  fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {
    panic!("receive fails");
  }
}

impl Drop for PanicsInReceive {
  fn drop(&mut self) {
    self.drops.fetch_add(1, Ordering::SeqCst);
  }
}

#[test]
fn a_panic_without_a_catcher_reaches_the_caller_and_the_next_call_terminates() {
  let mut system = system("FatalReceive");
  let drops = Arc::new(AtomicUsize::new(0));
  let counted = Arc::clone(&drops);
  let failing = system.actor_of(Props::new(move || PanicsInReceive { drops: Arc::clone(&counted) }));
  let log = new_log();
  let other = system.actor_of(recorder(&log, "other"));
  let mailboxes = [failing.mailbox_weak(), other.mailbox_weak()];
  failing.tell(1_u32, failing.clone());
  other.tell("late", other.clone());

  let unwound = panic::catch_unwind(panic::AssertUnwindSafe(|| system.run_until_idle()));

  assert!(unwound.is_err(), "the panic did not reach the caller");
  assert_eq!(drops.load(Ordering::SeqCst), 0, "a user type was dropped while the panic unwound");
  assert!(system.is_terminated(), "the call after the panic did not terminate the system");
  assert_eq!(drops.load(Ordering::SeqCst), 1, "terminating did not drop the actor");
  system.run_until_idle();
  assert!(entries(&log).is_empty(), "a message was delivered after the panic: {:?}", entries(&log));
  drop(failing);
  drop(other);
  assert!(mailboxes.iter().all(|mailbox| mailbox.upgrade().is_none()), "a mailbox outlived the terminated system");
}

struct PanicsInPreStart;

impl Actor for PanicsInPreStart {
  fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {}

  fn pre_start(&mut self, _context: &mut ActorContext<'_>) {
    panic!("pre_start fails");
  }
}

#[test]
fn a_panic_in_pre_start_without_a_catcher_terminates_on_the_next_call() {
  let mut system = system("FatalPreStart");

  let unwound = panic::catch_unwind(panic::AssertUnwindSafe(|| system.actor_of(Props::new(|| PanicsInPreStart))));

  assert!(unwound.is_err());
  assert!(system.is_terminated());
}

// ----- a stop takes the run that waits for the mailbox

#[test]
fn stopping_an_actor_takes_the_run_that_waits_for_it() {
  let mut system = system("StopTakesTheRun");
  let actor = system.actor_of(Props::new(|| Sink));
  actor.tell(1_u32, actor.clone());
  let waiting = Executor::lend(system.executor_for_test(), |executor| !executor.manual.is_empty());
  assert!(waiting, "the send left no run");

  system.stop(&actor);

  let waiting = Executor::lend(system.executor_for_test(), |executor| !executor.manual.is_empty());
  assert!(!waiting, "the run of the stopped actor still waits");
}
