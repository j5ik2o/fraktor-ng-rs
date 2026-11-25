// Kernel tests for the unhandled notification of Issue #207.
//
// Original (Pekko v2.0.0, SHA 9fbd2283b1cf701c90a1223bf621942c191db61f):
//   * `Props.empty` and `EmptyActor` (actor/src/main/scala/org/apache/pekko/actor/Props.scala)
//   * `Actor.unhandled` and `UnhandledMessage`
//     (actor/src/main/scala/org/apache/pekko/actor/Actor.scala)
//   * the `UnhandledMessage` arm of `TestEventListener`
//     (testkit/src/main/scala/org/apache/pekko/testkit/TestEventListener.scala)
//   * `PekkoSpecSpec` "warn about unhandled messages"
//     (testkit/src/test/scala/org/apache/pekko/testkit/PekkoSpecSpec.scala)
//
// This file covers the kernel API that #210 needs for the PekkoSpecSpec case.
// It is not a port of a test class, so it carries no ledger node.

use core::any::Any;
use std::sync::Arc;
use std::sync::Mutex;

use fraktor_actor::adaptor::ChannelBoundary;
use fraktor_actor::kernel::actor::Logging::EventSink;
use fraktor_actor::kernel::actor::Logging::LogEvent;
use fraktor_actor::kernel::actor::Logging::LogLevel;
use fraktor_actor::kernel::actor::{Actor, ActorContext, Props, UnhandledMessage};
use fraktor_actor::kernel::actor_ref::ActorRef;
use fraktor_actor::kernel::actor_system::ActorSystem;
use fraktor_actor::kernel::event::EventClass;
use fraktor_actor::kernel::executor::ShardBoundary;
use fraktor_actor::kernel::message::Message;
use fraktor_testkit::fraktorspec::FraktorSpec;
use fraktor_testkit::test_kit::TestProbe;

/// A message type that the empty actor cannot know about.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Custom(u8);

/// An actor that reports every message it receives as unhandled.
///
/// Mirrors what `Props::empty` builds, but calls `unhandled` from a behaviour
/// of its own so the context entry is exercised directly.
struct Rejector;

impl Actor for Rejector {
  fn receive(&mut self, context: &mut ActorContext<'_>, message: Message) {
    context.unhandled(message);
  }
}

/// Records the warnings an executor publishes to the event sink.
///
/// The sink stands in for `TestEventListener`, which turns an `UnhandledMessage`
/// into a warning. The kernel publishes the same warning to the sink, so a test
/// can read the source and the text.
#[derive(Default)]
struct RecordingSink {
  warnings: Mutex<Vec<(String, String)>>,
}

impl EventSink for RecordingSink {
  fn publish(&self, event: &dyn LogEvent) {
    if event.level() == LogLevel::Warning {
      self.warnings.lock().unwrap().push((event.log_source().to_string(), event.message().to_string()));
    }
  }

  fn into_any(self: Arc<Self>) -> Arc<dyn Any + Send + Sync> {
    self
  }
}

/// Takes one `UnhandledMessage` from `probe` and checks its three fields.
fn assert_unhandled<T>(probe: &mut TestProbe, expected: &T, sender: &ActorRef, recipient: &ActorRef)
where
  T: Any + core::fmt::Debug + PartialEq,
{
  let unhandled: UnhandledMessage = probe.expect_msg_type();
  let message = unhandled.message().downcast_ref::<T>().expect("the report keeps the message type");
  assert_eq!(message, expected);
  assert_eq!(unhandled.sender(), sender);
  assert_eq!(unhandled.recipient(), recipient);
}

/// P1 and P4: every message that reaches an empty actor is reported once, with
/// its sender and recipient, and the actor keeps running for the next one.
#[test]
fn props_empty_reports_every_message_once_for_every_bounded_input() {
  let mut spec = FraktorSpec::new("UnhandledNotificationSpecP1");
  let mut subscriber = TestProbe::new(spec.system_mut());
  spec
    .system()
    .event_stream()
    .subscribe(Some(subscriber.ref_handle()), EventClass::of::<UnhandledMessage>())
    .expect("the probe is not null");
  let empty = spec.system_mut().actor_of(Props::empty());

  let dead_letters = spec.system().dead_letters();
  let test_actor = spec.test_actor();
  let other = TestProbe::new(spec.system_mut());
  let senders = [dead_letters, test_actor, other.ref_handle()];

  for (index, value) in [-2_i32, -1, 0, 1, 2].iter().enumerate() {
    let sender = &senders[index % senders.len()];
    empty.tell(*value, sender.clone());
    assert_unhandled(&mut subscriber, value, sender, &empty);
  }

  for text in ["alpha", "", "with spaces"] {
    let sender = &senders[0];
    empty.tell(text, sender.clone());
    assert_unhandled(&mut subscriber, &text, sender, &empty);
  }

  let custom = Custom(7);
  let sender = &senders[1];
  empty.tell(custom.clone(), sender.clone());
  assert_unhandled(&mut subscriber, &custom, sender, &empty);

  spec.terminate();
}

/// P2: an actor that calls `ActorContext::unhandled` reports the message with
/// the sender of the delivery and itself as the recipient.
#[test]
fn actor_context_unhandled_reports_the_message_and_sender() {
  let mut spec = FraktorSpec::new("UnhandledNotificationSpecP2");
  let mut subscriber = TestProbe::new(spec.system_mut());
  spec
    .system()
    .event_stream()
    .subscribe(Some(subscriber.ref_handle()), EventClass::of::<UnhandledMessage>())
    .expect("the probe is not null");
  let rejector = spec.system_mut().actor_of(Props::new(|| Rejector));
  let sender = spec.test_actor();

  rejector.tell(99_i32, sender.clone());

  assert_unhandled(&mut subscriber, &99, &sender, &rejector);
  spec.terminate();
}

/// P3: the event sink receives one warning per report, naming the recipient as
/// its source and the sender and message in its text.
#[test]
fn unhandled_is_reported_as_a_warning_to_the_event_sink() {
  let mut spec = FraktorSpec::new("UnhandledNotificationSpecP3");
  let recording = Arc::new(RecordingSink::default());
  let sink: Arc<dyn EventSink> = recording.clone();
  spec.system().set_event_sink(sink);
  let empty = spec.system_mut().actor_of(Props::empty());
  let sender = spec.system().dead_letters();
  let values = [-2_i32, 0, 2];

  for value in values {
    empty.tell(value, sender.clone());
  }
  spec.system_mut().run_until_idle();

  let warnings = recording.warnings.lock().unwrap();
  assert_eq!(warnings.len(), values.len());
  for (index, value) in values.iter().enumerate() {
    let (source, text) = &warnings[index];
    assert_eq!(source, &empty.path().to_string());
    assert_eq!(text, &format!("unhandled message from {}: {}", sender.path(), value));
  }

  spec.terminate();
}

/// I5: without a subscriber or an event sink the notification is dropped and
/// the actor still runs.
#[test]
fn unhandled_without_subscribers_or_a_sink_does_not_panic() {
  let boundary = Arc::new(ChannelBoundary::new());
  let mut system = ActorSystem::new("UnhandledNotificationSpecNoSink", Arc::clone(&boundary) as Arc<dyn ShardBoundary>);
  let empty = system.actor_of(Props::empty());

  empty.tell(42_i32, system.dead_letters());
  system.run_until_idle();

  system.terminate();
}
