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

//! The validated number of sketch rows.

use super::FrequencySketchError;

// The number of rows, one hash function each. Zero rows is allowed, as in the
// original: such a sketch counts nothing.
#[derive(Clone, Copy)]
pub(super) struct SketchDepth {
  rows: usize,
}

impl SketchDepth {
  // The four rows of the fixed-shape sketch.
  pub(super) const FOUR: Self = Self { rows: 4 };

  pub(super) fn parse(depth: i32) -> Result<Self, FrequencySketchError> {
    usize::try_from(depth).map(|rows| Self { rows }).map_err(|_| FrequencySketchError::NegativeDepth { depth })
  }

  pub(super) const fn rows(self) -> usize {
    self.rows
  }
}
