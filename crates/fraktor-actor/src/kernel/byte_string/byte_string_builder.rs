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

//! A builder that assembles a [`ByteString`] from many bytes.

use alloc::vec::Vec;

use super::{ByteChunk, ByteString};
use crate::kernel::sync::Arc;

#[cfg(test)]
#[path = "byte_string_builder_test.rs"]
mod tests;

/// Builds a [`ByteString`] by adding bytes at the end.
///
/// Mirrors `org.apache.pekko.util.ByteStringBuilder`, but only the operations
/// that assembling needs. It keeps the confirmed bytes as shared arrays and the
/// bytes added with [`add_all`](Self::add_all) as one pending block, so
/// [`append`](Self::append) shares the arrays of a byte string instead of
/// copying them, as Pekko's does.
#[derive(Clone, Debug, Default)]
pub struct ByteStringBuilder {
  // Never holds an empty chunk.
  chunks: Vec<ByteChunk>,
  // Not yet confirmed into `chunks`; copied once `append` or `result` needs it.
  pending: Vec<u8>,
}

impl ByteStringBuilder {
  /// Adds the bytes of `bytes`, in order, at the end.
  ///
  /// Corresponds to Pekko's `addAll(xs: IterableOnce[Byte])` and its alias
  /// `++=(xs: IterableOnce[Byte])`.
  pub fn add_all<I: IntoIterator<Item = u8>>(&mut self, bytes: I) -> &mut Self {
    self.pending.extend(bytes);
    self
  }

  /// Adds the bytes of `bytes` at the end, sharing its arrays instead of
  /// copying them. An empty `bytes` changes nothing.
  ///
  /// Corresponds to Pekko's `addAll(bytes: ByteString)`, its alias
  /// `++=(bytes: ByteString)`, and the Java API `append(bs)`.
  pub fn append(&mut self, bytes: &ByteString) -> &mut Self {
    if bytes.arrays().is_empty() {
      return self;
    }
    self.confirm_pending();
    self.chunks.extend(bytes.arrays().iter().cloned());
    self
  }

  /// Returns a byte string of every byte added so far, in the order they were
  /// added. The builder can still be used after this, and a returned byte
  /// string never changes when more bytes are added.
  ///
  /// Corresponds to Pekko's `result()`.
  #[must_use]
  pub fn result(&self) -> ByteString {
    let mut chunks = self.chunks.clone();
    if !self.pending.is_empty() {
      chunks.push(ByteChunk::full(Arc::from(self.pending.as_slice())));
    }
    ByteString::from_chunks(chunks)
  }

  // Moves the pending bytes into `chunks` as one chunk, before more chunks are
  // added, so the added bytes keep their order.
  fn confirm_pending(&mut self) {
    if !self.pending.is_empty() {
      let bytes = core::mem::take(&mut self.pending);
      self.chunks.push(ByteChunk::full(Arc::from(bytes)));
    }
  }
}
