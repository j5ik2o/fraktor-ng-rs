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

//! One piece of work that waits for the dispatcher a test drives by hand.

use crate::kernel::dispatch::MailboxRun;
use crate::kernel::message::Envelope;

/// What waits in a [`ManualExecutor`](super::ManualExecutor).
pub(crate) enum ManualTask<C> {
  /// A run of a mailbox that won its scheduled bit.
  Run(MailboxRun<C>),
  /// An envelope the executor delivers itself: one that came across the
  /// shard boundary, or one a closed or full mailbox with no refused route
  /// installed turned away, which goes to dead letters when the executor
  /// reaches it.
  /// A mailbox of a spawned actor has a refused route, so its refused
  /// envelopes go to dead letters on the sender's thread instead (issue
  /// #286).
  Envelope(Envelope),
}
