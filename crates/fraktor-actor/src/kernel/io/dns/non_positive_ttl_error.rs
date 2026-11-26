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

//! The error a non-positive TTL reports.

use core::fmt;

/// The reason a cache time-to-live could not be built.
///
/// Mirrors the `IllegalArgumentException` the original
/// `org.apache.pekko.io.dns.CachePolicy.Ttl.fromPositive` throws for a
/// non-positive duration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NonPositiveTtlError;

impl fmt::Display for NonPositiveTtlError {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str("the cache time-to-live must be positive")
  }
}

impl core::error::Error for NonPositiveTtlError {}
