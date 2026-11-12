// Ported from Pekko v1.7.1:
//   testkit/src/test/scala/org/apache/pekko/testkit/TestActorsSpec.scala
//
// Original test names are kept as comments next to each ported case.

use fraktor_actor::kernel::{actor_ref::ActorRef, actor_system::ActorSystem};
use fraktor_testkit::{
  implicit_sender::ImplicitSender,
  pekkospec::PekkoSpec,
  test_actors::{echo_actor_props, forward_actor_props},
};

const CUSTOM_SPEC_SYSTEM_NAME: &str = "PekkoCustomSpec";

struct TestActorsSpecCase {
  pekko_spec: PekkoSpec,
}

impl TestActorsSpecCase {
  fn new() -> Self {
    Self { pekko_spec: PekkoSpec::new(CUSTOM_SPEC_SYSTEM_NAME) }
  }

  fn system(&mut self) -> &mut ActorSystem {
    self.pekko_spec.system_mut()
  }

  fn test_actor(&self) -> ActorRef {
    self.pekko_spec.test_actor()
  }

  fn self_actor_ref(&self) -> ActorRef {
    ImplicitSender::self_actor_ref(&self.pekko_spec)
  }

  fn expect_msg(&mut self, expected: &str) {
    self.pekko_spec.expect_msg(expected);
  }
}

impl Drop for TestActorsSpecCase {
  fn drop(&mut self) {
    self.pekko_spec.terminate();
  }
}

// "A EchoActor" must { "send back messages unchanged" in { ... } }
#[test]
fn send_back_messages_unchanged() {
  let mut spec = TestActorsSpecCase::new();
  let message = "hello world";
  let echo: ActorRef = spec.system().actor_of(echo_actor_props());

  echo.tell(message, spec.self_actor_ref());

  spec.expect_msg(message);
}

// "A ForwardActor" must { "forward messages to target actor" in { ... } }
#[test]
fn forward_messages_to_target_actor() {
  let mut spec = TestActorsSpecCase::new();
  let message = "forward me";
  let target = spec.test_actor();
  let forward: ActorRef = spec.system().actor_of(forward_actor_props(target));

  forward.tell(message, spec.self_actor_ref());

  spec.expect_msg(message);
}
