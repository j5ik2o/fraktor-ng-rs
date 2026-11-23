//! The multiplier for scaled test-kit timeouts.

use core::time::Duration;

/// A finite, positive multiplier for test-kit durations.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TestTimeFactor(f64);

impl TestTimeFactor {
  /// Creates a test time factor.
  ///
  /// # Panics
  ///
  /// Panics when `factor` is not finite and strictly positive.
  pub fn new(factor: f64) -> Self {
    assert!(factor.is_finite() && factor > 0.0, "the time factor must be a finite positive number, was {factor}");
    Self(factor)
  }

  pub(super) fn value(self) -> f64 {
    self.0
  }

  pub(super) fn dilate(self, duration: Duration) -> Duration {
    duration.mul_f64(self.0)
  }
}

impl Default for TestTimeFactor {
  fn default() -> Self {
    Self(1.0)
  }
}
