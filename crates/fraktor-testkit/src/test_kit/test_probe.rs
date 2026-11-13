//! A probe that sends, receives, and replies on behalf of a test.

use core::any::Any;
use core::time::Duration;

use fraktor_actor::kernel::actor::Props;
use fraktor_actor::kernel::actor::Terminated;
use fraktor_actor::kernel::actor_ref::ActorRef;
use fraktor_actor::kernel::actor_system::ActorSystem;
use fraktor_actor::kernel::message::Message;
use fraktor_actor::kernel::supervision::SupervisorStrategy;

use crate::test_kit::expected_messages::ClassOf;
use crate::test_kit::AutoPilot;
use crate::test_kit::ReceivedMessage;
use crate::test_kit::TestKitBase;
use crate::test_kit_extension::TestKitSettings;

#[cfg(test)]
#[path = "test_probe_test.rs"]
mod tests;

/// The name a probe uses when it is given none.
const DEFAULT_PROBE_NAME: &str = "testProbe";

/// A test actor that a test uses directly, without owning the system.
///
/// Mirrors `org.apache.pekko.testkit.TestProbe`, which is a `TestKit` whose
/// test actor carries the probe's name. The probe reaches the system through a
/// weak handle, because the test owns the system and the probe must not keep
/// it alive; it creates children under its own test actor, watches actors, and
/// waits for messages with the same procedure as a test kit.
pub struct TestProbe {
  base: TestKitBase,
}

impl TestProbe {
  /// Creates a probe in `system` with the default name.
  pub fn new(system: &mut ActorSystem) -> Self {
    Self::with_name(system, DEFAULT_PROBE_NAME)
  }

  /// Creates a probe in `system` whose test actor is named `name`.
  ///
  /// The name is a prefix: a counter is appended, as Pekko does, so two probes
  /// can share a name.
  pub fn with_name(system: &mut ActorSystem, name: &str) -> Self {
    let settings = TestKitSettings::from_system(system);
    let base = TestKitBase::new(system.downgrade(), settings, Some(name));
    Self { base }
  }

  /// Returns the handle of the probe's test actor, corresponding to `ref`.
  pub fn ref_handle(&self) -> ActorRef {
    self.base.test_actor()
  }

  /// Sends `message` to `target` using the probe as the sender.
  pub fn send(&self, target: &ActorRef, message: impl Any + Send + Sync) {
    target.tell(message, self.base.test_actor());
  }

  /// Returns the last message the probe took from its queue.
  ///
  /// # Panics
  ///
  /// Panics when no message has been taken yet, as Pekko's `lastMessage`
  /// yields `NullMessage` and any use of it fails.
  pub fn last_message(&self) -> &ReceivedMessage {
    self.base.last_message().expect("no message has been received yet")
  }

  /// Installs the pilot that runs for each message before it is queued.
  pub fn set_auto_pilot<P: AutoPilot + 'static>(&self, pilot: P) {
    self.base.set_auto_pilot(Box::new(pilot));
  }

  /// Drops each message that `filter` accepts before it is queued.
  pub fn ignore_msg(&self, filter: impl Fn(&Message) -> bool + Send + 'static) {
    self.base.ignore_msg(Box::new(filter));
  }

  /// Starts a child of the probe's test actor with a generated name and the
  /// stopping strategy, corresponding to Pekko's `childActorOf(props)`.
  pub fn child_actor_of(&mut self, props: Props) -> ActorRef {
    self.base.child_actor_of(props, None, SupervisorStrategy::stopping_strategy())
  }

  /// Starts a child of the probe's test actor with `name` and the stopping
  /// strategy.
  pub fn child_actor_of_named(&mut self, props: Props, name: &str) -> ActorRef {
    self.base.child_actor_of(props, Some(name), SupervisorStrategy::stopping_strategy())
  }

  /// Starts a child of the probe's test actor with `strategy`.
  ///
  /// Mirrors `TestKitBase.childActorOf(props, supervisorStrategy)`.
  pub fn child_actor_of_with_supervisor(&mut self, props: Props, strategy: SupervisorStrategy) -> ActorRef {
    self.base.child_actor_of(props, None, strategy)
  }

  /// Watches `target` from the probe's test actor.
  pub fn watch(&mut self, target: &ActorRef) {
    self.base.watch(target);
  }

  /// Waits for the next message and checks it.
  pub fn expect_msg<E>(&mut self, expected: &E) -> Message
  where
    E: ?Sized + core::fmt::Debug,
    Message: PartialEq<E>,
  {
    let max = self.default_timeout();
    self.base.expect_msg_with(max, expected, None)
  }

  /// Waits at most `max` for the next message and checks it.
  pub fn expect_msg_within<E>(&mut self, max: Duration, expected: &E) -> Message
  where
    E: ?Sized + core::fmt::Debug,
    Message: PartialEq<E>,
  {
    let max = self.base.settings().dilated(max);
    self.base.expect_msg_with(max, expected, None)
  }

  /// Waits at most `max` for the next message, checks it, and names `hint`
  /// when it fails.
  pub fn expect_msg_hint<E>(&mut self, max: Duration, hint: &str, expected: &E) -> Message
  where
    E: ?Sized + core::fmt::Debug,
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

  /// Waits at most `max` for the next message and returns it when it is a
  /// `Terminated` message.
  pub fn expect_msg_terminated_within(&mut self, max: Duration) -> Terminated {
    let max = self.base.settings().dilated(max);
    let Some(received) = self.base.receive_one(max) else {
      panic!("timeout ({max:?}) during expectMsg waiting for Terminated");
    };
    let found = received.message();
    match found.downcast_ref::<Terminated>() {
      | Some(terminated) => terminated.clone(),
      | None => panic!("expected Terminated, found {found:?}"),
    }
  }

  /// Waits until `max` for no message at all.
  ///
  /// `max` is used as given; Pekko's `expectNoMessage(max)` does not dilate its
  /// argument either. The port has no argument-less `expectNoMessage` yet. For
  /// the dilated default, compute it from the public settings:
  /// `let settings = TestKitSettings::from_system(system); settings.dilated(
  /// settings.expect_no_message_default_timeout())`.
  pub fn expect_no_message(&mut self, max: Duration) {
    self.base.expect_no_message(max);
  }

  /// Waits until `predicate` accepts a message, dropping the ones before it.
  ///
  /// # Panics
  ///
  /// Panics when no message is accepted before `max` passes.
  pub fn fish_for_message(&mut self, max: Duration, predicate: impl Fn(&Message) -> bool) -> Message {
    let max = self.base.settings().dilated(max);
    self.base.fish_for_message(max, predicate)
  }

  /// Waits until `pick` accepts a message, returning what it picks.
  ///
  /// # Panics
  ///
  /// Panics when no message is picked before `max` passes.
  pub fn fish_for_specific_message<T>(&mut self, max: Duration, pick: impl Fn(&Message) -> Option<T>) -> T {
    let max = self.base.settings().dilated(max);
    self.base.fish_for_specific_message(max, pick)
  }

  fn default_timeout(&self) -> Duration {
    let settings = self.base.settings();
    settings.dilated(settings.single_expect_default_timeout())
  }
}
