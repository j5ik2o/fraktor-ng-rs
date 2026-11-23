//! A clock backed by the standard library's monotonic instant.

use std::time::Instant;

use core::time::Duration;

use crate::kernel::Clock;

/// Reports monotonic time from the standard library's [`Instant`].
///
/// Mirrors the `System.nanoTime` clock that Pekko's restart window reads. The
/// epoch is the instant the clock is created, which is enough because a window
/// only compares elapsed durations.
pub struct SystemClock {
  epoch: Instant,
}

impl SystemClock {
  /// Creates a clock whose epoch is now.
  pub fn new() -> Self {
    Self { epoch: Instant::now() }
  }
}

impl Default for SystemClock {
  fn default() -> Self {
    Self::new()
  }
}

impl Clock for SystemClock {
  fn monotonic(&self) -> Duration {
    self.epoch.elapsed()
  }
}
