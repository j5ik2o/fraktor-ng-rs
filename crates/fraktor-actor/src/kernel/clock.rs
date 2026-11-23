//! A port that reports monotonic time.

use core::time::Duration;

/// Reports monotonic elapsed time for the parts of supervision that need a
/// time window.
///
/// Mirrors the monotonic clock that Pekko reads through `System.nanoTime` when
/// a `SupervisorStrategy` counts the restarts of a child within a
/// `withinTimeRange`. The kernel cannot reach the platform clock, so it asks
/// this port instead. The value only has to be monotonic: a restart window
/// measures elapsed durations, not wall-clock instants, so the epoch the
/// implementation chooses does not matter. The adaptor supplies the system
/// clock, and a test may supply a clock it drives by hand.
pub trait Clock: Send + Sync {
  /// Returns the monotonic time since the clock's epoch.
  fn monotonic(&self) -> Duration;
}
