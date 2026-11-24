//! An immutable sequence of bytes, kept as one or more shared byte arrays.

use alloc::{sync::Arc, vec::Vec};

use super::ByteStringInputStream;
use crate::kernel::byte_iterator::ByteIterator;

#[cfg(test)]
#[path = "byte_string_value_test.rs"]
mod tests;

/// An immutable sequence of bytes.
///
/// Mirrors the parts of `org.apache.pekko.util.ByteString` that reading needs.
/// Pekko has three representations: a whole array (`ByteString1C`), a slice of
/// an array (`ByteString1`), and a concatenation (`ByteStrings`). This type
/// covers all three with a list of shared arrays: concatenating shares the
/// arrays of both operands instead of copying them, as `ByteStrings` does.
#[derive(Clone, Debug, Default)]
pub struct ByteString {
  // Never holds an empty array.
  arrays: Vec<Arc<[u8]>>,
}

impl ByteString {
  /// Creates a byte string with no bytes.
  ///
  /// Corresponds to Pekko's `ByteString.empty`.
  #[must_use]
  pub fn empty() -> Self {
    Self { arrays: Vec::new() }
  }

  /// Creates a byte string from the UTF-8 encoding of `string`.
  ///
  /// Corresponds to Pekko's `ByteString1C.fromString(string)` and
  /// `ByteString1.fromString(string)`, which also encode as UTF-8.
  #[must_use]
  pub fn from_string(string: &str) -> Self {
    Self::from_bytes(string.as_bytes())
  }

  /// Creates a byte string from a copy of up to `length` bytes of `array`,
  /// starting at `offset`.
  ///
  /// Corresponds to Pekko's `ByteString.fromArray(array, offset, length)`.
  /// Like Pekko's, it copies only the bytes that exist: an `offset` past the
  /// end gives an empty byte string, and a `length` past the end stops at the
  /// end.
  #[must_use]
  pub fn from_array(array: &[u8], offset: usize, length: usize) -> Self {
    let start = offset.min(array.len());
    let end = start + length.min(array.len() - start);
    Self::from_bytes(&array[start..end])
  }

  fn from_bytes(bytes: &[u8]) -> Self {
    if bytes.is_empty() {
      Self::empty()
    } else {
      Self { arrays: alloc::vec![Arc::from(bytes)] }
    }
  }

  /// Returns a byte string with the bytes of `self` followed by the bytes of
  /// `other`.
  ///
  /// Corresponds to Pekko's `ByteStrings(self, other)` and `self ++ other`.
  /// Both operands' arrays are shared, not copied.
  #[must_use]
  pub fn concat(&self, other: &ByteString) -> Self {
    let arrays = self.arrays.iter().chain(other.arrays.iter()).cloned().collect();
    Self { arrays }
  }

  /// Returns an iterator over the bytes, in order.
  ///
  /// Corresponds to Pekko's `iterator`.
  #[must_use]
  pub fn iterator(&self) -> ByteIterator {
    ByteIterator::over_arrays(self.arrays.clone())
  }

  /// Returns a stream that reads the bytes from the start, in order.
  ///
  /// Corresponds to Pekko's `asInputStream`.
  #[must_use]
  pub fn as_input_stream(&self) -> ByteStringInputStream {
    ByteStringInputStream::new(self.iterator())
  }
}
