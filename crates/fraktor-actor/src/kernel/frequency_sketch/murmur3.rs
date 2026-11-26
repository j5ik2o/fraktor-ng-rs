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

//! The 32-bit MurmurHash3 steps the configurable sketch hashes with.

// A MurmurHash3 hash in progress, with the steps of `scala.util.hashing.MurmurHash3`.
#[derive(Clone, Copy)]
pub(super) struct Murmur3 {
  hash: i32,
}

impl Murmur3 {
  pub(super) const fn new(seed: i32) -> Self {
    Self { hash: seed }
  }

  // `MurmurHash3.mix`.
  pub(super) const fn mix(self, data: i32) -> Self {
    let hash = self.mix_last(data).hash.rotate_left(13);
    Self { hash: hash.wrapping_mul(5).wrapping_add(0xE654_6B64_u32 as i32) }
  }

  // `MurmurHash3.mixLast`.
  pub(super) const fn mix_last(self, data: i32) -> Self {
    let mut k = data.wrapping_mul(0xCC9E_2D51_u32 as i32);
    k = k.rotate_left(15);
    k = k.wrapping_mul(0x1B87_3593);
    Self { hash: self.hash ^ k }
  }

  // `MurmurHash3.finalizeHash`, which ends with the avalanche step.
  pub(super) const fn finalize(self, length: i32) -> i32 {
    let mut hash = self.hash ^ length;
    hash ^= ((hash as u32) >> 16) as i32;
    hash = hash.wrapping_mul(0x85EB_CA6B_u32 as i32);
    hash ^= ((hash as u32) >> 13) as i32;
    hash = hash.wrapping_mul(0xC2B2_AE35_u32 as i32);
    hash ^ ((hash as u32) >> 16) as i32
  }
}
