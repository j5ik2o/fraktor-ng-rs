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
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.

//! Power-of-two helpers on 32-bit integers.

#[cfg(test)]
#[path = "bits_test.rs"]
mod tests;

/// Power-of-two arithmetic on `i32`, with the wrapping and shift semantics of
/// 32-bit two's complement integers whose shift counts are taken modulo 32.
///
/// Mirrors `FrequencySketch.Bits` in `org.apache.pekko.util`, which is
/// `private[pekko]` there and crate-private here.
pub(crate) struct Bits;

impl Bits {
  /// Returns the smallest power of two that is at least `value`.
  ///
  /// Values below one give one. A value above `2^30` gives `i32::MIN`, the
  /// bit pattern of `2^31`.
  #[must_use]
  pub(crate) const fn ceiling_power_of_two(value: i32) -> i32 {
    let leading_zeros = value.wrapping_sub(1).leading_zeros();
    1_i32.wrapping_shl(0_u32.wrapping_sub(leading_zeros))
  }

  /// Returns whether `value` has at most one bit set, so zero and `i32::MIN`
  /// count as powers of two.
  #[must_use]
  pub(crate) const fn is_power_of_two(value: i32) -> bool {
    value & value.wrapping_sub(1) == 0
  }

  /// Returns the exponent of the smallest power of two that is at least
  /// `value`.
  #[must_use]
  pub(crate) const fn power_of_two_exponent(value: i32) -> i32 {
    32 - value.wrapping_sub(1).leading_zeros() as i32
  }
}
