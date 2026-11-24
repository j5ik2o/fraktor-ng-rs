//! The error a non-positive TTL reports.

use core::fmt;

/// The reason a cache time-to-live could not be built.
///
/// Mirrors the `IllegalArgumentException` the original
/// `org.apache.pekko.io.dns.CachePolicy.Ttl.fromPositive` throws for a
/// non-positive duration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NonPositiveTtlError;

impl fmt::Display for NonPositiveTtlError {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str("the cache time-to-live must be positive")
  }
}

impl core::error::Error for NonPositiveTtlError {}
