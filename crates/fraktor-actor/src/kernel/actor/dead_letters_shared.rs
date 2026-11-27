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

//! The state the dead letters handle and the executor share, and the
//! routing rules that turn an undeliverable message into a dead letter
//! event.

// Issue #297: the unit tests for `route_system_undeliverable` are written in
// `dead_letters_shared_test.rs` (design `issue-296-297-dead-letters-followup.md`,
// section 7.2). The tests call that method, which #297 adds, so the module does
// not compile yet; `cfg(any())` keeps it out of the build so the rest of the
// kernel's unit tests stay green, and the red is pinned here for the
// implementation step. Replace `cfg(any())` with `cfg(test)` when
// `route_system_undeliverable` exists.
#[cfg(any())]
#[path = "dead_letters_shared_test.rs"]
mod tests;

use alloc::collections::BTreeSet;
use core::any::TypeId;

use spin::Mutex;

use crate::kernel::actor::DeadLetter;
use crate::kernel::actor::SuppressedDeadLetter;
use crate::kernel::actor::Terminated;
use crate::kernel::actor_ref::ActorRef;
use crate::kernel::event::EventStreamBusWeak;
use crate::kernel::message::Message;

/// The state behind the dead letters handle.
///
/// The handle is a function reference whose handler runs wherever a sender
/// posts to it, and the executor routes to the same rules when a delivery
/// finds no actor, so the two keep their shared facts here instead of in
/// the executor: the weak handle of the event stream's subscription table,
/// which is filled in once the system has built its stream, and the table
/// of payload types registered for suppression. The bus is filled in
/// once, when the system is built, so it sits in a `spin::Once` that
/// publishing reads without a lock. The suppression table sits behind a
/// spin mutex because a function reference must be `Send + Sync`, the
/// handler never borrows the executor, and payload types can be
/// registered at any time.
pub(crate) struct DeadLettersShared {
  bus: spin::Once<EventStreamBusWeak>,
  suppression: Mutex<BTreeSet<TypeId>>,
}

impl DeadLettersShared {
  /// Creates the shared state with the kernel's own suppressed types
  /// already registered.
  ///
  /// Mirrors the `isSuppressed` knowledge Pekko's dead letter handling
  /// carries: a `Terminated` notice, the two control messages `PoisonPill`
  /// and `Kill` (which the port carries as string payloads, so they are
  /// recognised by value in [`Self::is_suppressed`]), and the dead letter
  /// events themselves are never counted as fresh dead letters.
  pub(crate) fn new() -> Self {
    let mut suppression = BTreeSet::new();
    suppression.insert(TypeId::of::<Terminated>());
    suppression.insert(TypeId::of::<DeadLetter>());
    suppression.insert(TypeId::of::<SuppressedDeadLetter>());
    Self { bus: spin::Once::new(), suppression: Mutex::new(suppression) }
  }

  /// Records the subscription table the routing rules publish to.
  ///
  /// The system calls this once, when it builds its event stream; a
  /// later call keeps the first table.
  pub(crate) fn set_bus(&self, bus: EventStreamBusWeak) {
    self.bus.call_once(|| bus);
  }

  /// Registers the payload type `id` for suppression.
  pub(crate) fn register_suppression(&self, id: TypeId) {
    self.suppression.lock().insert(id);
  }

  /// Returns whether the payload of `message` is marked for suppression.
  ///
  /// The string control messages are recognised by value, not by their
  /// `&str` type: a user message of the same type must still be reported.
  pub(crate) fn is_suppressed(&self, message: &Message) -> bool {
    if let Some(text) = message.downcast_ref::<&'static str>() {
      if *text == "PoisonPill" || *text == "Kill" {
        return true;
      }
    }
    self.suppression.lock().contains(&message.payload_type_id())
  }

  /// Routes a message whose delivery found no actor.
  ///
  /// Mirrors `LocalActorRef.!` handing the message to the dead letters
  /// provider together with the mailbox clean-up rules: a `DeadLetter`
  /// addressed to a stopped actor is dropped, never published, so a
  /// subscriber that stopped while subscribed to dead letters cannot make
  /// dead letters circulate for ever. Any other message is published once:
  /// a suppressed payload as a [`SuppressedDeadLetter`] naming the dead
  /// letters handle as the reference that processed it, and any other
  /// payload as a [`DeadLetter`] naming the actor it was addressed to.
  pub(crate) fn route_undeliverable(
    &self,
    message: Message,
    sender: ActorRef,
    recipient: ActorRef,
    dead_letters: &ActorRef,
  ) {
    if message.downcast_ref::<DeadLetter>().is_some() {
      return;
    }
    if let Some(suppressed) = message.downcast_ref::<SuppressedDeadLetter>() {
      // A suppressed event that itself could not be delivered (for example
      // to a subscriber that stopped while subscribed) is published once
      // more, wrapped in a `DeadLetter` naming the actor it was addressed
      // to. Republishing it as a fresh `SuppressedDeadLetter` would fail
      // the same delivery again and circulate for ever; wrapped in a
      // `DeadLetter` it is dropped by the rule above when its own delivery
      // fails, so the chain ends.
      let suppressed = suppressed.clone();
      self.publish(DeadLetter::new(suppressed, sender, recipient));
      return;
    }
    if self.is_suppressed(&message) {
      self.publish(SuppressedDeadLetter::from_message(message, sender, dead_letters.clone()));
    } else {
      self.publish(DeadLetter::from_message(message, sender, recipient));
    }
  }

  /// Routes a message that reached the dead letters handle itself.
  ///
  /// Mirrors `DeadLetterActorRef.!`: a `DeadLetter` is published as it is,
  /// keeping the recipient it already names, unless its own message is
  /// suppressed, in which case a [`SuppressedDeadLetter`] naming the dead
  /// letters handle is published instead. Any other message is wrapped:
  /// suppressed payloads as a `SuppressedDeadLetter`, the rest as a
  /// `DeadLetter`, both naming the dead letters handle as the recipient.
  pub(crate) fn route_at_dead_letters(&self, message: Message, sender: ActorRef, dead_letters: &ActorRef) {
    if let Some(dead) = message.downcast_ref::<DeadLetter>() {
      let dead = dead.clone();
      if self.is_suppressed(dead.message()) {
        self.publish(SuppressedDeadLetter::from_message(
          dead.message().clone(),
          dead.sender().clone(),
          dead_letters.clone(),
        ));
      } else {
        self.publish(dead);
      }
      return;
    }
    if self.is_suppressed(&message) {
      self.publish(SuppressedDeadLetter::from_message(message, sender, dead_letters.clone()));
    } else {
      self.publish(DeadLetter::from_message(message, sender, dead_letters.clone()));
    }
  }

  /// Routes a message whose selection resolved to nothing.
  ///
  /// The empty reference for the unresolved path is both the recipient of
  /// a `DeadLetter` and the reference that processed a suppressed payload,
  /// which is why the selection case of `SupressionSpec` compares only the
  /// message: the reference is freshly made for this delivery.
  pub(crate) fn route_unresolved(&self, message: Message, sender: ActorRef, empty: ActorRef) {
    if self.is_suppressed(&message) {
      self.publish(SuppressedDeadLetter::from_message(message, sender, empty));
    } else {
      self.publish(DeadLetter::from_message(message, sender, empty));
    }
  }

  fn publish<E: core::any::Any + Send + Sync>(&self, event: E) {
    if let Some(bus) = self.bus.get() {
      bus.publish(event);
    }
  }
}
