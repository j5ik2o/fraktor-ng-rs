//! The predicate that decides whether a log event is expected.

use std::string::String;
use std::string::ToString;

use fraktor_actor::kernel::actor::Logging::Error;
use fraktor_actor::kernel::actor::Logging::LogEvent;
use fraktor_actor::kernel::actor::Logging::LogLevel;

/// Decides whether a log event is one that a test expects.
///
/// Mirrors `org.apache.pekko.testkit.EventFilter`. Pekko builds a filter from
/// the kind of the event, an optional message, and the number of occurrences
/// the test expects, and its `intercept` mutes the matching events while the
/// test body runs. The port keeps the parts the ported tests use: an error
/// filter narrowed to a cause and, optionally, to an exact message.
#[derive(Clone, Debug)]
pub enum EventFilter {
  /// An error filter narrowed to a cause and, optionally, to a message.
  Error {
    /// The name of the class of the cause that the event must carry.
    cause_name: String,
    /// The exact message the event must carry, when the filter names one.
    message: Option<String>,
  },
  /// A warning filter.
  Warning,
}

impl EventFilter {
  /// Creates the filter for error events, whatever cause they carry.
  ///
  /// Mirrors `EventFilter.error()`, which builds an `ErrorFilter` for
  /// `Logging.Error.NoCause`'s class.
  pub fn error() -> Self {
    Self::Error { cause_name: Error::NO_CAUSE_NAME.to_string(), message: None }
  }

  /// Creates the filter for warning events.
  ///
  /// Mirrors `EventFilter.warning()`.
  pub fn warning() -> Self {
    Self::Warning
  }

  /// Creates the error filter narrowed to the cause whose class name is
  /// `cause_name`.
  ///
  /// Mirrors `EventFilter[A]()`, which takes the runtime class of the cause
  /// type. The port takes the name that the cause reports.
  pub fn typed(cause_name: &str) -> Self {
    Self::Error { cause_name: cause_name.to_string(), message: None }
  }
}

impl EventFilter {
  /// Returns whether `event` is the kind of event this filter expects.
  ///
  /// Mirrors `EventFilter.apply` with its `matches`. Pekko's `ErrorFilter`
  /// matches an error event when the filter names `Error.NoCause`'s class or
  /// when the event's cause is an instance of that class, and, when the filter
  /// names a message, when the message equals it exactly; the port compares the
  /// cause names and the messages as strings. Pekko's `WarningFilter` matches a
  /// warning event.
  pub fn apply(&self, event: &dyn LogEvent) -> bool {
    match *self {
      | Self::Error { ref cause_name, ref message } => {
        event.level() == LogLevel::Error
          && (cause_name == Error::NO_CAUSE_NAME || event.cause_name() == Some(cause_name.as_str()))
          && message.as_ref().is_none_or(|expected| event.message() == expected.as_str())
      },
      | Self::Warning => event.level() == LogLevel::Warning,
    }
  }
}

impl EventFilter {
  /// Returns this filter narrowed to the events whose message is exactly
  /// `message`.
  ///
  /// Mirrors `EventFilter.withMessage(message)`, which a test uses to mute only
  /// the expected failure. Pekko compares the message as a string, and so does
  /// the port.
  pub fn with_message(mut self, message: &str) -> Self {
    if let Self::Error { message: ref mut slot, .. } = self {
      *slot = Some(message.to_string());
    }
    self
  }
}
