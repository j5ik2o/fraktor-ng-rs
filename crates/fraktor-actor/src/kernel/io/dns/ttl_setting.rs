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

//! The configured value for a cache TTL setting.

use core::time::Duration;

use super::{CachePolicy, NonPositiveTtlError, Ttl};

/// The raw `positive-ttl` or `negative-ttl` setting before it is resolved.
///
/// Mirrors the original's cache TTL keys, which are `forever`, `never`, or a
/// duration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TtlSetting {
  /// Cache forever.
  Forever,
  /// Do not cache.
  Never,
  /// Cache for the given duration.
  Duration(Duration),
}

impl TtlSetting {
  /// Resolves the setting into a cache policy, as the original's `getTtl` does.
  ///
  /// A zero duration is rejected with [`NonPositiveTtlError`], as the original's
  /// `Ttl.fromPositive` rejects it.
  pub(crate) fn cache_policy(self) -> Result<CachePolicy, NonPositiveTtlError> {
    match self {
      | Self::Forever => Ok(CachePolicy::Forever),
      | Self::Never => Ok(CachePolicy::Never),
      | Self::Duration(duration) => Ttl::from_positive(duration).map(CachePolicy::Ttl),
    }
  }
}
