//! The restart history of one child, used by a time-windowed strategy.

use crate::kernel::supervision::RestartLimit;
use core::time::Duration;

#[cfg(test)]
#[path = "restart_history_test.rs"]
mod tests;

/// Counts a child's restart requests and remembers the start of the time
/// window they fall in.
///
/// Mirrors `org.apache.pekko.actor.ChildRestartStats` for the parts a
/// time-windowed strategy reads: the number of restarts already granted and
/// the monotonic instant the current window opened at. The history belongs to
/// one child for its whole life, so a restart keeps the same history and a
/// window that has expired starts over, as Pekko's does. The owner supplies
/// the monotonic time so the kernel never reads a platform clock.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct RestartHistory {
  count: u32,
  window_start: Option<Duration>,
}

impl RestartHistory {
  /// Creates an empty history, as a freshly started child has.
  pub(crate) fn new() -> Self {
    Self { count: 0, window_start: None }
  }

  /// Returns the number of restarts already granted in the current window.
  pub(crate) fn count(&self) -> u32 {
    self.count
  }

  /// Records one plain restart request with no time window.
  ///
  /// Mirrors the `(Some(retries), None)` branch of
  /// `ChildRestartStats.requestRestartPermission`: the request always counts,
  /// and the caller compares the count against the limit.
  pub(crate) fn record_plain(&mut self) {
    self.count = self.count.saturating_add(1);
  }

  /// Answers whether one more restart is allowed and updates the history.
  ///
  /// Mirrors `ChildRestartStats.requestRestartPermission` for a finite limit.
  /// `window` is `None` for no time range. A zero limit denies at once.
  /// With a window, the history opens it at `now`
  /// the first time, allows restarts while `now` is at most `window` past the
  /// start (the boundary counts), and otherwise closes the window and opens a
  /// new one with a count of one, which always allows the next restart.
  pub(crate) fn request_permission(&mut self, limit: RestartLimit, window: Option<Duration>, now: Duration) -> bool {
    if limit.forbids_restart() {
      return false;
    }
    match window {
      | None => {
        self.record_plain();
        limit.permits(self.count)
      },
      | Some(window) => self.retries_in_window_okay(limit, window, now),
    }
  }

  /// Applies Pekko's simple window algorithm for a bounded time range.
  fn retries_in_window_okay(&mut self, limit: RestartLimit, window: Duration, now: Duration) -> bool {
    let retries_done = self.count.saturating_add(1);
    let window_start = match self.window_start {
      | Some(start) => start,
      | None => {
        self.window_start = Some(now);
        now
      },
    };
    let inside_window = now.saturating_sub(window_start) <= window;
    if inside_window {
      self.count = retries_done;
      limit.permits(retries_done)
    } else {
      self.count = 1;
      self.window_start = Some(now);
      true
    }
  }
}
