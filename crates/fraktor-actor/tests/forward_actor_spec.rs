// Ported from Pekko v1.7.1:
//   actor-tests/src/test/scala/org/apache/pekko/actor/ForwardActorSpec.scala
//
// Original test names are kept as comments next to each ported case.

use core::time::Duration;

use fraktor_actor::kernel::actor::{Actor, ActorContext, Props};
use fraktor_actor::kernel::actor_ref::ActorRef;
use fraktor_actor::kernel::message::Message;
use fraktor_testkit::pekkospec::PekkoSpec;

const EXPECTED_MESSAGE: &str = "FOO";

const CUSTOM_SPEC_SYSTEM_NAME: &str = "ForwardActorSpec";

fn five_seconds() -> Duration {
  Duration::from_secs(5)
}

// Original: object ForwardActorSpec.replier = Props(new Actor { def receive = { case x => sender() ! x } })
struct Replier;

impl Actor for Replier {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    context.sender().tell(message, context.self_ref());
  }
}

// Original: object ForwardActorSpec.mkforwarder(forwardTo) = Props(new Actor { def receive = { case x => forwardTo.forward(x) } })
struct Forwarder {
  forward_to: ActorRef,
}

impl Actor for Forwarder {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    self.forward_to.forward(message, context);
  }
}

// Original: createForwardingChain(system) = mkforwarder(mkforwarder(mkforwarder(replier)))
fn create_forwarding_chain(spec: &mut PekkoSpec) -> ActorRef {
  let replier = spec.system_mut().actor_of(Props::new(|| Replier));
  let inner = spec.system_mut().actor_of({
    let forward_to = replier.clone();
    Props::new(move || Forwarder { forward_to: forward_to.clone() })
  });
  let middle = spec.system_mut().actor_of({
    let forward_to = inner.clone();
    Props::new(move || Forwarder { forward_to: forward_to.clone() })
  });
  spec.system_mut().actor_of({
    let forward_to = middle.clone();
    Props::new(move || Forwarder { forward_to: forward_to.clone() })
  })
}

// Original: Props(new Actor { def receive = { case ExpectedMessage => testActor ! ExpectedMessage } })
struct ReplyToTestActor {
  test_actor: ActorRef,
}

impl Actor for ReplyToTestActor {
  fn receive(&mut self, _context: &mut ActorContext<'_>, message: Message) {
    if message == EXPECTED_MESSAGE {
      self.test_actor.tell(EXPECTED_MESSAGE, self.test_actor.clone());
    }
  }
}

// "A Forward Actor" must { "forward actor reference when invoking forward on tell" in { ... } }
#[test]
fn forward_actor_reference_when_invoking_forward_on_tell() {
  let mut spec = PekkoSpec::new(CUSTOM_SPEC_SYSTEM_NAME);

  let test_actor = spec.test_actor();
  let reply_to = spec.system_mut().actor_of(Props::new({
    let test_actor = test_actor.clone();
    move || ReplyToTestActor { test_actor: test_actor.clone() }
  }));

  let chain = create_forwarding_chain(&mut spec);

  chain.tell(EXPECTED_MESSAGE, reply_to);
  spec.expect_msg_within(five_seconds(), EXPECTED_MESSAGE);

  spec.terminate();
}

// "A Forward Actor" must { "forward actor reference when invoking forward on ask" in { ... } }
#[test]
fn forward_actor_reference_when_invoking_forward_on_ask() {
  let mut spec = PekkoSpec::new(CUSTOM_SPEC_SYSTEM_NAME);

  let test_actor = spec.test_actor();
  let chain = create_forwarding_chain(&mut spec);

  chain.ask(EXPECTED_MESSAGE).pipe_to(test_actor);
  spec.expect_msg_within(five_seconds(), EXPECTED_MESSAGE);

  spec.terminate();
}
