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

//! The port a waiting caller sleeps on until work may have come.

use core::time::Duration;

/// A signal that wakes a caller waiting for work for the actor system.
///
/// A test drives a system by hand, on its own thread, while other threads may
/// send to the actors. The kernel cannot block a thread, so the adaptor gives
/// it this port: whatever may bring work [`notify`](Self::notify)s, and
/// `await_envelope` [`wait`](Self::wait)s. A shard boundary hands it out
/// through `ShardBoundary::wake_signal` (design of #234, section 11.11.6).
pub trait WakeSignal: Send + Sync {
  /// Raises the signal and wakes a caller that waits on it. A raise that
  /// comes before the wait is not lost: the next wait returns at once.
  fn notify(&self);

  /// Waits until the signal is raised or `max` has passed, and lowers it.
  /// Returns whether it was raised.
  fn wait(&self, max: Duration) -> bool;
}
