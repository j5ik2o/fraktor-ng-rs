// SPDX-License-Identifier: Apache-2.0
//
// Licensed to the Apache Software Foundation (ASF) under one or more
// license agreements; and to You under the Apache License, version 2.0:
//
//   https://www.apache.org/licenses/LICENSE-2.0
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.

//! The red unit tests of stage 4b (design of #234, section 11.13).
//!
//! Each test states one behaviour section 11.13 decides for stage 4b and reads
//! the evidence where section 11.13.12 says it is readable: the public surface
//! first, and the crate-internal suspend count for the one read no public entry
//! exposes (11.13.8). Each one fails on the stage 4a implementation it
//! accompanies.
//!
//! The file is a submodule of `actor_system_value_test`, the crate's test
//! module for `ActorSystem`, so the suspend read reaches the executor through
//! the test-only `stage4b_suspend_counts` helper that module exposes.
//!
//! The scenarios not reached here (the restart count, which the parent's table
//! already keeps, and the two variants of a failure of one run that the stage
//! 4a executor decides without the parent's own run) are left to the
//! implementation's PR, where the behaviour is present; they are not written as
//! tests here because they do not fail on this implementation.

use core::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use super::stage4b_suspend_counts;
use crate::{
  adaptor::{ChannelBoundary, UnwindCatcher},
  kernel::{
    actor::{
      Actor, ActorContext,
      Logging::{EventSink, LogEvent},
      Props,
    },
    actor_ref::ActorRef,
    actor_system::ActorSystem,
    message::Message,
    supervision::{Directive, RestartWindow, SupervisorStrategy},
    sync::Arc,
  },
};

type Log = Arc<Mutex<Vec<String>>>;

fn new_log() -> Log {
  Arc::new(Mutex::new(Vec::new()))
}

fn entries(log: &Log) -> Vec<String> {
  log.lock().unwrap().clone()
}

fn counted(log: &Log, text: &str) -> usize {
  entries(log).iter().filter(|entry| entry.as_str() == text).count()
}

fn with_catcher(name: &str) -> ActorSystem {
  ActorSystem::with_failure_catcher(name, Arc::new(ChannelBoundary::new()), Arc::new(UnwindCatcher))
}

/// The suspend counts of the given actors, through the crate-internal mailbox
/// entry section 11.13.8 makes production. A stopped actor answers `None`; the
/// callers make sure their actors are alive.
fn counts_of(system: &mut ActorSystem, targets: &[&ActorRef]) -> Vec<Option<usize>> {
  stage4b_suspend_counts(system, targets)
}

// ----- a shared worker and parent

/// Records its lifecycle, fails on `boom`, and records what it receives.
struct Worker {
  name: &'static str,
  log: Log,
}

impl Actor for Worker {
  fn pre_start(&mut self, _context: &mut ActorContext<'_>) {
    self.log.lock().unwrap().push(std::format!("{}:pre_start", self.name));
  }

  fn pre_restart(&mut self, _context: &mut ActorContext<'_>, _reason: &(dyn core::any::Any + Send)) {
    self.log.lock().unwrap().push(std::format!("{}:pre_restart", self.name));
  }

  fn post_restart(&mut self, _context: &mut ActorContext<'_>) {
    self.log.lock().unwrap().push(std::format!("{}:post_restart", self.name));
  }

  fn receive(&mut self, _context: &mut ActorContext<'_>, message: Message) {
    if message.downcast_ref::<&'static str>() == Some(&"boom") {
      panic!("simulated failure of {}", self.name);
    }
    if let Some(text) = message.downcast_ref::<&'static str>() {
      self.log.lock().unwrap().push(std::format!("{}:{text}", self.name));
    }
  }
}

fn worker(name: &'static str, log: &Log) -> Props {
  let log = Arc::clone(log);
  Props::new(move || Worker { name, log: Arc::clone(&log) })
}

/// Records the parent's part of a scenario, fails on `boom`, and carries the
/// strategy the test gives it.
struct Parent {
  name: &'static str,
  log: Log,
  strategy: SupervisorStrategy,
}

impl Actor for Parent {
  fn receive(&mut self, _context: &mut ActorContext<'_>, message: Message) {
    if message.downcast_ref::<&'static str>() == Some(&"boom") {
      panic!("simulated failure of {}", self.name);
    }
    if let Some(text) = message.downcast_ref::<&'static str>() {
      self.log.lock().unwrap().push(std::format!("{}:receive:{text}", self.name));
    }
  }

  fn pre_restart(&mut self, _context: &mut ActorContext<'_>, _reason: &(dyn core::any::Any + Send)) {
    self.log.lock().unwrap().push(std::format!("{}:pre_restart", self.name));
  }

  fn post_restart(&mut self, _context: &mut ActorContext<'_>) {
    self.log.lock().unwrap().push(std::format!("{}:post_restart", self.name));
  }

  fn supervisor_strategy(&self) -> SupervisorStrategy {
    self.strategy.clone()
  }
}

fn parent(name: &'static str, log: &Log, strategy: SupervisorStrategy) -> Props {
  let log = Arc::clone(log);
  Props::new(move || Parent { name, log: Arc::clone(&log), strategy: strategy.clone() })
}

// ----- failures (design section 11.13.3)

#[test]
fn a_message_to_a_child_under_a_failed_parent_waits_for_the_decision() {
  // Design 11.13.3 item 3 and 11.13.12 "失敗" first line: a failure suspends the
  // child, so a message sent to the child after the parent failed is not
  // handled until the parent's decision has taken effect. Stage 4a does not
  // suspend children, so the child answers at once.
  let log = new_log();
  let mut system = with_catcher("Stage4bSuspendedChild");
  let guardian = system.actor_of(parent("guardian", &log, SupervisorStrategy::default_strategy()));
  let failing = system.downgrade().spawn_child(
    &guardian,
    parent("parent", &log, SupervisorStrategy::one_for_one(3).with_decider(|_, _| panic!("simulated decider panic"))),
    Some("parent"),
    SupervisorStrategy::one_for_one(3),
  );
  let child = system.downgrade().spawn_child(
    &failing,
    worker("child", &log),
    Some("child"),
    SupervisorStrategy::default_strategy(),
  );
  system.run_until_idle();

  failing.tell("boom", guardian.clone());
  system.run_one();
  assert!(!system.is_actor_terminated(&failing), "the parent stopped; the scenario needs it to restart");
  // In stage 4b the child is suspended until the parent's decision has taken
  // effect, so this message waits. Stage 4a never suspends the child, so it is
  // handled right away.
  log.lock().unwrap().clear();
  child.tell("ping", guardian.clone());
  system.run_until_idle();
  assert_eq!(counted(&log, "child:ping"), 0, "the child handled a message under its failed parent");
}

#[test]
fn a_decider_panic_on_a_childs_failure_makes_the_parent_fail() {
  // Design 11.13.3 item 4 and 11.13.4 (a decider's panic is the `Escalate` of
  // a parent without an instance): the decider runs in the parent's run for the
  // child's `Failed`, so its panic is the parent's own failure and the
  // guardian restarts the parent. Stage 4a runs the decider in the executor's
  // `finish_run`, where a panic is handled without failing the parent, so the
  // parent does not restart.
  let log = new_log();
  let mut system = with_catcher("Stage4bPerpetrator");
  let guardian = system.actor_of(parent("guardian", &log, SupervisorStrategy::default_strategy()));
  let failing = system.downgrade().spawn_child(
    &guardian,
    parent("parent", &log, SupervisorStrategy::one_for_one(3).with_decider(|_, _| panic!("simulated decider panic"))),
    Some("parent"),
    SupervisorStrategy::one_for_one(3),
  );
  let child =
    system.downgrade().spawn_child(&failing, worker("child", &log), Some("child"), SupervisorStrategy::one_for_one(3));
  system.run_until_idle();
  let before = counted(&log, "parent:pre_restart");

  let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
    child.tell("boom", guardian.clone());
    system.run_until_idle();
  }));
  assert!(outcome.is_ok(), "the decider panic escaped the catcher");
  assert!(
    counted(&log, "parent:pre_restart") > before,
    "the decider's panic did not become the parent's failure: the parent was not restarted"
  );
}

#[test]
fn a_child_terminated_hook_failure_makes_the_parent_fail() {
  // Design 11.13.3, the "子の終了のフックの失敗" row, and 11.13.4: the hook's
  // failure becomes the parent's failure, and the guardian restarts the parent.
  // Stage 4a only publishes the hook's failure.
  let log = new_log();
  let mut system = with_catcher("Stage4bHookFailure");
  let guardian = system.actor_of(parent("guardian", &log, SupervisorStrategy::default_strategy()));
  let failing = system.downgrade().spawn_child(
    &guardian,
    parent(
      "parent",
      &log,
      SupervisorStrategy::one_for_one(3).with_child_terminated(|_context, _child, _siblings| {
        panic!("simulated hook failure");
      }),
    ),
    Some("parent"),
    SupervisorStrategy::one_for_one(3),
  );
  let kid =
    system.downgrade().spawn_child(&failing, worker("kid", &log), Some("kid"), SupervisorStrategy::default_strategy());
  system.run_until_idle();

  system.stop(&kid);
  system.run_until_idle();
  assert!(counted(&log, "parent:pre_restart") >= 1, "the hook failure did not restart the parent");
}

// ----- decisions (design section 11.13.4)

#[test]
fn a_failed_child_is_decided_in_the_parents_run_not_the_childs() {
  // Design 11.13.12 "判断" first line and "1 手ずつの順" (2): the parent decides
  // after the child's run has returned, so the child's `pre_restart` cannot
  // run before the decision. The parent and the child record in one log; stage
  // 4a's executor decides after the child's run in the same `run_one` and the
  // child restarts before the parent's run.
  let log = new_log();
  let decided = Arc::new(AtomicUsize::new(0));
  let mut system = with_catcher("Stage4bDecideInParent");
  let guardian = system.actor_of(parent("guardian", &log, SupervisorStrategy::default_strategy()));
  let failing = system.downgrade().spawn_child(
    &guardian,
    parent(
      "parent",
      &log,
      SupervisorStrategy::one_for_one(3).with_decider({
        let decided = Arc::clone(&decided);
        move |_, _| {
          decided.fetch_add(1, Ordering::SeqCst);
          Directive::Restart
        }
      }),
    ),
    Some("parent"),
    SupervisorStrategy::one_for_one(3),
  );
  let child =
    system.downgrade().spawn_child(&failing, worker("child", &log), Some("child"), SupervisorStrategy::one_for_one(3));
  system.run_until_idle();
  log.lock().unwrap().clear();

  child.tell("boom", failing.clone());
  // One `run_one` must not take the child's failing run, the parent's decision
  // and the child's restart together. Stage 4a decides inside the executor as
  // the child's run returns, so the child restarts before the parent runs.
  let ran = system.run_one();
  assert!(ran, "the child's failing run did not run");
  let seen = entries(&log);
  assert_eq!(decided.load(Ordering::SeqCst), 0, "the parent decided before its own run had the `Failed`: {seen:?}");
  assert_eq!(counted(&log, "child:pre_restart"), 0, "the child restarted before the parent decided: {seen:?}");
}

// ----- all-for-one (design section 11.13.5)

/// A worker whose `pre_start` and `post_restart` record their order.
struct LateStarter {
  name: &'static str,
  log: Log,
}

impl Actor for LateStarter {
  fn pre_start(&mut self, _context: &mut ActorContext<'_>) {
    self.log.lock().unwrap().push(std::format!("{}:pre_start", self.name));
  }

  fn post_restart(&mut self, _context: &mut ActorContext<'_>) {
    self.log.lock().unwrap().push(std::format!("{}:post_restart", self.name));
  }

  fn receive(&mut self, _context: &mut ActorContext<'_>, message: Message) {
    if message.downcast_ref::<&'static str>() == Some(&"boom") {
      panic!("simulated failure of {}", self.name);
    }
  }
}

fn late_starter(name: &'static str, log: &Log) -> Props {
  let log = Arc::clone(log);
  Props::new(move || LateStarter { name, log: Arc::clone(&log) })
}

#[test]
fn an_all_for_one_restart_restarts_a_sibling_that_has_not_started() {
  // Design 11.13.5, the "まだ最初の `Create` を処理していない" row, and #254
  // comment 2: a sibling that has not handled its `Create` receives `Create`,
  // `Suspend`, `Recreate`, runs `pre_start` twice and `post_restart` once, and
  // counts one restart. Stage 4a leaves it to its `Create`, so it starts once
  // and post_restarts never.
  let log = new_log();
  let mut system = with_catcher("Stage4bAllForOneNotStarted");
  let failing = system.actor_of(parent("failing", &log, SupervisorStrategy::all_for_one(2, RestartWindow::Unbounded)));
  let a = system.downgrade().spawn_child(
    &failing,
    late_starter("a", &log),
    Some("a"),
    SupervisorStrategy::default_strategy(),
  );
  let b = system.downgrade().spawn_child(
    &failing,
    late_starter("b", &log),
    Some("b"),
    SupervisorStrategy::default_strategy(),
  );
  // Neither `a` nor `b` has run when `a` fails, so `b` is still waiting for its
  // `Create` when the all-for-one decision restarts it.
  a.tell("boom", failing.clone());
  system.run_until_idle();
  assert_eq!(
    counted(&log, "b:post_restart"),
    1,
    "the sibling that had not started did not restart: {:?}",
    entries(&log)
  );
  assert_eq!(counted(&log, "b:pre_start"), 2, "the sibling started once, exactly as its own `Create` does");
  assert!(!system.is_actor_terminated(&b));
}

// ----- recreate while waiting (design section 11.13.8)

/// Starts a grandchild and stops it in `pre_restart`, so its restart waits for
/// the grandchild.
struct WaitsForItsChild {
  log: Log,
  child: Arc<Mutex<Option<ActorRef>>>,
}

impl Actor for WaitsForItsChild {
  fn pre_start(&mut self, context: &mut ActorContext<'_>) {
    if let Ok(grandchild) = context.actor_of(Props::new(|| Leaf), "grandchild") {
      *self.child.lock().unwrap() = Some(grandchild);
    }
    self.log.lock().unwrap().push(String::from("child:pre_start"));
  }

  fn pre_restart(&mut self, context: &mut ActorContext<'_>, _reason: &(dyn core::any::Any + Send)) {
    self.log.lock().unwrap().push(String::from("child:pre_restart"));
    if let Some(grandchild) = self.child.lock().unwrap().clone() {
      context.stop(&grandchild);
    }
  }

  fn post_restart(&mut self, _context: &mut ActorContext<'_>) {
    self.log.lock().unwrap().push(String::from("child:post_restart"));
  }

  fn receive(&mut self, _context: &mut ActorContext<'_>, message: Message) {
    if message.downcast_ref::<&'static str>() == Some(&"boom") {
      panic!("simulated failure of the waiting child");
    }
  }
}

struct Leaf;

impl Actor for Leaf {
  fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {}
}

#[test]
fn a_recreate_delivered_while_the_actor_waits_for_children_runs_after_the_wait() {
  // Design 11.13.8 "子を待つ間の `Recreate`" and #254 comment 1: an actor that is
  // waiting for its children keeps the `Recreate` and handles it after the wait,
  // so it restarts a second time and the recovery does not nest. Stage 4a drops
  // the `Recreate` for a waiting actor, so `post_restart` runs once.
  let log = new_log();
  let mut system = with_catcher("Stage4bKeptRecreate");
  let guardian = system.actor_of(parent("guardian", &log, SupervisorStrategy::default_strategy()));
  let failing = system.downgrade().spawn_child(
    &guardian,
    parent("parent", &log, SupervisorStrategy::one_for_one(3).with_decider(|_, _| Directive::Restart)),
    Some("parent"),
    SupervisorStrategy::one_for_one(3),
  );
  let child = system.downgrade().spawn_child(
    &failing,
    Props::new({
      let log = Arc::clone(&log);
      move || WaitsForItsChild { log: Arc::clone(&log), child: Arc::new(Mutex::new(None)) }
    }),
    Some("child"),
    SupervisorStrategy::one_for_one(3),
  );
  system.run_until_idle();
  log.lock().unwrap().clear();

  child.tell("boom", failing.clone());
  system.run_until_idle();
  assert_eq!(
    counted(&log, "child:post_restart"),
    2,
    "a `Recreate` that reached a waiting child was dropped instead of kept: {:?}",
    entries(&log)
  );
}

// ----- survivors (design section 11.13.6, the marshal's decision A)

/// A parent that keeps its child on restart.
struct KeepsItsChild {
  log: Log,
}

impl Actor for KeepsItsChild {
  fn pre_restart(&mut self, _context: &mut ActorContext<'_>, _reason: &(dyn core::any::Any + Send)) {
    self.log.lock().unwrap().push(String::from("parent:pre_restart"));
  }

  fn post_restart(&mut self, _context: &mut ActorContext<'_>) {
    self.log.lock().unwrap().push(String::from("parent:post_restart"));
  }

  fn receive(&mut self, _context: &mut ActorContext<'_>, message: Message) {
    if message.downcast_ref::<&'static str>() == Some(&"boom") {
      panic!("simulated failure of the keeping parent");
    }
  }
}

#[test]
fn a_parents_restart_recreates_the_children_it_keeps() {
  // Design 11.13.6 `finishRecreate` item 4 with decision A: every failure of a
  // parent, not only a `Kill`, restarts the children it keeps. Stage 4a
  // restarts survivors only after a `Kill`, so the child stays as it is.
  let log = new_log();
  let mut system = with_catcher("Stage4bSurvivors");
  let guardian = system.actor_of(parent("guardian", &log, SupervisorStrategy::default_strategy()));
  let failing = system.downgrade().spawn_child(
    &guardian,
    Props::new({
      let log = Arc::clone(&log);
      move || KeepsItsChild { log: Arc::clone(&log) }
    }),
    Some("parent"),
    SupervisorStrategy::one_for_one(3),
  );
  let child = system.downgrade().spawn_child(
    &failing,
    worker("child", &log),
    Some("child"),
    SupervisorStrategy::default_strategy(),
  );
  system.run_until_idle();
  log.lock().unwrap().clear();

  failing.tell("boom", guardian.clone());
  system.run_until_idle();
  assert_eq!(
    counted(&log, "child:post_restart"),
    1,
    "a kept child was not recreated after its parent's restart: {:?}",
    entries(&log)
  );
  assert!(!system.is_actor_terminated(&child));
}

// ----- creation under a suspended parent (design section 11.13.7)

/// Creates a child from `pre_restart`, which runs under a suspended parent.
struct CreatesFromPreRestart {
  log: Log,
  created: Arc<Mutex<Option<ActorRef>>>,
}

impl Actor for CreatesFromPreRestart {
  fn pre_restart(&mut self, context: &mut ActorContext<'_>, _reason: &(dyn core::any::Any + Send)) {
    self.log.lock().unwrap().push(String::from("parent:pre_restart"));
    if let Ok(late) = context.actor_of(worker("late", &self.log), "late") {
      *self.created.lock().unwrap() = Some(late);
    }
  }

  fn post_restart(&mut self, _context: &mut ActorContext<'_>) {
    self.log.lock().unwrap().push(String::from("parent:post_restart"));
  }

  fn receive(&mut self, _context: &mut ActorContext<'_>, message: Message) {
    if message.downcast_ref::<&'static str>() == Some(&"boom") {
      panic!("simulated failure of the creating parent");
    }
  }
}

#[test]
fn a_child_created_under_a_suspended_parent_receives_the_parents_suspend() {
  // Design 11.13.7 "suspend された親の下での子の生成" (`Children.scala:340`) and
  // 11.13.12 "生成" first line: a child created under a suspended parent gets a
  // `Suspend` behind its `Create` for each suspend the parent holds, so it does
  // not handle a normal message before the parent's decision. Stage 4a creates
  // it without the `Suspend`.
  let log = new_log();
  let mut system = with_catcher("Stage4bSuspendInheritance");
  let guardian = system.actor_of(parent("guardian", &log, SupervisorStrategy::default_strategy()));
  let created = Arc::new(Mutex::new(None));
  let failing = system.downgrade().spawn_child(
    &guardian,
    Props::new({
      let log = Arc::clone(&log);
      let created = Arc::clone(&created);
      move || CreatesFromPreRestart { log: Arc::clone(&log), created: Arc::clone(&created) }
    }),
    Some("parent"),
    SupervisorStrategy::one_for_one(3),
  );
  system.run_until_idle();
  log.lock().unwrap().clear();
  // The parent's decider panics, so the guardian restarts the parent, whose
  // `pre_restart` creates `late` while the parent is suspended.
  failing.tell("boom", guardian.clone());
  system.run_until_idle();
  assert_eq!(counted(&log, "parent:post_restart"), 1, "the parent did not restart: {:?}", entries(&log));
  let late = created.lock().unwrap().clone().expect("the parent created its child from `pre_restart`");
  let counts = counts_of(&mut system, &[&late]);
  assert_eq!(
    counts,
    alloc::vec![Some(1)],
    "the child created under the suspended parent got no `Suspend` for it: {:?}",
    entries(&log)
  );
}

// ----- post_restart failures (design section 11.13.6)

/// An actor whose `post_restart` fails.
struct FailsPostRestart {
  log: Log,
}

impl Actor for FailsPostRestart {
  fn post_restart(&mut self, _context: &mut ActorContext<'_>) {
    self.log.lock().unwrap().push(String::from("parent:post_restart"));
    panic!("simulated failure of post_restart");
  }

  fn receive(&mut self, _context: &mut ActorContext<'_>, message: Message) {
    if message.downcast_ref::<&'static str>() == Some(&"boom") {
      panic!("simulated failure of the post_restart actor");
    }
  }
}

#[test]
fn an_actor_whose_post_restart_fails_stops_before_deciding_a_kept_failed() {
  // Design 11.13.6 "3 が失敗したとき" item 3 ("その実行の中で `Terminate` の処理
  // を行う") and 11.13.12 "post_restart の失敗" first line: an actor with a kept
  // `Failed` whose `post_restart` fails stops in that run without deciding the
  // kept `Failed`. Stage 4a neither keeps the `Failed` nor stops the actor when
  // `post_restart` fails.
  let log = new_log();
  let decisions = Arc::new(AtomicUsize::new(0));
  let mut system = with_catcher("Stage4bPostRestartFails");
  let guardian = system.actor_of(parent("guardian", &log, SupervisorStrategy::default_strategy()));
  let failing = system.downgrade().spawn_child(
    &guardian,
    Props::new({
      let log = Arc::clone(&log);
      move || FailsPostRestart { log: Arc::clone(&log) }
    }),
    Some("parent"),
    SupervisorStrategy::one_for_one(3).with_decider({
      let decisions = Arc::clone(&decisions);
      move |_, _| {
        decisions.fetch_add(1, Ordering::SeqCst);
        Directive::Restart
      }
    }),
  );
  let _child =
    system.downgrade().spawn_child(&failing, worker("child", &log), Some("child"), SupervisorStrategy::one_for_one(3));
  system.run_until_idle();
  log.lock().unwrap().clear();
  let before = decisions.load(Ordering::SeqCst);

  failing.tell("boom", guardian.clone());
  system.run_until_idle();
  assert!(counted(&log, "parent:post_restart") >= 1, "the parent did not restart: {:?}", entries(&log));
  assert!(system.is_actor_terminated(&failing), "the actor whose `post_restart` failed did not stop in that run");
  assert_eq!(
    decisions.load(Ordering::SeqCst),
    before,
    "the kept `Failed` was decided after the `post_restart` failure"
  );
}

// ----- child-terminated hook failures (design section 11.13.3, two publishes)

#[test]
fn a_child_terminated_hook_failure_publishes_twice_with_the_two_causes() {
  // Design 11.13.3, the two-publish table: the first publish names
  // `IllegalStateException("handleChildTerminated failed")`, the second the
  // original failure. Stage 4a publishes only the first.
  let events = Arc::new(Mutex::new(Vec::<(Option<&'static str>, String)>::new()));
  let log = new_log();
  let mut system = with_catcher("Stage4bHookPublishesTwice");
  system.set_event_sink(Arc::new(RecordingSink { events: Arc::clone(&events) }));
  let failing = system.actor_of(parent(
    "parent",
    &log,
    SupervisorStrategy::one_for_one(3).with_child_terminated(|_context, _child, _siblings| {
      panic!("simulated hook failure");
    }),
  ));
  let kid =
    system.downgrade().spawn_child(&failing, worker("kid", &log), Some("kid"), SupervisorStrategy::default_strategy());
  system.run_until_idle();
  system.stop(&kid);
  system.run_until_idle();
  let published = events.lock().unwrap().clone();
  assert_eq!(
    published
      .iter()
      .filter(|(cause, message)| cause == &Some("IllegalStateException") && message == "handleChildTerminated failed")
      .count(),
    1,
    "the first publish is not the `handleChildTerminated failed` one: {published:?}"
  );
  assert_eq!(
    published.len(),
    2,
    "the hook failure was not published twice (the hook failure, then the parent's failure): {published:?}"
  );
}

/// Records the cause name and the message of every event it is given.
struct RecordingSink {
  events: Arc<Mutex<Vec<(Option<&'static str>, String)>>>,
}

impl EventSink for RecordingSink {
  fn publish(&self, event: &dyn LogEvent) {
    let cause = event.cause_name().map(|cause| KNOWN_CAUSES.iter().copied().find(|known| *known == cause)).flatten();
    self.events.lock().unwrap().push((cause, String::from(event.message())));
  }

  fn into_any(self: Arc<Self>) -> Arc<dyn core::any::Any + Send + Sync> {
    self
  }
}

// The cause names the events of this file can carry, as `&'static str` so the
// recording sink can keep them past the publish call.
const KNOWN_CAUSES: [&str; 5] =
  ["IllegalStateException", "RuntimeException", "ActorInitializationException", "ActorKilledException", "Exception"];
