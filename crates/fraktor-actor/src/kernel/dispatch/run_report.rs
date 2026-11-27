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

//! What one run of a mailbox did, and what it leaves.

use crate::kernel::dispatch::RunOutcome;

/// The end of one [`MailboxRun::run`](super::MailboxRun::run).
pub(crate) struct RunReport<C> {
  /// Whether the mailbox is to run again.
  pub(crate) outcome: RunOutcome<C>,
  /// How many normal messages the run took from the queue. A run that handles
  /// only system messages, or finds nothing, takes none.
  pub(crate) delivered: usize,
}
