//! The event bus that maps a classifier actor to the subscribers of its events.

use alloc::boxed::Box;
use alloc::format;
use alloc::rc::Rc;
use alloc::string::String;
use alloc::vec::Vec;
use core::cell::RefCell;

use crate::kernel::actor::Logging::Debug;
use crate::kernel::actor_ref::ActorRef;
use crate::kernel::actor_system::WeakActorSystem;
use crate::kernel::event::EventStream;
use crate::kernel::message::Message;

/// The class the ported debug events report, standing in for the `Class` of the
/// unsubscriber that Pekko names.
const DEBUG_LOG_CLASS: i32 = 0;

/// An event bus whose classifier is an actor.
///
/// Mirrors `org.apache.pekko.event.ActorEventBus` with
/// `ManagedActorClassification`: an event is delivered to the subscribers of the
/// actor that classifies it, and the bus stops watching a subscriber once the
/// subscriber stops. The classifier actor is not watched, so its end leaves the
/// subscriptions in place, as Pekko's unsubscriber does by watching only the
/// subscribers.
///
/// As `ActorClassificationUnsubscriber` does, a subscriber that had already
/// stopped when it subscribed is reported at once: the subscription is removed
/// during `subscribe`, which is the same set of targets that `ActorContext.watch`
/// reports with `existence_confirmed = false`. A subscriber that stops later is
/// removed through the termination callback.
///
/// The bus shares its subscription table with one termination callback of the
/// executor. The callback touches only the table and the event stream, never the
/// executor, because the executor is borrowed while the callback runs.
pub struct ActorEventBus<E: Clone + Send + Sync + 'static> {
  system: WeakActorSystem,
  event_stream: EventStream,
  name: String,
  debug: bool,
  classify: Box<dyn Fn(&E) -> ActorRef>,
  subscriptions: Rc<RefCell<Subscriptions>>,
}

impl<E: Clone + Send + Sync + 'static> ActorEventBus<E> {
  /// Creates a bus that delivers the events of a classifier actor.
  ///
  /// `name` is the name the bus reports in its debug events and `debug` decides
  /// whether it reports them at all, standing in for the
  /// `pekko.actor.debug.event-stream` switch that the original test turns on
  /// through configuration.
  pub fn new<F>(system: WeakActorSystem, event_stream: EventStream, name: &str, debug: bool, classify: F) -> Self
  where
    F: Fn(&E) -> ActorRef + 'static,
  {
    let subscriptions = Rc::new(RefCell::new(Subscriptions::new()));
    let callback = {
      let subscriptions = Rc::clone(&subscriptions);
      let event_stream = event_stream.clone();
      let name = name.to_string();
      move |target: &ActorRef| detach_subscriber(&subscriptions, &event_stream, &name, debug, target)
    };
    system.on_termination(callback);
    Self { system, event_stream, name: name.to_string(), debug, classify: Box::new(classify), subscriptions }
  }

  /// Subscribes `subscriber` to the events of `to`.
  ///
  /// Mirrors `ManagedActorClassification.subscribe`: a new pair answers `true`
  /// and one the table already holds answers `false`. A subscriber that had
  /// already stopped is unsubscribed at once, the way
  /// `ActorClassificationUnsubscriber` answers `watch` for a stopped actor.
  pub fn subscribe(&mut self, subscriber: ActorRef, to: ActorRef) -> bool {
    let added = self.subscriptions.borrow_mut().add(&to, &subscriber);
    match added {
      | None => false,
      | Some(newly_watched) => {
        if newly_watched {
          if self.system.is_actor_terminated(&subscriber) {
            detach_subscriber(&self.subscriptions, &self.event_stream, &self.name, self.debug, &subscriber);
          } else if self.debug {
            let registered = format!("registered watch for {} in {}", subscriber.path(), self.name);
            self.event_stream.publish(Debug::new(self.name.clone(), DEBUG_LOG_CLASS, &registered));
          }
        }
        true
      },
    }
  }

  /// Removes the subscription of `subscriber` from `from`.
  ///
  /// Mirrors `ManagedActorClassification.unsubscribe`: a pair the table holds
  /// answers `true` and one it does not hold answers `false`. The watch of the
  /// subscriber ends only with its last subscription, and only that last
  /// removal reports the end of the watch.
  pub fn unsubscribe(&mut self, subscriber: &ActorRef, from: &ActorRef) -> bool {
    let (removed, unwatched) = {
      let mut subscriptions = self.subscriptions.borrow_mut();
      let removed = subscriptions.remove(from, subscriber);
      let unwatched = removed && !subscriptions.holds_subscription_of(subscriber);
      if unwatched {
        subscriptions.unwatch(subscriber);
      }
      (removed, unwatched)
    };
    if !removed {
      return false;
    }
    if unwatched && self.debug {
      let unregistered = format!("unregistered watch of {} in {}", subscriber.path(), self.name);
      self.event_stream.publish(Debug::new(self.name.clone(), DEBUG_LOG_CLASS, &unregistered));
    }
    true
  }

  /// Publishes `event` to every subscriber of its classifier actor.
  pub fn publish(&mut self, event: &E) {
    let classifier = (self.classify)(event);
    let recipients = self.subscriptions.borrow().subscribers_of(&classifier);
    let message = Message::new(event.clone());
    for recipient in recipients {
      recipient.tell(message.clone(), recipient.clone());
    }
  }

  /// Returns the name the bus reports in its debug events.
  pub fn name(&self) -> &str {
    &self.name
  }
}

/// Removes every subscription of `target` and reports the end of its watch.
///
/// Shared by the termination callback and `subscribe` so a subscriber that stops
/// later and one that had already stopped both leave the table through the same
/// rule, as `ActorClassificationUnsubscriber` reports them through
/// `Terminated`.
fn detach_subscriber(
  subscriptions: &Rc<RefCell<Subscriptions>>,
  event_stream: &EventStream,
  name: &str,
  debug: bool,
  target: &ActorRef,
) {
  let watched = subscriptions.borrow_mut().detach(target);
  if !watched || !debug {
    return;
  }
  let path = target.path();
  let terminated = format!("actor {path} has terminated, unsubscribing it from {name}");
  event_stream.publish(Debug::new(String::from(name), DEBUG_LOG_CLASS, &terminated));
  let unregistered = format!("unregistered watch of {path} in {name}");
  event_stream.publish(Debug::new(String::from(name), DEBUG_LOG_CLASS, &unregistered));
}

/// The classifier-to-subscriber table and the subscribers the bus watches.
struct Subscriptions {
  entries: Vec<(ActorRef, ActorRef)>,
  watched: Vec<ActorRef>,
}

impl Subscriptions {
  fn new() -> Self {
    Self { entries: Vec::new(), watched: Vec::new() }
  }

  /// Records the subscription of `subscriber` to `classifier`.
  ///
  /// Returns `None` when the pair is already present. Returns `Some(true)` when
  /// the pair is new and the subscriber became watched, and `Some(false)` when
  /// the pair is new but the subscriber was already watched.
  fn add(&mut self, classifier: &ActorRef, subscriber: &ActorRef) -> Option<bool> {
    if self.entries.iter().any(|(to, existing)| to == classifier && existing == subscriber) {
      return None;
    }
    self.entries.push((classifier.clone(), subscriber.clone()));
    if self.watched.iter().any(|watched| watched == subscriber) {
      Some(false)
    } else {
      self.watched.push(subscriber.clone());
      Some(true)
    }
  }

  /// Removes the subscription of `subscriber` to `classifier`.
  fn remove(&mut self, classifier: &ActorRef, subscriber: &ActorRef) -> bool {
    match self.entries.iter().position(|(to, existing)| to == classifier && existing == subscriber) {
      | Some(index) => {
        self.entries.remove(index);
        true
      },
      | None => false,
    }
  }

  /// Returns whether `subscriber` still holds a subscription.
  fn holds_subscription_of(&self, subscriber: &ActorRef) -> bool {
    self.entries.iter().any(|(_, existing)| existing == subscriber)
  }

  /// Removes every subscription of `subscriber` and stops watching it.
  ///
  /// Returns whether the subscriber was watched.
  fn detach(&mut self, subscriber: &ActorRef) -> bool {
    self.entries.retain(|(_, existing)| existing != subscriber);
    self.unwatch(subscriber)
  }

  /// Stops watching `subscriber` and returns whether it was watched.
  fn unwatch(&mut self, subscriber: &ActorRef) -> bool {
    match self.watched.iter().position(|watched| watched == subscriber) {
      | Some(index) => {
        self.watched.remove(index);
        true
      },
      | None => false,
    }
  }

  /// Returns the subscribers that `classifier` holds.
  fn subscribers_of(&self, classifier: &ActorRef) -> Vec<ActorRef> {
    self.entries.iter().filter(|(to, _)| to == classifier).map(|(_, subscriber)| subscriber.clone()).collect()
  }
}
