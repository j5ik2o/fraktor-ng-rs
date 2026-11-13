// Ported from Pekko v1.7.1:
//   testkit/src/test/scala/org/apache/pekko/testkit/ImplicitSenderSpec.scala
//
// Original test names are kept as comments next to each ported case.

use fraktor_actor::kernel::actor_ref::ActorRef;
use fraktor_testkit::{fraktorspec::FraktorSpec, implicit_sender::ImplicitSender};

const CUSTOM_SPEC_SYSTEM_NAME: &str = "PekkoCustomSpec";

struct ImplicitSenderSpecCase {
  fraktor_spec: FraktorSpec,
}

impl ImplicitSenderSpecCase {
  fn new() -> Self {
    Self { fraktor_spec: FraktorSpec::new(CUSTOM_SPEC_SYSTEM_NAME) }
  }

  fn test_actor(&self) -> ActorRef {
    self.fraktor_spec.test_actor()
  }

  fn self_actor_ref(&self) -> ActorRef {
    ImplicitSender::self_actor_ref(&self.fraktor_spec)
  }
}

impl Drop for ImplicitSenderSpecCase {
  fn drop(&mut self) {
    self.fraktor_spec.terminate();
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
