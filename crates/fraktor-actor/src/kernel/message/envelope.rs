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
use crate::kernel::message::message_value::Message;

/// One delivery: what the message is, who sent it, and, when it travels
/// without a mailbox, who receives it.
///
/// Mirrors `org.apache.pekko.dispatch.Envelope`, which holds the message and
/// the sender: an envelope in a mailbox has no recipient, because the mailbox
/// is the recipient. An envelope that crosses a shard boundary, or that a
/// mailbox turned away, carries its recipient.
pub struct Envelope {
  recipient: Option<ActorRef>,
  message: Message,
  sender: ActorRef,
}

impl Envelope {
  pub(crate) fn new(recipient: ActorRef, message: Message, sender: ActorRef) -> Self {
    Self { recipient: Some(recipient), message, sender }
  }

  /// Creates an envelope for a mailbox, which is its recipient.
  pub(crate) fn without_recipient(message: Message, sender: ActorRef) -> Self {
    Self { recipient: None, message, sender }
  }

  /// Returns the envelope addressed to `recipient`.
  pub(crate) fn with_recipient(self, recipient: ActorRef) -> Self {
    Self { recipient: Some(recipient), ..self }
  }

  pub(crate) fn message(&self) -> &Message {
    &self.message
  }

  pub(crate) fn into_parts(self) -> (Option<ActorRef>, Message, ActorRef) {
    (self.recipient, self.message, self.sender)
  }

  /// Returns the message and the sender, dropping the recipient.
  pub(crate) fn into_message(self) -> (Message, ActorRef) {
    (self.message, self.sender)
  }
}
