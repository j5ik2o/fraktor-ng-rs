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
// Copyright (C) 2016-2022 Lightbend Inc. <https://www.lightbend.com>
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.

//! The type of a DNS record.

use core::fmt;

use super::{WireFormatError, WireReader};

#[cfg(test)]
#[path = "record_type_test.rs"]
mod tests;

/// A DNS record type, such as `A` or `AAAA`.
///
/// Mirrors `org.apache.pekko.io.dns.RecordType`: a code and a name, with one
/// constant for each type the original registers. Unlike the original's public
/// `case class` constructor, only those constants exist.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RecordType {
  code: u16,
  name: &'static str,
}

impl RecordType {
  /// A host address.
  pub const A: Self = Self::new(1, "A");
  /// A host address in IPv6 (RFC 3596).
  pub const AAAA: Self = Self::new(28, "AAAA");
  /// A request for a transfer of an entire zone.
  pub const AXFR: Self = Self::new(252, "AXFR");
  /// The canonical name for an alias.
  pub const CNAME: Self = Self::new(5, "CNAME");
  /// Host information.
  pub const HINFO: Self = Self::new(13, "HINFO");
  /// A request for mail agent records (obsolete, use `MX`).
  pub const MAILA: Self = Self::new(254, "MAILA");
  /// A request for mailbox related records.
  pub const MAILB: Self = Self::new(253, "MAILB");
  /// A mailbox domain name (experimental).
  pub const MB: Self = Self::new(7, "MB");
  /// A mail destination (obsolete, use `MX`).
  pub const MD: Self = Self::new(3, "MD");
  /// A mail forwarder (obsolete, use `MX`).
  pub const MF: Self = Self::new(4, "MF");
  /// A mail group member (experimental).
  pub const MG: Self = Self::new(8, "MG");
  /// Mailbox or mail list information.
  pub const MINFO: Self = Self::new(14, "MINFO");
  /// A mail rename domain name (experimental).
  pub const MR: Self = Self::new(9, "MR");
  /// A mail exchange.
  pub const MX: Self = Self::new(15, "MX");
  /// An authoritative name server.
  pub const NS: Self = Self::new(2, "NS");
  /// A null record (experimental).
  pub const NULL: Self = Self::new(10, "NULL");
  /// A domain name pointer.
  pub const PTR: Self = Self::new(12, "PTR");
  /// The start of a zone of authority.
  pub const SOA: Self = Self::new(6, "SOA");
  /// A service location (RFC 2782).
  pub const SRV: Self = Self::new(33, "SRV");
  /// Text strings.
  pub const TXT: Self = Self::new(16, "TXT");
  /// A request for all records.
  pub const WILDCARD: Self = Self::new(255, "WILDCARD");
  /// A well known service description.
  pub const WKS: Self = Self::new(11, "WKS");

  const REGISTERED: [Self; 22] = [
    Self::A,
    Self::NS,
    Self::MD,
    Self::MF,
    Self::CNAME,
    Self::SOA,
    Self::MB,
    Self::MG,
    Self::MR,
    Self::NULL,
    Self::WKS,
    Self::PTR,
    Self::HINFO,
    Self::MINFO,
    Self::MX,
    Self::TXT,
    Self::AAAA,
    Self::SRV,
    Self::AXFR,
    Self::MAILB,
    Self::MAILA,
    Self::WILDCARD,
  ];

  const fn new(code: u16, name: &'static str) -> Self {
    Self { code, name }
  }

  /// Returns the record type that has `code`, or `None` when no constant has
  /// it.
  ///
  /// Mirrors `RecordType.apply(id)`, which returns `OptionVal.None` for an id
  /// outside 1 to 255 and for an id no type is registered under.
  #[must_use]
  pub fn from_code(code: u16) -> Option<Self> {
    Self::REGISTERED.into_iter().find(|record_type| record_type.code == code)
  }

  /// Returns the code a DNS message uses for this type.
  #[must_use]
  pub const fn code(self) -> u16 {
    self.code
  }

  /// Returns the name of this type, such as `"AAAA"`.
  #[must_use]
  pub const fn name(self) -> &'static str {
    self.name
  }

  /// Reads a record type code and returns its record type.
  ///
  /// Mirrors `RecordTypeSerializer.parse(it)`, which reads a 16-bit code and
  /// throws `IllegalArgumentException` when no type has it.
  pub(crate) fn parse(reader: &mut WireReader<'_>) -> Result<Self, WireFormatError> {
    let code = reader.read_u16()?;
    Self::from_code(code).ok_or(WireFormatError::UnknownRecordType(code))
  }
}

impl fmt::Display for RecordType {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str(self.name)
  }
}
