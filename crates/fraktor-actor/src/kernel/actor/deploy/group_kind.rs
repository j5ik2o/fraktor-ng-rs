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

//! The group kinds of the routing package, as typed values.

use core::time::Duration;

/// The group kinds of `org.apache.pekko.routing`, as typed values.
///
/// Groups read only `routees.paths` from a section, plus the values
/// their own kind is built from here.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GroupKind {
  /// Mirrors `RoundRobinGroup`: routees take turns.
  RoundRobin,
  /// Mirrors `RandomGroup`: one routee is chosen at random.
  Random,
  /// Mirrors `BroadcastGroup`: every routee receives every message.
  Broadcast,
  /// Mirrors `ScatterGatherFirstCompletedGroup`: every routee
  /// receives the message and the first reply wins, within `within`.
  ScatterGatherFirstCompleted {
    /// The time the head waits for the first reply.
    within: Duration,
  },
  /// Mirrors `TailChoppingGroup`: the message goes to one routee and,
  /// until a reply arrives, again to another every `interval`.
  TailChopping {
    /// The time the head waits for a reply.
    within: Duration,
    /// The interval between sends.
    interval: Duration,
  },
  /// Mirrors `ConsistentHashingGroup`: the routee is chosen by the
  /// hash of the message. The factor is always zero: the original's
  /// `Config` constructor reads only `routees.paths`.
  ConsistentHashing {
    /// The virtual nodes factor; always zero from a section.
    virtual_nodes_factor: u32,
  },
}
