// SPDX-License-Identifier: Apache-2.0
//
// Licensed to the Apache Software Foundation (ASF) under one or more
// license agreements; and to You under the Apache License, version 2.0:
//
//   https://www.apache.org/licenses/LICENSE-2.0
//
// This file is part of the Apache Pekko project, which was derived from Akka.
//
// Copyright (C) 2009-2022 Lightbend Inc. <https://www.lightbend.com>
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.

//! The event bus that maps a classifier actor to the subscribers of its events.

use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;

use portable_atomic::AtomicBool;
use portable_atomic::Ordering;

use crate::kernel::actor::Logging::Debug;
use crate::kernel::actor_ref::ActorRef;
use crate::kernel::actor_system::WeakActorSystem;
use crate::kernel::event::EventStream;
use crate::kernel::message::Message;
use crate::kernel::sync::lock;
use crate::kernel::sync::Arc;

#[cfg(test)]
#[path = "actor_event_bus_test.rs"]
mod tests;

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
/// The subscription table itself is not shared: the bus owns it without a
/// lock, and `publish` reads it directly (stage 1 of #234, design section
/// 2.4). Only the watched-subscriber set is shared with the executor's
/// termination callback, together with the list of subscribers whose removal
/// is pending and the flag that marks it. The callback holds only weak
/// handles to that shared state and to the event stream, so it cannot extend
/// their lifetimes after the bus is dropped. Both handles must upgrade before
/// subscriptions or debug events change. The callback never touches the
/// executor, which is locked while it runs.
///
/// `publish`, `subscribe`, and `unsubscribe` first read the flag with
/// `Acquire`; only when it stands do they take the lock and remove the
/// pending subscribers from the table, before they decide anything else. A
/// plain `publish` therefore costs one atomic load and no lock, and a
/// subscriber that has stopped is out of the table before the next
/// `publish` chooses its recipients, so it receives nothing, as before.
pub struct ActorEventBus<E: Clone + Send + Sync + 'static> {
  system: WeakActorSystem,
  event_stream: EventStream,
  name: String,
  debug: bool,
  classify: Box<dyn Fn(&E) -> ActorRef>,
  subscriptions: Subscriptions,
  shared: Arc<spin::Mutex<SharedSubscriptions>>,
  removal_pending: Arc<AtomicBool>,
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
    let shared = Arc::new(spin::Mutex::new(SharedSubscriptions::new()));
    let removal_pending = Arc::new(AtomicBool::new(false));
    let callback = {
      let shared = Arc::downgrade(&shared);
      let removal_pending = Arc::clone(&removal_pending);
      let event_stream = event_stream.downgrade();
      let name = name.to_string();
      move |target: &ActorRef| {
        let Some(shared) = shared.upgrade() else {
          return;
        };
        let Some(event_stream) = event_stream.upgrade() else {
          return;
        };
        if !lock(&shared).unwatch(target) {
          return;
        }
        if debug {
          publish_detach_debug(|event| event_stream.publish(event), &name, target);
        }
        lock(&shared).pending_removals.push(target.clone());
        removal_pending.store(true, Ordering::Release);
      }
    };
    system.on_termination(callback);
    Self {
      system,
      event_stream,
      name: name.to_string(),
      debug,
      classify: Box::new(classify),
      subscriptions: Subscriptions::new(),
      shared,
      removal_pending,
    }
  }

  /// Subscribes `subscriber` to the events of `to`.
  ///
  /// Mirrors `ManagedActorClassification.subscribe`: a new pair answers `true`
  /// and one the table already holds answers `false`. A subscriber that had
  /// already stopped is unsubscribed at once, the way
  /// `ActorClassificationUnsubscriber` answers `watch` for a stopped actor.
  pub fn subscribe(&mut self, subscriber: ActorRef, to: ActorRef) -> bool {
    self.apply_pending_removals();
    if !self.subscriptions.add(&to, &subscriber) {
      return false;
    }
    if lock(&self.shared).watch(&subscriber) {
      if self.system.is_actor_terminated(&subscriber) {
        self.subscriptions.remove_all(&subscriber);
        lock(&self.shared).unwatch(&subscriber);
        if self.debug {
          publish_detach_debug(|event| self.event_stream.publish(event), &self.name, &subscriber);
        }
      } else if self.debug {
        let registered = format!("registered watch for {} in {}", subscriber.path(), self.name);
        self.event_stream.publish(Debug::new(self.name.clone(), DEBUG_LOG_CLASS, &registered));
      }
    }
    true
  }

  /// Removes the subscription of `subscriber` from `from`.
  ///
  /// Mirrors `ManagedActorClassification.unsubscribe`: a pair the table holds
  /// answers `true` and one it does not hold answers `false`. The watch of the
  /// subscriber ends only with its last subscription, and only that last
  /// removal reports the end of the watch.
  pub fn unsubscribe(&mut self, subscriber: &ActorRef, from: &ActorRef) -> bool {
    self.apply_pending_removals();
    if !self.subscriptions.remove(from, subscriber) {
      return false;
    }
    let unwatched = !self.subscriptions.holds_subscription_of(subscriber) && lock(&self.shared).unwatch(subscriber);
    if unwatched && self.debug {
      let unregistered = format!("unregistered watch of {} in {}", subscriber.path(), self.name);
      self.event_stream.publish(Debug::new(self.name.clone(), DEBUG_LOG_CLASS, &unregistered));
    }
    true
  }

  /// Publishes `event` to every subscriber of its classifier actor.
  pub fn publish(&mut self, event: &E) {
    self.apply_pending_removals();
    let classifier = (self.classify)(event);
    let recipients = self.subscriptions.subscribers_of(&classifier);
    let message = Message::new(event.clone());
    for recipient in recipients {
      recipient.tell(message.clone(), recipient.clone());
    }
  }

  /// Returns the name the bus reports in its debug events.
  pub fn name(&self) -> &str {
    &self.name
  }

  /// Removes the subscribers the termination callback marked as stopped
  /// from the table the bus owns.
  ///
  /// The flag is read first, so the plain path takes no lock. The flag is
  /// cleared while the lock is held and the list is taken, so a removal the
  /// callback records afterwards sets it again; a removal recorded before
  /// the take but reported after the clear only leaves the flag standing
  /// over an empty list, which the next call clears the same way.
  fn apply_pending_removals(&mut self) {
    if !self.removal_pending.load(Ordering::Acquire) {
      return;
    }
    let pending = {
      let mut shared = lock(&self.shared);
      let pending = core::mem::take(&mut shared.pending_removals);
      self.removal_pending.store(false, Ordering::Release);
      pending
    };
    for target in &pending {
      self.subscriptions.remove_all(target);
    }
  }
}

/// Reports the end of the watch of a subscriber that has terminated.
///
/// Shared by the termination callback and `subscribe` so a subscriber that stops
/// later and one that had already stopped both report through the same rule,
/// as `ActorClassificationUnsubscriber` reports them through `Terminated`.
fn publish_detach_debug(publish_debug: impl Fn(Debug), name: &str, target: &ActorRef) {
  let path = target.path();
  let terminated = format!("actor {path} has terminated, unsubscribing it from {name}");
  publish_debug(Debug::new(String::from(name), DEBUG_LOG_CLASS, &terminated));
  let unregistered = format!("unregistered watch of {path} in {name}");
  publish_debug(Debug::new(String::from(name), DEBUG_LOG_CLASS, &unregistered));
}

/// The classifier-to-subscriber table the bus owns without a lock.
struct Subscriptions {
  entries: Vec<(ActorRef, ActorRef)>,
}

impl Subscriptions {
  fn new() -> Self {
    Self { entries: Vec::new() }
  }

  /// Records the subscription of `subscriber` to `classifier`.
  ///
  /// Returns whether the pair is new.
  fn add(&mut self, classifier: &ActorRef, subscriber: &ActorRef) -> bool {
    if self.entries.iter().any(|(to, existing)| to == classifier && existing == subscriber) {
      return false;
    }
    self.entries.push((classifier.clone(), subscriber.clone()));
    true
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

  /// Removes every subscription of `subscriber`.
  fn remove_all(&mut self, subscriber: &ActorRef) {
    self.entries.retain(|(_, existing)| existing != subscriber);
  }

  /// Returns the subscribers that `classifier` holds.
  fn subscribers_of(&self, classifier: &ActorRef) -> Vec<ActorRef> {
    self.entries.iter().filter(|(to, _)| to == classifier).map(|(_, subscriber)| subscriber.clone()).collect()
  }
}

/// The subscription state the termination callback shares with the bus:
/// the subscribers the bus watches, and the ones whose removal from the
/// table is still pending.
struct SharedSubscriptions {
  watched: Vec<ActorRef>,
  pending_removals: Vec<ActorRef>,
}

impl SharedSubscriptions {
  fn new() -> Self {
    Self { watched: Vec::new(), pending_removals: Vec::new() }
  }

  /// Starts watching `subscriber` and returns whether it is newly watched.
  fn watch(&mut self, subscriber: &ActorRef) -> bool {
    if self.watched.iter().any(|watched| watched == subscriber) {
      return false;
    }
    self.watched.push(subscriber.clone());
    true
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
}
