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

// Checked against Pekko v2.0.0 (SHA 9fbd2283b1cf701c90a1223bf621942c191db61f): the upstream test
// file changes only by ec27d6dd8c #2724, 00b0a99278 #2081; the cases and their inputs are
// unchanged.
// The record is docs/baseline/pekko-v2.0.0-migration/batch-3.md.
//
// Ported from Pekko v1.7.1 (SHA 9a54d1294c3ac7821bbed3c940769c3d5027d1ff):
//   actor-tests/src/test/java/org/apache/pekko/util/ByteStringTest.java
//
// The original's two `@Test` cases are kept as one test each, named after the
// case, with the original name as a comment.
//
// The original API and its Rust counterpart:
//
// - `ByteString.fromString(s)` is `ByteString::from_string(s)`.
// - `ByteString.fromInts(1, 2, 3)` is `ByteString::from_ints(&[1, 2, 3])`, whose
//   argument is a `&[i32]`.
// - `ByteString.createBuilder()` is `ByteString::create_builder()`.
// - Java's `append(bs)` on the builder is `append(&bytes)`, the dedicated
//   `ByteString` overload.
// - `result()` is `result()`.
// - `assertEquals` is `assert_eq!`, which needs `ByteString: PartialEq + Debug`.
//
// `testCreation` only creates the two byte strings and binds them, as the
// original does, so it has no assertion.
//
// The test calls the production `ByteString` and `ByteStringBuilder`. Until they
// exist the file does not compile: that compile error is the intended red for
// this node.

use fraktor_actor::kernel::byte_string::ByteString;

// "testCreation"
#[test]
fn test_creation() {
  let empty: ByteString = ByteString::from_string("");
  let ints: ByteString = ByteString::from_ints(&[1, 2, 3]);
  let _ = (&empty, &ints);
}

// "testBuilderCreation"
#[test]
fn test_builder_creation() {
  let mut builder = ByteString::create_builder();
  builder.append(&ByteString::from_string("Hello"));
  builder.append(&ByteString::from_string(" "));
  builder.append(&ByteString::from_string("World"));
  assert_eq!(ByteString::from_string("Hello World"), builder.result());
}
