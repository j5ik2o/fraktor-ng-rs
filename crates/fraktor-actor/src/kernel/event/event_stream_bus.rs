//! The subscription table that an event stream shares with the loggers it
//! starts.

use alloc::sync::Arc;
use alloc::vec::Vec;
use core::any::Any;
use core::any::TypeId;

use spin::Mutex;

use crate::kernel::actor_ref::ActorRef;
use crate::kernel::event::EventClass;
use crate::kernel::event::NullSubscriber;
use crate::kernel::message::Message;

/// The subscription table of an event stream.
///
/// Mirrors the `SubchannelClassification` part of
/// `org.apache.pekko.event.EventStream`: a subscriber registers for a channel
/// and receives every event whose class is that channel or a declared subclass
/// of it. A subscription to [`EventClass::any`] therefore reaches every event,
/// because the port treats `any` as the parent of every class.
///
/// The table is shared by cloning the handle. A logger receives it through
/// [`InitializeLogger`](crate::kernel::actor::Logging::InitializeLogger), so it
/// must be usable from another thread and cannot hold the actor system. It
/// therefore owns only the channel data, while the stream that creates it adds
/// the debug reporting and the watch on a subscriber's termination.
#[derive(Clone)]
pub struct EventStreamBus {
  state: Arc<Mutex<BusState>>,
}

impl EventStreamBus {
  /// Creates an empty subscription table.
  pub fn new() -> Self {
    Self { state: Arc::new(Mutex::new(BusState::new())) }
  }

  /// Records that `child` is a subclass of `parent`.
  ///
  /// Rust has no run-time class hierarchy, so a test declares the links that
  /// Pekko reads from `Class.isAssignableFrom`. Declaring the same link twice
  /// changes nothing.
  pub(crate) fn declare_subclass(&self, child: EventClass, parent: EventClass) {
    self.state.lock().declare_subclass(child, parent);
  }

  /// Subscribes `subscriber` to `event_class`.
  ///
  /// Mirrors `EventStream.subscribe`: the subscriber is added to the channel
  /// and to every known subclass channel, and the call answers whether the
  /// table changed. A repeated subscription answers `false`.
  ///
  /// # Errors
  ///
  /// Returns [`NullSubscriber`] when `subscriber` is `None`, which is the
  /// `IllegalArgumentException` Pekko throws for a null subscriber.
  pub fn subscribe(&self, subscriber: Option<ActorRef>, event_class: EventClass) -> Result<bool, NullSubscriber> {
    let Some(subscriber) = subscriber else {
      return Err(NullSubscriber);
    };
    Ok(self.state.lock().subscribe(&subscriber, event_class))
  }

  /// Removes the subscription of `subscriber` from `event_class`.
  ///
  /// Mirrors `EventStream.unsubscribe`: the subscriber leaves the channel and
  /// every known subclass channel, and the call answers whether the table
  /// changed. Removing a channel the subscriber never held answers `false`.
  pub(crate) fn unsubscribe(&self, subscriber: &ActorRef, event_class: EventClass) -> bool {
    self.state.lock().unsubscribe(subscriber, event_class)
  }

  /// Removes every subscription of `subscriber`.
  ///
  /// Mirrors `EventStream.unsubscribe(subscriber)`.
  pub(crate) fn unsubscribe_all(&self, subscriber: &ActorRef) {
    self.state.lock().unsubscribe_all(subscriber);
  }

  /// Publishes `event` to every subscriber of its class.
  ///
  /// Mirrors `EventStream.publish`: the recipients are collected before
  /// anything is sent, so a delivery may touch the stream again without
  /// holding the table.
  ///
  /// A value that is already a [`Message`] is published to the subscribers of
  /// the class its payload carries, which is the `event.getClass` of Pekko's
  /// `EventStream.classify`; republishing a received message therefore reaches
  /// the subscribers of the carried event rather than the subscribers of
  /// `Message`. Every other value is published to the subscribers of its own
  /// class.
  pub fn publish<E: Any + Send + Sync>(&self, event: E) {
    let message = Message::new(event);
    let class = if TypeId::of::<E>() == TypeId::of::<Message>() {
      EventClass::of_type_id(message.payload_type_id())
    } else {
      EventClass::of::<E>()
    };
    let recipients = self.state.lock().recipients_of(class);
    for recipient in recipients {
      recipient.tell(message.clone(), recipient.clone());
    }
  }

  /// Returns whether `subscriber` still holds a subscription on any channel.
  ///
  /// Mirrors the check that `EventStreamUnsubscriber` makes before it stops
  /// watching an actor.
  pub(crate) fn has_subscriptions(&self, subscriber: &ActorRef) -> bool {
    self.state.lock().has_subscriptions(subscriber)
  }

  /// Returns every subscriber that holds at least one subscription.
  ///
  /// Mirrors the `initiallySubscribed` set that Pekko registers with an
  /// unsubscriber when it starts.
  pub(crate) fn subscribers(&self) -> Vec<ActorRef> {
    self.state.lock().subscribers()
  }
}

impl Default for EventStreamBus {
  fn default() -> Self {
    Self::new()
  }
}

/// The channels of a bus and the subclass links declared on it.
struct BusState {
  channels: Vec<(EventClass, Vec<ActorRef>)>,
  subclass_links: Vec<(EventClass, EventClass)>,
}

impl BusState {
  fn new() -> Self {
    Self { channels: Vec::new(), subclass_links: Vec::new() }
  }

  fn declare_subclass(&mut self, child: EventClass, parent: EventClass) {
    if self.subclass_links.iter().any(|(c, p)| *c == child && *p == parent) {
      return;
    }
    self.subclass_links.push((child, parent));
    self.propagate_subclass_link(child, parent);
  }

  /// Reflects a newly declared link in `child` and the channels below it.
  ///
  /// Mirrors `SubclassifiedIndex.innerAddKey`: the new key is integrated under
  /// its most specific known super-class, and `addValue` recurses into the
  /// sub-keys. Only the subscribers that newly appear in `child` flow down:
  /// the subscribers of the new parent's channel, plus the seed of a channel
  /// created by this declaration. The older parents are not re-read, so a
  /// subscriber explicitly removed from `child` is not brought back by an
  /// unrelated link. The same set is then pushed directly into every
  /// descendant channel, so the order in which descendants are visited does
  /// not affect the result: a deep descendant receives the new subscribers
  /// even when its channel was created before its parent's.
  fn propagate_subclass_link(&mut self, child: EventClass, new_parent: EventClass) {
    self.ensure(new_parent);
    let before = self.channel(child);
    self.ensure(child);
    let contributed = self.channel(new_parent);
    let Some(index) = self.channel_index(child) else {
      return;
    };
    for subscriber in contributed {
      push_unique(&mut self.channels[index].1, subscriber);
    }
    let added: Vec<ActorRef> =
      self.channels[index].1.iter().filter(|subscriber| !before.contains(subscriber)).cloned().collect();
    let descendants = self.descendants_of(child);
    for descendant in descendants {
      let Some(descendant_index) = self.channel_index(descendant) else {
        continue;
      };
      for subscriber in &added {
        push_unique(&mut self.channels[descendant_index].1, subscriber.clone());
      }
    }
  }

  /// Returns the parents that `class` directly declares, plus `any`.
  ///
  /// This is the "most specific known super-class" set that Pekko's
  /// `SubclassifiedIndex.innerAddKey` inherits from. `any` stands for the root
  /// super-class (`classOf[Any]`), so a class with no declared parent still
  /// inherits the subscribers of [`EventClass::any`].
  fn direct_parents(&self, class: EventClass) -> Vec<EventClass> {
    let mut parents = Vec::new();
    for (child, parent) in &self.subclass_links {
      if *child == class && !parents.contains(parent) {
        parents.push(*parent);
      }
    }
    let any = EventClass::any();
    if class != any && !parents.contains(&any) {
      parents.push(any);
    }
    parents
  }

  /// Returns the declared parents of `class`, transitively, plus `any`.
  fn ancestors(&self, class: EventClass) -> Vec<EventClass> {
    let mut found = Vec::new();
    let mut pending = self.direct_parents(class);
    while let Some(next) = pending.pop() {
      if next == class || found.contains(&next) {
        continue;
      }
      found.push(next);
      pending.extend(self.direct_parents(next));
    }
    found
  }

  fn is_descendant_of(&self, class: EventClass, ancestor: EventClass) -> bool {
    class == ancestor || self.ancestors(class).contains(&ancestor)
  }

  fn channel_index(&self, class: EventClass) -> Option<usize> {
    self.channels.iter().position(|(known, _)| *known == class)
  }

  fn channel(&self, class: EventClass) -> Vec<ActorRef> {
    match self.channel_index(class) {
      | Some(index) => self.channels[index].1.clone(),
      | None => Vec::new(),
    }
  }

  /// Creates the channel of `class` when it is unknown, seeded with the
  /// subscribers of the most specific known super-class channels.
  ///
  /// Mirrors `SubclassifiedIndex.innerAddKey`: a new key inherits from the
  /// super-classes it is integrated under, not from every transitive ancestor.
  fn ensure(&mut self, class: EventClass) {
    if self.channel_index(class).is_some() {
      return;
    }
    self.channels.push((class, Vec::new()));
    let mut inherited = Vec::new();
    for parent in self.direct_parents(class) {
      self.ensure(parent);
      for subscriber in self.channel(parent) {
        push_unique(&mut inherited, subscriber);
      }
    }
    if let Some(index) = self.channel_index(class) {
      self.channels[index].1 = inherited;
    }
  }

  fn descendants_of(&self, class: EventClass) -> Vec<EventClass> {
    self
      .channels
      .iter()
      .map(|(known, _)| *known)
      .filter(|known| *known != class && self.is_descendant_of(*known, class))
      .collect()
  }

  fn insert(&mut self, class: EventClass, subscriber: &ActorRef) -> bool {
    let index = match self.channel_index(class) {
      | Some(index) => index,
      | None => {
        self.ensure(class);
        self.channel_index(class).expect("the channel was just created")
      },
    };
    if self.channels[index].1.iter().any(|existing| existing == subscriber) {
      return false;
    }
    self.channels[index].1.push(subscriber.clone());
    true
  }

  fn remove(&mut self, class: EventClass, subscriber: &ActorRef) -> bool {
    let Some(index) = self.channel_index(class) else {
      return false;
    };
    let before = self.channels[index].1.len();
    self.channels[index].1.retain(|existing| existing != subscriber);
    self.channels[index].1.len() != before
  }

  fn subscribe(&mut self, subscriber: &ActorRef, class: EventClass) -> bool {
    self.ensure(class);
    let mut changed = self.insert(class, subscriber);
    for descendant in self.descendants_of(class) {
      changed |= self.insert(descendant, subscriber);
    }
    changed
  }

  fn unsubscribe(&mut self, subscriber: &ActorRef, class: EventClass) -> bool {
    self.ensure(class);
    let mut changed = self.remove(class, subscriber);
    let mut descendants = self.descendants_of(class);
    // Parents before their descendants, so a re-derived child reads the
    // already-updated cache of a parent that is itself under `class`.
    descendants.sort_by_key(|known| self.ancestors(*known).len());
    for descendant in descendants {
      changed |= self.remove(descendant, subscriber);
      // Mirrors `SubclassifiedIndex.removeValue` re-deriving each changed key
      // from the remaining paths: the subscriber returns when another direct
      // super-class channel still holds it.
      let restored =
        self.direct_parents(descendant).into_iter().any(|parent| self.channel(parent).contains(subscriber));
      if restored {
        changed |= self.insert(descendant, subscriber);
      }
    }
    changed
  }

  fn unsubscribe_all(&mut self, subscriber: &ActorRef) {
    for (_, subscribers) in &mut self.channels {
      subscribers.retain(|existing| existing != subscriber);
    }
  }

  fn has_subscriptions(&self, subscriber: &ActorRef) -> bool {
    self.channels.iter().any(|(_, subscribers)| subscribers.iter().any(|existing| existing == subscriber))
  }

  fn subscribers(&self) -> Vec<ActorRef> {
    let mut all = Vec::new();
    for (_, subscribers) in &self.channels {
      for subscriber in subscribers {
        push_unique(&mut all, subscriber.clone());
      }
    }
    all
  }

  fn recipients_of(&mut self, class: EventClass) -> Vec<ActorRef> {
    self.ensure(class);
    self.channel(class)
  }
}

fn push_unique(subscribers: &mut Vec<ActorRef>, subscriber: ActorRef) {
  if !subscribers.contains(&subscriber) {
    subscribers.push(subscriber);
  }
}
