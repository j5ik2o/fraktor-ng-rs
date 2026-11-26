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

//! The debug event.

use alloc::string::String;
use alloc::string::ToString;

use crate::kernel::actor::Logging::empty_mdc;
use crate::kernel::actor::Logging::log_event::LogEvent;
use crate::kernel::actor::Logging::log_level::LogLevel;
use crate::kernel::actor::Logging::Mdc;

/// A debug event.
///
/// Mirrors `org.apache.pekko.event.Logging.Debug`, the event an event stream
/// publishes when the ported bus reports the life cycle of a subscriber. Pekko
/// keeps the class of the log source; the port keeps the integer that the
/// other events of this milestone use.
#[derive(Clone)]
pub struct Debug {
  log_source: String,
  log_class: i32,
  message: String,
  mdc: Mdc,
}

impl Debug {
  /// Creates a debug event without a cause.
  ///
  /// Mirrors `Debug(logSource, logClass, message = "")`.
  pub fn new(log_source: String, log_class: i32, message: &str) -> Self {
    Self { log_source, log_class, message: message.to_string(), mdc: empty_mdc() }
  }

  /// Returns this event carrying `mdc`.
  #[must_use]
  pub(crate) fn with_mdc(mut self, mdc: Mdc) -> Self {
    self.mdc = mdc;
    self
  }
}

impl LogEvent for Debug {
  fn level(&self) -> LogLevel {
    LogLevel::Debug
  }

  fn log_source(&self) -> &str {
    &self.log_source
  }

  fn log_class(&self) -> i32 {
    self.log_class
  }

  fn message(&self) -> &str {
    &self.message
  }

  fn mdc(&self) -> &Mdc {
    &self.mdc
  }

  fn cause_name(&self) -> Option<&str> {
    None
  }
}
