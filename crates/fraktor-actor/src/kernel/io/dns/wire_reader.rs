//! A read position in a DNS message.

use super::WireFormatError;

#[cfg(test)]
#[path = "wire_reader_test.rs"]
mod tests;

/// Reads big-endian values from a DNS message, one after another.
///
/// Plays the part of the pair that `org.apache.pekko.io.dns.internal` passes
/// around while reading a message: the `ByteIterator` that holds the read
/// position, and the whole `ByteString` that compression pointers refer to.
///
/// A read that runs past the end of the message fails with
/// [`WireFormatError::Truncated`] and leaves the reader at the end, as Pekko's
/// `getShort` and `getInt` consume the bytes that are left before they throw.
#[derive(Debug, Clone)]
pub struct WireReader<'a> {
  message: &'a [u8],
  position: usize,
}

impl<'a> WireReader<'a> {
  /// Creates a reader at the start of `message`.
  ///
  /// Corresponds to Pekko's `msg.iterator`.
  #[must_use]
  pub const fn new(message: &'a [u8]) -> Self {
    Self::starting_at(message, 0)
  }

  /// Creates a reader at `position` in `message`, or at its end when
  /// `position` lies past the end.
  ///
  /// Corresponds to Pekko's `msg.iterator.drop(position)`.
  #[must_use]
  pub const fn starting_at(message: &'a [u8], position: usize) -> Self {
    let position = if position < message.len() { position } else { message.len() };
    Self { message, position }
  }

  /// Returns the offset of the next byte to read.
  #[must_use]
  pub const fn position(&self) -> usize {
    self.position
  }

  /// Creates a reader over the same message at `position`, which is how a
  /// compression pointer moves the read.
  pub(crate) const fn jump_to(&self, position: usize) -> Self {
    Self::starting_at(self.message, position)
  }

  /// Reads one byte.
  pub(crate) fn read_u8(&mut self) -> Result<u8, WireFormatError> {
    let byte = *self.message.get(self.position).ok_or(WireFormatError::Truncated)?;
    self.position += 1;
    Ok(byte)
  }

  /// Reads a big-endian `u16`.
  pub(crate) fn read_u16(&mut self) -> Result<u16, WireFormatError> {
    Ok(u16::from_be_bytes([self.read_u8()?, self.read_u8()?]))
  }

  /// Reads a big-endian `i32`.
  pub(crate) fn read_i32(&mut self) -> Result<i32, WireFormatError> {
    Ok(i32::from_be_bytes([self.read_u8()?, self.read_u8()?, self.read_u8()?, self.read_u8()?]))
  }

  /// Reads up to `length` bytes, fewer when the message ends first.
  ///
  /// Corresponds to Pekko's `it.clone().take(length)` followed by
  /// `it.drop(length)`, neither of which fails at the end of the message.
  pub(crate) fn read_up_to(&mut self, length: usize) -> &'a [u8] {
    let end = self.position.saturating_add(length).min(self.message.len());
    let bytes = &self.message[self.position..end];
    self.position = end;
    bytes
  }
}
