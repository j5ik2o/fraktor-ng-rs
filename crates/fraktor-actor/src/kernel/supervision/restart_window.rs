//! The period in which a supervisor counts restart requests.

use core::time::Duration;

/// Specifies how a supervisor measures its restart allowance.
#[derive(Clone, Copy, Debug)]
pub enum RestartWindow {
  /// Count all restart requests made over the child's lifetime.
  Unbounded,
  /// Start a new count once this duration has passed since the window opened.
  Within(Duration),
}

impl RestartWindow {
  /// Returns the duration of a finite window.
  pub(crate) fn duration(self) -> Option<Duration> {
    match self {
      | Self::Unbounded => None,
      | Self::Within(duration) => Some(duration),
    }
  }
}
