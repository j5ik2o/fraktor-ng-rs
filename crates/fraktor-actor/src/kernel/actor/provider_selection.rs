// SPDX-License-Identifier: Apache-2.0
//
// Licensed to the Apache Software Foundation (ASF) under one or more
// license agreements; and to You under the Apache License, version 2.0:
//
//   https://www.apache.org/licenses/LICENSE-2.0
//
// This file is part of the Apache Pekko project, which was derived from Akka.
//
// Copyright (C) 2009-2023 Lightbend Inc. <https://www.lightbend.com>
//
// Copyright (C) 2009-2022 Lightbend Inc. <https://www.lightbend.com>
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.

//! The selection of the actor reference provider a system would use.
//!
//! Mirrors `org.apache.pekko.actor.ProviderSelection`. Pekko reads the
//! `pekko.actor.provider` setting and turns it into a selection whose
//! `fqcn` names the provider class; the port has no configuration file,
//! so the typed entry [`ProviderSelection::from_setting`] takes the
//! setting string directly and copies the original's
//! `ProviderSelection.apply` (`ActorSystem.scala`).

use alloc::string::String;

#[cfg(test)]
#[path = "provider_selection_test.rs"]
mod tests;

/// The fully qualified class name of Pekko's local provider.
///
/// Mirrors the `fqcn` of `ProviderSelection.Local`. Note that, in the
/// original's `apply`, this name is not a key: passing it yields a
/// [`ProviderSelection::Custom`], not `Local`.
pub const LOCAL_PROVIDER_FQCN: &str = "org.apache.pekko.actor.LocalActorRefProvider";

/// The fully qualified class name of Pekko's remote provider.
pub const REMOTE_PROVIDER_FQCN: &str = "org.apache.pekko.remote.RemoteActorRefProvider";

/// The fully qualified class name of Pekko's cluster provider.
pub const CLUSTER_PROVIDER_FQCN: &str = "org.apache.pekko.cluster.ClusterActorRefProvider";

/// Which actor reference provider a setting selects.
///
/// Mirrors `org.apache.pekko.actor.ProviderSelection`: the three built-in
/// selections and a `Custom` that keeps the given class name unchecked,
/// as the original defers the check to the point of use.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum ProviderSelection {
  /// The local provider.
  Local,
  /// The remote provider.
  Remote,
  /// The cluster provider.
  Cluster,
  /// Any other provider, named by its class name.
  Custom(String),
}

impl ProviderSelection {
  /// Turns a provider setting string into a selection.
  ///
  /// Mirrors `ProviderSelection.apply`: `"local"` is `Local`, `"remote"`
  /// and [`REMOTE_PROVIDER_FQCN`] are `Remote`, `"cluster"` and
  /// [`CLUSTER_PROVIDER_FQCN`] are `Cluster`, and every other string —
  /// including [`LOCAL_PROVIDER_FQCN`], the empty string, and strings
  /// that differ only in case or surrounding whitespace — is kept as
  /// [`Custom`](Self::Custom) without any check.
  pub fn from_setting(value: &str) -> Self {
    match value {
      | "local" => Self::Local,
      | "remote" | REMOTE_PROVIDER_FQCN => Self::Remote,
      | "cluster" | CLUSTER_PROVIDER_FQCN => Self::Cluster,
      | other => Self::Custom(String::from(other)),
    }
  }

  /// Returns the class name of the selected provider.
  ///
  /// Mirrors `ProviderSelection.fqcn`: the built-in selections answer
  /// their constant and `Custom` answers the string it keeps.
  pub fn fqcn(&self) -> &str {
    match self {
      | Self::Local => LOCAL_PROVIDER_FQCN,
      | Self::Remote => REMOTE_PROVIDER_FQCN,
      | Self::Cluster => CLUSTER_PROVIDER_FQCN,
      | Self::Custom(name) => name,
    }
  }

  /// Returns whether the selected provider is the cluster provider.
  ///
  /// Mirrors `ProviderSelection.hasCluster`, which only `Cluster`
  /// answers with `true`.
  pub fn has_cluster(&self) -> bool {
    matches!(self, Self::Cluster)
  }
}
