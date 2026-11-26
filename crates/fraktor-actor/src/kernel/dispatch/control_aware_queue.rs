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

//! A queue that hands out control messages before the others.

use alloc::boxed::Box;
use alloc::collections::VecDeque;
use alloc::vec::Vec;

use crate::kernel::dispatch::ClassifiedEnvelope;
use crate::kernel::dispatch::MessagePriority;
use crate::kernel::dispatch::MessageQueue;
use crate::kernel::message::Envelope;
use crate::kernel::message::Message;
use crate::kernel::sync::LockCell;
use crate::kernel::sync::LockFactory;

/// Two first-in, first-out queues without a limit: control messages, which go
/// out first, and the rest.
///
/// Mirrors `org.apache.pekko.dispatch.UnboundedControlAwareMailbox`. Pekko marks
/// a control message with the `ControlMessage` trait; a message here is type
/// erased, so the queue asks a function instead.
///
/// The function must be a pure function of the message, as a Pekko
/// `ControlMessage` test is. The queue calls it in
/// [`classify`](super::MessageQueue::classify), outside every lock.
pub struct ControlAwareQueue {
  is_control: Box<dyn Fn(&Message) -> bool + Send + Sync>,
  queues: LockCell<Queues>,
}

const CONTROL: MessagePriority = MessagePriority::new(0);
const NORMAL: MessagePriority = MessagePriority::new(1);

#[derive(Default)]
struct Queues {
  control: VecDeque<Envelope>,
  normal: VecDeque<Envelope>,
}

impl ControlAwareQueue {
  /// Creates an empty queue that sends the messages `is_control` accepts
  /// ahead of the others.
  pub fn new(is_control: impl Fn(&Message) -> bool + Send + Sync + 'static, locks: &dyn LockFactory) -> Self {
    Self { is_control: Box::new(is_control), queues: LockCell::new(locks.new_lock(), Queues::default()) }
  }
}

impl MessageQueue for ControlAwareQueue {
  // The predicate is user code, so it runs here, outside every lock
  // (invariant I16). A control message gets the higher priority.
  fn classify(&self, envelope: Envelope) -> ClassifiedEnvelope {
    let priority = if (self.is_control)(envelope.message()) { CONTROL } else { NORMAL };
    ClassifiedEnvelope::new(envelope, priority)
  }

  fn enqueue_classified(&self, envelope: ClassifiedEnvelope) -> Result<(), Envelope> {
    let control = envelope.priority() == CONTROL;
    let envelope = envelope.into_envelope();
    self
      .queues
      .with(|queues| if control { queues.control.push_back(envelope) } else { queues.normal.push_back(envelope) });
    Ok(())
  }

  fn dequeue(&self) -> Option<Envelope> {
    self.queues.with(|queues| queues.control.pop_front().or_else(|| queues.normal.pop_front()))
  }

  fn has_messages(&self) -> bool {
    self.queues.with(|queues| !queues.control.is_empty() || !queues.normal.is_empty())
  }

  fn number_of_messages(&self) -> usize {
    self.queues.with(|queues| queues.control.len() + queues.normal.len())
  }

  fn drain(&self) -> Vec<Envelope> {
    self.queues.with(|queues| queues.control.drain(..).chain(queues.normal.drain(..)).collect())
  }
}
