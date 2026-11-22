//! The informational event.

use alloc::string::String;
use alloc::string::ToString;

use crate::kernel::actor::Logging::empty_mdc;
use crate::kernel::actor::Logging::log_event::LogEvent;
use crate::kernel::actor::Logging::log_level::LogLevel;
use crate::kernel::actor::Logging::LogMarker;
use crate::kernel::actor::Logging::Mdc;

/// An informational event.
///
/// Mirrors `org.apache.pekko.event.Logging.Info`, the level a logger subscribes
/// to when it wants the informational events of the system as well. Pekko
/// keeps the message as an `Any`; the port keeps the rendered text, as the
/// other events of this milestone do.
#[derive(Clone)]
pub struct Info {
  log_source: String,
  log_class: i32,
  message: String,
  marker: Option<LogMarker>,
  mdc: Mdc,
}

impl Info {
  /// Creates an informational event without a cause.
  ///
  /// Mirrors `Info(logSource, logClass, message = "")`.
  pub fn new(log_source: String, log_class: i32, message: &str) -> Self {
    Self { log_source, log_class, message: message.to_string(), marker: None, mdc: empty_mdc() }
  }

  /// Returns this event carrying `marker`.
  ///
  /// Mirrors `Info3`, the event Pekko publishes for `info(marker, message)`.
  #[must_use]
  pub(crate) fn with_marker(mut self, marker: LogMarker) -> Self {
    self.marker = Some(marker);
    self
  }

  /// Returns the marker this event carries, if any.
  pub fn marker(&self) -> Option<LogMarker> {
    self.marker
  }
}

impl LogEvent for Info {
  fn level(&self) -> LogLevel {
    LogLevel::Info
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
