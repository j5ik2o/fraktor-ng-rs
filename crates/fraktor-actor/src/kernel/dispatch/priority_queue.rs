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

//! A queue that hands out the envelope of the highest priority first.

use alloc::boxed::Box;
use alloc::collections::BinaryHeap;
use alloc::vec::Vec;
use core::cmp::Ordering;
use core::cmp::Reverse;

use crate::kernel::dispatch::MessagePriority;
use crate::kernel::dispatch::MessageQueue;
use crate::kernel::message::Envelope;
use crate::kernel::message::Message;
use crate::kernel::sync::LockCell;
use crate::kernel::sync::LockFactory;

/// A queue ordered by a priority that a function gives each message, without
/// a limit, guarded by a lock.
///
/// Mirrors `org.apache.pekko.dispatch.UnboundedPriorityMailbox` and
/// `UnboundedStablePriorityMailbox` with a `PriorityGenerator`. A stable queue hands out envelopes of the same
/// priority in the order they came; a queue that is not stable makes no
/// promise about them, as Pekko's `PriorityBlockingQueue` makes none.
///
/// The function must be a pure function of the message. A mailbox with a
/// queue of its own ([`Mailbox::new`](super::Mailbox::new)) calls `enqueue`
/// inside its gate, so the function runs while the gate is held: it must not
/// send a message, and must not call any method of a mailbox. Reaching the
/// same gate again would never return (a spin lock spins, a thread lock
/// blocks).
pub struct PriorityQueue {
  priority: Box<dyn Fn(&Message) -> MessagePriority + Send + Sync>,
  stable: bool,
  state: LockCell<Heap>,
}

struct Heap {
  entries: BinaryHeap<Entry>,
  next_seq: u64,
}

// Ordered so that the heap's maximum is the next envelope out: the highest
// priority, then (in a stable queue) the lowest sequence number.
struct Entry {
  key: (MessagePriority, Reverse<u64>),
  envelope: Envelope,
}

impl PartialEq for Entry {
  fn eq(&self, other: &Self) -> bool {
    self.key == other.key
  }
}

impl Eq for Entry {}

impl PartialOrd for Entry {
  fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
    Some(self.cmp(other))
  }
}

impl Ord for Entry {
  fn cmp(&self, other: &Self) -> Ordering {
    self.key.cmp(&other.key)
  }
}

impl PriorityQueue {
  /// Creates an empty queue that orders messages by `priority`, lowest first.
  /// When `stable` is true, messages of the same priority keep their order.
  pub fn new(
    priority: impl Fn(&Message) -> MessagePriority + Send + Sync + 'static,
    stable: bool,
    locks: &dyn LockFactory,
  ) -> Self {
    Self {
      priority: Box::new(priority),
      stable,
      state: LockCell::new(locks.new_lock(), Heap { entries: BinaryHeap::new(), next_seq: 0 }),
    }
  }
}

impl MessageQueue for PriorityQueue {
  fn enqueue(&self, envelope: Envelope) -> Result<(), Envelope> {
    // The priority function is user code, so it runs before the lock is taken.
    let priority = (self.priority)(envelope.message());
    let stable = self.stable;
    self.state.with(|heap| {
      let seq = if stable { heap.next_seq } else { 0 };
      heap.next_seq += 1;
      heap.entries.push(Entry { key: (priority, Reverse(seq)), envelope });
    });
    Ok(())
  }

  fn dequeue(&self) -> Option<Envelope> {
    self.state.with(|heap| heap.entries.pop().map(|entry| entry.envelope))
  }

  fn has_messages(&self) -> bool {
    self.state.with(|heap| !heap.entries.is_empty())
  }

  fn number_of_messages(&self) -> usize {
    self.state.with(|heap| heap.entries.len())
  }

  fn drain(&self) -> Vec<Envelope> {
    self.state.with(|heap| {
      let mut drained = Vec::with_capacity(heap.entries.len());
      while let Some(entry) = heap.entries.pop() {
        drained.push(entry.envelope);
      }
      drained
    })
  }
}
