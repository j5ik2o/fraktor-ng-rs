//! One scheduled callback together with the interval that repeats it.

use core::time::Duration;

use super::Callback;

/// A single registration's callback and, for a repeating one, its interval.
///
/// A one-shot registration carries no interval, so it runs once and is gone. A
/// repeating registration carries its interval so the scheduler can compute the
/// next deadline after the callback returns.
pub(crate) struct Registration {
  interval: Option<Duration>,
  callback: Callback,
}

impl Registration {
  /// Makes a registration that repeats every `interval`.
  pub(crate) fn repeating(interval: Duration, callback: Callback) -> Self {
    Self { interval: Some(interval), callback }
  }

  /// Makes a registration that runs once and then is gone.
  pub(crate) fn once(callback: Callback) -> Self {
    Self { interval: None, callback }
  }

  /// Returns the interval if this registration repeats.
  pub(crate) fn interval(&self) -> Option<Duration> {
    self.interval
  }

  /// Returns the callback this registration runs.
  pub(crate) fn callback(&self) -> Callback {
    self.callback.clone()
  }
}
