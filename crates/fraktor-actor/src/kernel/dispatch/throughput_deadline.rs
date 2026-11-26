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

//! How long one run of a mailbox may keep taking messages.

use core::time::Duration;

/// The time after which a run takes no further message.
///
/// Mirrors `throughput-deadline-time` of a Pekko dispatcher
/// (`reference.conf:625`). Pekko treats a deadline that is not at least one
/// millisecond as no deadline (`AbstractDispatcher.scala:372`), so
/// [`new`](Self::new) gives none for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ThroughputDeadline(Duration);

impl ThroughputDeadline {
  /// A deadline of `duration`, or none when it is under one millisecond.
  pub fn new(duration: Duration) -> Option<Self> {
    if duration >= Duration::from_millis(1) {
      Some(Self(duration))
    } else {
      None
    }
  }

  /// Returns the moment the run must stop taking messages, when it starts at
  /// `start`.
  pub(crate) fn from(self, start: Duration) -> Duration {
    start + self.0
  }
}
