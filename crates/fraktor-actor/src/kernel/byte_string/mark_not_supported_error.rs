//! The error a stream that does not support marks reports on reset.

use core::fmt;

/// [`ByteStringInputStream::reset`](super::ByteStringInputStream::reset) was
/// called on a stream that does not support marks.
///
/// Mirrors the `IOException("mark/reset not supported")` that Java's
/// `InputStream.reset()` throws, as the `java.io.SequenceInputStream` of
/// Pekko's concatenated `ByteStrings` does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MarkNotSupportedError;

impl fmt::Display for MarkNotSupportedError {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str("mark/reset not supported")
  }
}

impl core::error::Error for MarkNotSupportedError {}
