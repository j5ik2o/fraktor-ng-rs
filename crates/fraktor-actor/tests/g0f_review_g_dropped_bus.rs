use std::sync::{Arc, Mutex};

use fraktor_actor::adaptor::ChannelBoundary;
use fraktor_actor::kernel::actor::Logging::{Debug, LogEvent};
use fraktor_actor::kernel::actor_system::ActorSystem;
use fraktor_actor::kernel::event::{ActorEventBus, EventClass};

#[test]
fn stopping_a_subscriber_after_dropping_its_bus_does_not_publish_bus_debug() {
  let mut system = ActorSystem::new("DroppedBusDebug", Arc::new(ChannelBoundary::new()));
  let stream = system.event_stream();
  let received = Arc::new(Mutex::new(Vec::new()));
  let observer = {
    let received = Arc::clone(&received);
    system.function_ref("debug-observer", move |_, message| {
      if let Some(event) = message.downcast_ref::<Debug>() {
        received.lock().unwrap().push((event.log_source().to_string(), event.message().to_string()));
      }
    })
  };
  assert!(stream.subscribe(Some(observer.clone()), EventClass::of::<Debug>()).unwrap());
  let subscriber = system.function_ref("subscriber", |_, _| {});
  let classifier = system.dead_letters();
  let mut bus = ActorEventBus::new(system.downgrade(), stream.clone(), "DroppedBusDebug.Bus", true, {
    let classifier = classifier.clone();
    move |_: &u32| classifier.clone()
  });
  assert!(bus.subscribe(subscriber.clone(), classifier));
  system.run_until_idle();
  assert!(received.lock().unwrap().iter().any(|(source, _)| source == "DroppedBusDebug.Bus"));
  received.lock().unwrap().clear();

  drop(bus);
  system.stop(&subscriber);
  system.run_until_idle();
  assert!(system.is_actor_terminated(&subscriber));

  // Prove the same stream and observer still deliver after the bus is gone.
  stream.publish(Debug::new("observer-check".to_string(), 0, "still observing"));
  system.run_until_idle();
  let events = received.lock().unwrap().clone();
  assert!(events.iter().any(|(source, _)| source == "observer-check"));
  assert!(!system.is_actor_terminated(&observer));
  system.terminate();
  assert!(
    events.iter().all(|(source, _)| source != "DroppedBusDebug.Bus"),
    "a dropped bus must not publish termination Debug events: {events:?}"
  );
}
