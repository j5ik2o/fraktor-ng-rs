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
