//! Unit tests for [`TestKit`](super::TestKit).

use std::panic::catch_unwind;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;
use std::time::Instant;

use fraktor_actor::adaptor::ChannelBoundary;
use fraktor_actor::kernel::actor::Actor;
use fraktor_actor::kernel::actor::ActorContext;
use fraktor_actor::kernel::actor::Props;
use fraktor_actor::kernel::actor_ref::ActorRef;
use fraktor_actor::kernel::actor_system::ActorSystem;
use fraktor_actor::kernel::executor::ShardBoundary;
use fraktor_actor::kernel::message::Envelope;
use fraktor_actor::kernel::message::Message;

use super::TestKit;

fn test_kit() -> TestKit {
  TestKit::new(ActorSystem::new("TestKitSpec", Arc::new(ChannelBoundary::new())))
}

struct SlowBoundary {
  inner: ChannelBoundary,
}

impl SlowBoundary {
  fn new() -> Self {
    Self { inner: ChannelBoundary::new() }
  }
}

impl ShardBoundary for SlowBoundary {
  fn post(&self, envelope: Envelope) {
    self.inner.post(envelope);
  }

  fn poll(&self) -> Option<Envelope> {
    None
  }

  fn poll_within(&self, max: Duration) -> Option<Envelope> {
    std::thread::sleep(max);
    self.inner.poll()
  }

  fn has_pending(&self) -> bool {
    self.inner.has_pending()
  }

  fn close(&self) {
    self.inner.close();
  }
}

struct GreetThenBurst {
  test_actor: ActorRef,
  burst_target: ActorRef,
}

impl Actor for GreetThenBurst {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    if message.downcast_ref::<&'static str>().is_some() {
      self.test_actor.tell("hello", context.self_ref());
      self.burst_target.forward(Message::new(1usize), context);
    }
  }
}

struct BurstOnly {
  remaining: Arc<Mutex<usize>>,
}

impl Actor for BurstOnly {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    let Some(next) = message.downcast_ref::<usize>().copied() else {
      return;
    };
    let mut remaining = self.remaining.lock().unwrap();
    if *remaining == 0 {
      return;
    }
    *remaining -= 1;
    context.self_ref().tell(next, context.self_ref());
  }
}

#[test]
fn expect_msg_returns_the_expected_message() {
  let mut kit = test_kit();
  let test_actor = kit.test_actor();
  test_actor.tell("hello", test_actor.clone());

  let received = kit.expect_msg("hello");

  assert_eq!(received, *"hello");
}

#[test]
#[should_panic(expected = "expected")]
fn expect_msg_fails_on_a_different_value() {
  let mut kit = test_kit();
  let test_actor = kit.test_actor();
  test_actor.tell("goodbye", test_actor.clone());

  kit.expect_msg("hello");
}

#[test]
#[should_panic(expected = "timeout")]
fn expect_msg_times_out_when_nothing_arrives() {
  let mut kit = test_kit();

  kit.expect_msg("hello");
}

#[test]
fn expect_msg_receives_a_message_that_arrives_at_the_deadline() {
  let mut kit = TestKit::new(ActorSystem::new("TestKitSpec", Arc::new(SlowBoundary::new())));
  let test_actor = kit.test_actor();
  test_actor.tell("hello", test_actor.clone());

  let received = kit.expect_msg("hello");

  assert_eq!(received, *"hello");
}

#[test]
fn expect_msg_returns_as_soon_as_the_expected_message_arrives() {
  let mut kit = test_kit();
  let test_actor = kit.test_actor();
  let remaining = Arc::new(Mutex::new(10_000usize));
  let burst_target = kit.system_mut().actor_of(Props::new({
    let remaining = Arc::clone(&remaining);
    move || BurstOnly { remaining: Arc::clone(&remaining) }
  }));
  let actor = kit.system_mut().actor_of(Props::new({
    let test_actor = test_actor.clone();
    let burst_target = burst_target.clone();
    move || GreetThenBurst { test_actor: test_actor.clone(), burst_target: burst_target.clone() }
  }));

  actor.tell("go", actor.clone());
  kit.expect_msg("hello");

  assert!(*remaining.lock().unwrap() > 0, "the whole burst ran before expect_msg returned");
}

#[test]
fn expect_msg_times_out_while_an_actor_keeps_sending() {
  let mut kit = test_kit();
  let remaining = Arc::new(Mutex::new(20_000_000usize));
  let actor = kit.system_mut().actor_of(Props::new({
    let remaining = Arc::clone(&remaining);
    move || BurstOnly { remaining: Arc::clone(&remaining) }
  }));
  actor.tell(1usize, actor.clone());
  let started = Instant::now();

  let outcome = catch_unwind(AssertUnwindSafe(|| kit.expect_msg("hello")));

  let elapsed = started.elapsed();
  assert!(outcome.is_err(), "expect_msg returned although the expected message never arrived");
  assert!(
    elapsed < core::time::Duration::from_secs(4),
    "expect_msg checked its deadline after {elapsed:?}, only once the burst had drained"
  );
}

struct EndlessSender;

impl Actor for EndlessSender {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    if message.downcast_ref::<usize>().is_some() {
      context.self_ref().tell(1usize, context.self_ref());
    }
  }
}

#[test]
fn await_assert_times_out_while_an_actor_keeps_sending() {
  let mut kit = test_kit();
  let actor = kit.system_mut().actor_of(Props::new(|| EndlessSender));
  actor.tell(1usize, actor.clone());
  let started = Instant::now();

  let outcome = catch_unwind(AssertUnwindSafe(|| {
    kit.await_assert(Duration::from_millis(200), Duration::from_millis(10), || {
      panic!("the assertion never holds");
    });
  }));

  let elapsed = started.elapsed();
  assert!(outcome.is_err(), "await_assert returned although the assertion never held");
  assert!(
    elapsed < Duration::from_secs(4),
    "await_assert checked its deadline after {elapsed:?}, only once the burst had drained"
  );
}

#[test]
fn expect_no_message_times_out_while_an_actor_keeps_sending() {
  let mut kit = test_kit();
  let actor = kit.system_mut().actor_of(Props::new(|| EndlessSender));
  actor.tell(1usize, actor.clone());
  let started = Instant::now();

  kit.expect_no_message(Duration::from_millis(200));

  let elapsed = started.elapsed();
  assert!(
    elapsed < Duration::from_secs(4),
    "expect_no_message checked its deadline after {elapsed:?}, only once the burst had drained"
  );
}

#[test]
fn eventually_retries_within_the_remaining_time_when_the_interval_is_longer() {
  let mut kit = test_kit();
  let started = Instant::now();
  let mut attempts = 0;

  kit.eventually(Duration::from_millis(200), Duration::from_secs(1), || {
    attempts += 1;
    assert!(started.elapsed() >= Duration::from_millis(50), "the assertion held before the message could arrive");
  });

  assert!(attempts >= 2, "eventually failed before the remaining time could be retried");
}

fn failure_text(failure: Box<dyn core::any::Any + Send>) -> String {
  match failure.downcast::<String>() {
    | Ok(text) => *text,
    | Err(failure) => failure.downcast::<&'static str>().map(|text| text.to_string()).unwrap_or_default(),
  }
}

#[test]
fn expect_no_message_with_zero_max_fails_on_a_queued_message_and_takes_it() {
  let mut kit = test_kit();
  let test_actor = kit.test_actor();
  test_actor.tell("hello", test_actor.clone());

  let failure = catch_unwind(AssertUnwindSafe(|| {
    kit.expect_no_message(Duration::ZERO);
  }))
  .expect_err("expect_no_message succeeded although a message was queued");

  assert!(failure_text(failure).contains("received unexpected message"));
  kit.expect_no_message(Duration::ZERO);
}

#[test]
fn expect_no_message_failure_keeps_last_message() {
  let mut kit = test_kit();
  let test_actor = kit.test_actor();
  test_actor.tell("a", test_actor.clone());
  kit.expect_msg("a");
  test_actor.tell("b", test_actor.clone());

  let outcome = catch_unwind(AssertUnwindSafe(|| {
    kit.expect_no_message(Duration::ZERO);
  }));
  assert!(outcome.is_err(), "expect_no_message succeeded although a message was queued");

  let last = kit.last_message().expect("expectMsg recorded a last message");
  assert_eq!(*last.message(), *"a");
}

#[test]
fn expect_msg_all_class_of_shares_one_deadline() {
  use crate::test_kit::expected_messages::ClassOf;

  let mut kit = test_kit();
  let test_actor = kit.test_actor();
  test_actor.tell(42_i32, test_actor.clone());
  let started = Instant::now();

  let outcome = catch_unwind(AssertUnwindSafe(|| {
    kit.expect_msg_all_class_of_within(Duration::from_millis(200), &[ClassOf::of::<i32>(), ClassOf::of::<i32>()])
  }));

  let elapsed = started.elapsed();
  let failure = outcome.expect_err("expect_msg_all_class_of succeeded although only one message arrived");
  let text = failure_text(failure);
  assert!(text.contains("while expecting 2 messages (got 1)"), "unexpected failure text: {text}");
  assert!(
    elapsed < Duration::from_millis(400),
    "expect_msg_all_class_of waited {elapsed:?}, which is longer than one deadline"
  );
}

#[test]
fn expect_msg_all_class_of_fails_on_a_message_of_another_type() {
  use crate::test_kit::expected_messages::ClassOf;

  let mut kit = test_kit();
  let test_actor = kit.test_actor();
  test_actor.tell(42_i32, test_actor.clone());
  test_actor.tell("x", test_actor.clone());

  let outcome = catch_unwind(AssertUnwindSafe(|| {
    kit.expect_msg_all_class_of_within(Duration::from_secs(1), &[ClassOf::of::<i32>(), ClassOf::of::<i32>()])
  }));

  let failure = outcome.expect_err("expect_msg_all_class_of succeeded although a message of another type arrived");
  let text = failure_text(failure);
  assert!(text.contains("found non-matching object(s)"), "unexpected failure text: {text}");
  assert!(text.contains("\"x\""), "unexpected failure text: {text}");
}

#[test]
fn expect_msg_all_conforming_of_returns_every_received_message_in_arrival_order() {
  use crate::test_kit::expected_messages::ClassOf;

  let mut kit = test_kit();
  let test_actor = kit.test_actor();
  test_actor.tell(1_i32, test_actor.clone());
  test_actor.tell(2_i32, test_actor.clone());

  let values =
    kit.expect_msg_all_conforming_of_within(Duration::from_secs(1), &[ClassOf::of::<i32>(), ClassOf::of::<i32>()]);

  assert_eq!(values, vec![1, 2]);
}
