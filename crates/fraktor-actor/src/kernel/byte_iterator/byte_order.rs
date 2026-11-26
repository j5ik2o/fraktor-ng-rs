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

//! The order in which the bytes of a multi-byte number are read.

/// The order in which [`ByteIterator`](super::ByteIterator) reads the bytes of
/// a multi-byte number.
///
/// Mirrors `java.nio.ByteOrder`, the implicit order Pekko's
/// `ByteIterator.getShort`, `getInt` and `getLong` take.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ByteOrder {
  /// The most significant byte comes first, as `ByteOrder.BIG_ENDIAN`.
  BigEndian,
  /// The least significant byte comes first, as `ByteOrder.LITTLE_ENDIAN`.
  LittleEndian,
}
