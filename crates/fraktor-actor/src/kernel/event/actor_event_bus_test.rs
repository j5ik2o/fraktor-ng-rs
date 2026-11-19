use std::rc::Rc;
use std::sync::Arc;

use crate::adaptor::ChannelBoundary;
use crate::kernel::actor_system::ActorSystem;
use crate::kernel::event::EventStream;

use super::ActorEventBus;

#[test]
fn dropping_a_bus_releases_its_subscription_table_before_the_subscriber_stops() {
  let mut system = ActorSystem::new("DroppedBusTable", Arc::new(ChannelBoundary::new()));
  let subscriber = system.function_ref("subscriber", |_, _| {});
  let classifier = system.dead_letters();
  let mut bus = ActorEventBus::new(system.downgrade(), system.event_stream(), "DroppedBusTable.Bus", true, {
    let classifier = classifier.clone();
    move |_: &u32| classifier.clone()
  });
  assert!(bus.subscribe(subscriber.clone(), classifier));
  let subscriptions = Rc::downgrade(&bus.subscriptions);
  assert!(subscriptions.upgrade().is_some());
  assert!(!system.is_actor_terminated(&subscriber));

  drop(bus);

  assert!(
    subscriptions.upgrade().is_none(),
    "the termination callback must not retain the subscription table after the bus is dropped"
  );
  assert!(!system.is_actor_terminated(&subscriber));
  system.terminate();
}

#[test]
fn an_expired_stream_leaves_the_subscription_table_unchanged_on_termination() {
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
  // Retain only the table to exercise stream expiration independently of table expiration.
  let subscriptions = Rc::clone(&bus.subscriptions);
  assert_eq!(subscriptions.borrow().subscribers_of(&classifier), vec![subscriber.clone()]);
  assert!(subscriptions.borrow().holds_subscription_of(&subscriber));
  assert!(stream_weak.upgrade().is_some());

  drop(bus);
  assert!(stream_weak.upgrade().is_none(), "the callback must not retain the independent stream");
  system.stop(&subscriber);

  assert!(system.is_actor_terminated(&subscriber));
  assert_eq!(
    subscriptions.borrow().subscribers_of(&classifier),
    vec![subscriber.clone()],
    "stream upgrade failure must leave the registration unchanged"
  );
  assert!(subscriptions.borrow().holds_subscription_of(&subscriber));
  assert!(subscriptions.borrow().watched.contains(&subscriber));
  system.terminate();
}
