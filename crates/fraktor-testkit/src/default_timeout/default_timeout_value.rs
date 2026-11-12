//! The implicit timeout a test gets from its settings.

use fraktor_actor::kernel::actor_system::ActorSystem;
use fraktor_actor::kernel::timeout::Timeout;

use crate::test_kit_extension::TestKitSettings;

/// Provides the timeout that a test uses when none is given explicitly.
///
/// Mirrors `org.apache.pekko.testkit.DefaultTimeout`, whose
/// `implicit val timeout` is `testKitSettings.DefaultTimeout`. The Rust port
/// cannot mix a trait into a test type implicitly, so the value is produced by
/// a function that takes the actor system of the test.
pub struct DefaultTimeout;

impl DefaultTimeout {
  /// Returns the `DefaultTimeout` of the settings of the given system.
  pub fn timeout(system: &ActorSystem) -> Timeout {
    TestKitSettings::from_system(system).default_timeout()
  }
}
