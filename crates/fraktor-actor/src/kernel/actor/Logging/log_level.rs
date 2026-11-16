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
