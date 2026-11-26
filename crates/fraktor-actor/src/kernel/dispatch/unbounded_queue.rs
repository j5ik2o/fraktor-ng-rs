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

//! The kernel's default queue: first in, first out, without a limit.

use alloc::collections::VecDeque;
use alloc::vec::Vec;

use crate::kernel::dispatch::ClassifiedEnvelope;
use crate::kernel::dispatch::MessageQueue;
use crate::kernel::message::Envelope;
use crate::kernel::sync::LockCell;
use crate::kernel::sync::LockFactory;

/// A first-in, first-out queue without a limit, guarded by a lock.
///
/// Mirrors `org.apache.pekko.dispatch.UnboundedMailbox`.
pub struct UnboundedQueue {
  envelopes: LockCell<VecDeque<Envelope>>,
}

impl UnboundedQueue {
  /// Creates an empty queue guarded by a lock from `locks`.
  pub fn new(locks: &dyn LockFactory) -> Self {
    Self { envelopes: LockCell::new(locks.new_lock(), VecDeque::new()) }
  }
}

impl MessageQueue for UnboundedQueue {
  fn enqueue_classified(&self, envelope: ClassifiedEnvelope) -> Result<(), Envelope> {
    let envelope = envelope.into_envelope();
    self.envelopes.with(|queue| queue.push_back(envelope));
    Ok(())
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
    self.envelopes.with(|queue| queue.drain(..).collect())
  }
}
