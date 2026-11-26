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

use crate::kernel::dispatch::ClassifiedEnvelope;
use crate::kernel::dispatch::MessagePriority;
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
/// Putting an envelope on the queue takes two steps, so that no user code runs
/// inside a lock (invariant I16): [`classify`](Self::classify) runs outside
/// every lock and may call a function the user gave the queue;
/// [`enqueue_classified`](Self::enqueue_classified) runs inside the queue's
/// lock, and also inside the gate of a mailbox with a queue of its own, so it
/// only moves the envelope in and never drops one there.
pub trait MessageQueue: Send + Sync {
  /// Gives `envelope` its place before it is queued. The default gives every
  /// envelope the same priority, for a queue that keeps arrival order.
  fn classify(&self, envelope: Envelope) -> ClassifiedEnvelope {
    ClassifiedEnvelope::new(envelope, MessagePriority::new(0))
  }

  /// Puts a classified envelope on the queue, or hands the envelope back when
  /// the queue has no room. A rejected envelope goes to dead letters.
  ///
  /// # Errors
  ///
  /// Returns the envelope when a bounded queue is full.
  fn enqueue_classified(&self, envelope: ClassifiedEnvelope) -> Result<(), Envelope>;

  /// Classifies `envelope` and puts it on the queue.
  ///
  /// # Errors
  ///
  /// Returns the envelope when a bounded queue is full.
  fn enqueue(&self, envelope: Envelope) -> Result<(), Envelope> {
    self.enqueue_classified(self.classify(envelope))
  }

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

  /// Puts `envelopes` at the front of the queue, keeping their order, ahead
  /// of everything already queued (Pekko's `DequeBasedMessageQueue.enqueueFirst`,
  /// which `unstashAll` uses). The default cannot, and hands them back.
  ///
  /// # Errors
  ///
  /// Returns the envelopes when the queue cannot put them first.
  fn enqueue_first(&self, envelopes: Vec<Envelope>) -> Result<(), Vec<Envelope>> {
    Err(envelopes)
  }
}
