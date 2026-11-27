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

//! A first-in, first-out queue that holds at most a given number of envelopes.

use alloc::collections::VecDeque;
use alloc::vec::Vec;

use crate::kernel::dispatch::ClassifiedEnvelope;
use crate::kernel::dispatch::MessageQueue;
use crate::kernel::message::Envelope;
use crate::kernel::sync::LockCell;
use crate::kernel::sync::LockFactory;

/// A first-in, first-out queue with a capacity, guarded by a lock.
///
/// Mirrors `org.apache.pekko.dispatch.BoundedMailbox` with a push time-out of
/// zero: a full queue hands the envelope back at once, and the mailbox sends
/// it to dead letters. Waiting for room needs a condition variable, so the
/// queue that waits lives in the std adaptor.
pub struct BoundedQueue {
  capacity: usize,
  envelopes: LockCell<VecDeque<Envelope>>,
}

impl BoundedQueue {
  /// Creates an empty queue that holds at most `capacity` envelopes, guarded
  /// by a lock from `locks`. A capacity of 0 rejects every envelope.
  pub fn new(capacity: usize, locks: &dyn LockFactory) -> Self {
    Self { capacity, envelopes: LockCell::new(locks.new_lock(), VecDeque::new()) }
  }
}

impl MessageQueue for BoundedQueue {
  fn enqueue_classified(&self, envelope: ClassifiedEnvelope) -> Result<(), Envelope> {
    let envelope = envelope.into_envelope();
    let capacity = self.capacity;
    self.envelopes.with(|queue| {
      if queue.len() < capacity {
        queue.push_back(envelope);
        Ok(())
      } else {
        Err(envelope)
      }
    })
  }

  fn dequeue(&self) -> Option<Envelope> {
    self.envelopes.with(VecDeque::pop_front)
  }

  fn has_messages(&self) -> bool {
    self.envelopes.with(|queue| !queue.is_empty())
  }

  fn number_of_messages(&self) -> usize {
    self.envelopes.with(|queue| queue.len())
  }

  fn drain(&self) -> Vec<Envelope> {
    // The queue is swapped out whole, so a closed mailbox keeps no buffer
    // and no envelope is dropped inside the lock.
    Vec::from(self.envelopes.with(core::mem::take))
  }
}
