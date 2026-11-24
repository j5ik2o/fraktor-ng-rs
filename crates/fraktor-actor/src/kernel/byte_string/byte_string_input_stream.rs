//! A stream that reads the bytes of a byte string.

use super::MarkNotSupportedError;
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
///
/// Whether the stream supports [`mark`](Self::mark) and
/// [`reset`](Self::reset) follows Pekko's representations. A byte string of at
/// most one array is Pekko's `ByteString1C` or `ByteString1`, whose stream is
/// an `UnsynchronizedByteArrayInputStream` and supports marks. A byte string of
/// several arrays is Pekko's `ByteStrings`, whose stream is a
/// `java.io.SequenceInputStream` and does not.
#[derive(Clone, Debug)]
pub struct ByteStringInputStream {
  bytes: ByteIterator,
  // Where `reset` goes back to: the start of the stream until `mark` is called.
  // `None` when the stream does not support marks.
  marked: Option<ByteIterator>,
}

impl ByteStringInputStream {
  /// Creates a stream over `bytes` that supports marks when `mark_supported`
  /// is `true`.
  pub(crate) fn new(bytes: ByteIterator, mark_supported: bool) -> Self {
    let marked = mark_supported.then(|| bytes.clone());
    Self { bytes, marked }
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
  ///
  /// A read fills `buffer` across the parts of a concatenated byte string.
  /// Pekko's concatenation reads through `java.io.SequenceInputStream`, which
  /// stops each read at the end of the current part, so reading `a` followed
  /// by `bc` into a 3-byte buffer returns 1 byte there and 3 bytes here. The
  /// bytes and their order are the same; only how many one read returns
  /// differs, which the `InputStream` contract leaves to the implementation.
  pub fn read_into(&mut self, buffer: &mut [u8]) -> Option<usize> {
    if buffer.is_empty() {
      Some(0)
    } else if self.bytes.has_next() {
      Some(self.bytes.copy_into(buffer))
    } else {
      None
    }
  }

  /// Returns whether the stream supports [`mark`](Self::mark) and
  /// [`reset`](Self::reset): `true` for a byte string of at most one array,
  /// `false` for a concatenation of several.
  ///
  /// Corresponds to Java's `InputStream.markSupported()`.
  #[must_use]
  pub const fn mark_supported(&self) -> bool {
    self.marked.is_some()
  }

  /// Marks the current position, so that [`reset`](Self::reset) goes back to
  /// it. Does nothing when the stream does not support marks.
  ///
  /// Corresponds to Java's `InputStream.mark(readAheadLimit)`. As Pekko's
  /// `UnsynchronizedByteArrayInputStream` and Java's `ByteArrayInputStream` do,
  /// the stream ignores `read_ahead_limit`: the mark stays valid however many
  /// bytes are read after it.
  pub fn mark(&mut self, read_ahead_limit: usize) {
    let _ = read_ahead_limit;
    if let Some(marked) = &mut self.marked {
      marked.clone_from(&self.bytes);
    }
  }

  /// Goes back to the position of the last [`mark`](Self::mark), or to the
  /// start of the stream when nothing was marked.
  ///
  /// Corresponds to Java's `InputStream.reset()`.
  ///
  /// # Errors
  ///
  /// Returns [`MarkNotSupportedError`] when the stream does not support marks,
  /// where Java throws an `IOException`. The position does not change.
  pub fn reset(&mut self) -> Result<(), MarkNotSupportedError> {
    let marked = self.marked.as_ref().ok_or(MarkNotSupportedError)?;
    self.bytes.clone_from(marked);
    Ok(())
  }

  /// Moves past up to `count` bytes without reading them, and returns how many
  /// it moved past: `count`, or the bytes left when fewer are left, so `0` at
  /// the end of the stream.
  ///
  /// Corresponds to Java's `InputStream.skip(n)`, for streams that support
  /// marks and for those that do not. Pekko's
  /// `UnsynchronizedByteArrayInputStream` moves its position by the whole `n`
  /// even past the end, so a later skip there returns a negative count; this
  /// stream stops at the end and returns `0`.
  pub fn skip(&mut self, count: usize) -> usize {
    self.bytes.skip_bytes(count)
  }
}
