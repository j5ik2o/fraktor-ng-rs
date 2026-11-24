// Ported from Pekko v1.7.1 (SHA 9a54d1294c3ac7821bbed3c940769c3d5027d1ff):
//   actor-tests/src/test/scala/org/apache/pekko/io/UnsynchronizedByteArrayInputStreamSpec.scala
//
// The original's three cases under "UnsynchronizedByteArrayInputStream must"
// are kept as one test each, named after the case, with the original name as a
// comment. Every assertion runs in the original order.
//
// The original builds the stream as
// `new UnsynchronizedByteArrayInputStream("abc".getBytes(UTF_8))`. Pekko's
// `ByteString1C.asInputStream` builds exactly that stream over its bytes
// (ByteString.scala L308), so the port builds it as
// `ByteString::from_string("abc").as_input_stream()`, which encodes "abc" as
// UTF-8 and returns the port's `ByteStringInputStream`.
//
// - `markSupported()` is `mark_supported()`.
// - `mark(readAheadLimit)` is `mark(read_ahead_limit)`. The original ignores the
//   limit (its comment on L34); the port ignores it too.
// - `reset()` is `reset()`. Java's `reset()` throws an `IOException` when the
//   stream does not support marks; the port's returns that failure as an
//   `Err`. This stream supports marks, so the test expects `Ok`.
// - `skip(n)` is `skip(n)`, which returns how many bytes it skipped. Java's
//   `long` count is `usize` here.
// - `read()` returns `Some(byte)` where Java returns the byte as an `Int`, and
//   `None` where Java returns `-1` at the end of the stream. `'a'` is `b'a'`.
// - `close()` is dropping the stream, which holds no other resource.
//
// The test calls `mark_supported`, `mark`, `reset` and `skip` on the production
// `ByteStringInputStream`. Until they exist the file does not compile: that
// compile error is the intended red for this node.

use fraktor_actor::kernel::byte_string::{ByteString, ByteStringInputStream};

// The original's `new UnsynchronizedByteArrayInputStream("abc".getBytes(UTF_8))`.
fn abc_stream() -> ByteStringInputStream {
  ByteString::from_string("abc").as_input_stream()
}

// "support mark and reset"
#[test]
fn support_mark_and_reset() {
  let mut stream = abc_stream();
  assert!(stream.mark_supported());
  assert_eq!(stream.read(), Some(b'a'));
  // The limit is ignored, as it is in the original and in `ByteArrayInputStream`.
  stream.mark(1);
  assert_eq!(stream.read(), Some(b'b'));
  stream.reset().expect("a stream over one array supports reset");
  assert_eq!(stream.read(), Some(b'b'));
  drop(stream);
}

// "support skip"
#[test]
fn support_skip() {
  let mut stream = abc_stream();
  assert_eq!(stream.skip(1), 1);
  assert_eq!(stream.read(), Some(b'b'));
  drop(stream);
}

// "support skip with large value"
#[test]
fn support_skip_with_large_value() {
  let mut stream = abc_stream();
  // Only 3 bytes are left to skip.
  assert_eq!(stream.skip(50), 3);
  assert_eq!(stream.read(), None);
  drop(stream);
}
