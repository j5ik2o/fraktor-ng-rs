//! Unit tests for [`RecordTypeCode`](super::RecordTypeCode).

use super::RecordTypeCode;
use crate::kernel::io::dns::RecordType;

#[test]
fn a_code_keeps_its_value() {
  assert_eq!(RecordTypeCode::new(99).value(), 99);
}

#[test]
fn a_registered_code_has_its_record_type() {
  assert_eq!(RecordTypeCode::new(28).record_type(), Some(RecordType::AAAA));
}

#[test]
fn an_unregistered_code_has_no_record_type() {
  assert_eq!(RecordTypeCode::new(99).record_type(), None);
}

#[test]
fn a_record_type_converts_to_its_code() {
  assert_eq!(RecordTypeCode::from(RecordType::SRV), RecordTypeCode::new(33));
}
