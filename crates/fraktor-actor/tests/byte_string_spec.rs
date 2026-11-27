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

// Ported from Pekko v2.0.0 (SHA 9fbd2283b1cf701c90a1223bf621942c191db61f):
//   actor-tests/src/test/scala/org/apache/pekko/util/ByteStringSpec.scala
// Original test names are kept as comments next to the ported cases, with the
// line of the original.
//
// Design: docs/design/issue-190-bytestring.md. This file holds group 1, the 43
// cases of the first half of "A ByteString" (original L2140-L2459), and the
// properties of group 1 (design section 4.3) after them. The other groups are
// added as modules later.
//
// The original's ScalaCheck `check` runs each property on 1000 inputs
// (`minSuccessful = 1000`, L40-L41). The port runs each one on 1000 inputs from
// a fixed-seed SplitMix64; a failure names the property, the seed, and the
// iteration, so it repeats. The generators copy `genSimpleByteString` (L43-L49),
// `arbitraryByteString` (L51-L58), `arbitraryByteStringSlice` (L62-L71) and
// `arbitraryByteStringGrouped` (L73-L80). ScalaCheck's size grows from 0 to 100
// over a run; the port's size is the iteration modulo 101. Half the values come
// from the copy of `arbitraryByteString`; the other half come in one of five
// shapes picked at random (empty, a whole array, part of an array, two
// fragments, three or more fragments), so that every shape is drawn often.
//
// Each generated value comes with a model: its bytes, and the part of each
// shared array it holds. `ByteString(bytes)` of the original is built as
// `ByteString::from_shared` of a fresh array, so the test knows where every
// array starts in memory. The model is computed from the generator's inputs,
// never from the byte string under test.
//
// The original API and its Rust counterpart:
//
// - `a ++ b` is `a.concat(&b)`; `size` and `length` are `len()`.
// - `ByteString1C(array)` is `ByteString::from_shared(array)` and
//   `ByteString1(array, offset, length)` is
//   `ByteString::from_shared_range(array, offset, length)`, the two forms of
//   `fromArrayUnsafe`. `ByteString1(array)` is the range over the whole array.
//   `ByteStrings(Vector(x, y))` is `x.concat(&y)`.
//   `ByteString1.fromString(s)` is `from_shared_range` over the UTF-8 bytes of
//   `s`; `ByteString(s)` and `ByteString(s, UTF_8)` are `from_string(s)`.
// - `expectByteString2` and `expectByteStrings` (L108-L119) test the class at
//   run time. The port has one type, so it counts the fragments
//   (`as_byte_slices`): two for `ByteString2`, three or more for `ByteStrings`.
// - `eq` and `theSameInstanceAs` on a non-empty value are checked by pointer:
//   the values must expose the same slices of the same arrays
//   (`as_non_empty_byte_slices`, the start pointer and the length of each).
//   For an empty value both sides expose no slice.
// - An `Int` argument of the original can be negative or larger than the
//   length. `usize` has no negative values; the original treats a negative
//   count or index like zero, so the port draws `0`, an index inside, the
//   length, the length plus one, `usize::MAX`, and a random `usize`.
// - `indexWhere`, `indexOf` return `-1` for no match; the port returns `None`.
// - `asByteBuffers` is `as_byte_slices` and `asByteBuffersUnsafe` is
//   `as_non_empty_byte_slices`, slices of the shared arrays instead of
//   `ByteBuffer`s. `getByteBuffers` is the Java alias of `asByteBuffers`
//   (`asByteBuffers.asJava`), so it is the same method.
// - `decodeString(UTF_8)` is `decode_string()`, `encodeBase64` is
//   `encode_base64()`, and `decodeBase64` is `decode_base64()`, which returns
//   `None` where the JDK decoder throws. `java.util.Base64.getEncoder` is a
//   reference encoder written in this file.
// - `toArray` is `to_vec()`, `apply(i)` is `get(i)`, `head` and `last` return
//   an `Option`. `foreach` and `foldLeft` are the `for_each` and `fold` of
//   `iterator()`; the sums add the bytes as signed `Byte`s into an `i32`, as the
//   original's `Int` accumulator does.
// - `toString` is `Display`.

use std::{
  panic::{self, AssertUnwindSafe},
  sync::Arc,
};

use fraktor_actor::kernel::byte_string::ByteString;

// ---------------------------------------------------------------------------
// Inputs

const MIN_SUCCESSFUL: usize = 1000;
const MAX_SIZE: usize = 100;

struct SplitMix64 {
  state: u64,
}

impl SplitMix64 {
  fn new(seed: u64) -> Self {
    Self { state: seed }
  }

  fn next_u64(&mut self) -> u64 {
    self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = self.state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
  }
}

// The part of a shared array that one fragment of a byte string holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Piece {
  base: usize,
  array_len: usize,
  start: usize,
  len: usize,
}

impl Piece {
  fn address(&self) -> usize {
    self.base + self.start
  }
}

// What a byte string must hold: its bytes and the parts of the arrays it
// shares, in order, with no empty part.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Model {
  bytes: Vec<u8>,
  pieces: Vec<Piece>,
}

impl Model {
  fn empty() -> Self {
    Self { bytes: Vec::new(), pieces: Vec::new() }
  }

  fn whole(array: &Arc<[u8]>) -> Self {
    let pieces = if array.is_empty() {
      Vec::new()
    } else {
      vec![Piece { base: base_of(array), array_len: array.len(), start: 0, len: array.len() }]
    };
    Self { bytes: array.to_vec(), pieces }
  }

  fn len(&self) -> usize {
    self.bytes.len()
  }

  fn slice(&self, from: usize, until: usize) -> Self {
    let until = until.min(self.len());
    if from >= until {
      return Self::empty();
    }
    let mut pieces = Vec::new();
    let mut offset = 0;
    for piece in &self.pieces {
      let (low, high) = (offset, offset + piece.len);
      let (start, end) = (from.max(low), until.min(high));
      if start < end {
        pieces.push(Piece { start: piece.start + (start - low), len: end - start, ..*piece });
      }
      offset = high;
    }
    Self { bytes: self.bytes[from..until].to_vec(), pieces }
  }

  fn take(&self, n: usize) -> Self {
    self.slice(0, n)
  }

  fn drop(&self, n: usize) -> Self {
    self.slice(n, self.len())
  }

  fn take_right(&self, n: usize) -> Self {
    self.drop(self.len().saturating_sub(n))
  }

  fn drop_right(&self, n: usize) -> Self {
    self.take(self.len().saturating_sub(n))
  }

  // Pekko joins two single fragments that are adjacent ranges of one array
  // (`ByteString1 ++ ByteString1`, ByteString.scala L667-L669); every other
  // concatenation keeps the fragments of both sides.
  fn concat(&self, other: &Self) -> Self {
    let bytes = [self.bytes.as_slice(), other.bytes.as_slice()].concat();
    let pieces = match (self.pieces.as_slice(), other.pieces.as_slice()) {
      | ([], right) => right.to_vec(),
      | (left, []) => left.to_vec(),
      | ([left], [right]) if left.base == right.base && left.start + left.len == right.start => {
        vec![Piece { len: left.len + right.len, ..*left }]
      },
      | (left, right) => [left, right].concat(),
    };
    Self { bytes, pieces }
  }

  fn is_compact(&self) -> bool {
    match self.pieces.as_slice() {
      | [] => true,
      | [piece] => piece.start == 0 && piece.len == piece.array_len,
      | _ => false,
    }
  }

  fn addresses(&self) -> Vec<(usize, usize)> {
    self.pieces.iter().map(|piece| (piece.address(), piece.len)).collect()
  }
}

// A generated byte string, its model, and the arrays it was built from, kept
// alive so that no other allocation can take their addresses.
#[derive(Clone, Debug)]
struct Sample {
  value: ByteString,
  model: Model,
  _arrays: Vec<Arc<[u8]>>,
}

impl Sample {
  fn empty() -> Self {
    Self { value: ByteString::empty(), model: Model::empty(), _arrays: Vec::new() }
  }

  fn len(&self) -> usize {
    self.model.len()
  }

  fn bytes(&self) -> &[u8] {
    &self.model.bytes
  }

  fn concat(&self, other: &Self) -> Self {
    Self {
      value: self.value.concat(&other.value),
      model: self.model.concat(&other.model),
      _arrays: [self._arrays.as_slice(), other._arrays.as_slice()].concat(),
    }
  }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Shape {
  Empty,
  WholeArray,
  PartOfArray,
  TwoFragments,
  ManyFragments,
}

const SHAPES: [Shape; 5] =
  [Shape::Empty, Shape::WholeArray, Shape::PartOfArray, Shape::TwoFragments, Shape::ManyFragments];

fn shape_of(model: &Model) -> Shape {
  match model.pieces.as_slice() {
    | [] => Shape::Empty,
    | [_] if model.is_compact() => Shape::WholeArray,
    | [_] => Shape::PartOfArray,
    | [_, _] => Shape::TwoFragments,
    | _ => Shape::ManyFragments,
  }
}

struct Gen {
  random: SplitMix64,
  size: usize,
}

impl Gen {
  fn new(seed: u64, iteration: usize) -> Self {
    let mut mixer = SplitMix64::new(seed ^ (iteration as u64).wrapping_mul(0xD1B5_4A32_D192_ED03));
    Self { random: SplitMix64::new(mixer.next_u64()), size: iteration % (MAX_SIZE + 1) }
  }

  // A value in `0..bound`; `bound` is positive.
  fn below(&mut self, bound: usize) -> usize {
    (self.random.next_u64() % bound as u64) as usize
  }

  // A value in `low..=high`, as ScalaCheck's `Gen.choose`.
  fn choose(&mut self, low: usize, high: usize) -> usize {
    low + self.below(high - low + 1)
  }

  fn byte(&mut self) -> u8 {
    self.random.next_u64() as u8
  }

  fn bytes(&mut self, n: usize) -> Vec<u8> {
    (0..n).map(|_| self.byte()).collect()
  }

  fn array(&mut self, n: usize) -> Arc<[u8]> {
    Arc::from(self.bytes(n))
  }

  // An index or a count for a value of length `len`: the original's `Int`,
  // whose negative values act as zero.
  fn index(&mut self, len: usize) -> usize {
    match self.below(6) {
      | 0 => 0,
      | 1 if len > 1 => self.choose(1, len - 1),
      | 1 | 2 => len,
      | 3 => len + 1,
      | 4 => usize::MAX,
      | _ => self.random.next_u64() as usize,
    }
  }

  // Original: `genSimpleByteString(min, max)` (L43-L49), `ByteString(b).slice(from, until)`.
  fn simple(&mut self, min: usize, max: usize) -> Sample {
    let n = self.choose(min, max);
    let array = self.array(n);
    let from = self.choose(0, n);
    let until = self.choose(from, n);
    Sample {
      value: ByteString::from_shared(array.clone()).slice(from, until),
      model: Model::whole(&array).slice(from, until),
      _arrays: vec![array],
    }
  }

  // Original: `arbitraryByteString` (L51-L58).
  fn arbitrary_byte_string(&mut self) -> Sample {
    let size = self.size;
    let chunks = self.choose(0, size);
    let max = 1.max(size / chunks.max(1));
    let mut result = Sample::empty();
    for _ in 0..chunks {
      let chunk = self.simple(1, max);
      result = result.concat(&chunk);
    }
    result
  }

  // A fragment of at least one byte: a whole array or a part of one.
  fn fragment(&mut self) -> Sample {
    if self.below(2) == 0 {
      let n = self.choose(1, 1.max(self.size));
      let array = self.array(n);
      Sample { value: ByteString::from_shared(array.clone()), model: Model::whole(&array), _arrays: vec![array] }
    } else {
      self.part_of_array()
    }
  }

  fn part_of_array(&mut self) -> Sample {
    let n = self.choose(2, 2.max(self.size));
    let array = self.array(n);
    let (from, until) = loop {
      let from = self.choose(0, n - 1);
      let until = self.choose(from + 1, n);
      if until - from < n {
        break (from, until);
      }
    };
    let value = if self.below(2) == 0 {
      ByteString::from_shared_range(array.clone(), from, until - from)
    } else {
      ByteString::from_shared(array.clone()).slice(from, until)
    };
    Sample { value, model: Model::whole(&array).slice(from, until), _arrays: vec![array] }
  }

  fn shaped(&mut self, shape: Shape) -> Sample {
    match shape {
      | Shape::Empty => {
        if self.below(2) == 0 {
          Sample::empty()
        } else {
          let n = self.choose(1, 1.max(self.size));
          let array = self.array(n);
          let at = self.choose(0, n);
          Sample {
            value: ByteString::from_shared(array.clone()).slice(at, at),
            model: Model::empty(),
            _arrays: vec![array],
          }
        }
      },
      | Shape::WholeArray => {
        let n = self.choose(1, 1.max(self.size));
        let array = self.array(n);
        Sample { value: ByteString::from_shared(array.clone()), model: Model::whole(&array), _arrays: vec![array] }
      },
      | Shape::PartOfArray => self.part_of_array(),
      | Shape::TwoFragments => {
        let first = self.fragment();
        first.concat(&self.fragment())
      },
      | Shape::ManyFragments => {
        let count = self.choose(3, 6);
        let mut result = Sample::empty();
        for _ in 0..count {
          let fragment = self.fragment();
          result = result.concat(&fragment);
        }
        result
      },
    }
  }

  // The `ByteString` that every property of the original draws.
  fn byte_string(&mut self) -> Sample {
    if self.below(2) == 0 {
      self.arbitrary_byte_string()
    } else {
      let shape = SHAPES[self.below(SHAPES.len())];
      self.shaped(shape)
    }
  }

  // Original: `arbitraryByteStringSlice` (L62-L71).
  fn byte_string_slice(&mut self) -> (Sample, usize, usize) {
    let xs = self.byte_string();
    let from = self.choose(0, xs.len().saturating_sub(1));
    let until = self.choose(from, from.max(xs.len()));
    (xs, from, until)
  }

  // Original: `arbitraryByteStringGrouped` (L73-L80).
  fn byte_string_grouped(&mut self) -> (Sample, usize) {
    let xs = self.byte_string();
    let size = self.choose(1, 1.max(xs.len()));
    (xs, size)
  }

  // A byte string of exactly `len` bytes in several fragments.
  fn byte_string_of_len(&mut self, len: usize) -> Sample {
    let mut result = Sample::empty();
    let mut left = len;
    while left > 0 {
      let n = self.choose(1, left);
      let extra_front = self.choose(0, 2);
      let extra_back = self.choose(0, 2);
      let array = self.array(extra_front + n + extra_back);
      let fragment = Sample {
        value: ByteString::from_shared(array.clone()).slice(extra_front, extra_front + n),
        model: Model::whole(&array).slice(extra_front, extra_front + n),
        _arrays: vec![array],
      };
      result = result.concat(&fragment);
      left -= n;
    }
    result
  }

  // Original: `arbitrary[String]`. The port also draws characters outside the
  // Basic Multilingual Plane, which take four bytes in UTF-8.
  fn string(&mut self) -> String {
    let len = self.choose(0, self.size);
    (0..len)
      .map(|_| loop {
        let candidate = match self.below(4) {
          | 0 => self.choose(0, 0x7F),
          | 1 => self.choose(0x80, 0x7FF),
          | 2 => self.choose(0x800, 0xFFFF),
          | _ => self.choose(0x1_0000, 0x10_FFFF),
        };
        if let Some(c) = char::from_u32(candidate as u32) {
          break c;
        }
      })
      .collect()
  }
}

fn seed_of(name: &str) -> u64 {
  name.bytes().fold(0xCBF2_9CE4_8422_2325, |hash, byte| (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01B3))
}

fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
  if let Some(message) = payload.downcast_ref::<String>() {
    message.clone()
  } else if let Some(message) = payload.downcast_ref::<&str>() {
    (*message).to_owned()
  } else {
    String::from("<panic without a message>")
  }
}

// Original: `check(property)` with `minSuccessful = 1000`.
fn check(name: &str, mut property: impl FnMut(&mut Gen)) {
  let seed = seed_of(name);
  for iteration in 0..MIN_SUCCESSFUL {
    let mut input = Gen::new(seed, iteration);
    let size = input.size;
    if let Err(payload) = panic::catch_unwind(AssertUnwindSafe(|| property(&mut input))) {
      panic!("{name}: failed at seed={seed:#018x} iteration={iteration} size={size}: {}", panic_message(&*payload));
    }
  }
}

// ---------------------------------------------------------------------------
// Views of a byte string

fn base_of(array: &Arc<[u8]>) -> usize {
  Arc::as_ptr(array).cast::<u8>() as usize
}

fn shared(bytes: &[u8]) -> Arc<[u8]> {
  Arc::from(bytes)
}

// Original: `ByteString(array)` and `ByteString(bb)`, a copy of the bytes.
fn copy_of(bytes: &[u8]) -> ByteString {
  ByteString::from_array(bytes, 0, bytes.len())
}

fn content(value: &ByteString) -> Vec<u8> {
  value.iterator().collect()
}

// The start address and the length of each non-empty slice.
fn non_empty_slices(value: &ByteString) -> Vec<(usize, usize)> {
  value.as_non_empty_byte_slices().map(|slice| (slice.as_ptr() as usize, slice.len())).collect()
}

fn slice_lengths(value: &ByteString) -> Vec<usize> {
  value.as_byte_slices().map(<[u8]>::len).collect()
}

// Original: `expectByteString2` (L108-L112).
fn expect_byte_string2(value: &ByteString) {
  let fragments = value.as_byte_slices().count();
  assert_eq!(fragments, 2, "Expected ByteString2, got {fragments} fragments");
}

// Original: `expectByteStrings` (L114-L118).
fn expect_byte_strings(value: &ByteString) {
  let fragments = value.as_byte_slices().count();
  assert!(fragments >= 3, "Expected ByteStrings, got {fragments} fragments");
}

// Original: `x eq y` on two byte strings. Both must expose the same slices of
// the same arrays.
fn assert_same_instance(actual: &ByteString, expected: &ByteString, what: &str) {
  assert_eq!(non_empty_slices(actual), non_empty_slices(expected), "{what}: not the same instance");
}

// Every observable part of `value` against its model.
fn assert_matches_model(value: &ByteString, model: &Model, what: &str) {
  assert_eq!(content(value), model.bytes, "{what}: bytes");
  assert_eq!(value.len(), model.len(), "{what}: len");
  assert_eq!(value.is_empty(), model.bytes.is_empty(), "{what}: is_empty");
  assert_eq!(non_empty_slices(value), model.addresses(), "{what}: shared slices");
  assert_eq!(value.is_compact(), model.is_compact(), "{what}: is_compact");
  let all: Vec<(usize, usize)> = value.as_byte_slices().map(|slice| (slice.as_ptr() as usize, slice.len())).collect();
  if model.bytes.is_empty() {
    assert_eq!(all.len(), 1, "{what}: as_byte_slices of an empty value");
    assert_eq!(all[0].1, 0, "{what}: as_byte_slices of an empty value");
  } else {
    assert_eq!(all, model.addresses(), "{what}: as_byte_slices");
  }
}

// The Base64 alphabet of RFC 4648 section 4, Table 1, by value ranges.
fn reference_base64_symbol(value: u8) -> u8 {
  match value {
    | 0..=25 => b'A' + value,
    | 26..=51 => b'a' + (value - 26),
    | 52..=61 => b'0' + (value - 52),
    | 62 => b'+',
    | 63 => b'/',
    | _ => unreachable!("{value} is not a 6-bit value"),
  }
}

// `java.util.Base64.getEncoder.encode`, written as RFC 4648 section 4 says
// it: the input is one stream of bits, most significant bit first; each 6
// bits become one symbol, the last symbol is filled up with zero bits, and
// `=` pads the output to a multiple of 4 symbols. The vectors in
// `reference_base64_matches_known_vectors` check this reference.
fn reference_base64(bytes: &[u8]) -> Vec<u8> {
  let bits: Vec<bool> = bytes.iter().flat_map(|&byte| (0..8).rev().map(move |shift| byte >> shift & 1 == 1)).collect();
  let mut out: Vec<u8> = bits
    .chunks(6)
    .map(|chunk| {
      let value = (0..6).fold(0_u8, |acc, index| acc << 1 | u8::from(chunk.get(index).copied().unwrap_or(false)));
      reference_base64_symbol(value)
    })
    .collect();
  while !out.len().is_multiple_of(4) {
    out.push(b'=');
  }
  out
}

// `new String(bytes, StandardCharsets.UTF_8)` of the JDK. It replaces as
// `String::from_utf8_lossy` does (one U+FFFD for each maximal subpart of an
// ill-formed sequence), except for `0xED` followed by `0xA0..=0xBF`, the start
// of an encoded surrogate: the JDK replaces `0xED`, that byte and a next
// continuation byte (`0x80..=0xBF`), if there is one, with a single U+FFFD.
// `0xED` never ends up inside another replaced sequence, so the parts between
// those sequences decode on their own.
fn reference_decode_utf8(bytes: &[u8]) -> String {
  let mut out = String::new();
  let mut start = 0;
  let mut index = 0;
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

fn reference_display(bytes: &[u8]) -> String {
  let shown: Vec<String> = bytes.iter().take(100).map(|&byte| (byte as i8).to_string()).collect();
  let mut text = format!("ByteString({})", shown.join(", "));
  if bytes.len() > 100 {
    text.push_str(&format!("... and [{}] more", bytes.len() - 100));
  }
  text
}

fn signed_sum(bytes: impl IntoIterator<Item = u8>) -> i32 {
  bytes.into_iter().fold(0, |acc, byte| acc + i32::from(byte as i8))
}

// ---------------------------------------------------------------------------
// "A ByteString" must (original L2140)

mod a_byte_string {
  use super::*;

  // "have correct size" when (L2141)
  mod have_correct_size {
    use super::*;

    // "concatenating" (L2142)
    #[test]
    fn concatenating() {
      check("have correct size when concatenating", |input| {
        let (a, b) = (input.byte_string(), input.byte_string());
        assert_eq!(a.value.concat(&b.value).len(), a.value.len() + b.value.len());
      });
    }

    // "dropping" (L2143)
    #[test]
    fn dropping() {
      check("have correct size when dropping", |input| {
        let (a, b) = (input.byte_string(), input.byte_string());
        assert_eq!(a.value.concat(&b.value).drop(b.value.len()).len(), a.value.len());
      });
    }

    // "taking" (L2144)
    #[test]
    fn taking() {
      check("have correct size when taking", |input| {
        let (a, b) = (input.byte_string(), input.byte_string());
        assert_eq!(a.value.concat(&b.value).take(a.value.len()), a.value);
      });
    }

    // "takingRight" (L2145)
    #[test]
    fn taking_right() {
      check("have correct size when takingRight", |input| {
        let (a, b) = (input.byte_string(), input.byte_string());
        assert_eq!(a.value.concat(&b.value).take_right(b.value.len()), b.value);
      });
    }

    // "dropping then taking" (L2146)
    #[test]
    fn dropping_then_taking() {
      check("have correct size when dropping then taking", |input| {
        let (a, b) = (input.byte_string(), input.byte_string());
        assert_eq!(b.value.concat(&a.value).concat(&b.value).drop(b.value.len()).take(a.value.len()), a.value);
      });
    }

    // "droppingRight" (L2149)
    #[test]
    fn dropping_right() {
      check("have correct size when droppingRight", |input| {
        let (a, b) = (input.byte_string(), input.byte_string());
        assert_eq!(b.value.concat(&a.value).concat(&b.value).drop(b.value.len()).drop_right(b.value.len()), a.value);
      });
    }
  }

  // "be sequential" when (L2152)
  mod be_sequential {
    use super::*;

    // "taking" (L2153)
    #[test]
    fn taking() {
      check("be sequential when taking", |input| {
        let (a, b) = (input.byte_string(), input.byte_string());
        assert_eq!(a.value.concat(&b.value).take(a.value.len()), a.value);
      });
    }

    // "dropping" (L2154)
    #[test]
    fn dropping() {
      check("be sequential when dropping", |input| {
        let (a, b) = (input.byte_string(), input.byte_string());
        assert_eq!(a.value.concat(&b.value).drop(a.value.len()), b.value);
      });
    }
  }

  // "preserve concatenation direction" when (L2157)
  mod preserve_concatenation_direction {
    use super::*;

    // "creating ByteString2 from two simple fragments" (L2158)
    #[test]
    fn creating_byte_string2_from_two_simple_fragments() {
      let compact = ByteString::from_shared(shared(&[1, 2]));
      let sliced = ByteString::from_shared_range(shared(&[0, 3, 4, 5]), 1, 2);

      let forward = compact.concat(&sliced);
      expect_byte_string2(&forward);
      assert_eq!(forward, copy_of(&[1, 2, 3, 4]));
      assert_eq!(slice_lengths(&forward), vec![2, 2]);

      let reverse = sliced.concat(&compact);
      expect_byte_string2(&reverse);
      assert_eq!(reverse, copy_of(&[3, 4, 1, 2]));
      assert_eq!(slice_lengths(&reverse), vec![2, 2]);
    }

    // "promoting ByteString2 to ByteStrings when prepending or appending another fragment" (L2171)
    #[test]
    fn promoting_byte_string2_to_byte_strings_when_prepending_or_appending_another_fragment() {
      let middle = ByteString::from_shared(shared(&[2, 3])).concat(&ByteString::from_shared(shared(&[4, 5])));
      expect_byte_string2(&middle);
      let prefix = ByteString::from_shared(shared(&[1]));
      let suffix = ByteString::from_shared(shared(&[6]));

      let prepended = prefix.concat(&middle);
      expect_byte_strings(&prepended);
      assert_eq!(prepended, copy_of(&[1, 2, 3, 4, 5]));
      // The fragments of both sides are kept as they are, as in Pekko's
      // `ByteStrings(this.toByteString1, bs)`.
      assert_eq!(slice_lengths(&prepended), vec![1, 2, 2]);

      let appended = middle.concat(&suffix);
      expect_byte_strings(&appended);
      assert_eq!(appended, copy_of(&[2, 3, 4, 5, 6]));
      assert_eq!(slice_lengths(&appended), vec![2, 2, 1]);

      let both = prepended.concat(&suffix);
      expect_byte_strings(&both);
      assert_eq!(both, copy_of(&[1, 2, 3, 4, 5, 6]));
      assert_eq!(slice_lengths(&both), vec![1, 2, 2, 1]);
    }
  }

  // "be equal to the original" when (L2186)
  mod be_equal_to_the_original {
    use super::*;

    // "compacting" (L2187)
    #[test]
    fn compacting() {
      check("be equal to the original when compacting", |input| {
        let xs = input.byte_string();
        let ys = xs.value.compact();
        assert_eq!(xs.value, ys);
        assert!(ys.is_compact());
      });
    }

    // "recombining" (L2192)
    #[test]
    fn recombining() {
      check("be equal to the original when recombining", |input| {
        let xs = input.byte_string();
        let from = input.index(xs.len());
        let until = input.index(xs.len());
        exercise_recombining(&xs.value, from, until);
      });
    }

    // Original: `excerciseRecombining` (L2199-L2203).
    fn exercise_recombining(xs: &ByteString, from: usize, until: usize) {
      let (tmp, c) = xs.split_at(until);
      let (a, b) = tmp.split_at(from);
      assert_eq!(a.concat(&b).concat(&c), *xs, "from={from} until={until}");
    }

    // Original: `ByteString1(bytes)`, a `ByteString1` over the whole array.
    fn byte_string1(bytes: &[u8]) -> ByteString {
      ByteString::from_shared_range(shared(bytes), 0, bytes.len())
    }

    // "recombining - edge cases" (L2204)
    //
    // The negative `Int`s of the original (`-2147483648`, `-324234`) act as
    // zero. Each large positive `Int` is passed as it is, and once more as
    // `usize::MAX`.
    #[test]
    fn recombining_edge_cases() {
      let large = |value: usize| [value, usize::MAX];

      for until in large(112_121_212) {
        exercise_recombining(&byte_string1(&[1]).concat(&byte_string1(&[2])), 0, until);
      }
      exercise_recombining(&byte_string1(&[100]), 0, 2);
      // -2147483648 acts as 0.
      exercise_recombining(&byte_string1(&[100]), 0, 2);
      exercise_recombining(&byte_string1(b"ab").concat(&byte_string1(b"cd")), 0, 1);
      for until in large(234_232) {
        exercise_recombining(&byte_string1(b"abc").drop(1).take(1), 0, until);
      }
      for until in large(2_147_483_647) {
        exercise_recombining(&ByteString::from_string("a"), 0, until);
      }
      for from in large(2_147_483_647) {
        exercise_recombining(&byte_string1(b"ab").concat(&byte_string1(b"cd")).drop(2), from, 1);
      }
      for from_and_until in large(2_147_483_647) {
        exercise_recombining(&byte_string1(b"ab").drop(1), from_and_until, from_and_until);
      }
    }
  }

  // "behave as expected" when (L2222)
  mod behave_as_expected {
    use super::*;

    // "created from and decoding to String" (L2223)
    #[test]
    fn created_from_and_decoding_to_string() {
      check("behave as expected when created from and decoding to String", |input| {
        let s = input.string();
        assert_eq!(ByteString::from_string(&s).decode_string(), s);
      });
    }

    // "taking its own length" (L2229)
    #[test]
    fn taking_its_own_length() {
      check("behave as expected when taking its own length", |input| {
        let b = input.byte_string();
        assert_same_instance(&b.value.take(b.value.len()), &b.value, "b.take(b.length) eq b");
      });
    }

    // "created from and decoding to Base64" (L2235)
    #[test]
    fn created_from_and_decoding_to_base64() {
      check("behave as expected when created from and decoding to Base64", |input| {
        let a = input.byte_string();
        let encoded = a.value.encode_base64();
        assert_eq!(encoded, copy_of(&reference_base64(a.bytes())));
        assert_eq!(encoded.decode_base64(), Some(a.value.clone()));
      });
    }

    // "created from iterator" (L2243)
    #[test]
    fn created_from_iterator() {
      check("behave as expected when created from iterator", |input| {
        let a = input.byte_string();
        let as_iterator = a.value.iterator();
        assert_eq!(ByteString::from_iterator(as_iterator), a.value);
      });
    }

    // "compacting" (L2250)
    #[test]
    fn compacting() {
      check("behave as expected when compacting", |input| {
        let a = input.byte_string();
        let was_compact = a.value.is_compact();
        let b = a.value.compact();
        if was_compact {
          assert_same_instance(&b, &a.value, "b eq a");
        }
        assert_eq!(b, a.value);
        assert!(b.is_compact());
        assert_same_instance(&b.compact(), &b, "b.compact eq b");
      });
    }

    // "asByteBuffers" (L2261)
    #[test]
    fn as_byte_buffers() {
      check("behave as expected when asByteBuffers (1)", |input| {
        let a = input.byte_string();
        if a.value.is_compact() {
          let slices: Vec<&[u8]> = a.value.as_byte_slices().collect();
          assert_eq!(slices.len(), 1);
          // Pekko's `asByteBuffer` of a compact value wraps all of its bytes.
          assert_eq!(slices[0], a.bytes());
        } else {
          assert!(a.value.as_byte_slices().count() > 0);
        }
      });
      check("behave as expected when asByteBuffers (2)", |input| {
        let a = input.byte_string();
        let joined = a.value.as_byte_slices().fold(ByteString::empty(), |bs, bb| bs.concat(&copy_of(bb)));
        assert_eq!(joined, a.value);
      });
      // `isReadOnly`: `as_byte_slices` yields `&[u8]`, shared borrows, which the
      // type keeps from being written.
      check("behave as expected when asByteBuffers (3)", |input| {
        let a = input.byte_string();
        let read_only: Vec<&[u8]> = a.value.as_byte_slices().collect();
        assert!(!read_only.is_empty());
      });
      check("behave as expected when asByteBuffers (4)", |input| {
        let a = input.byte_string();
        let as_byte_buffers: Vec<&[u8]> = a.value.as_byte_slices().collect();
        let get_byte_buffers: Vec<&[u8]> = a.value.as_byte_slices().collect();
        assert_eq!(as_byte_buffers, get_byte_buffers);
      });
    }

    // "asByteBuffersUnsafe" (L2280)
    #[test]
    fn as_byte_buffers_unsafe() {
      check("behave as expected when asByteBuffersUnsafe (1)", |input| {
        let a = input.byte_string();
        let joined = a.value.as_non_empty_byte_slices().fold(ByteString::empty(), |bs, bb| bs.concat(&copy_of(bb)));
        assert_eq!(joined, a.value);
      });
      check("behave as expected when asByteBuffersUnsafe (2)", |input| {
        let a = input.byte_string();
        // JDK's `hasArray()` is `hb != null && !isReadOnly` (ByteBuffer.java
        // L1505-L1507), so a slice that borrows the backing array itself checks both.
        assert_eq!(non_empty_slices(&a.value), a.model.addresses(), "hasArray && !isReadOnly");
        assert!(a.value.as_non_empty_byte_slices().all(|bb| !bb.is_empty()), "hasRemaining");
      });
      check("behave as expected when asByteBuffersUnsafe (3)", |input| {
        let a = input.byte_string();
        assert_eq!(
          a.value.as_non_empty_byte_slices().count(),
          a.value.as_byte_slices().filter(|bb| !bb.is_empty()).count()
        );
      });
    }

    // "asByteBuffersUnsafe wraps the backing arrays without copying" (L2294)
    #[test]
    fn as_byte_buffers_unsafe_wraps_the_backing_arrays_without_copying() {
      let array = shared(&[0, 1, 2, 3, 4, 5]);
      let base = base_of(&array);
      let compact = ByteString::from_shared(array.clone());
      assert_eq!(non_empty_slices(&compact), vec![(base, 6)]);

      let sliced = ByteString::from_shared_range(array.clone(), 1, 3);
      let sliced_buffer = sliced.as_non_empty_byte_slices().next().expect("one slice");
      // `theSameInstanceAs(array)` and `(position, limit) == (1, 4)`.
      assert_eq!(sliced_buffer.as_ptr() as usize, base + 1);
      assert_eq!(sliced_buffer.len(), 4 - 1);

      let other = shared(&[6, 7]);
      let fragmented = sliced.concat(&ByteString::from_shared(other.clone())).concat(&ByteString::from_shared_range(
        array.clone(),
        4,
        2,
      ));
      assert_eq!(non_empty_slices(&fragmented), vec![(base + 1, 3), (base_of(&other), 2), (base + 4, 2)]);
      assert_eq!(ByteString::empty().as_non_empty_byte_slices().count(), 0);
    }

    // "toString should start with ByteString(" (L2310)
    #[test]
    fn to_string_should_start_with_byte_string() {
      check("behave as expected when toString should start with ByteString(", |input| {
        let bs = input.byte_string();
        assert!(bs.value.to_string().starts_with("ByteString("));
      });
    }
  }

  // "behave like a Vector" when (L2316)
  //
  // Original: `likeVector(bs)(body)` (L183-L192) runs `body` on the byte string
  // and on `Vector(bs: _*)` and compares the results. The port runs it on the
  // byte string and on the generator's model of its bytes.
  mod behave_like_a_vector {
    use super::*;

    // "concatenating" (L2317)
    #[test]
    fn concatenating() {
      check("behave like a Vector when concatenating", |input| {
        let (a, b) = (input.byte_string(), input.byte_string());
        assert_eq!(content(&a.value.concat(&b.value)), [a.bytes(), b.bytes()].concat());
      });
    }

    // "calling apply" (L2323)
    #[test]
    fn calling_apply() {
      check("behave like a Vector when calling apply", |input| {
        let (xs, i1, i2) = input.byte_string_slice();
        let seq = xs.bytes();
        assert_eq!(
          (xs.value.get(i1), xs.value.get(i2)),
          (seq.get(i1).copied(), seq.get(i2).copied()),
          "i1={i1} i2={i2}"
        );
      });
    }

    // "calling head" (L2334)
    #[test]
    fn calling_head() {
      check("behave like a Vector when calling head", |input| {
        let a = input.byte_string();
        if !a.value.is_empty() {
          assert_eq!(a.value.head(), a.bytes().first().copied());
        }
      });
    }

    // "calling tail" (L2339)
    #[test]
    fn calling_tail() {
      check("behave like a Vector when calling tail", |input| {
        let a = input.byte_string();
        if !a.value.is_empty() {
          assert_eq!(content(&a.value.tail()), a.bytes()[1..]);
        }
      });
    }

    // "calling last" (L2344)
    #[test]
    fn calling_last() {
      check("behave like a Vector when calling last", |input| {
        let a = input.byte_string();
        if !a.value.is_empty() {
          assert_eq!(a.value.last(), a.bytes().last().copied());
        }
      });
    }

    // "calling init" (L2349)
    #[test]
    fn calling_init() {
      check("behave like a Vector when calling init", |input| {
        let a = input.byte_string();
        if !a.value.is_empty() {
          assert_eq!(content(&a.value.init()), a.bytes()[..a.len() - 1]);
        }
      });
    }

    // "calling length" (L2354)
    #[test]
    fn calling_length() {
      check("behave like a Vector when calling length", |input| {
        let a = input.byte_string();
        assert_eq!(a.value.len(), a.bytes().len());
      });
    }

    fn prefix_len(seq: &[u8], mut pred: impl FnMut(u8) -> bool) -> usize {
      seq.iter().take_while(|&&x| pred(x)).count()
    }

    // "calling span" (L2360)
    #[test]
    fn calling_span() {
      check("behave like a Vector when calling span", |input| {
        let (a, b) = (input.byte_string(), input.byte());
        let (left, right) = a.value.span(|x| x != b);
        let (seq_left, seq_right) = a.bytes().split_at(prefix_len(a.bytes(), |x| x != b));
        assert_eq!((content(&left), content(&right)), (seq_left.to_vec(), seq_right.to_vec()));
      });
    }

    // "calling takeWhile" (L2366)
    #[test]
    fn calling_take_while() {
      check("behave like a Vector when calling takeWhile", |input| {
        let (a, b) = (input.byte_string(), input.byte());
        assert_eq!(content(&a.value.take_while(|x| x != b)), a.bytes()[..prefix_len(a.bytes(), |x| x != b)]);
      });
    }

    // "calling dropWhile" (L2371)
    #[test]
    fn calling_drop_while() {
      check("behave like a Vector when calling dropWhile", |input| {
        let (a, b) = (input.byte_string(), input.byte());
        assert_eq!(content(&a.value.drop_while(|x| x != b)), a.bytes()[prefix_len(a.bytes(), |x| x != b)..]);
      });
    }

    fn index_from(seq: &[u8], from: usize, mut pred: impl FnMut(u8) -> bool) -> Option<usize> {
      seq.iter().enumerate().skip(from).find(|&(_, &x)| pred(x)).map(|(index, _)| index)
    }

    // "calling indexWhere" (L2376)
    #[test]
    fn calling_index_where() {
      check("behave like a Vector when calling indexWhere", |input| {
        let (a, b) = (input.byte_string(), input.byte());
        assert_eq!(a.value.index_where(|x| x == b), index_from(a.bytes(), 0, |x| x == b));
      });
    }

    // "calling indexWhere(p, idx)" (L2381)
    #[test]
    fn calling_index_where_p_idx() {
      check("behave like a Vector when calling indexWhere(p, idx)", |input| {
        let (a, b) = (input.byte_string(), input.byte());
        let idx = input.index(a.len());
        assert_eq!(a.value.index_where_from(|x| x == b, idx), index_from(a.bytes(), idx, |x| x == b), "idx={idx}");
      });
    }

    // "calling indexOf" (L2386)
    #[test]
    fn calling_index_of() {
      check("behave like a Vector when calling indexOf", |input| {
        let (a, b) = (input.byte_string(), input.byte());
        assert_eq!(a.value.index_of(b), index_from(a.bytes(), 0, |x| x == b));
      });
    }

    // "calling indexOf(elem, idx)" (L2393)
    #[test]
    fn calling_index_of_elem_idx() {
      check("behave like a Vector when calling indexOf(elem, idx)", |input| {
        let (a, b) = (input.byte_string(), input.byte());
        let idx = input.index(a.len());
        assert_eq!(a.value.index_of_from(b, idx), index_from(a.bytes(), idx, |x| x == b), "idx={idx}");
      });
    }

    // "calling foreach" (L2399)
    #[test]
    fn calling_foreach() {
      check("behave like a Vector when calling foreach", |input| {
        let a = input.byte_string();
        let mut acc = 0i32;
        a.value.iterator().for_each(|x| acc += i32::from(x as i8));
        let mut seq_acc = 0i32;
        a.bytes().iter().for_each(|&x| seq_acc += i32::from(x as i8));
        assert_eq!(acc, seq_acc);
      });
    }

    // "calling foldLeft" (L2406)
    #[test]
    fn calling_fold_left() {
      check("behave like a Vector when calling foldLeft", |input| {
        let a = input.byte_string();
        assert_eq!(
          a.value.iterator().fold(0i32, |acc, x| acc + i32::from(x as i8)),
          signed_sum(a.bytes().iter().copied())
        );
      });
    }

    // "calling toArray" (L2411)
    #[test]
    fn calling_to_array() {
      check("behave like a Vector when calling toArray", |input| {
        let a = input.byte_string();
        assert_eq!(a.value.to_vec(), a.bytes());
      });
    }

    // "calling slice" (L2417)
    #[test]
    fn calling_slice() {
      check("behave like a Vector when calling slice", |input| {
        let (xs, from, until) = input.byte_string_slice();
        assert_eq!(content(&xs.value.slice(from, until)), xs.bytes()[from..until], "from={from} until={until}");
      });
    }

    // "calling take and drop" (L2428)
    #[test]
    fn calling_take_and_drop() {
      check("behave like a Vector when calling take and drop", |input| {
        let (xs, from, until) = input.byte_string_slice();
        let seq = xs.bytes().iter().skip(from).take(until - from).copied().collect::<Vec<u8>>();
        assert_eq!(content(&xs.value.drop(from).take(until - from)), seq, "from={from} until={until}");
      });
    }

    // "calling grouped" (L2439)
    #[test]
    fn calling_grouped() {
      check("behave like a Vector when calling grouped", |input| {
        let (bs, size) = input.byte_string_grouped();
        let groups: Vec<Vec<u8>> = bs.value.grouped(size).map(|group| content(&group)).collect();
        let seq_groups: Vec<Vec<u8>> = bs.bytes().chunks(size).map(<[u8]>::to_vec).collect();
        assert_eq!(groups, seq_groups, "size={size}");
      });
    }

    // "calling copyToArray" (L2447)
    //
    // Scala's `copyToArray(xs, start, len)` copies
    // `min(len, size, xs.length - start)` elements. The count it returns is
    // checked too.
    #[test]
    fn calling_copy_to_array() {
      check("behave like a Vector when calling copyToArray", |input| {
        let (xs, from, until) = input.byte_string_slice();
        let mut array = vec![0u8; xs.len()];
        let copied = xs.value.copy_to_array(&mut array, from, until);
        let mut seq_array = vec![0u8; xs.len()];
        let seq_copied = until.min(xs.len()).min(xs.len().saturating_sub(from));
        seq_array[from..from + seq_copied].copy_from_slice(&xs.bytes()[..seq_copied]);
        assert_eq!(array, seq_array, "from={from} until={until}");
        assert_eq!(copied, seq_copied, "from={from} until={until}");
      });
    }
  }
}

// ---------------------------------------------------------------------------
// Properties of group 1 (design section 4.3). These are not cases of the
// original and are not counted in the ledger.

mod group1_properties {
  use super::*;

  // The generator draws each of the five shapes often enough.
  #[test]
  fn the_generator_draws_every_shape() {
    let mut counts = [0usize; 5];
    check("the generator draws every shape", |input| {
      let sample = input.byte_string();
      let shape = shape_of(&sample.model);
      counts[SHAPES.iter().position(|&known| known == shape).expect("a known shape")] += 1;
      assert_matches_model(&sample.value, &sample.model, "generated value");
    });
    for (shape, count) in SHAPES.iter().zip(counts) {
      assert!(count >= 50, "{shape:?} was drawn {count} times in {MIN_SUCCESSFUL}");
    }
  }

  // Properties 1 to 4: every cut gives the bytes of the same cut of a flat
  // array, shares the arrays without copying, holds no empty fragment, and
  // clamps every count and index, `usize::MAX` included, without a panic. The
  // value it is cut from does not change.
  #[test]
  fn cuts_match_the_flat_model_and_share_the_arrays() {
    check("cuts match the flat model and share the arrays", |input| {
      let a = input.byte_string();
      let before = non_empty_slices(&a.value);
      let (n, m) = (input.index(a.len()), input.index(a.len()));
      let v = &a.value;
      let model = &a.model;
      assert_matches_model(&v.take(n), &model.take(n), &format!("take({n})"));
      assert_matches_model(&v.drop(n), &model.drop(n), &format!("drop({n})"));
      assert_matches_model(&v.take_right(n), &model.take_right(n), &format!("take_right({n})"));
      assert_matches_model(&v.drop_right(n), &model.drop_right(n), &format!("drop_right({n})"));
      assert_matches_model(&v.slice(n, m), &model.slice(n, m), &format!("slice({n}, {m})"));
      assert_matches_model(&v.tail(), &model.drop(1), "tail");
      assert_matches_model(&v.init(), &model.drop_right(1), "init");
      let (left, right) = v.split_at(n);
      assert_matches_model(&left, &model.take(n), &format!("split_at({n}).0"));
      assert_matches_model(&right, &model.drop(n), &format!("split_at({n}).1"));
      assert_eq!(v.get(n), model.bytes.get(n).copied(), "get({n})");
      assert_eq!(v.head(), model.bytes.first().copied(), "head");
      assert_eq!(v.last(), model.bytes.last().copied(), "last");
      assert_matches_model(v, model, "the original value");
      assert_eq!(non_empty_slices(v), before, "the original value changed");
    });
  }

  // Property 3: a concatenation shares both sides, and joins two fragments
  // only when both sides are single fragments of adjacent ranges of one array.
  // Cutting a value of one fragment in two and joining the halves gives back
  // that fragment; a value of several fragments keeps the cut.
  #[test]
  fn concat_joins_only_adjacent_single_fragments() {
    check("concat joins only adjacent single fragments", |input| {
      let (a, b) = (input.byte_string(), input.byte_string());
      assert_matches_model(&a.value.concat(&b.value), &a.model.concat(&b.model), "a ++ b");
      let at = input.index(a.len());
      let (left, right) = a.value.split_at(at);
      let rejoined = left.concat(&right);
      assert_matches_model(&rejoined, &a.model.take(at).concat(&a.model.drop(at)), &format!("split_at({at}) rejoined"));
      if a.model.pieces.len() <= 1 {
        assert_eq!(non_empty_slices(&rejoined), non_empty_slices(&a.value), "split_at({at}) rejoined");
      }
    });
  }

  // Property 3: `compact` of a compact value shares it; of any other value it
  // copies the bytes into one new array.
  #[test]
  fn compact_copies_only_when_not_compact() {
    check("compact copies only when not compact", |input| {
      let a = input.byte_string();
      let compacted = a.value.compact();
      if a.model.is_compact() {
        assert_matches_model(&compacted, &a.model, "compact of a compact value");
      } else {
        assert_eq!(content(&compacted), a.model.bytes);
        assert!(compacted.is_compact());
        let slices = non_empty_slices(&compacted);
        assert_eq!(slices.len(), 1);
        let (address, len) = slices[0];
        assert_eq!(len, a.len());
        for piece in &a.model.pieces {
          let inside = address >= piece.base && address < piece.base + piece.array_len;
          assert!(!inside, "compact of a non-compact value did not copy");
        }
      }
      assert_eq!(non_empty_slices(&compacted.compact()), non_empty_slices(&compacted));
    });
  }

  // Property 4: `copy_to_array` copies `min(len, self.len(), dest.len() -
  // start)` bytes for every argument, and writes nothing else.
  #[test]
  fn copy_to_array_clamps_every_argument() {
    check("copy_to_array clamps every argument", |input| {
      let a = input.byte_string();
      let dest_len = input.choose(0, a.len() + 3);
      let start = input.index(dest_len);
      let len = input.index(a.len());
      let fill = input.byte();
      let mut dest = vec![fill; dest_len];
      let copied = a.value.copy_to_array(&mut dest, start, len);
      let expected_copied = len.min(a.len()).min(dest_len.saturating_sub(start));
      let mut expected = vec![fill; dest_len];
      if expected_copied > 0 {
        expected[start..start + expected_copied].copy_from_slice(&a.bytes()[..expected_copied]);
      }
      assert_eq!(copied, expected_copied, "dest_len={dest_len} start={start} len={len}");
      assert_eq!(dest, expected, "dest_len={dest_len} start={start} len={len}");
    });
  }

  // Properties 1, 3 and 4: `grouped` gives the chunks of the flat array, each
  // sharing the arrays, for every positive size, `usize::MAX` included.
  #[test]
  fn grouped_matches_chunks_and_shares_the_arrays() {
    check("grouped matches chunks and shares the arrays", |input| {
      let a = input.byte_string();
      let size = input.index(a.len()).max(1);
      let groups: Vec<ByteString> = a.value.grouped(size).collect();
      let expected_count = a.len().div_ceil(size);
      assert_eq!(groups.len(), expected_count, "size={size}");
      for (index, group) in groups.iter().enumerate() {
        let from = index * size;
        assert_matches_model(
          group,
          &a.model.slice(from, from.saturating_add(size)),
          &format!("group {index} of {size}"),
        );
      }
    });
  }

  // Property 1: `from_iterator` keeps the bytes in one compact array.
  #[test]
  fn from_iterator_builds_one_compact_array() {
    check("from_iterator builds one compact array", |input| {
      let a = input.byte_string();
      let built = ByteString::from_iterator(a.bytes().iter().copied());
      assert_eq!(content(&built), a.model.bytes);
      assert!(built.is_compact());
      assert_eq!(non_empty_slices(&built).len(), usize::from(!a.model.bytes.is_empty()));
    });
  }

  // Property 5: a string round-trips through UTF-8, and malformed UTF-8
  // decodes as the JDK decodes it.
  #[test]
  fn decode_string_round_trips_and_replaces_malformed_input() {
    check("decode_string round-trips and replaces malformed input", |input| {
      let s = input.string();
      let encoded = ByteString::from_string(&s);
      assert_eq!(content(&encoded), s.as_bytes());
      assert_eq!(encoded.decode_string(), s);
      let a = input.byte_string();
      assert_eq!(a.value.decode_string(), reference_decode_utf8(a.bytes()), "{:02X?}", a.bytes());
    });
  }

  // The reference encoder against outputs it does not compute: the RFC 4648
  // section 10 vectors, and the output of Python's `base64.b64encode` for the
  // other inputs.
  #[test]
  fn reference_base64_matches_known_vectors() {
    let ascending: Vec<u8> = (0..=255).collect();
    let descending: Vec<u8> = (0..=255).rev().collect();
    let vectors: [(&[u8], &str); 18] = [
      (b"", ""),
      (b"f", "Zg=="),
      (b"fo", "Zm8="),
      (b"foo", "Zm9v"),
      (b"foob", "Zm9vYg=="),
      (b"fooba", "Zm9vYmE="),
      (b"foobar", "Zm9vYmFy"),
      (&[0x00], "AA=="),
      (&[0x00, 0x00], "AAA="),
      (&[0x00, 0x00, 0x00], "AAAA"),
      (&[0xFF], "/w=="),
      (&[0xFF, 0xFF], "//8="),
      (&[0xFF, 0xFF, 0xFF], "////"),
      (&[0xFB, 0xFF], "+/8="),
      (&[0xFF, 0xEF], "/+8="),
      (&[0x14, 0xFB, 0x9C, 0x03, 0xD9, 0x7E], "FPucA9l+"),
      (
        &ascending,
        concat!(
          "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4vMDEyMzQ1Njc4OTo7PD0+P0BBQkNERUZH",
          "SElKS0xNTk9QUVJTVFVWV1hZWltcXV5fYGFiY2RlZmdoaWprbG1ub3BxcnN0dXZ3eHl6e3x9fn+AgYKDhIWGh4iJiouMjY6P",
          "kJGSk5SVlpeYmZqbnJ2en6ChoqOkpaanqKmqq6ytrq+wsbKztLW2t7i5uru8vb6/wMHCw8TFxsfIycrLzM3Oz9DR0tPU1dbX",
          "2Nna29zd3t/g4eLj5OXm5+jp6uvs7e7v8PHy8/T19vf4+fr7/P3+/w==",
        ),
      ),
      (
        &descending,
        concat!(
          "//79/Pv6+fj39vX08/Lx8O/u7ezr6uno5+bl5OPi4eDf3t3c29rZ2NfW1dTT0tHQz87NzMvKycjHxsXEw8LBwL++vby7urm4",
          "t7a1tLOysbCvrq2sq6qpqKempaSjoqGgn56dnJuamZiXlpWUk5KRkI+OjYyLiomIh4aFhIOCgYB/fn18e3p5eHd2dXRzcnFw",
          "b25tbGtqaWhnZmVkY2JhYF9eXVxbWllYV1ZVVFNSUVBPTk1MS0pJSEdGRURDQkFAPz49PDs6OTg3NjU0MzIxMC8uLSwrKiko",
          "JyYlJCMiISAfHh0cGxoZGBcWFRQTEhEQDw4NDAsKCQgHBgUEAwIBAA==",
        ),
      ),
    ];
    for (plain, encoded) in vectors {
      assert_eq!(String::from_utf8(reference_base64(plain)).unwrap(), encoded, "{plain:02X?}");
    }
  }

  // The reference decoder against what `new String(bytes, UTF_8)` returns on
  // the JDK for the same bytes.
  #[test]
  fn reference_decode_utf8_matches_known_jdk_outputs() {
    let vectors: [(&[u8], &str); 8] = [
      (&[0xED, 0xA0, 0x80], "\u{FFFD}"),
      (&[b'a', 0xED, 0xBF, 0xBF, b'b'], "a\u{FFFD}b"),
      (&[0xED, 0xA0], "\u{FFFD}"),
      (&[0xED, 0xA0, b'A'], "\u{FFFD}A"),
      (&[0xE1, 0x80, 0xED, 0xA0, 0x80, 0x80], "\u{FFFD}\u{FFFD}\u{FFFD}"),
      (&[0xE0, 0x80, 0x80], "\u{FFFD}\u{FFFD}\u{FFFD}"),
      (&[0xF0, 0x9F, 0x98, b'A'], "\u{FFFD}A"),
      (&[0xED, 0x9F, 0xBF, 0xF0, 0x9F, 0x98, 0x80], "\u{D7FF}\u{1F600}"),
    ];
    for (bytes, decoded) in vectors {
      assert_eq!(reference_decode_utf8(bytes), decoded, "{bytes:02X?}");
    }
  }

  // Property 5: Base64 matches the reference encoder, round-trips with and
  // without padding, gives compact values, and rejects any byte outside the
  // alphabet and anything after the padding.
  #[test]
  fn base64_matches_the_reference_and_round_trips() {
    check("base64 matches the reference and round-trips", |input| {
      let a = input.byte_string();
      let reference = reference_base64(a.bytes());
      let encoded = a.value.encode_base64();
      assert_eq!(content(&encoded), reference);
      assert!(encoded.is_compact());
      let decoded = encoded.decode_base64().expect("the encoding decodes");
      assert_eq!(content(&decoded), a.model.bytes);
      assert!(decoded.is_compact());

      let unpadded: Vec<u8> = reference.iter().copied().filter(|&byte| byte != b'=').collect();
      assert_eq!(copy_of(&unpadded).decode_base64().map(|value| content(&value)), Some(a.model.bytes.clone()));

      const OUTSIDE: [u8; 12] = [b' ', b'\t', b'\r', b'\n', b'-', b'_', 0, b'*', b'.', 0x80, 0xC3, 0xFF];
      let mut corrupted = reference.clone();
      let at = input.choose(0, corrupted.len());
      corrupted.insert(at, OUTSIDE[input.below(OUTSIDE.len())]);
      assert_eq!(copy_of(&corrupted).decode_base64(), None, "{corrupted:?}");

      if reference.last() == Some(&b'=') {
        let mut trailing = reference.clone();
        trailing.push(b'A');
        assert_eq!(copy_of(&trailing).decode_base64(), None, "{trailing:?}");
      }
    });
  }

  // Property 6: `Display` prints `ByteString(` and each byte as a signed
  // decimal, separated by `, `, and cuts after 100 bytes with
  // `... and [<rest>] more` (ByteString.scala L2683-L2689).
  #[test]
  fn display_matches_the_reference_format() {
    check("display matches the reference format", |input| {
      let len = input.choose(0, 250);
      let a = input.byte_string_of_len(len);
      assert_eq!(a.value.to_string(), reference_display(a.bytes()), "len={len}");
    });
  }
}
