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

//! The error a stream reports when it ends before the bytes it must skip.

use core::fmt;

/// [`ByteStringInputStream::skip_n_bytes`](super::ByteStringInputStream::skip_n_bytes)
/// reached the end of the stream before it had skipped the bytes it was asked
/// to.
///
/// Mirrors the `java.io.EOFException` that `InputStream.skipNBytes` throws.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EndOfStreamError;

impl fmt::Display for EndOfStreamError {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str("end of stream")
  }
}

impl core::error::Error for EndOfStreamError {}
