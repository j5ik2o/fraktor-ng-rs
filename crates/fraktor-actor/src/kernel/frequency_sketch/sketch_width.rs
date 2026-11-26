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

//! The validated number of counters per sketch row.

use super::{bits::Bits, FrequencySketchError};

// The number of counters per row: a positive power of two that fits in an
// `i32`, so a hash masked with `width - 1` picks a counter.
#[derive(Clone, Copy)]
pub(super) struct SketchWidth {
  width: i32,
}

impl SketchWidth {
  // The width multiplier times the capacity rounded up to a power of two,
  // computed in 64 bits as the original's `apply` does.
  pub(super) fn of(capacity: i32, width_multiplier: i32) -> Result<Self, FrequencySketchError> {
    let wide = i64::from(width_multiplier) * i64::from(Bits::ceiling_power_of_two(capacity));
    if wide > i64::from(i32::MAX) {
      return Err(FrequencySketchError::WidthOverflow { width: wide });
    }
    // The original's `toInt`, which keeps the low 32 bits.
    let width = wide as i32;
    if !Bits::is_power_of_two(width) {
      return Err(FrequencySketchError::WidthNotPowerOfTwo { width });
    }
    if width <= 0 {
      return Err(FrequencySketchError::WidthNotPositive { width });
    }
    Ok(Self { width })
  }

  pub(super) const fn get(self) -> i32 {
    self.width
  }

  // The mask that keeps a hash within the row.
  pub(super) const fn index_mask(self) -> i32 {
    self.width - 1
  }
}
