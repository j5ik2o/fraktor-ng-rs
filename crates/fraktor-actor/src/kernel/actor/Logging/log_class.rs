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

//! The log class that a marker logging entry carries.

/// The class of the source of a log event that a marker logging entry publishes.
///
/// Mirrors the `logClass` that `org.apache.pekko.event.MarkerLoggingAdapter`
/// takes. Pekko passes the runtime class of the source and keeps it as an `Any`;
/// the port carries the class as an `i32`, the same form the other log events
/// use, and keeps every `i32` it is given. The existing `Info`/`Error`
/// constructors still take the plain `i32`; this type only narrows the new
/// adapter entry and is converted back where those events are built.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LogClass(i32);

impl LogClass {
  /// Creates a log class for `value`.
  ///
  /// Every `i32` is a valid log class, so this keeps `value` unchanged.
  pub const fn new(value: i32) -> Self {
    Self(value)
  }

  /// Returns the `i32` that the log events carry.
  pub(crate) const fn value(&self) -> i32 {
    self.0
  }
}
