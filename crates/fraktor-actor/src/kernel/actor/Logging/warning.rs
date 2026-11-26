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

//! The warning event.

use alloc::format;
use alloc::string::String;
use alloc::string::ToString;
use core::fmt;

use crate::kernel::actor::Logging::empty_mdc;
use crate::kernel::actor::Logging::log_event::LogEvent;
use crate::kernel::actor::Logging::log_level::LogLevel;
use crate::kernel::actor::Logging::Mdc;

/// A warning event.
///
/// Mirrors `org.apache.pekko.event.Logging.Warning` together with `Warning4`,
/// the subclass that carries a cause. Pekko keeps the cause as a `Throwable`;
/// the port keeps its name, as [`Error`](super::Error) does.
#[derive(Clone)]
pub struct Warning {
  cause: Option<String>,
  log_source: String,
  log_class: i32,
  message: String,
  mdc: Mdc,
}

impl Warning {
  /// Creates a warning event without a cause.
  ///
  /// Mirrors `Warning(logSource, logClass, message)`.
  pub fn new(log_source: String, log_class: i32, message: &str) -> Self {
    Self { cause: None, log_source, log_class, message: message.to_string(), mdc: empty_mdc() }
  }

  /// Creates a warning event whose cause is `cause` and whose diagnostic
  /// context is `mdc`.
  ///
  /// Mirrors `Warning4(logSource, logClass, message, mdc, marker, cause)`, which
  /// Pekko also exposes as `Warning(logSource, logClass, message, mdc)` when the
  /// cause is the last argument a test passes.
  pub fn with_cause<C>(cause: C, log_source: String, log_class: i32, message: &str, mdc: Mdc) -> Self
  where
    C: fmt::Debug,
  {
    Self { cause: Some(format!("{:?}", cause)), log_source, log_class, message: message.to_string(), mdc }
  }

  /// Returns this event carrying `mdc`.
  #[must_use]
  pub(crate) fn with_mdc(mut self, mdc: Mdc) -> Self {
    self.mdc = mdc;
    self
  }
}

impl LogEvent for Warning {
  fn level(&self) -> LogLevel {
    LogLevel::Warning
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
    self.cause.as_deref()
  }
}
