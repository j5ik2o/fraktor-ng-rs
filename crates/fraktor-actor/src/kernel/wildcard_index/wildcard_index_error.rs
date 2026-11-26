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

//! The error a wildcard index insert reports.
//!
//! Mirrors `org.apache.pekko.util.WildcardIndex`.

use alloc::string::String;
use core::fmt;

/// The reason a wildcard index insert rejected its elements.
///
/// Mirrors the `IllegalArgumentException` the original's `insert` throws when
/// an element uses a double wildcard as a suffix, such as `"a**"`. The port
/// returns this error instead of throwing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WildcardIndexError {
  /// An element other than `"**"` itself ends with `"**"`.
  DoubleWildcardAsSuffix {
    /// The rejected element.
    element: String,
  },
}

impl fmt::Display for WildcardIndexError {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      | Self::DoubleWildcardAsSuffix { element } => {
        write!(formatter, "a double wildcard may not be used as a suffix: {element}")
      },
    }
  }
}

impl core::error::Error for WildcardIndexError {}
