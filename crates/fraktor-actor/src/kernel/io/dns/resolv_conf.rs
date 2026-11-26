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

//! The result of parsing a `resolv.conf`.

use alloc::{string::String, vec::Vec};

#[cfg(test)]
#[path = "resolv_conf_test.rs"]
mod tests;

/// The search list and `ndots` a `resolv.conf` holds.
///
/// Mirrors `org.apache.pekko.io.dns.internal.ResolvConf`, the `case class` that
/// `ResolvConfParser.parseLines` builds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvConf {
  search: Vec<String>,
  // A plain `i32`, as the original's `Int`: the original puts no range or
  // format rule on it beyond being an integer (a negative value is kept), and
  // nothing in this port computes with it yet. A dedicated type belongs with
  // the first logic that interprets the threshold.
  ndots: i32,
}

impl ResolvConf {
  /// Creates a configuration from its search list and `ndots`.
  pub fn new(search: Vec<String>, ndots: i32) -> Self {
    Self { search, ndots }
  }

  /// Returns the domains to search, in order.
  pub fn search(&self) -> &[String] {
    &self.search
  }

  /// Returns the number of dots a name needs before it is tried as absolute.
  pub fn ndots(&self) -> i32 {
    self.ndots
  }
}
