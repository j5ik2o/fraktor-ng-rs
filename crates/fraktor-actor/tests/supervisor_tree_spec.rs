// Ported from Pekko v1.7.1:
//   actor-tests/src/test/scala/org/apache/pekko/actor/SupervisorTreeSpec.scala

use core::time::Duration;

use fraktor_actor::kernel::actor::{Actor, ActorContext, Props};
use fraktor_actor::kernel::actor_path::ActorPath;
use fraktor_actor::kernel::actor_ref::ActorRef;
use fraktor_actor::kernel::message::Message;
use fraktor_actor::kernel::supervision::SupervisorStrategy;
use fraktor_testkit::event_filter::{EventFilter, EventSinkRegistry};
use fraktor_testkit::fraktorspec::FraktorSpec;
use fraktor_testkit::implicit_sender::ImplicitSender;

const SPEC_SYSTEM_NAME: &str = "SupervisorTreeSpec";

// Original: the anonymous `Actor` of the single case, reused for all three
// levels. `child_name` is the name the link gives the child it starts from a
// `Props` request.
struct TreeLinkActor {
  child_name: &'static str,
  test_actor: ActorRef,
}

impl Actor for TreeLinkActor {
  // Original: `def receive = { case p: Props => sender() ! context.actorOf(p) }`
  //
  // The original asks for a child with no name, so each child gets a generated
  // name. The port's `actor_of` takes a name, and the tree is linked once at
  // startup, so a fixed name per level keeps the paths stable.
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    let Some(props) = message.downcast_ref::<Props>() else {
      return;
    };
    let child = context.actor_of(props.clone(), self.child_name).expect("the supervisor can start its child");
    context.sender().tell(child, context.self_ref());
  }

  // Original: `override val supervisorStrategy = OneForOneStrategy(maxNrOfRetries
  // = 3, withinTimeRange = 1.second)(List(classOf[Exception]))`
  //
  // The original's one-second retry window cannot affect its single Kill.
  // Both strategies restart on that first failure under the three-retry limit.
  fn supervisor_strategy(&self) -> SupervisorStrategy {
    SupervisorStrategy::one_for_one(3)
  }

  // Original: `override def preRestart(cause: Throwable, msg: Option[Any]): Unit
  // = { testActor ! self.path }`
  //
  // The override does not call `super.preRestart`, so the default hook that
  // stops the children does not run and a surviving child stays alive across the
  // restart. The port keeps that by not calling `stop_children` here.
  fn pre_restart(&mut self, context: &mut ActorContext<'_>, _reason: &(dyn core::any::Any + Send)) {
    let path = context.self_ref().path();
    self.test_actor.tell(path, self.test_actor.clone());
  }
}

fn link_props(test_actor: &ActorRef, child_name: &'static str) -> Props {
  let test_actor = test_actor.clone();
  Props::new(move || TreeLinkActor { child_name, test_actor: test_actor.clone() })
}

// Original: `Await.result((supervisor ? props).mapTo[ActorRef], timeout.duration)`
fn ask_child(supervisor: &ActorRef, props: Props, timeout: Duration) -> ActorRef {
  let reply = supervisor.ask(props).await_result(timeout);
  reply.downcast_ref::<ActorRef>().cloned().expect("a child handle from the supervisor")
}

// Original: `expectMsg(<path>)`, read as the `ActorPath` the link reported.
fn expect_path(spec: &mut FraktorSpec, expected: &ActorPath) {
  let found: ActorPath = spec.expect_msg_type();
  assert_eq!(&found, expected, "expected {expected:?}, found {found:?}");
}

// "In a 3 levels deep supervisor tree (linked in the constructor) we" must {
//   "be able to kill the middle actor and see itself and its child restarted" in { ... }
// }
#[test]
fn be_able_to_kill_the_middle_actor_and_see_itself_and_its_child_restarted() {
  let mut spec = FraktorSpec::new(SPEC_SYSTEM_NAME);
  // Keep the filter armed across the whole timed case while TestKit borrows the spec.
  let sink = spec.system().event_sink().expect("the spec system has an event sink");
  let registry = sink.into_any().downcast::<EventSinkRegistry>().expect("the spec uses an EventSinkRegistry");
  let armed = registry.arm(EventFilter::typed("ActorKilledException"));
  spec.within(Duration::from_secs(5), |spec| {
    let test_actor = spec.test_actor();
    let sender = ImplicitSender::self_actor_ref(spec);
    let timeout = spec.test_kit_settings().default_timeout().duration();
    let head = spec.system_mut().actor_of(link_props(&test_actor, "middle"));
    let middle = ask_child(&head, link_props(&test_actor, "last"), timeout);
    let last = ask_child(&middle, link_props(&test_actor, "leaf"), timeout);

    middle.tell("Kill", sender);
    expect_path(spec, &middle.path());
    expect_path(spec, &last.path());
    spec.test_kit_mut().expect_no_message(Duration::from_secs(2));
    spec.system_mut().stop(&head);
    spec.system_mut().run_until_idle();
  });
  let matched = registry.matched(armed);
  registry.disarm(armed);
  assert_eq!(matched, 1, "expected exactly one ActorKilledException event");
  spec.terminate();
}
