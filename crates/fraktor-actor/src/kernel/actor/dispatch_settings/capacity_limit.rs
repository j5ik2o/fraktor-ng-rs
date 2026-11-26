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

//! A capacity that is either unlimited or a positive bound.

use core::num::NonZeroUsize;

/// A capacity that is either unlimited or a positive bound.
///
/// Mirrors the capacity readings of Pekko's `Mailboxes`: a configured
/// capacity of zero or less means an unbounded queue (the default
/// `stash-capacity` is -1), so zero has no representation here and the
/// unlimited case is explicit instead.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CapacityLimit {
  /// No bound: every message fits.
  Unlimited,
  /// At most this many messages fit.
  Limited(NonZeroUsize),
}

impl CapacityLimit {
  /// Turns a configured count into a limit: zero is unlimited, as in
  /// the original's `capacity <= 0` reading, and a positive count is
  /// that bound.
  pub fn from_count(count: usize) -> Self {
    match NonZeroUsize::new(count) {
      | Some(bound) => Self::Limited(bound),
      | None => Self::Unlimited,
    }
  }

  /// Returns the bound, or `None` when unlimited.
  pub fn bound(&self) -> Option<usize> {
    match self {
      | Self::Unlimited => None,
      | Self::Limited(bound) => Some(bound.get()),
    }
  }

  /// Returns whether one more message fits when `current` are held.
  pub fn allows(&self, current: usize) -> bool {
    match self {
      | Self::Unlimited => true,
      | Self::Limited(bound) => current < bound.get(),
    }
  }
}
