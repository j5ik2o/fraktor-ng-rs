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

//! The typed value of one `pekko.actor.deployment` section.

use alloc::string::String;
use alloc::vec::Vec;
use core::time::Duration;

use super::number_of_instances::NumberOfInstances;
use super::resizer_section::ResizerSection;
use super::router_selection::RouterSelection;

/// One section of `pekko.actor.deployment`, as a typed value.
///
/// Mirrors one deployment section together with the `default` section
/// of reference.conf it falls back to: every field carries the default
/// the original would read, and a section is written by overriding the
/// fields it sets. The port has no configuration file, so a `default`
/// section supplied by a user cannot be layered over the others; the
/// `Deployer` skips that key.
#[derive(Clone, PartialEq, Debug)]
pub struct DeploymentSection {
  /// The `router` of the section.
  pub router: RouterSelection,
  /// The `nr-of-instances` of the section.
  pub nr_of_instances: NumberOfInstances,
  /// The `within` of the section; the default is five seconds.
  pub within: Duration,
  /// The `virtual-nodes-factor` of the section; the default is ten.
  /// The pool and group constructors of the original do not read it
  /// for consistent hashing, so it never reaches a result.
  pub virtual_nodes_factor: u32,
  /// The `dispatcher` of the section; empty is `NoDispatcherGiven`.
  pub dispatcher: String,
  /// The `mailbox` of the section; empty is `NoMailboxGiven`.
  pub mailbox: String,
  /// The `resizer` of the section. `Some` is itself what enables the
  /// resizer: the original turns the resizer on whenever a section
  /// holds any of it, even with `enabled = off` written out.
  pub resizer: Option<ResizerSection>,
  /// Whether the section names a `pool-dispatcher`; the original's
  /// `config.hasPath("pool-dispatcher")`.
  pub pool_dispatcher_given: bool,
  /// The `routees.paths` the group kinds read; empty by default.
  pub routees_paths: Vec<String>,
  /// The `tail-chopping-router.interval` of the section; the default
  /// is ten milliseconds.
  pub tail_chopping_interval: Duration,
}

impl Default for DeploymentSection {
  /// The values of reference.conf's `pekko.actor.deployment.default`.
  fn default() -> Self {
    Self {
      router: RouterSelection::FromCode,
      nr_of_instances: NumberOfInstances::Value(1),
      within: Duration::from_secs(5),
      virtual_nodes_factor: 10,
      dispatcher: String::new(),
      mailbox: String::new(),
      resizer: None,
      pool_dispatcher_given: false,
      routees_paths: Vec::new(),
      tail_chopping_interval: Duration::from_millis(10),
    }
  }
}
