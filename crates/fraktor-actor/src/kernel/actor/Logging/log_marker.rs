//! The marker that a log event may carry.

/// A marker attached to a log event.
///
/// Mirrors `org.apache.pekko.event.LogMarker`, of which only `Security` exists
/// in this milestone.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LogMarker {
  /// The marker Pekko publishes as `LogMarker.Security`.
  Security,
}

impl LogMarker {
  /// Returns the name the marker reports.
  ///
  /// Mirrors `LogMarker.Security.name`, which is the `name` of the user-defined
  /// marker it stands for.
  pub const fn name(&self) -> &'static str {
    match *self {
      | Self::Security => "SECURITY",
    }
  }
}
