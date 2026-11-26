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

//! Unit tests for the setting value types.

use alloc::string::String;
use alloc::vec;
use core::time::Duration;

use super::{DnsSettingsConfig, NameserversSetting, NdotsSetting, SearchDomainsSetting, TtlSetting};
use crate::kernel::io::dns::{CachePolicy, IdGeneratorPolicy, Ttl};

// The builder setters must reach the matching getters. A setter wired to the
// wrong field would pass an all-defaults test, so every field is changed from
// its default and read back through its own getter.
#[test]
fn the_builder_applies_each_setting_to_its_own_getter() {
  let config = DnsSettingsConfig::new(
    NameserversSetting::Single(String::from("127.0.0.1")),
    Duration::from_secs(1),
    SearchDomainsSetting::List(vec![String::from("example.com")]),
    NdotsSetting::Value(1),
    TtlSetting::Forever,
    TtlSetting::Never,
    "thread-local-random",
  );

  let config = config
    .with_nameservers(NameserversSetting::List(vec![String::from("127.0.0.2")]))
    .with_search_domains(SearchDomainsSetting::Single(String::from("example.net")))
    .with_ndots(NdotsSetting::Value(5))
    .with_positive_ttl(TtlSetting::Duration(Duration::from_secs(10)))
    .with_negative_ttl(TtlSetting::Duration(Duration::from_secs(20)));

  assert_eq!(config.nameservers(), &NameserversSetting::List(vec![String::from("127.0.0.2")]));
  assert_eq!(config.resolve_timeout(), Duration::from_secs(1));
  assert_eq!(config.search_domains(), &SearchDomainsSetting::Single(String::from("example.net")));
  assert_eq!(config.ndots(), NdotsSetting::Value(5));
  assert_eq!(config.positive_ttl(), TtlSetting::Duration(Duration::from_secs(10)));
  assert_eq!(config.negative_ttl(), TtlSetting::Duration(Duration::from_secs(20)));
  assert_eq!(config.id_generator_policy(), "thread-local-random");
}

// The original keys `id-generator-policy` as a string; the constructor accepts
// any `Into<String>` and keeps its text verbatim (including different cases),
// leaving the rejection to `DnsSettingsValues::new`.
#[test]
fn the_id_generator_policy_text_is_kept_verbatim() {
  let config = DnsSettingsConfig::new(
    NameserversSetting::Default,
    Duration::from_secs(1),
    SearchDomainsSetting::Default,
    NdotsSetting::Default,
    TtlSetting::Forever,
    TtlSetting::Never,
    String::from("Thread-Local-Random"),
  );

  assert_eq!(config.id_generator_policy(), "Thread-Local-Random");
}

#[test]
fn the_cache_policy_variants_carry_the_ttl() {
  let ttl = Ttl::from_positive(Duration::from_secs(3)).expect("a positive duration");
  assert_ne!(CachePolicy::Never, CachePolicy::Forever);
  assert_ne!(CachePolicy::Forever, CachePolicy::Ttl(ttl));
  let same = Ttl::from_positive(Duration::from_secs(3)).expect("a positive duration");
  assert_eq!(CachePolicy::Ttl(ttl), CachePolicy::Ttl(same));
}

#[test]
fn the_id_generator_policy_variants_are_distinct() {
  assert_ne!(IdGeneratorPolicy::ThreadLocalRandom, IdGeneratorPolicy::SecureRandom);
  assert_ne!(IdGeneratorPolicy::SecureRandom, IdGeneratorPolicy::EnhancedDoubleHashRandom);
  assert_ne!(IdGeneratorPolicy::EnhancedDoubleHashRandom, IdGeneratorPolicy::ThreadLocalRandom);
}
