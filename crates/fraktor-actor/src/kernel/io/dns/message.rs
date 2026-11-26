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

//! A DNS message read from its wire format.

use alloc::vec::Vec;

use super::{MessageFlags, Question, ResourceRecord, WireFormatError, WireReader};

#[cfg(test)]
#[path = "message_test.rs"]
mod tests;

// A section count of `0x8000` or more is a negative `Short` in the original,
// and `0 until count` is then empty.
const LARGEST_SECTION_COUNT: u16 = 0x7FFF;

/// A DNS message: the header's id and flags, the questions, and the answer,
/// authority and additional records.
///
/// Mirrors `org.apache.pekko.io.dns.internal.Message`. Writing a message
/// (`write`) belongs to sending queries and is not part of this port.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
  // A plain `u16`: the id is an opaque value with no rule beyond its width,
  // and nothing in this port matches it against a query yet. The original's
  // `Short` holds the same 16 bits.
  id: u16,
  flags: MessageFlags,
  questions: Vec<Question>,
  answer_records: Vec<ResourceRecord>,
  authority_records: Vec<ResourceRecord>,
  additional_records: Vec<ResourceRecord>,
}

impl Message {
  /// Reads a whole DNS message.
  ///
  /// Mirrors `Message.parse(msg)`. The header holds the id, the flags and the
  /// number of entries in each of the four sections; the entries follow in
  /// order. When the flags have the TC bit set, the message was cut to fit the
  /// transport: an entry that cannot be read is left out and reading goes on
  /// with the next one, so a response cut after its questions keeps them and
  /// has empty record sections. Without the TC bit, the first entry that
  /// cannot be read fails the whole message. A section count of `0x8000` or
  /// more is read as zero entries, as the original's signed count is.
  ///
  /// # Errors
  ///
  /// - [`WireFormatError::Truncated`] when the message ends inside the
  ///   12-byte header.
  /// - Without the TC bit, the error of the first question or resource record
  ///   that cannot be read.
  pub fn parse(message: &[u8]) -> Result<Self, WireFormatError> {
    let mut reader = WireReader::new(message);
    let id = reader.read_u16()?;
    let flags = MessageFlags::new(reader.read_u16()?);
    let question_count = reader.read_u16()?;
    let answer_count = reader.read_u16()?;
    let authority_count = reader.read_u16()?;
    let additional_count = reader.read_u16()?;
    let questions = Self::read_section(&mut reader, flags, question_count, Question::parse)?;
    let answer_records = Self::read_section(&mut reader, flags, answer_count, ResourceRecord::parse)?;
    let authority_records = Self::read_section(&mut reader, flags, authority_count, ResourceRecord::parse)?;
    let additional_records = Self::read_section(&mut reader, flags, additional_count, ResourceRecord::parse)?;
    Ok(Self { id, flags, questions, answer_records, authority_records, additional_records })
  }

  // Reads `count` entries; a truncated message leaves out the entries that
  // cannot be read, as the original's `Try` flattening does.
  fn read_section<T>(
    reader: &mut WireReader<'_>,
    flags: MessageFlags,
    count: u16,
    parse: fn(&mut WireReader<'_>) -> Result<T, WireFormatError>,
  ) -> Result<Vec<T>, WireFormatError> {
    let count = if count > LARGEST_SECTION_COUNT { 0 } else { count };
    let mut entries = Vec::new();
    for _ in 0..count {
      match parse(reader) {
        | Ok(entry) => entries.push(entry),
        | Err(_) if flags.is_truncated() => {},
        | Err(error) => return Err(error),
      }
    }
    Ok(entries)
  }

  /// Returns the id that pairs the message with its query.
  #[must_use]
  pub const fn id(&self) -> u16 {
    self.id
  }

  /// Returns the header's flags.
  #[must_use]
  pub const fn flags(&self) -> MessageFlags {
    self.flags
  }

  /// Returns the questions that could be read.
  #[must_use]
  pub fn questions(&self) -> &[Question] {
    &self.questions
  }

  /// Returns the answer records that could be read.
  #[must_use]
  pub fn answer_records(&self) -> &[ResourceRecord] {
    &self.answer_records
  }

  /// Returns the authority records that could be read.
  #[must_use]
  pub fn authority_records(&self) -> &[ResourceRecord] {
    &self.authority_records
  }

  /// Returns the additional records that could be read.
  #[must_use]
  pub fn additional_records(&self) -> &[ResourceRecord] {
    &self.additional_records
  }
}
