// Ported from Pekko v1.7.1 (SHA 9a54d1294c3ac7821bbed3c940769c3d5027d1ff):
//   actor-tests/src/test/scala/org/apache/pekko/io/dns/internal/MessageSpec.scala
//
// The original's one case under `"The Message" should` is kept as one test,
// named after the case, with the original name as a comment. The input is the
// original's `ByteString(...)` of signed bytes, written here as the same
// `i8` values and converted to `u8` bit for bit: a response with id 4, flags
// `0x8380` (an answer with the truncation flag set), one question and 48
// answer records, cut off right after the question `many.bzzt.net` AAAA IN.
//
// The original's `Message.parse(bytes)` returns a `Message`; the port's
// `Message::parse(&bytes)` returns `Result<Message, WireFormatError>`, and the
// case unwraps it, since the original's parse does not throw for this input.
// The original asserts `msg.questions.head should be(Question("many.bzzt.net",
// RecordType.AAAA, RecordClass.IN))`; `Question` has exactly those three
// fields, so the port asserts each of them. The lengths of the answer,
// authority and additional sections are asserted as in the original.
//
// The port's types live at `fraktor_actor::kernel::io::dns`. Until they exist
// the file does not compile: that compile error is the intended red for this
// node.

use fraktor_actor::kernel::io::dns::{Message, RecordClass, RecordType};

// "parse a response that is truncated mid-message"
#[test]
fn parse_a_response_that_is_truncated_mid_message() {
  let signed: [i8; 31] = [
    0, 4, -125, -128, 0, 1, 0, 48, 0, 0, 0, 0, 4, 109, 97, 110, 121, 4, 98, 122, 122, 116, 3, 110, 101, 116, 0, 0, 28,
    0, 1,
  ];
  let bytes = signed.map(|byte| byte.to_ne_bytes()[0]);
  let msg = Message::parse(&bytes).unwrap();
  assert_eq!(msg.id(), 4);
  assert!(msg.flags().is_truncated());
  assert_eq!(msg.questions().len(), 1);
  let question = &msg.questions()[0];
  assert_eq!(question.name().as_str(), "many.bzzt.net");
  assert_eq!(question.record_type(), RecordType::AAAA);
  assert_eq!(question.record_class(), RecordClass::IN);
  assert_eq!(msg.answer_records().len(), 0);
  assert_eq!(msg.authority_records().len(), 0);
  assert_eq!(msg.additional_records().len(), 0);
}
