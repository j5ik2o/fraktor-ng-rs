//! A consuming iterator over the bytes of one or more byte arrays.

use alloc::{sync::Arc, vec, vec::Vec};

#[cfg(test)]
#[path = "byte_iterator_value_test.rs"]
mod tests;

/// Yields the bytes of one or more byte arrays in order, consuming them.
///
/// Mirrors `org.apache.pekko.util.ByteIterator`, both its single-array form
/// (`ByteArrayIterator`) and its form over several arrays
/// (`MultiByteArrayIterator`). Searching consumes the iterator up to and
/// including the byte it finds, as Pekko's does, so each search that should
/// start from the beginning needs a new iterator.
#[derive(Clone, Debug)]
pub struct ByteIterator {
  // Never holds an empty array, so the iterator has a next byte exactly when
  // `current` is still in range.
  arrays: Vec<Arc<[u8]>>,
  current: usize,
  offset: usize,
}

impl ByteIterator {
  /// Creates an iterator over a copy of `bytes`.
  ///
  /// Corresponds to Pekko's `ByteArrayIterator(array)`.
  #[must_use]
  pub fn from_array(bytes: &[u8]) -> Self {
    Self::over_arrays(vec![Arc::from(bytes)])
  }

  /// Creates an iterator over the bytes of `arrays`, one array after another.
  pub(crate) fn over_arrays(arrays: Vec<Arc<[u8]>>) -> Self {
    let arrays = arrays.into_iter().filter(|array| !array.is_empty()).collect();
    Self { arrays, current: 0, offset: 0 }
  }

  /// Returns whether a byte is left to read.
  pub(crate) fn has_next(&self) -> bool {
    self.current < self.arrays.len()
  }

  /// Consumes the iterator up to the first byte equal to `elem`, and returns
  /// that byte's index counted from where the iterator started.
  ///
  /// Corresponds to Pekko's `indexOf(elem)`, which returns `-1` instead of
  /// `None` when no byte matches.
  pub fn index_of(&mut self, elem: u8) -> Option<usize> {
    self.index_of_from(elem, 0)
  }

  /// Skips `from` bytes, then consumes the iterator up to the first byte equal
  /// to `elem`, and returns that byte's index counted from where the iterator
  /// started.
  ///
  /// Corresponds to both of Pekko's `indexOf(elem, from)` overloads, which
  /// return `-1` instead of `None` when no byte matches.
  pub fn index_of_from(&mut self, elem: u8, from: usize) -> Option<usize> {
    self.index_where(|byte| byte == elem, from)
  }

  // Pekko's `indexWhere(p, from)`: the index counts the skipped bytes too.
  fn index_where(&mut self, predicate: impl Fn(u8) -> bool, from: usize) -> Option<usize> {
    let mut index = 0;
    while index < from && self.next().is_some() {
      index += 1;
    }
    for byte in self.by_ref() {
      if predicate(byte) {
        return Some(index);
      }
      index += 1;
    }
    None
  }

  /// Moves up to `buffer.len()` bytes into `buffer`, and returns how many it
  /// moved.
  pub(crate) fn copy_into(&mut self, buffer: &mut [u8]) -> usize {
    let mut copied = 0;
    while copied < buffer.len() && self.has_next() {
      let array = &self.arrays[self.current];
      let count = (array.len() - self.offset).min(buffer.len() - copied);
      buffer[copied..copied + count].copy_from_slice(&array[self.offset..self.offset + count]);
      copied += count;
      self.advance(count);
    }
    copied
  }

  // Moves past `count` bytes of the current array, which has at least that
  // many left, and onto the next array when this one is used up.
  fn advance(&mut self, count: usize) {
    self.offset += count;
    if self.offset == self.arrays[self.current].len() {
      self.current += 1;
      self.offset = 0;
    }
  }
}

impl Iterator for ByteIterator {
  type Item = u8;

  fn next(&mut self) -> Option<u8> {
    let byte = *self.arrays.get(self.current)?.get(self.offset)?;
    self.advance(1);
    Some(byte)
  }
}
