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
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.

//! The polynomial string hash of the fixed-shape sketch.

use super::{hasher_seal::HasherSeal, FrequencySketchHasher};

#[cfg(test)]
#[path = "string_hash_code_test.rs"]
mod tests;

/// Hashes a string as `s[0]*31^(n-1) + ... + s[n-1]` over its UTF-16 code
/// units, wrapping in 32 bits.
///
/// `FastFrequencySketch` in `org.apache.pekko.util` hashes values with their
/// `hashCode`, which for a string is this hash, so strings land in the same
/// counters as on the JVM.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StringHashCode;

impl HasherSeal<str> for StringHashCode {}

impl FrequencySketchHasher<str> for StringHashCode {
  fn hash(&self, value: &str) -> i32 {
    value.encode_utf16().fold(0_i32, |hash, unit| hash.wrapping_mul(31).wrapping_add(i32::from(unit)))
  }
}
