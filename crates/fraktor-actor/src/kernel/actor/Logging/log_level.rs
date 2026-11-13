//! The severity of a log event.

/// The severity of a log event.
///
/// Mirrors the `ErrorLevel` and `WarningLevel` values of
/// `org.apache.pekko.event.Logging.LogLevel` that the ported events use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogLevel {
  /// An error event.
  Error,
  /// A warning event.
  Warning,
}
