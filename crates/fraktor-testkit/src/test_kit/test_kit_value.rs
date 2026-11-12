//! The test kit that owns an actor system, its test actor, and the expected
//! messages.

use core::fmt::Debug;
use core::time::Duration;

use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::Instant;

use fraktor_actor::kernel::actor::Props;
use fraktor_actor::kernel::actor_ref::ActorRef;
use fraktor_actor::kernel::actor_system::ActorSystem;
use fraktor_actor::kernel::message::Message;

use crate::test_kit::test_actor::TestActor;
use crate::test_kit_extension::TestKitSettings;

#[cfg(test)]
#[path = "test_kit_value_test.rs"]
mod tests;

/// Owns an actor system and the test actor whose messages a test inspects.
///
/// Mirrors `org.apache.pekko.testkit.TestKit` for what G0b needs: creating the
/// test actor inside the system, exposing the system and its settings, and
/// `expectMsg`, which waits for the next message of the test actor and compares
/// it with the expected value. Pekko runs the test actor on a dispatcher and
/// blocks on a concurrent queue; the port drives the system on the test thread
/// and reads a standard-library channel. That difference is invisible to a
/// test: `expect_msg` still waits for the dilated single-expect-default timeout
/// and still fails on a timeout or on a mismatch.
pub struct TestKit {
  system: ActorSystem,
  test_actor: ActorRef,
  queue: Receiver<Message>,
}

impl TestKit {
  /// Creates the test kit on top of `system` and creates the test actor in it.
  pub fn new(mut system: ActorSystem) -> Self {
    let (sender, queue) = mpsc::channel();
    let test_actor = system.actor_of(Props::new(move || TestActor::new(sender.clone())));
    Self { system, test_actor, queue }
  }

  /// Returns the actor system of the test.
  pub fn system(&self) -> &ActorSystem {
    &self.system
  }

  /// Returns the actor system of the test, mutably.
  pub fn system_mut(&mut self) -> &mut ActorSystem {
    &mut self.system
  }

  /// Returns the settings of the test.
  pub fn test_kit_settings(&self) -> TestKitSettings {
    TestKitSettings::from_system(&self.system)
  }

  /// Returns the handle of the test actor, corresponding to Pekko's
  /// `testActor`.
  pub fn test_actor(&self) -> ActorRef {
    self.test_actor.clone()
  }

  /// Waits for the next message of the test actor and returns it if it equals
  /// `expected`.
  ///
  /// Mirrors `org.apache.pekko.testkit.TestKitBase.expectMsg`: it waits at most
  /// the dilated single-expect-default timeout, fails if nothing arrives, and
  /// fails if the value differs.
  pub fn expect_msg<E>(&mut self, expected: &E) -> Message
  where
    E: ?Sized + Debug,
    Message: PartialEq<E>,
  {
    let settings = self.test_kit_settings();
    let max = settings.dilated(settings.single_expect_default_timeout());
    let deadline = Instant::now() + max;
    loop {
      match self.queue.try_recv() {
        | Ok(message) => {
          assert!(message == *expected, "expected {expected:?}, found {message:?}");
          return message;
        },
        | Err(TryRecvError::Empty) => {
          let remaining = deadline.saturating_duration_since(Instant::now());
          if remaining == Duration::ZERO {
            panic!("timeout ({max:?}) during expectMsg while waiting for {expected:?}");
          }
          if !self.system.run_one() {
            self.system.await_envelope(remaining);
          }
        },
        | Err(TryRecvError::Disconnected) => {
          panic!("the test actor stopped while expectMsg was waiting for {expected:?}");
        },
      }
    }
  }

  /// Stops the actor system, corresponding to Pekko's `shutdown()`.
  pub fn shutdown(&mut self) {
    self.system.terminate();
  }
}
