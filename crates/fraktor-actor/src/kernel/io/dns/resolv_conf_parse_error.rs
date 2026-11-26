// SPDX-License-Identifier: Apache-2.0
//
// Licensed to the Apache Software Foundation (ASF) under one or more
// license agreements; and to You under the Apache License, version 2.0:
//
//   https://www.apache.org/licenses/LICENSE-2.0
//
// This file is part of the Apache Pekko project, which was derived from Akka.
//
// Copyright (C) 2018-2022 Lightbend Inc. <https://www.lightbend.com>
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.

//! The error a `resolv.conf` line parser reports.

use core::fmt;

/// The reason a `resolv.conf` could not be parsed.
///
/// Mirrors the `NumberFormatException` that `org.apache.pekko.io.dns.internal.ResolvConfParser`
/// raises when an `ndots:` option does not hold an integer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvConfParseError {
  /// A `ndots:` option's value is not an `i32`.
  InvalidNdots,
}

impl fmt::Display for ResolvConfParseError {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str(match self {
      | Self::InvalidNdots => "resolv.conf ndots option is not an integer",
    })
  }
}
