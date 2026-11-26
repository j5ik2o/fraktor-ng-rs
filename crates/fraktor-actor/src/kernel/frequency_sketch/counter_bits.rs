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

//! The validated size of a sketch counter.

use super::FrequencySketchError;

// The size of one counter in bits: 2, 4, 8, 16, 32 or 64. Counters are packed
// into 64-bit words, so the size divides 64.
#[derive(Clone, Copy)]
pub(super) struct CounterBits {
  bits: u32,
}

impl CounterBits {
  // The four-bit counters of the fixed-shape sketch.
  pub(super) const FOUR: Self = Self { bits: 4 };

  pub(super) const fn parse(counter_bits: i32) -> Result<Self, FrequencySketchError> {
    match counter_bits {
      | 2 | 4 | 8 | 16 | 32 | 64 => Ok(Self { bits: counter_bits as u32 }),
      | _ => Err(FrequencySketchError::UnsupportedCounterBits { counter_bits }),
    }
  }

  pub(super) const fn bits(self) -> u32 {
    self.bits
  }

  // The number of counters in one 64-bit word.
  pub(super) const fn per_word(self) -> u32 {
    u64::BITS / self.bits
  }

  // The largest value a counter holds. A 64-bit counter stops at `i64::MAX`, as
  // the original's `Long.MaxValue` mask does.
  pub(super) const fn max_value(self) -> u64 {
    if self.bits == u64::BITS {
      i64::MAX as u64
    } else {
      (1 << self.bits) - 1
    }
  }
}
