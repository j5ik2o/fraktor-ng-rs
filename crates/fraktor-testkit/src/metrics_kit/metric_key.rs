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

//! The dotted name a metric is registered under.

use core::fmt;

/// A metric name built from parts, each sanitized as the original does.
///
/// Mirrors `MetricKeyDSL.MetricKey`.
#[derive(Clone)]
pub struct MetricKey {
  path: String,
}

/// Sanitizes one part of a key, in the original's order.
///
/// Mirrors `MetricKey.sanitizeMetricKeyPart`.
fn sanitize_part(part: &str) -> String {
  part
    .replace("...", "\u{2026}")
    .replace('.', "-")
    .replace(['[', ']', '(', ')', '<', '>'], "|")
    .replace([' ', '/'], "-")
}

impl MetricKey {
  /// Creates a key of one sanitized part.
  ///
  /// Mirrors `MetricKey.fromString`.
  pub fn from_string(root: &str) -> Self {
    Self { path: sanitize_part(root) }
  }

  /// Appends a sanitized part after a `.`.
  ///
  /// Mirrors the original's `/` operator.
  pub fn child(&self, part: &str) -> Self {
    Self { path: format!("{}.{}", self.path, sanitize_part(part)) }
  }

  /// Returns the path.
  pub fn as_str(&self) -> &str {
    &self.path
  }
}

impl fmt::Display for MetricKey {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str(&self.path)
  }
}
