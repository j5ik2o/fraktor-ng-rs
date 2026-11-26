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
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.

//! The number-of-instances value of a deployment section.

use alloc::string::String;

/// The `nr-of-instances` value of a section.
///
/// The original reads the value with `getInt`, a signed 32-bit read:
/// negative values build a pool, and a value out of range is a
/// `ConfigException.WrongType`. [`NotANumber`](Self::NotANumber) holds
/// a value that cannot become an `i32` — a non-numeric string such as
/// `"boom"` and an out-of-range number alike — for the interpretation
/// step to refuse with the same error (design of #195, F1).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum NumberOfInstances {
  /// A value that fits the original's `getInt`.
  Value(i32),
  /// A value that does not fit, kept as written.
  NotANumber(String),
}
