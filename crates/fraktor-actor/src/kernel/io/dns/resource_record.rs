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

//! A resource record in a DNS message.

use alloc::vec::Vec;
use core::time::Duration;

use super::{DomainName, RecordClassCode, RecordTypeCode, WireFormatError, WireReader};

#[cfg(test)]
#[path = "resource_record_test.rs"]
mod tests;

// A record data length of `0x8000` or more is a negative `Short` in the
// original, for which `take` and `drop` read nothing and stay in place.
const LARGEST_DATA_LENGTH: u16 = 0x7FFF;

/// A resource record: an owner name, a type and class code, a time to live and
/// the record data, kept as the bytes the message holds.
///
/// Mirrors the envelope that `org.apache.pekko.io.dns.ResourceRecord.parse`
/// reads before it hands the data to a record type. The original decodes the
/// data of `A`, `CNAME`, `AAAA` and `SRV` records into typed records and keeps
/// the data of any other type as bytes (`UnknownRecord`). This port keeps the
/// data of every type as bytes: decoding record data is not part of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceRecord {
  name: DomainName,
  record_type_code: RecordTypeCode,
  record_class_code: RecordClassCode,
  ttl: Duration,
  data: Vec<u8>,
}

impl ResourceRecord {
  /// Reads a resource record at the reader's position.
  ///
  /// Mirrors `ResourceRecord.parse(it, msg)`: a domain name, a 16-bit type
  /// code, a 16-bit class code, a 32-bit time to live in seconds, a 16-bit
  /// data length and that many bytes of data. As in the original, data that
  /// runs past the end of the message is cut at the end, and a data length of
  /// `0x8000` or more reads no data.
  ///
  /// # Errors
  ///
  /// - The errors of [`DomainName::parse`] for the owner name.
  /// - [`WireFormatError::Truncated`] when the message ends inside the fixed
  ///   fields.
  /// - [`WireFormatError::NegativeTtl`] when the time to live is negative, as
  ///   the original's `Ttl.fromPositive` rejects it.
  pub(crate) fn parse(reader: &mut WireReader<'_>) -> Result<Self, WireFormatError> {
    let name = DomainName::parse(reader)?;
    let record_type_code = RecordTypeCode::new(reader.read_u16()?);
    let record_class_code = RecordClassCode::new(reader.read_u16()?);
    let ttl_seconds = reader.read_i32()?;
    let ttl =
      u64::try_from(ttl_seconds).map(Duration::from_secs).map_err(|_| WireFormatError::NegativeTtl(ttl_seconds))?;
    let data_length = reader.read_u16()?;
    let data =
      if data_length > LARGEST_DATA_LENGTH { Vec::new() } else { reader.read_up_to(usize::from(data_length)).to_vec() };
    Ok(Self { name, record_type_code, record_class_code, ttl, data })
  }

  /// Returns the name the record belongs to.
  #[must_use]
  pub const fn name(&self) -> &DomainName {
    &self.name
  }

  /// Returns the record type code.
  #[must_use]
  pub const fn record_type_code(&self) -> RecordTypeCode {
    self.record_type_code
  }

  /// Returns the record class code.
  #[must_use]
  pub const fn record_class_code(&self) -> RecordClassCode {
    self.record_class_code
  }

  /// Returns how long the record may be cached. Zero means it must not be
  /// cached, which the original calls `Ttl.never`.
  #[must_use]
  pub const fn ttl(&self) -> Duration {
    self.ttl
  }

  /// Returns the record data as the message holds it.
  #[must_use]
  pub fn data(&self) -> &[u8] {
    &self.data
  }
}
