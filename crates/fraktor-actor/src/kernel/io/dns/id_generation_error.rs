//! The error an identifier generator reports.

use core::fmt;

#[cfg(test)]
#[path = "id_generation_error_test.rs"]
mod tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// The reason an identifier could not be produced.
///
/// Mirrors the failure the original's secure generator reports when the source
/// of randomness cannot be read.
pub enum IdGenerationError {
  /// The operating system's entropy source could not be read.
  EntropyUnavailable,
}

impl fmt::Display for IdGenerationError {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str(match self {
      | Self::EntropyUnavailable => "the operating system entropy source is unavailable",
    })
  }
}

impl core::error::Error for IdGenerationError {}
