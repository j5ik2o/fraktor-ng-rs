//! A strictly positive cache time-to-live.

use core::time::Duration;

use super::NonPositiveTtlError;

#[cfg(test)]
#[path = "ttl_test.rs"]
mod tests;

/// A cache time-to-live that is strictly greater than zero.
///
/// Mirrors `org.apache.pekko.io.dns.CachePolicy.Ttl`, whose `fromPositive`
/// rejects a non-positive value. A [`Duration`] cannot be negative, so the type
/// carries the original's negative check and only zero reaches
/// [`from_positive`](Self::from_positive) as a rejection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ttl {
  value: Duration,
}

impl Ttl {
  /// Turns a strictly positive [`Duration`] into a TTL.
  ///
  /// Returns [`NonPositiveTtlError`] for a zero duration, as the original's
  /// `fromPositive` does.
  pub fn from_positive(value: Duration) -> Result<Self, NonPositiveTtlError> {
    if value.is_zero() {
      return Err(NonPositiveTtlError);
    }
    Ok(Self { value })
  }

  /// Returns the TTL duration.
  pub fn value(&self) -> Duration {
    self.value
  }
}
