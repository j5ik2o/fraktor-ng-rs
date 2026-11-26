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

//! The error a version compatibility check returns.

use alloc::{format, string::String};
use core::fmt;

#[cfg(test)]
#[path = "unsupported_fraktor_version_test.rs"]
mod tests;

/// A required version is not satisfied by the current version.
///
/// Mirrors `org.apache.pekko.UnsupportedPekkoVersion`: it carries the library
/// that required the version, the required version, and the current one. The
/// port renames Pekko to Fraktor and returns this value from
/// [`FraktorVersion::require`](super::FraktorVersion::require) instead of
/// throwing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnsupportedFraktorVersion {
  library_name: String,
  required_version: String,
  current_version: String,
}

impl UnsupportedFraktorVersion {
  pub(crate) fn new(library_name: &str, required_version: &str, current_version: &str) -> Self {
    Self {
      library_name: String::from(library_name),
      required_version: String::from(required_version),
      current_version: String::from(current_version),
    }
  }

  /// Returns the name of the library that required the version.
  pub fn library_name(&self) -> &str {
    &self.library_name
  }

  /// Returns the version the library required.
  pub fn required_version(&self) -> &str {
    &self.required_version
  }

  /// Returns the current version.
  pub fn current_version(&self) -> &str {
    &self.current_version
  }
}

impl fmt::Display for UnsupportedFraktorVersion {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str(&format!(
      "Current version of Fraktor is [{}], but {} requires version [{}]",
      self.current_version, self.library_name, self.required_version
    ))
  }
}

impl core::error::Error for UnsupportedFraktorVersion {}
