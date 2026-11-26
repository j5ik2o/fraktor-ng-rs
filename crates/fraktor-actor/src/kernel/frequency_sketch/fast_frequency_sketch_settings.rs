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

//! The settings of the fixed-shape frequency sketch.

/// The settings [`FastFrequencySketch::new`](super::FastFrequencySketch::new)
/// takes next to the capacity.
///
/// Mirrors the named arguments of `FastFrequencySketch.apply` in
/// `org.apache.pekko.util`, and [`Default`] holds the same defaults. The depth
/// is fixed at four rows and the counters at four bits.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FastFrequencySketchSettings {
  /// The multiplier on the capacity, rounded up to a power of two, that gives
  /// the number of counters per row.
  pub width_multiplier: i32,
  /// The multiplier on the capacity that gives the number of increments after
  /// which every counter is halved.
  pub reset_multiplier: f64,
}

impl Default for FastFrequencySketchSettings {
  fn default() -> Self {
    Self { width_multiplier: 4, reset_multiplier: 10.0 }
  }
}
