//! The error a version parse returns.

use core::fmt;

#[cfg(test)]
#[path = "version_parse_error_test.rs"]
mod tests;

/// Why a version string did not parse.
///
/// Mirrors the exceptions that `org.apache.pekko.util.Version` throws when it
/// parses at construction: `IllegalArgumentException` for an empty or
/// unsupported segment count and `NumberFormatException` for a segment whose
/// number does not read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionParseError {
  /// The text had no segments, as in an empty string.
  Empty,
  /// The text had zero segments or more than three.
  UnsupportedSegmentCount,
  /// A segment expected a number but did not read one.
  InvalidNumber,
}

impl fmt::Display for VersionParseError {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str(match self {
      | Self::Empty => "empty version",
      | Self::UnsupportedSegmentCount => "unsupported segment count",
      | Self::InvalidNumber => "invalid number",
    })
  }
}

impl core::error::Error for VersionParseError {}
