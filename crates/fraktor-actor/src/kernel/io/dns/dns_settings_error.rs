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
// Copyright 2017 The Netty Project
//
// The Netty Project licenses this file to you under the Apache License,
// version 2.0 (the "License"); you may not use this file except in compliance
// with the License. You may obtain a copy of the License at:
//
//   https://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS, WITHOUT
// WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the
// License for the specific language governing permissions and limitations
// under the License.
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.

//! The error a DNS settings build reports.

use core::fmt;

use super::NameserverAddressError;

/// The reason [`DnsSettingsValues::new`](super::DnsSettingsValues::new)
/// rejected a configuration.
///
/// Mirrors the failures the original `class DnsSettings` raises while resolving
/// its fields, keeping the evaluation order so the first failure wins.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DnsSettingsError {
  /// A single name server address did not parse.
  InvalidNameserver(NameserverAddressError),
  /// An explicit `nameservers` list was empty.
  EmptyNameservers,
  /// `nameservers = default` named no server on the host.
  UnableToDetermineDefaultNameservers,
  /// An explicit `ndots` was negative.
  NegativeNdots,
  /// A cache TTL was not positive.
  NonPositiveTtl,
  /// The `id-generator-policy` text named no known policy.
  InvalidIdGeneratorPolicy,
}

impl fmt::Display for DnsSettingsError {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      | Self::InvalidNameserver(error) => write!(formatter, "invalid name server: {error}"),
      | Self::EmptyNameservers => formatter.write_str("the nameservers list is empty"),
      | Self::UnableToDetermineDefaultNameservers => {
        formatter.write_str("could not determine the default name servers from the host")
      },
      | Self::NegativeNdots => formatter.write_str("the ndots value must not be negative"),
      | Self::NonPositiveTtl => formatter.write_str("the cache time-to-live must be positive"),
      | Self::InvalidIdGeneratorPolicy => formatter.write_str("the id-generator-policy is not recognised"),
    }
  }
}

impl core::error::Error for DnsSettingsError {
  fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
    match self {
      | Self::InvalidNameserver(error) => Some(error),
      | _ => None,
    }
  }
}
