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

//! The port that makes the queue of each mailbox.

use crate::kernel::dispatch::MessageQueue;
use crate::kernel::sync::Arc;
use crate::kernel::sync::LockFactory;

/// Makes the queue of normal messages for a mailbox.
///
/// Mirrors `org.apache.pekko.dispatch.MailboxType.create`. A dispatcher's
/// settings hold one and call it for each mailbox; a balancing dispatcher
/// calls it once and gives the same queue to every mailbox of its team. The
/// adaptor may pass a queue that takes no lock (std, tokio) or one of fixed
/// length (embassy). `locks` is the
/// factory of the system, for a queue that guards itself with a lock.
pub trait MessageQueueFactory: Send + Sync {
  /// Returns a new, empty queue.
  fn create_queue(&self, locks: &dyn LockFactory) -> Arc<dyn MessageQueue>;
}
