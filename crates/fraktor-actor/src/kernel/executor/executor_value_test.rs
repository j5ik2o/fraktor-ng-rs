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

impl Actor for Guardian {
  fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {}
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
    self.parents.len()
  }
}
