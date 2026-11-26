// SPDX-License-Identifier: Apache-2.0
//
// Licensed to the Apache Software Foundation (ASF) under one or more
// license agreements; and to You under the Apache License, version 2.0:
//
//   https://www.apache.org/licenses/LICENSE-2.0
//
// This file is part of the Apache Pekko project, which was derived from Akka.
//
// Copyright (C) 2014-2022 Lightbend Inc. <https://www.lightbend.com>
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.

//! A handle that addresses one actor.

use core::any::Any;
use core::fmt;
use core::hash::{Hash, Hasher};

use portable_atomic::AtomicU64;
use portable_atomic::Ordering;

use crate::kernel::actor::ActorContext;
use crate::kernel::actor_path::ActorPath;
use crate::kernel::actor_ref::Endpoint;
use crate::kernel::dispatch::Mailbox;
use crate::kernel::dispatch::ManualTask;
use crate::kernel::executor::ActorCell;
use crate::kernel::executor::ShardBoundary;
use crate::kernel::message::Envelope;
use crate::kernel::message::Message;
use crate::kernel::sync::Arc;

#[cfg(test)]
#[path = "actor_ref_value_test.rs"]
mod tests;

/// A handle that lets a caller address one actor.
///
/// Mirrors `org.apache.pekko.actor.ActorRef`. It carries an identifier, a
/// generation number, the path of the actor, and where its messages go: the
/// actor's mailbox, or a shard boundary for a handle without a cell behind
/// it. A handle to a re-created actor carries a new identifier, so it does
/// not compare equal to the handle of the actor it replaced. Equality and
/// hashing use only the identifier and the generation; the path and the
/// endpoint are carried along and never take part in comparing two handles.
pub struct ActorRef {
  id: u64,
  generation: u32,
  path: Arc<ActorPath>,
  endpoint: Endpoint,
  detached: bool,
}

impl ActorRef {
  pub(crate) fn new(id: u64, generation: u32, path: Arc<ActorPath>, boundary: Arc<dyn ShardBoundary>) -> Self {
    Self { id, generation, path, endpoint: Endpoint::Boundary(boundary), detached: false }
  }

  /// Creates the handle of an actor whose messages go to `mailbox`.
  pub(crate) fn for_mailbox(id: u64, path: Arc<ActorPath>, mailbox: Arc<Mailbox<ActorCell>>) -> Self {
    Self { id, generation: 0, path, endpoint: Endpoint::Mailbox(mailbox), detached: false }
  }

  /// Returns the path of the actor.
  pub fn path(&self) -> ActorPath {
    (*self.path).clone()
  }

  /// Returns whether this handle's mailbox is already closed, as a stopped
  /// actor's is. A handle that posts to a boundary has no mailbox to close.
  pub(crate) fn mailbox_is_closed(&self) -> bool {
    match &self.endpoint {
      | Endpoint::Mailbox(mailbox) => mailbox.is_closed(),
      | Endpoint::Boundary(_) => false,
    }
  }

  /// Sends a message to the actor, naming `sender` as the sender.
  ///
  /// If the actor has stopped, the message goes to dead letters on this
  /// (the sender's) thread, before `tell` returns (issue #286): a dead
  /// letter subscriber whose handler runs where it is told (a function
  /// reference) is called on this thread. Such a handler runs while the
  /// sender's work is in progress, so it must not call back into the same
  /// system's driving calls (spawning, stopping, or running the system):
  /// the executor is lent for the duration of a run and those calls would
  /// wait for it. Telling another actor from the handler is fine, as it
  /// only enqueues.
  pub fn tell<M: Any + Send + Sync>(&self, message: M, sender: ActorRef) {
    self.send_message(Message::new(message), sender);
  }

  /// Sends a message to the actor while keeping the sender being handled.
  ///
  /// Mirrors `org.apache.pekko.actor.ActorRef.forward`, which is
  /// `tell(message, context.sender())`.
  pub fn forward(&self, message: Message, context: &mut ActorContext<'_>) {
    context.forward(self, message);
  }

  /// Sends `message` from `sender` to where this handle's messages go.
  ///
  /// A message for a mailbox goes onto its queue without the handle: the
  /// mailbox is the recipient, as Pekko's `Envelope` holds only the message
  /// and the sender. Only when the mailbox turns it away (closed, or a full
  /// bounded queue) does the handle go with it: the mailbox's refused route
  /// hands it to dead letters on this (the sender's) thread, as Pekko's
  /// dead letter mailbox does (issue #286). A mailbox with no route
  /// installed hands it to the dispatcher instead, which routes it when it
  /// gets to it (design of #234, section 11.11.1).
  pub(crate) fn send_message(&self, message: Message, sender: ActorRef) {
    match &self.endpoint {
      | Endpoint::Mailbox(mailbox) => {
        if let Err(refused) = Mailbox::send(mailbox, Envelope::without_recipient(message, sender)) {
          let refused = refused.with_recipient(self.clone());
          if let Some(refused) = mailbox.route_refused(refused) {
            mailbox.hand_over(ManualTask::Envelope(refused));
          }
        }
      },
      | Endpoint::Boundary(boundary) => boundary.post(Envelope::new(self.clone(), message, sender)),
    }
  }

  /// Returns the shard boundary this handle posts to, when it has no
  /// mailbox.
  pub(crate) fn boundary(&self) -> Option<&Arc<dyn ShardBoundary>> {
    match &self.endpoint {
      | Endpoint::Mailbox(_) => None,
      | Endpoint::Boundary(boundary) => Some(boundary),
    }
  }

  /// Returns whether the actor's path is `path`, without copying it.
  pub(crate) fn has_path(&self, path: &ActorPath) -> bool {
    *self.path == *path
  }

  /// Returns the key of the identity of the actor: its identifier and
  /// generation.
  pub(crate) fn key(&self) -> (u64, u32) {
    (self.id, self.generation)
  }

  pub(crate) fn is_detached(&self) -> bool {
    self.detached
  }

  /// Returns a handle with this actor's identity that posts into a delivery
  /// entity that swallows everything.
  ///
  /// The port cannot build Pekko's `EvilCollidingActorRef` on its own from a
  /// test, because the fields are private and equality already ignores the
  /// path and the endpoint. This is the limited operation that produces one:
  /// it keeps the identifier, generation, and path of the actor it is called
  /// on and swaps in a no-op shard, so the handle collides with that actor
  /// while being a distinct reference. It is only meaningful on an actor that
  /// has stopped, which [`crate::kernel::actor::ActorContext::stopped_noop_ref`]
  /// checks for.
  pub(crate) fn detached_noop(&self) -> Self {
    Self {
      id: self.id,
      generation: self.generation,
      path: Arc::clone(&self.path),
      endpoint: Endpoint::Boundary(Arc::new(crate::kernel::executor::NoOpBoundary)),
      detached: true,
    }
  }

  /// Returns whether this handle and `other` send to the same place.
  pub(crate) fn shares_endpoint_with(&self, other: &ActorRef) -> bool {
    self.endpoint.same_as(&other.endpoint)
  }

  /// Returns the shard key of the system this handle sends into: the
  /// address of its shard boundary, which a waiting `ask` drives that system
  /// by. A handle to a mailbox whose system is gone has no key, and no system
  /// answers to zero.
  pub(crate) fn system_key(&self) -> usize {
    match &self.endpoint {
      | Endpoint::Mailbox(mailbox) => mailbox.executor().map_or(0, |executor| executor.key()),
      | Endpoint::Boundary(boundary) => Arc::as_ptr(boundary) as *const () as usize,
    }
  }
}

/// The counter the fresh handles take their identifiers from.
///
/// The counter sits with the handle it numbers: the executor and the `ask`
/// adaptor both create handles through [`next_actor_ref`] and
/// [`next_actor_id`], so the factory lives here instead of behind the
/// executor's module, whose re-export nothing in the kernel used (the
/// no_std check warned on it while only the adaptor took that path).
static NEXT_ID: AtomicU64 = AtomicU64::new(0);

/// Returns the identifier the next fresh handle carries.
pub(crate) fn next_actor_id() -> u64 {
  NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

/// Creates one handle with a fresh identifier and generation zero.
pub(crate) fn next_actor_ref(path: ActorPath, boundary: Arc<dyn ShardBoundary>) -> ActorRef {
  ActorRef::new(next_actor_id(), 0, Arc::new(path), boundary)
}

impl Clone for ActorRef {
  fn clone(&self) -> Self {
    // syn-check: allow(tests-location) -- 所有権モデルの条件1の数え上げ（設計 §11.11.7、オーナーの許可 2026-11-27、元帥の項目472）
    #[cfg(test)]
    crate::kernel::sync::counts::count_path();
    Self {
      id: self.id,
      generation: self.generation,
      path: Arc::clone(&self.path),
      endpoint: self.endpoint.clone(),
      detached: self.detached,
    }
  }
}

impl PartialEq for ActorRef {
  fn eq(&self, other: &Self) -> bool {
    self.id == other.id && self.generation == other.generation
  }
}

impl Eq for ActorRef {}

impl Hash for ActorRef {
  fn hash<H: Hasher>(&self, state: &mut H) {
    self.id.hash(state);
    self.generation.hash(state);
  }
}

impl fmt::Debug for ActorRef {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(formatter, "ActorRef({}, generation {})", self.id, self.generation)
  }
}
