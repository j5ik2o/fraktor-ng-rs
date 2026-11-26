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

//! The port that carries messages across a shard boundary.

use core::time::Duration;

use crate::kernel::dispatch::WakeSignal;
use crate::kernel::message::Envelope;
use crate::kernel::sync::Arc;

/// The transport that carries the messages that have no mailbox to go to.
///
/// A message to an actor goes onto the actor's mailbox. A boundary carries
/// the rest: messages to a function reference, a guardian, a path that
/// resolved to no actor, and a selection that the system resolves when it
/// delivers it. Pekko has no direct equivalent: it always uses a mailbox and
/// a dispatcher. The port keeps the kernel free of threading and IO, and the
/// standard-library implementation lives in the adaptor.
pub trait ShardBoundary: Send + Sync {
  /// Reports elapsed monotonic time when the boundary supplies platform time.
  ///
  /// A system using a custom boundary without a clock can install one before
  /// starting actors whose supervisor counts restarts in a finite window.
  fn monotonic_time(&self) -> Option<Duration> {
    None
  }

  /// Posts one envelope for another shard to pick up.
  fn post(&self, envelope: Envelope);

  /// Takes one envelope if one is already waiting.
  fn poll(&self) -> Option<Envelope>;

  /// Waits at most `max` for one envelope.
  fn poll_within(&self, max: Duration) -> Option<Envelope>;

  /// Returns whether an envelope is already waiting on this boundary.
  fn has_pending(&self) -> bool;

  /// Returns the signal that wakes a caller waiting for work for the system
  /// of this boundary, when the boundary has one. The boundary raises it when
  /// an envelope is posted; the system raises it when a mailbox has work or
  /// a function reference has handled a message (design of #234, section
  /// 11.11.6). Without one, a wait polls the boundary.
  fn wake_signal(&self) -> Option<Arc<dyn WakeSignal>> {
    None
  }

  /// Stops the boundary from taking new envelopes.
  ///
  /// After this call, `post` discards the envelope instead of handing it on,
  /// so a terminated shard cannot accumulate messages that would keep its
  /// handles, and through them the boundary, alive. Envelopes already waiting
  /// can still be taken by `poll`, so the shard can drain them before it goes
  /// away.
  fn close(&self);
}
