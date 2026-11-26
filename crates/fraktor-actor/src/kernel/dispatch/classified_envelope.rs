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

//! An envelope with the place its queue gave it, before it is queued.

use crate::kernel::dispatch::MessagePriority;
use crate::kernel::message::Envelope;

/// An envelope and the priority its queue gave it.
///
/// A queue that orders by a function of the message runs that function in
/// [`MessageQueue::classify`](super::MessageQueue::classify), before any lock
/// is taken, and keeps the result here for
/// [`MessageQueue::enqueue_classified`](super::MessageQueue::enqueue_classified),
/// which runs inside the locks. A queue that keeps arrival order ignores the
/// priority.
pub struct ClassifiedEnvelope {
  envelope: Envelope,
  priority: MessagePriority,
}

impl ClassifiedEnvelope {
  /// Binds `envelope` to the `priority` its queue gave it.
  pub fn new(envelope: Envelope, priority: MessagePriority) -> Self {
    Self { envelope, priority }
  }

  /// Returns the priority the queue gave the envelope.
  pub fn priority(&self) -> MessagePriority {
    self.priority
  }

  /// Returns the envelope.
  pub fn into_envelope(self) -> Envelope {
    self.envelope
  }
}
