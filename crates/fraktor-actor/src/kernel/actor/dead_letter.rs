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

//! The event a message becomes when it reaches dead letters.
//!
//! Mirrors `org.apache.pekko.actor.DeadLetter`, the event stream
//! event published for a message that could not be delivered.

use crate::kernel::actor_ref::ActorRef;
use crate::kernel::message::Message;

/// A message that was sent to dead letters, with its sender and the
/// actor it was addressed to.
///
/// Mirrors `org.apache.pekko.actor.DeadLetter`. The executor publishes
/// one on the event stream for each message it hands to dead letters
/// from the bounded stash paths (the overflow of `unstash_all`, the
/// stash of a stopping actor, and the queue of a stopping actor), so a
/// test subscribes to it as the original's cases do. No `PartialEq` is
/// derived: `Message` has none, so a test reads the parts through the
/// accessors instead, as with
/// [`UnhandledMessage`](crate::kernel::actor::UnhandledMessage).
#[derive(Clone, Debug)]
pub struct DeadLetter {
  message: Message,
  sender: ActorRef,
  recipient: ActorRef,
}

impl DeadLetter {
  /// Creates the event for `message` from `sender` to `recipient`.
  pub fn new(message: Message, sender: ActorRef, recipient: ActorRef) -> Self {
    Self { message, sender, recipient }
  }

  /// Returns the message that reached dead letters.
  pub fn message(&self) -> &Message {
    &self.message
  }

  /// Returns the sender of the message.
  pub fn sender(&self) -> ActorRef {
    self.sender.clone()
  }

  /// Returns the actor the message was addressed to.
  pub fn recipient(&self) -> ActorRef {
    self.recipient.clone()
  }
}
