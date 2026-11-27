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

//! A stream that reads the bytes of a byte string.

use alloc::vec::Vec;

use super::{EndOfStreamError, IndexOutOfBoundsError, MarkNotSupportedError};
use crate::kernel::byte_iterator::ByteIterator;

#[cfg(test)]
#[path = "byte_string_input_stream_test.rs"]
mod tests;

/// Reads the bytes of a [`ByteString`](super::ByteString) from the start, in
/// order.
///
/// Mirrors what `ByteString.asInputStream` returns in Pekko, a
/// `java.io.InputStream`. Java reports the end of the stream as `-1`; this
/// type reports it as `None`.
///
/// Whether the stream supports [`mark`](Self::mark) and
/// [`reset`](Self::reset) follows Pekko's representations. A byte string of at
/// most one array is Pekko's `ByteString1C` or `ByteString1`, whose stream is
/// an `UnsynchronizedByteArrayInputStream` and supports marks. A byte string of
/// several arrays is Pekko's `ByteStrings`, whose stream is a
/// `java.io.SequenceInputStream` and does not.
#[derive(Clone, Debug)]
pub struct ByteStringInputStream {
  bytes: ByteIterator,
  // Where `reset` goes back to: the start of the stream until `mark` is called.
  // `None` when the stream does not support marks.
  marked: Option<ByteIterator>,
}

impl ByteStringInputStream {
  /// Creates a stream over `bytes` that supports marks when `mark_supported`
  /// is `true`.
  pub(crate) fn new(bytes: ByteIterator, mark_supported: bool) -> Self {
    let marked = mark_supported.then(|| bytes.clone());
    Self { bytes, marked }
  }

  /// Reads the next byte, or returns `None` at the end of the stream.
  ///
  /// Corresponds to Java's `InputStream.read()`.
  pub fn read(&mut self) -> Option<u8> {
    self.bytes.next()
  }

  /// Reads up to `buffer.len()` bytes into the start of `buffer` and returns
  /// how many it read, or returns `None` at the end of the stream.
  ///
  /// Corresponds to Java's `InputStream.read(buffer)`. As the `InputStream`
  /// contract and Pekko's `UnsynchronizedByteArrayInputStream` do, an empty
  /// `buffer` reads `Some(0)` without looking for the end of the stream.
  ///
  /// A read fills `buffer` across the parts of a concatenated byte string.
  /// Pekko's concatenation reads through `java.io.SequenceInputStream`, which
  /// stops each read at the end of the current part, so reading `a` followed
  /// by `bc` into a 3-byte buffer returns 1 byte there and 3 bytes here. The
  /// bytes and their order are the same; only how many one read returns
  /// differs, which the `InputStream` contract leaves to the implementation.
  pub fn read_into(&mut self, buffer: &mut [u8]) -> Option<usize> {
    if buffer.is_empty() {
      Some(0)
    } else if self.bytes.has_next() {
      Some(self.bytes.copy_into(buffer))
    } else {
      None
    }
  }

  /// Reads up to `length` bytes into `buffer[offset..offset + length]` and
  /// returns how many it read, or returns `None` at the end of the stream.
  ///
  /// Corresponds to Java's `InputStream.read(buffer, off, len)`. A zero
  /// `length` reads `Some(0)` without looking for the end of the stream, as
  /// [`read_into`](Self::read_into) does for an empty buffer.
  ///
  /// # Errors
  ///
  /// Returns [`IndexOutOfBoundsError`] when the range does not lie inside
  /// `buffer`, where Java throws an `IndexOutOfBoundsException`. The stream
  /// does not move.
  pub fn read_range(
    &mut self,
    buffer: &mut [u8],
    offset: usize,
    length: usize,
  ) -> Result<Option<usize>, IndexOutOfBoundsError> {
    let end = offset.checked_add(length).filter(|end| *end <= buffer.len()).ok_or(IndexOutOfBoundsError)?;
    Ok(self.read_into(&mut buffer[offset..end]))
  }

  /// Reads up to `length` bytes into `buffer[offset..offset + length]` and
  /// returns how many it read: `0` at the end of the stream.
  ///
  /// Corresponds to Java's `InputStream.readNBytes(buffer, off, len)`, which
  /// reads until `length` bytes or the end of the stream.
  ///
  /// # Errors
  ///
  /// Returns [`IndexOutOfBoundsError`] when the range does not lie inside
  /// `buffer`. The stream does not move.
  pub fn read_n_bytes_into(
    &mut self,
    buffer: &mut [u8],
    offset: usize,
    length: usize,
  ) -> Result<usize, IndexOutOfBoundsError> {
    Ok(self.read_range(buffer, offset, length)?.unwrap_or(0))
  }

  /// Reads every byte that is left.
  ///
  /// Corresponds to Java's `InputStream.readAllBytes()`.
  pub fn read_all_bytes(&mut self) -> Vec<u8> {
    self.read_n_bytes(usize::MAX)
  }

  /// Reads up to `length` bytes: `length`, or the bytes left when fewer are
  /// left.
  ///
  /// Corresponds to Java's `InputStream.readNBytes(len)`.
  pub fn read_n_bytes(&mut self, length: usize) -> Vec<u8> {
    let mut bytes = alloc::vec![0_u8; self.bytes.remaining_up_to(length)];
    self.bytes.copy_into(&mut bytes);
    bytes
  }

  /// Appends every byte that is left to `out`, and returns how many it
  /// appended.
  ///
  /// Corresponds to Java's `InputStream.transferTo(out)`. The kernel has no
  /// `std::io::Write`, so the output is a byte vector.
  pub fn transfer_to(&mut self, out: &mut Vec<u8>) -> usize {
    let bytes = self.read_all_bytes();
    out.extend_from_slice(&bytes);
    bytes.len()
  }

  /// Returns how many bytes are left to read.
  ///
  /// Corresponds to Java's `InputStream.available()`. Pekko's
  /// `UnsynchronizedByteArrayInputStream` returns every byte left; this stream
  /// does so for a concatenation of several arrays too, where Pekko's
  /// `java.io.SequenceInputStream` reports only the current part. A count that
  /// a `usize` cannot hold is reported as `usize::MAX`.
  #[must_use]
  pub fn available(&self) -> usize {
    self.bytes.remaining()
  }

  /// Returns whether the stream supports [`mark`](Self::mark) and
  /// [`reset`](Self::reset): `true` for a byte string of at most one array,
  /// `false` for a concatenation of several.
  ///
  /// Corresponds to Java's `InputStream.markSupported()`.
  #[must_use]
  pub const fn mark_supported(&self) -> bool {
    self.marked.is_some()
  }

  /// Marks the current position, so that [`reset`](Self::reset) goes back to
  /// it. Does nothing when the stream does not support marks.
  ///
  /// Corresponds to Java's `InputStream.mark(readAheadLimit)`. As Pekko's
  /// `UnsynchronizedByteArrayInputStream` and Java's `ByteArrayInputStream` do,
  /// the stream ignores `read_ahead_limit`: the mark stays valid however many
  /// bytes are read after it.
  pub fn mark(&mut self, _read_ahead_limit: usize) {
    if let Some(marked) = &mut self.marked {
      marked.clone_from(&self.bytes);
    }
  }

  /// Goes back to the position of the last [`mark`](Self::mark), or to the
  /// start of the stream when nothing was marked.
  ///
  /// Corresponds to Java's `InputStream.reset()`.
  ///
  /// # Errors
  ///
  /// Returns [`MarkNotSupportedError`] when the stream does not support marks,
  /// where Java throws an `IOException`. The position does not change.
  pub fn reset(&mut self) -> Result<(), MarkNotSupportedError> {
    let marked = self.marked.as_ref().ok_or(MarkNotSupportedError)?;
    self.bytes.clone_from(marked);
    Ok(())
  }

  /// Moves past up to `count` bytes without reading them, and returns how many
  /// it moved past: `count`, or the bytes left when fewer are left, so `0` at
  /// the end of the stream.
  ///
  /// Corresponds to Java's `InputStream.skip(n)`, for streams that support
  /// marks and for those that do not. Pekko's
  /// `UnsynchronizedByteArrayInputStream` also stops at the end since v2.0.0;
  /// in v1.7.1 it moved its position by the whole `n` even past the end, so a
  /// later skip there returned a negative count.
  ///
  /// `count` is a `usize`, so a negative count does not compile. This stands in
  /// for the excluded case "reject negative skip" of Pekko v2.0.0's
  /// `UnsynchronizedByteArrayInputStreamSpec` (L66-70), where `skip(-1)` throws
  /// `IllegalArgumentException`; it is not counted as one of the node's cases.
  ///
  /// ```compile_fail
  /// use fraktor_actor::kernel::byte_string::ByteString;
  ///
  /// let mut stream = ByteString::from_string("abc").as_input_stream();
  /// stream.skip(-1);
  /// ```
  ///
  /// The same code with a positive count compiles and runs, so the example above
  /// fails only because the count is negative:
  ///
  /// ```
  /// use fraktor_actor::kernel::byte_string::ByteString;
  ///
  /// let mut stream = ByteString::from_string("abc").as_input_stream();
  /// stream.skip(1);
  /// ```
  pub fn skip(&mut self, count: usize) -> usize {
    self.bytes.skip_bytes(count)
  }

  /// Moves past exactly `count` bytes without reading them.
  ///
  /// Corresponds to Java's `InputStream.skipNBytes(n)`.
  ///
  /// # Errors
  ///
  /// Returns [`EndOfStreamError`] when fewer than `count` bytes are left,
  /// where Java throws an `EOFException`. The stream is then at its end.
  pub fn skip_n_bytes(&mut self, count: usize) -> Result<(), EndOfStreamError> {
    if self.skip(count) == count {
      Ok(())
    } else {
      Err(EndOfStreamError)
    }
  }
}
