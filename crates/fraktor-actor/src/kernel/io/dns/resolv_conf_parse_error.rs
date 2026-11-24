//! The error a `resolv.conf` line parser reports.

use core::fmt;

/// The reason a `resolv.conf` could not be parsed.
///
/// Mirrors the `NumberFormatException` that `org.apache.pekko.io.dns.internal.ResolvConfParser`
/// raises when an `ndots:` option does not hold an integer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvConfParseError {
  /// A `ndots:` option's value is not an `i32`.
  InvalidNdots,
}

impl fmt::Display for ResolvConfParseError {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str(match self {
      | Self::InvalidNdots => "resolv.conf ndots option is not an integer",
    })
  }
}
