// Checked against Pekko v2.0.0 (SHA 9fbd2283b1cf701c90a1223bf621942c191db61f): the upstream test
// file changes only by 9dfca01566 #3348, 870d4499cc #2235; the cases and their inputs are
// unchanged.
// The record is docs/baseline/pekko-v2.0.0-migration/batch-3.md.
//
// Ported from Pekko v1.7.1:
//   testkit/src/test/scala/org/apache/pekko/testkit/DefaultTimeoutSpec.scala
//
// Original test names are kept as comments next to each ported case.

use fraktor_actor::kernel::{actor_system::ActorSystem, timeout::Timeout};
use fraktor_testkit::{default_timeout::DefaultTimeout, fraktorspec::FraktorSpec};

const CUSTOM_SPEC_SYSTEM_NAME: &str = "PekkoCustomSpec";

struct DefaultTimeoutSpecCase {
  fraktor_spec: FraktorSpec,
}

impl DefaultTimeoutSpecCase {
  fn new() -> Self {
    Self { fraktor_spec: FraktorSpec::new(CUSTOM_SPEC_SYSTEM_NAME) }
  }

  fn system(&self) -> &ActorSystem {
    self.fraktor_spec.system()
  }
}

impl Drop for DefaultTimeoutSpecCase {
  fn drop(&mut self) {
    self.fraktor_spec.terminate();
  }
}

// "A spec with DefaultTimeout" should { "use timeout from settings" in { ... } }
#[test]
fn use_timeout_from_settings() {
  let spec = DefaultTimeoutSpecCase::new();
  let system: &ActorSystem = spec.system();

  let timeout: Timeout = DefaultTimeout::timeout(system);
  let settings_default_timeout: Timeout = spec.fraktor_spec.test_kit_settings().default_timeout();

  assert_eq!(timeout, settings_default_timeout);
}
