//! The error a wildcard index insert reports.

use alloc::string::String;
use core::fmt;

/// The reason a wildcard index insert rejected its elements.
///
/// Mirrors the `IllegalArgumentException` the original's `insert` throws when
/// an element uses a double wildcard as a suffix, such as `"a**"`. The port
/// returns this error instead of throwing (see the design document for #187,
/// section 2.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WildcardIndexError {
  /// An element other than `"**"` itself ends with `"**"`.
  DoubleWildcardAsSuffix {
    /// The rejected element.
    element: String,
  },
}

impl fmt::Display for WildcardIndexError {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      | Self::DoubleWildcardAsSuffix { element } => {
        write!(formatter, "a double wildcard may not be used as a suffix: {element}")
      },
    }
  }
}

impl core::error::Error for WildcardIndexError {}
