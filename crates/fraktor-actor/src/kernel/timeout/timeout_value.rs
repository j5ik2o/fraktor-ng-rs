//! A wrapper around a duration that makes the meaning of the duration explicit.

use core::time::Duration;

#[cfg(test)]
#[path = "timeout_value_test.rs"]
mod tests;

/// A duration that denotes how long a caller is willing to wait.
///
/// Mirrors `org.apache.pekko.util.Timeout`: it wraps a duration so that a bare
/// duration cannot be passed where a timeout is expected, and it compares by
/// value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Timeout {
  duration: Duration,
}

impl Timeout {
  /// Creates a timeout from the given duration.
  pub fn new(duration: Duration) -> Self {
    Self { duration }
  }

  /// Returns the wrapped duration.
  pub fn duration(&self) -> Duration {
    self.duration
  }
}
