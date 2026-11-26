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

//! Why a dispatcher or mailbox name could not be resolved.

use alloc::string::String;
use core::fmt;

/// Why a dispatcher or mailbox name could not be resolved.
///
/// Mirrors the `ConfigurationException` Pekko raises for a broken
/// dispatcher or mailbox configuration. A name that is simply not
/// defined is not an error: it keeps the port's previous behaviour of
/// recording the name only, with no capacity bound.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum DispatchResolutionError {
  /// The alias chain returns to a name it already visited.
  AliasCycle {
    /// The name whose resolution found the cycle.
    name: String,
  },
  /// An alias points at a name that is not defined.
  AliasTargetUnknown {
    /// The undefined name the alias points at.
    name: String,
  },
  /// The alias chain is longer than the original's limit of 20.
  AliasTooDeep {
    /// The name whose resolution exceeded the limit.
    name: String,
  },
}

impl fmt::Display for DispatchResolutionError {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      | Self::AliasCycle { name } => write!(formatter, "dispatcher/mailbox alias cycle at [{name}]"),
      | Self::AliasTargetUnknown { name } => {
        write!(formatter, "dispatcher/mailbox alias target [{name}] is not defined")
      },
      | Self::AliasTooDeep { name } => {
        write!(formatter, "dispatcher/mailbox alias chain from [{name}] exceeds 20 steps")
      },
    }
  }
}

impl core::error::Error for DispatchResolutionError {}
