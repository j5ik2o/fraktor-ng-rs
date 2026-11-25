// Checked against Pekko v2.0.0 (SHA 9fbd2283b1cf701c90a1223bf621942c191db61f): the upstream test
// file changes only by 870d4499cc #2235; the cases and their inputs are unchanged.
// The record is docs/baseline/pekko-v2.0.0-migration/batch-3.md.
//
// Ported from Pekko v1.7.1:
//   actor-tests/src/test/scala/org/apache/pekko/actor/Ticket669Spec.scala

use core::time::Duration;

use fraktor_actor::kernel::actor::{Actor, ActorContext, Props};
use fraktor_actor::kernel::actor_ref::ActorRef;
use fraktor_actor::kernel::message::Message;
use fraktor_actor::kernel::supervision::{RestartWindow, SupervisorStrategy};
use fraktor_testkit::event_filter::{EventFilter, EventSinkRegistry};
use fraktor_testkit::fraktorspec::FraktorSpec;
use fraktor_testkit::implicit_sender::ImplicitSender;

const SPEC_SYSTEM_NAME: &str = "Ticket669Spec";

// The cause name the port records for a message-handling panic, which the
// original's `Exception("test")` becomes; the message is the panic text.
const CAUSE_NAME: &str = "RuntimeException";

const CAUSE_MESSAGE: &str = "test";

// Original: the `Supervisor` helper of `actor-tests/.../Supervisor.scala`, which
// answers a `Props` with a child it creates under its own strategy and keeps
// that child alive across its own restart.
struct SupervisorActor {
  strategy: SupervisorStrategy,
  created: u32,
}

impl Actor for SupervisorActor {
  // Original: `case x: Props => sender() ! context.actorOf(x)`
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    let Some(props) = message.downcast_ref::<Props>() else {
      return;
    };
    let name = format!("supervised-{}", self.created);
    self.created += 1;
    let child = match context.actor_of(props.clone(), &name) {
      | Ok(child) => child,
      | Err(error) => panic!("the supervisor could not start its child: {error}"),
    };
    context.sender().tell(child, context.self_ref());
  }

  // Original: `override val supervisorStrategy = <the strategy the test passes>`
  fn supervisor_strategy(&self) -> SupervisorStrategy {
    self.strategy.clone()
  }

  // Original: `override def preRestart(cause: Throwable, msg: Option[Any]): Unit
  // = {}` — the supervisor keeps its children around when it restarts.
  fn pre_restart(&mut self, _context: &mut ActorContext<'_>, _reason: &(dyn core::any::Any + Send)) {}
}

// Original: the `Supervised` actor of `Ticket669Spec` (object, `:63–77`). It
// throws for any message, replies "failure1" from `preRestart` and "failure2"
// from `postStop` to the message's original sender.
struct SupervisedActor;

impl Actor for SupervisedActor {
  // Original: `def receive = { case _ => throw new Exception("test") }`
  fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {
    panic!("{CAUSE_MESSAGE}");
  }

  // Original: `override def preRestart(reason, msg) = { sender() ! "failure1" }`
  //
  // The original's override does not call `super.preRestart`; the port keeps
  // that by not stopping the children here. `sender()` in the original is the
  // actor that sent the failing message; the port lends it as `context.sender()`.
  fn pre_restart(&mut self, context: &mut ActorContext<'_>, _reason: &(dyn core::any::Any + Send)) {
    context.sender().tell("failure1", context.self_ref());
  }

  // Original: `override def postStop() = { sender() ! "failure2" }`
  //
  // `post_stop` has no context in the port, so the stop hook that carries the
  // original sender is `post_stop_with_context`, which defaults to `post_stop`.
  // `sender()` there is the actor that sent the failing message.
  fn post_stop_with_context(&mut self, context: &mut ActorContext<'_>) {
    context.sender().tell("failure2", context.self_ref());
  }
}

fn supervisor_props(strategy: SupervisorStrategy) -> Props {
  Props::new(move || SupervisorActor { strategy: strategy.clone(), created: 0 })
}

fn supervised_props() -> Props {
  Props::new(|| SupervisedActor)
}

// Original: `Await.result((supervisor ? props).mapTo[ActorRef], timeout.duration)`
fn child_under(supervisor: &ActorRef, props: Props, timeout: Duration) -> ActorRef {
  let reply = supervisor.ask(props).await_result(timeout);
  reply.downcast_ref::<ActorRef>().cloned().expect("a child handle from the supervisor")
}

// Original: `supervised.!("test")(testActor)` then `expectMsg(<reply>)`, wrapped
// together with the `EventFilter[Exception]("test", occurrences = 1)` that
// surrounds the whole case in the original. The filter is armed before the
// supervisor is created so the failure log cannot escape it.
fn run_case(strategy: SupervisorStrategy, expected_reply: &'static str) {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  let sink = spec.system().event_sink().expect("the spec system has an event sink");
  let registry = sink.into_any().downcast::<EventSinkRegistry>().expect("the spec uses an EventSinkRegistry");
  let armed = registry.arm(EventFilter::typed(CAUSE_NAME).with_message(CAUSE_MESSAGE));
  {
    let sender = ImplicitSender::self_actor_ref(&spec);
    let timeout = spec.test_kit_settings().default_timeout().duration();
    let supervisor = spec.system_mut().actor_of(supervisor_props(strategy));
    let supervised = child_under(&supervisor, supervised_props(), timeout);

    supervised.tell("test", sender);
    spec.expect_msg(&expected_reply);

    spec.system_mut().stop(&supervisor);
    spec.system_mut().run_until_idle();
  }
  let matched = registry.matched(armed);
  registry.disarm(armed);
  assert_eq!(matched, 1, "expected exactly one {CAUSE_NAME}({CAUSE_MESSAGE}) event");
  spec.terminate();
}

// "A supervised actor with lifecycle PERMANENT" should {
//   "be able to reply on failure during preRestart" in { ... }
// }
#[test]
fn be_able_to_reply_on_failure_during_pre_restart() {
  // Original: `AllForOneStrategy(5, 10.seconds)(List(classOf[Exception]))`
  run_case(SupervisorStrategy::all_for_one(5, RestartWindow::Within(Duration::from_secs(10))), "failure1");
}

// "A supervised actor with lifecycle PERMANENT" should {
//   "be able to reply on failure during postStop" in { ... }
// }
#[test]
fn be_able_to_reply_on_failure_during_post_stop() {
  // Original: `AllForOneStrategy(maxNrOfRetries = 0)(List(classOf[Exception]))`.
  // The default `withinTimeRange` is `Duration.Inf`, so the port names it
  // explicitly as no window.
  run_case(SupervisorStrategy::all_for_one(0, RestartWindow::Unbounded), "failure2");
}
