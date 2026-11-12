// Ported from Pekko v1.7.1:
//   testkit/src/test/scala/org/apache/pekko/testkit/ImplicitSenderSpec.scala
//
// Original test names are kept as comments next to each ported case.

use fraktor_actor::kernel::actor_ref::ActorRef;
use fraktor_testkit::{implicit_sender::ImplicitSender, pekkospec::PekkoSpec};

const CUSTOM_SPEC_SYSTEM_NAME: &str = "PekkoCustomSpec";

struct ImplicitSenderSpecCase {
  pekko_spec: PekkoSpec,
}

impl ImplicitSenderSpecCase {
  fn new() -> Self {
    Self { pekko_spec: PekkoSpec::new(CUSTOM_SPEC_SYSTEM_NAME) }
  }

  fn test_actor(&self) -> ActorRef {
    self.pekko_spec.test_actor()
  }

  fn self_actor_ref(&self) -> ActorRef {
    ImplicitSender::self_actor_ref(&self.pekko_spec)
  }
}

impl Drop for ImplicitSenderSpecCase {
  fn drop(&mut self) {
    self.pekko_spec.terminate();
  }
}

// "An ImplicitSender" should { "have testActor as its self" in { ... } }
#[test]
fn have_test_actor_as_its_self() {
  let spec = ImplicitSenderSpecCase::new();

  let self_actor_ref: ActorRef = spec.self_actor_ref();
  let test_actor: ActorRef = spec.test_actor();

  assert_eq!(self_actor_ref, test_actor);
}
