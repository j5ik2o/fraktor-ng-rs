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

//! The error a stream that does not support marks reports on reset.

use core::fmt;

/// [`ByteStringInputStream::reset`](super::ByteStringInputStream::reset) was
/// called on a stream that does not support marks.
///
/// Mirrors the `IOException("mark/reset not supported")` that Java's
/// `InputStream.reset()` throws, as the `java.io.SequenceInputStream` of
/// Pekko's concatenated `ByteStrings` does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MarkNotSupportedError;

impl fmt::Display for MarkNotSupportedError {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str("mark/reset not supported")
  }
}

impl core::error::Error for MarkNotSupportedError {}
