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

//! An envelope that binds a message to its recipient and sender.

use crate::kernel::actor_ref::ActorRef;
use crate::kernel::executor::ShardBoundary;
use crate::kernel::message::message_value::Message;
use crate::kernel::sync::Arc;

/// One delivery: who receives the message, what it is, and who sent it.
///
/// Mirrors `org.apache.pekko.dispatch.Envelope` for the fields G0b needs. It is
/// created when a message is posted and consumed when the executor delivers it.
pub struct Envelope {
  recipient: ActorRef,
  message: Message,
  sender: ActorRef,
}

impl Envelope {
  pub(crate) fn new(recipient: ActorRef, message: Message, sender: ActorRef) -> Self {
    Self { recipient, message, sender }
  }

  pub(crate) fn is_addressed_to(&self, recipient: &ActorRef) -> bool {
    &self.recipient == recipient
  }

  pub(crate) fn into_parts(self) -> (ActorRef, Message, ActorRef) {
    (self.recipient, self.message, self.sender)
  }

  /// Returns the envelope back if the recipient lives in the shard of
  /// `boundary`, so the caller can put it on that shard's local queue.
  pub(crate) fn into_local_if_same_shard(self, boundary: &Arc<dyn ShardBoundary>) -> Result<Self, Self> {
    if self.recipient.shares_boundary_with(boundary) {
      Ok(self)
    } else {
      Err(self)
    }
  }

  /// Posts the envelope to the boundary of its recipient.
  pub(crate) fn post_to_boundary(self) {
    let boundary = self.recipient.boundary().clone();
    boundary.post(self);
  }
}
