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

//! The queue of normal messages that a mailbox reads.

use alloc::vec::Vec;

use crate::kernel::message::Envelope;

#[cfg(test)]
#[path = "message_queue_test.rs"]
mod tests;

/// The queue of normal messages behind one mailbox, or behind the mailboxes
/// of a balancing team that share it.
///
/// Mirrors `org.apache.pekko.dispatch.MessageQueue`. The queue does not know
/// whether its mailbox is closed: the mailbox's gate decides that, so that a
/// queue can be shared by mailboxes that close one by one (design of #234,
/// section 3.2, "the gate"). The adaptor may pass its own queue through a
/// [`MessageQueueFactory`](super::MessageQueueFactory).
///
/// An implementation must not run user code inside its lock: it moves
/// envelopes in and out and never drops one there (invariant I16). A mailbox
/// with a queue of its own calls `enqueue` inside its gate, so `enqueue` must
/// not call back into any mailbox.
pub trait MessageQueue: Send + Sync {
  /// Puts `envelope` at the end of the queue, or hands it back when the queue
  /// has no room. A rejected envelope goes to dead letters.
  ///
  /// # Errors
  ///
  /// Returns the envelope when a bounded queue is full.
  fn enqueue(&self, envelope: Envelope) -> Result<(), Envelope>;

  /// Takes the next envelope, if there is one.
  fn dequeue(&self) -> Option<Envelope>;

  /// Returns whether the queue holds an envelope.
  fn has_messages(&self) -> bool;

  /// Returns how many envelopes the queue holds.
  fn number_of_messages(&self) -> usize;

  /// Takes every envelope, in the order `dequeue` would return them.
  ///
  /// A mailbox with a queue of its own calls this when it closes and sends
  /// what it gets to dead letters (Pekko's `cleanUp`).
  fn drain(&self) -> Vec<Envelope>;
}
