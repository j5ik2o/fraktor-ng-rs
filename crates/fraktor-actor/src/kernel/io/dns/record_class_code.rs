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

//! The record class code a resource record holds.

use super::RecordClass;

#[cfg(test)]
#[path = "record_class_code_test.rs"]
mod tests;

/// The 16-bit record class code of a resource record, known or not.
///
/// Mirrors the `recClass` that `org.apache.pekko.io.dns.ResourceRecord.parse`
/// reads and that `UnknownRecord` keeps as a `Short`. A record may hold any
/// code, including one [`RecordClass`] has no constant for, so this type keeps
/// the code itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RecordClassCode {
  value: u16,
}

impl RecordClassCode {
  /// Wraps a record class code as a message holds it.
  #[must_use]
  pub const fn new(value: u16) -> Self {
    Self { value }
  }

  /// Returns the code as a message holds it.
  #[must_use]
  pub const fn value(self) -> u16 {
    self.value
  }
}

impl From<RecordClass> for RecordClassCode {
  fn from(record_class: RecordClass) -> Self {
    Self::new(record_class.code())
  }
}
