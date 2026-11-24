//! The order in which the bytes of a multi-byte number are read.

/// The order in which [`ByteIterator`](super::ByteIterator) reads the bytes of
/// a multi-byte number.
///
/// Mirrors `java.nio.ByteOrder`, the implicit order Pekko's
/// `ByteIterator.getShort`, `getInt` and `getLong` take.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ByteOrder {
  /// The most significant byte comes first, as `ByteOrder.BIG_ENDIAN`.
  BigEndian,
  /// The least significant byte comes first, as `ByteOrder.LITTLE_ENDIAN`.
  LittleEndian,
}
