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

//! The configured value for the `nameservers` setting.

use alloc::string::String;
use alloc::vec::Vec;
use core::net::SocketAddr;

use super::{DnsSettings, DnsSettingsError};

/// The raw `nameservers` setting before it is resolved.
///
/// Mirrors the original's `nameservers` key, which is either the string
/// `"default"`, a single address, or a list of addresses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NameserversSetting {
  /// Use the host's name servers.
  Default,
  /// Use the single address written as `host` or `host:port`.
  Single(String),
  /// Use the list of addresses in order.
  List(Vec<String>),
}

impl NameserversSetting {
  /// Resolves the name servers, as the original's `NameServers` does.
  ///
  /// `Default` calls `host_name_servers` and fails with
  /// [`DnsSettingsError::UnableToDetermineDefaultNameservers`] when it returns
  /// `None` or an empty list; no other variant calls it. A single address and
  /// each list element are read with [`DnsSettings::parse_nameserver_address`],
  /// and an empty list is [`DnsSettingsError::EmptyNameservers`].
  pub(crate) fn resolve(
    &self,
    host_name_servers: impl FnOnce() -> Option<Vec<SocketAddr>>,
  ) -> Result<Vec<SocketAddr>, DnsSettingsError> {
    match self {
      | Self::Default => match host_name_servers() {
        | Some(servers) if !servers.is_empty() => Ok(servers),
        | _ => Err(DnsSettingsError::UnableToDetermineDefaultNameservers),
      },
      | Self::Single(text) => Ok(alloc::vec![parse(text)?]),
      | Self::List(texts) if texts.is_empty() => Err(DnsSettingsError::EmptyNameservers),
      | Self::List(texts) => texts.iter().map(|text| parse(text)).collect(),
    }
  }
}

fn parse(text: &str) -> Result<SocketAddr, DnsSettingsError> {
  DnsSettings::parse_nameserver_address(text).map_err(DnsSettingsError::InvalidNameserver)
}
