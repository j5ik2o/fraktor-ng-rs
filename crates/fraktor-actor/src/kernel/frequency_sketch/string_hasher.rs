// SPDX-License-Identifier: Apache-2.0
//
// Licensed to the Apache Software Foundation (ASF) under one or more
// license agreements; and to You under the Apache License, version 2.0:
//
//   https://www.apache.org/licenses/LICENSE-2.0
//
// This file is part of the Apache Pekko project, which was derived from Akka.
//
// Copyright (C) 2021-2022 Lightbend Inc. <https://www.lightbend.com>
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

//! The seeded MurmurHash3 string hash of the configurable sketch.

use super::{hasher_seal::HasherSeal, murmur3::Murmur3, FrequencySketchHasher};

#[cfg(test)]
#[path = "string_hasher_test.rs"]
mod tests;

/// Hashes a string with the seeded 32-bit MurmurHash3 over its UTF-16 code
/// units, two units per block.
///
/// Mirrors `FrequencySketch.Hasher.StringHasher` in `org.apache.pekko.util`,
/// which calls `scala.util.hashing.MurmurHash3.stringHash`. Strings hash to
/// the same values as on the JVM. [`Default`] uses the original's default
/// seed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StringHasher {
  seed: i32,
}

impl StringHasher {
  /// The seed of the original's implicit string hasher.
  pub const DEFAULT_SEED: i32 = 135_283_237;

  /// Creates a hasher with the given seed.
  #[must_use]
  pub const fn new(seed: i32) -> Self {
    Self { seed }
  }
}

impl Default for StringHasher {
  fn default() -> Self {
    Self::new(Self::DEFAULT_SEED)
  }
}

impl HasherSeal<str> for StringHasher {}

impl FrequencySketchHasher<str> for StringHasher {
  fn hash(&self, value: &str) -> i32 {
    let mut state = Murmur3::new(self.seed);
    let mut pending: Option<u16> = None;
    let mut length: i32 = 0;
    for unit in value.encode_utf16() {
      length = length.wrapping_add(1);
      pending = match pending {
        | Some(high) => {
          state = state.mix((i32::from(high) << 16).wrapping_add(i32::from(unit)));
          None
        },
        | None => Some(unit),
      };
    }
    if let Some(last) = pending {
      state = state.mix_last(i32::from(last));
    }
    state.finalize(length)
  }
}
