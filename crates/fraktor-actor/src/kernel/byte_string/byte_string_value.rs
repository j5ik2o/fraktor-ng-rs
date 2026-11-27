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
/// One part of a byte string: a range of a shared array.
///
/// The range is never empty, and `start + len` never passes the end of
/// `bytes`.
#[derive(Clone, Debug)]
pub(crate) struct ByteChunk {
  bytes: Arc<[u8]>,
  start: usize,
  len: usize,
}

impl ByteChunk {
  // Creates a chunk over `bytes[start..start + len]`, checking the invariant
  // here so that no caller has to: the range is never empty and never passes
  // the end of the array.
  pub(crate) fn new(bytes: Arc<[u8]>, start: usize, len: usize) -> Self {
    assert!(len > 0, "ByteChunk length must be positive: len={len}");
    assert!(
      matches!(start.checked_add(len), Some(end) if end <= bytes.len()),
      "ByteChunk range must lie inside its array: start={start} len={len} array_len={}",
      bytes.len()
    );
    Self { bytes, start, len }
  }

  pub(crate) fn full(bytes: Arc<[u8]>) -> Self {
    let len = bytes.len();
    Self::new(bytes, 0, len)
  }

  pub(crate) fn len(&self) -> usize {
    self.len
  }

  pub(crate) fn as_slice(&self) -> &[u8] {
    &self.bytes[self.start..self.start + self.len]
  }

  pub(crate) fn is_full(&self) -> bool {
    self.start == 0 && self.len == self.bytes.len()
  }
}

#[derive(Clone, Debug, Default)]
pub struct ByteString {
  // Never holds an empty chunk.
  chunks: Vec<ByteChunk>,
}

impl ByteString {
  /// Creates a byte string with no bytes.
  ///
  /// Corresponds to Pekko's `ByteString.empty`.
  #[must_use]
  pub fn empty() -> Self {
    Self { chunks: Vec::new() }
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
      Self { chunks: alloc::vec![ByteChunk::full(Arc::from(bytes))] }
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

  // Takes over `chunks` without copying or filtering; the caller passes only
  // non-empty chunks, so the byte string keeps its invariant.
  pub(super) fn from_chunks(chunks: Vec<ByteChunk>) -> Self {
    Self { chunks }
  }

  // The parts this byte string holds, in order. Never holds an empty part.
  pub(super) fn arrays(&self) -> &[ByteChunk] {
    &self.chunks
  }

  // The bytes from `from` on, for `count` bytes at most, sharing the arrays.
  fn range(&self, from: usize, count: usize) -> Self {
    if count == 0 || from >= self.len() {
      return Self::empty();
    }
    let end = from.saturating_add(count).min(self.len());
    let mut chunks = Vec::new();
    let mut offset = 0_usize;
    for chunk in &self.chunks {
      let low = offset;
      // The chunks together can hold more bytes than a `usize` counts on a
      // 32-bit target, because concatenating shares arrays without copying.
      // That total has no position type to hold it, so it is a contract
      // violation with a clear message, not a wrapped or silent value.
      let high = offset
        .checked_add(chunk.len)
        .expect("ByteString position overflow: the chunks together hold more bytes than a usize can count");
      let (start, stop) = (from.max(low), end.min(high));
      if start < stop {
        chunks.push(ByteChunk::new(chunk.bytes.clone(), chunk.start + (start - low), stop - start));
      }
      offset = high;
      if offset >= end {
        break;
      }
    }
    Self { chunks }
  }

  fn prefix_len(&self, mut pred: impl FnMut(u8) -> bool) -> usize {
    let mut count = 0_usize;
    for chunk in &self.chunks {
      for &byte in chunk.as_slice() {
        if !pred(byte) {
          return count;
        }
        count += 1;
      }
    }
    count
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
    if other.chunks.is_empty() {
      return self.clone();
    }
    if self.chunks.is_empty() {
      return other.clone();
    }
    if let ([left], [right]) = (self.chunks.as_slice(), other.chunks.as_slice()) {
      if Arc::ptr_eq(&left.bytes, &right.bytes) && left.start + left.len == right.start {
        let merged = ByteChunk::new(left.bytes.clone(), left.start, left.len + right.len);
        return Self { chunks: alloc::vec![merged] };
      }
    }
    let chunks = self.chunks.iter().chain(other.chunks.iter()).cloned().collect();
    Self { chunks }
  }

  /// Returns an iterator over the bytes, in order.
  ///
  /// Corresponds to Pekko's `iterator`.
  #[must_use]
  pub fn iterator(&self) -> ByteIterator {
    ByteIterator::over_chunks(self.chunks.clone())
  }

  /// Returns a stream that reads the bytes from the start, in order.
  ///
  /// Corresponds to Pekko's `asInputStream`. The stream supports marks when
  /// the byte string holds at most one array, as the stream of Pekko's
  /// `ByteString1C` and `ByteString1` does, and not when it holds several, as
  /// the stream of Pekko's `ByteStrings` does not.
  #[must_use]
  pub fn as_input_stream(&self) -> ByteStringInputStream {
    ByteStringInputStream::new(self.iterator(), self.chunks.len() <= 1)
  }
}

impl ByteString {
  /// Creates a byte string that holds `bytes` itself, without copying them.
  ///
  /// Corresponds to Pekko's `ByteString.fromArrayUnsafe(array)`, which wraps
  /// the array as a `ByteString1C`. An empty array gives an empty byte string.
  #[must_use]
  pub fn from_shared(bytes: Arc<[u8]>) -> Self {
    if bytes.is_empty() {
      Self::empty()
    } else {
      Self { chunks: alloc::vec![ByteChunk::full(bytes)] }
    }
  }

  /// Creates a byte string that holds up to `length` bytes of `bytes`,
  /// starting at `offset`, without copying them.
  ///
  /// Corresponds to Pekko's `ByteString.fromArrayUnsafe(array, offset,
  /// length)`, which wraps the range as a `ByteString1`. The range stops at
  /// the end of the array, as in [`from_array`](Self::from_array); an empty
  /// range gives an empty byte string.
  #[must_use]
  pub fn from_shared_range(bytes: Arc<[u8]>, offset: usize, length: usize) -> Self {
    let start = offset.min(bytes.len());
    let len = length.min(bytes.len() - start);
    if len == 0 {
      Self::empty()
    } else {
      Self { chunks: alloc::vec![ByteChunk::new(bytes, start, len)] }
    }
  }

  /// Creates a compact byte string from the bytes of `iter`, going over it
  /// once.
  ///
  /// Corresponds to Pekko's `ByteString(iterableOnce)`.
  #[must_use]
  pub fn from_iterator(iter: impl IntoIterator<Item = u8>) -> Self {
    let bytes: Vec<u8> = iter.into_iter().collect();
    Self::from_bytes(&bytes)
  }

  /// Returns the number of bytes.
  ///
  /// Corresponds to Pekko's `length` and `size`.
  #[must_use]
  pub fn len(&self) -> usize {
    self.chunks.iter().fold(0_usize, |total, chunk| total.saturating_add(chunk.len))
  }

  /// Returns whether the byte string has no bytes.
  ///
  /// Corresponds to Pekko's `isEmpty`.
  #[must_use]
  pub fn is_empty(&self) -> bool {
    self.chunks.is_empty()
  }

  /// Returns the byte at `index`, or `None` when `index` is not less than the
  /// length.
  ///
  /// Corresponds to Pekko's `apply(index)`.
  #[must_use]
  pub fn get(&self, index: usize) -> Option<u8> {
    let mut offset = index;
    for chunk in &self.chunks {
      if offset < chunk.len {
        return Some(chunk.as_slice()[offset]);
      }
      offset -= chunk.len;
    }
    None
  }

  /// Returns the first byte, or `None` when the byte string is empty.
  ///
  /// Corresponds to Pekko's `head`.
  #[must_use]
  pub fn head(&self) -> Option<u8> {
    self.chunks.first().map(|chunk| chunk.as_slice()[0])
  }

  /// Returns the last byte, or `None` when the byte string is empty.
  ///
  /// Corresponds to Pekko's `last`.
  #[must_use]
  pub fn last(&self) -> Option<u8> {
    self.chunks.last().map(|chunk| chunk.as_slice()[chunk.len - 1])
  }

  /// Returns all bytes but the first; empty for an empty byte string.
  ///
  /// Corresponds to Pekko's `tail`, which is `drop(1)`.
  #[must_use]
  pub fn tail(&self) -> Self {
    self.drop(1)
  }

  /// Returns all bytes but the last; empty for an empty byte string.
  ///
  /// Corresponds to Pekko's `init`, which is `dropRight(1)`.
  #[must_use]
  pub fn init(&self) -> Self {
    self.drop_right(1)
  }

  /// Returns the first `n` bytes, or all of them when there are fewer.
  ///
  /// Corresponds to Pekko's `take(n)`.
  #[must_use]
  pub fn take(&self, n: usize) -> Self {
    if n >= self.len() {
      return self.clone();
    }
    self.range(0, n)
  }

  /// Returns the bytes after the first `n`, or none when there are fewer.
  ///
  /// Corresponds to Pekko's `drop(n)`.
  #[must_use]
  pub fn drop(&self, n: usize) -> Self {
    if n == 0 {
      return self.clone();
    }
    let len = self.len();
    self.range(n, len.saturating_sub(n))
  }

  /// Returns the last `n` bytes, or all of them when there are fewer.
  ///
  /// Corresponds to Pekko's `takeRight(n)`.
  #[must_use]
  pub fn take_right(&self, n: usize) -> Self {
    let len = self.len();
    self.range(len.saturating_sub(n), n)
  }

  /// Returns the bytes before the last `n`, or none when there are fewer.
  ///
  /// Corresponds to Pekko's `dropRight(n)`.
  #[must_use]
  pub fn drop_right(&self, n: usize) -> Self {
    let len = self.len();
    self.take(len.saturating_sub(n))
  }

  /// Returns the bytes from index `from` up to, not including, index `until`.
  ///
  /// Corresponds to Pekko's `slice(from, until)`.
  #[must_use]
  pub fn slice(&self, from: usize, until: usize) -> Self {
    let len = self.len();
    if from >= len || until == 0 || from >= until {
      return Self::empty();
    }
    if from == 0 && until >= len {
      return self.clone();
    }
    let end = until.min(len);
    self.range(from, end - from)
  }

  /// Returns the first `at` bytes and the rest.
  ///
  /// Corresponds to Pekko's `splitAt(n)`.
  #[must_use]
  pub fn split_at(&self, at: usize) -> (Self, Self) {
    (self.take(at), self.drop(at))
  }

  /// Returns the longest prefix whose bytes satisfy `pred`, and the rest.
  ///
  /// Corresponds to Pekko's `span(p)`.
  #[must_use]
  pub fn span(&self, pred: impl FnMut(u8) -> bool) -> (Self, Self) {
    let count = self.prefix_len(pred);
    self.split_at(count)
  }

  /// Returns the longest prefix whose bytes satisfy `pred`.
  ///
  /// Corresponds to Pekko's `takeWhile(p)`.
  #[must_use]
  pub fn take_while(&self, pred: impl FnMut(u8) -> bool) -> Self {
    let count = self.prefix_len(pred);
    self.take(count)
  }

  /// Returns the bytes after the longest prefix whose bytes satisfy `pred`.
  ///
  /// Corresponds to Pekko's `dropWhile(p)`.
  #[must_use]
  pub fn drop_while(&self, pred: impl FnMut(u8) -> bool) -> Self {
    let count = self.prefix_len(pred);
    self.drop(count)
  }

  /// Returns the index of the first byte that satisfies `pred`.
  ///
  /// Corresponds to Pekko's `indexWhere(p)`, with `None` for its `-1`.
  #[must_use]
  pub fn index_where(&self, pred: impl FnMut(u8) -> bool) -> Option<usize> {
    self.index_where_from(pred, 0)
  }

  /// Returns the index of the first byte at or after `from` that satisfies
  /// `pred`.
  ///
  /// Corresponds to Pekko's `indexWhere(p, from)`, with `None` for its `-1`.
  #[must_use]
  pub fn index_where_from(&self, mut pred: impl FnMut(u8) -> bool, from: usize) -> Option<usize> {
    let mut index = 0_usize;
    for chunk in &self.chunks {
      for &byte in chunk.as_slice() {
        if index >= from && pred(byte) {
          return Some(index);
        }
        index += 1;
      }
    }
    None
  }

  /// Returns the index of the first byte equal to `elem`.
  ///
  /// Corresponds to Pekko's `indexOf(elem)`, with `None` for its `-1`.
  #[must_use]
  pub fn index_of(&self, elem: u8) -> Option<usize> {
    self.index_of_from(elem, 0)
  }

  /// Returns the index of the first byte at or after `from` equal to `elem`.
  ///
  /// Corresponds to Pekko's `indexOf(elem, from)`, with `None` for its `-1`.
  #[must_use]
  pub fn index_of_from(&self, elem: u8, from: usize) -> Option<usize> {
    self.index_where_from(|byte| byte == elem, from)
  }

  /// Returns a copy of the bytes, in order.
  ///
  /// Corresponds to Pekko's `toArray`.
  #[must_use]
  pub fn to_vec(&self) -> Vec<u8> {
    let mut bytes = Vec::new();
    for chunk in &self.chunks {
      bytes.extend_from_slice(chunk.as_slice());
    }
    bytes
  }

  /// Copies up to `len` bytes into `dest` from index `start` on, and returns
  /// how many it copied.
  ///
  /// Corresponds to Pekko's `copyToArray(dest, start, len)`.
  pub fn copy_to_array(&self, dest: &mut [u8], start: usize, len: usize) -> usize {
    let to_copy = len.min(self.len()).min(dest.len().saturating_sub(start));
    if to_copy == 0 {
      return 0;
    }
    let mut copied = 0_usize;
    for chunk in &self.chunks {
      if copied >= to_copy {
        break;
      }
      let count = chunk.len.min(to_copy - copied);
      dest[start + copied..start + copied + count].copy_from_slice(&chunk.as_slice()[..count]);
      copied += count;
    }
    copied
  }

  /// Returns the bytes in groups of `size`; the last group may be shorter.
  ///
  /// Corresponds to Pekko's `grouped(size)`.
  ///
  /// # Panics
  ///
  /// Panics when `size` is zero, where Pekko throws an
  /// `IllegalArgumentException`.
  pub fn grouped(&self, size: usize) -> impl Iterator<Item = ByteString> {
    assert!(size > 0, "size={size} must be positive");
    let source = self.clone();
    let mut offset = 0_usize;
    core::iter::from_fn(move || {
      if offset >= source.len() {
        return None;
      }
      let group = source.range(offset, size);
      offset = offset.saturating_add(size);
      Some(group)
    })
  }

  /// Returns whether the bytes fill one whole array.
  ///
  /// Corresponds to Pekko's `isCompact`.
  #[must_use]
  pub fn is_compact(&self) -> bool {
    match self.chunks.as_slice() {
      | [] => true,
      | [chunk] => chunk.is_full(),
      | _ => false,
    }
  }

  /// Returns a byte string with the same bytes in one whole array.
  ///
  /// Corresponds to Pekko's `compact`.
  #[must_use]
  pub fn compact(&self) -> Self {
    if self.is_compact() {
      self.clone()
    } else {
      Self::from_bytes(&self.to_vec())
    }
  }

  /// Decodes the bytes as UTF-8, replacing malformed sequences with U+FFFD.
  ///
  /// Corresponds to Pekko's `decodeString(StandardCharsets.UTF_8)`.
  #[must_use]
  pub fn decode_string(&self) -> String {
    // The JDK replaces a UTF-8 encoded surrogate as one malformed unit:
    // `0xED`, the byte after it in `0xA0..=0xBF`, and one more continuation
    // byte when one follows, become a single U+FFFD. Every other malformed
    // unit follows the maximal subpart rule, which `from_utf8_lossy` already
    // implements, so the bytes are decoded in segments around those units.
    // The bytes are flattened first, so a unit that spans two chunks is
    // treated the same as one inside a chunk.
    let bytes = self.to_vec();
    let mut out = String::new();
    let mut start = 0_usize;
    let mut index = 0_usize;
    while index < bytes.len() {
      if bytes[index] == 0xED && bytes.get(index + 1).is_some_and(|byte| (0xA0..=0xBF).contains(byte)) {
        out.push_str(&String::from_utf8_lossy(&bytes[start..index]));
        out.push('\u{FFFD}');
        index += 2;
        if bytes.get(index).is_some_and(|byte| (0x80..=0xBF).contains(byte)) {
          index += 1;
        }
        start = index;
      } else {
        index += 1;
      }
    }
    out.push_str(&String::from_utf8_lossy(&bytes[start..]));
    out
  }

  /// Encodes the bytes as padded RFC 4648 Base64.
  ///
  /// Corresponds to Pekko's `encodeBase64`.
  #[must_use]
  pub fn encode_base64(&self) -> Self {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes = self.to_vec();
    if bytes.is_empty() {
      return Self::empty();
    }
    let mut out = Vec::new();
    for group in bytes.chunks(3) {
      let b0 = u32::from(group[0]);
      let b1 = group.get(1).copied().map_or(0, u32::from);
      let b2 = group.get(2).copied().map_or(0, u32::from);
      let triple = (b0 << 16) | (b1 << 8) | b2;
      out.push(ALPHABET[(triple >> 18) as usize & 63]);
      out.push(ALPHABET[(triple >> 12) as usize & 63]);
      out.push(if group.len() > 1 { ALPHABET[(triple >> 6) as usize & 63] } else { b'=' });
      out.push(if group.len() > 2 { ALPHABET[triple as usize & 63] } else { b'=' });
    }
    Self::from_bytes(&out)
  }

  /// Decodes the bytes as RFC 4648 Base64, or returns `None` when they are not
  /// valid Base64.
  ///
  /// Corresponds to Pekko's `decodeBase64`.
  #[must_use]
  pub fn decode_base64(&self) -> Option<Self> {
    fn value_of(byte: u8) -> Option<u32> {
      match byte {
        | b'A'..=b'Z' => Some(u32::from(byte - b'A')),
        | b'a'..=b'z' => Some(u32::from(byte - b'a') + 26),
        | b'0'..=b'9' => Some(u32::from(byte - b'0') + 52),
        | b'+' => Some(62),
        | b'/' => Some(63),
        | _ => None,
      }
    }
    let bytes = self.to_vec();
    if bytes.is_empty() {
      return Some(Self::empty());
    }
    if bytes.len() % 4 == 1 {
      return None;
    }
    let padding = match bytes.iter().position(|&byte| byte == b'=') {
      | Some(first) => {
        let count = bytes.len() - first;
        if count > 2 || bytes[first..].iter().any(|&byte| byte != b'=') {
          return None;
        }
        if !bytes.len().is_multiple_of(4) {
          return None;
        }
        if (count == 2 && first % 4 != 2) || (count == 1 && first % 4 != 3) {
          return None;
        }
        count
      },
      | None => 0,
    };
    if bytes[..bytes.len() - padding].iter().any(|&byte| value_of(byte).is_none()) {
      return None;
    }
    let mut out = Vec::new();
    let mut index = 0_usize;
    while index < bytes.len() {
      let rest = bytes.len() - index;
      let take = rest.min(4);
      let mut values = [0_u32; 4];
      for (slot, &byte) in values.iter_mut().zip(bytes[index..index + take].iter()) {
        *slot = if byte == b'=' { 0 } else { value_of(byte)? };
      }
      let triple = (values[0] << 18) | (values[1] << 12) | (values[2] << 6) | values[3];
      out.push((triple >> 16) as u8);
      let is_last = index + take == bytes.len();
      let output = if is_last && padding > 0 {
        3 - padding
      } else if is_last && take < 4 {
        take - 1
      } else {
        3
      };
      if output > 1 {
        out.push((triple >> 8) as u8);
      }
      if output > 2 {
        out.push(triple as u8);
      }
      index += take;
    }
    Some(Self::from_bytes(&out))
  }

  /// Returns the bytes as slices of the shared arrays, at least one.
  ///
  /// Corresponds to Pekko's `asByteBuffers`.
  pub fn as_byte_slices(&self) -> impl Iterator<Item = &[u8]> {
    let slices: Vec<&[u8]> = if self.chunks.is_empty() {
      alloc::vec![&[][..]]
    } else {
      self.chunks.iter().map(|chunk| chunk.as_slice()).collect()
    };
    slices.into_iter()
  }

  /// Returns the non-empty slices of the shared arrays, none for an empty byte
  /// string.
  ///
  /// Corresponds to Pekko's `asByteBuffersUnsafe`.
  pub fn as_non_empty_byte_slices(&self) -> impl Iterator<Item = &[u8]> {
    let slices: Vec<&[u8]> = self.chunks.iter().map(|chunk| chunk.as_slice()).collect();
    slices.into_iter()
  }
}

impl core::fmt::Display for ByteString {
  fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
    f.write_str("ByteString(")?;
    let mut shown = 0_usize;
    for chunk in &self.chunks {
      for &byte in chunk.as_slice() {
        if shown >= 100 {
          break;
        }
        if shown > 0 {
          f.write_str(", ")?;
        }
        core::fmt::Write::write_fmt(f, format_args!("{}", byte as i8))?;
        shown += 1;
      }
      if shown >= 100 {
        break;
      }
    }
    f.write_str(")")?;
    let len = self.len();
    if len > 100 {
      core::fmt::Write::write_fmt(f, format_args!("... and [{}] more", len - 100))?;
    }
    Ok(())
  }
}

impl PartialEq for ByteString {
  fn eq(&self, other: &Self) -> bool {
    self.iterator().eq(other.iterator())
  }
}

impl Eq for ByteString {}
