// Ported from Pekko v1.7.1:
//   testkit/src/test/scala/org/apache/pekko/testkit/DefaultTimeoutSpec.scala
//
// Original test names are kept as comments next to each ported case.

use fraktor_actor::kernel::{actor_system::ActorSystem, timeout::Timeout};
use fraktor_testkit::{default_timeout::DefaultTimeout, pekkospec::PekkoSpec};

const CUSTOM_SPEC_SYSTEM_NAME: &str = "PekkoCustomSpec";

struct DefaultTimeoutSpecCase {
  pekko_spec: PekkoSpec,
}

impl DefaultTimeoutSpecCase {
  fn new() -> Self {
    Self { pekko_spec: PekkoSpec::new(CUSTOM_SPEC_SYSTEM_NAME) }
  }

  fn system(&self) -> &ActorSystem {
    self.pekko_spec.system()
  }
}

impl Drop for DefaultTimeoutSpecCase {
  fn drop(&mut self) {
    self.pekko_spec.terminate();
  }
}

// "A spec with DefaultTimeout" should { "use timeout from settings" in { ... } }
#[test]
fn use_timeout_from_settings() {
  let spec = DefaultTimeoutSpecCase::new();
  let system: &ActorSystem = spec.system();

  let timeout: Timeout = DefaultTimeout::timeout(system);
  let settings_default_timeout: Timeout = spec.pekko_spec.test_kit_settings().default_timeout();

  assert_eq!(timeout, settings_default_timeout);
}
