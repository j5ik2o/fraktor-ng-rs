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

//! The pool kinds of the routing package, as typed values.

use core::time::Duration;

/// The pool kinds of `org.apache.pekko.routing`, as typed values.
///
/// The original selects a pool by the class name in a configuration
/// string; the port names the kind with this enum instead. The cases of
/// `ActorConfigurationVerificationSpec` guard the meaning — which pool
/// combined with which dispatcher is refused — not the class loading.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum PoolKind {
  /// Mirrors `RoundRobinPool`: routees take turns.
  RoundRobin,
  /// Mirrors `BroadcastPool`: every routee receives every message.
  Broadcast,
  /// Mirrors `RandomPool`: one routee is chosen at random.
  Random,
  /// Mirrors `SmallestMailboxPool`: the routee with the emptiest
  /// mailbox is chosen.
  SmallestMailbox,
  /// Mirrors `ScatterGatherFirstCompletedPool`: every routee receives
  /// the message and the first reply wins, within `within`.
  ScatterGatherFirstCompleted {
    /// The time the head waits for the first reply.
    within: Duration,
  },
}
