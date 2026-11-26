// SPDX-License-Identifier: Apache-2.0
//
// Licensed to the Apache Software Foundation (ASF) under one or more
// license agreements; and to You under the Apache License, version 2.0:
//
//   https://www.apache.org/licenses/LICENSE-2.0
//
// This file is part of the Apache Pekko project, which was derived from Akka.
//
// Copyright (C) 2019-2022 Lightbend Inc. <https://www.lightbend.com>
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.

// Ported from Pekko v2.0.0 (SHA 9fbd2283b1cf701c90a1223bf621942c191db61f):
//   actor-tests/src/test/scala/org/apache/pekko/util/ByteIteratorSpec.scala
// First ported from v1.7.1 (SHA 9a54d1294c3ac7821bbed3c940769c3d5027d1ff), which
// had the first case. v2.0.0 adds "match ByteArrayIterator semantics for
// getShort/Int/Long across both fast and cross-fragment paths" (upstream
// f481c59dd9, #2952). Its other changes to the file (246fae4fb8 #3300, which
// reaches `otherIndexOf` through a method handle instead of reflection,
// 9dfca01566 #3348 and 870d4499cc #2235, which reorder imports) leave the first
// case's assertions unchanged.
//
// The original's two cases under "A ByteIterator should" are kept as one test
// each, named after the case, with the original name as a comment. The notes
// below up to the first test are about the first case. Its eight
// assertions run in the original order, each on a fresh iterator over the same
// six bytes, because a search consumes the iterator (original L25-L27).
//
// The original builds the iterator with `ByteArrayIterator(Array(...))`; the
// port's `ByteIterator::from_array(&[...])` is the same iterator over a byte
// array. The original's `indexOf(elem)` is the port's `index_of(elem)`, and its
// `indexOf(elem, from)` is the port's `index_of_from(elem, from)`. Both return
// the index counted from where the iterator started, as the original's
// `indexWhere` does (ByteIterator.scala L487-L506). The original returns `-1`
// when nothing matches; the port returns `None`. Every search in this case
// finds a byte, so each expected value is `Some(index)`.
//
// The original has two `indexOf(elem, from)` overloads: the generic
// `indexOf[B >: Byte](elem: B, from: Int)` that a plain call resolves to, and
// `indexOf(elem: Byte, from: Int)`, which the original reaches only by
// reflection in `otherIndexOf` (L36-L40). Both delegate to
// `indexWhere(_ == elem, from)`. Rust has neither overloads nor reflection, so
// both are the port's single `index_of_from`. The port keeps the original's
// `otherIndexOf` helper as `other_index_of`, so the last three assertions still
// go through a helper as the original's do.
//
// The test calls the production `ByteIterator`. Until it exists the file does
// not compile: that compile error is the intended red for this node.

use fraktor_actor::kernel::byte_iterator::{ByteIterator, ByteOrder};
use fraktor_actor::kernel::byte_string::ByteString;

// The original's `freshIterator()`.
fn fresh_iterator() -> ByteIterator {
  ByteIterator::from_array(&[0x20, 0x20, 0x10, 0x20, 0x20, 0x10])
}

// The original's `otherIndexOf(iterator, byte, from)`, which calls the
// `indexOf(Byte, Int)` overload by reflection.
fn other_index_of(mut iterator: ByteIterator, byte: u8, from: usize) -> Option<usize> {
  iterator.index_of_from(byte, from)
}

// "A ByteIterator should correctly implement indexOf"
#[test]
fn a_byte_iterator_should_correctly_implement_index_of() {
  assert_eq!(fresh_iterator().index_of(0x20), Some(0));
  assert_eq!(fresh_iterator().index_of(0x10), Some(2));

  assert_eq!(fresh_iterator().index_of_from(0x20, 1), Some(1));
  assert_eq!(fresh_iterator().index_of_from(0x10, 1), Some(2));
  assert_eq!(fresh_iterator().index_of_from(0x10, 3), Some(5));

  assert_eq!(other_index_of(fresh_iterator(), 0x20, 1), Some(1));
  assert_eq!(other_index_of(fresh_iterator(), 0x10, 1), Some(2));
  assert_eq!(other_index_of(fresh_iterator(), 0x10, 3), Some(5));
}

// Notes on the second case.
//
// The original reads with an implicit `java.nio.ByteOrder`; the port passes a
// `ByteOrder` to `get_short`, `get_int` and `get_long`, which return `i16`,
// `i32` and `i64` as Scala's `Short`, `Int` and `Long` are signed. The original
// throws `NoSuchElementException` when too few bytes are left; the port returns
// `None`. Every read in this case has the bytes it needs, so each expected value
// is `Some`.
//
// The original's `iterator.drop(off)` is the helper `drop` below, which reads
// and discards `off` bytes and hands the same iterator back. `left ++ right` is
// `concat`, whose iterator walks both arrays as the original's
// `MultiByteArrayIterator` does. `withClue` becomes the assertion message.

// The original's `iterator.drop(n)`.
fn drop(mut iterator: ByteIterator, count: usize) -> ByteIterator {
  for _ in 0..count {
    iterator.next();
  }
  iterator
}

// "A ByteIterator should match ByteArrayIterator semantics for getShort/Int/Long across both fast and cross-fragment paths"
#[test]
fn a_byte_iterator_should_match_byte_array_iterator_semantics_for_get_short_int_long_across_both_fast_and_cross_fragment_paths(
) {
  // 16 bytes is large enough to fit a long (8B) entirely inside one fragment for the fast
  // path AND to span every possible split for the cross-fragment path.
  let bytes: Vec<u8> = (0..16_u32).map(|i| ((i * 31 + 7) & 0xFF) as u8).collect();

  let reference = |off: usize, byte_order: ByteOrder| -> (Option<i16>, Option<i32>, Option<i64>) {
    let it = drop(ByteIterator::from_array(&bytes), off);
    let s = it.clone().get_short(byte_order);
    let i = it.clone().get_int(byte_order);
    let l = it.clone().get_long(byte_order);
    (s, i, l)
  };

  // Build a multi-fragment ByteString for every split point and read at every offset.
  for split in 1..bytes.len() {
    let left = ByteString::from_array(&bytes, 0, split);
    let right = ByteString::from_array(&bytes, split, bytes.len() - split);
    let combined = left.concat(&right);

    for byte_order in [ByteOrder::BigEndian, ByteOrder::LittleEndian] {
      for off in 0..=bytes.len() - 8 {
        let (ref_s, ref_i, ref_l) = reference(off, byte_order);
        assert!(ref_s.is_some() && ref_i.is_some() && ref_l.is_some());

        assert_eq!(
          drop(combined.iterator(), off).get_short(byte_order),
          ref_s,
          "split={split}, off={off}, byteOrder={byte_order:?}, getShort: "
        );
        assert_eq!(
          drop(combined.iterator(), off).get_int(byte_order),
          ref_i,
          "split={split}, off={off}, byteOrder={byte_order:?}, getInt: "
        );
        assert_eq!(
          drop(combined.iterator(), off).get_long(byte_order),
          ref_l,
          "split={split}, off={off}, byteOrder={byte_order:?}, getLong: "
        );
      }
    }
  }
}
