use std::sync::Arc;

use crate::adaptor::ChannelBoundary;
use crate::kernel::actor_system::ActorSystem;
use crate::kernel::event::EventStream;

use super::ActorEventBus;

#[test]
fn dropping_a_bus_releases_its_shared_state_before_the_subscriber_stops() {
  let mut system = ActorSystem::new("DroppedBusTable", Arc::new(ChannelBoundary::new()));
  let subscriber = system.function_ref("subscriber", |_, _| {});
  let classifier = system.dead_letters();
  let mut bus = ActorEventBus::new(system.downgrade(), system.event_stream(), "DroppedBusTable.Bus", true, {
    let classifier = classifier.clone();
    move |_: &u32| classifier.clone()
  });
  assert!(bus.subscribe(subscriber.clone(), classifier));
  let shared = Arc::downgrade(&bus.shared);
  assert!(shared.upgrade().is_some());
  assert!(!system.is_actor_terminated(&subscriber));

  drop(bus);

  assert!(
    shared.upgrade().is_none(),
    "the termination callback must not retain the shared subscription state after the bus is dropped"
  );
  assert!(!system.is_actor_terminated(&subscriber));
  system.terminate();
}

#[test]
fn an_expired_stream_leaves_the_shared_state_unchanged_on_termination() {
  let mut system = ActorSystem::new("ExpiredBusStream", Arc::new(ChannelBoundary::new()));
  // The system retains its default stream, so use an independently owned stream.
  let stream = EventStream::new(system.downgrade(), false);
  let stream_weak = stream.downgrade();
  let subscriber = system.function_ref("subscriber", |_, _| {});
  let classifier = system.dead_letters();
  let mut bus = ActorEventBus::new(system.downgrade(), stream, "ExpiredBusStream.Bus", true, {
    let classifier = classifier.clone();
    move |_: &u32| classifier.clone()
  });
  assert!(bus.subscribe(subscriber.clone(), classifier.clone()));
  // Retain only the shared state to exercise stream expiration independently
  // of its expiration. The subscription table itself is owned by the bus
  // without a lock (stage 1 of #234), so it cannot be retained on its own;
  // the table's contents are checked here, before the bus is dropped.
  let shared = Arc::clone(&bus.shared);
  assert_eq!(bus.subscriptions.subscribers_of(&classifier), vec![subscriber.clone()]);
  assert!(bus.subscriptions.holds_subscription_of(&subscriber));
  assert!(shared.lock().watched.contains(&subscriber));
  assert!(stream_weak.upgrade().is_some());

  drop(bus);
  assert!(stream_weak.upgrade().is_none(), "the callback must not retain the independent stream");
  system.stop(&subscriber);

  assert!(system.is_actor_terminated(&subscriber));
  assert!(shared.lock().watched.contains(&subscriber), "stream upgrade failure must leave the registration unchanged");
  assert!(shared.lock().pending_removals.is_empty(), "stream upgrade failure must not record a pending removal");
  system.terminate();
}
