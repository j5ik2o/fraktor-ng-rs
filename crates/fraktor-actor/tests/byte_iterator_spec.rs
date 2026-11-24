// Ported from Pekko v1.7.1 (SHA 9a54d1294c3ac7821bbed3c940769c3d5027d1ff):
//   actor-tests/src/test/scala/org/apache/pekko/util/ByteIteratorSpec.scala
//
// The original's single case under "A ByteIterator should" is kept as one test,
// named after the case, with the original name as a comment. Its eight
// assertions run in the original order, each on a fresh iterator over the same
// six bytes, because a search consumes the iterator (original L25-L27).
//
// The original builds the iterator with `ByteArrayIterator(Array(...))`; the
// port's `ByteIterator::from_array(&[...])` is the same iterator over a byte
// array. The original's `indexOf(elem)` is the port's `index_of(elem)`, and its
// `indexOf(elem, from)` is the port's `index_of_from(elem, from)`. Both return
// the index counted from where the iterator started, as the original's
// `indexWhere` does (ByteIterator.scala L487-L506). The original returns `-1`
// when nothing matches; the port returns `None`. Every search in this case
// finds a byte, so each expected value is `Some(index)`.
//
// The original has two `indexOf(elem, from)` overloads: the generic
// `indexOf[B >: Byte](elem: B, from: Int)` that a plain call resolves to, and
// `indexOf(elem: Byte, from: Int)`, which the original reaches only by
// reflection in `otherIndexOf` (L36-L40). Both delegate to
// `indexWhere(_ == elem, from)`. Rust has neither overloads nor reflection, so
// both are the port's single `index_of_from`. The port keeps the original's
// `otherIndexOf` helper as `other_index_of`, so the last three assertions still
// go through a helper as the original's do.
//
// The test calls the production `ByteIterator`. Until it exists the file does
// not compile: that compile error is the intended red for this node.

use fraktor_actor::kernel::byte_iterator::ByteIterator;

// The original's `freshIterator()`.
fn fresh_iterator() -> ByteIterator {
  ByteIterator::from_array(&[0x20, 0x20, 0x10, 0x20, 0x20, 0x10])
}

// The original's `otherIndexOf(iterator, byte, from)`, which calls the
// `indexOf(Byte, Int)` overload by reflection.
fn other_index_of(mut iterator: ByteIterator, byte: u8, from: usize) -> Option<usize> {
  iterator.index_of_from(byte, from)
}

// "A ByteIterator should correctly implement indexOf"
#[test]
fn a_byte_iterator_should_correctly_implement_index_of() {
  assert_eq!(fresh_iterator().index_of(0x20), Some(0));
  assert_eq!(fresh_iterator().index_of(0x10), Some(2));

  assert_eq!(fresh_iterator().index_of_from(0x20, 1), Some(1));
  assert_eq!(fresh_iterator().index_of_from(0x10, 1), Some(2));
  assert_eq!(fresh_iterator().index_of_from(0x10, 3), Some(5));

  assert_eq!(other_index_of(fresh_iterator(), 0x20, 1), Some(1));
  assert_eq!(other_index_of(fresh_iterator(), 0x10, 1), Some(2));
  assert_eq!(other_index_of(fresh_iterator(), 0x10, 3), Some(5));
}
