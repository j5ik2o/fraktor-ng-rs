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

//! The error a stream reports when a buffer range lies outside the buffer.

use core::fmt;

/// The range `offset .. offset + length` given to
/// [`ByteStringInputStream::read_range`](super::ByteStringInputStream::read_range)
/// or [`read_n_bytes_into`](super::ByteStringInputStream::read_n_bytes_into)
/// does not lie inside the buffer.
///
/// Mirrors the `IndexOutOfBoundsException` that `Objects.checkFromIndexSize`
/// throws in Java's `InputStream.read(b, off, len)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexOutOfBoundsError;

impl fmt::Display for IndexOutOfBoundsError {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str("range out of bounds")
  }
}

impl core::error::Error for IndexOutOfBoundsError {}
