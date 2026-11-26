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

//! The number of increments after which a sketch halves its counters.

// The size at which the TinyLFU reset runs. A size the sketch cannot reach,
// such as zero or a negative value, disables the reset, as in the original.
#[derive(Clone, Copy)]
pub(super) struct ResetSize {
  size: i32,
}

impl ResetSize {
  // The original's `math.min(Int.MaxValue.toLong, (resetMultiplier * capacity).toLong).toInt`.
  // The float conversion saturates and maps NaN to zero, and the final
  // conversion keeps the low 32 bits, both as on the JVM.
  pub(super) fn of(capacity: i32, reset_multiplier: f64) -> Self {
    let wide = i64::from(i32::MAX).min((reset_multiplier * f64::from(capacity)) as i64);
    Self { size: wide as i32 }
  }

  pub(super) const fn is_reached_by(self, size: i32) -> bool {
    size == self.size
  }
}
