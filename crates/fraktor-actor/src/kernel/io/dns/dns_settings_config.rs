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

//! The typed inputs that build [`DnsSettingsValues`](super::DnsSettingsValues).

use alloc::string::String;
use core::time::Duration;

use super::{NameserversSetting, NdotsSetting, SearchDomainsSetting, TtlSetting};

#[cfg(test)]
#[path = "dns_settings_config_test.rs"]
mod tests;

/// The typed `DnsSettings` configuration before the host is read.
///
/// Mirrors the keys of the original `class DnsSettings`: `nameservers`,
/// `resolve-timeout`, `search-domains`, `ndots`, `positive-ttl`,
/// `negative-ttl`, and `id-generator-policy`. The strings are kept as written;
/// validation happens in [`DnsSettingsValues::new`](super::DnsSettingsValues::new).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DnsSettingsConfig {
  nameservers: NameserversSetting,
  resolve_timeout: Duration,
  search_domains: SearchDomainsSetting,
  ndots: NdotsSetting,
  positive_ttl: TtlSetting,
  negative_ttl: TtlSetting,
  id_generator_policy: String,
}

impl DnsSettingsConfig {
  /// Creates a configuration from every setting, in the original's key order.
  pub fn new(
    nameservers: NameserversSetting,
    resolve_timeout: Duration,
    search_domains: SearchDomainsSetting,
    ndots: NdotsSetting,
    positive_ttl: TtlSetting,
    negative_ttl: TtlSetting,
    id_generator_policy: impl Into<String>,
  ) -> Self {
    Self {
      nameservers,
      resolve_timeout,
      search_domains,
      ndots,
      positive_ttl,
      negative_ttl,
      id_generator_policy: id_generator_policy.into(),
    }
  }

  /// Replaces the `nameservers` setting.
  pub fn with_nameservers(self, nameservers: NameserversSetting) -> Self {
    Self { nameservers, ..self }
  }

  /// Replaces the `search-domains` setting.
  pub fn with_search_domains(self, search_domains: SearchDomainsSetting) -> Self {
    Self { search_domains, ..self }
  }

  /// Replaces the `ndots` setting.
  pub fn with_ndots(self, ndots: NdotsSetting) -> Self {
    Self { ndots, ..self }
  }

  /// Replaces the `positive-ttl` setting.
  pub fn with_positive_ttl(self, positive_ttl: TtlSetting) -> Self {
    Self { positive_ttl, ..self }
  }

  /// Replaces the `negative-ttl` setting.
  pub fn with_negative_ttl(self, negative_ttl: TtlSetting) -> Self {
    Self { negative_ttl, ..self }
  }

  /// Returns the raw `nameservers` setting.
  pub fn nameservers(&self) -> &NameserversSetting {
    &self.nameservers
  }

  /// Returns the `resolve-timeout`.
  pub fn resolve_timeout(&self) -> Duration {
    self.resolve_timeout
  }

  /// Returns the raw `search-domains` setting.
  pub fn search_domains(&self) -> &SearchDomainsSetting {
    &self.search_domains
  }

  /// Returns the raw `ndots` setting.
  pub fn ndots(&self) -> NdotsSetting {
    self.ndots
  }

  /// Returns the raw `positive-ttl` setting.
  pub fn positive_ttl(&self) -> TtlSetting {
    self.positive_ttl
  }

  /// Returns the raw `negative-ttl` setting.
  pub fn negative_ttl(&self) -> TtlSetting {
    self.negative_ttl
  }

  /// Returns the `id-generator-policy` text as written.
  pub fn id_generator_policy(&self) -> &str {
    &self.id_generator_policy
  }
}
