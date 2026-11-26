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

// Checked against Pekko v2.0.0 (SHA 9fbd2283b1cf701c90a1223bf621942c191db61f): the upstream test
// file is the same as in v1.7.1.
// The record is docs/baseline/pekko-v2.0.0-migration/batch-3.md.
//
// Ported from Pekko v1.7.1 (SHA 9a54d1294c3ac7821bbed3c940769c3d5027d1ff):
//   actor-tests/src/test/scala/org/apache/pekko/io/dns/NameserverAddressParserSpec.scala
//
// The original's four cases under `"Parser" should` are kept as one test each,
// named after the case, with the original name as a comment. Each case asserts
// `DnsSettings.parseNameserverAddress(<input>) shouldEqual new
// InetSocketAddress(<host>, <port>)`; the port keeps the original's value,
// including the default `DnsFallbackPort` of 53 when the address has no port.
//
// The original returns a `java.net.InetSocketAddress` built from a numeric IP
// string and a port; the port's `parseNameserverAddress` returns
// `Result<core::net::SocketAddr, NameserverAddressError>`, and each `Ok` value is
// `SocketAddr::new(<numeric IP>.parse::<IpAddr>().unwrap(), <port>)`, which is
// the same address and port the original's `InetSocketAddress` holds.
//
// The port's `DnsSettings` lives at `fraktor_actor::kernel::io::dns`. Until it
// exists the file does not compile: that compile error is the intended red for
// this node.

use std::net::{IpAddr, SocketAddr};

use fraktor_actor::kernel::io::dns::DnsSettings;

// "handle explicit port in IPv4 address"
#[test]
fn handle_explicit_port_in_ipv4_address() {
  assert_eq!(
    DnsSettings::parse_nameserver_address("8.8.8.8:153"),
    Ok(SocketAddr::new("8.8.8.8".parse::<IpAddr>().unwrap(), 153))
  );
}

// "handle explicit port in IPv6 address"
#[test]
fn handle_explicit_port_in_ipv6_address() {
  assert_eq!(
    DnsSettings::parse_nameserver_address("[2001:4860:4860::8888]:153"),
    Ok(SocketAddr::new("2001:4860:4860::8888".parse::<IpAddr>().unwrap(), 153))
  );
}

// "handle default port in IPv4 address"
#[test]
fn handle_default_port_in_ipv4_address() {
  assert_eq!(
    DnsSettings::parse_nameserver_address("8.8.8.8"),
    Ok(SocketAddr::new("8.8.8.8".parse::<IpAddr>().unwrap(), 53))
  );
}

// "handle default port in IPv6 address"
#[test]
fn handle_default_port_in_ipv6_address() {
  assert_eq!(
    DnsSettings::parse_nameserver_address("[2001:4860:4860::8888]"),
    Ok(SocketAddr::new("2001:4860:4860::8888".parse::<IpAddr>().unwrap(), 53))
  );
}
