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

use super::*;

use std::sync::Mutex;

use crate::adaptor::ChannelBoundary;
use crate::adaptor::UnwindCatcher;
use crate::kernel::actor::Actor;
use crate::kernel::actor_system::ActorSystem;

#[derive(Default)]
struct RecoveryObservations {
  attempts: usize,
  restarts: usize,
  active_recoveries: usize,
  restart_depths: Vec<usize>,
  worker_creations: Vec<bool>,
  workers: Vec<ActorRef>,
  previous_worker_stopped: Vec<bool>,
  duplicate_worker_rejected: Option<bool>,
  child: Option<ActorRef>,
  received: Vec<&'static str>,
  terminated: Vec<ActorRef>,
  events: Vec<(String, String)>,
}

struct RecoveryScope(Arc<Mutex<RecoveryObservations>>);

impl RecoveryScope {
  fn enter(observations: &Arc<Mutex<RecoveryObservations>>) -> Self {
    observations.lock().unwrap().active_recoveries += 1;
    Self(Arc::clone(observations))
  }
}

impl Drop for RecoveryScope {
  fn drop(&mut self) {
    self.0.lock().unwrap().active_recoveries -= 1;
  }
}

struct RecoveryEventSink(Arc<Mutex<RecoveryObservations>>);

impl EventSink for RecoveryEventSink {
  fn publish(&self, event: &dyn crate::kernel::actor::Logging::LogEvent) {
    self
      .0
      .lock()
      .unwrap()
      .events
      .push((event.cause_name().expect("failure has a cause").to_string(), event.log_source().to_string()));
  }

  fn into_any(self: Arc<Self>) -> Arc<dyn core::any::Any + Send + Sync> {
    self
  }
}

struct HealthyChild {
  value: usize,
  values: Arc<Mutex<Vec<(ActorRef, usize)>>>,
}

impl Actor for HealthyChild {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    match message.downcast_ref::<&'static str>() {
      | Some(&"increment") => self.value += 1,
      | Some(&"read") => self.values.lock().unwrap().push((context.self_ref(), self.value)),
      | _ => {},
    }
  }
}

struct Guardian;

mod initialization_restart_window {
  use super::*;
  use crate::kernel::supervision::{RestartLimit, RestartWindow};

  struct ControlledClock(Mutex<Duration>);

  impl Clock for ControlledClock {
    fn monotonic(&self) -> Duration {
      *self.0.lock().unwrap()
    }
  }

  struct Worker {
    clock: Arc<ControlledClock>,
    cleanup_time: Duration,
  }

  impl Actor for Worker {
    fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {}

    fn post_stop(&mut self) {
      *self.clock.0.lock().unwrap() += self.cleanup_time;
    }
  }

  #[derive(Default)]
  struct Observations {
    attempts: Vec<ActorRef>,
    worker: Option<ActorRef>,
    received: Vec<ActorRef>,
    terminated: Vec<ActorRef>,
  }

  struct Employee {
    clock: Arc<ControlledClock>,
    cleanup_time: Duration,
    observed: Arc<Mutex<Observations>>,
  }

  impl Actor for Employee {
    fn pre_start(&mut self, context: &mut ActorContext<'_>) {
      let attempt = {
        let mut observed = self.observed.lock().unwrap();
        observed.attempts.push(context.self_ref());
        observed.attempts.len()
      };
      if attempt == 1 {
        let clock = self.clock.clone();
        let cleanup_time = self.cleanup_time;
        let worker = context.actor_of(Props::new(move || Worker { clock: clock.clone(), cleanup_time }), "worker");
        self.observed.lock().unwrap().worker = Some(worker.expect("worker starts"));
        *self.clock.0.lock().unwrap() = Duration::from_millis(100);
      }
      assert!(attempt > 2, "initialization fails twice");
    }

    fn receive(&mut self, context: &mut ActorContext<'_>, _message: Message) {
      self.observed.lock().unwrap().received.push(context.self_ref());
    }
  }

  struct Parent {
    props: Props,
    strategy: SupervisorStrategy,
    child: Arc<Mutex<Option<ActorRef>>>,
  }

  impl Actor for Parent {
    fn receive(&mut self, context: &mut ActorContext<'_>, _message: Message) {
      *self.child.lock().unwrap() = Some(context.actor_of(self.props.clone(), "employee").expect("employee starts"));
    }

    fn supervisor_strategy(&self) -> SupervisorStrategy {
      self.strategy.clone()
    }
  }

  fn run(cleanup_millis: u64, expected_attempts: usize, stopped: bool) {
    let observed = Arc::new(Mutex::new(Observations::default()));
    let clock = Arc::new(ControlledClock(Mutex::new(Duration::ZERO)));
    let props = Props::new({
      let observed = observed.clone();
      let clock = clock.clone();
      move || Employee {
        clock: clock.clone(),
        cleanup_time: Duration::from_millis(cleanup_millis),
        observed: observed.clone(),
      }
    });
    let notices = observed.clone();
    let strategy = SupervisorStrategy::one_for_one_with_window(
      RestartLimit::new(1),
      RestartWindow::Within(Duration::from_millis(10)),
    )
    .with_decider(|_, _| Directive::Restart)
    .with_child_terminated(move |_, child, _| notices.lock().unwrap().terminated.push(child.clone()));
    let child = Arc::new(Mutex::new(None));
    let mut system = ActorSystem::with_failure_catcher(
      "InitializationWindow",
      Arc::new(ChannelBoundary::new()),
      Arc::new(UnwindCatcher),
    );
    system.set_clock(clock.clone());
    let parent = system.actor_of(Props::new({
      let child = child.clone();
      move || Parent { props: props.clone(), strategy: strategy.clone(), child: child.clone() }
    }));
    parent.tell("start", system.dead_letters());
    system.run_until_idle();
    let child = child.lock().unwrap().clone().expect("same employee handle returned");
    let worker = observed.lock().unwrap().worker.clone().expect("first attempt created worker");
    assert!(system.is_actor_terminated(&worker));
    assert_eq!(clock.monotonic(), Duration::from_millis(100 + cleanup_millis));
    assert_eq!(system.is_actor_terminated(&child), stopped);
    child.tell("ping", parent.clone());
    system.run_until_idle();
    let observed = observed.lock().unwrap();
    assert_eq!(observed.attempts.len(), expected_attempts);
    assert!(observed.attempts.iter().all(|attempt| attempt == &child));
    assert_eq!(observed.received, if stopped { Vec::new() } else { alloc::vec![child.clone()] });
    assert_eq!(observed.terminated, if stopped { alloc::vec![child] } else { Vec::new() });
  }

  #[test]
  fn cleanup_past_the_window_allows_the_same_child_to_finish_initializing() {
    run(11, 3, false);
  }

  #[test]
  fn cleanup_at_the_inclusive_boundary_stops_the_same_child() {
    run(10, 2, true);
  }
}

impl Actor for Guardian {
  fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {}
}

/// Regression (review of PR #230): the dead letters handler used to
/// capture its own handle strongly, cycling handle -> boundary ->
/// handler -> handle, so dropping an executor leaked the boundary, the
/// handler, and the shared routing state.
#[test]
fn dropping_the_executor_releases_the_dead_letters_boundary() {
  let executor = death_watch_executor();
  let boundary = Arc::downgrade(executor.dead_letters().boundary().expect("dead letters posts to a boundary"));
  drop(executor);
  assert!(boundary.upgrade().is_none(), "the dead letters handle must not keep its own boundary alive");
}

fn death_watch_executor() -> Executor {
  let address = crate::kernel::address::Address::new("pekko", "DeathWatch");
  let root = crate::kernel::actor_path::RootActorPath::new(address);
  Executor::new(ActorPath::from_root(root), Arc::new(ChannelBoundary::new()))
}

#[test]
fn a_detached_watch_survives_an_old_reference_termination_with_the_same_uid() {
  let mut executor = death_watch_executor();
  let old = executor.try_spawn_user(Props::new(|| Guardian), None).unwrap();
  let watcher = executor.try_spawn_user(Props::new(|| Guardian), None).unwrap();
  executor.stop(&old);
  // The actor stops in its own run (Pekko's `Terminate`).
  executor.run_until_idle();
  let detached = executor.stopped_noop_ref(&old);
  assert_eq!(detached, old);
  executor.watch(&watcher, &detached);

  assert_eq!(executor.watchers.len(), 1);
  executor.notify_watchers(&old, true, false);
  assert!(executor.pending_terminations.is_empty());
  assert_eq!(executor.watchers.len(), 1);
  assert!(executor.watchers[0].1.is_detached());

  executor.unwatch(&watcher, &detached);
  assert!(executor.watchers.is_empty());
}

struct TerminationRecorder(Arc<Mutex<Vec<ActorRef>>>);

impl Actor for TerminationRecorder {
  fn receive(&mut self, _context: &mut ActorContext<'_>, message: Message) {
    if let Some(notice) = message.downcast_ref::<Terminated>() {
      self.0.lock().unwrap().push(notice.target().clone());
    }
  }
}

#[test]
fn unwatch_cancels_the_queued_notice_without_cancelling_a_new_watch() {
  let mut executor = death_watch_executor();
  let target = executor.try_spawn_user(Props::new(|| Guardian), None).unwrap();
  let received = Arc::new(Mutex::new(Vec::new()));
  let inbox = received.clone();
  let watcher = executor.try_spawn_user(Props::new(move || TerminationRecorder(inbox.clone())), None).unwrap();
  executor.stop(&target);
  // Force notices through the boundary queue as well as the local executor.
  watcher.tell("queued first", watcher.clone());
  executor.watch(&watcher, &target);
  executor.unwatch(&watcher, &target);
  executor.watch(&watcher, &target);
  executor.run_until_idle();

  assert_eq!(*received.lock().unwrap(), alloc::vec![target]);
}

struct UnwatchBeforeDelivery {
  target: ActorRef,
  observed: Arc<Mutex<Vec<&'static str>>>,
}

impl Actor for UnwatchBeforeDelivery {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    if message == "stop target" {
      context.watch(&self.target);
      context.tell(&context.self_ref(), "unwatch target");
      context.stop(&self.target);
    } else if message == "unwatch target" {
      context.unwatch(&self.target);
      self.observed.lock().unwrap().push("unwatched");
    } else if message.is::<Terminated>() {
      self.observed.lock().unwrap().push("terminated");
    }
  }
}

#[test]
fn an_actor_can_unwatch_after_the_target_stops_and_before_its_notice_is_delivered() {
  let mut executor = death_watch_executor();
  let target = executor.try_spawn_user(Props::new(|| Guardian), None).unwrap();
  let observed = Arc::new(Mutex::new(Vec::new()));
  let trace = observed.clone();
  let target_ref = target.clone();
  let watcher = executor
    .try_spawn_user(
      Props::new(move || UnwatchBeforeDelivery { target: target_ref.clone(), observed: trace.clone() }),
      None,
    )
    .unwrap();

  watcher.tell("stop target", watcher.clone());
  executor.run_until_idle();

  assert!(executor.is_actor_terminated(&target));
  assert_eq!(*observed.lock().unwrap(), alloc::vec!["unwatched"]);
}

struct RecoveringParent {
  observations: Arc<Mutex<RecoveryObservations>>,
  failures: usize,
  stop_on_success: bool,
  spawn_worker: bool,
  strategy: SupervisorStrategy,
}

impl Actor for RecoveringParent {
  fn receive(&mut self, context: &mut ActorContext<'_>, _message: Message) {
    let observations = Arc::clone(&self.observations);
    let failures = self.failures;
    let stop_on_success = self.stop_on_success;
    let spawn_worker = self.spawn_worker;
    let child = context
      .actor_of(
        Props::new(move || RecoveringChild {
          observations: Arc::clone(&observations),
          failures,
          stop_on_success,
          spawn_worker,
        }),
        "bob",
      )
      .expect("the child name is free");
    self.observations.lock().unwrap().child = Some(child);
  }

  fn pre_restart(&mut self, context: &mut ActorContext<'_>, _reason: &(dyn core::any::Any + Send)) {
    let scope = RecoveryScope::enter(&self.observations);
    // The discarded behavior stack outlives child recovery, unlike this hook.
    context.change_behavior(move |_, _| {
      let _scope = &scope;
    });
  }

  fn post_restart(&mut self, _context: &mut ActorContext<'_>) {
    let mut observations = self.observations.lock().unwrap();
    let depth = observations.active_recoveries;
    observations.restarts += 1;
    observations.restart_depths.push(depth);
  }

  fn supervisor_strategy(&self) -> SupervisorStrategy {
    self.strategy.clone()
  }
}

struct RecoveringChild {
  observations: Arc<Mutex<RecoveryObservations>>,
  failures: usize,
  stop_on_success: bool,
  spawn_worker: bool,
}

impl Actor for RecoveringChild {
  fn pre_start(&mut self, context: &mut ActorContext<'_>) {
    let attempt = {
      let mut observations = self.observations.lock().unwrap();
      observations.attempts += 1;
      observations.attempts
    };
    if self.spawn_worker {
      let previous = self.observations.lock().unwrap().workers.last().cloned();
      if let Some(previous) = previous {
        let stopped = context.is_terminated(&previous);
        self.observations.lock().unwrap().previous_worker_stopped.push(stopped);
      }
      let worker = context.actor_of(Props::new(|| Guardian), "worker");
      {
        let mut observations = self.observations.lock().unwrap();
        observations.worker_creations.push(worker.is_ok());
        if let Ok(reference) = &worker {
          observations.workers.push(reference.clone());
        }
      }
      worker.expect("a failed attempt must release its worker name before recovery");
    }
    assert!(attempt > self.failures, "child initialization failed");
    if self.stop_on_success {
      context.stop(&context.self_ref());
    }
  }

  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    if let Some(text) = message.downcast_ref::<&'static str>() {
      self.observations.lock().unwrap().received.push(text);
      if self.spawn_worker {
        let rejected = context.actor_of(Props::new(|| Guardian), "worker").is_err();
        self.observations.lock().unwrap().duplicate_worker_rejected = Some(rejected);
      }
    }
  }
}

fn run_recovery(
  failures: usize,
  max_retries: u32,
  later_decision: Option<Directive>,
  stop_on_success: bool,
  spawn_worker: bool,
) -> (usize, usize, usize, bool, bool, usize) {
  let observations = Arc::new(Mutex::new(RecoveryObservations::default()));
  let decisions = Arc::new(Mutex::new(0));
  let notices = Arc::clone(&observations);
  let strategy = SupervisorStrategy::one_for_one(3)
    .with_decider(move |_, _| {
      let decision = {
        let mut count = decisions.lock().unwrap();
        *count += 1;
        *count
      };
      if decision > 1 {
        if let Some(directive) = later_decision {
          return directive;
        }
      }
      panic!("decider failed");
    })
    .with_child_terminated(move |_, child, _| notices.lock().unwrap().terminated.push(child.clone()));
  let mut system =
    ActorSystem::with_failure_catcher("RecoverySpec", Arc::new(ChannelBoundary::new()), Arc::new(UnwindCatcher));
  system.set_event_sink(Arc::new(RecoveryEventSink(Arc::clone(&observations))));
  let guardian = system.actor_of(Props::new(|| Guardian));
  let parent = system.downgrade().spawn_child(
    &guardian,
    Props::new({
      let observations = Arc::clone(&observations);
      move || RecoveringParent {
        observations: Arc::clone(&observations),
        failures,
        stop_on_success,
        spawn_worker,
        strategy: strategy.clone(),
      }
    }),
    Some("parent"),
    SupervisorStrategy::one_for_one(max_retries),
  );
  let healthy_creations = Arc::new(Mutex::new(0));
  let values = Arc::new(Mutex::new(Vec::new()));
  let healthy = system.downgrade().spawn_child(
    &parent,
    Props::new({
      let creations = Arc::clone(&healthy_creations);
      let values = Arc::clone(&values);
      move || {
        *creations.lock().unwrap() += 1;
        HealthyChild { value: 0, values: Arc::clone(&values) }
      }
    }),
    Some("healthy"),
    SupervisorStrategy::default_strategy(),
  );
  healthy.tell("increment", guardian.clone());
  healthy.tell("read", guardian.clone());
  system.run_until_idle();
  assert_eq!(*values.lock().unwrap(), alloc::vec![(healthy.clone(), 1)]);

  parent.tell("go", guardian.clone());
  system.run_until_idle();
  let child = observations.lock().unwrap().child.clone().expect("creation returned the child handle");
  if spawn_worker {
    let observed = observations.lock().unwrap();
    let stopped: Vec<bool> = observed.workers.iter().map(|worker| system.is_actor_terminated(worker)).collect();
    std::eprintln!(
      "worker creations: {:?}; previous worker stopped before retry: {:?}; worker stopped at end: {stopped:?}",
      observed.worker_creations,
      observed.previous_worker_stopped
    );
    assert_eq!(observed.worker_creations, alloc::vec![true, true]);
    assert_eq!(observed.previous_worker_stopped, alloc::vec![true]);
    assert_eq!(stopped, alloc::vec![true, false]);
    assert_ne!(observed.workers[0], observed.workers[1]);
  }
  let parent_stopped = system.is_actor_terminated(&parent);
  let child_stopped = system.is_actor_terminated(&child);
  assert_eq!(system.is_actor_terminated(&healthy), parent_stopped);
  if !parent_stopped {
    healthy.tell("read", guardian.clone());
    system.run_until_idle();
    let parent_restarts = observations.lock().unwrap().restarts;
    // D4b-1 (A): the parent keeps `healthy` across its restart, and the
    // `Recreate` the parent's later half sends recreates it, so the child is
    // created once up front and once more per parent restart.
    assert_eq!(*healthy_creations.lock().unwrap(), 1 + parent_restarts);
    // The recreated instance starts empty again, so the read after the
    // restart sees a fresh value.
    assert_eq!(*values.lock().unwrap(), alloc::vec![(healthy.clone(), 1), (healthy, 0)]);
  }
  if !child_stopped {
    child.tell("recovered", guardian);
    system.run_until_idle();
    assert_eq!(observations.lock().unwrap().received, alloc::vec!["recovered"]);
    if spawn_worker {
      assert_eq!(observations.lock().unwrap().duplicate_worker_rejected, Some(true));
    }
  }
  let observed = observations.lock().unwrap();
  assert_eq!(observed.active_recoveries, 0, "all parent recovery scopes must be released");
  assert_eq!(observed.restart_depths.len(), observed.restarts);
  assert!(observed.restart_depths.iter().all(|depth| *depth > 0), "parent recovery must be observed");
  let depth = *observed.restart_depths.iter().max().expect("parent restart was observed");
  std::eprintln!(
    "overlapping parent recoveries: {:?}; active at end: {}",
    observed.restart_depths,
    observed.active_recoveries
  );
  let child_notices = observed.terminated.iter().filter(|notice| *notice == &child).count();
  assert!(observed.terminated.iter().all(|notice| notice == &child || parent_stopped));
  let initialization_failures = observed.attempts.min(failures);
  let decider_failures = if later_decision.is_some() { 1 } else { initialization_failures };
  assert_eq!(
    observed
      .events
      .iter()
      .filter(|(cause, source)| cause == ACTOR_INITIALIZATION_CAUSE && source == &child.path().to_string())
      .count(),
    initialization_failures
  );
  assert_eq!(
    observed
      .events
      .iter()
      .filter(|(cause, source)| cause == RUNTIME_CAUSE && source == &parent.path().to_string())
      .count(),
    decider_failures
  );
  assert_eq!(observed.events.len(), initialization_failures + decider_failures);
  std::eprintln!("failures={failures} limit={max_retries} attempts={} restarts={} depth={depth} parent_stopped={parent_stopped} child_stopped={child_stopped} notices={}", observed.attempts, observed.restarts, observed.terminated.len());
  (observed.attempts, observed.restarts, depth, parent_stopped, child_stopped, child_notices)
}

#[test]
fn repeated_child_recovery_failures_do_not_nest_parent_recovery() {
  assert_eq!(run_recovery(1, 3, None, false, false), (2, 1, 1, false, false, 0));
  assert_eq!(run_recovery(3, 3, None, false, false), (4, 3, 1, false, false, 0));
}

#[test]
fn a_retained_child_handles_messages_with_a_fresh_behavior_after_kill_recovery() {
  let mut system = ActorSystem::new("RecoverySpec", Arc::new(ChannelBoundary::new()));
  let guardian = system.actor_of(Props::new(|| Guardian));
  let observations = Arc::new(Mutex::new(RecoveryObservations::default()));
  let parent = system.downgrade().spawn_child(
    &guardian,
    Props::new({
      let observations = Arc::clone(&observations);
      move || RecoveringParent {
        observations: Arc::clone(&observations),
        failures: 0,
        stop_on_success: false,
        spawn_worker: false,
        strategy: SupervisorStrategy::default_strategy(),
      }
    }),
    Some("parent"),
    SupervisorStrategy::one_for_one(3),
  );
  let values = Arc::new(Mutex::new(Vec::new()));
  let child = system.downgrade().spawn_child(
    &parent,
    Props::new({
      let values = Arc::clone(&values);
      move || HealthyChild { value: 0, values: Arc::clone(&values) }
    }),
    Some("child"),
    SupervisorStrategy::default_strategy(),
  );
  child.tell("increment", guardian.clone());
  child.tell("read", guardian.clone());
  system.run_until_idle();

  parent.tell("Kill", guardian.clone());
  system.run_until_idle();
  child.tell("read", guardian);
  system.run_until_idle();

  assert_eq!(observations.lock().unwrap().restarts, 1);
  assert_eq!(*values.lock().unwrap(), alloc::vec![(child.clone(), 1), (child, 0)]);
}

#[test]
fn repeated_child_recovery_failures_obey_parent_retry_limit() {
  assert_eq!(run_recovery(3, 2, None, false, false), (3, 2, 1, true, true, 1));
}

#[test]
fn child_recovery_preserves_restart_and_stop_decisions() {
  assert_eq!(run_recovery(3, 3, Some(Directive::Restart), false, false), (4, 1, 1, false, false, 0));
  assert_eq!(run_recovery(3, 3, Some(Directive::Stop), false, false), (2, 1, 1, false, true, 1));
}

#[test]
fn child_recovery_drains_stop_requests_and_termination_notices() {
  assert_eq!(run_recovery(1, 3, None, false, false), (2, 1, 1, false, false, 0));
  assert_eq!(run_recovery(1, 3, None, true, false), (2, 1, 1, false, true, 1));
}

#[test]
fn child_recovery_stops_workers_from_the_failed_initialization_attempt() {
  assert_eq!(run_recovery(1, 3, None, false, false), (2, 1, 1, false, false, 0));
  assert_eq!(run_recovery(1, 3, None, false, true), (2, 1, 1, false, false, 0));
}

impl Executor {
  /// Returns how many parent entries the registry holds.
  ///
  /// A stopped actor must not leave its entry behind, so the tests read the
  /// count after a spawn-and-stop cycle. This accessor exists only for tests
  /// and is not part of the crate's API.
  pub(crate) fn parent_entry_count(&self) -> usize {
    self.actors.values().filter(|entry| entry.supervision.is_some()).count() + self.ref_parents.len()
  }

  /// Returns the suspend count of `target`'s mailbox, or `None` when the actor
  /// has stopped.
  ///
  /// Stage 4b (design of #234, section 11.13.8) makes `suspend_count` a
  /// production read entry on `Mailbox`; this accessor stands in for it until
  /// then, so the stage 4b unit tests can read the count without the executor
  /// exposing its entries.
  pub(crate) fn suspend_count_of(&self, target: &ActorRef) -> Option<usize> {
    self.entry(target).map(|entry| entry.mailbox.suspend_count())
  }
}

#[cfg(test)]
mod uid_clash_restart {
  //! Supporting checks for the invariants the UidClash restart path relies on.
  //!
  //! Each test drives a real supervised restart through the executor and reads
  //! the ordering it produces. They complement the ported case in
  //! `crates/fraktor-actor/tests/uid_clash_test.rs`: the ported case owns the
  //! "no bogus Terminated" contract, and these read the individual steps it
  //! rests on so a partial implementation shows up as a focused failure.

  use std::sync::Mutex;

  use alloc::sync::Arc;

  use crate::adaptor::ChannelBoundary;
  use crate::adaptor::UnwindCatcher;
  use crate::kernel::actor::Actor;
  use crate::kernel::actor::ActorContext;
  use crate::kernel::actor::Props;
  use crate::kernel::actor::Terminated;
  use crate::kernel::actor_ref::ActorRef;
  use crate::kernel::actor_system::ActorSystem;
  use crate::kernel::executor::ShardBoundary;
  use crate::kernel::message::Message;
  use crate::kernel::supervision::SupervisorStrategy;

  /// A system whose actors run under the real panic catcher and a boundary that
  /// keeps messages queued until `run_until_idle` drains them. `ActorSystem::new`
  /// neither catches a failing hook nor keeps a queue, so the restart cases need
  /// both the catcher (to turn a hook panic into a supervision event) and the
  /// queue (so the post_restart self send is delivered, not dropped).
  fn system(name: &str) -> ActorSystem {
    let boundary: Arc<dyn ShardBoundary> = Arc::new(ChannelBoundary::new());
    ActorSystem::with_failure_catcher(name, boundary, Arc::new(UnwindCatcher))
  }

  #[derive(Default)]
  struct RestartObservations {
    steps: Vec<&'static str>,
    // Whether the watched grandchild had finished stopping when `post_restart`
    // started a replacement.
    grandchild_stopped_before_post_restart: Option<bool>,
    // What this actor saw for the grandchild it watched and then stopped in
    // `pre_restart`.
    grandchild_terminated_notice: Option<bool>,
    old_grandchild: Option<ActorRef>,
    sibling_alive_in_post_restart: Option<bool>,
  }

  // A leaf actor that does nothing; the restart cases only need it to exist and
  // to be stoppable.
  struct Leaf;

  impl Actor for Leaf {
    fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {}
  }

  // The actor whose restart the case drives. It is supervised for a restart,
  // keeps one watched grandchild, and reads the grandchild's stop state around
  // its own `post_restart`.
  struct RestartedParent {
    observations: Arc<Mutex<RestartObservations>>,
    grandchild: Option<ActorRef>,
    sibling: Option<ActorRef>,
  }

  impl Actor for RestartedParent {
    fn receive(&mut self, _context: &mut ActorContext<'_>, message: Message) {
      if message.downcast_ref::<&'static str>() == Some(&"fail") {
        panic!("restart on demand");
      }
      if message.downcast_ref::<Terminated>().is_some() {
        self.observations.lock().expect("observations are not poisoned").grandchild_terminated_notice = Some(true);
      }
    }

    fn pre_start(&mut self, context: &mut ActorContext<'_>) {
      let grandchild = context.actor_of(Props::new(|| Leaf), "grandchild").expect("the grandchild name is free");
      self.grandchild = Some(context.watch(&grandchild));
      self.observations.lock().expect("observations are not poisoned").steps.push("grandchild started");
    }

    fn pre_restart(&mut self, context: &mut ActorContext<'_>, _reason: &(dyn core::any::Any + Send)) {
      if let Some(sibling) = &self.sibling {
        context.stop(sibling);
      }
      if let Some(grandchild) = self.grandchild.clone() {
        let mut observations = self.observations.lock().expect("observations are not poisoned");
        observations.steps.push("pre_restart");
        observations.old_grandchild = Some(grandchild.clone());
        drop(observations);
        context.stop(&grandchild);
      }
    }

    fn post_restart(&mut self, context: &mut ActorContext<'_>) {
      // Read the old grandchild from shared state, not `self`: the replacement
      // is a fresh instance whose own `grandchild` is still `None` at this
      // point, so only the shared handle reaches the instance that stopped it.
      let old_grandchild = self.observations.lock().expect("observations are not poisoned").old_grandchild.clone();
      let stopped = old_grandchild.map(|old_grandchild| context.is_terminated(&old_grandchild));
      {
        let mut observations = self.observations.lock().expect("observations are not poisoned");
        observations.steps.push("post_restart");
        observations.grandchild_stopped_before_post_restart = stopped;
        observations.sibling_alive_in_post_restart =
          self.sibling.as_ref().map(|sibling| !context.is_terminated(sibling));
      }
    }
  }

  fn spawn_restarted_parent(
    system: &mut ActorSystem,
    guardian: &ActorRef,
    observations: &Arc<Mutex<RestartObservations>>,
    sibling: Option<ActorRef>,
  ) -> ActorRef {
    let observations = Arc::clone(observations);
    system.downgrade().spawn_child(
      guardian,
      Props::new(move || RestartedParent {
        observations: Arc::clone(&observations),
        grandchild: None,
        sibling: sibling.clone(),
      }),
      Some("parent"),
      SupervisorStrategy::one_for_one(1),
    )
  }

  #[test]
  fn a_restart_completes_the_stop_of_the_grandchild_before_post_restart_replaces_it() {
    // The grandchild is stopped in `pre_restart`; `post_restart` must observe it
    // already stopped so the replacement never overlaps the old instance.
    let observations = Arc::new(Mutex::new(RestartObservations::default()));
    let mut system = system("RestartGuardSpec");
    let guardian = system.actor_of(Props::new(|| Leaf));
    let parent = spawn_restarted_parent(&mut system, &guardian, &observations, None);
    parent.tell("fail", parent.clone());
    system.run_until_idle();
    let observed = observations.lock().expect("observations are not poisoned");
    assert_eq!(observed.steps, alloc::vec!["grandchild started", "pre_restart", "post_restart"]);
    assert_eq!(
      observed.grandchild_stopped_before_post_restart,
      Some(true),
      "the stopped grandchild must report terminated inside post_restart"
    );
    drop(observed);
    system.terminate();
  }

  #[test]
  fn a_restart_keeps_the_watch_response_for_the_stopped_grandchild() {
    // The parent watched the grandchild. Stopping it must reach the parent as a
    // Terminated, which a no-op watch response would not replace.
    let observations = Arc::new(Mutex::new(RestartObservations::default()));
    let mut system = system("RestartWatchSpec");
    let guardian = system.actor_of(Props::new(|| Leaf));
    let parent = spawn_restarted_parent(&mut system, &guardian, &observations, None);
    parent.tell("fail", parent.clone());
    system.run_until_idle();
    let observed = observations.lock().expect("observations are not poisoned");
    assert_eq!(observed.grandchild_terminated_notice, Some(true), "the watcher must learn the grandchild stopped");
    drop(observed);
    system.terminate();
  }

  #[test]
  fn a_restart_of_one_child_keeps_the_other_child_stop_waiting() {
    let observations = Arc::new(Mutex::new(RestartObservations::default()));
    let mut system = system("RestartSiblingStopSpec");
    let guardian = system.actor_of(Props::new(|| Leaf));
    let sibling = system.downgrade().spawn_child(
      &guardian,
      Props::new(|| Leaf),
      Some("sibling"),
      SupervisorStrategy::one_for_one(1),
    );
    let received = Arc::new(Mutex::new(Vec::new()));
    let inbox = received.clone();
    let watcher = system.actor_of(Props::new(move || super::TerminationRecorder(inbox.clone())));
    system.watch(&watcher, &sibling);
    let restarted = spawn_restarted_parent(&mut system, &guardian, &observations, Some(sibling.clone()));

    // The failing child requests both stops in pre_restart. Its own child's
    // stop completes before post_restart; the restart waits for no other.
    restarted.tell("fail", guardian.clone());
    system.run_until_idle();

    let observed = observations.lock().unwrap();
    assert_eq!(observed.steps, alloc::vec!["grandchild started", "pre_restart", "post_restart"]);
    assert_eq!(observed.grandchild_stopped_before_post_restart, Some(true));
    // Whether the sibling is still alive in `post_restart` is not checked:
    // Pekko leaves it unordered. A stop of an actor that is not one's own
    // child marks nothing to wait for (Children.scala:132-133, `shallDie`
    // only for a child in `childrenRefs`), and the restart waits only for its
    // own dying children (FaultHandling.scala:114,
    // `setChildrenTerminationReason(Recreation(cause))`), while the sibling
    // handles its `Terminate` in its own run.
    assert!(system.is_actor_terminated(&sibling));
    assert_eq!(*received.lock().unwrap(), alloc::vec![sibling]);
  }
}

struct StashEverything;

impl Actor for StashEverything {
  fn receive(&mut self, context: &mut ActorContext<'_>, _message: Message) {
    context.stash();
  }
}

type DeadLetterLog = Arc<Mutex<Vec<(ActorRef, &'static str)>>>;

/// Stands in for the system's dead letters with a handle that records what
/// reaches it, so the routing of a stash is observable.
///
/// A leftover reaches the dead letters handle wrapped in a `DeadLetter`
/// that names its original sender and recipient, as the mailbox clean-up
/// hands it over; the recorder reads the pair back out of the report.
fn record_dead_letters(executor: &mut Executor) -> DeadLetterLog {
  let delivered: DeadLetterLog = Arc::new(Mutex::new(Vec::new()));
  let inbox = delivered.clone();
  executor.dead_letters = executor.function_ref("recordedDeadLetters", move |_: &ActorRef, message: Message| {
    let dead = message.downcast_ref::<crate::kernel::actor::DeadLetter>().unwrap();
    inbox.lock().unwrap().push((dead.sender().clone(), *dead.message().downcast_ref::<&'static str>().unwrap()));
  });
  delivered
}

#[test]
fn a_stopped_actor_sends_its_stash_to_dead_letters_with_the_original_senders_in_order() {
  let mut executor = death_watch_executor();
  let delivered = record_dead_letters(&mut executor);
  let stasher = executor.try_spawn_user(Props::new(|| StashEverything), None).unwrap();
  let first_sender = executor.try_spawn_user(Props::new(|| Guardian), None).unwrap();
  let second_sender = executor.try_spawn_user(Props::new(|| Guardian), None).unwrap();
  stasher.tell("a", first_sender.clone());
  stasher.tell("b", second_sender.clone());
  executor.run_until_idle();
  assert!(delivered.lock().unwrap().is_empty());

  executor.stop(&stasher);
  executor.run_until_idle();

  assert_eq!(*delivered.lock().unwrap(), alloc::vec![(first_sender, "a"), (second_sender, "b")]);
  assert!(executor.actors.values().all(|entry| entry.stash.is_empty()));
}

// Stashes every message and returns the stash to the mailbox in its stop hook,
// as `UnrestrictedStash.postStop` does.
struct UnstashOnStop;

impl Actor for UnstashOnStop {
  fn receive(&mut self, context: &mut ActorContext<'_>, _message: Message) {
    context.stash();
  }

  fn post_stop_with_context(&mut self, context: &mut ActorContext<'_>) {
    context.unstash_all();
  }
}

struct TextRecorder(Arc<Mutex<Vec<&'static str>>>);

impl Actor for TextRecorder {
  fn receive(&mut self, _context: &mut ActorContext<'_>, message: Message) {
    self.0.lock().unwrap().push(*message.downcast_ref::<&'static str>().unwrap());
  }
}

#[test]
fn a_stash_returned_by_the_stop_hook_reaches_dead_letters_and_other_actors_keep_their_queue() {
  let mut executor = death_watch_executor();
  let delivered = record_dead_letters(&mut executor);
  let stasher = executor.try_spawn_user(Props::new(|| UnstashOnStop), None).unwrap();
  let received = Arc::new(Mutex::new(Vec::new()));
  let other = executor
    .try_spawn_user(
      Props::new({
        let received = received.clone();
        move || TextRecorder(received.clone())
      }),
      None,
    )
    .unwrap();
  let sender = executor.try_spawn_user(Props::new(|| Guardian), None).unwrap();
  stasher.tell("a", sender.clone());
  stasher.tell("b", sender.clone());
  executor.run_until_idle();
  // A message for another actor waits in the shard's local queue during the stop.
  executor.send(sender.clone(), &other, Message::new("x"));

  executor.stop(&stasher);
  // The stasher stops in its own run (Pekko's `Terminate`); the other
  // actor's message stays in its own mailbox and reaches it.
  executor.run_until_idle();

  assert_eq!(*delivered.lock().unwrap(), alloc::vec![(sender.clone(), "a"), (sender, "b")]);
  assert_eq!(*received.lock().unwrap(), alloc::vec!["x"]);
  assert_eq!(delivered.lock().unwrap().len(), 2);
}

// Stashes every message except `crash`, which fails it. The replacement fails
// in `post_restart`, so the restart that returned the stash ends in a stop.
struct FailingRestartStasher;

impl Actor for FailingRestartStasher {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    if message == "crash" {
      panic!("crash (expected)");
    }
    context.stash();
  }

  fn post_restart(&mut self, _context: &mut ActorContext<'_>) {
    panic!("the replacement fails (expected)");
  }
}

#[test]
fn a_stash_returned_by_a_restart_whose_replacement_fails_reaches_dead_letters() {
  let mut executor = death_watch_executor();
  executor.set_failure_catcher(Arc::new(UnwindCatcher));
  let delivered = record_dead_letters(&mut executor);
  let stasher = executor.try_spawn_user(Props::new(|| FailingRestartStasher), None).unwrap();
  let sender = executor.try_spawn_user(Props::new(|| Guardian), None).unwrap();
  stasher.tell("a", sender.clone());
  stasher.tell("b", sender.clone());
  stasher.tell("crash", sender.clone());

  executor.run_until_idle();

  assert!(executor.is_actor_terminated(&stasher));
  assert_eq!(*delivered.lock().unwrap(), alloc::vec![(sender.clone(), "a"), (sender, "b")]);
}

#[test]
fn stashing_the_newest_stashed_delivery_again_is_refused_and_another_delivery_is_not() {
  let mut executor = death_watch_executor();
  let owner = executor.try_spawn_user(Props::new(|| Guardian), None).unwrap();
  let sender = executor.try_spawn_user(Props::new(|| Guardian), None).unwrap();
  executor.stash(owner.clone(), 1, Message::new("a"), sender.clone());

  let again = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
    executor.stash(owner.clone(), 1, Message::new("a"), sender.clone());
  }));

  assert!(again.is_err());
  executor.stash(owner.clone(), 2, Message::new("b"), sender.clone());
  executor.unstash_all(&owner);
  executor.stash(owner.clone(), 2, Message::new("b"), sender);
  assert_eq!(executor.actors[&owner.key()].stash.len(), 1);
}

// Unit tests for stage 3 of #234 (design, section 11.11.8): the count of
// condition 1, the release of condition 4, the wait on the wake signal, the
// work a run leaves, the stop of a running actor, and a panic that no
// catcher catches.
mod stage3 {
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
  use crate::kernel::dispatch::ManualTask;
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
    // The actor starts in its own run (Pekko's `Create`), before the count.
    system.run_until_idle();
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
    // The actor starts in its own run (Pekko's `Create`), before the count.
    system.run_until_idle();
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
    // Both actors start in their own runs (Pekko's `Create`), before the count.
    system.run_until_idle();
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

  // The refused path: a message to a stopped actor goes to dead letters on the
  // sender's thread, as Pekko's dead letter mailbox sends it on (#286). The
  // `DeadLetter` names the actor it was addressed to, as Pekko's
  // `DeadLetter(message, sender, receiver)` does, so the handle is cloned
  // once (its path and its endpoint); with the sender, 4 clones. Nothing goes
  // to the executor, so there is no walk back (design of #234, section
  // 11.11.7). Condition 1's (1-2) is met; (1-1) is not, by the endpoint clone.
  #[test]
  fn a_send_to_a_stopped_actor_clones_the_sender_and_the_recipient_and_does_not_walk_back() {
    let mut system = system("ConditionOneRefused");
    let target = system.actor_of(Props::new(|| Sink));
    let sender = system.dead_letters();
    system.stop(&target);
    system.run_until_idle();
    let strong_before = target.mailbox_strong_count();

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
    assert_eq!(sent.atomic_ops(), 4 * N, "{sent:?}");
    assert_eq!(sent.upgrades, 0, "{sent:?}");
    // The tell routed every message, so the executor has none left to route.
    assert_eq!(routed.arc_clones(), sent.arc_clones(), "the executor routed a refused message: {routed:?}");
    assert_eq!(routed.upgrades, sent.upgrades, "{routed:?}");
    assert_eq!(target.mailbox_strong_count(), strong_before, "a clone of the mailbox leaked");
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
    let actor =
      system.actor_of(Props::new(move || StopsThenFails { log: Arc::clone(&actor_log), reply_to: to.clone() }));
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
  fn a_stop_asked_for_in_receive_takes_effect_in_the_run_of_the_target() {
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

    // `context.stop` puts a `Terminate` on the target's system queue
    // (Pekko's `ActorCell.stop`): the run of x goes on up to the
    // throughput, and the target stops in its own run, after x's.
    assert_eq!(
      entries(&log),
      [
        "x:stop",
        "x:check, stopped false",
        "x:check, stopped false",
        "x:check, stopped false",
        "x:check, stopped false",
        "other:b"
      ]
    );
    assert!(system.is_actor_terminated(&target));
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

    // The actor starts in its own run (Pekko's `Create`), so the panic comes
    // out of the call that runs it, not out of `actor_of`.
    system.actor_of(Props::new(|| PanicsInPreStart));
    assert!(!system.is_terminated());
    let unwound = panic::catch_unwind(panic::AssertUnwindSafe(|| system.run_until_idle()));

    assert!(unwound.is_err());
    assert!(system.is_terminated());
  }

  // ----- a stop is handled by the run that waits for the mailbox

  #[test]
  fn a_stop_is_handled_by_the_run_that_waits_for_the_mailbox() {
    let mut system = system("StopTakesTheRun");
    let dead = dead_letters_of(&mut system);
    let actor = system.actor_of(Props::new(|| Sink));
    system.run_until_idle();
    actor.tell("queued", actor.clone());
    let waiting = Executor::lend(system.executor_for_test(), |executor| !executor.manual.is_empty());
    assert!(waiting, "the send left no run");

    system.stop(&actor);

    // The `Terminate` joins the run that waits (Pekko's `systemDispatch`
    // finds the mailbox scheduled): that run handles it before the message,
    // and the message goes to dead letters as the mailbox closes.
    let run = Executor::lend(system.executor_for_test(), |executor| executor.manual.pop());
    let Some(ManualTask::Run(run)) = run else { panic!("the run of the actor no longer waits") };
    let waiting = Executor::lend(system.executor_for_test(), |executor| !executor.manual.is_empty());
    assert!(!waiting, "the stop left a second run");
    Executor::lend(system.executor_for_test(), |executor| executor.run_mailbox(run, None));
    assert!(system.is_actor_terminated(&actor));
    assert_eq!(entries(&dead), ["queued"]);
  }

  // ----- review of PR #284

  // Pekko resolves a selection when it is sent: a path that names no actor
  // then gives a dead letter, and an actor created at that path afterwards
  // does not receive it.
  #[test]
  fn a_selection_sent_before_the_actor_exists_does_not_reach_it_later() {
    let mut system = system("SelectionBeforeCreation");
    let dead = dead_letters_of(&mut system);
    let log = new_log();
    let path = system.dead_letters().path().parent().with_child("user").with_child("worker");
    let sender = system.dead_letters();

    system.actor_selection(&path).tell("init", sender.clone());
    let worker = system.actor_of_named(recorder(&log, "worker"), "worker").expect("the name is free");
    system.actor_selection(&path).tell("start", sender);
    system.run_until_idle();

    assert_eq!(entries(&log), ["worker:start"]);
    assert_eq!(entries(&dead), ["init"]);
    drop(worker);
  }

  // Starts a child, and stops itself once a child has stopped.
  struct StopsOnChildTerminated;

  impl Actor for StopsOnChildTerminated {
    fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {}

    fn pre_start(&mut self, context: &mut ActorContext<'_>) {
      context.actor_of(Props::new(|| Sink), "child").expect("the child starts");
    }

    fn supervisor_strategy(&self) -> crate::kernel::supervision::SupervisorStrategy {
      crate::kernel::supervision::SupervisorStrategy::default_strategy().with_child_terminated(
        |context: &mut ActorContext<'_>, _child: &ActorRef, _siblings: &[ActorRef]| {
          let me = context.self_ref();
          context.stop(&me);
        },
      )
    }
  }

  // A PoisonPill stops at the boundary, as before stage 3: a stop that the
  // stop's own child-terminated hook asks for waits until the first stop has
  // finished, and the actor stops once.
  #[test]
  fn a_poison_pill_stops_an_actor_once_when_its_hook_asks_to_stop_it_again() {
    let mut system = system("PoisonPillOnce");
    let parent = system.actor_of(Props::new(|| StopsOnChildTerminated));
    let stopped = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&stopped);
    let watched = parent.clone();
    system.downgrade().on_termination(move |actor: &ActorRef| {
      if actor == &watched {
        seen.lock().unwrap().push(actor.clone());
      }
    });

    parent.tell("PoisonPill", parent.clone());
    system.run_until_idle();

    assert_eq!(stopped.lock().unwrap().len(), 1, "the parent stopped twice");
    assert!(system.is_actor_terminated(&parent));
  }

  // Watches `target` with a custom notice.
  struct WatchesWith {
    target: ActorRef,
  }

  impl Actor for WatchesWith {
    fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {}

    fn pre_start(&mut self, context: &mut ActorContext<'_>) {
      context.watch_with(&self.target, "gone");
    }
  }

  #[test]
  fn terminating_releases_mailboxes_that_a_custom_watch_holds() {
    let mut system = system("ReleaseCustomWatch");
    let target = system.actor_of(Props::new(|| Sink));
    let to = target.clone();
    let watcher = system.actor_of(Props::new(move || WatchesWith { target: to.clone() }));
    let mailboxes = [target.mailbox_weak(), watcher.mailbox_weak()];

    system.terminate();
    drop(target);
    drop(watcher);

    assert!(mailboxes.iter().all(|mailbox| mailbox.upgrade().is_none()), "a custom watch kept a mailbox");
  }

  // A boundary that hands out no wake signal.
  struct WithoutSignal(ChannelBoundary);

  impl crate::kernel::executor::ShardBoundary for WithoutSignal {
    fn monotonic_time(&self) -> Option<Duration> {
      self.0.monotonic_time()
    }

    fn post(&self, envelope: crate::kernel::message::Envelope) {
      self.0.post(envelope);
    }

    fn poll(&self) -> Option<crate::kernel::message::Envelope> {
      self.0.poll()
    }

    fn poll_within(&self, max: Duration) -> Option<crate::kernel::message::Envelope> {
      self.0.poll_within(max)
    }

    fn has_pending(&self) -> bool {
      self.0.has_pending()
    }

    fn close(&self) {
      self.0.close();
    }
  }

  // A boundary that hands out a wake signal but no clock.
  struct WithoutClock(ChannelBoundary);

  impl crate::kernel::executor::ShardBoundary for WithoutClock {
    fn post(&self, envelope: crate::kernel::message::Envelope) {
      self.0.post(envelope);
    }

    fn poll(&self) -> Option<crate::kernel::message::Envelope> {
      self.0.poll()
    }

    fn poll_within(&self, max: Duration) -> Option<crate::kernel::message::Envelope> {
      self.0.poll_within(max)
    }

    fn has_pending(&self) -> bool {
      self.0.has_pending()
    }

    fn wake_signal(&self) -> Option<Arc<dyn crate::kernel::dispatch::WakeSignal>> {
      self.0.wake_signal()
    }

    fn close(&self) {
      self.0.close();
    }
  }

  #[test]
  fn without_a_signal_a_send_to_a_mailbox_from_another_thread_ends_the_wait() {
    let mut system = ActorSystem::new("WaitWithoutSignal", Arc::new(WithoutSignal(ChannelBoundary::new())));
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
    assert!(waited < Duration::from_secs(2), "the wait slept past the send: {waited:?}");
  }

  #[test]
  fn without_a_clock_a_signal_for_work_already_done_does_not_end_the_wait() {
    let mut system = ActorSystem::new("WaitWithoutClock", Arc::new(WithoutClock(ChannelBoundary::new())));
    let actor = system.actor_of(Props::new(|| Sink));
    actor.tell(1_u32, system.dead_letters());
    system.run_until_idle();

    let started = Instant::now();
    assert!(!system.await_envelope(Duration::from_millis(100)), "no work came");
    assert!(started.elapsed() >= Duration::from_millis(100), "the wait ended before its deadline");
  }

  #[test]
  fn a_named_child_of_the_user_guardian_is_refused_a_top_level_name_once_the_guardian_has_a_record() {
    let mut system = system("UserGuardianNames");
    let taken = system.actor_of_named(Props::new(|| Sink), "x").expect("the name is free");
    let guardian = system.resolve_actor_ref(&taken.path().parent());
    let weak = system.downgrade();
    weak
      .try_spawn_child(
        &guardian,
        Props::new(|| Sink),
        Some("y"),
        crate::kernel::supervision::SupervisorStrategy::default_strategy(),
      )
      .expect("y is free");

    let again = weak.try_spawn_child(
      &guardian,
      Props::new(|| Sink),
      Some("x"),
      crate::kernel::supervision::SupervisorStrategy::default_strategy(),
    );

    assert!(again.is_err(), "a second top-level actor named x started");
  }
}

// Stage 4a of #234 (design, section 11.12.12): the start and the stop of an
// actor are its own `Create` and `Terminate`, a parent learns of a child's
// end from its `ChildTerminated`, a restart waits for the children it
// stopped, and the decision for a child's failure waits while its parent is
// suspended.
mod stage4a {
  use std::string::String;
  use std::sync::atomic::AtomicUsize;
  use std::sync::atomic::Ordering;
  use std::sync::Mutex;
  use std::vec::Vec;

  use crate::adaptor::ChannelBoundary;
  use crate::adaptor::UnwindCatcher;
  use crate::kernel::actor::Actor;
  use crate::kernel::actor::ActorContext;
  use crate::kernel::actor::DeadLetter;
  use crate::kernel::actor::Props;
  use crate::kernel::actor::SuppressedDeadLetter;
  use crate::kernel::actor::Terminated;
  use crate::kernel::actor_ref::ActorRef;
  use crate::kernel::actor_system::ActorSystem;
  use crate::kernel::event::EventClass;
  use crate::kernel::message::Message;
  use crate::kernel::supervision::Directive;
  use crate::kernel::supervision::RestartWindow;
  use crate::kernel::supervision::SupervisorStrategy;
  use crate::kernel::sync::Arc;

  type Log = Arc<Mutex<Vec<String>>>;
  type Hook = Arc<dyn Fn(&mut ActorContext<'_>) + Send + Sync>;

  fn with_catcher(name: &str) -> ActorSystem {
    ActorSystem::with_failure_catcher(name, Arc::new(ChannelBoundary::new()), Arc::new(UnwindCatcher))
  }

  fn new_log() -> Log {
    Arc::new(Mutex::new(Vec::new()))
  }

  fn entries(log: &Log) -> Vec<String> {
    log.lock().unwrap().clone()
  }

  fn position(log: &[String], entry: &str) -> usize {
    log.iter().position(|held| held == entry).unwrap_or_else(|| panic!("{entry} is not in {log:?}"))
  }

  // Records its life and every `&str` it receives as `label:what`. It fails
  // `receive` on `fail_on`, and `pre_restart` runs `on_pre_restart` in place
  // of the default (stop the children, then `post_stop`).
  #[derive(Clone)]
  struct Node {
    label: &'static str,
    log: Log,
    fail_on: Option<&'static str>,
    on_pre_restart: Option<Hook>,
    post_restart_runs_pre_start: bool,
    strategy: Option<SupervisorStrategy>,
  }

  impl Node {
    fn new(label: &'static str, log: &Log) -> Self {
      Self {
        label,
        log: Arc::clone(log),
        fail_on: None,
        on_pre_restart: None,
        post_restart_runs_pre_start: false,
        strategy: None,
      }
    }

    fn failing_on(mut self, text: &'static str) -> Self {
      self.fail_on = Some(text);
      self
    }

    fn on_pre_restart(mut self, hook: impl Fn(&mut ActorContext<'_>) + Send + Sync + 'static) -> Self {
      self.on_pre_restart = Some(Arc::new(hook));
      self
    }

    /// Makes `post_restart` run `pre_start` after recording itself, as the
    /// default `Actor::post_restart` does, so a restart records the second
    /// `pre_start` too (design section 11.13.5).
    fn post_restart_runs_pre_start(mut self) -> Self {
      self.post_restart_runs_pre_start = true;
      self
    }

    /// Makes the node apply `strategy` to its own children, in place of
    /// the default strategy.
    fn supervising_with(mut self, strategy: SupervisorStrategy) -> Self {
      self.strategy = Some(strategy);
      self
    }

    fn props(&self) -> Props {
      let node = self.clone();
      Props::new(move || node.clone())
    }

    fn record(&self, what: &str) {
      self.log.lock().unwrap().push(std::format!("{}:{what}", self.label));
    }
  }

  impl Actor for Node {
    fn pre_start(&mut self, _context: &mut ActorContext<'_>) {
      self.record("pre_start");
    }

    fn supervisor_strategy(&self) -> SupervisorStrategy {
      self.strategy.clone().unwrap_or_else(SupervisorStrategy::default_strategy)
    }

    fn receive(&mut self, _context: &mut ActorContext<'_>, message: Message) {
      if let Some(text) = message.downcast_ref::<&'static str>() {
        if Some(*text) == self.fail_on {
          panic!("{} fails on {text}", self.label);
        }
        self.record(text);
      }
    }

    fn post_stop(&mut self) {
      self.record("post_stop");
    }

    fn pre_restart(&mut self, context: &mut ActorContext<'_>, _reason: &(dyn core::any::Any + Send)) {
      self.record("pre_restart");
      match self.on_pre_restart.clone() {
        | Some(hook) => hook(context),
        | None => {
          context.stop_children();
          self.post_stop_with_context(context);
        },
      }
    }

    fn post_restart(&mut self, context: &mut ActorContext<'_>) {
      self.record("post_restart");
      if self.post_restart_runs_pre_start {
        self.pre_start(context);
      }
    }
  }

  // Collects the `&str` payloads, and the `Terminated` notices (a dead
  // letter suppression, as in Pekko), that reach dead letters.
  fn dead_letters_of(system: &mut ActorSystem) -> Log {
    let collected = new_log();
    let sink = Arc::clone(&collected);
    let collector = system.function_ref("dead-letter-collector", move |_: &ActorRef, message: Message| {
      let letter = match (message.downcast_ref::<DeadLetter>(), message.downcast_ref::<SuppressedDeadLetter>()) {
        | (Some(letter), _) => letter.message().clone(),
        | (None, Some(letter)) => letter.message().clone(),
        | (None, None) => return,
      };
      if let Some(text) = letter.downcast_ref::<&'static str>() {
        sink.lock().unwrap().push(String::from(*text));
      }
      if letter.downcast_ref::<Terminated>().is_some() {
        sink.lock().unwrap().push(String::from("Terminated"));
      }
    });
    let stream = system.event_stream();
    stream.subscribe(Some(collector.clone()), EventClass::of::<DeadLetter>()).expect("a collector");
    stream.subscribe(Some(collector), EventClass::of::<SuppressedDeadLetter>()).expect("a collector");
    collected
  }

  fn spawn(system: &ActorSystem, parent: &ActorRef, node: &Node, strategy: SupervisorStrategy) -> ActorRef {
    system.downgrade().spawn_child(parent, node.props(), Some(node.label), strategy)
  }

  // A strategy whose decider panics, and counts how often it was asked.
  fn panicking_decider(asked: &Arc<AtomicUsize>) -> SupervisorStrategy {
    let asked = Arc::clone(asked);
    SupervisorStrategy::one_for_one(10).with_decider(move |_, _| {
      asked.fetch_add(1, Ordering::SeqCst);
      panic!("the decider fails");
    })
  }

  // ----- the start

  #[test]
  fn actor_of_returns_before_pre_start_runs() {
    let mut system = with_catcher("Stage4aCreate");
    let log = new_log();
    let actor = system.actor_of(Node::new("a", &log).props());

    assert!(entries(&log).is_empty(), "pre_start ran inside actor_of");
    assert!(!system.is_actor_terminated(&actor), "a child that has not started yet is alive");
    system.run_until_idle();
    assert_eq!(entries(&log), ["a:pre_start"]);
  }

  #[test]
  fn a_name_is_taken_when_the_child_is_created() {
    let mut system = with_catcher("Stage4aName");
    let log = new_log();
    let parent = system.actor_of(Node::new("p", &log).props());
    let first =
      system.downgrade().try_spawn_child(&parent, Props::empty(), Some("kid"), SupervisorStrategy::default_strategy());
    let second =
      system.downgrade().try_spawn_child(&parent, Props::empty(), Some("kid"), SupervisorStrategy::default_strategy());

    assert!(first.is_ok());
    assert!(second.is_err(), "the name of a child that has not started yet was free");
  }

  #[test]
  fn a_child_stopped_right_after_its_creation_starts_and_then_stops() {
    let mut system = with_catcher("Stage4aCreateThenStop");
    let log = new_log();
    let actor = system.actor_of(Node::new("a", &log).props());
    system.stop(&actor);
    actor.tell("hello", actor.clone());
    system.run_until_idle();

    assert_eq!(entries(&log), ["a:pre_start", "a:post_stop"]);
    assert!(system.is_actor_terminated(&actor));
  }

  // ----- the stop (T1 to T5)

  // Tells its parent a message from its `post_stop`, and records whether the
  // parent had stopped then.
  struct TellsTheParentWhenItStops {
    parent: ActorRef,
    log: Log,
  }

  impl Actor for TellsTheParentWhenItStops {
    fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {}

    fn post_stop_with_context(&mut self, context: &mut ActorContext<'_>) {
      let stopped = context.is_terminated(&self.parent);
      self.log.lock().unwrap().push(std::format!("child:post_stop, parent stopped {stopped}"));
      context.tell(&self.parent, "late");
    }
  }

  #[test]
  fn a_parent_waits_suspended_for_its_children_and_stops_after_them() {
    let mut system = with_catcher("Stage4aT1T2");
    let dead = dead_letters_of(&mut system);
    let log = new_log();
    let parent = system.actor_of(Node::new("parent", &log).props());
    let (to, child_log) = (parent.clone(), Arc::clone(&log));
    let child = system.downgrade().spawn_child(
      &parent,
      Props::new(move || TellsTheParentWhenItStops { parent: to.clone(), log: Arc::clone(&child_log) }),
      Some("child"),
      SupervisorStrategy::default_strategy(),
    );
    system.run_until_idle();

    system.stop(&parent);
    parent.tell("during", parent.clone());
    system.run_until_idle();

    // T1: the parent is still open while its child stops, and takes no
    // normal message while it waits. T2: its `post_stop` runs after the
    // child's.
    assert_eq!(entries(&log), ["parent:pre_start", "child:post_stop, parent stopped false", "parent:post_stop"]);
    assert_eq!(entries(&dead), ["during", "late"]);
    assert!(system.is_actor_terminated(&parent));
    assert!(system.is_actor_terminated(&child));
  }

  // Records, when a `Terminated` reaches it, whether the watched actor is
  // already gone, and whether the name the watched child held is free again.
  struct WatchingParent {
    log: Log,
  }

  impl Actor for WatchingParent {
    fn pre_start(&mut self, context: &mut ActorContext<'_>) {
      let child = context.actor_of(Props::empty(), "kid").expect("the name is free");
      context.watch(&child);
    }

    fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
      if let Some(notice) = message.downcast_ref::<Terminated>() {
        let gone = context.is_terminated(notice.target());
        let reused = context.actor_of(Props::empty(), "kid").is_ok();
        self.log.lock().unwrap().push(std::format!("terminated, gone {gone}, name free {reused}"));
      } else if message == "stop kid" {
        context.stop_children();
      }
    }
  }

  #[test]
  fn a_watching_parent_learns_of_its_childs_end_before_the_terminated() {
    let mut system = with_catcher("Stage4aT3T4");
    let log = new_log();
    let observed = Arc::clone(&log);
    let parent = system.actor_of(Props::new(move || WatchingParent { log: Arc::clone(&observed) }));
    system.run_until_idle();

    parent.tell("stop kid", parent.clone());
    system.run_until_idle();

    // T3: the watched child has closed when the notice comes. T4: its
    // parent has handled its `ChildTerminated` before the `Terminated`, so
    // the name is free again.
    assert_eq!(entries(&log), ["terminated, gone true, name free true"]);
  }

  // Asks to stop after a reply on its first message.
  struct RepliesWhenItStops {
    reply_to: ActorRef,
  }

  impl Actor for RepliesWhenItStops {
    fn receive(&mut self, context: &mut ActorContext<'_>, _message: Message) {
      context.stop_and_reply_after_termination(self.reply_to.clone(), "reply");
    }
  }

  #[test]
  fn the_termination_callbacks_and_the_reply_come_after_the_notices() {
    let mut system = with_catcher("Stage4aT5");
    let log = new_log();
    let (watch_log, reply_log, callback_log) = (Arc::clone(&log), Arc::clone(&log), Arc::clone(&log));
    let watcher = system.function_ref("watcher", move |_: &ActorRef, message: Message| {
      if message.downcast_ref::<Terminated>().is_some() {
        watch_log.lock().unwrap().push(String::from("terminated"));
      }
    });
    let reply_to = system.function_ref("reply-to", move |_: &ActorRef, message: Message| {
      if message == "reply" {
        reply_log.lock().unwrap().push(String::from("reply"));
      }
    });
    let to = reply_to.clone();
    let actor = system.actor_of(Props::new(move || RepliesWhenItStops { reply_to: to.clone() }));
    let watched = actor.clone();
    system.downgrade().on_termination(move |stopped| {
      if *stopped == watched {
        callback_log.lock().unwrap().push(String::from("callback"));
      }
    });
    system.watch(&watcher, &actor);

    actor.tell("go", actor.clone());
    system.run_until_idle();

    assert_eq!(entries(&log), ["terminated", "callback", "reply"]);
  }

  #[test]
  fn a_stopping_actor_gets_no_terminated_of_an_actor_it_watched() {
    let mut system = with_catcher("Stage4aUnwatch");
    let dead = dead_letters_of(&mut system);
    let log = new_log();
    let watched = system.actor_of(Node::new("watched", &log).props());
    let parent = system.actor_of(Node::new("parent", &log).props());
    spawn(&system, &parent, &Node::new("child", &log), SupervisorStrategy::default_strategy());
    system.watch(&parent, &watched);
    system.run_until_idle();

    // The parent waits for its child, and the watched actor stops in the
    // meantime: the parent's `Terminate` has removed its watch (T1).
    system.stop(&parent);
    system.stop(&watched);
    system.run_until_idle();

    assert!(system.is_actor_terminated(&parent));
    assert!(system.is_actor_terminated(&watched));
    assert!(entries(&dead).is_empty(), "a notice reached the stopping parent: {:?}", entries(&dead));
  }

  // Sends a message to `peer` when the instance is dropped.
  struct SendsWhenDropped {
    peer: ActorRef,
  }

  impl Actor for SendsWhenDropped {
    fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {}
  }

  impl Drop for SendsWhenDropped {
    fn drop(&mut self) {
      self.peer.tell("dropped", self.peer.clone());
    }
  }

  #[test]
  fn the_run_that_closes_an_actor_drops_it_after_the_stop() {
    let mut system = with_catcher("Stage4aDrop");
    let log = new_log();
    let peer = system.actor_of(Node::new("peer", &log).props());
    let to = peer.clone();
    let actor = system.actor_of(Props::new(move || SendsWhenDropped { peer: to.clone() }));
    system.run_until_idle();

    system.stop(&actor);
    system.run_until_idle();

    // The cell is dropped by the run that handled the `Terminate`, outside
    // the mailbox's lock (design of #234, section 11.6, L3): its `Drop`
    // reaches another actor.
    assert!(system.is_actor_terminated(&actor));
    assert_eq!(entries(&log), ["peer:pre_start", "peer:dropped"]);
  }

  #[test]
  fn a_second_stop_does_nothing() {
    let mut system = with_catcher("Stage4aStopTwice");
    let dead = dead_letters_of(&mut system);
    let log = new_log();
    let notices = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&notices);
    let watcher = system.function_ref("watcher", move |_: &ActorRef, message: Message| {
      if message.downcast_ref::<Terminated>().is_some() {
        counted.fetch_add(1, Ordering::SeqCst);
      }
    });
    let actor = system.actor_of(Node::new("a", &log).props());
    system.watch(&watcher, &actor);
    system.stop(&actor);
    system.stop(&actor);
    system.run_until_idle();
    system.stop(&actor);
    system.run_until_idle();

    assert_eq!(entries(&log), ["a:pre_start", "a:post_stop"]);
    assert_eq!(notices.load(Ordering::SeqCst), 1);
    assert!(entries(&dead).is_empty());
  }

  // ----- the restart waits for the children it stopped

  #[test]
  fn a_restart_takes_no_message_until_the_children_it_stopped_have_gone() {
    let mut system = with_catcher("Stage4aRecreation");
    let log = new_log();
    let guardian = system.actor_of(Props::empty());
    let parent =
      spawn(&system, &guardian, &Node::new("parent", &log).failing_on("boom"), SupervisorStrategy::default_strategy());
    let child = spawn(&system, &parent, &Node::new("child", &log), SupervisorStrategy::default_strategy());
    system.run_until_idle();

    parent.tell("boom", parent.clone());
    parent.tell("after", parent.clone());
    system.run_until_idle();

    let log = entries(&log);
    assert!(position(&log, "child:post_stop") < position(&log, "parent:post_restart"), "{log:?}");
    assert!(position(&log, "parent:post_restart") < position(&log, "parent:after"), "{log:?}");
    assert!(system.is_actor_terminated(&child));
    assert!(!system.is_actor_terminated(&parent));
  }

  #[test]
  fn an_all_for_one_sibling_takes_no_message_while_its_restart_waits() {
    let mut system = with_catcher("Stage4aAllForOne");
    let log = new_log();
    let guardian = system.actor_of(Props::empty());
    let all_for_one = SupervisorStrategy::all_for_one(3, RestartWindow::Unbounded);
    let failing = spawn(&system, &guardian, &Node::new("a", &log).failing_on("boom"), all_for_one.clone());
    let sibling = spawn(&system, &guardian, &Node::new("b", &log), all_for_one);
    spawn(&system, &sibling, &Node::new("b-child", &log), SupervisorStrategy::default_strategy());
    system.run_until_idle();

    failing.tell("boom", failing.clone());
    // The first `run_one` delivers `boom` to `a`, whose `Failed` then waits in
    // the guardian's mailbox. The guardian's run takes that `Failed` and the
    // ordinary `tick` behind it, so the second `run_one` returns once the
    // all-for-one decision has sent `b` its `Suspend` and `Recreate`, before
    // `b` has handled them.
    assert!(system.run_one(), "the failing run of `a` did not run");
    guardian.tell("tick", guardian.clone());
    assert!(system.run_one(), "the guardian's run did not run");
    assert!(!entries(&log).contains(&String::from("b:post_restart")), "{:?}", entries(&log));
    // `m` reaches `b` while it waits to restart, behind its `Suspend`
    // (design of #234, section 11.13.18 item 3).
    sibling.tell("m", sibling.clone());
    system.run_until_idle();

    let log = entries(&log);
    assert!(position(&log, "b-child:post_stop") < position(&log, "b:post_restart"), "{log:?}");
    assert!(position(&log, "b:post_restart") < position(&log, "b:m"), "{log:?}");
    assert!(log.contains(&String::from("a:post_restart")), "{log:?}");
  }

  // ----- one suspend per failure, undone once (section 11.12.6)

  #[test]
  fn a_restarted_actor_takes_the_next_message_with_its_new_instance() {
    let mut system = with_catcher("Stage4aResumeOnRestart");
    let log = new_log();
    let actor = system.actor_of(Node::new("a", &log).failing_on("boom").props());
    actor.tell("boom", actor.clone());
    actor.tell("after", actor.clone());
    system.run_until_idle();

    assert_eq!(entries(&log), ["a:pre_start", "a:pre_restart", "a:post_stop", "a:post_restart", "a:after"]);
  }

  #[test]
  fn a_stopped_actor_hands_the_messages_behind_its_failure_to_dead_letters() {
    let mut system = with_catcher("Stage4aStopOnFailure");
    let dead = dead_letters_of(&mut system);
    let log = new_log();
    let guardian = system.actor_of(Props::empty());
    let actor =
      spawn(&system, &guardian, &Node::new("a", &log).failing_on("boom"), SupervisorStrategy::stopping_strategy());
    system.run_until_idle();
    actor.tell("boom", actor.clone());
    actor.tell("after", actor.clone());
    system.run_until_idle();

    assert_eq!(entries(&log), ["a:pre_start", "a:post_stop"]);
    assert_eq!(entries(&dead), ["after"]);
  }

  #[test]
  fn a_panicking_decider_resumes_the_child_only_when_its_parent_restarts() {
    for (parent_directive, child_resumed) in [(Directive::Restart, true), (Directive::Stop, false)] {
      let mut system = with_catcher("Stage4aDeciderPanic");
      let dead = dead_letters_of(&mut system);
      let log = new_log();
      let asked = Arc::new(AtomicUsize::new(0));
      let guardian = system.actor_of(Props::empty());
      let decides_parent = SupervisorStrategy::one_for_one(3).with_decider(move |_, _| parent_directive);
      // The parent keeps its children across its restart.
      let parent = spawn(&system, &guardian, &Node::new("parent", &log).on_pre_restart(|_| {}), decides_parent);
      let child = spawn(&system, &parent, &Node::new("child", &log).failing_on("boom"), panicking_decider(&asked));
      system.run_until_idle();

      child.tell("boom", child.clone());
      child.tell("after", child.clone());
      system.run_until_idle();

      assert_eq!(asked.load(Ordering::SeqCst), 1);
      let log = entries(&log);
      assert_eq!(log.contains(&String::from("child:after")), child_resumed, "{parent_directive:?}: {log:?}");
      assert_eq!(entries(&dead) == ["after"], !child_resumed, "{parent_directive:?}: {:?}", entries(&dead));
      assert_eq!(system.is_actor_terminated(&child), !child_resumed);
    }
  }

  // Fails its `pre_start` on the first `failures` attempts.
  struct FailsToStart {
    attempts: Arc<AtomicUsize>,
    failures: usize,
    log: Log,
  }

  impl Actor for FailsToStart {
    fn pre_start(&mut self, _context: &mut ActorContext<'_>) {
      let attempt = self.attempts.fetch_add(1, Ordering::SeqCst) + 1;
      assert!(attempt > self.failures, "the start fails");
    }

    fn receive(&mut self, _context: &mut ActorContext<'_>, message: Message) {
      if let Some(text) = message.downcast_ref::<&'static str>() {
        self.log.lock().unwrap().push(String::from(*text));
      }
    }

    fn post_stop(&mut self) {
      self.log.lock().unwrap().push(String::from("post_stop"));
    }
  }

  fn fails_to_start(attempts: &Arc<AtomicUsize>, failures: usize, log: &Log) -> Props {
    let (attempts, log) = (Arc::clone(attempts), Arc::clone(log));
    Props::new(move || FailsToStart { attempts: Arc::clone(&attempts), failures, log: Arc::clone(&log) })
  }

  #[test]
  fn a_failed_start_that_is_stopped_runs_no_post_stop_and_frees_its_name() {
    let mut system = with_catcher("Stage4aStartStop");
    let dead = dead_letters_of(&mut system);
    let log = new_log();
    let attempts = Arc::new(AtomicUsize::new(0));
    let guardian = system.actor_of(Props::empty());
    let child = system.downgrade().spawn_child(
      &guardian,
      fails_to_start(&attempts, 1, &log),
      Some("kid"),
      SupervisorStrategy::stopping_strategy(),
    );
    child.tell("hello", child.clone());
    system.run_until_idle();

    assert!(system.is_actor_terminated(&child));
    assert!(entries(&log).is_empty(), "the child without an instance ran post_stop or receive: {:?}", entries(&log));
    assert_eq!(entries(&dead), ["hello"]);
    let again = system.downgrade().try_spawn_child(
      &guardian,
      Props::empty(),
      Some("kid"),
      SupervisorStrategy::default_strategy(),
    );
    assert!(again.is_ok(), "the name of the stopped child is still taken");
  }

  #[test]
  fn a_failed_start_of_a_child_that_also_stopped_is_still_decided() {
    let mut system = with_catcher("Stage4aStartFailedAndStopped");
    let log = new_log();
    let attempts = Arc::new(AtomicUsize::new(0));
    let asked = Arc::new(AtomicUsize::new(0));
    let guardian = system.actor_of(Props::empty());
    let parent = spawn(&system, &guardian, &Node::new("parent", &log), SupervisorStrategy::default_strategy());
    system.run_until_idle();
    let child = system.downgrade().spawn_child(
      &parent,
      fails_to_start(&attempts, 1, &log),
      Some("kid"),
      panicking_decider(&asked),
    );
    // The `Terminate` waits behind the `Create`: the child fails to start and
    // closes in one run, and its parent decides afterwards.
    system.stop(&child);
    system.run_until_idle();

    assert!(system.is_actor_terminated(&child));
    assert_eq!(asked.load(Ordering::SeqCst), 1, "the failed start of the stopped child was not decided");
    let log = entries(&log);
    assert!(log.contains(&String::from("parent:post_restart")), "the decider panic did not fail the parent: {log:?}");
  }

  // Starts two children in `pre_start`: `c1` fails its first three starts and
  // is restarted, `c2` fails its first start and its decider panics, which
  // restarts the parent while the retry of `c1` is still queued.
  struct StartsTwoFailingChildren {
    c1_attempts: Arc<AtomicUsize>,
    c2_attempts: Arc<AtomicUsize>,
    c1_decisions: Arc<AtomicUsize>,
    restarts: Arc<AtomicUsize>,
    log: Log,
  }

  impl Actor for StartsTwoFailingChildren {
    fn pre_start(&mut self, context: &mut ActorContext<'_>) {
      context.actor_of(fails_to_start(&self.c1_attempts, 3, &self.log), "c1").expect("c1 is free");
      context.actor_of(fails_to_start(&self.c2_attempts, 1, &self.log), "c2").expect("c2 is free");
    }

    fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {}

    fn pre_restart(&mut self, _context: &mut ActorContext<'_>, _reason: &(dyn core::any::Any + Send)) {}

    fn post_restart(&mut self, _context: &mut ActorContext<'_>) {
      self.restarts.fetch_add(1, Ordering::SeqCst);
    }

    fn supervisor_strategy(&self) -> SupervisorStrategy {
      let decisions = Arc::clone(&self.c1_decisions);
      SupervisorStrategy::one_for_one(10).with_decider(move |child, _| {
        if child.path().name() == "c1" {
          decisions.fetch_add(1, Ordering::SeqCst);
          Directive::Restart
        } else {
          panic!("the decider fails for c2");
        }
      })
    }
  }

  #[test]
  fn two_queued_creates_retry_a_failed_start_once_per_decision() {
    let mut system = with_catcher("Stage4aTwoCreates");
    let log = new_log();
    let (c1_attempts, c2_attempts) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
    let (c1_decisions, restarts) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
    let parent = system.actor_of(Props::new({
      let (c1_attempts, c2_attempts) = (Arc::clone(&c1_attempts), Arc::clone(&c2_attempts));
      let (c1_decisions, restarts, log) = (Arc::clone(&c1_decisions), Arc::clone(&restarts), Arc::clone(&log));
      move || StartsTwoFailingChildren {
        c1_attempts: Arc::clone(&c1_attempts),
        c2_attempts: Arc::clone(&c2_attempts),
        c1_decisions: Arc::clone(&c1_decisions),
        restarts: Arc::clone(&restarts),
        log: Arc::clone(&log),
      }
    }));
    system.run_until_idle();

    // c1: the first failure is restarted and its `Recreate` queued; the
    // parent's restart queues a second. Under section 11.13.11 the second
    // `Recreate` restarts the child the first one created, and that restart's
    // default `post_restart` runs `pre_start` again, so c1 attempts one more
    // start than the stage 4a "second Create does nothing" rule gave. Every
    // failure is still decided once, and the child ends up resumed.
    assert_eq!(restarts.load(Ordering::SeqCst), 1);
    assert_eq!(c1_attempts.load(Ordering::SeqCst), 5);
    assert_eq!(c1_decisions.load(Ordering::SeqCst), 3);
    assert_eq!(c2_attempts.load(Ordering::SeqCst), 2);
    assert!(!system.is_actor_terminated(&parent));
    let c1 = system.resolve_actor_ref(&parent.path().with_child("c1"));
    c1.tell("hello", c1.clone());
    system.run_until_idle();
    // The second `Recreate` restarts the created c1, and that restart's
    // `pre_restart` runs the default stop hook once; the resumed child then
    // receives "hello", which shows it is no longer suspended.
    assert_eq!(
      entries(&log),
      ["post_stop", "hello"],
      "the retried child resumes after its restart and receives `hello`"
    );
  }

  // ----- the decision waits while the parent is suspended

  #[test]
  fn a_childs_failure_while_its_parent_waits_is_decided_after_the_restart() {
    let mut system = with_catcher("Stage4aHeldDecision");
    let log = new_log();
    let asked = Arc::new(AtomicUsize::new(0));
    let guardian = system.actor_of(Props::empty());
    let slow = Arc::new(Mutex::new(None::<ActorRef>));
    let to_stop = Arc::clone(&slow);
    let parent_node = Node::new("parent", &log).failing_on("boom").on_pre_restart(move |context| {
      if let Some(child) = to_stop.lock().unwrap().clone() {
        context.stop(&child);
      }
    });
    let parent = spawn(&system, &guardian, &parent_node, SupervisorStrategy::one_for_one(5));
    let a = spawn(&system, &parent, &Node::new("a", &log), SupervisorStrategy::default_strategy());
    *slow.lock().unwrap() = Some(a);
    let b = spawn(&system, &parent, &Node::new("b", &log).failing_on("boom"), panicking_decider(&asked));
    system.run_until_idle();

    // The parent's restart waits for `a`; `b` fails meanwhile, and its
    // decision waits for the restart to finish.
    parent.tell("boom", parent.clone());
    b.tell("boom", b.clone());
    b.tell("after", b.clone());
    system.run_until_idle();

    let log = entries(&log);
    let restarts: Vec<&String> = log.iter().filter(|entry| entry.starts_with("parent:p")).collect();
    assert_eq!(
      restarts,
      ["parent:pre_start", "parent:pre_restart", "parent:post_restart", "parent:pre_restart", "parent:post_restart"],
      "a restart of the parent ran inside another: {log:?}"
    );
    assert_eq!(asked.load(Ordering::SeqCst), 1);
    assert!(log.contains(&String::from("b:after")), "the parent's restart did not resume b: {log:?}");
  }

  // ----- a restart and a stop of the same actor

  // Asks to stop itself on "stop and fail", then fails the same message.
  struct StopsItselfThenFails {
    log: Log,
  }

  impl Actor for StopsItselfThenFails {
    fn pre_start(&mut self, context: &mut ActorContext<'_>) {
      context.actor_of(Props::empty(), "kid").expect("the name is free");
    }

    fn receive(&mut self, context: &mut ActorContext<'_>, _message: Message) {
      let me = context.self_ref();
      context.stop(&me);
      panic!("fails after asking to stop");
    }

    fn post_stop(&mut self) {
      self.log.lock().unwrap().push(String::from("post_stop"));
    }

    fn post_restart(&mut self, _context: &mut ActorContext<'_>) {
      self.log.lock().unwrap().push(String::from("post_restart"));
    }
  }

  #[test]
  fn a_stopping_actor_that_fails_is_not_restarted() {
    let mut system = with_catcher("Stage4aStopThenFail");
    let log = new_log();
    let observed = Arc::clone(&log);
    let actor = system.actor_of(Props::new(move || StopsItselfThenFails { log: Arc::clone(&observed) }));
    system.run_until_idle();

    actor.tell("stop and fail", actor.clone());
    system.run_until_idle();

    // Pekko's `faultRecreate` restarts only an actor that is not terminating
    // (`isNormal`): the stop goes on and finishes.
    assert_eq!(entries(&log), ["post_stop"]);
    assert!(system.is_actor_terminated(&actor));
  }

  #[test]
  fn an_all_for_one_restart_leaves_a_stopping_sibling_to_its_stop() {
    let mut system = with_catcher("Stage4aAllForOneStopping");
    let log = new_log();
    let guardian = system.actor_of(Props::empty());
    let all_for_one = SupervisorStrategy::all_for_one(3, RestartWindow::Unbounded);
    let failing = spawn(&system, &guardian, &Node::new("a", &log).failing_on("boom"), all_for_one.clone());
    let stopping = spawn(&system, &guardian, &Node::new("b", &log), all_for_one);
    spawn(&system, &stopping, &Node::new("b-child", &log), SupervisorStrategy::default_strategy());
    system.run_until_idle();

    // `b` waits for its child to stop when `a` fails.
    system.stop(&stopping);
    failing.tell("boom", failing.clone());
    system.run_until_idle();

    let log = entries(&log);
    assert!(log.contains(&String::from("a:post_restart")), "{log:?}");
    assert!(!log.contains(&String::from("b:post_restart")), "the stopping sibling was restarted: {log:?}");
    assert!(system.is_actor_terminated(&stopping));
  }

  #[test]
  fn an_all_for_one_restart_leaves_a_sibling_that_has_not_started_to_its_create() {
    let mut system = with_catcher("Stage4aAllForOneUnborn");
    let log = new_log();
    let attempts = Arc::new(AtomicUsize::new(0));
    let guardian = system.actor_of(Props::empty());
    system.run_until_idle();
    let all_for_one =
      SupervisorStrategy::all_for_one(3, RestartWindow::Unbounded).with_decider(|_, _| Directive::Restart);
    // `a` fails its first start, and its parent decides before `b` has
    // handled its `Create`. Every sibling is suspended and recreated, so `b`
    // handles `Create` then `Recreate`: `pre_start` twice, and a `post_restart`
    // that runs the second `pre_start` (section 11.13.5).
    system.downgrade().spawn_child(&guardian, fails_to_start(&attempts, 1, &log), Some("a"), all_for_one.clone());
    let unborn = spawn(&system, &guardian, &Node::new("b", &log).post_restart_runs_pre_start(), all_for_one);
    system.run_until_idle();

    assert_eq!(attempts.load(Ordering::SeqCst), 2);
    assert_eq!(
      entries(&log),
      ["b:pre_start", "b:pre_restart", "b:post_stop", "b:post_restart", "b:pre_start"],
      "the sibling that had not started handles its `Create`, then is restarted by the `Recreate`"
    );
    assert!(!system.is_actor_terminated(&unborn));
  }

  // ----- a function reference as a parent

  #[test]
  fn watching_a_function_reference_that_waits_for_its_child_gives_one_notice() {
    let mut system = with_catcher("Stage4aFunctionRefWatch");
    let notices = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&notices);
    let watcher = system.function_ref("watcher", move |_: &ActorRef, message: Message| {
      if message.downcast_ref::<Terminated>().is_some() {
        counted.fetch_add(1, Ordering::SeqCst);
      }
    });
    let parent = system.function_ref("parent", |_: &ActorRef, _: Message| {});
    system.downgrade().spawn_child(&parent, Props::empty(), Some("kid"), SupervisorStrategy::default_strategy());
    system.run_until_idle();
    system.watch(&watcher, &parent);

    // The function reference waits for its child: a watch now is the same
    // watch, and the end of the last child tells it once.
    system.stop(&parent);
    system.watch(&watcher, &parent);
    system.run_until_idle();

    assert_eq!(notices.load(Ordering::SeqCst), 1);
    assert!(system.is_actor_terminated(&parent));
  }

  #[test]
  fn a_child_of_a_function_reference_whose_decider_panics_is_resumed() {
    let mut system = with_catcher("Stage4aFunctionRefDecider");
    let log = new_log();
    let asked = Arc::new(AtomicUsize::new(0));
    let parent = system.function_ref("parent", |_: &ActorRef, _: Message| {});
    let child = system.downgrade().spawn_child(
      &parent,
      Node::new("c", &log).failing_on("boom").props(),
      Some("c"),
      panicking_decider(&asked),
    );
    system.run_until_idle();

    child.tell("boom", child.clone());
    child.tell("ping", child.clone());
    system.run_until_idle();

    // A function reference cannot fail in its turn, so nothing restarts it
    // to resume the child: the child is resumed at once, as in stage 3.
    assert_eq!(asked.load(Ordering::SeqCst), 1);
    assert_eq!(entries(&log), ["c:pre_start", "c:ping"]);
    assert!(!system.is_actor_terminated(&child));
  }

  #[test]
  fn a_resume_after_a_failure_carries_the_cause_to_the_perpetrator_child_only() {
    // Design 11.13.7, the `Resume` row, and 11.13.18 item 9.5: Pekko v2.0.0's
    // `faultResume` resumes its children with the cause of its own failure
    // only for the child that failure names as the perpetrator, and without
    // a cause for every other child (`dungeon/FaultHandling.scala:146-151`,
    // `dungeon/Children.scala:216-222`,
    // `child.resume(if (perp == child) causedByFailure else null)`).
    // `k`'s decider panics on `x`'s failure, so `k` fails with `x` as the
    // perpetrator; `k`'s parent is a function reference, whose decider for
    // `k` panics too, so `k` is resumed with the cause (11.13.7: the one
    // place a `Resume` is sent). The runs are taken one at a time, and the
    // `Resume`s `k` sends are read from its children's mailboxes before
    // either child runs: `x` is given the cause, `y` is not.
    let address = crate::kernel::address::Address::new("pekko", "Stage4bResumeCause");
    let root = crate::kernel::actor_path::RootActorPath::new(address);
    let mut executor = super::Executor::new(super::ActorPath::from_root(root), Arc::new(ChannelBoundary::new()));
    executor.set_failure_catcher(Arc::new(UnwindCatcher));
    let log = new_log();
    let asked_for_k = Arc::new(AtomicUsize::new(0));
    let asked_for_x = Arc::new(AtomicUsize::new(0));
    let parent = executor.function_ref("parent", |_: &ActorRef, _: Message| {});
    let k = executor.spawn_child(
      &parent,
      Node::new("k", &log).supervising_with(panicking_decider(&asked_for_x)).props(),
      Some("k"),
      panicking_decider(&asked_for_k),
    );
    executor.run_until_idle();
    let x = executor.spawn_named_child(&k, Node::new("x", &log).failing_on("boom").props(), "x").expect("`x`");
    let y = executor.spawn_named_child(&k, Node::new("y", &log).props(), "y").expect("`y`");
    executor.run_until_idle();

    x.tell("boom", x.clone());
    let mut runs = 0;
    while asked_for_k.load(Ordering::SeqCst) == 0
      || executor.entry(&k).is_some_and(|entry| entry.mailbox.suspend_count() > 0)
    {
      let task = executor.manual.pop().expect("a run is left before `k` is resumed");
      executor.run_task(task, None);
      runs += 1;
      assert!(runs <= 2, "`k` was not resumed in the runs of `x` and `k`");
    }
    assert_eq!(asked_for_x.load(Ordering::SeqCst), 1, "`k`'s decider was not asked once for `x`");
    assert_eq!(asked_for_k.load(Ordering::SeqCst), 1, "the function reference's decider was not asked once for `k`");

    let resumes_of = |executor: &super::Executor, child: &ActorRef| -> Vec<bool> {
      let entry = executor.entry(child).expect("the child is in the table");
      entry
        .mailbox
        .close()
        .1
        .into_iter()
        .filter_map(|message| match message {
          | super::SystemMessage::Resume { caused_by_failure } => Some(caused_by_failure),
          | _ => None,
        })
        .collect()
    };
    assert_eq!(resumes_of(&executor, &x), [true], "the perpetrator `x` was not resumed with the cause once");
    assert_eq!(resumes_of(&executor, &y), [false], "`y`, which did not fail, was resumed with the cause");
  }

  #[test]
  fn a_held_failure_of_a_child_that_has_left_the_table_is_dropped() {
    let mut system = with_catcher("Stage4aDroppedDecision");
    let log = new_log();
    let dead = dead_letters_of(&mut system);
    let asked = Arc::new(AtomicUsize::new(0));
    let slow = Arc::new(Mutex::new(None::<ActorRef>));
    let to_stop = Arc::clone(&slow);
    let grandparent_node = Node::new("g", &log).failing_on("boom").on_pre_restart(move |context| {
      if let Some(child) = to_stop.lock().unwrap().clone() {
        context.stop(&child);
      }
    });
    let grandparent = system.actor_of(grandparent_node.props());
    let x = spawn(&system, &grandparent, &Node::new("x", &log), SupervisorStrategy::default_strategy());
    *slow.lock().unwrap() = Some(x);
    // Both the strategy `p` applies to its children and the one `c` is
    // started with count the decisions asked of them, so `asked` reads every
    // decider that could answer for `c`.
    let parent = spawn(
      &system,
      &grandparent,
      &Node::new("p", &log).supervising_with(panicking_decider(&asked)),
      SupervisorStrategy::one_for_one(5),
    );
    let child = spawn(&system, &parent, &Node::new("c", &log).failing_on("boom"), panicking_decider(&asked));
    system.run_until_idle();

    // `g` fails and suspends `p`, and `c` takes its own `boom` before
    // `p`'s `Suspend` reaches it, so `c` fails and its `Failed` is held by
    // the suspended `p`. `g` waits for `x` in its restart; once `g` has
    // restarted, `p`, a survivor of `g`'s restart, is recreated once
    // (design 11.13.6, D4b-1 A), and its default `pre_restart` stops `c`,
    // which leaves `p`'s table. The held `Failed(c)` is then dropped, as
    // Pekko v2.0.0 drops a `Failed` from an unknown child
    // (`dungeon/FaultHandling.scala:307-324`): the decider is never asked
    // (design 11.13.13, decision 1, and 11.13.18 item 6). That `c` failed
    // at all is read from dead letters: its `boom` was taken, not left.
    grandparent.tell("boom", grandparent.clone());
    child.tell("boom", child.clone());
    system.run_until_idle();

    assert_eq!(entries(&dead), Vec::<String>::new(), "`c` did not take its `boom`, so it never failed");
    assert_eq!(asked.load(Ordering::SeqCst), 0, "the decider was asked about a child that had left the table");
    let log = entries(&log);
    assert_eq!(log.iter().filter(|entry| *entry == "p:post_restart").count(), 1, "{log:?}");
    assert!(!system.is_actor_terminated(&parent));
    assert!(system.is_actor_terminated(&child));
  }
}

mod closed_mailbox_system_messages {
  use core::time::Duration;
  use std::sync::Mutex;
  use std::vec::Vec;

  use crate::adaptor::ChannelBoundary;
  use crate::kernel::actor::Actor;
  use crate::kernel::actor::DeadLetter;
  use crate::kernel::actor::DeadLetterSettings;
  use crate::kernel::actor::Props;
  use crate::kernel::actor::SuppressedDeadLetter;
  use crate::kernel::actor_path::ActorPath;
  use crate::kernel::actor_ref::ActorRef;
  use crate::kernel::dispatch::SystemMessage;
  use crate::kernel::event::DeadLetterListener;
  use crate::kernel::event::EventClass;
  use crate::kernel::event::EventStreamBus;
  use crate::kernel::executor::Executor;
  use crate::kernel::message::Message;
  use crate::kernel::sync::Arc;

  /// The kind of a system message, without the handles inside it, so the
  /// comparisons below do not need `SystemMessage: PartialEq`.
  #[derive(Clone, Debug, PartialEq, Eq)]
  enum Kind {
    Create,
    Suspend,
    Resume,
    Recreate,
    Failed,
    Terminate,
    ChildTerminated,
  }

  fn kind_of(message: &SystemMessage) -> Kind {
    match message {
      | SystemMessage::Create => Kind::Create,
      | SystemMessage::Suspend => Kind::Suspend,
      | SystemMessage::Resume { .. } => Kind::Resume,
      | SystemMessage::Recreate { .. } => Kind::Recreate,
      | SystemMessage::Failed(_) => Kind::Failed,
      | SystemMessage::Terminate => Kind::Terminate,
      | SystemMessage::ChildTerminated { .. } => Kind::ChildTerminated,
    }
  }

  /// One report that reached dead letters, read back as what it carries.
  #[derive(Clone, Debug, PartialEq)]
  enum Routed {
    /// A `DeadLetter` whose payload is a system message, with the sender and
    /// the recipient it names.
    Dead(Kind, ActorRef, ActorRef),
    /// A `SuppressedDeadLetter` whose payload is a system message, with the
    /// sender and the reference that processed it.
    Suppressed(Kind, ActorRef, ActorRef),
  }

  /// What dead letters received: the routed reports in order.
  type Reports = Arc<Mutex<Vec<Routed>>>;

  fn read_report(message: &Message) -> Option<Routed> {
    if let Some(dead) = message.downcast_ref::<DeadLetter>() {
      return dead
        .message()
        .downcast_ref::<SystemMessage>()
        .map(|carried| Routed::Dead(kind_of(carried), dead.sender().clone(), dead.recipient().clone()));
    }
    if let Some(dead) = message.downcast_ref::<SuppressedDeadLetter>() {
      return dead
        .message()
        .downcast_ref::<SystemMessage>()
        .map(|carried| Routed::Suppressed(kind_of(carried), dead.sender().clone(), dead.recipient().clone()));
    }
    None
  }

  /// Records the line the dead letter listener would write for a report, if
  /// it logs it at all.
  fn message_for(report: &Routed) -> SystemMessage {
    match report {
      | Routed::Dead(kind, _, _) | Routed::Suppressed(kind, _, _) => match kind {
        | Kind::Create => SystemMessage::Create,
        | Kind::Suspend => SystemMessage::Suspend,
        | Kind::Resume => SystemMessage::Resume { caused_by_failure: false },
        | Kind::Recreate | Kind::Failed => {
          unreachable!("the dead letter line tests route Create, Suspend, Resume, Terminate and ChildTerminated only")
        },
        | Kind::Terminate => SystemMessage::Terminate,
        | Kind::ChildTerminated => SystemMessage::Create,
      },
    }
  }

  fn listener_line(report: &Routed, dead_letters: &ActorRef) -> Option<alloc::string::String> {
    let mut listener = DeadLetterListener::new(DeadLetterSettings::defaults(), dead_letters.clone());
    let is_suppressed = |_: &Message| false;
    match report {
      | Routed::Dead(_, sender, recipient) => {
        let dead = DeadLetter::from_message(Message::new(message_for(report)), sender.clone(), recipient.clone());
        listener.on_event(&crate::kernel::actor::AnyDeadLetter::DeadLetter(&dead), Duration::ZERO, &is_suppressed)
      },
      | Routed::Suppressed(_, sender, recipient) => {
        let dead =
          SuppressedDeadLetter::from_message(Message::new(message_for(report)), sender.clone(), recipient.clone());
        listener.on_event(&crate::kernel::actor::AnyDeadLetter::Suppressed(&dead), Duration::ZERO, &is_suppressed)
      },
    }
  }

  /// Builds an executor whose dead letters publish to a bus of its own, and
  /// subscribes a function reference to every class of that bus so each
  /// report is recorded as the system message it carries.
  fn executor_with_reports() -> (Executor, EventStreamBus, Reports) {
    let address = crate::kernel::address::Address::new("pekko", "ClosedSystemMessages");
    let root = crate::kernel::actor_path::RootActorPath::new(address);
    let mut executor = Executor::new(ActorPath::from_root(root), Arc::new(ChannelBoundary::new()));
    let bus = EventStreamBus::new();
    executor.set_dead_letter_bus(bus.downgrade());
    let delivered: Reports = Arc::new(Mutex::new(Vec::new()));
    let inbox = delivered.clone();
    let collector = executor.function_ref("systemMessageCollector", move |_: &ActorRef, message: Message| {
      if let Some(report) = read_report(&message) {
        inbox.lock().unwrap().push(report);
      }
    });
    bus.subscribe(Some(collector.clone()), EventClass::of::<DeadLetter>()).expect("the collector");
    bus.subscribe(Some(collector), EventClass::of::<SuppressedDeadLetter>()).expect("the collector");
    (executor, bus, delivered)
  }

  struct Sink;

  impl Actor for Sink {
    fn receive(&mut self, _context: &mut crate::kernel::actor::ActorContext<'_>, _message: Message) {}
  }

  /// Starts an actor and closes its mailbox without taking it out of the
  /// table, so a system message sent to it meets a closed mailbox exactly as
  /// a stop overtaking a `Resume` does. Returns the handle and any system
  /// message the mailbox held when it closed.
  fn actor_with_a_closed_mailbox(executor: &mut Executor) -> (ActorRef, Vec<SystemMessage>) {
    let target = executor.try_spawn_user(Props::new(|| Sink), None).unwrap();
    executor.run_until_idle();
    let held = {
      let entry = executor.entry(&target).expect("the actor is in the table");
      entry.mailbox.close().1
    };
    (target, held)
  }

  // Issue #297, design section 7.2, first case: a `Resume` for a closed
  // mailbox is `DeadLetter(Resume, target, target)`, and the listener writes
  // one `Message [..Resume]` line.
  //
  // The `Resume` is sent through `send_system`, the entry the design fixes
  // for a child of a restart or a resume (design section 5.4). After #297 the
  // `Err` of `Mailbox::system_send` goes to `route_closed_system_message`.
  #[test]
  fn a_resume_for_a_closed_mailbox_is_a_dead_letter_and_the_listener_logs_it() {
    let (mut executor, _bus, reports) = executor_with_reports();
    let (target, _held) = actor_with_a_closed_mailbox(&mut executor);

    executor.send_system(&target, SystemMessage::Resume { caused_by_failure: false });

    let seen = reports.lock().unwrap().clone();
    assert_eq!(
      seen,
      std::vec![Routed::Dead(Kind::Resume, target.clone(), target.clone())],
      "a Resume to a closed mailbox goes to dead letters naming the target on both ends"
    );
    let line = listener_line(&seen[0], &executor.dead_letters()).expect("the listener writes the Resume");
    assert!(
      line.contains("[fraktor_actor::kernel::dispatch::system_message::Resume]"),
      "the class name is the kind of the system message, not `SystemMessage`: {line}"
    );
  }

  // Issue #297: `Create` for a closed mailbox is a plain
  // `DeadLetter(Create, target, target)`.
  #[test]
  fn a_create_for_a_closed_mailbox_is_a_dead_letter() {
    let (mut executor, _bus, reports) = executor_with_reports();
    let (target, _held) = actor_with_a_closed_mailbox(&mut executor);

    executor.send_system(&target, SystemMessage::Create);

    assert_eq!(
      *reports.lock().unwrap(),
      std::vec![Routed::Dead(Kind::Create, target.clone(), target)],
      "a Create to a closed mailbox goes to dead letters naming the target on both ends"
    );
  }

  // Issue #297: `Terminate` and `ChildTerminated` for a closed mailbox are
  // suppressed, and the dead letter listener does not count them.
  //
  // `ChildTerminated` is the port's own kind, and the commander decided it is
  // suppressed too (design section 6, decision 5).
  #[test]
  fn terminate_and_child_terminated_for_a_closed_mailbox_are_suppressed() {
    let (mut executor, _bus, reports) = executor_with_reports();
    let (target, _held) = actor_with_a_closed_mailbox(&mut executor);
    let (child, _held) = actor_with_a_closed_mailbox(&mut executor);
    let dead_letters = executor.dead_letters();

    executor.send_system(&target, SystemMessage::Terminate);
    executor.send_system(&target, SystemMessage::ChildTerminated { child: child.clone() });

    let seen = reports.lock().unwrap().clone();
    assert_eq!(
      seen,
      std::vec![
        Routed::Suppressed(Kind::Terminate, target.clone(), dead_letters.clone()),
        Routed::Suppressed(Kind::ChildTerminated, target, dead_letters.clone()),
      ],
      "a Terminate and a ChildTerminated to a closed mailbox are suppressed, naming the dead letters handle"
    );
    for report in &seen {
      assert!(listener_line(report, &dead_letters).is_none(), "a suppressed report is not logged: {report:?}");
    }
  }

  /// What the stop-time clean-up routed, in the order it reached dead
  /// letters: a normal message by its text, a system message by its kind.
  #[derive(Clone, Debug, PartialEq, Eq)]
  enum Step {
    Text(&'static str),
    System(Kind),
  }

  /// Builds an executor whose dead letters publish to a bus of its own and
  /// records every report there, normal or system, in order.
  fn executor_with_steps() -> (Executor, EventStreamBus, Arc<Mutex<Vec<Step>>>) {
    let address = crate::kernel::address::Address::new("pekko", "ClosedSystemMessagesOrder");
    let root = crate::kernel::actor_path::RootActorPath::new(address);
    let mut executor = Executor::new(ActorPath::from_root(root), Arc::new(ChannelBoundary::new()));
    let bus = EventStreamBus::new();
    executor.set_dead_letter_bus(bus.downgrade());
    let steps: Arc<Mutex<Vec<Step>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = steps.clone();
    let collector = executor.function_ref("stopOrderCollector", move |_: &ActorRef, message: Message| {
      let carried = message
        .downcast_ref::<DeadLetter>()
        .map(DeadLetter::message)
        .or_else(|| message.downcast_ref::<SuppressedDeadLetter>().map(SuppressedDeadLetter::message));
      let Some(carried) = carried else {
        return;
      };
      if let Some(text) = carried.downcast_ref::<&'static str>() {
        sink.lock().unwrap().push(Step::Text(text));
      } else if let Some(system) = carried.downcast_ref::<SystemMessage>() {
        sink.lock().unwrap().push(Step::System(kind_of(system)));
      }
    });
    bus.subscribe(Some(collector.clone()), EventClass::of::<DeadLetter>()).expect("the collector");
    bus.subscribe(Some(collector), EventClass::of::<SuppressedDeadLetter>()).expect("the collector");
    (executor, bus, steps)
  }

  /// Stashes every message, so a stop has a stash to route.
  struct StashEverything;

  impl Actor for StashEverything {
    fn receive(&mut self, context: &mut crate::kernel::actor::ActorContext<'_>, _message: Message) {
      context.stash();
    }
  }

  // Issue #297, design section 7.2, fourth case, and section 5.1 with
  // decision 6: the stop-time clean-up routes what is left in the order
  // "overflow -> system messages -> stash -> queued envelopes".
  //
  // The actor has a mailbox of capacity 3, stashes three messages (the
  // overflow is routed first), holds a `Resume` on its system queue, and has
  // two more envelopes queued. On stop, the overflow goes first, then the
  // `Resume`, then the stash, then the queued envelopes.
  #[test]
  fn stopping_routes_the_overflow_then_system_messages_then_stash_then_envelopes() {
    use crate::kernel::actor::dispatch_settings::DispatchSettings;
    use crate::kernel::actor::dispatch_settings::MailboxDefinition;

    let (mut executor, _bus, steps) = executor_with_steps();
    let mut settings = DispatchSettings::new();
    settings.define_dispatcher("bounded3", MailboxDefinition::bounded(3, 20));
    executor.set_dispatch_settings(settings);
    let owner = executor.try_spawn_user(Props::new(|| StashEverything).with_dispatcher("bounded3"), None).unwrap();
    let sender = executor.try_spawn_user(Props::new(|| Sink), None).unwrap();
    for text in ["a", "b", "c"] {
      owner.tell(text, sender.clone());
    }
    executor.run_until_idle();
    // Two more envelopes stay in the queue while the stop runs.
    owner.tell("late1", sender.clone());
    owner.tell("late2", sender.clone());
    // Ask for the stop first, which queues `Terminate`. Then put a `Resume`
    // behind it on the same system queue, so the run takes `Terminate`,
    // closes the mailbox, and leaves the `Resume` for the close to return:
    // that is the `Resume` a stop overtook, and it reaches dead letters as
    // `DeadLetter(Resume)` (design section 5.1, the stop-time clean-up).
    executor.stop(&owner);
    {
      let entry = executor.entry(&owner).expect("the actor is in the table");
      let mailbox = entry.mailbox.clone();
      crate::kernel::dispatch::Mailbox::system_send(&mailbox, SystemMessage::Resume { caused_by_failure: false }).ok();
    }

    executor.run_until_idle();

    // Design section 5.1: the overflow from `postStop`'s `unstashAll` goes
    // first (b, a: the oldest excess, newest victim first), then the system
    // messages left when the mailbox closed (the `Resume`), then the rest of
    // the stash (c), then the queued envelopes (late1, late2). The same
    // direction as `bounded_stash_stop_counts_the_queued_backlog_against_the_mailbox_capacity`
    // (hello2, hello1, hello3, late1, late2).
    assert_eq!(
      *steps.lock().unwrap(),
      std::vec![
        Step::Text("b"),
        Step::Text("a"),
        Step::System(Kind::Resume),
        Step::Text("c"),
        Step::Text("late1"),
        Step::Text("late2"),
      ],
      "overflow, then system messages, then the stash, then the queued envelopes"
    );
  }

  // Issue #297, design section 7.2, fifth case: the `Terminate` that `stop`
  // sends directly (the `system_send` the design calls out) to a mailbox that
  // is already closed is suppressed, not dropped.
  #[test]
  fn a_terminate_that_stop_sends_to_a_closed_mailbox_is_suppressed() {
    let (mut executor, _bus, reports) = executor_with_reports();
    let (target, _held) = actor_with_a_closed_mailbox(&mut executor);
    let dead_letters = executor.dead_letters();

    executor.stop(&target);

    let seen = reports.lock().unwrap().clone();
    assert!(
      seen.iter().any(|report| matches!(
        report,
        Routed::Suppressed(Kind::Terminate, recipient, handle)
          if recipient == &target && handle == &dead_letters
      )),
      "stop routed the Terminate of a closed mailbox as a suppressed dead letter: {seen:?}"
    );
  }
}
