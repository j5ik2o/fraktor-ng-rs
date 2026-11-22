//! The error event.

use alloc::format;
use alloc::string::String;
use alloc::string::ToString;
use core::fmt;

use crate::kernel::actor::Logging::empty_mdc;
use crate::kernel::actor::Logging::log_event::LogEvent;
use crate::kernel::actor::Logging::log_level::LogLevel;
use crate::kernel::actor::Logging::LogMarker;
use crate::kernel::actor::Logging::Mdc;

/// An error event.
///
/// Mirrors `org.apache.pekko.event.Logging.Error`. Pekko keeps the cause as a
/// `Throwable`; the port keeps its name, because a filter compares the kind of
/// the cause and never the cause itself. An event without a cause carries
/// `Error.NoCause`, whose name [`Error::NO_CAUSE_NAME`] makes visible to a
/// filter, as Pekko's `Error.NoCause.getClass` does.
#[derive(Clone)]
pub struct Error {
  cause: Option<String>,
  cause_message: Option<String>,
  log_source: String,
  log_class: i32,
  message: String,
  marker: Option<LogMarker>,
  mdc: Mdc,
}

impl Error {
  /// The name of the null cause that an error event without a `Throwable`
  /// carries.
  ///
  /// Mirrors `org.apache.pekko.event.Logging.Error.NoCause`.
  pub const NO_CAUSE_NAME: &'static str = "NoCause";

  /// Creates an error event without a cause.
  ///
  /// Mirrors `Error(logSource, logClass, message)`, which supplies
  /// `Error.NoCause` as the cause.
  pub fn new(log_source: String, log_class: i32, message: &str) -> Self {
    Self {
      cause: None,
      cause_message: None,
      log_source,
      log_class,
      message: message.to_string(),
      marker: None,
      mdc: empty_mdc(),
    }
  }

  /// Creates an error event whose cause is `cause`.
  ///
  /// Mirrors `Error(cause, logSource, logClass, message)`; the name of the
  /// cause stands in for the `Throwable` itself.
  pub fn with_cause<C>(cause: C, log_source: String, log_class: i32, message: &str) -> Self
  where
    C: fmt::Debug,
  {
    Self {
      cause: Some(format!("{:?}", cause)),
      cause_message: None,
      log_source,
      log_class,
      message: message.to_string(),
      marker: None,
      mdc: empty_mdc(),
    }
  }

  /// Creates an error event whose cause is named `cause_name` exactly.
  ///
  /// Mirrors `Error(cause, logSource, logClass, message)` for a cause whose
  /// runtime class is already known by name, as the executor knows the class
  /// of the exception that Pekko would throw for a failing actor. Unlike
  /// [`Error::with_cause`], the name is not passed through `Debug`, so it is
  /// compared and reported as written.
  pub fn with_cause_name(cause_name: &str, log_source: String, log_class: i32, message: &str) -> Self {
    Self {
      cause: Some(cause_name.to_string()),
      cause_message: None,
      log_source,
      log_class,
      message: message.to_string(),
      marker: None,
      mdc: empty_mdc(),
    }
  }

  /// Creates an error event whose cause is named `cause_name` and whose cause
  /// message is `cause_message`.
  ///
  /// Mirrors `Error(cause, logSource, logClass, message)` for a caller that can
  /// read `cause.getMessage`: the cause name is what the filter compares, and
  /// `cause_message` is the cause's `Display` text. Both are passed already
  /// rendered, so this constructor does not take the cause itself.
  pub(crate) fn with_cause_and_message(
    cause_name: String,
    cause_message: String,
    log_source: String,
    log_class: i32,
    message: &str,
  ) -> Self {
    Self {
      cause: Some(cause_name),
      cause_message: Some(cause_message),
      log_source,
      log_class,
      message: message.to_string(),
      marker: None,
      mdc: empty_mdc(),
    }
  }

  /// Returns this event carrying `marker`.
  #[must_use]
  pub(crate) fn with_marker(mut self, marker: LogMarker) -> Self {
    self.marker = Some(marker);
    self
  }

  /// Returns the marker this event carries, if any.
  pub fn marker(&self) -> Option<LogMarker> {
    self.marker
  }

  /// Returns the cause's `Display` text, if the event was built with one.
  pub fn cause_message(&self) -> Option<&str> {
    self.cause_message.as_deref()
  }
}

impl LogEvent for Error {
  fn level(&self) -> LogLevel {
    LogLevel::Error
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
