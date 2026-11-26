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

//! The error a name server address parser reports.

use core::fmt;

/// The reason a name server address could not be parsed.
///
/// Mirrors the failures `org.apache.pekko.io.dns.DnsSettings.parseNameserverAddress`
/// turns into exceptions: a port that is not a valid `u16`, a host that is not a
/// numeric IP literal, or text the original's regular expression does not match.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameserverAddressError {
  /// The text after the last `:` is a run of digits but not a `u16` port.
  InvalidPort,
  /// The host is not a numeric IPv4 or a bracketed IPv6 address.
  UnsupportedHost,
  /// The text contains a line terminator, so the original's regular expression
  /// does not match it at all.
  Unparseable,
}

impl fmt::Display for NameserverAddressError {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str(match self {
      | Self::InvalidPort => "name server port is not a valid u16",
      | Self::UnsupportedHost => "name server host is not a numeric IP address",
      | Self::Unparseable => "name server address contains a line terminator",
    })
  }
}

impl core::error::Error for NameserverAddressError {}
