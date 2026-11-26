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

//! How much one run of a mailbox may do.

use core::time::Duration;

use crate::kernel::dispatch::Throughput;
use crate::kernel::dispatch::ThroughputDeadline;
use crate::kernel::sync::Arc;
use crate::kernel::Clock;

/// The throughput and the throughput deadline of one run.
///
/// Mirrors `throughput` and `throughput-deadline-time` of a Pekko dispatcher
/// (`reference.conf:622-625`, `Mailbox.processMailbox`). A run handles at most
/// `throughput` normal messages, and stops taking more once the deadline has
/// passed; the message it has taken is always handled to the end.
#[derive(Clone)]
pub struct RunLimits {
  throughput: Throughput,
  deadline: Option<(ThroughputDeadline, Arc<dyn Clock>)>,
}

impl RunLimits {
  /// Limits a run to `throughput` messages, without a deadline.
  pub fn new(throughput: Throughput) -> Self {
    Self { throughput, deadline: None }
  }

  /// Adds a deadline, measured on `clock` from the start of the run.
  #[must_use]
  pub fn with_deadline(mut self, deadline: ThroughputDeadline, clock: Arc<dyn Clock>) -> Self {
    self.deadline = Some((deadline, clock));
    self
  }

  pub(crate) fn throughput(&self) -> Throughput {
    self.throughput
  }

  /// Returns the moment the run must stop taking messages, if any.
  pub(crate) fn deadline_from_now(&self) -> Option<(Duration, &dyn Clock)> {
    self.deadline.as_ref().map(|(deadline, clock)| (deadline.from(clock.monotonic()), &**clock))
  }
}
