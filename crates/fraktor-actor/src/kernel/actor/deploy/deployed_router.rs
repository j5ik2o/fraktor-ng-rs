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

//! The router a deployment section was interpreted into.

use super::default_resizer::DefaultResizer;
use super::group_kind::GroupKind;
use crate::kernel::actor::PoolKind;

/// The interpretation result a [`Deploy`](super::Deploy) holds for its
/// router.
///
/// Mirrors the `routerConfig` of a `Deploy` produced by
/// `Deployer.createRouterConfig`: `NoRouter`, a pool, or a group. The
/// existing [`RouterConfig`](crate::kernel::actor::RouterConfig) of
/// #194 is a narrower shape kept for verification and is not changed;
/// the two are left to be unified in a later issue.
#[derive(Clone, PartialEq, Debug)]
pub enum DeployedRouter {
  /// `router = from-code`. Mirrors `NoRouter`.
  NoRouter,
  /// A pool. Mirrors the `Pool` implementations.
  Pool {
    /// The kind of pool.
    kind: PoolKind,
    /// The number of routees, in the original's `Int` range: a
    /// negative value is held as written.
    nr_of_instances: i32,
    /// The resizer, for the kinds that read one.
    resizer: Option<DefaultResizer>,
    /// Whether the pool uses the pool dispatcher. Balancing pools
    /// never do: the original's constructor does not read it.
    use_pool_dispatcher: bool,
  },
  /// A group. Mirrors the `Group` implementations.
  Group {
    /// The kind of group.
    kind: GroupKind,
    /// The routee paths, from the section's `routees.paths`.
    paths: alloc::vec::Vec<alloc::string::String>,
  },
}
