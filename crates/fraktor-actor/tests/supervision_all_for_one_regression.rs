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

// Regression tests for the all-for-one supervision and lifecycle-sender
// behaviour of the port. These are not ported from a Pekko file; they pin the
// invariants the Ticket669 port relies on and that its two frozen cases do not
// reach on their own: sibling restarts and stops, retry windows, decider
// failures, and isolation of the failing message's sender in lifecycle hooks.

use core::time::Duration;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::sync::Mutex;

use fraktor_actor::adaptor::{attach_wait_driver, ChannelBoundary, UnwindCatcher};
use fraktor_actor::kernel::actor::{Actor, ActorContext, Props};
use fraktor_actor::kernel::actor_ref::ActorRef;
use fraktor_actor::kernel::actor_system::ActorSystem;
use fraktor_actor::kernel::message::Message;
use fraktor_actor::kernel::supervision::{Directive, FailureKind, RestartWindow, SupervisorStrategy};
use fraktor_actor::kernel::Clock;
use fraktor_testkit::fraktorspec::FraktorSpec;
use fraktor_testkit::implicit_sender::ImplicitSender;

const SPEC_SYSTEM_NAME: &str = "AllForOneRegression";

fn default_timeout() -> Duration {
  Duration::from_secs(3)
}

// The sender each worker's stop hook saw, keyed by the worker's own path.
type StopReports = Arc<Mutex<Vec<(String, String)>>>;

// Answers a `Props` with a child it creates under its own strategy, as the
// `Supervisor` helper of the Ticket669 original does.
struct SupervisorActor {
  strategy: SupervisorStrategy,
  created: u32,
}

impl Actor for SupervisorActor {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    let Some(props) = message.downcast_ref::<Props>() else {
      return;
    };
    let name = format!("child-{}", self.created);
    self.created += 1;
    let child = context.actor_of(props.clone(), &name).expect("the supervisor starts its child");
    context.sender().tell(child, context.self_ref());
  }

  fn supervisor_strategy(&self) -> SupervisorStrategy {
    self.strategy.clone()
  }

  fn pre_restart(&mut self, _context: &mut ActorContext<'_>, _reason: &(dyn core::any::Any + Send)) {}
}

// Records in `reports` the handle its stop hook saw as `sender()`, then fails
// on any message so the stop hook carries that message's original sender.
struct ReportingWorker {
  reports: StopReports,
}

impl Actor for ReportingWorker {
  fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {
    panic!("boom");
  }

  fn post_stop_with_context(&mut self, context: &mut ActorContext<'_>) {
    let entry = (context.self_ref().path().to_string(), context.sender().path().to_string());
    self.reports.lock().expect("the stop-report lock is not poisoned").push(entry);
  }
}

fn supervisor_props(strategy: SupervisorStrategy) -> Props {
  Props::new(move || SupervisorActor { strategy: strategy.clone(), created: 0 })
}

fn reporting_props(reports: StopReports) -> Props {
  Props::new(move || ReportingWorker { reports: Arc::clone(&reports) })
}

fn child_under(supervisor: &ActorRef, props: Props) -> ActorRef {
  let reply = supervisor.ask(props).await_result(default_timeout());
  reply.downcast_ref::<ActorRef>().cloned().expect("a child handle from the supervisor")
}

// With a retry limit of zero, an all-for-one strategy stops every sibling, not
// only the failing one. One child fails, and both stop hooks run. The failed
// child's hook sees the test sender and the sibling's hook sees dead letters.
#[test]
fn an_all_for_one_stop_stops_every_sibling_and_only_the_failed_child_sees_the_sender() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let sender = ImplicitSender::self_actor_ref(&spec);
  let sender_path = sender.path().to_string();
  let dead_letters_path = spec.system().dead_letters().path().to_string();
  let reports: StopReports = Arc::new(Mutex::new(Vec::new()));

  let supervisor =
    spec.system_mut().actor_of(supervisor_props(SupervisorStrategy::all_for_one(0, RestartWindow::Unbounded)));
  let failing = child_under(&supervisor, reporting_props(Arc::clone(&reports)));
  let sibling = child_under(&supervisor, reporting_props(Arc::clone(&reports)));

  failing.tell("boom", sender);
  spec.system_mut().run_until_idle();

  assert!(spec.system().is_actor_terminated(&failing), "the failed child stops when the limit is zero");
  assert!(spec.system().is_actor_terminated(&sibling), "the sibling stops with the failed child");

  let recorded = reports.lock().expect("the stop-report lock is not poisoned").clone();
  let failing_path = failing.path().to_string();
  let sibling_path = sibling.path().to_string();
  let sender_of =
    |path: &str| recorded.iter().find(|(self_path, _)| self_path == path).map(|(_, sender)| sender.clone());

  assert_eq!(
    sender_of(&failing_path).as_deref(),
    Some(sender_path.as_str()),
    "the failed child's stop hook sees the test sender"
  );
  assert_eq!(
    sender_of(&sibling_path).as_deref(),
    Some(dead_letters_path.as_str()),
    "the sibling's stop hook sees dead letters"
  );

  spec.terminate();
}

struct CountingWorker {
  generation: usize,
  restart_senders: StopReports,
  fail_start: bool,
}

impl Actor for CountingWorker {
  fn pre_start(&mut self, _context: &mut ActorContext<'_>) {
    assert!(!self.fail_start, "startup failure");
  }

  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    assert!(message != "boom", "message failure");
    context.sender().tell(self.generation, context.self_ref());
  }

  fn pre_restart(&mut self, context: &mut ActorContext<'_>, _reason: &(dyn core::any::Any + Send)) {
    self
      .restart_senders
      .lock()
      .unwrap()
      .push((context.self_ref().path().to_string(), context.sender().path().to_string()));
  }
}

fn counting_props(count: Arc<AtomicUsize>, restart_senders: StopReports, fail_start: bool) -> Props {
  Props::new(move || CountingWorker {
    generation: count.fetch_add(1, Ordering::SeqCst) + 1,
    restart_senders: Arc::clone(&restart_senders),
    fail_start,
  })
}

#[test]
fn all_siblings_restart_and_only_the_failed_child_inherits_the_message_sender() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let reports: StopReports = Arc::new(Mutex::new(Vec::new()));
  let supervisor =
    spec.system_mut().actor_of(supervisor_props(SupervisorStrategy::all_for_one(2, RestartWindow::Unbounded)));
  let failing = child_under(&supervisor, counting_props(Arc::new(AtomicUsize::new(0)), Arc::clone(&reports), false));
  let sibling = child_under(&supervisor, counting_props(Arc::new(AtomicUsize::new(0)), Arc::clone(&reports), false));
  let sender = spec.test_actor();
  failing.tell("boom", sender.clone());
  spec.system_mut().run_until_idle();
  for child in [&failing, &sibling] {
    let reply = child.ask("generation").await_result(default_timeout());
    assert_eq!(reply.downcast_ref::<usize>(), Some(&2));
  }
  let recorded = reports.lock().unwrap();
  assert_eq!(recorded.len(), 2);
  assert!(recorded.contains(&(sibling.path().to_string(), sibling.path().to_string())));
  assert!(recorded.contains(&(failing.path().to_string(), sender.path().to_string())));
  drop(recorded);
  spec.terminate();
}

#[test]
fn a_panicking_all_for_one_decider_restarts_its_supervisor() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let generations = Arc::new(AtomicUsize::new(0));
  let supervisor = spec.system_mut().actor_of(Props::new({
    let generations = Arc::clone(&generations);
    move || {
      generations.fetch_add(1, Ordering::SeqCst);
      SupervisorActor {
        strategy: SupervisorStrategy::all_for_one(2, RestartWindow::Unbounded)
          .with_decider(|_, _| panic!("decider failure")),
        created: 0,
      }
    }
  }));
  let child =
    child_under(&supervisor, counting_props(Arc::new(AtomicUsize::new(0)), Arc::new(Mutex::new(Vec::new())), false));
  child.tell("boom", spec.test_actor());
  let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| spec.system_mut().run_until_idle()));
  assert!(outcome.is_ok(), "the decider panic escapes the executor");
  assert_eq!(generations.load(Ordering::SeqCst), 2);
  assert!(!spec.system().is_actor_terminated(&child));
  spec.terminate();
}

#[test]
fn kill_stops_all_siblings_and_cannot_bypass_a_zero_restart_limit() {
  for restart in [false, true] {
    let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
    let dead_letters = spec.system().dead_letters().path().to_string();
    let strategy = SupervisorStrategy::all_for_one(0, RestartWindow::Unbounded).with_decider(move |_, _| {
      if restart {
        Directive::Restart
      } else {
        Directive::Stop
      }
    });
    let supervisor = spec.system_mut().actor_of(supervisor_props(strategy));
    let reports: StopReports = Arc::new(Mutex::new(Vec::new()));
    let failing = child_under(&supervisor, reporting_props(Arc::clone(&reports)));
    let sibling = child_under(&supervisor, reporting_props(Arc::clone(&reports)));
    failing.tell("Kill", spec.test_actor());
    spec.system_mut().run_until_idle();
    for child in [&failing, &sibling] {
      assert!(spec.system().is_actor_terminated(child));
      assert!(reports.lock().unwrap().contains(&(child.path().to_string(), dead_letters.clone())));
    }
    spec.terminate();
  }
}

#[test]
fn initialization_failure_stops_all_siblings_and_obeys_restart_permission() {
  for restart in [false, true] {
    let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
    let dead_letters = spec.system().dead_letters().path().to_string();
    let attempts = Arc::new(AtomicUsize::new(0));
    let strategy = SupervisorStrategy::all_for_one(1, RestartWindow::Unbounded).with_decider(move |_, kind| {
      assert_eq!(kind, FailureKind::Initialization);
      if restart {
        Directive::Restart
      } else {
        Directive::Stop
      }
    });
    let supervisor = spec.system_mut().actor_of(supervisor_props(strategy));
    let reports: StopReports = Arc::new(Mutex::new(Vec::new()));
    let sibling = child_under(&supervisor, reporting_props(Arc::clone(&reports)));
    let failing =
      child_under(&supervisor, counting_props(Arc::clone(&attempts), Arc::new(Mutex::new(Vec::new())), true));
    spec.system_mut().run_until_idle();
    assert!(spec.system().is_actor_terminated(&failing));
    assert!(spec.system().is_actor_terminated(&sibling));
    assert_eq!(attempts.load(Ordering::SeqCst), if restart { 2 } else { 1 });
    let sibling_path = sibling.path().to_string();
    let mut expected = Vec::new();
    if restart {
      expected.push((sibling_path.clone(), sibling_path.clone()));
    }
    expected.push((sibling_path, dead_letters));
    assert_eq!(*reports.lock().unwrap(), expected);
    spec.terminate();
  }
}

#[test]
fn kill_restarts_all_siblings_when_permitted_and_stops_them_when_exhausted() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let reports = Arc::new(Mutex::new(Vec::new()));
  let strategy = SupervisorStrategy::all_for_one(1, RestartWindow::Unbounded);
  let supervisor = spec.system_mut().actor_of(supervisor_props(strategy));
  let first = child_under(&supervisor, counting_props(Arc::new(AtomicUsize::new(0)), Arc::clone(&reports), false));
  let second = child_under(&supervisor, counting_props(Arc::new(AtomicUsize::new(0)), Arc::clone(&reports), false));

  first.tell("Kill", spec.test_actor());
  spec.system_mut().run_until_idle();
  for child in [&first, &second] {
    let reply = child.ask("generation").await_result(default_timeout());
    assert_eq!(reply.downcast_ref::<usize>(), Some(&2));
  }
  let recorded = reports.lock().unwrap().clone();
  assert_eq!(recorded.len(), 2);
  for child in [&first, &second] {
    assert!(recorded.contains(&(child.path().to_string(), child.path().to_string())));
  }

  second.tell("Kill", spec.test_actor());
  spec.system_mut().run_until_idle();
  assert!(spec.system().is_actor_terminated(&first));
  assert!(spec.system().is_actor_terminated(&second));
  spec.terminate();
}

#[test]
fn initialization_retry_creates_the_failed_child_and_restarts_its_sibling() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let reports = Arc::new(Mutex::new(Vec::new()));
  let strategy = SupervisorStrategy::all_for_one(1, RestartWindow::Unbounded);
  let supervisor = spec.system_mut().actor_of(supervisor_props(strategy));
  let sibling = child_under(&supervisor, counting_props(Arc::new(AtomicUsize::new(0)), Arc::clone(&reports), false));
  let attempts = Arc::new(AtomicUsize::new(0));
  let failed = child_under(
    &supervisor,
    Props::new({
      let reports = Arc::clone(&reports);
      move || {
        let generation = attempts.fetch_add(1, Ordering::SeqCst) + 1;
        CountingWorker { generation, restart_senders: Arc::clone(&reports), fail_start: generation == 1 }
      }
    }),
  );
  for child in [&failed, &sibling] {
    let reply = child.ask("generation").await_result(default_timeout());
    assert_eq!(reply.downcast_ref::<usize>(), Some(&2));
    assert!(!spec.system().is_actor_terminated(child));
  }
  assert_eq!(*reports.lock().unwrap(), vec![(sibling.path().to_string(), sibling.path().to_string())]);
  spec.terminate();
}

#[test]
fn decider_panics_on_kill_and_initialization_fail_the_supervisor() {
  for failure in [FailureKind::Killed, FailureKind::Initialization] {
    let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
    let generations = Arc::new(AtomicUsize::new(0));
    let supervisor = spec.system_mut().actor_of(Props::new({
      let generations = Arc::clone(&generations);
      move || {
        generations.fetch_add(1, Ordering::SeqCst);
        SupervisorActor {
          strategy: SupervisorStrategy::all_for_one(1, RestartWindow::Unbounded).with_decider(move |_, kind| {
            assert_eq!(kind, failure);
            panic!("decider failure");
          }),
          created: 0,
        }
      }
    }));
    let attempts = Arc::new(AtomicUsize::new(0));
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
      let child = child_under(
        &supervisor,
        Props::new(move || {
          let generation = attempts.fetch_add(1, Ordering::SeqCst) + 1;
          CountingWorker {
            generation,
            restart_senders: Arc::new(Mutex::new(Vec::new())),
            fail_start: failure == FailureKind::Initialization && generation == 1,
          }
        }),
      );
      // The child starts, and the supervisor decides for its failed start,
      // in the runs after the spawn (Pekko's `Create` and `Failed`).
      spec.system_mut().run_until_idle();
      if failure == FailureKind::Killed {
        child.tell("Kill", spec.test_actor());
        spec.system_mut().run_until_idle();
      }
      child
    }));
    let child = outcome.expect("the decider panic must be handled by the supervisor");
    assert_eq!(generations.load(Ordering::SeqCst), 2);
    assert!(!spec.system().is_actor_terminated(&supervisor));
    assert!(!spec.system().is_actor_terminated(&child));
    spec.terminate();
  }
}

struct ControlledClock(AtomicUsize);

impl Clock for ControlledClock {
  fn monotonic(&self) -> Duration {
    Duration::from_secs(self.0.load(Ordering::SeqCst) as u64)
  }
}

#[test]
fn a_restart_window_includes_its_boundary_and_reopens_after_expiry() {
  for elapsed in [10, 11] {
    let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
    let clock = Arc::new(ControlledClock(AtomicUsize::new(0)));
    spec.system().set_clock(clock.clone());
    let supervisor = spec
      .system_mut()
      .actor_of(supervisor_props(SupervisorStrategy::all_for_one(1, RestartWindow::Within(Duration::from_secs(10)))));
    let generations = Arc::new(AtomicUsize::new(0));
    let child =
      child_under(&supervisor, counting_props(Arc::clone(&generations), Arc::new(Mutex::new(Vec::new())), false));
    child.tell("boom", spec.test_actor());
    spec.system_mut().run_until_idle();
    assert_eq!(generations.load(Ordering::SeqCst), 2);
    clock.0.store(elapsed, Ordering::SeqCst);
    child.tell("boom", spec.test_actor());
    spec.system_mut().run_until_idle();
    assert_eq!(spec.system().is_actor_terminated(&child), elapsed == 10);
    assert_eq!(generations.load(Ordering::SeqCst), if elapsed == 10 { 2 } else { 3 });
    spec.terminate();
  }
}

#[test]
fn a_new_siblings_unused_allowance_does_not_replace_an_older_childs_history() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let clock = Arc::new(ControlledClock(AtomicUsize::new(0)));
  spec.system().set_clock(clock.clone());
  let supervisor = spec
    .system_mut()
    .actor_of(supervisor_props(SupervisorStrategy::all_for_one(1, RestartWindow::Within(Duration::from_secs(10)))));
  let older_count = Arc::new(AtomicUsize::new(0));
  let older =
    child_under(&supervisor, counting_props(Arc::clone(&older_count), Arc::new(Mutex::new(Vec::new())), false));
  older.tell("boom", spec.test_actor());
  spec.system_mut().run_until_idle();
  assert_eq!(older_count.load(Ordering::SeqCst), 2);
  clock.0.store(5, Ordering::SeqCst);
  let newer_count = Arc::new(AtomicUsize::new(0));
  let newer =
    child_under(&supervisor, counting_props(Arc::clone(&newer_count), Arc::new(Mutex::new(Vec::new())), false));
  newer.tell("boom", spec.test_actor());
  spec.system_mut().run_until_idle();
  assert!(spec.system().is_actor_terminated(&older));
  assert!(spec.system().is_actor_terminated(&newer));
  assert_eq!(older_count.load(Ordering::SeqCst), 2);
  assert_eq!(newer_count.load(Ordering::SeqCst), 1);
  spec.terminate();
}

struct StopFromHook(ActorRef);

impl Actor for StopFromHook {
  fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {}

  fn post_stop_with_context(&mut self, context: &mut ActorContext<'_>) {
    context.stop(&self.0);
  }
}

#[test]
fn kill_and_weak_stop_drain_requests_from_post_stop() {
  for kill in [false, true] {
    let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
    let other = spec.system_mut().actor_of(reporting_props(Arc::new(Mutex::new(Vec::new()))));
    let stopping = spec.system_mut().actor_of(Props::new({
      let other = other.clone();
      move || StopFromHook(other.clone())
    }));
    if kill {
      stopping.tell("Kill", spec.test_actor());
    } else {
      spec.system().downgrade().stop(&stopping);
    }
    spec.system_mut().run_until_idle();
    assert!(spec.system().is_actor_terminated(&other));
    spec.terminate();
  }
}

struct SpawnFromStop(Arc<AtomicUsize>);

impl Actor for SpawnFromStop {
  fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {}

  fn post_stop_with_context(&mut self, context: &mut ActorContext<'_>) {
    context.actor_of_with_strategy(
      counting_props(Arc::clone(&self.0), Arc::new(Mutex::new(Vec::new())), false),
      SupervisorStrategy::default_strategy(),
    );
  }
}

#[test]
fn post_stop_cannot_create_an_explicitly_supervised_child() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let starts = Arc::new(AtomicUsize::new(0));
  let parent = spec.system_mut().actor_of(Props::new({
    let starts = Arc::clone(&starts);
    move || SpawnFromStop(Arc::clone(&starts))
  }));
  spec.system_mut().stop(&parent);
  // The parent stops in its own run (Pekko's `Terminate`).
  spec.system_mut().run_until_idle();
  assert!(spec.system().is_actor_terminated(&parent));
  assert_eq!(starts.load(Ordering::SeqCst), 0);
  spec.terminate();
}

struct StopsSiblingDuringRestart(Arc<Mutex<Option<ActorRef>>>);

impl Actor for StopsSiblingDuringRestart {
  fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {}

  fn pre_restart(&mut self, context: &mut ActorContext<'_>, _reason: &(dyn core::any::Any + Send)) {
    context.stop(self.0.lock().unwrap().as_ref().unwrap());
  }
}

struct ReplyOnFailure;

impl Actor for ReplyOnFailure {
  fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {
    panic!("boom");
  }

  fn pre_restart(&mut self, context: &mut ActorContext<'_>, _reason: &(dyn core::any::Any + Send)) {
    context.sender().tell("restarting", context.self_ref());
  }

  fn post_stop_with_context(&mut self, context: &mut ActorContext<'_>) {
    context.sender().tell("stopping", context.self_ref());
  }
}

#[test]
fn sibling_restart_hook_cannot_stop_the_failed_child_before_its_reply() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let supervisor =
    spec.system_mut().actor_of(supervisor_props(SupervisorStrategy::all_for_one(1, RestartWindow::Unbounded)));
  let failed_child = Arc::new(Mutex::new(None));
  child_under(
    &supervisor,
    Props::new({
      let failed_child = Arc::clone(&failed_child);
      move || StopsSiblingDuringRestart(Arc::clone(&failed_child))
    }),
  );
  let failing = child_under(&supervisor, Props::new(|| ReplyOnFailure));
  *failed_child.lock().unwrap() = Some(failing.clone());
  failing.tell("boom", spec.test_actor());
  spec.system_mut().run_until_idle();
  spec.expect_msg(&"restarting");
  assert!(spec.system().is_actor_terminated(&failing));
  spec.terminate();
}

#[test]
fn sibling_termination_hook_cannot_stop_the_failed_child_before_its_reply() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let failed_child: Arc<Mutex<Option<ActorRef>>> = Arc::new(Mutex::new(None));
  let strategy = SupervisorStrategy::all_for_one(0, RestartWindow::Unbounded).with_child_terminated({
    let failed_child = Arc::clone(&failed_child);
    move |context, _, _| context.stop(failed_child.lock().unwrap().as_ref().unwrap())
  });
  let supervisor = spec.system_mut().actor_of(supervisor_props(strategy));
  child_under(&supervisor, reporting_props(Arc::new(Mutex::new(Vec::new()))));
  let failing = child_under(&supervisor, Props::new(|| ReplyOnFailure));
  *failed_child.lock().unwrap() = Some(failing.clone());
  failing.tell("boom", spec.test_actor());
  spec.system_mut().run_until_idle();
  spec.expect_msg(&"stopping");
  spec.terminate();
}

#[test]
fn a_finite_restart_window_works_with_the_standard_channel_boundary() {
  let mut system =
    ActorSystem::with_failure_catcher(SPEC_SYSTEM_NAME, Arc::new(ChannelBoundary::new()), Arc::new(UnwindCatcher));
  attach_wait_driver(&system);
  let strategy = SupervisorStrategy::all_for_one(1, RestartWindow::Within(Duration::from_secs(10)));
  let supervisor = system.actor_of(supervisor_props(strategy));
  let generations = Arc::new(AtomicUsize::new(0));
  let child =
    child_under(&supervisor, counting_props(Arc::clone(&generations), Arc::new(Mutex::new(Vec::new())), false));
  child.tell("boom", system.dead_letters());
  system.run_until_idle();
  assert_eq!(generations.load(Ordering::SeqCst), 2);
  system.terminate();
}
