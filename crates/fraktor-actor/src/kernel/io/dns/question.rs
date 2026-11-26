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

//! A question in a DNS message.

use super::{DomainName, RecordClass, RecordType, WireFormatError, WireReader};

#[cfg(test)]
#[path = "question_test.rs"]
mod tests;

/// The name, record type and record class a DNS message asks about.
///
/// Mirrors `org.apache.pekko.io.dns.internal.Question`. Writing a question
/// into a message (`write`) and the comparison that ignores a trailing dot
/// (`isSame`) belong to the resolver and are not part of this port.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Question {
  name: DomainName,
  record_type: RecordType,
  record_class: RecordClass,
}

impl Question {
  /// Reads a question at the reader's position.
  ///
  /// Mirrors `Question.parse(it, msg)`: a domain name, a record type code and
  /// a record class code.
  pub(crate) fn parse(reader: &mut WireReader<'_>) -> Result<Self, WireFormatError> {
    let name = DomainName::parse(reader)?;
    let record_type = RecordType::parse(reader)?;
    let record_class = RecordClass::parse(reader)?;
    Ok(Self { name, record_type, record_class })
  }

  /// Returns the name asked about.
  #[must_use]
  pub const fn name(&self) -> &DomainName {
    &self.name
  }

  /// Returns the record type asked for.
  #[must_use]
  pub const fn record_type(&self) -> RecordType {
    self.record_type
  }

  /// Returns the record class asked for.
  #[must_use]
  pub const fn record_class(&self) -> RecordClass {
    self.record_class
  }
}
