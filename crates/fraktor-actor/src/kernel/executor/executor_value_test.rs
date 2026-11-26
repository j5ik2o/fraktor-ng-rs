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
