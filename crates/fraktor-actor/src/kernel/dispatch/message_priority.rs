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

//! The priority a priority queue gives a message.

use core::cmp::Ordering;

/// The priority of a message in a [`PriorityQueue`](super::PriorityQueue).
///
/// Mirrors the `Int` that a Pekko `PriorityGenerator` returns: a lower number
/// is a higher priority. The ordering of this type puts the higher priority
/// first, so `a > b` means that `a` leaves the queue before `b`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MessagePriority(i32);

impl MessagePriority {
  /// The priority Pekko's generator writes as `rank`: 0 goes before 1.
  pub const fn new(rank: i32) -> Self {
    Self(rank)
  }

  /// Returns the rank, where a lower number is a higher priority.
  pub const fn rank(self) -> i32 {
    self.0
  }
}

impl PartialOrd for MessagePriority {
  fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
    Some(self.cmp(other))
  }
}

impl Ord for MessagePriority {
  fn cmp(&self, other: &Self) -> Ordering {
    other.0.cmp(&self.0)
  }
}
