//! The error a stream reports when a buffer range lies outside the buffer.

use core::fmt;

/// The range `offset .. offset + length` given to
/// [`ByteStringInputStream::read_range`](super::ByteStringInputStream::read_range)
/// or [`read_n_bytes_into`](super::ByteStringInputStream::read_n_bytes_into)
/// does not lie inside the buffer.
///
/// Mirrors the `IndexOutOfBoundsException` that `Objects.checkFromIndexSize`
/// throws in Java's `InputStream.read(b, off, len)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexOutOfBoundsError;

impl fmt::Display for IndexOutOfBoundsError {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str("range out of bounds")
  }
}

impl core::error::Error for IndexOutOfBoundsError {}
