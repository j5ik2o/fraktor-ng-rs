// Checked against Pekko v2.0.0 (SHA 9fbd2283b1cf701c90a1223bf621942c191db61f): the upstream test
// file is the same as in v1.7.1.
// The record is docs/baseline/pekko-v2.0.0-migration/batch-3.md.
//
// Ported from Pekko v1.7.1:
// actor-tests/src/test/scala/org/apache/pekko/actor/ActorTimeoutSpec.scala

use core::time::Duration;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use fraktor_actor::adaptor::{AskError, AskFuture};
use fraktor_actor::kernel::actor::{Actor, ActorContext, Props};
use fraktor_actor::kernel::actor_ref::ActorRef;
use fraktor_actor::kernel::message::Message;
use fraktor_actor::kernel::timeout::Timeout;
use fraktor_testkit::event_filter::{EventFilter, EventSinkRegistry};
use fraktor_testkit::fraktorspec::FraktorSpec;

// Props.empty does not reply. Record delivery without adding a reply path.
struct NonReplyingActor {
  received_hallo: Arc<AtomicBool>,
}

impl Actor for NonReplyingActor {
  fn receive(&mut self, _context: &mut ActorContext<'_>, message: Message) {
    if message.downcast_ref::<&str>() == Some(&"hallo") {
      self.received_hallo.store(true, Ordering::SeqCst);
    }
  }
}

struct TimeoutFixture {
  spec: FraktorSpec,
  default_timeout: Timeout,
  leeway: Duration,
  registry: Arc<EventSinkRegistry>,
  warning_filter: usize,
}

impl TimeoutFixture {
  fn new() -> Self {
    let spec = FraktorSpec::new("ActorTimeoutSpec");
    let settings = spec.test_kit_settings();
    let default_timeout = Timeout::new(settings.dilated(Duration::from_millis(200)));
    let leeway = settings.dilated(Duration::from_millis(500));
    let registry = spec
      .system()
      .event_sink()
      .expect("FraktorSpec installs an event sink")
      .into_any()
      .downcast::<EventSinkRegistry>()
      .expect("FraktorSpec installs EventSinkRegistry");
    // The original warning mute is configuration, not an event-count assertion.
    let warning_filter = registry.arm(EventFilter::warning());
    Self { spec, default_timeout, leeway, registry, warning_filter }
  }

  fn spawn_non_replying_actor(&mut self, received_hallo: Arc<AtomicBool>) -> ActorRef {
    self.spec.system_mut().actor_of(Props::new(move || NonReplyingActor { received_hallo: received_hallo.clone() }))
  }

  // Rust has no implicit arguments; this fixture owns the original implicit timeout.
  fn ask(&self, actor: &ActorRef, message: &'static str) -> AskFuture {
    actor.ask_with_timeout(message, self.default_timeout)
  }
}

impl Drop for TimeoutFixture {
  fn drop(&mut self) {
    self.spec.terminate();
    self.registry.disarm(self.warning_filter);
  }
}

// Original L34: "use implicitly supplied timeout" in
#[test]
fn use_implicitly_supplied_timeout() {
  let mut fixture = TimeoutFixture::new();
  let received_hallo = Arc::new(AtomicBool::new(false));
  let actor = fixture.spawn_non_replying_actor(received_hallo.clone());

  let started = Instant::now();
  let request = fixture.ask(&actor, "hallo");
  let result = request.try_await_result(fixture.default_timeout.duration() + fixture.leeway);
  let elapsed = started.elapsed();

  assert!(received_hallo.load(Ordering::SeqCst), "the actor must receive hallo without replying");
  assert!(matches!(&result, Err(AskError::AskTimedOut)), "expected the ask's own timeout, got {result:?}");
  assert!(elapsed >= fixture.default_timeout.duration(), "ask expired too early: {elapsed:?}");
}

// Original L41: "use explicitly supplied timeout" in
#[test]
fn use_explicitly_supplied_timeout() {
  let mut fixture = TimeoutFixture::new();
  let received_hallo = Arc::new(AtomicBool::new(false));
  let actor = fixture.spawn_non_replying_actor(received_hallo.clone());
  let explicit_timeout = Timeout::new(fixture.spec.test_kit_settings().dilated(Duration::from_millis(200)));

  let started = Instant::now();
  let request = actor.ask_with_timeout("hallo", explicit_timeout);
  let result = request.try_await_result(explicit_timeout.duration() + fixture.leeway);
  let elapsed = started.elapsed();

  assert!(received_hallo.load(Ordering::SeqCst), "the actor must receive hallo without replying");
  assert!(matches!(&result, Err(AskError::AskTimedOut)), "expected the ask's own timeout, got {result:?}");
  assert!(elapsed >= explicit_timeout.duration(), "ask expired too early: {elapsed:?}");
}
