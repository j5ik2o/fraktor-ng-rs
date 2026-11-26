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

//! The deployment of one actor path, as a value.

use alloc::string::String;

use super::deployed_router::DeployedRouter;
use super::deployment_section::DeploymentSection;
use super::scope::Scope;

/// A deployment: the section that was read for a path, the router it
/// was interpreted into, and the scope, dispatcher and mailbox it
/// assigns.
///
/// Mirrors `org.apache.pekko.actor.Deploy`. The original's `tags` are
/// not held: no case of this port reads them, and the stage that needs
/// them adds them then.
#[derive(Clone, PartialEq, Debug)]
pub struct Deploy {
  /// The key of the section, wildcards included, as written.
  pub path: String,
  /// The section the deployment was read from, defaults applied.
  pub section: DeploymentSection,
  /// The router the section was interpreted into.
  pub router: DeployedRouter,
  /// The scope of the deployment.
  pub scope: Scope,
  /// The dispatcher name; empty is `NoDispatcherGiven`.
  pub dispatcher: String,
  /// The mailbox name; empty is `NoMailboxGiven`.
  pub mailbox: String,
}
