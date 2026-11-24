//! Unit tests for [`RecordClass`](super::RecordClass).

use super::RecordClass;
use crate::kernel::io::dns::{WireFormatError, WireReader};

fn parse(bytes: &[u8]) -> Result<RecordClass, WireFormatError> {
  RecordClass::parse(&mut WireReader::new(bytes))
}

#[test]
fn the_classes_the_original_reads_are_parsed() {
  assert_eq!(parse(&[0, 1]), Ok(RecordClass::IN));
  assert_eq!(parse(&[0, 2]), Ok(RecordClass::CS));
  assert_eq!(parse(&[0, 3]), Ok(RecordClass::CH));
  assert_eq!(parse(&[0, 255]), Ok(RecordClass::WILDCARD));
}

// The original's `RecordClassSerializer.parse` has no case for `HS`.
#[test]
fn the_hesiod_code_is_rejected_as_the_original_rejects_it() {
  assert_eq!(RecordClass::HS.code(), 4);
  assert_eq!(parse(&[0, 4]), Err(WireFormatError::UnknownRecordClass(4)));
}

#[test]
fn an_unknown_code_is_rejected() {
  assert_eq!(parse(&[1, 0]), Err(WireFormatError::UnknownRecordClass(256)));
}

#[test]
fn a_record_class_has_its_code_and_name() {
  assert_eq!(RecordClass::IN.code(), 1);
  assert_eq!(RecordClass::IN.name(), "IN");
  assert_eq!(alloc::format!("{}", RecordClass::CH), "CH");
}
