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

//! The capacities one dispatcher or mailbox definition sets.

use super::CapacityLimit;

/// The capacities one dispatcher or mailbox definition sets.
///
/// Mirrors the parts of a Pekko dispatcher or mailbox configuration
/// section that the bounded stash reads: the mailbox capacity (the
/// `mailbox-capacity` of a bounded mailbox type) and the
/// `stash-capacity`. A `None` field is "not set in this section", which
/// the resolution keeps apart from an explicit
/// [`CapacityLimit::Unlimited`].
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct MailboxDefinition {
  /// The capacity of the mailbox itself, when this section sets one.
  pub mailbox_capacity: Option<CapacityLimit>,
  /// The capacity of the stash of an actor using this section, when set.
  pub stash_capacity: Option<CapacityLimit>,
}

impl MailboxDefinition {
  /// Creates a definition from its two optional capacities.
  pub fn new(mailbox_capacity: Option<CapacityLimit>, stash_capacity: Option<CapacityLimit>) -> Self {
    Self { mailbox_capacity, stash_capacity }
  }

  /// Creates a definition with both capacities given as counts, where
  /// zero means unlimited, as a Pekko configuration value would read.
  pub fn bounded(mailbox_capacity: usize, stash_capacity: usize) -> Self {
    Self {
      mailbox_capacity: Some(CapacityLimit::from_count(mailbox_capacity)),
      stash_capacity: Some(CapacityLimit::from_count(stash_capacity)),
    }
  }
}
