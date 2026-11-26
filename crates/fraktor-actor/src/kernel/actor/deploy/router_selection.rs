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

//! The router selection of a deployment section, and the class names
//! it corresponds to.

use alloc::string::String;

/// The fully qualified class name of Pekko's `NoRouter`.
pub const NO_ROUTER_FQCN: &str = "org.apache.pekko.routing.NoRouter";
/// The fully qualified class name of Pekko's `RoundRobinPool`.
pub const ROUND_ROBIN_POOL_FQCN: &str = "org.apache.pekko.routing.RoundRobinPool";
/// The fully qualified class name of Pekko's `RoundRobinGroup`.
pub const ROUND_ROBIN_GROUP_FQCN: &str = "org.apache.pekko.routing.RoundRobinGroup";
/// The fully qualified class name of Pekko's `RandomPool`.
pub const RANDOM_POOL_FQCN: &str = "org.apache.pekko.routing.RandomPool";
/// The fully qualified class name of Pekko's `RandomGroup`.
pub const RANDOM_GROUP_FQCN: &str = "org.apache.pekko.routing.RandomGroup";
/// The fully qualified class name of Pekko's `BalancingPool`.
pub const BALANCING_POOL_FQCN: &str = "org.apache.pekko.routing.BalancingPool";
/// The fully qualified class name of Pekko's `SmallestMailboxPool`.
pub const SMALLEST_MAILBOX_POOL_FQCN: &str = "org.apache.pekko.routing.SmallestMailboxPool";
/// The fully qualified class name of Pekko's `BroadcastPool`.
pub const BROADCAST_POOL_FQCN: &str = "org.apache.pekko.routing.BroadcastPool";
/// The fully qualified class name of Pekko's `BroadcastGroup`.
pub const BROADCAST_GROUP_FQCN: &str = "org.apache.pekko.routing.BroadcastGroup";
/// The fully qualified class name of Pekko's `ScatterGatherFirstCompletedPool`.
pub const SCATTER_GATHER_POOL_FQCN: &str = "org.apache.pekko.routing.ScatterGatherFirstCompletedPool";
/// The fully qualified class name of Pekko's `ScatterGatherFirstCompletedGroup`.
pub const SCATTER_GATHER_GROUP_FQCN: &str = "org.apache.pekko.routing.ScatterGatherFirstCompletedGroup";
/// The fully qualified class name of Pekko's `TailChoppingPool`.
pub const TAIL_CHOPPING_POOL_FQCN: &str = "org.apache.pekko.routing.TailChoppingPool";
/// The fully qualified class name of Pekko's `TailChoppingGroup`.
pub const TAIL_CHOPPING_GROUP_FQCN: &str = "org.apache.pekko.routing.TailChoppingGroup";
/// The fully qualified class name of Pekko's `ConsistentHashingPool`.
pub const CONSISTENT_HASHING_POOL_FQCN: &str = "org.apache.pekko.routing.ConsistentHashingPool";
/// The fully qualified class name of Pekko's `ConsistentHashingGroup`.
pub const CONSISTENT_HASHING_GROUP_FQCN: &str = "org.apache.pekko.routing.ConsistentHashingGroup";

/// The `router` value of a section.
///
/// Mirrors the strings of `pekko.actor.router.type-mapping` as typed
/// values (precedents: #154 D1, G0b decision E, #192 E1). The original
/// maps a short name to a class name and instantiates it by reflection;
/// the port names the kind directly and keeps any other string as
/// [`Custom`](Self::Custom) for the interpretation step to refuse.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum RouterSelection {
  /// `from-code`: the code supplies the router.
  FromCode,
  /// `round-robin-pool`.
  RoundRobinPool,
  /// `round-robin-group`.
  RoundRobinGroup,
  /// `random-pool`.
  RandomPool,
  /// `random-group`.
  RandomGroup,
  /// `balancing-pool`.
  BalancingPool,
  /// `smallest-mailbox-pool`.
  SmallestMailboxPool,
  /// `broadcast-pool`.
  BroadcastPool,
  /// `broadcast-group`.
  BroadcastGroup,
  /// `scatter-gather-pool`.
  ScatterGatherPool,
  /// `scatter-gather-group`.
  ScatterGatherGroup,
  /// `tail-chopping-pool`.
  TailChoppingPool,
  /// `tail-chopping-group`.
  TailChoppingGroup,
  /// `consistent-hashing-pool`.
  ConsistentHashingPool,
  /// `consistent-hashing-group`.
  ConsistentHashingGroup,
  /// Any other string, kept unchecked; in the original it is passed
  /// to reflection as a class name.
  Custom(String),
}

impl RouterSelection {
  /// Turns a router setting string into a selection.
  ///
  /// Mirrors the original's `routerTypeMapping.getOrElse(routerType,
  /// routerType)` followed by reflection: the fifteen short names and
  /// fourteen of the built-in class names select their kind, and every
  /// other string is kept as [`Custom`](Self::Custom). The class name
  /// of `NoRouter` is the exception: the original returns `NoRouter`
  /// only for the string `"from-code"` and fails to instantiate the
  /// class name at startup, so [`NO_ROUTER_FQCN`] is `Custom` here and
  /// the interpretation step refuses it, as the original does.
  pub fn from_setting(value: &str) -> Self {
    match value {
      | "from-code" => Self::FromCode,
      | "round-robin-pool" | ROUND_ROBIN_POOL_FQCN => Self::RoundRobinPool,
      | "round-robin-group" | ROUND_ROBIN_GROUP_FQCN => Self::RoundRobinGroup,
      | "random-pool" | RANDOM_POOL_FQCN => Self::RandomPool,
      | "random-group" | RANDOM_GROUP_FQCN => Self::RandomGroup,
      | "balancing-pool" | BALANCING_POOL_FQCN => Self::BalancingPool,
      | "smallest-mailbox-pool" | SMALLEST_MAILBOX_POOL_FQCN => Self::SmallestMailboxPool,
      | "broadcast-pool" | BROADCAST_POOL_FQCN => Self::BroadcastPool,
      | "broadcast-group" | BROADCAST_GROUP_FQCN => Self::BroadcastGroup,
      | "scatter-gather-pool" | SCATTER_GATHER_POOL_FQCN => Self::ScatterGatherPool,
      | "scatter-gather-group" | SCATTER_GATHER_GROUP_FQCN => Self::ScatterGatherGroup,
      | "tail-chopping-pool" | TAIL_CHOPPING_POOL_FQCN => Self::TailChoppingPool,
      | "tail-chopping-group" | TAIL_CHOPPING_GROUP_FQCN => Self::TailChoppingGroup,
      | "consistent-hashing-pool" | CONSISTENT_HASHING_POOL_FQCN => Self::ConsistentHashingPool,
      | "consistent-hashing-group" | CONSISTENT_HASHING_GROUP_FQCN => Self::ConsistentHashingGroup,
      | other => Self::Custom(String::from(other)),
    }
  }

  /// Returns the class name of the selected router.
  ///
  /// Mirrors the values of `routerTypeMapping`: the built-in
  /// selections answer their constant and `Custom` answers the string
  /// it keeps. `FromCode` answers [`NO_ROUTER_FQCN`], which
  /// [`from_setting`](Self::from_setting) does not accept back; the
  /// round trip holds for every other selection.
  pub fn fqcn(&self) -> &str {
    match self {
      | Self::FromCode => NO_ROUTER_FQCN,
      | Self::RoundRobinPool => ROUND_ROBIN_POOL_FQCN,
      | Self::RoundRobinGroup => ROUND_ROBIN_GROUP_FQCN,
      | Self::RandomPool => RANDOM_POOL_FQCN,
      | Self::RandomGroup => RANDOM_GROUP_FQCN,
      | Self::BalancingPool => BALANCING_POOL_FQCN,
      | Self::SmallestMailboxPool => SMALLEST_MAILBOX_POOL_FQCN,
      | Self::BroadcastPool => BROADCAST_POOL_FQCN,
      | Self::BroadcastGroup => BROADCAST_GROUP_FQCN,
      | Self::ScatterGatherPool => SCATTER_GATHER_POOL_FQCN,
      | Self::ScatterGatherGroup => SCATTER_GATHER_GROUP_FQCN,
      | Self::TailChoppingPool => TAIL_CHOPPING_POOL_FQCN,
      | Self::TailChoppingGroup => TAIL_CHOPPING_GROUP_FQCN,
      | Self::ConsistentHashingPool => CONSISTENT_HASHING_POOL_FQCN,
      | Self::ConsistentHashingGroup => CONSISTENT_HASHING_GROUP_FQCN,
      | Self::Custom(name) => name,
    }
  }
}
