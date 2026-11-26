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

//! The router configuration a recipe can carry, as verification sees it.

use core::time::Duration;

/// The pool kinds of `org.apache.pekko.routing`, as typed values.
///
/// The original selects a pool by the class name in a configuration
/// string; the port names the kind with this enum instead (precedents:
/// #154 D1, G0b decision E). The cases of
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

/// A router configuration, as the verification rules see it: a pool,
/// the number of routees, and the dispatcher of the router head.
///
/// Mirrors `org.apache.pekko.routing.RouterConfig` narrowed to what
/// verification reads. The dispatcher of the routees is the recipe's
/// own dispatcher (`Props::with_dispatcher`); the head's dispatcher is
/// `router_dispatcher`, which is what the original's pool
/// `withDispatcher` sets in the ported cases; see the design of #194,
/// section 2.3, rules 2 and 3.
///
/// The router of this issue is minimal: verification runs and the head
/// is created, but no routees are created and no routing happens, so a
/// message to the head is reported as unhandled. Routing itself is
/// issue #269 (https://github.com/j5ik2o/fraktor-ng-rs/issues/269).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct RouterConfig {
  /// The pool the routees would form.
  pub pool: PoolKind,
  /// The number of routees the pool would hold.
  pub nr_of_instances: usize,
  /// The dispatcher of the router head, when one is named.
  pub router_dispatcher: Option<alloc::string::String>,
}
