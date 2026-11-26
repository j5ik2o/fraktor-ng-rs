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

//! The severity of a log event.

use crate::kernel::event::EventClass;

/// The severity of a log event.
///
/// Mirrors the `ErrorLevel`, `WarningLevel`, `InfoLevel`, and `DebugLevel`
/// values of `org.apache.pekko.event.Logging.LogLevel` that the ported events
/// use. The order is the one Pekko gives them: a smaller value is more severe,
/// so `Error < Warning < Info < Debug`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
  /// An error event.
  Error,
  /// A warning event.
  Warning,
  /// An informational event.
  Info,
  /// A debug event.
  Debug,
}

impl LogLevel {
  /// Returns the channel that carries the events of this level.
  ///
  /// The stream subscribes a logger to one channel per level, and a logger
  /// that listens to this level receives the events published on this channel.
  pub fn event_class(&self) -> EventClass {
    match self {
      | Self::Error => EventClass::of::<super::Error>(),
      | Self::Warning => EventClass::of::<super::Warning>(),
      | Self::Info => EventClass::of::<super::Info>(),
      | Self::Debug => EventClass::of::<super::Debug>(),
    }
  }
}
