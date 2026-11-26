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

// Checked against Pekko v2.0.0 (SHA 9fbd2283b1cf701c90a1223bf621942c191db61f): the upstream test
// file changes only by 8391a2212e #3490 (also on 1.x); the cases and their inputs are unchanged.
// The record is docs/baseline/pekko-v2.0.0-migration/batch-3.md.
//
// Ported from Pekko v1.7.1 (SHA 9a54d1294c3ac7821bbed3c940769c3d5027d1ff):
//   actor-tests/src/test/scala/org/apache/pekko/io/dns/internal/DomainNameSpec.scala
//
// The original's six cases under `"DomainName.parse" should` are kept as one
// test each, named after the case, with the original name as a comment. The
// helpers `label` and `pointer` build the same bytes as the original's: a label
// is its length byte followed by its US-ASCII bytes, and a compression pointer
// is `0xC0 | (offset >> 8)` followed by `offset & 0xFF`.
//
// The original calls `DomainName.parse(bytes.iterator, bytes)`, where the
// iterator is the read position and `bytes` is the whole message that pointers
// refer to. The port's `WireReader` holds both: `WireReader::new(&bytes)` reads
// from offset 0 like `bytes.iterator`, and `WireReader::starting_at(&bytes, 13)`
// reads from offset 13 like `bytes.iterator.drop(13)`.
//
// The original's `parse` returns the name as a `String` and fails with an
// exception. The port's `DomainName::parse` returns
// `Result<DomainName, WireFormatError>`, and each case asserts the variant that
// stands for the original's exception:
//
// - `an[IllegalArgumentException]` for the pointer to itself and the pointer
//   cycle is `WireFormatError::TooManyCompressionPointers`, the original's
//   "more than 16 compression pointers, probable pointer loop".
// - `an[IllegalArgumentException]` for the reserved label type `0x40` is
//   `WireFormatError::ReservedLabelType(1)`, the original's "unsupported label
//   type [1]" (`(0x40 & 0xC0) >> 6`).
// - `a[NoSuchElementException]` for the truncated name is
//   `WireFormatError::Truncated`, the original's read past the end of the
//   message.
//
// The port's types live at `fraktor_actor::kernel::io::dns`. Until they exist
// the file does not compile: that compile error is the intended red for this
// node.

use fraktor_actor::kernel::io::dns::{DomainName, WireFormatError, WireReader};

fn label(s: &str) -> Vec<u8> {
  let mut bytes = vec![u8::try_from(s.len()).unwrap()];
  bytes.extend_from_slice(s.as_bytes());
  bytes
}

fn pointer(offset: usize) -> Vec<u8> {
  vec![u8::try_from(0xC0 | (offset >> 8)).unwrap(), u8::try_from(offset & 0xFF).unwrap()]
}

// "parse a plain name"
#[test]
fn parse_a_plain_name() {
  let bytes = [label("www"), label("example"), label("com"), vec![0]].concat();
  let name = DomainName::parse(&mut WireReader::new(&bytes)).unwrap();
  assert_eq!(name.as_str(), "www.example.com");
}

// "follow a backwards compression pointer"
#[test]
fn follow_a_backwards_compression_pointer() {
  // offset 0: example.com   offset 13: www + pointer to offset 0
  let bytes = [label("example"), label("com"), vec![0], label("www"), pointer(0)].concat();
  let name = DomainName::parse(&mut WireReader::starting_at(&bytes, 13)).unwrap();
  assert_eq!(name.as_str(), "www.example.com");
}

// "reject a pointer that points at itself"
#[test]
fn reject_a_pointer_that_points_at_itself() {
  // offset 0: www, offset 4: pointer to offset 4
  let bytes = [label("www"), pointer(4)].concat();
  assert_eq!(DomainName::parse(&mut WireReader::new(&bytes)), Err(WireFormatError::TooManyCompressionPointers));
}

// "reject a pointer cycle"
#[test]
fn reject_a_pointer_cycle() {
  // offset 0: a + pointer to offset 4, offset 4: b + pointer to offset 0
  let bytes = [label("a"), pointer(4), label("b"), pointer(0)].concat();
  assert_eq!(DomainName::parse(&mut WireReader::new(&bytes)), Err(WireFormatError::TooManyCompressionPointers));
}

// "reject reserved label types"
#[test]
fn reject_reserved_label_types() {
  let bytes = [0x40, b'a', 0];
  assert_eq!(DomainName::parse(&mut WireReader::new(&bytes)), Err(WireFormatError::ReservedLabelType(1)));
}

// "fail rather than loop on a truncated name"
#[test]
fn fail_rather_than_loop_on_a_truncated_name() {
  let bytes = label("www");
  assert_eq!(DomainName::parse(&mut WireReader::new(&bytes)), Err(WireFormatError::Truncated));
}
