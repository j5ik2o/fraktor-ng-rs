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

//! Whether a run may take the next message.

/// What [`MailboxCell::invoke`](super::MailboxCell::invoke) tells the run
/// after one normal message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Next {
  /// Nothing is left for after the run: take the next message, up to the
  /// throughput.
  Continue,
  /// Work is left that needs the actor back in its mailbox: end the run as a
  /// spent throughput ends it, after the waiting system messages (design of
  /// #234, section 11.11.2).
  Yield,
}
