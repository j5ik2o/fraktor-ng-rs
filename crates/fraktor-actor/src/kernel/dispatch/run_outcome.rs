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

//! What one run of a mailbox leaves for the adaptor to do.

use crate::kernel::dispatch::MailboxRun;

/// The end of one [`MailboxRun::run`].
pub(crate) enum RunOutcome<C> {
  /// The run reserved the mailbox again and still holds its scheduled bit.
  /// The adaptor hands this run back to the pool as it is, without touching
  /// the bit: reserving again would lose to its own bit and drop the work
  /// (Devin's review on PR #236).
  ///
  /// When the pool refuses it, the adaptor treats it as any refused
  /// submission: it tries once more, and if
  /// that is refused too, clears the bit and reports the error. Clearing the
  /// bit comes with the pool (stage 5, invariant I15).
  Again(MailboxRun<C>),
  /// The mailbox needs no further run from this one.
  Idle,
}
