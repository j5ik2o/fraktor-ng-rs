//! The adapter that publishes a marked log event.

use alloc::format;
use alloc::string::String;
use core::fmt;

use crate::kernel::actor::Logging::{Error, Info, LogMarker};
use crate::kernel::event::EventStream;

/// Publishes marked log events on an event stream.
///
/// Mirrors `org.apache.pekko.event.MarkerLoggingAdapter`: it is built from an
/// event stream, a log source and a log class, and its `info` and `error`
/// publish the corresponding events on that stream.
#[derive(Clone)]
pub struct LogMarkerAdapter {
  event_stream: EventStream,
  log_source: String,
  log_class: i32,
}

impl LogMarkerAdapter {
  /// Creates an adapter that publishes on `event_stream`.
  pub fn new(event_stream: EventStream, log_source: &str, log_class: i32) -> Self {
    Self { event_stream, log_source: log_source.to_string(), log_class }
  }

  /// Publishes an informational event that carries `marker`.
  pub fn info(&self, marker: LogMarker, message: &str) {
    let event = Info::new(self.log_source.clone(), self.log_class, message).with_marker(marker);
    self.event_stream.publish(event);
  }

  /// Publishes an error event that carries `marker`, `message`, and the cause.
  ///
  /// The cause keeps its `Debug` name, which a filter compares, and its
  /// `Display` text, which stands in for Pekko's `cause.getMessage`.
  pub fn error<C>(&self, marker: LogMarker, cause: C, message: &str)
  where
    C: fmt::Debug + fmt::Display,
  {
    let event = Error::with_cause_and_message(
      format!("{:?}", cause),
      format!("{}", cause),
      self.log_source.clone(),
      self.log_class,
      message,
    )
    .with_marker(marker);
    self.event_stream.publish(event);
  }
}
