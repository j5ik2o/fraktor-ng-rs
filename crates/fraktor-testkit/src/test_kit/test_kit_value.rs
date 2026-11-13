//! The test kit that owns an actor system, its test actor, and the expected
//! messages.

use core::any::Any;
use core::fmt::Debug;
use core::time::Duration;

use fraktor_actor::kernel::actor::Props;
use fraktor_actor::kernel::actor_ref::ActorRef;
use fraktor_actor::kernel::actor_system::ActorSystem;
use fraktor_actor::kernel::message::Message;

use crate::test_kit::expected_messages::ClassOf;
use crate::test_kit::AutoPilot;
use crate::test_kit::ReceivedMessage;
use crate::test_kit::TestKitBase;
use crate::test_kit_extension::TestKitSettings;

#[cfg(test)]
#[path = "test_kit_value_test.rs"]
mod tests;

/// Owns an actor system and the test actor whose messages a test inspects.
///
/// Mirrors `org.apache.pekko.testkit.TestKit`: it owns the system and the test
/// actor, and offers the `expectMsg...` family, which waits for the next
/// message of the test actor and compares it with the expected value. Pekko
/// runs the test actor on a dispatcher and blocks on a concurrent queue; the
/// port drives the system on the test thread and reads a queue guarded by a
/// mutex. That difference is invisible to a test: `expect_msg` still waits for
/// the dilated single-expect-default timeout and still fails on a timeout or on
/// a mismatch.
pub struct TestKit {
  system: ActorSystem,
  base: TestKitBase,
}

impl TestKit {
  /// Creates the test kit on top of `system` and creates the test actor in it.
  pub fn new(system: ActorSystem) -> Self {
    let settings = TestKitSettings::from_system(&system);
    let base = TestKitBase::new(system.downgrade(), settings, None);
    Self { system, base }
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
    self.base.test_actor()
  }

  /// Returns the last message the test actor handed to a waiting method.
  pub fn last_message(&self) -> Option<&ReceivedMessage> {
    self.base.last_message()
  }

  /// Installs the filter that drops matching messages before they are queued.
  pub fn ignore_msg(&self, filter: impl Fn(&Message) -> bool + Send + 'static) {
    self.base.ignore_msg(Box::new(filter));
  }

  /// Installs the pilot that runs for each message before it is queued.
  pub fn set_auto_pilot<P: AutoPilot + 'static>(&self, pilot: P) {
    self.base.set_auto_pilot(Box::new(pilot));
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
    let max = self.default_timeout();
    self.base.expect_msg_with(max, expected, None)
  }

  /// Waits at most `max` for the next message and returns it if it equals
  /// `expected`.
  pub fn expect_msg_within<E>(&mut self, max: Duration, expected: &E) -> Message
  where
    E: ?Sized + Debug,
    Message: PartialEq<E>,
  {
    let max = self.base.settings().dilated(max);
    self.base.expect_msg_with(max, expected, None)
  }

  /// Waits at most `max` for the next message, checks it, and names `hint` in
  /// the failure.
  pub fn expect_msg_hint<E>(&mut self, max: Duration, hint: &str, expected: &E) -> Message
  where
    E: ?Sized + Debug,
    Message: PartialEq<E>,
  {
    let max = self.base.settings().dilated(max);
    self.base.expect_msg_with(max, expected, Some(hint))
  }

  /// Waits for the next message and returns it when it carries a `T`.
  pub fn expect_msg_type<T: Any + Clone>(&mut self) -> T {
    let max = self.default_timeout();
    self.base.expect_msg_type(max)
  }

  /// Waits for the next message and returns it when it is of one of `classes`.
  pub fn expect_msg_any_class_of<T: Any + Clone>(&mut self, classes: &[ClassOf<T>]) -> T {
    let max = self.default_timeout();
    self.base.expect_msg_any_class_of(max, classes)
  }

  /// Waits for one message per entry in `classes` and returns them.
  pub fn expect_msg_all_class_of<T: Any + Clone>(&mut self, classes: &[ClassOf<T>]) -> Vec<T> {
    let max = self.default_timeout();
    self.base.expect_msg_all_class_of(max, classes)
  }

  /// Waits at most `max` for one message per entry in `classes`.
  pub fn expect_msg_all_class_of_within<T: Any + Clone>(&mut self, max: Duration, classes: &[ClassOf<T>]) -> Vec<T> {
    let max = self.base.settings().dilated(max);
    self.base.expect_msg_all_class_of(max, classes)
  }

  /// Waits for one message per entry in `classes` and returns them.
  pub fn expect_msg_all_conforming_of<T: Any + Clone>(&mut self, classes: &[ClassOf<T>]) -> Vec<T> {
    let max = self.default_timeout();
    self.base.expect_msg_all_class_of(max, classes)
  }

  /// Waits at most `max` for one message per entry in `classes`.
  pub fn expect_msg_all_conforming_of_within<T: Any + Clone>(
    &mut self,
    max: Duration,
    classes: &[ClassOf<T>],
  ) -> Vec<T> {
    let max = self.base.settings().dilated(max);
    self.base.expect_msg_all_class_of(max, classes)
  }

  /// Waits until `max` for no message at all.
  ///
  /// `max` is used as given; Pekko's `expectNoMessage(max)` does not dilate its
  /// argument either. The port has no argument-less `expectNoMessage` yet. For
  /// the dilated default, compute it from the public settings:
  /// `let settings = self.test_kit_settings(); settings.dilated(
  /// settings.expect_no_message_default_timeout())`.
  pub fn expect_no_message(&mut self, max: Duration) {
    self.base.expect_no_message(max);
  }

  /// Runs `assertion` until it stops failing, naming a failure when the
  /// deadline passes first.
  pub fn await_assert<T>(&mut self, max: Duration, interval: Duration, assertion: impl FnMut() -> T) -> T {
    let max = self.base.settings().dilated(max);
    self.base.await_assert(max, interval, assertion)
  }

  /// Runs `assertion` again and again until it stops failing or `max` passes.
  pub fn eventually(&mut self, max: Duration, interval: Duration, assertion: impl FnMut()) {
    let max = self.base.settings().dilated(max);
    self.base.eventually(max, interval, assertion)
  }

  /// Starts a child of the test actor with `name` and `strategy`.
  pub fn child_actor_of(
    &mut self,
    props: Props,
    name: Option<&str>,
    strategy: fraktor_actor::kernel::supervision::SupervisorStrategy,
  ) -> ActorRef {
    self.base.child_actor_of(props, name, strategy)
  }

  /// Watches `target` from the test actor.
  pub fn watch(&mut self, target: &ActorRef) {
    self.base.watch(target);
  }

  /// Stops the actor system, corresponding to Pekko's `shutdown()`.
  pub fn shutdown(&mut self) {
    self.system.terminate();
  }

  fn default_timeout(&self) -> Duration {
    let settings = self.test_kit_settings();
    settings.dilated(settings.single_expect_default_timeout())
  }
}
