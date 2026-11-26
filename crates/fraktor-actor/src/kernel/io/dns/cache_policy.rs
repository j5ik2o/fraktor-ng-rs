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

//! The resolved positive and negative cache policies.

use super::Ttl;

/// How long a record is cached.
///
/// Mirrors `org.apache.pekko.io.dns.CachePolicy`, which is `Never`, `Forever`,
/// or a [`Ttl`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CachePolicy {
  /// Do not cache.
  Never,
  /// Cache forever.
  Forever,
  /// Cache for the given TTL.
  Ttl(Ttl),
}
