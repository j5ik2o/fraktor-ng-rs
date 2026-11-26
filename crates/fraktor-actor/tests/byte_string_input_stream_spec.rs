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
// file changes only by 30a0046944 #2300 (also on 1.x); the cases and their inputs are unchanged.
// The record is docs/baseline/pekko-v2.0.0-migration/batch-3.md.
//
// Ported from Pekko v1.7.1 (SHA 9a54d1294c3ac7821bbed3c940769c3d5027d1ff):
//   actor-tests/src/test/scala/org/apache/pekko/util/ByteStringInputStreamSpec.scala
//
// The original's three cases ("ByteString1 must", "ByteString1C must" and
// "ByteStrings must", each "support asInputStream") are kept as one test each,
// named after the case, with the original name as a comment. Every assertion
// runs in the original order, and each one opens a new stream, as the original
// does.
//
// The original's three representations are one type in the port, `ByteString`:
//
// - `ByteString1.empty` is `ByteString::empty()`.
// - `ByteString1.fromString(s)` and `ByteString1C.fromString(s)` are
//   `ByteString::from_string(s)`, which encodes `s` as UTF-8 as both originals do.
// - `ByteString.fromArray(bytes, 1, 3)` is `ByteString::from_array(bytes, 1, 3)`,
//   which copies `length` bytes from `offset` as the original does
//   (ByteString.scala L100 and L1234-L1243). `"abcdef".getBytes(US_ASCII)` is
//   `b"abcdef"`.
// - `ByteStrings(a, b)` is `a.concat(&b)`, which keeps the bytes of `a` before
//   the bytes of `b`.
//
// The original's `asInputStream` is the port's `as_input_stream()`, which
// returns a `ByteStringInputStream`. Java's `InputStream.read()` is the port's
// `read()`, and `read(byte[])` is `read_into(&mut [u8])`. Java reports the end
// of the stream from both as `-1`; the port reports it as `None`.
//
// The original's `toUtf8String`, `toByteArray` and `copy` helpers (L57-L80) are
// ported as helpers with the same meaning: read with a 4096-byte buffer until
// the end of the stream, collect what was read, and decode it as UTF-8. The
// original's `copy` returns the number of bytes copied, which no caller uses; the
// port's `copy` returns it as well.
//
// The test calls the production `ByteString`. Until it exists the file does not
// compile: that compile error is the intended red for this node.

use fraktor_actor::kernel::byte_string::{ByteString, ByteStringInputStream};

// The original's `toUtf8String(input)`.
fn to_utf8_string(input: ByteStringInputStream) -> String {
  String::from_utf8(to_byte_array(input)).expect("the stream holds UTF-8")
}

// The original's `toByteArray(input)`.
fn to_byte_array(mut input: ByteStringInputStream) -> Vec<u8> {
  let mut output = Vec::new();
  copy(&mut input, &mut output);
  output
}

// The original's `copy(input, output)`.
fn copy(input: &mut ByteStringInputStream, output: &mut Vec<u8>) -> usize {
  let mut buffer = [0_u8; 4096];
  let mut count = 0;
  let mut n = input.read_into(&mut buffer);
  while let Some(read) = n {
    output.extend_from_slice(&buffer[..read]);
    count += read;
    n = input.read_into(&mut buffer);
  }
  count
}

// "ByteString1 must support asInputStream"
#[test]
fn byte_string1_must_support_as_input_stream() {
  assert_eq!(ByteString::empty().as_input_stream().read(), None);
  assert_eq!(ByteString::empty().as_input_stream().read_into(&mut [1]), None);
  assert_eq!(to_utf8_string(ByteString::empty().as_input_stream()), "");
  assert_eq!(to_utf8_string(ByteString::from_string("abc").as_input_stream()), "abc");
}

// "ByteString1C must support asInputStream"
#[test]
fn byte_string1c_must_support_as_input_stream() {
  assert_eq!(to_utf8_string(ByteString::from_string("").as_input_stream()), "");
  assert_eq!(to_utf8_string(ByteString::from_string("abc").as_input_stream()), "abc");
  let bytes = b"abcdef";
  assert_eq!(to_utf8_string(ByteString::from_array(bytes, 1, 3).as_input_stream()), "bcd");
}

// "ByteStrings must support asInputStream"
#[test]
fn byte_strings_must_support_as_input_stream() {
  let empty = ByteString::from_string("").concat(&ByteString::from_string(""));
  assert_eq!(empty.as_input_stream().read(), None);
  assert_eq!(empty.as_input_stream().read_into(&mut [1]), None);
  assert_eq!(to_utf8_string(empty.as_input_stream()), "");
  let abc = ByteString::from_string("a").concat(&ByteString::from_string("bc"));
  assert_eq!(to_utf8_string(abc.as_input_stream()), "abc");
}
