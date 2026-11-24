//! A stream that reads the bytes of a byte string.

use crate::kernel::byte_iterator::ByteIterator;

#[cfg(test)]
#[path = "byte_string_input_stream_test.rs"]
mod tests;

/// Reads the bytes of a [`ByteString`](super::ByteString) from the start, in
/// order.
///
/// Mirrors what `ByteString.asInputStream` returns in Pekko, a
/// `java.io.InputStream`. Java reports the end of the stream as `-1`; this
/// type reports it as `None`.
#[derive(Clone, Debug)]
pub struct ByteStringInputStream {
  bytes: ByteIterator,
}

impl ByteStringInputStream {
  pub(crate) fn new(bytes: ByteIterator) -> Self {
    Self { bytes }
  }

  /// Reads the next byte, or returns `None` at the end of the stream.
  ///
  /// Corresponds to Java's `InputStream.read()`.
  pub fn read(&mut self) -> Option<u8> {
    self.bytes.next()
  }

  /// Reads up to `buffer.len()` bytes into the start of `buffer` and returns
  /// how many it read, or returns `None` at the end of the stream.
  ///
  /// Corresponds to Java's `InputStream.read(buffer)`. As the `InputStream`
  /// contract and Pekko's `UnsynchronizedByteArrayInputStream` do, an empty
  /// `buffer` reads `Some(0)` without looking for the end of the stream.
  pub fn read_into(&mut self, buffer: &mut [u8]) -> Option<usize> {
    if buffer.is_empty() {
      Some(0)
    } else if self.bytes.has_next() {
      Some(self.bytes.copy_into(buffer))
    } else {
      None
    }
  }
}
