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

//! The queue of one shard and the boundary of the shard that follows it.

use alloc::collections::VecDeque;
use alloc::vec::Vec;

use crate::kernel::actor_ref::ActorRef;
use crate::kernel::executor::ShardBoundary;
use crate::kernel::message::Envelope;
use crate::kernel::sync::Arc;

/// The place a message sent from inside a shard lands, and the escape out of
/// it.
///
/// Pekko has one mailbox per actor, so a message reaches an actor only through
/// that mailbox. The temporary policy of this port keeps a single local queue
/// per shard, so a message may reach the shard's queue either directly (from
/// an actor of the same shard) or through the boundary (from a handle). Those
/// two ways could reorder messages from one sender to one recipient, which
/// Pekko promises never happens. This type keeps the promise: a message that
/// stays local is queued behind every envelope the same shard has already
/// posted to the boundary, because a sender that posted first must not be
/// overtaken by its own later message.
pub(crate) struct ShardQueue {
  local: VecDeque<Envelope>,
  boundary: Arc<dyn ShardBoundary>,
}

impl ShardQueue {
  pub(crate) fn new(boundary: Arc<dyn ShardBoundary>) -> Self {
    Self { local: VecDeque::new(), boundary }
  }

  pub(crate) fn boundary(&self) -> &Arc<dyn ShardBoundary> {
    &self.boundary
  }

  /// Queues a message that an actor of this shard sent to another actor of
  /// this shard, unless an earlier envelope is still waiting at the boundary.
  ///
  /// The order is kept per shard rather than per sender because the local
  /// queue is shared. That is stricter than Pekko, and it costs a turn at the
  /// boundary only when the boundary is not empty, so a sender to a shard
  /// that is already busy waits for the boundary just as Pekko would wait for
  /// the mailbox of the recipient.
  pub(crate) fn enqueue_locally(&mut self, envelope: Envelope) {
    if self.boundary.has_pending() {
      self.boundary.post(envelope);
    } else {
      self.local.push_back(envelope);
    }
  }

  /// Takes at most one envelope from the boundary and appends it locally.
  pub(crate) fn take_one_from_boundary(&mut self) -> bool {
    match self.boundary.poll() {
      | Some(envelope) => {
        self.local.push_back(envelope);
        true
      },
      | None => false,
    }
  }

  /// Waits at most `max` for one envelope from the boundary and appends it
  /// locally.
  pub(crate) fn await_one_from_boundary(&mut self, max: core::time::Duration) -> bool {
    match self.boundary.poll_within(max) {
      | Some(envelope) => {
        self.local.push_back(envelope);
        true
      },
      | None => false,
    }
  }

  /// Puts `envelopes` at the front of the local queue, keeping their order.
  ///
  /// Mirrors `DequeBasedMessageQueue.enqueueFirst`, which `unstashAll` uses:
  /// the envelopes were received before anything still queued.
  pub(crate) fn prepend_locally(&mut self, envelopes: Vec<Envelope>) {
    for envelope in envelopes.into_iter().rev() {
      self.local.push_front(envelope);
    }
  }

  /// Removes the locally queued envelopes addressed to `recipient`, keeping
  /// their order and the order of the rest.
  pub(crate) fn take_addressed_to(&mut self, recipient: &ActorRef) -> Vec<Envelope> {
    let mut taken = Vec::new();
    let mut kept = VecDeque::with_capacity(self.local.len());
    for envelope in self.local.drain(..) {
      if envelope.is_addressed_to(recipient) {
        taken.push(envelope);
      } else {
        kept.push_back(envelope);
      }
    }
    self.local = kept;
    taken
  }

  /// Counts the locally queued envelopes addressed to `recipient`.
  pub(crate) fn count_addressed_to(&self, recipient: &ActorRef) -> usize {
    self.local.iter().filter(|envelope| envelope.is_addressed_to(recipient)).count()
  }

  /// Pops the oldest queued envelope.
  pub(crate) fn pop(&mut self) -> Option<Envelope> {
    self.local.pop_front()
  }

  /// Drops every queued envelope of the shard.
  pub(crate) fn clear(&mut self) {
    self.local.clear();
  }
}
