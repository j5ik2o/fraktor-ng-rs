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

//! An immutable sequence of bytes, kept as one or more shared byte arrays.

use alloc::{string::String, vec::Vec};

use super::{ByteStringBuilder, ByteStringInputStream};
use crate::kernel::byte_iterator::ByteIterator;
use crate::kernel::sync::Arc;

#[cfg(test)]
#[path = "byte_string_value_test.rs"]
mod tests;

/// An immutable sequence of bytes.
///
/// Mirrors the parts of `org.apache.pekko.util.ByteString` that reading needs.
/// Pekko has three representations: a whole array (`ByteString1C`), a slice of
/// an array (`ByteString1`), and a concatenation (`ByteStrings`). This type
/// covers all three with a list of shared arrays: concatenating shares the
/// arrays of both operands instead of copying them, as `ByteStrings` does.
#[derive(Clone, Debug, Default)]
pub struct ByteString {
  // Never holds an empty array.
  arrays: Vec<Arc<[u8]>>,
}

impl ByteString {
  /// Creates a byte string with no bytes.
  ///
  /// Corresponds to Pekko's `ByteString.empty`.
  #[must_use]
  pub fn empty() -> Self {
    Self { arrays: Vec::new() }
  }

  /// Creates a byte string from the UTF-8 encoding of `string`.
  ///
  /// Corresponds to Pekko's `ByteString1C.fromString(string)` and
  /// `ByteString1.fromString(string)`, which also encode as UTF-8.
  #[must_use]
  pub fn from_string(string: &str) -> Self {
    Self::from_bytes(string.as_bytes())
  }

  /// Creates a byte string from a copy of up to `length` bytes of `array`,
  /// starting at `offset`.
  ///
  /// Corresponds to Pekko's `ByteString.fromArray(array, offset, length)`.
  /// Like Pekko's, it copies only the bytes that exist: an `offset` past the
  /// end gives an empty byte string, and a `length` past the end stops at the
  /// end.
  #[must_use]
  pub fn from_array(array: &[u8], offset: usize, length: usize) -> Self {
    let start = offset.min(array.len());
    let end = start + length.min(array.len() - start);
    Self::from_bytes(&array[start..end])
  }

  fn from_bytes(bytes: &[u8]) -> Self {
    if bytes.is_empty() {
      Self::empty()
    } else {
      Self { arrays: alloc::vec![Arc::from(bytes)] }
    }
  }

  /// Creates a byte string from the low byte of each value in `ints`, in order.
  ///
  /// Corresponds to Pekko's `fromInts(array: Int*)`, which stores each `Int` as
  /// a `Byte`. A value outside the range of a byte keeps only its low eight
  /// bits, as Rust's `as u8` does.
  #[must_use]
  pub fn from_ints(ints: &[i32]) -> Self {
    let bytes: Vec<u8> = ints.iter().map(|&value| value as u8).collect();
    Self::from_bytes(&bytes)
  }

  // Takes over `arrays` without copying or filtering; the caller passes only
  // non-empty arrays, so the byte string keeps its invariant.
  pub(super) fn from_arrays(arrays: Vec<Arc<[u8]>>) -> Self {
    Self { arrays }
  }

  // The shared arrays, in order. Never holds an empty array.
  pub(super) fn arrays(&self) -> &[Arc<[u8]>] {
    &self.arrays
  }

  /// Creates an empty builder for assembling a byte string from many bytes.
  ///
  /// Corresponds to Pekko's `newBuilder`.
  #[must_use]
  pub fn new_builder() -> ByteStringBuilder {
    ByteStringBuilder::default()
  }

  /// Creates an empty builder for assembling a byte string from many bytes.
  ///
  /// Corresponds to Pekko's Java API `createBuilder`.
  #[must_use]
  pub fn create_builder() -> ByteStringBuilder {
    ByteStringBuilder::default()
  }

  /// Returns a byte string with the bytes of `self` followed by the bytes of
  /// `other`.
  ///
  /// Corresponds to Pekko's `ByteStrings(self, other)` and `self ++ other`.
  /// Both operands' arrays are shared, not copied.
  #[must_use]
  pub fn concat(&self, other: &ByteString) -> Self {
    let arrays = self.arrays.iter().chain(other.arrays.iter()).cloned().collect();
    Self { arrays }
  }

  /// Returns an iterator over the bytes, in order.
  ///
  /// Corresponds to Pekko's `iterator`.
  #[must_use]
  pub fn iterator(&self) -> ByteIterator {
    ByteIterator::over_arrays(self.arrays.clone())
  }

  /// Returns a stream that reads the bytes from the start, in order.
  ///
  /// Corresponds to Pekko's `asInputStream`. The stream supports marks when
  /// the byte string holds at most one array, as the stream of Pekko's
  /// `ByteString1C` and `ByteString1` does, and not when it holds several, as
  /// the stream of Pekko's `ByteStrings` does not.
  #[must_use]
  pub fn as_input_stream(&self) -> ByteStringInputStream {
    ByteStringInputStream::new(self.iterator(), self.arrays.len() <= 1)
  }
}

impl ByteString {
  /// Creates a byte string that holds `bytes` itself, without copying them.
  ///
  /// Corresponds to Pekko's `ByteString.fromArrayUnsafe(array)`, which wraps
  /// the array as a `ByteString1C`. An empty array gives an empty byte string.
  #[must_use]
  pub fn from_shared(_bytes: Arc<[u8]>) -> Self {
    todo!("ByteString::from_shared is not implemented yet")
  }

  /// Creates a byte string that holds up to `length` bytes of `bytes`,
  /// starting at `offset`, without copying them.
  ///
  /// Corresponds to Pekko's `ByteString.fromArrayUnsafe(array, offset,
  /// length)`, which wraps the range as a `ByteString1`. The range stops at
  /// the end of the array, as in [`from_array`](Self::from_array); an empty
  /// range gives an empty byte string.
  #[must_use]
  pub fn from_shared_range(_bytes: Arc<[u8]>, _offset: usize, _length: usize) -> Self {
    todo!("ByteString::from_shared_range is not implemented yet")
  }

  /// Creates a compact byte string from the bytes of `iter`, going over it
  /// once.
  ///
  /// Corresponds to Pekko's `ByteString(iterableOnce)`.
  #[must_use]
  pub fn from_iterator(_iter: impl IntoIterator<Item = u8>) -> Self {
    todo!("ByteString::from_iterator is not implemented yet")
  }

  /// Returns the number of bytes.
  ///
  /// Corresponds to Pekko's `length` and `size`.
  #[must_use]
  pub fn len(&self) -> usize {
    todo!("ByteString::len is not implemented yet")
  }

  /// Returns whether the byte string has no bytes.
  ///
  /// Corresponds to Pekko's `isEmpty`.
  #[must_use]
  pub fn is_empty(&self) -> bool {
    todo!("ByteString::is_empty is not implemented yet")
  }

  /// Returns the byte at `index`, or `None` when `index` is not less than the
  /// length.
  ///
  /// Corresponds to Pekko's `apply(index)`.
  #[must_use]
  pub fn get(&self, _index: usize) -> Option<u8> {
    todo!("ByteString::get is not implemented yet")
  }

  /// Returns the first byte, or `None` when the byte string is empty.
  ///
  /// Corresponds to Pekko's `head`.
  #[must_use]
  pub fn head(&self) -> Option<u8> {
    todo!("ByteString::head is not implemented yet")
  }

  /// Returns the last byte, or `None` when the byte string is empty.
  ///
  /// Corresponds to Pekko's `last`.
  #[must_use]
  pub fn last(&self) -> Option<u8> {
    todo!("ByteString::last is not implemented yet")
  }

  /// Returns all bytes but the first; empty for an empty byte string.
  ///
  /// Corresponds to Pekko's `tail`, which is `drop(1)`.
  #[must_use]
  pub fn tail(&self) -> Self {
    todo!("ByteString::tail is not implemented yet")
  }

  /// Returns all bytes but the last; empty for an empty byte string.
  ///
  /// Corresponds to Pekko's `init`, which is `dropRight(1)`.
  #[must_use]
  pub fn init(&self) -> Self {
    todo!("ByteString::init is not implemented yet")
  }

  /// Returns the first `n` bytes, or all of them when there are fewer.
  ///
  /// Corresponds to Pekko's `take(n)`.
  #[must_use]
  pub fn take(&self, _n: usize) -> Self {
    todo!("ByteString::take is not implemented yet")
  }

  /// Returns the bytes after the first `n`, or none when there are fewer.
  ///
  /// Corresponds to Pekko's `drop(n)`.
  #[must_use]
  pub fn drop(&self, _n: usize) -> Self {
    todo!("ByteString::drop is not implemented yet")
  }

  /// Returns the last `n` bytes, or all of them when there are fewer.
  ///
  /// Corresponds to Pekko's `takeRight(n)`.
  #[must_use]
  pub fn take_right(&self, _n: usize) -> Self {
    todo!("ByteString::take_right is not implemented yet")
  }

  /// Returns the bytes before the last `n`, or none when there are fewer.
  ///
  /// Corresponds to Pekko's `dropRight(n)`.
  #[must_use]
  pub fn drop_right(&self, _n: usize) -> Self {
    todo!("ByteString::drop_right is not implemented yet")
  }

  /// Returns the bytes from index `from` up to, not including, index `until`.
  ///
  /// Corresponds to Pekko's `slice(from, until)`.
  #[must_use]
  pub fn slice(&self, _from: usize, _until: usize) -> Self {
    todo!("ByteString::slice is not implemented yet")
  }

  /// Returns the first `at` bytes and the rest.
  ///
  /// Corresponds to Pekko's `splitAt(n)`.
  #[must_use]
  pub fn split_at(&self, _at: usize) -> (Self, Self) {
    todo!("ByteString::split_at is not implemented yet")
  }

  /// Returns the longest prefix whose bytes satisfy `pred`, and the rest.
  ///
  /// Corresponds to Pekko's `span(p)`.
  #[must_use]
  pub fn span(&self, _pred: impl FnMut(u8) -> bool) -> (Self, Self) {
    todo!("ByteString::span is not implemented yet")
  }

  /// Returns the longest prefix whose bytes satisfy `pred`.
  ///
  /// Corresponds to Pekko's `takeWhile(p)`.
  #[must_use]
  pub fn take_while(&self, _pred: impl FnMut(u8) -> bool) -> Self {
    todo!("ByteString::take_while is not implemented yet")
  }

  /// Returns the bytes after the longest prefix whose bytes satisfy `pred`.
  ///
  /// Corresponds to Pekko's `dropWhile(p)`.
  #[must_use]
  pub fn drop_while(&self, _pred: impl FnMut(u8) -> bool) -> Self {
    todo!("ByteString::drop_while is not implemented yet")
  }

  /// Returns the index of the first byte that satisfies `pred`.
  ///
  /// Corresponds to Pekko's `indexWhere(p)`, with `None` for its `-1`.
  #[must_use]
  pub fn index_where(&self, _pred: impl FnMut(u8) -> bool) -> Option<usize> {
    todo!("ByteString::index_where is not implemented yet")
  }

  /// Returns the index of the first byte at or after `from` that satisfies
  /// `pred`.
  ///
  /// Corresponds to Pekko's `indexWhere(p, from)`, with `None` for its `-1`.
  #[must_use]
  pub fn index_where_from(&self, _pred: impl FnMut(u8) -> bool, _from: usize) -> Option<usize> {
    todo!("ByteString::index_where_from is not implemented yet")
  }

  /// Returns the index of the first byte equal to `elem`.
  ///
  /// Corresponds to Pekko's `indexOf(elem)`, with `None` for its `-1`.
  #[must_use]
  pub fn index_of(&self, _elem: u8) -> Option<usize> {
    todo!("ByteString::index_of is not implemented yet")
  }

  /// Returns the index of the first byte at or after `from` equal to `elem`.
  ///
  /// Corresponds to Pekko's `indexOf(elem, from)`, with `None` for its `-1`.
  #[must_use]
  pub fn index_of_from(&self, _elem: u8, _from: usize) -> Option<usize> {
    todo!("ByteString::index_of_from is not implemented yet")
  }

  /// Returns a copy of the bytes, in order.
  ///
  /// Corresponds to Pekko's `toArray`.
  #[must_use]
  pub fn to_vec(&self) -> Vec<u8> {
    todo!("ByteString::to_vec is not implemented yet")
  }

  /// Copies up to `len` bytes into `dest` from index `start` on, and returns
  /// how many it copied.
  ///
  /// Corresponds to Pekko's `copyToArray(dest, start, len)`.
  pub fn copy_to_array(&self, _dest: &mut [u8], _start: usize, _len: usize) -> usize {
    todo!("ByteString::copy_to_array is not implemented yet")
  }

  /// Returns the bytes in groups of `size`; the last group may be shorter.
  ///
  /// Corresponds to Pekko's `grouped(size)`.
  ///
  /// # Panics
  ///
  /// Panics when `size` is zero, where Pekko throws an
  /// `IllegalArgumentException`.
  pub fn grouped(&self, _size: usize) -> impl Iterator<Item = ByteString> {
    core::iter::from_fn(|| todo!("ByteString::grouped is not implemented yet"))
  }

  /// Returns whether the bytes fill one whole array.
  ///
  /// Corresponds to Pekko's `isCompact`.
  #[must_use]
  pub fn is_compact(&self) -> bool {
    todo!("ByteString::is_compact is not implemented yet")
  }

  /// Returns a byte string with the same bytes in one whole array.
  ///
  /// Corresponds to Pekko's `compact`.
  #[must_use]
  pub fn compact(&self) -> Self {
    todo!("ByteString::compact is not implemented yet")
  }

  /// Decodes the bytes as UTF-8, replacing malformed sequences with U+FFFD.
  ///
  /// Corresponds to Pekko's `decodeString(StandardCharsets.UTF_8)`.
  #[must_use]
  pub fn decode_string(&self) -> String {
    todo!("ByteString::decode_string is not implemented yet")
  }

  /// Encodes the bytes as padded RFC 4648 Base64.
  ///
  /// Corresponds to Pekko's `encodeBase64`.
  #[must_use]
  pub fn encode_base64(&self) -> Self {
    todo!("ByteString::encode_base64 is not implemented yet")
  }

  /// Decodes the bytes as RFC 4648 Base64, or returns `None` when they are not
  /// valid Base64.
  ///
  /// Corresponds to Pekko's `decodeBase64`.
  #[must_use]
  pub fn decode_base64(&self) -> Option<Self> {
    todo!("ByteString::decode_base64 is not implemented yet")
  }

  /// Returns the bytes as slices of the shared arrays, at least one.
  ///
  /// Corresponds to Pekko's `asByteBuffers`.
  pub fn as_byte_slices(&self) -> impl Iterator<Item = &[u8]> {
    core::iter::from_fn(|| todo!("ByteString::as_byte_slices is not implemented yet"))
  }

  /// Returns the non-empty slices of the shared arrays, none for an empty byte
  /// string.
  ///
  /// Corresponds to Pekko's `asByteBuffersUnsafe`.
  pub fn as_non_empty_byte_slices(&self) -> impl Iterator<Item = &[u8]> {
    core::iter::from_fn(|| todo!("ByteString::as_non_empty_byte_slices is not implemented yet"))
  }
}

impl core::fmt::Display for ByteString {
  fn fmt(&self, _f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
    todo!("Display for ByteString is not implemented yet")
  }
}

impl PartialEq for ByteString {
  fn eq(&self, other: &Self) -> bool {
    self.iterator().eq(other.iterator())
  }
}

impl Eq for ByteString {}
