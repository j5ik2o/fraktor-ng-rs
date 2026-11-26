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

//! A path-based address whose target is resolved when the message is
//! delivered.

use core::any::Any;

use crate::kernel::actor_path::ActorPath;
use crate::kernel::actor_ref::ActorRef;
use crate::kernel::executor::LiveActors;
use crate::kernel::executor::ShardBoundary;
use crate::kernel::message::Envelope;
use crate::kernel::message::Message;
use crate::kernel::sync::Arc;
use crate::kernel::sync::Weak;

/// The envelope a selection posts to its shard.
///
/// Mirrors `org.apache.pekko.actor.ActorSelectionMessage`: the selection
/// itself is resolved by the executor when the envelope is delivered, not
/// when it is sent, so a selection created for an actor that stops in the
/// meantime is routed by the dead letter rules for an unresolved path.
#[derive(Clone)]
pub(crate) struct ActorSelectionMessage {
  path: ActorPath,
  message: Message,
  sender: ActorRef,
}

impl ActorSelectionMessage {
  /// Returns the path the delivery resolves.
  pub(crate) fn path(&self) -> &ActorPath {
    &self.path
  }

  /// Returns the message the delivery hands on.
  pub(crate) fn message(&self) -> &Message {
    &self.message
  }

  /// Returns the actor that sent the message.
  pub(crate) fn sender(&self) -> &ActorRef {
    &self.sender
  }
}

/// An address that names its target by path.
///
/// Mirrors `org.apache.pekko.actor.ActorSelection` for the local scope of
/// this port. The selection never borrows the executor, which is what makes
/// it safe to use from inside an actor's `receive`, and it keeps the
/// selection `Send`.
///
/// A path that names a live actor is resolved when `tell` is called, on the
/// sender's thread, as Pekko's `deliverSelection` does, so the message is in
/// the actor's mailbox before `tell` returns. Any other path is resolved by
/// the executor when it delivers the envelope `tell` posts to the shard
/// boundary: a function reference receives the message, and a path that
/// names a stopped actor or nothing is routed by the dead letter rules with
/// an empty reference for that path as the recipient.
pub struct ActorSelection {
  boundary: Arc<dyn ShardBoundary>,
  carrier: ActorRef,
  path: ActorPath,
  actors: Weak<LiveActors>,
}

impl ActorSelection {
  pub(crate) fn new(
    boundary: Arc<dyn ShardBoundary>,
    carrier: ActorRef,
    path: ActorPath,
    actors: Weak<LiveActors>,
  ) -> Self {
    Self { boundary, carrier, path, actors }
  }

  /// Returns the path this selection resolves.
  pub fn path(&self) -> ActorPath {
    self.path.clone()
  }

  /// Sends `message` to the actor this selection resolves to, naming
  /// `sender` as the sender.
  pub fn tell<M: Any + Send + Sync>(&self, message: M, sender: ActorRef) {
    if let Some(target) = self.actors.upgrade().and_then(|actors| actors.find(&self.path)) {
      target.tell(message, sender);
      return;
    }
    let selection = ActorSelectionMessage { path: self.path.clone(), message: Message::new(message), sender };
    let envelope = Envelope::new(self.carrier.clone(), Message::new(selection), self.carrier.clone());
    self.boundary.post(envelope);
  }
}
