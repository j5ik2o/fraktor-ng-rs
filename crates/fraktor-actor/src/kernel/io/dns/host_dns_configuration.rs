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

//! The host DNS configuration the settings read their defaults from.

use alloc::vec::Vec;
use core::net::SocketAddr;

use super::ResolvConf;

/// The DNS configuration of the host the resolver runs on.
///
/// Mirrors the host lookups the original `class DnsSettings` performs: the name
/// servers from JNDI and reflection, and `/etc/resolv.conf`. An implementation
/// that reads the real host lives outside the kernel, which does not use `std`.
pub trait HostDnsConfiguration {
  /// Returns the host's name servers, or `None` when none could be determined.
  ///
  /// `None` stands for the original's failure to read the name servers through
  /// either JNDI or reflection.
  fn name_servers(&self) -> Option<Vec<SocketAddr>>;

  /// Returns the parsed `/etc/resolv.conf`, or `None` when there is none.
  ///
  /// `None` stands for the original's cases with no readable `resolv.conf`:
  /// Windows, a missing file, or a parse failure.
  fn resolv_conf(&self) -> Option<ResolvConf>;
}
