//! The waiting and matching that a test kit and a test probe share.

use core::any::Any;
use core::fmt::Debug;
use core::time::Duration;

use std::panic::AssertUnwindSafe;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Instant;

use fraktor_actor::kernel::actor::Props;
use fraktor_actor::kernel::actor_ref::ActorRef;
use fraktor_actor::kernel::actor_system::WeakActorSystem;
use fraktor_actor::kernel::message::Message;
use fraktor_actor::kernel::supervision::SupervisorStrategy;

use crate::test_kit::expected_messages::ClassOf;
use crate::test_kit::AutoPilot;
use crate::test_kit::ReceivedMessage;
use crate::test_kit::TestActor;
use crate::test_kit_extension::TestKitSettings;

/// The name a test actor gets when its owner does not choose one.
const DEFAULT_TEST_ACTOR_NAME: &str = "testActor";

/// The interval that names one test actor apart from the next.
static TEST_ACTOR_ID: AtomicUsize = AtomicUsize::new(0);

/// The interval at which a waiting test checks for a message while it waits on
/// a boundary that does not signal arrivals by itself.
const POLL_INTERVAL: Duration = Duration::from_millis(10);

/// The part of a test kit that receives messages and checks them.
///
/// Mirrors `org.apache.pekko.testkit.TestKitBase`. Pekko keeps a blocking
/// queue and a test actor that runs on the calling-thread dispatcher; the port
/// keeps the same queue and the same observation order, and drives the actor
/// system on the test thread instead of blocking on a dispatcher.
///
/// Every waiting method goes through [`TestKitBase::receive_one`], so the
/// deadline is checked between messages rather than after a burst. An actor
/// that keeps sending therefore cannot push a wait past its deadline.
pub(crate) struct TestKitBase {
  system: WeakActorSystem,
  test_actor: ActorRef,
  actor: Arc<TestActor>,
  last_message: Option<ReceivedMessage>,
  settings: TestKitSettings,
}

impl TestKitBase {
  /// Creates the test actor named `name`, or the default name, in `system`.
  pub(crate) fn new(system: WeakActorSystem, settings: TestKitSettings, name: Option<&str>) -> Self {
    let actor = Arc::new(TestActor::new());
    let handler_actor = Arc::clone(&actor);
    let label = match name {
      | Some(name) => format!("{name}-{}", TEST_ACTOR_ID.fetch_add(1, Ordering::Relaxed)),
      | None => format!("{DEFAULT_TEST_ACTOR_NAME}-{}", TEST_ACTOR_ID.fetch_add(1, Ordering::Relaxed)),
    };
    let test_actor = spawn_function_ref(&system, &label, handler_actor);
    Self { system, test_actor, actor, last_message: None, settings }
  }

  /// Returns the handle of the test actor.
  pub(crate) fn test_actor(&self) -> ActorRef {
    self.test_actor.clone()
  }

  /// Returns the settings of the test kit.
  pub(crate) fn settings(&self) -> TestKitSettings {
    self.settings
  }

  /// Returns the last message taken from the queue.
  pub(crate) fn last_message(&self) -> Option<&ReceivedMessage> {
    self.last_message.as_ref()
  }

  /// Installs the filter that drops matching messages before they are queued.
  pub(crate) fn ignore_msg(&self, filter: Box<dyn Fn(&Message) -> bool + Send>) {
    self.actor.set_ignore(filter);
  }

  /// Installs the pilot that runs for each message before it is queued.
  pub(crate) fn set_auto_pilot(&self, pilot: Box<dyn AutoPilot>) {
    self.actor.set_auto_pilot(pilot);
  }

  /// Waits for the next message and returns it when it equals `expected`.
  ///
  /// Mirrors `TestKitBase.expectMsg_internal`: a timeout produces a failure
  /// that names the deadline, and a different value produces a failure that
  /// names both values. Both carry the hint when one is given.
  pub(crate) fn expect_msg_with<E>(&mut self, max: Duration, expected: &E, hint: Option<&str>) -> Message
  where
    E: ?Sized + Debug,
    Message: PartialEq<E>,
  {
    let hint = hint.map(|hint| format!(": {hint}")).unwrap_or_default();
    let Some(received) = self.receive_one(max) else {
      panic!("timeout ({max:?}) during expectMsg while waiting for {expected:?}{hint}");
    };
    let found = received.message();
    assert!(*found == *expected, "expected {expected:?}, found {found:?}{hint}");
    found.clone()
  }

  /// Waits for the next message and returns it when it carries a `T`.
  pub(crate) fn expect_msg_type<T: Any + Clone>(&mut self, max: Duration) -> T {
    let Some(received) = self.receive_one(max) else {
      panic!("timeout ({max:?}) during expectMsgType waiting for a value of the expected type");
    };
    let found = received.message();
    match found.downcast_ref::<T>() {
      | Some(value) => value.clone(),
      | None => panic!("expected a value of the expected type, found {found:?}"),
    }
  }

  /// Waits for the next message and returns it when it is of one of `classes`.
  pub(crate) fn expect_msg_any_class_of<T: Any + Clone>(&mut self, max: Duration, classes: &[ClassOf<T>]) -> T {
    let Some(received) = self.receive_one(max) else {
      panic!("timeout ({max:?}) during expectMsgAnyClassOf waiting for {classes:?}");
    };
    let found = received.message();
    match found.downcast_ref::<T>() {
      | Some(value) if classes.iter().any(|class| class.matches(found)) => value.clone(),
      | _ => panic!("found unexpected {found:?} while waiting for {classes:?}"),
    }
  }

  /// Receives `n` messages before a single shared deadline.
  ///
  /// Mirrors `TestKitBase.receiveN_internal`: it computes `stop` once and
  /// hands each `receiveOne` only the time that is left, so the whole call
  /// costs at most `max` rather than `n * max`.
  pub(crate) fn receive_n(&mut self, n: usize, max: Duration) -> Vec<ReceivedMessage> {
    let stop = Instant::now() + max;
    let mut received = Vec::with_capacity(n);
    for got in 0..n {
      let remaining = stop.saturating_duration_since(Instant::now());
      let Some(message) = self.receive_one(remaining) else {
        panic!("timeout ({max:?}) while expecting {n} messages (got {got})");
      };
      received.push(message);
    }
    received
  }

  /// Waits for one message per entry in `classes` and returns them.
  ///
  /// Mirrors `internalExpectMsgAllClassOf` / `internalExpectMsgAllConformingOf`:
  /// it takes every message under one shared deadline first, then fails when a
  /// `class` matches no received message (missing) or a received message
  /// matches no `class` (unexpected), as `checkMissingAndUnexpected` does. It
  /// returns the received messages in arrival order. Pekko separates "equal
  /// class" from "conforming class"; Rust has no subtyping between the values a
  /// message carries, so both checks reduce to "the message has the tagged
  /// type".
  pub(crate) fn expect_msg_all_class_of<T: Any + Clone>(&mut self, max: Duration, classes: &[ClassOf<T>]) -> Vec<T> {
    let received = self.receive_n(classes.len(), max);
    let missing: Vec<&ClassOf<T>> =
      classes.iter().filter(|class| !received.iter().any(|message| class.matches(message.message()))).collect();
    let mut values = Vec::with_capacity(received.len());
    let mut unexpected = Vec::new();
    for message in &received {
      let found = message.message();
      match found.downcast_ref::<T>() {
        | Some(value) if classes.iter().any(|class| class.matches(found)) => values.push(value.clone()),
        | _ => unexpected.push(found),
      }
    }
    let missing_part = if missing.is_empty() { String::new() } else { format!("not found {missing:?} ") };
    let unexpected_part =
      if unexpected.is_empty() { String::new() } else { format!("found non-matching object(s) {unexpected:?}") };
    assert!(missing.is_empty() && unexpected.is_empty(), "{missing_part}{unexpected_part}");
    values
  }

  /// Waits until `max` for no message at all.
  ///
  /// Mirrors `TestKitBase.expectNoMsg_internal`. The queue is checked before
  /// the deadline, so a message already waiting fails the call even when `max`
  /// is zero, as `queue.peekFirst` does before Pekko enters its polling loop.
  /// The message that fails the call is taken off the queue, so the next call
  /// checks the following period, but `lastMessage` is left untouched because
  /// Pekko only calls `queue.pop()` here and never `receiveOne`.
  ///
  /// `max` is used as given. Unlike `expectMsg`, Pekko does not dilate the
  /// value of `expectNoMessage`; its own documentation says "Supplied value is
  /// not dilated". This port has no argument-less `expectNoMessage` yet, so a
  /// test that wants the dilated default computes it from the settings:
  /// `let settings = TestKitSettings::from_system(system); settings.dilated(
  /// settings.expect_no_message_default_timeout())`.
  pub(crate) fn expect_no_message(&mut self, max: Duration) {
    let start = Instant::now();
    let finish = start + max;
    loop {
      if let Some(received) = self.actor.take_next() {
        let elapsed = start.elapsed();
        panic!(
          "assertion failed: received unexpected message {:?} after {} millis",
          received.message(),
          elapsed.as_millis()
        );
      }
      let left = finish.saturating_duration_since(Instant::now());
      if left == Duration::ZERO {
        return;
      }
      if !self.system.run_one() {
        self.await_one(left.min(POLL_INTERVAL));
      }
    }
  }

  /// Waits until `predicate` accepts a message, dropping the ones before it.
  ///
  /// Mirrors `TestKitBase.fishForMessage`: it shares one deadline across the
  /// whole call and hands the remaining time to `receiveOne` for each message,
  /// so every discarded message also updates `lastMessage`, and the call fails
  /// when the deadline passes before `predicate` accepts a message.
  ///
  /// Each iteration measures the remaining time again, as `fishForMessage`
  /// does with `receiveOne(end - now)`. `receiveOne` still takes a message that
  /// is already queued even when the remaining time is zero, which is what
  /// Pekko's `pollFirst` does, so messages that are already waiting are never
  /// abandoned at the deadline.
  pub(crate) fn fish_for_message(&mut self, max: Duration, predicate: impl Fn(&Message) -> bool) -> Message {
    let end = Instant::now() + max;
    loop {
      let remaining = end.saturating_duration_since(Instant::now());
      let Some(received) = self.receive_one(remaining) else {
        panic!("timeout ({max:?}) during fishForMessage");
      };
      if predicate(received.message()) {
        return received.message().clone();
      }
    }
  }

  /// Waits until `pick` accepts a message, returning what it picks.
  ///
  /// Mirrors `TestKitBase.fishForSpecificMessage`: like `fish_for_message`, it
  /// shares one deadline and updates `lastMessage` for each message it
  /// receives, and fails when the deadline passes before `pick` accepts one.
  /// The remaining time is measured again on each iteration, and a message
  /// that is already queued is still taken at the deadline.
  pub(crate) fn fish_for_specific_message<T>(&mut self, max: Duration, pick: impl Fn(&Message) -> Option<T>) -> T {
    let end = Instant::now() + max;
    loop {
      let remaining = end.saturating_duration_since(Instant::now());
      let Some(received) = self.receive_one(remaining) else {
        panic!("timeout ({max:?}) during fishForSpecificMessage");
      };
      if let Some(value) = pick(received.message()) {
        return value;
      }
    }
  }

  /// Runs `assertion` until it stops failing or `max` has passed.
  ///
  /// Mirrors `TestKitBase.awaitAssert`: the last failure is thrown again when
  /// the deadline arrives just after a failed attempt, and the successful
  /// value is returned otherwise. Messages that arrive between attempts are
  /// delivered, so a probe can observe what the driven actors send.
  pub(crate) fn await_assert<T>(&mut self, max: Duration, interval: Duration, mut assertion: impl FnMut() -> T) -> T {
    let stop = Instant::now() + max;
    loop {
      let attempt = AssertUnwindSafe(&mut assertion);
      match std::panic::catch_unwind(attempt) {
        | Ok(value) => return value,
        | Err(failure) => {
          self.drive_for(POLL_INTERVAL);
          if Instant::now() + interval >= stop {
            std::panic::resume_unwind(failure);
          }
          let remaining = stop.saturating_duration_since(Instant::now());
          self.drive_for(remaining.min(interval));
        },
      }
    }
  }

  /// Runs `assertion` again and again until it stops failing or `max` has
  /// passed, mirroring ScalaTest's `Eventually`.
  ///
  /// The wait before the next attempt is `min(interval, remaining time)`, the
  /// same shape as `awaitCond`: a long interval cannot use up the whole budget
  /// after the first failure, and the last attempt still happens before the
  /// deadline.
  pub(crate) fn eventually(&mut self, max: Duration, interval: Duration, mut assertion: impl FnMut()) {
    let stop = Instant::now() + max;
    loop {
      let attempt = AssertUnwindSafe(&mut assertion);
      match std::panic::catch_unwind(attempt) {
        | Ok(()) => return,
        | Err(failure) => {
          let remaining = stop.saturating_duration_since(Instant::now());
          if remaining == Duration::ZERO {
            std::panic::resume_unwind(failure);
          }
          self.drive_for(remaining.min(interval));
        },
      }
    }
  }

  /// Delivers messages of the system for `max`, stopping at the deadline.
  pub(crate) fn drive_for(&mut self, max: Duration) {
    let finish = Instant::now() + max;
    loop {
      if !self.system.run_one() {
        let left = finish.saturating_duration_since(Instant::now());
        if left == Duration::ZERO {
          return;
        }
        self.await_one(left.min(POLL_INTERVAL));
      }
      if Instant::now() >= finish {
        return;
      }
    }
  }

  /// Waits at most `max` for a message at the boundary and delivers one.
  fn await_one(&self, max: Duration) {
    if self.system.await_envelope(max) {
      let _ = self.system.run_one();
    }
  }

  /// Takes at most one message from the test actor, delivering messages of
  /// the system until one arrives or `max` passes.
  ///
  /// This is the one place that decides how a wait proceeds, mirroring
  /// `TestKitBase.receiveOne`:
  ///
  /// 1. Take a queued message if there is one.
  /// 2. If the deadline has passed, report that nothing arrived.
  /// 3. Otherwise deliver one message of the system, so an actor that runs on
  ///    the test thread queues its reply within the same call, as it would on
  ///    the calling-thread dispatcher.
  /// 4. When nothing was ready, wait a short interval at the boundary and
  ///    deliver one message, then try again.
  ///
  /// Because the deadline is checked between single deliveries, a wait ends on
  /// time even while another actor keeps sending.
  pub(crate) fn receive_one(&mut self, max: Duration) -> Option<ReceivedMessage> {
    let deadline = Instant::now() + max;
    loop {
      if let Some(received) = self.actor.take_next() {
        self.last_message = Some(received.clone());
        return Some(received);
      }
      let remaining = deadline.saturating_duration_since(Instant::now());
      if remaining == Duration::ZERO {
        self.last_message = None;
        return None;
      }
      if self.system.run_one() {
        continue;
      }
      self.await_one(remaining.min(POLL_INTERVAL));
    }
  }

  /// Starts a child of the test actor.
  pub(crate) fn child_actor_of(&mut self, props: Props, name: Option<&str>, strategy: SupervisorStrategy) -> ActorRef {
    let parent = self.test_actor();
    self.system.spawn_child(&parent, props, name, strategy)
  }

  /// Watches `target` from the test actor.
  pub(crate) fn watch(&mut self, target: &ActorRef) {
    let watcher = self.test_actor();
    self.system.watch(&watcher, target);
  }
}

/// Creates a function reference in `system` whose messages run `actor`.
fn spawn_function_ref(system: &WeakActorSystem, name: &str, actor: Arc<TestActor>) -> ActorRef {
  system.function_ref(name, move |sender: &ActorRef, message: Message| {
    actor.receive(sender, message);
  })
}
