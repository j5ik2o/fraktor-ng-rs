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
// Copyright 2015 Ben Manes. All Rights Reserved.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// Scala (https://www.scala-lang.org)
//
// Copyright EPFL and Lightbend, Inc.
//
// Licensed under Apache License 2.0
// (http://www.apache.org/licenses/LICENSE-2.0).
//
// See the NOTICE file distributed with this work for
// additional information regarding copyright ownership.
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.

use super::StringHasher;
use crate::kernel::frequency_sketch::FrequencySketchHasher;

// The expected values are `MurmurHash3.stringHash(value, 135283237)` on Scala 2.13.18.
#[test]
fn hashes_strings_as_the_original() {
  let hasher = StringHasher::default();
  assert_eq!(hasher.hash("foo"), 537_187_922);
  assert_eq!(hasher.hash("bar"), -185_253_344);
  assert_eq!(hasher.hash("baz"), -478_924_459);
  assert_eq!(hasher.hash(""), -1_699_863_197);
  assert_eq!(hasher.hash("a"), 329_321_386);
}

#[test]
fn a_different_seed_gives_a_different_hash() {
  assert_ne!(StringHasher::new(1).hash("foo"), StringHasher::default().hash("foo"));
}
