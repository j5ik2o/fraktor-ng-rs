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

//! The configured value for the `ndots` setting.

use super::{DnsSettingsError, ResolvConf};

/// The raw `ndots` setting before it is resolved.
///
/// Mirrors the original's `ndots` key, which is either `"default"` or an
/// integer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NdotsSetting {
  /// Use `ndots` from the host's `resolv.conf`, or 1 when there is none.
  Default,
  /// Use the given threshold unchanged.
  Value(i32),
}

impl NdotsSetting {
  /// Returns whether the setting reads the host's `resolv.conf`.
  pub(crate) fn uses_host(self) -> bool {
    matches!(self, Self::Default)
  }

  /// Resolves the threshold, as the original's `NDots` does.
  ///
  /// `Default` takes `ndots` from `resolv_conf`, or 1 when there is none, and
  /// keeps a negative value as the original does. An explicit negative value is
  /// [`DnsSettingsError::NegativeNdots`].
  pub(crate) fn resolve(self, resolv_conf: Option<&ResolvConf>) -> Result<i32, DnsSettingsError> {
    match self {
      | Self::Default => Ok(resolv_conf.map_or(1, ResolvConf::ndots)),
      | Self::Value(value) if value < 0 => Err(DnsSettingsError::NegativeNdots),
      | Self::Value(value) => Ok(value),
    }
  }
}
