//! The startup and shutdown hooks of a spec.

use crate::test_kit::TestKit;

/// The points at which a spec may run code around the lifetime of its actor
/// system.
///
/// Mirrors the overridable methods of `org.apache.pekko.testkit.PekkoSpec`:
/// `atStartup` runs inside `beforeAll`, and `beforeTermination` and
/// `afterTermination` run inside `afterAll` around `shutdown`. Pekko inherits
/// and overrides them; the port takes the overrides as one object because a
/// Rust type cannot inherit from `FraktorSpec` and override its methods.
pub trait FraktorSpecHooks {
  /// Runs after the actor system and the test kit are ready.
  fn at_startup(&mut self, test_kit: &mut TestKit) {
    let _ = test_kit;
  }

  /// Runs while the actor system is still running, before it is shut down.
  fn before_termination(&mut self, test_kit: &mut TestKit) {
    let _ = test_kit;
  }

  /// Runs after the actor system has been shut down.
  fn after_termination(&mut self, test_kit: &mut TestKit) {
    let _ = test_kit;
  }
}
