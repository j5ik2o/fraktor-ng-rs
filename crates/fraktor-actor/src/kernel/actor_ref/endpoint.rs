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

//! Where a handle sends its messages.

use crate::kernel::dispatch::Mailbox;
use crate::kernel::executor::ActorCell;
use crate::kernel::executor::ShardBoundary;
use crate::kernel::sync::Arc;

/// Where the messages of one handle go.
pub(crate) enum Endpoint {
  /// The mailbox of an actor with a cell: a message goes straight onto its
  /// queue, as in Pekko, where an `ActorRef` reaches its `ActorCell`'s
  /// mailbox.
  Mailbox(Arc<Mailbox<ActorCell>>),
  /// A shard boundary: a function reference, a no-op handle, a guardian, or
  /// a path that resolved to no actor.
  Boundary(Arc<dyn ShardBoundary>),
}

impl Endpoint {
  /// Returns whether `self` and `other` send to the same place.
  pub(crate) fn same_as(&self, other: &Self) -> bool {
    match (self, other) {
      | (Self::Mailbox(mine), Self::Mailbox(theirs)) => Arc::ptr_eq(mine, theirs),
      | (Self::Boundary(mine), Self::Boundary(theirs)) => Arc::ptr_eq(mine, theirs),
      | _ => false,
    }
  }
}

// Written by hand, so that a test can count the clones.
impl Clone for Endpoint {
  fn clone(&self) -> Self {
    match self {
      | Self::Mailbox(mailbox) => {
        // syn-check: allow(tests-location) -- 所有権モデルの条件1の数え上げ（設計 §11.11.7、オーナーの許可 2026-11-27、元帥の項目472）
        #[cfg(test)]
        crate::kernel::sync::counts::count_mailbox_endpoint();
        Self::Mailbox(Arc::clone(mailbox))
      },
      | Self::Boundary(boundary) => {
        // syn-check: allow(tests-location) -- 所有権モデルの条件1の数え上げ（設計 §11.11.7、オーナーの許可 2026-11-27、元帥の項目472）
        #[cfg(test)]
        crate::kernel::sync::counts::count_boundary_endpoint();
        Self::Boundary(Arc::clone(boundary))
      },
    }
  }
}
