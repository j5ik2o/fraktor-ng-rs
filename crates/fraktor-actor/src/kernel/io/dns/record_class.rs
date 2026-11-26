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

//! The class of a DNS record.

use core::fmt;

use super::{WireFormatError, WireReader};

#[cfg(test)]
#[path = "record_class_test.rs"]
mod tests;

/// A DNS record class, such as `IN` for the Internet.
///
/// Mirrors `org.apache.pekko.io.dns.RecordClass`: a code and a name, with one
/// constant for each class the original defines. Unlike the original's public
/// `case class` constructor, only those constants exist.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RecordClass {
  code: u16,
  name: &'static str,
}

impl RecordClass {
  /// The Chaos class.
  pub const CH: Self = Self::new(3, "CH");
  /// The CSNET class (obsolete).
  pub const CS: Self = Self::new(2, "CS");
  /// The Hesiod class.
  pub const HS: Self = Self::new(4, "HS");
  /// The Internet.
  pub const IN: Self = Self::new(1, "IN");
  /// Any class.
  pub const WILDCARD: Self = Self::new(255, "WILDCARD");

  const fn new(code: u16, name: &'static str) -> Self {
    Self { code, name }
  }

  /// Returns the code a DNS message uses for this class.
  #[must_use]
  pub const fn code(self) -> u16 {
    self.code
  }

  /// Returns the name of this class, such as `"IN"`.
  #[must_use]
  pub const fn name(self) -> &'static str {
    self.name
  }

  /// Reads a record class code and returns its record class.
  ///
  /// Mirrors `RecordClassSerializer.parse(it)`, which accepts the codes of
  /// `IN`, `CS`, `CH` and `WILDCARD` and throws for any other code. The
  /// original's reader does not list `HS`, so its code 4 is rejected too.
  pub(crate) fn parse(reader: &mut WireReader<'_>) -> Result<Self, WireFormatError> {
    match reader.read_u16()? {
      | 1 => Ok(Self::IN),
      | 2 => Ok(Self::CS),
      | 3 => Ok(Self::CH),
      | 255 => Ok(Self::WILDCARD),
      | unknown => Err(WireFormatError::UnknownRecordClass(unknown)),
    }
  }
}

impl fmt::Display for RecordClass {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str(self.name)
  }
}
