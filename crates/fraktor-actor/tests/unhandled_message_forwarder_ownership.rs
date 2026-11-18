// Reproduction for issue #71:
//   the unhandled-message forwarder keeps the subscription table alive.
//
// `EventStream::start_unhandled_message_forwarder` builds a function reference
// whose handler captures `self.bus.clone()` by strong reference, then
// subscribes that reference to the same bus. The bus retains the forwarder's
// handle, the handle's boundary retains the handler, and the handler retains
// the bus. Dropping the system in that configuration leaves the cycle alive:
// the subscription table and every subscriber it holds never run their `Drop`.
//
// The forwarder must hold the bus through a `Weak` and skip the publish when
// the upgrade fails. The cases below observe the absence of the cycle with a
// value that records its own `Drop`, and they observe that the forwarder still
// turns an unhandled message into a `Debug`.

use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::sync::Mutex;

use fraktor_actor::adaptor::ChannelBoundary;
use fraktor_actor::kernel::actor::Logging::{Debug, LogEvent};
use fraktor_actor::kernel::actor::UnhandledMessage;
use fraktor_actor::kernel::actor_ref::ActorRef;
use fraktor_actor::kernel::actor_system::ActorSystem;
use fraktor_actor::kernel::event::EventClass;
use fraktor_actor::kernel::executor::ShardBoundary;
use fraktor_actor::kernel::message::Message;

/// Records that it was dropped through a shared flag.
///
/// The flag is an `Arc<AtomicBool>` because a subscription table entry and a
/// function handler must both be `Send + Sync`, and the test reads the flag
/// after the value that owns it is gone.
struct DropFlag {
  dropped: Arc<AtomicBool>,
}

impl DropFlag {
  fn new() -> (Self, Arc<AtomicBool>) {
    let dropped = Arc::new(AtomicBool::new(false));
    (Self { dropped: Arc::clone(&dropped) }, dropped)
  }
}

impl Drop for DropFlag {
  fn drop(&mut self) {
    self.dropped.store(true, Ordering::SeqCst);
  }
}

/// Builds a system on a boundary that never runs actors.
///
/// The subscription-table case only needs the system and the stream, not a
/// dispatcher, so a channel boundary keeps the system independent of the test
/// kit, whose handle owns the system and would keep it alive past the drop.
fn standalone_system(name: &str) -> ActorSystem {
  let boundary = Arc::new(ChannelBoundary::new());
  ActorSystem::new(name, boundary as Arc<dyn ShardBoundary>)
}

// Scenario: the handle counts are the ones the goal names, so the cycle is
// observable from the subscriber's side. Before the fix the forwarder keeps the
// table alive after the system is dropped, and the subscriber's `Drop` never
// runs.
#[test]
fn unhandled_message_forwarder_releases_the_subscription_table_when_the_system_is_dropped() {
  let mut system = standalone_system("ForwarderOwnershipDropped");
  let stream = system.event_stream();
  stream.start_unhandled_message_forwarder();

  let (subscriber_flag, dropped) = DropFlag::new();
  let subscriber = system.function_ref("forwarder-ownership-subscriber", move |_sender: &ActorRef, _message: Message| {
    let _ = &subscriber_flag;
  });
  stream.subscribe(Some(subscriber.clone()), EventClass::any()).expect("the subscriber is not null");

  assert!(!dropped.load(Ordering::SeqCst), "the subscriber is alive while the stream holds it");

  drop(subscriber);
  drop(stream);
  drop(system);

  assert!(
    dropped.load(Ordering::SeqCst),
    "the subscription table and its subscriber were not released after the system was dropped"
  );
}

// Scenario: subscribing only a flag-holding subscriber, without starting the
// forwarder, already releases it. This is the control case: it keeps the
// subscriber observable on its own and proves the drop flag records a real
// release, so the forwarder case above cannot pass by an inert flag.
#[test]
fn a_subscriber_registered_only_on_the_table_is_released_when_the_system_is_dropped() {
  let mut system = standalone_system("ForwarderOwnershipControl");
  let stream = system.event_stream();

  let (subscriber_flag, dropped) = DropFlag::new();
  let subscriber = system.function_ref("control-subscriber", move |_sender: &ActorRef, _message: Message| {
    let _ = &subscriber_flag;
  });
  stream.subscribe(Some(subscriber.clone()), EventClass::any()).expect("the subscriber is not null");

  drop(subscriber);
  drop(stream);
  drop(system);

  assert!(dropped.load(Ordering::SeqCst), "a subscriber without a cycle is released");
}

// Scenario: the forwarder must turn an unhandled message into a debug event
// that names the sender, the message, and the recipient. The existing
// event_stream_spec covers this through the test kit; this case reads the
// debug text directly so the report can cite the behavior next to the
// ownership case.
#[test]
fn unhandled_message_forwarder_publishes_a_debug_naming_sender_message_and_recipient() {
  let mut system = standalone_system("ForwarderOwnershipBehavior");
  let stream = system.event_stream();
  stream.start_unhandled_message_forwarder();

  let messages: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
  let captured = Arc::clone(&messages);
  let observer = system.function_ref("forwarder-behavior-observer", move |_sender: &ActorRef, message: Message| {
    if let Some(debug) = message.downcast_ref::<Debug>() {
      captured.lock().expect("the message log is not poisoned").push(debug.message().to_string());
    }
  });
  stream.subscribe(Some(observer), EventClass::any()).expect("the observer is not null");

  let dead_letters = system.dead_letters();
  stream.publish(UnhandledMessage::new(42, dead_letters.clone(), dead_letters.clone()));

  let texts = messages.lock().expect("the message log is not poisoned").clone();
  assert_eq!(texts.len(), 1, "the forwarder published exactly one debug event: {texts:?}");
  assert_eq!(texts[0], format!("unhandled message from {}: 42", dead_letters.path()));
}
