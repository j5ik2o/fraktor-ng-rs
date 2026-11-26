// SPDX-License-Identifier: Apache-2.0
//
// Licensed to the Apache Software Foundation (ASF) under one or more
// license agreements; and to You under the Apache License, version 2.0:
//
//   https://www.apache.org/licenses/LICENSE-2.0
//
// This file is part of the Apache Pekko project, which was derived from Akka.
//
// Copyright (C) 2018-2022 Lightbend Inc. <https://www.lightbend.com>
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.

//! The error a stash raises when a message does not fit its capacity.
//!
//! Mirrors `org.apache.pekko.actor.StashOverflowException`, the
//! failure the original raises from a bounded stash.

use alloc::string::String;
use core::fmt;

/// The failure of one `stash()` call whose stash is already at capacity.
///
/// Mirrors `org.apache.pekko.actor.StashOverflowException`. Pekko throws
/// it from `Stash.stash` when the stash size reaches the configured
/// `stash-capacity`; the port returns it from
/// [`ActorContext::try_stash`](crate::kernel::actor::ActorContext::try_stash)
/// so the actor can react without unwinding, and the plain
/// [`ActorContext::stash`](crate::kernel::actor::ActorContext::stash)
/// panics with it, as an uncaught throw reaches the supervisor in Pekko.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct StashOverflowException {
  message: String,
}

impl StashOverflowException {
  /// Creates the exception for a stash whose capacity is `capacity`.
  pub fn new(capacity: usize) -> Self {
    Self { message: alloc::format!("stash capacity ({capacity}) exceeded") }
  }

  /// Returns the description of the failure.
  pub fn message(&self) -> &str {
    &self.message
  }
}

impl fmt::Display for StashOverflowException {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str(&self.message)
  }
}

impl core::error::Error for StashOverflowException {}
