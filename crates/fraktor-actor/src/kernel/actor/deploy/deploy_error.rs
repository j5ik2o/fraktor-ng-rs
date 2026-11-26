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
//! The errors the reading and registration of deployments return.

use alloc::string::String;
use core::fmt;

use crate::kernel::actor::InvalidActorNameException;
use crate::kernel::wildcard_index::WildcardIndexError;

/// An error from building a [`Deployer`](super::Deployer) or
/// registering a [`Deploy`](super::Deploy).
///
/// Each variant mirrors the exception the original raises at the same
/// point: the port returns the error instead of throwing.
#[derive(Clone, PartialEq, Debug)]
pub enum DeployError {
  /// A path element failed validation, or was empty.
  ///
  /// Mirrors the `InvalidActorNameException` of `Deployer.deploy`.
  InvalidName(InvalidActorNameException),
  /// The wildcard index refused the pattern.
  ///
  /// Mirrors the `IllegalArgumentException` the original's index
  /// raises while a system starts.
  Index(WildcardIndexError),
  /// A value could not be read as the kind of value its key needs.
  ///
  /// Mirrors `ConfigException.WrongType` (the case of
  /// `nr-of-instances = boom` and of an out-of-range count alike).
  WrongType {
    /// The key whose value was refused.
    key: &'static str,
    /// The value as written.
    value: String,
  },
  /// The router selection names no router this port can build.
  ///
  /// Mirrors the `IllegalArgumentException` ("Cannot instantiate
  /// router [...]") the original raises when reflection fails.
  UnsupportedRouter {
    /// The selection string that was refused.
    selection: String,
    /// The key of the section that named it.
    key: String,
  },
  /// A resizer value failed the constructor's validation.
  ///
  /// Mirrors the `IllegalArgumentException` of `DefaultResizer`.
  InvalidResizer {
    /// The original's reason.
    reason: String,
  },
}

impl fmt::Display for DeployError {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      | Self::InvalidName(error) => write!(formatter, "{error}"),
      | Self::Index(error) => write!(formatter, "{error}"),
      | Self::WrongType { key, value } => {
        write!(formatter, "wrong type for {key}: {value} cannot be read as a 32-bit integer")
      },
      | Self::UnsupportedRouter { selection, key } => {
        write!(formatter, "cannot instantiate router [{selection}], defined in [{key}]")
      },
      | Self::InvalidResizer { reason } => write!(formatter, "{reason}"),
    }
  }
}
