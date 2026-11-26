// SPDX-License-Identifier: Apache-2.0
//
// Licensed to the Apache Software Foundation (ASF) under one or more
// contributor license agreements. See the NOTICE file distributed with
// this work for additional information regarding copyright ownership.
// The ASF licenses this file to You under the Apache License, Version 2.0
// (the "License"); you may not use this file except in compliance with
// the License. You may obtain a copy of the License at
//
//    http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.

// Ported from Pekko v2.0.0 (SHA 9fbd2283b1cf701c90a1223bf621942c191db61f):
//   actor-tests/src/test/scala/org/apache/pekko/util/ByteStringBuilderScala213PlusSpec.scala
// First ported from v1.7.1 (SHA 9a54d1294c3ac7821bbed3c940769c3d5027d1ff), where
// the file was `actor-tests/src/test/scala-2.13+/...` and had the first three
// cases. v2.0.0 moves it to `scala/` when it drops Scala 2.12 (upstream
// f2b677dfa0, #1986) and adds the two "handle addAll with ...ArraySeq.ofByte
// without aliasing the backing array" cases (upstream fef71efd85, #3539).
//
// The original's five `addAll` cases are kept as one test each, named after
// the case, with the original name as a comment. Each test builds the result in
// one chain and compares it with `shouldEqual` (`assert_eq!` in the port).
//
// In Scala 2.13 `++=` is an alias of `addAll`, so the Scala 2.13+ spec performs
// the same operation as ByteStringBuilderSpec through `addAll`. In the port both
// call `add_all`. The original API and its Rust counterpart:
//
// - `ByteString.newBuilder` is `ByteString::new_builder()`.
// - `addAll(xs: IterableOnce[Byte])` and `++=(xs: IterableOnce[Byte])` are the
//   same operation, `add_all<I: IntoIterator<Item = u8>>`.
// - `addAll(bytes: ByteString)` (and `++=(bytes: ByteString)`) is a dedicated
//   `ByteString` overload, so it corresponds to `append(&ByteString)`, not to
//   `add_all`.
// - `result()` is `result()`.
// - `ByteString("a")` and `ByteString.fromString(s)` are
//   `ByteString::from_string(s)`.
// - `List[Byte]('a')` (a LinearSeq) is `std::collections::LinkedList::from([b'a'])`.
// - `Vector[Byte]('a')` (an IndexedSeq) is `vec![b'a']`.
// - `shouldEqual` is `assert_eq!`, which needs `ByteString: PartialEq + Debug`.
//
// The test calls the production `ByteString` and `ByteStringBuilder`. Until they
// exist the file does not compile: that compile error is the intended red for
// this node.

use std::collections::LinkedList;

use fraktor_actor::kernel::byte_string::ByteString;

// "handle addAll with LinearSeq"
#[test]
fn handle_add_all_with_linear_seq() {
  let result: ByteString = ByteString::new_builder().add_all(LinkedList::from(*b"a")).result();
  assert_eq!(result, ByteString::from_string("a"));
}

// "handle addAll with IndexedSeq"
#[test]
fn handle_add_all_with_indexed_seq() {
  let result: ByteString = ByteString::new_builder().add_all(vec![b'a']).result();
  assert_eq!(result, ByteString::from_string("a"));
}

// "handle addAll with ByteString"
#[test]
fn handle_add_all_with_byte_string() {
  let result: ByteString = ByteString::new_builder().append(&ByteString::from_string("a")).result();
  assert_eq!(result, ByteString::from_string("a"));
}

// The two ArraySeq cases wrap a byte array without copying it, add it, then
// overwrite the array's first byte, and expect the built byte string to keep
// the bytes as they were when they were added. `ArraySeq.unsafeWrapArray(array)`
// (an immutable view) is a shared borrow of the array, `array.iter().copied()`;
// `mutable.ArraySeq.make(array)` (a mutable view) is a mutable borrow,
// `array.iter_mut().map(|byte| *byte)`. Rust's borrow rules end either borrow
// before the array is written again, so the builder cannot alias the array
// through safe code; the cases still check that the result has the bytes as
// added. `Array.emptyByteArray` is an empty array, and `ByteString.empty` is
// `ByteString::empty()`.

// "handle addAll with immutable.ArraySeq.ofByte without aliasing the backing array"
#[test]
fn handle_add_all_with_immutable_array_seq_of_byte_without_aliasing_the_backing_array() {
  let mut array = *b"abc";
  let mut builder = ByteString::new_builder();
  builder.add_all(array.iter().copied());
  array[0] = b'z';
  assert_eq!(array, *b"zbc");
  assert_eq!(builder.result(), ByteString::from_string("abc"));
  let empty: [u8; 0] = [];
  assert_eq!(ByteString::new_builder().add_all(empty.iter().copied()).result(), ByteString::empty());
}

// "handle addAll with mutable.ArraySeq.ofByte without aliasing the backing array"
#[test]
fn handle_add_all_with_mutable_array_seq_of_byte_without_aliasing_the_backing_array() {
  let mut array = *b"abc";
  let mut builder = ByteString::new_builder();
  builder.add_all(array.iter_mut().map(|byte| *byte));
  array[0] = b'z';
  assert_eq!(array, *b"zbc");
  assert_eq!(builder.result(), ByteString::from_string("abc"));
  let mut empty: [u8; 0] = [];
  assert_eq!(ByteString::new_builder().add_all(empty.iter_mut().map(|byte| *byte)).result(), ByteString::empty());
}
