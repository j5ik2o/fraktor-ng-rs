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

//! The facts every log event carries.

use crate::kernel::actor::Logging::log_level::LogLevel;
use crate::kernel::actor::Logging::Mdc;

/// The facts that every log event carries.
///
/// Mirrors `org.apache.pekko.event.Logging.LogEvent`. Pekko adds the thread and
/// the wall-clock time of the creation; the port leaves them out because no
/// ported test reads them and the kernel does not touch the clock. The kind of
/// a cause is reported as its name, which is what
/// `org.apache.pekko.event.Logging.LogEventWithCause` lets a filter compare.
pub trait LogEvent {
  /// Returns the severity of this event.
  fn level(&self) -> LogLevel;

  /// Returns the source of this event.
  fn log_source(&self) -> &str;

  /// Returns the class of the source of this event.
  fn log_class(&self) -> i32;

  /// Returns the message of this event.
  fn message(&self) -> &str;

  /// Returns the extra values added to the diagnostic context.
  fn mdc(&self) -> &Mdc;

  /// Returns the name of the cause, or `None` when the event has no cause.
  fn cause_name(&self) -> Option<&str>;

  /// Returns the message of the cause, or `None` when the event carries
  /// none.
  ///
  /// Mirrors `LogEventWithCause.cause.getMessage`, which Pekko's
  /// `ErrorFilter` matches a filter's text against as well as the
  /// event's own message. The default is `None`, so an event kind that
  /// carries no cause message answers as it did before.
  fn cause_message(&self) -> Option<&str> {
    None
  }
}
