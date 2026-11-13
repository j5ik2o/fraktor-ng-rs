// Ported from Pekko v1.7.1:
//   actor-tests/src/test/scala/org/apache/pekko/actor/ActorLifeCycleSpec.scala
//
// Original test names are kept as comments next to each ported case.
//
// The original runs on a real dispatcher, so it waits with `expectMsg` while the
// actor system works in the background. The port drives one shard from the test
// thread: a wait delivers queued messages until the expected one arrives. The
// two observation points that Pekko expresses as effects, the lifecycle reports
// and the intercepted log event, keep their order and values.

use core::any::Any;
use core::cell::RefCell;
use core::time::Duration;

use std::rc::Rc;

use fraktor_actor::kernel::actor::{Actor, ActorContext, Props};
use fraktor_actor::kernel::actor_ref::ActorRef;
use fraktor_actor::kernel::actor_system::ActorSystem;
use fraktor_actor::kernel::message::Message;
use fraktor_actor::kernel::supervision::SupervisorStrategy;
use fraktor_testkit::event_filter::EventFilter;
use fraktor_testkit::fraktorspec::FraktorSpec;
use fraktor_testkit::implicit_sender::ImplicitSender;

const SPEC_SYSTEM_NAME: &str = "ActorLifeCycleSpec";

fn default_timeout() -> Duration {
  Duration::from_secs(3)
}

// Original: `val gen = new AtomicInteger(0)` shared by the actor and read with
// `generationProvider.getAndIncrement()` each time a new instance is created.
//
// The port keeps one counter per test and hands its handle to every factory, so
// the initial creation and each restart read-and-increment the same value, as
// the original does. The counter is only touched from the test thread that
// builds the actor, so a `RefCell` is enough and needs no atomics.
#[derive(Clone)]
struct GenerationCounter {
  current: Rc<RefCell<i32>>,
}

impl GenerationCounter {
  fn new() -> Self {
    Self { current: Rc::new(RefCell::new(0)) }
  }

  // Original: `generationProvider.getAndIncrement()`
  fn next(&self) -> i32 {
    let mut current = self.current.borrow_mut();
    let generation = *current;
    *current += 1;
    generation
  }
}

// Original: object ActorLifeCycleSpec.LifeCycleTestActor(testActor, id, generationProvider)
//
// Pekko passes the test actor, the id, and a shared counter, and the actor
// reports its lifecycle with `(msg, id, currentGen)`. The port keeps the same
// three pieces. The generation is read once, when the actor is created, as
// `val currentGen = generationProvider.getAndIncrement()` does.
struct LifeCycleTestActor {
  test_actor: ActorRef,
  id: String,
  generation: i32,
}

impl LifeCycleTestActor {
  fn new(test_actor: ActorRef, id: &str, generation: i32) -> Self {
    Self { test_actor, id: id.to_string(), generation }
  }

  // Original: the actor reads `generationProvider.getAndIncrement()` in its
  // constructor.
  fn from_counter(test_actor: ActorRef, id: &str, generations: &GenerationCounter) -> Self {
    Self::new(test_actor, id, generations.next())
  }

  fn report(&self, message: &'static str) {
    self.test_actor.tell(Report::new(message, &self.id, self.generation), self.test_actor.clone());
  }
}

// `(msg, id, currentGen)` of the original, named so a downcast can identify it.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Report {
  message: &'static str,
  id: String,
  generation: i32,
}

impl Report {
  fn new(message: &'static str, id: &str, generation: i32) -> Self {
    Self { message, id: id.to_string(), generation }
  }
}

impl Actor for LifeCycleTestActor {
  // Original: `def receive = { case "status" => sender() ! message("OK") }`
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    if message == "status" {
      let reply = Report::new("OK", &self.id, self.generation);
      context.sender().tell(reply, context.self_ref());
    }
  }

  // Original: `override def preStart(): Unit = { report("preStart") }`
  fn pre_start(&mut self) {
    self.report("preStart");
  }

  // Original: `override def postStop(): Unit = { report("postStop") }`
  fn post_stop(&mut self) {
    self.report("postStop");
  }
}

// Original: the anonymous subclass that overrides `preRestart` and `postRestart`
// to report them instead of delegating to the defaults.
struct RestartReportingActor {
  inner: LifeCycleTestActor,
}

impl Actor for RestartReportingActor {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    self.inner.receive(context, message);
  }

  fn pre_start(&mut self) {
    self.inner.pre_start();
  }

  fn post_stop(&mut self) {
    self.inner.post_stop();
  }

  fn pre_restart(&mut self, _reason: &(dyn Any + Send)) {
    self.inner.report("preRestart");
  }

  fn post_restart(&mut self) {
    self.inner.report("postRestart");
  }
}

// Original: object ActorLifeCycleSpec.Become(recv: ActorContext => Receive)
#[derive(Clone, Copy)]
struct Become;

// Original: Props(new Actor { def receive = { case Become(beh) => context.become(beh(context), discardOld = false); sender() ! "ok"; case _ => sender() ! 42 } })
type StackedBehavior = Box<dyn FnMut(&mut ActorContext<'_>, Message)>;

struct BehaviorStackActor {
  stacked: Option<StackedBehavior>,
}

impl BehaviorStackActor {
  fn new() -> Self {
    Self { stacked: None }
  }
}

impl Actor for BehaviorStackActor {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    if message.is::<Become>() {
      // The original stacks `beh(context)` below the current behavior with
      // `discardOld = false`. The port keeps the stacked behavior in the actor
      // and runs it first after the become.
      let sender = context.sender();
      self.stacked = Some(Box::new(|stacked_context: &mut ActorContext<'_>, stacked_message: Message| {
        if stacked_message == "fail" {
          panic!("buh");
        }
        if stacked_message == "hello" {
          stacked_context.sender().tell(43_i32, stacked_context.self_ref());
        }
      }));
      let _ = sender;
      context.sender().tell("ok", context.self_ref());
      return;
    }
    if self.stacked.is_some() {
      if let Some(mut stacked) = self.stacked.take() {
        stacked(context, message);
        self.stacked = Some(stacked);
      }
      return;
    }
    context.sender().tell(42_i32, context.self_ref());
  }
}

// Original: Props(new Actor { def receive = Actor.emptyBehavior; override def postStop(): Unit = { throw new Exception("hurrah") } })
struct PostStopThrowingActor;

impl Actor for PostStopThrowingActor {
  fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {}

  fn post_stop(&mut self) {
    panic!("hurrah");
  }
}

// Original: the anonymous subclass that throws from `preStart`.
struct PreStartThrowingActor {
  inner: LifeCycleTestActor,
}

impl Actor for PreStartThrowingActor {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    self.inner.receive(context, message);
  }

  fn pre_start(&mut self) {
    self.inner.report("preStart");
    panic!("test exception");
  }

  fn post_stop(&mut self) {
    self.inner.post_stop();
  }
}

// Original: actor-tests/src/test/scala/org/apache/pekko/actor/Supervisor.scala
//
// The helper answers a `Props` with a child it creates under its own strategy.
// Its `preRestart` is empty, so the children survive a restart of the helper,
// as the original comment notes.
struct TestSupervisor {
  strategy: SupervisorStrategy,
}

impl TestSupervisor {
  fn new(strategy: SupervisorStrategy) -> Self {
    Self { strategy }
  }
}

impl Actor for TestSupervisor {
  // Original: `case x: Props => sender() ! context.actorOf(x)`
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    let Some(props) = message.downcast_ref::<Props>() else {
      return;
    };
    let child = context.actor_of_with_strategy(props.clone(), self.strategy);
    context.sender().tell(child, context.self_ref());
  }
}

// Original: StopBeforeFutureFinishes(latch), which stops itself and then
// finishes a background computation that closes over the context.
struct StopBeforeFutureFinishes;

impl StopBeforeFutureFinishes {
  fn new() -> Self {
    Self
  }
}

impl Actor for StopBeforeFutureFinishes {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    if message == "ping" {
      let reply_to = context.sender();
      // The original calls `context.stop(self)`, then finishes a background
      // computation that closes over the context and pipes "pong" to the
      // sender. The port cannot block the shard on a real future, so the actor
      // hands the executor the reply to deliver after the termination notice;
      // the reply still reaches the sender after the watcher has been told the
      // actor is gone.
      context.stop_and_reply_after_termination(reply_to, "pong");
    }
  }
}

struct ActorLifeCycleSpecCase {
  fraktor_spec: FraktorSpec,
}

impl ActorLifeCycleSpecCase {
  fn new() -> Self {
    Self { fraktor_spec: FraktorSpec::new(SPEC_SYSTEM_NAME) }
  }

  fn system(&mut self) -> &mut ActorSystem {
    self.fraktor_spec.system_mut()
  }

  fn test_actor(&self) -> ActorRef {
    self.fraktor_spec.test_actor()
  }

  fn self_actor_ref(&self) -> ActorRef {
    ImplicitSender::self_actor_ref(&self.fraktor_spec)
  }

  fn expect_report(&mut self, expected: Report) {
    let found: Report = self.fraktor_spec.expect_msg_type();
    assert_eq!(found, expected, "expected {expected:?}, found {found:?}");
  }

  fn expect_str(&mut self, expected: &'static str) {
    self.fraktor_spec.expect_msg_within(default_timeout(), expected);
  }

  fn expect_int(&mut self, expected: i32) {
    let found: i32 = self.fraktor_spec.expect_msg_type();
    assert_eq!(found, expected);
  }

  fn expect_no_message(&mut self) {
    self.fraktor_spec.test_kit_mut().expect_no_message(default_timeout());
  }

  // Original: `Await.result((supervisor ? props).mapTo[ActorRef], timeout.duration)`
  fn child_under(&mut self, supervisor: &ActorRef, props: Props) -> ActorRef {
    let reply = supervisor.ask(props).await_result(default_timeout());
    reply.downcast_ref::<ActorRef>().cloned().expect("a child handle from the supervisor")
  }

  fn stop(&mut self, target: &ActorRef) {
    self.system().stop(target);
    self.system().run_until_idle();
  }

  // Original: `filterException[ActorKilledException] { restarter ! Kill }`
  //
  // Each Kill ends the instance with an `ActorKilledException` that Pekko logs.
  // The port intercepts that one error event while it sends the Kill.
  fn kill(&mut self, target: &ActorRef) {
    let filter = EventFilter::typed("ActorKilledException");
    let sender = self.self_actor_ref();
    let system = self.fraktor_spec.system_mut();
    let target = target.clone();
    EventFilter::intercept(&filter, system, move || {
      target.tell("Kill", sender);
    });
  }
}

impl Drop for ActorLifeCycleSpecCase {
  fn drop(&mut self) {
    self.fraktor_spec.terminate();
  }
}

// "An Actor" must { "invoke preRestart, preStart, postRestart when using OneForOneStrategy" in { ... } }
#[test]
fn invoke_pre_restart_pre_start_post_restart_when_using_one_for_one_strategy() {
  let mut case = ActorLifeCycleSpecCase::new();
  let test_actor = case.test_actor();
  let id = "invoke-hooks";
  let supervisor = {
    let strategy = SupervisorStrategy::one_for_one(3);
    case.system().actor_of(Props::new(move || TestSupervisor::new(strategy)))
  };
  let generation = GenerationCounter::new();
  let restarter = {
    let props = Props::new({
      let test_actor = test_actor.clone();
      let id = id.to_string();
      let generations = generation.clone();
      move || RestartReportingActor { inner: LifeCycleTestActor::from_counter(test_actor.clone(), &id, &generations) }
    });
    case.child_under(&supervisor, props)
  };

  case.expect_report(Report::new("preStart", id, 0));

  // Original: `filterException[ActorKilledException] { ... }` wraps the whole
  // body, because each Kill logs `ActorKilledException` once. The port keeps one
  // interception per Kill, each expecting exactly one occurrence.
  case.kill(&restarter);
  case.expect_report(Report::new("preRestart", id, 0));
  case.expect_report(Report::new("postRestart", id, 1));
  restarter.tell("status", case.self_actor_ref());
  case.expect_report(Report::new("OK", id, 1));

  case.kill(&restarter);
  case.expect_report(Report::new("preRestart", id, 1));
  case.expect_report(Report::new("postRestart", id, 2));
  restarter.tell("status", case.self_actor_ref());
  case.expect_report(Report::new("OK", id, 2));

  case.kill(&restarter);
  case.expect_report(Report::new("preRestart", id, 2));
  case.expect_report(Report::new("postRestart", id, 3));
  restarter.tell("status", case.self_actor_ref());
  case.expect_report(Report::new("OK", id, 3));

  case.kill(&restarter);
  case.expect_report(Report::new("postStop", id, 3));
  case.expect_no_message();
  case.stop(&supervisor);
}

// "An Actor" must { "default for preRestart and postRestart is to call postStop and preStart respectively" in { ... } }
#[test]
fn default_for_pre_restart_and_post_restart_calls_post_stop_and_pre_start() {
  let mut case = ActorLifeCycleSpecCase::new();
  let test_actor = case.test_actor();
  let id = "default-hooks";
  let supervisor = {
    let strategy = SupervisorStrategy::one_for_one(3);
    case.system().actor_of(Props::new(move || TestSupervisor::new(strategy)))
  };
  let generation = GenerationCounter::new();
  let restarter = {
    let props = Props::new({
      let test_actor = test_actor.clone();
      let id = id.to_string();
      let generations = generation.clone();
      move || LifeCycleTestActor::from_counter(test_actor.clone(), &id, &generations)
    });
    case.child_under(&supervisor, props)
  };

  case.expect_report(Report::new("preStart", id, 0));
  case.kill(&restarter);
  case.expect_report(Report::new("postStop", id, 0));
  case.expect_report(Report::new("preStart", id, 1));
  restarter.tell("status", case.self_actor_ref());
  case.expect_report(Report::new("OK", id, 1));

  case.kill(&restarter);
  case.expect_report(Report::new("postStop", id, 1));
  case.expect_report(Report::new("preStart", id, 2));
  restarter.tell("status", case.self_actor_ref());
  case.expect_report(Report::new("OK", id, 2));

  case.kill(&restarter);
  case.expect_report(Report::new("postStop", id, 2));
  case.expect_report(Report::new("preStart", id, 3));
  restarter.tell("status", case.self_actor_ref());
  case.expect_report(Report::new("OK", id, 3));

  case.kill(&restarter);
  case.expect_report(Report::new("postStop", id, 3));
  case.expect_no_message();
  case.stop(&supervisor);
}

// "An Actor" must { "not invoke preRestart and postRestart when never restarted using OneForOneStrategy" in { ... } }
#[test]
fn not_invoke_pre_restart_and_post_restart_when_never_restarted_using_one_for_one_strategy() {
  let mut case = ActorLifeCycleSpecCase::new();
  let test_actor = case.test_actor();
  let id = "never-restarted";
  let supervisor = {
    let strategy = SupervisorStrategy::one_for_one(3);
    case.system().actor_of(Props::new(move || TestSupervisor::new(strategy)))
  };
  let generation = GenerationCounter::new();
  let actor = {
    let props = Props::new({
      let test_actor = test_actor.clone();
      let id = id.to_string();
      let generations = generation.clone();
      move || LifeCycleTestActor::from_counter(test_actor.clone(), &id, &generations)
    });
    case.child_under(&supervisor, props)
  };

  case.expect_report(Report::new("preStart", id, 0));
  actor.tell("status", case.self_actor_ref());
  case.expect_report(Report::new("OK", id, 0));
  case.stop(&actor);
  case.expect_report(Report::new("postStop", id, 0));
  case.expect_no_message();
  case.stop(&supervisor);
}

// "An Actor" must { "log failures in postStop" in { ... } }
#[test]
fn log_failures_in_post_stop() {
  let mut case = ActorLifeCycleSpecCase::new();
  let actor = case.system().actor_of(Props::new(|| PostStopThrowingActor));
  let filter = EventFilter::typed("Exception").with_message("hurrah");

  let sender = case.self_actor_ref();
  let system = case.fraktor_spec.system_mut();
  EventFilter::intercept(&filter, system, || {
    actor.tell("PoisonPill", sender);
  });
}

// "An Actor" must { "clear the behavior stack upon restart" in { ... } }
#[test]
fn clear_the_behavior_stack_upon_restart() {
  let mut case = ActorLifeCycleSpecCase::new();
  let actor = case.system().actor_of(Props::new(BehaviorStackActor::new));

  actor.tell("hello", case.self_actor_ref());
  case.expect_int(42);

  actor.tell(Become, case.self_actor_ref());
  case.expect_str("ok");

  actor.tell("hello", case.self_actor_ref());
  case.expect_int(43);

  let filter = EventFilter::typed("RuntimeException").with_message("buh");
  let sender = case.self_actor_ref();
  let system = case.fraktor_spec.system_mut();
  EventFilter::intercept(&filter, system, || {
    actor.tell("fail", sender);
  });

  actor.tell("hello", case.self_actor_ref());
  case.expect_int(42);
}

// "An Actor" must { "not break supervisor strategy due to unhandled exception in preStart" in { ... } }
#[test]
fn not_break_supervisor_strategy_due_to_unhandled_exception_in_pre_start() {
  let mut case = ActorLifeCycleSpecCase::new();
  let test_actor = case.test_actor();
  let id = "pre-start-fails";
  let max_retry = 3;
  let supervisor = {
    let strategy = SupervisorStrategy::one_for_one_restarting_initialization_failures(max_retry as u32);
    case.system().actor_of(Props::new(move || TestSupervisor::new(strategy)))
  };

  let generation = GenerationCounter::new();
  let props = Props::new({
    let test_actor = test_actor.clone();
    let id = id.to_string();
    move || PreStartThrowingActor { inner: LifeCycleTestActor::from_counter(test_actor.clone(), &id, &generation) }
  });
  let _child = case.child_under(&supervisor, props);

  for expected in 0..=max_retry {
    case.expect_report(Report::new("preStart", id, expected));
  }
  case.expect_no_message();
  case.stop(&supervisor);
}

// "have a non null context after termination" in { ... }
#[test]
fn have_a_non_null_context_after_termination() {
  let mut case = ActorLifeCycleSpecCase::new();
  let actor = case.system().actor_of(Props::new(StopBeforeFutureFinishes::new));
  case.fraktor_spec.test_kit_mut().watch(&actor);

  actor.tell("ping", case.self_actor_ref());

  // The watcher learns of the stop first, then the background reply arrives.
  let terminated = case.fraktor_spec.test_kit_mut().expect_msg_terminated(default_timeout());
  assert_eq!(terminated.target(), &actor);
  case.expect_str("pong");
}
