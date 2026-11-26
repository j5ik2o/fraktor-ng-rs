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

//! The flags field of a DNS message header.

#[cfg(test)]
#[path = "message_flags_test.rs"]
mod tests;

const ANSWER: u16 = 0x8000;
const AUTHORITATIVE_ANSWER: u16 = 1 << 10;
const TRUNCATED: u16 = 1 << 9;
const RECURSION_DESIRED: u16 = 1 << 8;
const RECURSION_AVAILABLE: u16 = 1 << 7;

/// The 16-bit flags field of a DNS message header.
///
/// Mirrors `org.apache.pekko.io.dns.internal.MessageFlags` for the single-bit
/// flags. The operation code and response code fields (`opCode`,
/// `responseCode`) and building flags for a query (`MessageFlags.apply`) are
/// not part of this port.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MessageFlags {
  bits: u16,
}

impl MessageFlags {
  /// Wraps the flags field as a message holds it.
  #[must_use]
  pub const fn new(bits: u16) -> Self {
    Self { bits }
  }

  /// Returns the flags field as a message holds it.
  #[must_use]
  pub const fn bits(self) -> u16 {
    self.bits
  }

  /// Returns whether the message is a query, that is, its QR bit is clear.
  #[must_use]
  pub const fn is_query(self) -> bool {
    self.bits & ANSWER == 0
  }

  /// Returns whether the message is an answer, that is, its QR bit is set.
  #[must_use]
  pub const fn is_answer(self) -> bool {
    !self.is_query()
  }

  /// Returns whether the AA bit is set.
  #[must_use]
  pub const fn is_authoritative_answer(self) -> bool {
    self.bits & AUTHORITATIVE_ANSWER != 0
  }

  /// Returns whether the TC bit is set: the message was cut to fit the
  /// transport.
  #[must_use]
  pub const fn is_truncated(self) -> bool {
    self.bits & TRUNCATED != 0
  }

  /// Returns whether the RD bit is set.
  #[must_use]
  pub const fn is_recursion_desired(self) -> bool {
    self.bits & RECURSION_DESIRED != 0
  }

  /// Returns whether the RA bit is set.
  #[must_use]
  pub const fn is_recursion_available(self) -> bool {
    self.bits & RECURSION_AVAILABLE != 0
  }
}
