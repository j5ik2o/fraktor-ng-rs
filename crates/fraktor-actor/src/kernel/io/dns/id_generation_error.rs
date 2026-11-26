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

//! The error an identifier generator reports.

use core::fmt;

#[cfg(test)]
#[path = "id_generation_error_test.rs"]
mod tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// The reason an identifier could not be produced.
///
/// Mirrors the failure the original's secure generator reports when the source
/// of randomness cannot be read.
pub enum IdGenerationError {
  /// The operating system's entropy source could not be read.
  EntropyUnavailable,
}

impl fmt::Display for IdGenerationError {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str(match self {
      | Self::EntropyUnavailable => "the operating system entropy source is unavailable",
    })
  }
}

impl core::error::Error for IdGenerationError {}
