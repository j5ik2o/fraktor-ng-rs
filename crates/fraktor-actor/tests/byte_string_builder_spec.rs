// Ported from Pekko v1.7.1 (SHA 9a54d1294c3ac7821bbed3c940769c3d5027d1ff):
//   actor-tests/src/test/scala/org/apache/pekko/util/ByteStringBuilderSpec.scala
//
// The original's three `++=` cases are kept as one test each, named after the
// case, with the original name as a comment. Each test builds the result in one
// chain and compares it with `shouldEqual` (`assert_eq!` in the port).
//
// The original API and its Rust counterpart:
//
// - `ByteString.newBuilder` is `ByteString::new_builder()`.
// - `++=(xs: IterableOnce[Byte])` is `add_all<I: IntoIterator<Item = u8>>`.
// - `++=(bytes: ByteString)` is a dedicated `ByteString` overload, so it
//   corresponds to `append(&ByteString)`, not to `add_all`.
// - `result()` is `result()`.
// - `ByteString("a")` and `ByteString.fromString(s)` are
//   `ByteString::from_string(s)`.
// - `List[Byte]('a')` (a LinearSeq) is `std::collections::LinkedList::from([b'a'])`.
// - `Vector[Byte]('a')` (an IndexedSeq) is `vec![b'a']`.
// - `shouldEqual` is `assert_eq!`, which needs `ByteString: PartialEq + Debug`.
//
// The test calls the production `ByteString` and `ByteStringBuilder`. Until they
// exist the file does not compile: that compile error is the intended red for
// this node.

use std::collections::LinkedList;

use fraktor_actor::kernel::byte_string::ByteString;

// "handle ++= with LinearSeq"
#[test]
fn handle_plus_plus_eq_with_linear_seq() {
  let result: ByteString = ByteString::new_builder().add_all(LinkedList::from(*b"a")).result();
  assert_eq!(result, ByteString::from_string("a"));
}

// "handle ++= with IndexedSeq"
#[test]
fn handle_plus_plus_eq_with_indexed_seq() {
  let result: ByteString = ByteString::new_builder().add_all(vec![b'a']).result();
  assert_eq!(result, ByteString::from_string("a"));
}

// "handle ++= with ByteString"
#[test]
fn handle_plus_plus_eq_with_byte_string() {
  let result: ByteString = ByteString::new_builder().append(&ByteString::from_string("a")).result();
  assert_eq!(result, ByteString::from_string("a"));
}
