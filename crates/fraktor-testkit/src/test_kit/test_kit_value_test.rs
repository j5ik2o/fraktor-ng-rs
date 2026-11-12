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
