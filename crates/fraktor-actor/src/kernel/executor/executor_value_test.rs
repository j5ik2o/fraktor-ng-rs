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
    assert_eq!(*healthy_creations.lock().unwrap(), 1);
    assert_eq!(*values.lock().unwrap(), alloc::vec![(healthy.clone(), 1), (healthy, 1)]);
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
    // stop completes before post_restart, while the sibling's stays pending.
    restarted.tell("fail", guardian.clone());
    system.run_until_idle();

    let observed = observations.lock().unwrap();
    assert_eq!(observed.steps, alloc::vec!["grandchild started", "pre_restart", "post_restart"]);
    assert_eq!(observed.grandchild_stopped_before_post_restart, Some(true));
    assert_eq!(observed.sibling_alive_in_post_restart, Some(true));
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

  assert_eq!(*delivered.lock().unwrap(), alloc::vec![(sender.clone(), "a"), (sender, "b")]);
  assert!(received.lock().unwrap().is_empty());
  executor.run_until_idle();
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
