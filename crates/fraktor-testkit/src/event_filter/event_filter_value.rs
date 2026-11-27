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

//! The predicate that decides whether a log event is expected.

use std::string::String;
use std::string::ToString;

use fraktor_actor::kernel::actor::Logging::Error;
use fraktor_actor::kernel::actor::Logging::LogEvent;
use fraktor_actor::kernel::actor::Logging::LogLevel;

/// How a filter's text narrows the events it matches.
///
/// Mirrors the one text a Pekko `EventFilter` carries: a `message` is
/// compared whole, a `start` as a prefix, and the port's `contains` as
/// a part of the message, which stands in for the original's regular
/// expression `pattern` for the texts the ported tests name.
#[derive(Clone, Debug)]
enum EventFilterText {
  /// No text restriction.
  Any,
  /// The whole text must equal this.
  Exact(String),
  /// The text must start with this.
  Start(String),
  /// The text must contain this.
  Contains(String),
}

/// Decides whether a log event is one that a test expects.
///
/// Mirrors `org.apache.pekko.testkit.EventFilter`. Pekko builds a filter from
/// the kind of the event, an optional source, an optional text, and the
/// number of occurrences the test expects, and its `intercept` mutes the
/// matching events while the test body runs. The port keeps those parts:
/// the level, for an error the cause, the source compared whole, one
/// text restriction, and the expected count, which defaults to one.
#[derive(Clone, Debug)]
pub struct EventFilter {
  level: LogLevel,
  cause_name: Option<String>,
  source: Option<String>,
  text: EventFilterText,
  occurrences: usize,
}

impl EventFilter {
  /// Creates the filter for error events, whatever cause they carry.
  ///
  /// Mirrors `EventFilter.error()`, which builds an `ErrorFilter` for
  /// `Logging.Error.NoCause`'s class.
  pub fn error() -> Self {
    Self {
      level: LogLevel::Error,
      cause_name: Some(Error::NO_CAUSE_NAME.to_string()),
      source: None,
      text: EventFilterText::Any,
      occurrences: 1,
    }
  }

  /// Creates the filter for warning events.
  ///
  /// Mirrors `EventFilter.warning()`.
  pub fn warning() -> Self {
    Self { level: LogLevel::Warning, cause_name: None, source: None, text: EventFilterText::Any, occurrences: 1 }
  }

  /// Creates the filter for info events.
  ///
  /// Mirrors `EventFilter.info()`.
  pub fn info() -> Self {
    Self { level: LogLevel::Info, cause_name: None, source: None, text: EventFilterText::Any, occurrences: 1 }
  }

  /// Creates the error filter narrowed to the cause whose class name is
  /// `cause_name`.
  ///
  /// Mirrors `EventFilter[A]()`, which takes the runtime class of the cause
  /// type. The port takes the name that the cause reports.
  pub fn typed(cause_name: &str) -> Self {
    Self {
      level: LogLevel::Error,
      cause_name: Some(cause_name.to_string()),
      source: None,
      text: EventFilterText::Any,
      occurrences: 1,
    }
  }

  /// Returns how many matching events this filter expects.
  pub fn occurrences(&self) -> usize {
    self.occurrences
  }
}

impl EventFilter {
  /// Returns whether `event` is the kind of event this filter expects.
  ///
  /// Mirrors `EventFilter.apply` with its `matches`. The level must
  /// agree; a named source must equal the event's source whole; an
  /// error's cause name must agree, unless the filter names
  /// `Error.NoCause`, which any cause satisfies. The text is compared
  /// against the event's message and, for an error, also against its
  /// cause's message, as Pekko's `ErrorFilter` matches either.
  pub fn apply(&self, event: &dyn LogEvent) -> bool {
    if event.level() != self.level {
      return false;
    }
    if let Some(source) = &self.source {
      if event.log_source() != source.as_str() {
        return false;
      }
    }
    if self.level == LogLevel::Error {
      if let Some(cause_name) = &self.cause_name {
        if cause_name != Error::NO_CAUSE_NAME && event.cause_name() != Some(cause_name.as_str()) {
          return false;
        }
      }
    }
    match &self.text {
      | EventFilterText::Any => true,
      | EventFilterText::Exact(expected) => {
        event.message() == expected.as_str()
          || (self.level == LogLevel::Error && event.cause_message() == Some(expected.as_str()))
      },
      | EventFilterText::Start(prefix) => {
        event.message().starts_with(prefix.as_str())
          || (self.level == LogLevel::Error
            && event.cause_message().is_some_and(|cause| cause.starts_with(prefix.as_str())))
      },
      | EventFilterText::Contains(fragment) => {
        event.message().contains(fragment.as_str())
          || (self.level == LogLevel::Error
            && event.cause_message().is_some_and(|cause| cause.contains(fragment.as_str())))
      },
    }
  }
}

impl EventFilter {
  /// Returns this filter narrowed to the events whose message is exactly
  /// `message`.
  ///
  /// Mirrors `EventFilter.withMessage(message)`, which a test uses to mute only
  /// the expected failure. Pekko compares the message as a string, and so does
  /// the port, at every level. A filter that names only the beginning
  /// of a line uses [`start`](Self::start) instead, as the design of
  /// the FSM timing port records for its existing warning call sites.
  pub fn with_message(mut self, message: &str) -> Self {
    self.text = EventFilterText::Exact(message.to_string());
    self
  }

  /// Returns this filter narrowed to the events whose message starts with
  /// `prefix`.
  ///
  /// Mirrors the `start` argument of Pekko's event filters: the expected
  /// text of a log line is long, and the ported tests name its
  /// beginning, which carries the type, the parties, and the count.
  pub fn start(mut self, prefix: &str) -> Self {
    self.text = EventFilterText::Start(prefix.to_string());
    self
  }

  /// Returns this filter narrowed to the events whose source is
  /// `source`.
  ///
  /// Mirrors the `source` argument of Pekko's event filters, which
  /// `doMatch` compares with the event's source as a whole string.
  pub fn with_source(mut self, source: &str) -> Self {
    self.source = Some(source.to_string());
    self
  }

  /// Returns this filter narrowed to the events whose message contains
  /// `fragment`.
  ///
  /// The port's stand-in for Pekko's regular-expression `pattern`,
  /// whose `findFirstIn` is a part match: for a text without regular
  /// expression characters the two agree. A filter that needs a real
  /// pattern belongs to a later port.
  pub fn contains(mut self, fragment: &str) -> Self {
    self.text = EventFilterText::Contains(fragment.to_string());
    self
  }

  /// Returns this filter expecting `occurrences` matching events.
  ///
  /// Mirrors the `occurrences` argument of Pekko's event filters. The
  /// port's default is one, which the existing `intercept` call sites
  /// already rely on.
  pub fn with_occurrences(mut self, occurrences: usize) -> Self {
    self.occurrences = occurrences;
    self
  }
}
