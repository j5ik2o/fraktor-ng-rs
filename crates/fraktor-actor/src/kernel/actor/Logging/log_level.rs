//! The severity of a log event.

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
