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

//! The scope a deployment assigns to the actors it creates.

/// The scope of a deployment.
///
/// Mirrors `org.apache.pekko.actor.Scope`: `NoScopeGiven` defers to the
/// fallback, `Local` is the scope of the deployments this port builds.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Scope {
  /// No scope was given; a fallback supplies the scope.
  NoScopeGiven,
  /// The actors are local to this system.
  Local,
}

impl Scope {
  /// Returns the scope that applies when this scope is layered over
  /// `other`.
  ///
  /// Mirrors `Scope.withFallback`: `Local` keeps itself and
  /// `NoScopeGiven` returns the other scope.
  #[must_use]
  pub fn with_fallback(self, other: Scope) -> Scope {
    match self {
      | Scope::Local => self,
      | Scope::NoScopeGiven => other,
    }
  }
}
