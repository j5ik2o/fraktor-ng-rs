//! A test base that owns an actor system.

use std::sync::Arc;

use fraktor_actor::adaptor::ChannelBoundary;
use fraktor_actor::kernel::actor_ref::ActorRef;
use fraktor_actor::kernel::actor_system::ActorSystem;

use crate::pekkospec::pekko_spec_hooks::PekkoSpecHooks;
use crate::test_kit::TestKit;
use crate::test_kit_extension::TestKitSettings;

#[cfg(test)]
#[path = "pekko_spec_test.rs"]
mod tests;

/// Owns the actor system of a test and stops it when the test is done.
///
/// Mirrors `org.apache.pekko.testkit.PekkoSpec` for the parts that G0b needs.
/// Pekko extends `TestKit`, which owns the system and the test actor, and mixes
/// in `BeforeAndAfterAll`, whose `beforeAll` runs `atStartup` and whose
/// `afterAll` runs `beforeTermination`, `shutdown`, and `afterTermination`. The
/// port composes a `TestKit` instead of extending it, takes the hooks as an
/// object, and makes `terminate` play the role of `afterAll`. The parts of the
/// original that belong to later goals are not ported: the coroner,
/// `muteDeadLetters`, `spawn`, `log`, the configuration constructors,
/// `patience`, and the equality constraints.
pub struct PekkoSpec {
  test_kit: TestKit,
  hooks: Box<dyn PekkoSpecHooks>,
  terminated: bool,
}

impl PekkoSpec {
  /// Creates the actor system with the given name, its test kit, and the test
  /// actor, then runs `at_startup`.
  pub fn with_hooks(system_name: &str, hooks: Box<dyn PekkoSpecHooks>) -> Self {
    let system = ActorSystem::new(system_name, Arc::new(ChannelBoundary::new()));
    let mut spec = Self { test_kit: TestKit::new(system), hooks, terminated: false };
    spec.hooks.at_startup(&mut spec.test_kit);
    spec
  }

  /// Creates the spec with the given name and no startup or shutdown hooks.
  pub fn new(system_name: &str) -> Self {
    Self::with_hooks(system_name, Box::new(NoHooks))
  }

  /// Returns the actor system owned by this spec.
  pub fn system(&self) -> &ActorSystem {
    self.test_kit.system()
  }

  /// Returns the actor system owned by this spec, mutably.
  pub fn system_mut(&mut self) -> &mut ActorSystem {
    self.test_kit.system_mut()
  }

  /// Returns the test kit settings of the actor system.
  pub fn test_kit_settings(&self) -> TestKitSettings {
    self.test_kit.test_kit_settings()
  }

  /// Returns the handle of the test actor, corresponding to Pekko's `testActor`.
  pub fn test_actor(&self) -> ActorRef {
    self.test_kit.test_actor()
  }

  /// Waits for the next message of the test actor and checks it, corresponding
  /// to Pekko's `expectMsg`.
  pub fn expect_msg<E>(&mut self, expected: &E) -> fraktor_actor::kernel::message::Message
  where
    E: ?Sized + core::fmt::Debug,
    fraktor_actor::kernel::message::Message: PartialEq<E>,
  {
    self.test_kit.expect_msg(expected)
  }

  /// Stops the actor system, corresponding to Pekko's `afterAll`.
  ///
  /// Runs `before_termination`, then the shutdown of the test kit, then
  /// `after_termination`, in that order. A second call is a no-op, as `afterAll`
  /// runs once.
  pub fn terminate(&mut self) {
    if self.terminated {
      return;
    }
    self.hooks.before_termination(&mut self.test_kit);
    self.test_kit.shutdown();
    self.hooks.after_termination(&mut self.test_kit);
    self.terminated = true;
  }
}

struct NoHooks;

impl PekkoSpecHooks for NoHooks {}
