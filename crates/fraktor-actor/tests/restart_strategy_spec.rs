// Ports the five cases of Pekko's `RestartStrategySpec`
// (`actor-tests/src/test/scala/org/apache/pekko/actor/RestartStrategySpec.scala`
// at 9a54d1294c3ac7821bbed3c940769c3d5027d1ff) onto the port's supervisor
// strategies. Each case builds a real parent and child pair, sends the
// original's `Ping` and `Crash` in the same order, and observes the restarts,
// the post-restart pings, the stop, and the parent's `Terminated` through the
// production supervision path.
//
// The original waits on TestLatches and sleeps to cross a time window. The port
// uses the test kit's bounded waits for the first, and a hand-driven clock
// (`ActorSystem::set_clock`) for the second, so the tests add no real sleep.
//
// The strategy is a one-for-one whose decider restarts every `Throwable`, as
// the original spells out with `(List(classOf[Throwable]))`; the decider is
// installed through `with_decider` so the parent decides `Restart` itself.

use core::time::Duration;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use fraktor_actor::kernel::actor::{Actor, ActorContext, Props, Terminated};
use fraktor_actor::kernel::actor_ref::ActorRef;
use fraktor_actor::kernel::message::Message;
use fraktor_actor::kernel::supervision::{Directive, FailureKind, RestartLimit, RestartWindow, SupervisorStrategy};
use fraktor_actor::kernel::Clock;
use fraktor_testkit::default_timeout::DefaultTimeout;
use fraktor_testkit::fraktorspec::FraktorSpec;

const SPEC_SYSTEM_NAME: &str = "RestartStrategySpec";
const EMPLOYEE_NAME: &str = "employee";

fn is_ping(message: &Message) -> bool {
  message.downcast_ref::<Ping>().is_some()
}

fn is_crash(message: &Message) -> bool {
  message.downcast_ref::<Crash>().is_some()
}

// The original's `object Ping`: a message the employee counts down a latch for.
#[derive(Clone, Copy)]
struct Ping;

// The original's `object Crash`: a message the employee throws on.
#[derive(Clone, Copy)]
struct Crash;

// A hand-driven monotonic clock the windowed cases advance without sleeping.
struct ControlledClock(AtomicUsize);

impl ControlledClock {
  fn set_millis(&self, millis: usize) {
    self.0.store(millis, Ordering::SeqCst);
  }
}

impl Clock for ControlledClock {
  fn monotonic(&self) -> Duration {
    Duration::from_millis(self.0.load(Ordering::SeqCst) as u64)
  }
}

// The original's `(List(classOf[Throwable]))` decider: restart every failure.
fn restart_on_throwable(_child: &ActorRef, _failure: FailureKind) -> Directive {
  Directive::Restart
}

// The original's `Supervisor` helper: answers a `Props` with a single named
// employee created under its own strategy.
struct Supervisor {
  strategy: SupervisorStrategy,
}

impl Actor for Supervisor {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    let Some(props) = message.downcast_ref::<Props>() else {
      return;
    };
    let employee = context.actor_of(props.clone(), EMPLOYEE_NAME).expect("the supervisor starts its employee");
    context.sender().tell(employee, context.self_ref());
  }

  fn supervisor_strategy(&self) -> SupervisorStrategy {
    self.strategy.clone()
  }
}

fn supervisor(spec: &mut FraktorSpec, strategy: SupervisorStrategy) -> ActorRef {
  spec.system_mut().actor_of(Props::new(move || Supervisor { strategy: strategy.clone() }))
}

// Asks the parent for the employee built from `make`, mirroring `boss ? employeeProps`.
fn employee_under<A: Actor + 'static>(
  spec: &FraktorSpec,
  boss: &ActorRef,
  make: impl Fn() -> A + Send + Sync + 'static,
) -> ActorRef {
  let props = Props::new(make);
  let timeout = DefaultTimeout::timeout(spec.system());
  boss
    .ask_with_timeout(props, timeout)
    .await_result(timeout.duration())
    .downcast_ref::<ActorRef>()
    .cloned()
    .expect("the employee handle")
}

// Shared by cases 1, 3, and 4. The original's stop latch also opens when the
// default `preRestart` destroys an old instance, so `postStop` counts include
// those instances; termination is checked separately.
struct CountingEmployee {
  pings: Arc<AtomicUsize>,
  restarts: Arc<AtomicUsize>,
  stops: Arc<AtomicUsize>,
}

impl Actor for CountingEmployee {
  fn receive(&mut self, _context: &mut ActorContext<'_>, message: Message) {
    if is_ping(&message) {
      self.pings.fetch_add(1, Ordering::SeqCst);
      return;
    }
    if is_crash(&message) {
      panic!("Crashing...");
    }
  }

  fn post_restart(&mut self, _context: &mut ActorContext<'_>) {
    self.restarts.fetch_add(1, Ordering::SeqCst);
  }

  fn post_stop(&mut self) {
    self.stops.fetch_add(1, Ordering::SeqCst);
  }
}

// The original's immortal employee for case 2: records one restart per crash.
struct ImmortalEmployee {
  restarts: Arc<AtomicUsize>,
}

impl Actor for ImmortalEmployee {
  fn receive(&mut self, _context: &mut ActorContext<'_>, message: Message) {
    if is_crash(&message) {
      panic!("Crashing...");
    }
  }

  fn post_restart(&mut self, _context: &mut ActorContext<'_>) {
    self.restarts.fetch_add(1, Ordering::SeqCst);
  }
}

// Case 1: `OneForOneStrategy(maxNrOfRetries = 2, withinTimeRange = 1.second)`.
// The original sends Ping, Crash, Ping (three pings in total with the one after
// the second restart), and one more Ping after the second crash. All three
// pings are served; the third crash, exactly on the 1 second boundary which
// counts as inside the window, is the third processed failure and stops the
// employee.
#[test]
fn stops_after_max_restarts_within_time_range() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let clock = Arc::new(ControlledClock(AtomicUsize::new(0)));
  spec.system().set_clock(clock.clone());
  let strategy =
    SupervisorStrategy::one_for_one_with_window(RestartLimit::new(2), RestartWindow::Within(Duration::from_secs(1)))
      .with_decider(restart_on_throwable);
  let boss = supervisor(&mut spec, strategy);
  let pings = Arc::new(AtomicUsize::new(0));
  let restarts = Arc::new(AtomicUsize::new(0));
  let stops = Arc::new(AtomicUsize::new(0));
  let employee = employee_under(&spec, &boss, {
    let pings = Arc::clone(&pings);
    let restarts = Arc::clone(&restarts);
    let stops = Arc::clone(&stops);
    move || CountingEmployee { pings: Arc::clone(&pings), restarts: Arc::clone(&restarts), stops: Arc::clone(&stops) }
  });

  employee.tell(Ping, spec.test_actor());
  employee.tell(Crash, spec.test_actor());
  employee.tell(Ping, spec.test_actor());
  spec.system_mut().run_until_idle();
  assert_eq!(restarts.load(Ordering::SeqCst), 1, "the first crash restarts the employee");
  assert_eq!(pings.load(Ordering::SeqCst), 2, "the pre-crash and first post-restart pings are served");
  assert_eq!(stops.load(Ordering::SeqCst), 1, "the first restart destroys one old instance");

  employee.tell(Crash, spec.test_actor());
  employee.tell(Ping, spec.test_actor());
  spec.system_mut().run_until_idle();
  assert_eq!(restarts.load(Ordering::SeqCst), 2, "the second crash restarts the employee");
  assert_eq!(pings.load(Ordering::SeqCst), 3, "the third ping after the second restart is served");
  assert_eq!(stops.load(Ordering::SeqCst), 2, "the second restart destroys the second old instance");
  assert!(!spec.system().is_actor_terminated(&employee), "the employee is alive after two restarts");

  // Exactly on the window boundary: the boundary counts as inside, so the third
  // crash is the third processed failure and the employee stops.
  clock.set_millis(1000);
  employee.tell(Crash, spec.test_actor());
  spec.system_mut().run_until_idle();
  assert_eq!(restarts.load(Ordering::SeqCst), 2, "the third crash inside the window does not restart");
  assert!(spec.system().is_actor_terminated(&employee), "the employee stops after max restarts in the window");
  assert_eq!(stops.load(Ordering::SeqCst), 3, "two restarts and the final stop run three stop hooks");
  spec.terminate();
}

// Case 2: `OneForOneStrategy()` with no retry bound and no window. One hundred
// crashes sent in a row restart the employee one hundred times and never stop
// it.
#[test]
fn restarts_one_hundred_times_without_limits() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let strategy = SupervisorStrategy::one_for_one_with_window(RestartLimit::unlimited(), RestartWindow::Unbounded)
    .with_decider(restart_on_throwable);
  let boss = supervisor(&mut spec, strategy);
  let restarts = Arc::new(AtomicUsize::new(0));
  let employee = employee_under(&spec, &boss, {
    let restarts = Arc::clone(&restarts);
    move || ImmortalEmployee { restarts: Arc::clone(&restarts) }
  });

  for _ in 0..100 {
    employee.tell(Crash, spec.test_actor());
  }
  spec.system_mut().run_until_idle();
  assert_eq!(restarts.load(Ordering::SeqCst), 100, "each crash restarts the employee");
  assert!(!spec.system().is_actor_terminated(&employee), "the employee is immortal without limits");
  spec.terminate();
}

// Case 3: `OneForOneStrategy(maxNrOfRetries = 2, withinTimeRange = 500.millis)`.
// Each Ping/Crash stage is checked: the first Ping and restart, then the second
// Ping and restart, then after 700ms on the driven clock (past the 500ms bound)
// the third crash reopens the window and restarts, and the third Ping is
// served.
#[test]
fn restarts_after_time_range_expires() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let clock = Arc::new(ControlledClock(AtomicUsize::new(0)));
  spec.system().set_clock(clock.clone());
  let strategy = SupervisorStrategy::one_for_one_with_window(
    RestartLimit::new(2),
    RestartWindow::Within(Duration::from_millis(500)),
  )
  .with_decider(restart_on_throwable);
  let boss = supervisor(&mut spec, strategy);
  let pings = Arc::new(AtomicUsize::new(0));
  let restarts = Arc::new(AtomicUsize::new(0));
  let stops = Arc::new(AtomicUsize::new(0));
  let employee = employee_under(&spec, &boss, {
    let pings = Arc::clone(&pings);
    let restarts = Arc::clone(&restarts);
    let stops = Arc::clone(&stops);
    move || CountingEmployee { pings: Arc::clone(&pings), restarts: Arc::clone(&restarts), stops: Arc::clone(&stops) }
  });

  employee.tell(Ping, spec.test_actor());
  employee.tell(Crash, spec.test_actor());
  spec.system_mut().run_until_idle();
  assert_eq!(pings.load(Ordering::SeqCst), 1, "the first ping is served");
  assert_eq!(restarts.load(Ordering::SeqCst), 1, "the first crash restarts the employee");

  employee.tell(Ping, spec.test_actor());
  employee.tell(Crash, spec.test_actor());
  spec.system_mut().run_until_idle();
  assert_eq!(pings.load(Ordering::SeqCst), 2, "the second ping is served");
  assert_eq!(restarts.load(Ordering::SeqCst), 2, "the second crash restarts the employee inside the window");
  assert!(!spec.system().is_actor_terminated(&employee), "the employee survives two restarts in the window");

  clock.set_millis(700);
  employee.tell(Crash, spec.test_actor());
  employee.tell(Ping, spec.test_actor());
  spec.system_mut().run_until_idle();
  assert_eq!(restarts.load(Ordering::SeqCst), 3, "the crash past the window restarts the employee");
  assert_eq!(pings.load(Ordering::SeqCst), 3, "the ping after the third restart is served");
  assert_eq!(stops.load(Ordering::SeqCst), 3, "three restarts destroy three old instances");
  assert!(!spec.system().is_actor_terminated(&employee), "the employee stays alive after the window reopens");
  spec.terminate();
}

// Case 4: `OneForOneStrategy(maxNrOfRetries = 2)` with no window. The original
// sends Ping, Crash, Ping (three pings in total with the one after the second
// restart). Two crashes restart the employee; the time that passes with the
// clock at 700ms does not restore the count, so the third crash stops it.
#[test]
fn stops_after_max_retries_without_time_range() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let clock = Arc::new(ControlledClock(AtomicUsize::new(0)));
  spec.system().set_clock(clock.clone());
  let strategy = SupervisorStrategy::one_for_one(2).with_decider(restart_on_throwable);
  let boss = supervisor(&mut spec, strategy);
  let pings = Arc::new(AtomicUsize::new(0));
  let restarts = Arc::new(AtomicUsize::new(0));
  let stops = Arc::new(AtomicUsize::new(0));
  let employee = employee_under(&spec, &boss, {
    let pings = Arc::clone(&pings);
    let restarts = Arc::clone(&restarts);
    let stops = Arc::clone(&stops);
    move || CountingEmployee { pings: Arc::clone(&pings), restarts: Arc::clone(&restarts), stops: Arc::clone(&stops) }
  });

  employee.tell(Ping, spec.test_actor());
  employee.tell(Crash, spec.test_actor());
  employee.tell(Ping, spec.test_actor());
  spec.system_mut().run_until_idle();
  assert_eq!(restarts.load(Ordering::SeqCst), 1, "the first crash restarts the employee");
  assert_eq!(pings.load(Ordering::SeqCst), 2, "the pre-crash and first post-restart pings are served");
  assert_eq!(stops.load(Ordering::SeqCst), 1, "the first restart destroys one old instance");

  employee.tell(Crash, spec.test_actor());
  employee.tell(Ping, spec.test_actor());
  spec.system_mut().run_until_idle();
  assert_eq!(restarts.load(Ordering::SeqCst), 2, "the second crash restarts the employee");
  assert_eq!(pings.load(Ordering::SeqCst), 3, "the third ping after the second restart is served");
  assert_eq!(stops.load(Ordering::SeqCst), 2, "the second restart destroys the second old instance");

  clock.set_millis(700);
  employee.tell(Crash, spec.test_actor());
  spec.system_mut().run_until_idle();
  assert_eq!(restarts.load(Ordering::SeqCst), 2, "time passing does not restore the count without a window");
  assert!(spec.system().is_actor_terminated(&employee), "the employee stops after max retries");
  assert_eq!(stops.load(Ordering::SeqCst), 3, "two restarts and the final stop run three stop hooks");
  spec.terminate();
}

// The parent of case 5: starts the single employee under its own windowed
// one-for-one strategy, watches it, and records the `Terminated` it receives for
// its own child, exactly as the original's anonymous actor does.
struct WatchingParent {
  strategy: SupervisorStrategy,
  terminated: Arc<AtomicUsize>,
  terminated_target: Arc<std::sync::Mutex<Option<ActorRef>>>,
}

impl Actor for WatchingParent {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    if let Some(terminated) = message.downcast_ref::<Terminated>() {
      self.terminated.fetch_add(1, Ordering::SeqCst);
      *self.terminated_target.lock().expect("the terminated-target lock is not poisoned") =
        Some(terminated.target().clone());
      return;
    }
    let Some(props) = message.downcast_ref::<Props>() else {
      return;
    };
    let employee = context.actor_of(props.clone(), EMPLOYEE_NAME).expect("the parent starts its employee");
    let watched = context.watch(&employee);
    context.sender().tell(watched, context.self_ref());
  }

  fn supervisor_strategy(&self) -> SupervisorStrategy {
    self.strategy.clone()
  }
}

// Case 5: `OneForOneStrategy(withinTimeRange = 1.second)` with no retry bound.
// A time window alone makes `requestRestartPermission` allow one restart, so the
// first crash restarts the employee inside the window and the second processed
// crash stops it. The parent, which watched the employee, receives its
// `Terminated`, and that notice names the same employee the test created. The
// original's extra `employee ! Ping` and `employee ! Crash` after the second
// crash reach an employee that is already stopping, so they are not processed;
// the test records two processed crashes and two pings, with one stop hook
// from the restart and one from termination.
#[test]
fn stops_and_notifies_parent_with_only_time_range() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let clock = Arc::new(ControlledClock(AtomicUsize::new(0)));
  spec.system().set_clock(clock.clone());
  let strategy = SupervisorStrategy::one_for_one_with_window(
    RestartLimit::unlimited(),
    RestartWindow::Within(Duration::from_secs(1)),
  )
  .with_decider(restart_on_throwable);
  let terminated = Arc::new(AtomicUsize::new(0));
  let terminated_target = Arc::new(std::sync::Mutex::new(None));
  let boss = spec.system_mut().actor_of(Props::new({
    let terminated = Arc::clone(&terminated);
    let terminated_target = Arc::clone(&terminated_target);
    move || WatchingParent {
      strategy: strategy.clone(),
      terminated: Arc::clone(&terminated),
      terminated_target: Arc::clone(&terminated_target),
    }
  }));
  let pings = Arc::new(AtomicUsize::new(0));
  let restarts = Arc::new(AtomicUsize::new(0));
  let stops = Arc::new(AtomicUsize::new(0));
  let crashes = Arc::new(AtomicUsize::new(0));
  let employee = employee_under(&spec, &boss, {
    let pings = Arc::clone(&pings);
    let restarts = Arc::clone(&restarts);
    let stops = Arc::clone(&stops);
    let crashes = Arc::clone(&crashes);
    move || CrashedWindowEmployee {
      pings: Arc::clone(&pings),
      restarts: Arc::clone(&restarts),
      stops: Arc::clone(&stops),
      crashes: Arc::clone(&crashes),
    }
  });

  employee.tell(Ping, spec.test_actor());
  employee.tell(Crash, spec.test_actor());
  employee.tell(Ping, spec.test_actor());
  spec.system_mut().run_until_idle();
  assert_eq!(crashes.load(Ordering::SeqCst), 1, "the first batch delivers one crash");
  assert_eq!(restarts.load(Ordering::SeqCst), 1, "a time window alone allows one restart");
  assert_eq!(pings.load(Ordering::SeqCst), 2, "both pre-stop pings are served");
  assert_eq!(stops.load(Ordering::SeqCst), 1, "the restart destroys one old instance");
  assert!(!spec.system().is_actor_terminated(&employee), "the employee is alive after one restart");

  employee.tell(Crash, spec.test_actor());
  employee.tell(Ping, spec.test_actor());
  employee.tell(Crash, spec.test_actor());
  spec.system_mut().run_until_idle();
  assert_eq!(crashes.load(Ordering::SeqCst), 2, "the first batch and the second crash reach the employee");
  assert_eq!(restarts.load(Ordering::SeqCst), 1, "the second processed crash does not restart");
  assert_eq!(stops.load(Ordering::SeqCst), 2, "one restart and the final stop run two stop hooks");
  assert!(spec.system().is_actor_terminated(&employee), "the employee is terminated");
  assert_eq!(pings.load(Ordering::SeqCst), 2, "the ping after the stop is not processed");
  assert_eq!(terminated.load(Ordering::SeqCst), 1, "the watching parent receives one Terminated");
  let target = terminated_target.lock().expect("the terminated-target lock is not poisoned").clone();
  assert_eq!(target.as_ref(), Some(&employee), "the Terminated names the employee the test created");
  spec.terminate();
}

// Case 5's employee: like `CountingEmployee`, but also counts the crash messages
// that reach it, so the test can show the stop-after ping and crash are not
// processed.
struct CrashedWindowEmployee {
  pings: Arc<AtomicUsize>,
  restarts: Arc<AtomicUsize>,
  stops: Arc<AtomicUsize>,
  crashes: Arc<AtomicUsize>,
}

impl Actor for CrashedWindowEmployee {
  fn receive(&mut self, _context: &mut ActorContext<'_>, message: Message) {
    if is_ping(&message) {
      self.pings.fetch_add(1, Ordering::SeqCst);
      return;
    }
    if is_crash(&message) {
      self.crashes.fetch_add(1, Ordering::SeqCst);
      panic!("Crashing...");
    }
  }

  fn post_restart(&mut self, _context: &mut ActorContext<'_>) {
    self.restarts.fetch_add(1, Ordering::SeqCst);
  }

  fn post_stop(&mut self) {
    self.stops.fetch_add(1, Ordering::SeqCst);
  }
}
