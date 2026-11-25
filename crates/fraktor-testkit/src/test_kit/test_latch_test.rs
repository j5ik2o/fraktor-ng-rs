//! Unit tests for [`TestLatch`](super::TestLatch).

use std::panic::catch_unwind;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use std::time::Instant;

use fraktor_actor::adaptor::ChannelBoundary;
use fraktor_actor::kernel::actor::Actor;
use fraktor_actor::kernel::actor::ActorContext;
use fraktor_actor::kernel::actor::Props;
use fraktor_actor::kernel::actor_system::ActorSystem;
use fraktor_actor::kernel::message::Message;

use super::TestLatch;
use crate::test_kit_extension::TestKitSettings;
use crate::test_kit_extension::TestTimeFactor;

fn system() -> ActorSystem {
  ActorSystem::new("TestLatchTest", Arc::new(ChannelBoundary::new()))
}

fn latch(count: usize, system: &ActorSystem) -> TestLatch {
  TestLatch::new(count, TestKitSettings::from_system(system))
}

fn failure_text(failure: Box<dyn core::any::Any + Send>) -> String {
  match failure.downcast::<String>() {
    | Ok(text) => *text,
    | Err(failure) => failure.downcast::<&'static str>().map(|text| text.to_string()).unwrap_or_default(),
  }
}

struct CountDownOnMessage {
  latch: TestLatch,
}

impl Actor for CountDownOnMessage {
  fn receive(&mut self, _context: &mut ActorContext<'_>, _message: Message) {
    self.latch.count_down();
  }
}

#[test]
fn a_new_latch_is_closed_until_it_is_counted_down_count_times() {
  let system = system();
  let latch = latch(2, &system);

  assert!(!latch.is_open());
  latch.count_down();
  assert!(!latch.is_open());
  latch.count_down();
  assert!(latch.is_open());
}

#[test]
fn counting_down_an_open_latch_keeps_it_open() {
  let system = system();
  let latch = latch(1, &system);

  latch.count_down();
  latch.count_down();

  assert!(latch.is_open());
}

#[test]
fn open_opens_the_latch_at_once() {
  let system = system();
  let latch = latch(3, &system);

  latch.open();

  assert!(latch.is_open());
}

#[test]
fn reset_closes_the_latch_with_the_original_count() {
  let system = system();
  let latch = latch(2, &system);
  latch.open();

  latch.reset();

  assert!(!latch.is_open());
  latch.count_down();
  assert!(!latch.is_open());
  latch.count_down();
  assert!(latch.is_open());
}

#[test]
fn a_clone_shares_the_count() {
  let system = system();
  let latch = latch(1, &system);
  let clone = latch.clone();

  clone.count_down();

  assert!(latch.is_open());
}

#[test]
fn ready_returns_at_once_when_the_latch_is_open() {
  let system = system();
  let latch = latch(1, &system);
  latch.count_down();
  let started = Instant::now();

  latch.ready(&system, Duration::from_secs(5));

  assert!(started.elapsed() < Duration::from_secs(1));
}

#[test]
fn ready_runs_the_system_so_an_actor_can_open_the_latch() {
  let mut system = system();
  let latch = latch(1, &system);
  let factory_latch = latch.clone();
  let actor = system.actor_of(Props::new(move || CountDownOnMessage { latch: factory_latch.clone() }));
  actor.tell("open", actor.clone());

  latch.ready(&system, Duration::from_secs(5));

  assert!(latch.is_open());
}

#[test]
fn ready_returns_when_another_thread_opens_the_latch() {
  let system = system();
  let latch = latch(1, &system);
  let other = latch.clone();
  let opener = thread::spawn(move || {
    thread::sleep(Duration::from_millis(50));
    other.count_down();
  });

  latch.ready(&system, Duration::from_secs(5));

  opener.join().unwrap();
  assert!(latch.is_open());
}

#[test]
fn ready_fails_with_the_max_and_the_time_factor_when_the_latch_stays_closed() {
  let system = system();
  let latch = latch(1, &system);

  let failure = catch_unwind(AssertUnwindSafe(|| latch.ready(&system, Duration::from_millis(50))))
    .expect_err("ready returned although the latch stayed closed");

  let text = failure_text(failure);
  assert!(text.contains("Timeout of 50ms with time factor of 1"), "failure: {text}");
}

#[test]
fn ready_waits_for_the_dilated_max() {
  let system = system();
  let factor = TestTimeFactor::new(3.0);
  let latch = TestLatch::new(1, TestKitSettings::with_time_factor(factor));
  let started = Instant::now();

  let failure = catch_unwind(AssertUnwindSafe(|| latch.ready(&system, Duration::from_millis(100))))
    .expect_err("ready returned although the latch stayed closed");

  assert!(started.elapsed() >= Duration::from_millis(300));
  assert!(failure_text(failure).contains("with time factor of 3"));
}

#[test]
fn ready_returns_on_a_terminated_system_when_the_latch_is_open() {
  let mut system = system();
  let latch = latch(1, &system);
  system.terminate();
  latch.count_down();

  latch.ready(&system, Duration::from_secs(2));

  assert!(latch.is_open());
}
