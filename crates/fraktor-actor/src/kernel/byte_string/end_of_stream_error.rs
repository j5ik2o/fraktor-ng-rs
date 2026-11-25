//! The error a stream reports when it ends before the bytes it must skip.

use core::fmt;

/// [`ByteStringInputStream::skip_n_bytes`](super::ByteStringInputStream::skip_n_bytes)
/// reached the end of the stream before it had skipped the bytes it was asked
/// to.
///
/// Mirrors the `java.io.EOFException` that `InputStream.skipNBytes` throws.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EndOfStreamError;

impl fmt::Display for EndOfStreamError {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str("end of stream")
  }
}

impl core::error::Error for EndOfStreamError {}
