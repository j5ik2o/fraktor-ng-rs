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

//! The combined error of the two checks an actor creation runs.

use core::fmt;

use crate::kernel::actor::ConfigurationException;
use crate::kernel::actor::InvalidActorNameException;

/// The two ways starting an actor can be refused, as one error.
///
/// Pekko throws both from the same `actorOf` call: the name checks
/// raise `InvalidActorNameException` and the configuration checks
/// raise `ConfigurationException`. The port's combined entry points
/// ([`ActorSystem::try_actor_of`](crate::kernel::actor_system::ActorSystem::try_actor_of)
/// and `actor_of_named`) return this one error so a caller matches a
/// single value (design of #194, M5).
#[derive(Clone, PartialEq, Eq)]
pub enum ActorCreationError {
  /// The name failed the name checks.
  InvalidName(InvalidActorNameException),
  /// The configuration failed verification.
  Configuration(ConfigurationException),
}

impl fmt::Display for ActorCreationError {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      | ActorCreationError::InvalidName(error) => write!(formatter, "{error}"),
      | ActorCreationError::Configuration(error) => write!(formatter, "{error}"),
    }
  }
}

impl fmt::Debug for ActorCreationError {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      | ActorCreationError::InvalidName(error) => write!(formatter, "{error:?}"),
      | ActorCreationError::Configuration(error) => write!(formatter, "{error:?}"),
    }
  }
}

impl core::error::Error for ActorCreationError {}

impl From<InvalidActorNameException> for ActorCreationError {
  fn from(error: InvalidActorNameException) -> Self {
    ActorCreationError::InvalidName(error)
  }
}

impl From<ConfigurationException> for ActorCreationError {
  fn from(error: ConfigurationException) -> Self {
    ActorCreationError::Configuration(error)
  }
}
