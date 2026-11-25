// Ported from Pekko v2.0.0 (SHA 9fbd2283b1cf701c90a1223bf621942c191db61f):
//   actor-tests/src/test/scala/org/apache/pekko/io/UnsynchronizedByteArrayInputStreamSpec.scala
// First ported from v1.7.1 (SHA 9a54d1294c3ac7821bbed3c940769c3d5027d1ff), which
// had the first three cases. v2.0.0 (upstream a5cf13f26c, #3541) adds eleven
// cases and three assertions to "support skip with large value". This file
// ports twelve of the fourteen; see "Cases not ported" below.
//
// The ported cases under "UnsynchronizedByteArrayInputStream must" are kept as
// one test each, named after the case, with the original name as a comment.
// Every assertion runs in the original order, except the ones whose inputs a
// `usize` cannot hold (listed where they occur).
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

// Cases not ported (whole-case exclusions need a human approval that is not
// recorded yet, so they are left out rather than excluded):
//   * "reject negative skip": `skip` takes a `usize`, which cannot be negative.
//   * "not copy the backing array": the original writes to the array after
//     building the stream and reads the new byte back. Safe Rust cannot write
//     to an array while a stream borrows it, and the port's stream copies the
//     bytes, so the observation is the opposite.
//
// The v2.0.0 methods map as follows. Java's `-1` at the end of the stream is
// `None`, its `IndexOutOfBoundsException` is `Err(IndexOutOfBoundsError)` and
// its `EOFException` is `Err(EndOfStreamError)`:
//   - `available()` is `available()`.
//   - `skipNBytes(n)` is `skip_n_bytes(n)`.
//   - `read(buf)` is `read_into(&mut buf)`; `read(buf, off, len)` is
//     `read_range(&mut buf, off, len)`.
//   - `readAllBytes()` is `read_all_bytes()`; `readNBytes(len)` is
//     `read_n_bytes(len)`; `readNBytes(buf, off, len)` is
//     `read_n_bytes_into(&mut buf, off, len)`.
//   - `transferTo(out)` is `transfer_to(&mut out)` with a `Vec<u8>` as the
//     output stream.
//   - `new UnsynchronizedByteArrayInputStream(data, offset, length)` is
//     `ByteString::from_array(data, offset, length).as_input_stream()`, which
//     clamps the range to the array as the constructor does.

use fraktor_actor::kernel::byte_string::{
  ByteString, ByteStringInputStream, EndOfStreamError, IndexOutOfBoundsError,
};

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
  assert_eq!(stream.available(), 0);
  assert_eq!(stream.read(), None);
  // `skip(Long.MaxValue)` must not overflow; `usize::MAX` stands in for it.
  assert_eq!(stream.skip(usize::MAX), 0);
  assert_eq!(stream.available(), 0);
  drop(stream);
}

// The original's `bytes(s)`: the UTF-8 bytes of `s`, as a stream over all of them.
fn stream_of(text: &str) -> ByteStringInputStream {
  ByteString::from_string(text).as_input_stream()
}

// The original's `str(b)`.
fn str_of(bytes: &[u8]) -> &str {
  core::str::from_utf8(bytes).expect("the case reads UTF-8 text")
}

// "support skipNBytes"
//
// `skipNBytes(-1)` (a no-op, as in `InputStream`) is not ported: a `usize`
// cannot be negative.
#[test]
fn support_skip_n_bytes() {
  let mut stream = stream_of("abcdef");
  assert_eq!(stream.skip_n_bytes(0), Ok(()));
  assert_eq!(stream.read(), Some(b'a'));
  assert_eq!(stream.skip_n_bytes(2), Ok(()));
  assert_eq!(stream.read(), Some(b'd'));
  assert_eq!(stream.skip_n_bytes(3), Err(EndOfStreamError));
  assert_eq!(stream.available(), 0);
  drop(stream);
}

// "support read into array"
//
// `read(buf, -1, 1)` is not ported: a `usize` cannot be negative.
#[test]
fn support_read_into_array() {
  let mut stream = stream_of("abcdef");
  let mut buf = [0_u8; 4];
  assert_eq!(stream.read_into(&mut buf), Some(4));
  assert_eq!(str_of(&buf), "abcd");
  assert_eq!(stream.read_range(&mut buf, 1, 0), Ok(Some(0)));
  assert_eq!(stream.read_range(&mut buf, 1, 3), Ok(Some(2)));
  assert_eq!(str_of(&buf), "aefd");
  assert_eq!(stream.read_into(&mut buf), None);
  // A zero-length read at the end of the stream returns 0, not -1.
  assert_eq!(stream.read_range(&mut buf, 0, 0), Ok(Some(0)));
  assert_eq!(stream.read_range(&mut buf, 2, 3), Err(IndexOutOfBoundsError));
  drop(stream);
}

// "support readAllBytes"
#[test]
fn support_read_all_bytes() {
  let mut stream = stream_of("abcdef");
  assert_eq!(stream.read(), Some(b'a'));
  assert_eq!(str_of(&stream.read_all_bytes()), "bcdef");
  assert_eq!(stream.available(), 0);
  assert_eq!(stream.read_all_bytes(), Vec::<u8>::new());
  drop(stream);
}

// "support readNBytes(int)"
//
// `readNBytes(-1)`, which throws `IllegalArgumentException`, is not ported: a
// `usize` cannot be negative. `Int.MaxValue` is `usize::MAX`.
#[test]
fn support_read_n_bytes_int() {
  let mut stream = stream_of("abcdef");
  assert_eq!(str_of(&stream.read_n_bytes(2)), "ab");
  assert_eq!(stream.read_n_bytes(0), Vec::<u8>::new());
  assert_eq!(str_of(&stream.read_n_bytes(usize::MAX)), "cdef");
  assert_eq!(stream.read_n_bytes(1), Vec::<u8>::new());
  drop(stream);
}

// "support readNBytes(byte[], int, int)"
#[test]
fn support_read_n_bytes_byte_array_int_int() {
  let mut stream = stream_of("abcdef");
  let mut buf = [0_u8; 4];
  assert_eq!(stream.read_n_bytes_into(&mut buf, 1, 3), Ok(3));
  assert_eq!(str_of(&buf[1..4]), "abc");
  assert_eq!(stream.read_n_bytes_into(&mut buf, 0, 4), Ok(3));
  assert_eq!(str_of(&buf[0..3]), "def");
  // 0 at the end of the stream, not -1.
  assert_eq!(stream.read_n_bytes_into(&mut buf, 0, 4), Ok(0));
  assert_eq!(stream.read_n_bytes_into(&mut buf, 2, 3), Err(IndexOutOfBoundsError));
  drop(stream);
}

// "support transferTo"
#[test]
fn support_transfer_to() {
  let mut stream = stream_of("abcdef");
  assert_eq!(stream.read(), Some(b'a'));
  let mut out = Vec::new();
  assert_eq!(stream.transfer_to(&mut out), 5);
  assert_eq!(str_of(&out), "bcdef");
  assert_eq!(stream.transfer_to(&mut out), 0);
  assert_eq!(out.len(), 5);
  drop(stream);
}

// "support offset and length constructor"
#[test]
fn support_offset_and_length_constructor() {
  let mut stream = ByteString::from_array(b"abcdef", 1, 3).as_input_stream();
  assert_eq!(stream.available(), 3);
  assert_eq!(stream.read(), Some(b'b'));
  stream.mark(0);
  assert_eq!(str_of(&stream.read_all_bytes()), "cd");
  assert_eq!(stream.read(), None);
  stream.reset().expect("a stream over one array supports reset");
  assert_eq!(str_of(&stream.read_n_bytes(10)), "cd");
  drop(stream);
}

// "clamp offset and length to the array bounds"
//
// The two constructions with a negative offset or length, which throw
// `IllegalArgumentException`, are not ported: a `usize` cannot be negative.
// `Int.MaxValue` is `usize::MAX`.
#[test]
fn clamp_offset_and_length_to_the_array_bounds() {
  let mut over_length = ByteString::from_array(b"abc", 1, usize::MAX).as_input_stream();
  assert_eq!(over_length.available(), 2);
  assert_eq!(str_of(&over_length.read_all_bytes()), "bc");
  drop(over_length);

  let mut over_offset = ByteString::from_array(b"abc", 10, 2).as_input_stream();
  assert_eq!(over_offset.available(), 0);
  assert_eq!(over_offset.read(), None);
  assert_eq!(over_offset.read_all_bytes(), Vec::<u8>::new());
  drop(over_offset);

  let mut empty = ByteString::from_array(&[], 5, 5).as_input_stream();
  assert_eq!(empty.available(), 0);
  assert_eq!(empty.read(), None);
  drop(empty);
}

// "behave like java.io.ByteArrayInputStream under randomized operation sequences"
//
// The original runs every operation against this stream and a
// `java.io.ByteArrayInputStream` built the same way, and compares return
// values, exceptions, buffer contents and `available()`. The port keeps the
// same seed and draws: `JavaRandom` reproduces `java.util.Random`, which
// `scala.util.Random` wraps, so iteration `i`, step `s` sees the same data and
// arguments as the original. `ByteArrayInputStreamModel` is the reference: the
// JDK's `ByteArrayInputStream` behaviour for the operations the case calls.
//
// Branches whose arguments a `usize` cannot hold are skipped after their draws
// are taken, so the later draws stay aligned. None of them changes either
// stream in the original (both throw, or both do nothing, before moving), so
// skipping them keeps the two streams in the same state:
//   - op 2 and op 5 with `off == -1` or `len == -1` (both throw
//     `IndexOutOfBoundsException`);
//   - op 4 with `len == -1` (both throw `IllegalArgumentException`);
//   - op 7 with `n > 0`, the negative skip (the reference returns 0 and the
//     original stream throws `IllegalArgumentException`: the documented
//     deviation);
//   - op 8 with `n == -1` (`skipNBytes` of a negative count does nothing in
//     both).
// Out-of-range but non-negative arguments, zero lengths, reads at the end of
// the stream, mark and reset are all kept.
#[test]
fn behave_like_java_io_byte_array_input_stream_under_randomized_operation_sequences() {
  let mut rnd = JavaRandom::new(20260922);
  for iteration in 1..=500 {
    let mut data = vec![0_u8; rnd.next_int(48) as usize];
    rnd.next_bytes(&mut data);
    let (mut stream, mut reference) = if rnd.next_boolean() {
      (ByteString::from_array(&data, 0, data.len()).as_input_stream(), ByteArrayInputStreamModel::new(&data, 0, data.len()))
    } else {
      let off = rnd.next_int(data.len() as i32 + 1) as usize;
      let len = rnd.next_int((data.len() - off) as i32 + 1) as usize;
      (ByteString::from_array(&data, off, len).as_input_stream(), ByteArrayInputStreamModel::new(&data, off, len))
    };
    for step in 1..=40 {
      let op = rnd.next_int(13);
      let clue = format!("iteration {iteration}, step {step}, op {op}: ");
      match op {
        | 0 => assert_eq!(stream.read(), reference.read(), "{clue}"),
        | 1 => {
          let mut a = vec![0_u8; rnd.next_int(6) as usize];
          let mut b = a.clone();
          let whole = b.len();
          let expected = reference.read_range(&mut b, 0, whole).expect("in bounds");
          let actual = stream.read_into(&mut a);
          if a.is_empty() && expected.is_none() {
            // The documented deviation: 0 here, -1 in ByteArrayInputStream.
            assert_eq!(actual, Some(0), "{clue}");
          } else {
            assert_eq!(actual, expected, "{clue}");
          }
          assert_eq!(a, b, "{clue}");
        },
        | 2 => {
          let mut a = vec![0_u8; rnd.next_int(8) as usize];
          let mut b = a.clone();
          let off = rnd.next_int(a.len() as i32 + 2) - 1;
          let len = rnd.next_int(a.len() as i32 + 2) - 1;
          if off >= 0 && len >= 0 {
            let (off, len) = (off as usize, len as usize);
            let expected = reference.read_range(&mut b, off, len);
            let actual = stream.read_range(&mut a, off, len);
            if len == 0 && expected == Ok(None) {
              // The documented deviation: 0 here, -1 in ByteArrayInputStream.
              assert_eq!(actual, Ok(Some(0)), "{clue}");
            } else {
              assert_eq!(actual, expected, "{clue}");
            }
          }
          assert_eq!(a, b, "{clue}");
        },
        | 3 => assert_eq!(stream.read_all_bytes(), reference.read_all_bytes(), "{clue}"),
        | 4 => {
          let len = rnd.next_int(12) - 1;
          if len >= 0 {
            assert_eq!(stream.read_n_bytes(len as usize), reference.read_n_bytes(len as usize), "{clue}");
          }
        },
        | 5 => {
          let mut a = vec![0_u8; rnd.next_int(8) as usize];
          let mut b = a.clone();
          let off = rnd.next_int(a.len() as i32 + 2) - 1;
          let len = rnd.next_int(a.len() as i32 + 2) - 1;
          if off >= 0 && len >= 0 {
            let (off, len) = (off as usize, len as usize);
            assert_eq!(stream.read_n_bytes_into(&mut a, off, len), reference.read_n_bytes_into(&mut b, off, len), "{clue}");
          }
          assert_eq!(a, b, "{clue}");
        },
        | 6 => {
          let n = rnd.next_int(12) as usize;
          assert_eq!(stream.skip(n), reference.skip(n), "{clue}");
        },
        | 7 => {
          let n = rnd.next_int(6);
          // `reference.skip(-n) should ===(0L)` and the stream's negative skip
          // throws; only `n == 0` can be expressed.
          if n == 0 {
            assert_eq!(reference.skip(0), 0, "{clue}");
            assert_eq!(stream.skip(0), 0, "{clue}");
          }
        },
        | 8 => {
          let n = rnd.next_int(12) - 1;
          if n >= 0 {
            assert_eq!(stream.skip_n_bytes(n as usize), reference.skip_n_bytes(n as usize), "{clue}");
          }
        },
        | 9 => {
          let mut a = Vec::new();
          let mut b = Vec::new();
          assert_eq!(stream.transfer_to(&mut a), reference.transfer_to(&mut b), "{clue}");
          assert_eq!(a, b, "{clue}");
        },
        | 10 => assert_eq!(stream.available(), reference.available(), "{clue}"),
        | 11 => {
          stream.mark(rnd.next_int(4) as usize);
          reference.mark(rnd.next_int(4) as usize);
        },
        | 12 => {
          stream.reset().expect("a stream over one array supports reset");
          reference.reset();
        },
        | _ => unreachable!("op is drawn below 13"),
      }
      assert_eq!(stream.available(), reference.available(), "{clue}");
    }
  }
}

// `java.util.Random`, for the draws the randomized case takes.
struct JavaRandom {
  seed: u64,
}

impl JavaRandom {
  const ADDEND: u64 = 0xB;
  const MASK: u64 = (1 << 48) - 1;
  const MULTIPLIER: u64 = 0x5_DEEC_E66D;

  fn new(seed: i64) -> Self {
    Self { seed: (seed as u64 ^ Self::MULTIPLIER) & Self::MASK }
  }

  fn next(&mut self, bits: u32) -> i32 {
    self.seed = (self.seed.wrapping_mul(Self::MULTIPLIER).wrapping_add(Self::ADDEND)) & Self::MASK;
    (self.seed >> (48 - bits)) as i64 as i32
  }

  // `nextInt(bound)` for a positive `bound`.
  fn next_int(&mut self, bound: i32) -> i32 {
    assert!(bound > 0);
    if bound & -bound == bound {
      return ((i64::from(bound) * i64::from(self.next(31))) >> 31) as i32;
    }
    loop {
      let bits = self.next(31);
      let value = bits % bound;
      if bits.wrapping_sub(value).wrapping_add(bound - 1) >= 0 {
        return value;
      }
    }
  }

  fn next_boolean(&mut self) -> bool {
    self.next(1) != 0
  }

  fn next_bytes(&mut self, bytes: &mut [u8]) {
    let mut i = 0;
    while i < bytes.len() {
      let mut rnd = self.next(32);
      let mut n = (bytes.len() - i).min(4);
      while n > 0 {
        bytes[i] = rnd as u8;
        rnd >>= 8;
        i += 1;
        n -= 1;
      }
    }
  }
}

// The JDK's `java.io.ByteArrayInputStream`, for the operations the randomized
// case calls (JDK 17 and later; `skipNBytes` is `InputStream`'s default).
// `None` is Java's `-1`.
struct ByteArrayInputStreamModel {
  buf: Vec<u8>,
  pos: usize,
  count: usize,
  mark: usize,
}

impl ByteArrayInputStreamModel {
  fn new(buf: &[u8], offset: usize, length: usize) -> Self {
    Self { buf: buf.to_vec(), pos: offset, count: (offset + length).min(buf.len()), mark: offset }
  }

  fn read(&mut self) -> Option<u8> {
    if self.pos < self.count {
      self.pos += 1;
      Some(self.buf[self.pos - 1])
    } else {
      None
    }
  }

  fn read_range(&mut self, b: &mut [u8], off: usize, len: usize) -> Result<Option<usize>, IndexOutOfBoundsError> {
    if off > b.len() || len > b.len() - off {
      return Err(IndexOutOfBoundsError);
    }
    if self.pos >= self.count {
      return Ok(None);
    }
    let len = len.min(self.count - self.pos);
    b[off..off + len].copy_from_slice(&self.buf[self.pos..self.pos + len]);
    self.pos += len;
    Ok(Some(len))
  }

  fn read_all_bytes(&mut self) -> Vec<u8> {
    self.read_n_bytes(usize::MAX)
  }

  fn read_n_bytes(&mut self, len: usize) -> Vec<u8> {
    let len = len.min(self.count - self.pos);
    let result = self.buf[self.pos..self.pos + len].to_vec();
    self.pos += len;
    result
  }

  fn read_n_bytes_into(&mut self, b: &mut [u8], off: usize, len: usize) -> Result<usize, IndexOutOfBoundsError> {
    Ok(self.read_range(b, off, len)?.unwrap_or(0))
  }

  fn skip(&mut self, n: usize) -> usize {
    let k = n.min(self.count - self.pos);
    self.pos += k;
    k
  }

  // `InputStream.skipNBytes`: skip, then read a byte at a time to find the end.
  fn skip_n_bytes(&mut self, mut n: usize) -> Result<(), EndOfStreamError> {
    while n > 0 {
      let skipped = self.skip(n);
      if skipped > 0 {
        n -= skipped;
      } else if self.read().is_none() {
        return Err(EndOfStreamError);
      } else {
        n -= 1;
      }
    }
    Ok(())
  }

  fn transfer_to(&mut self, out: &mut Vec<u8>) -> usize {
    let rest = self.read_all_bytes();
    out.extend_from_slice(&rest);
    rest.len()
  }

  fn available(&self) -> usize {
    self.count - self.pos
  }

  fn mark(&mut self, _read_ahead_limit: usize) {
    self.mark = self.pos;
  }

  fn reset(&mut self) {
    self.pos = self.mark;
  }
}
