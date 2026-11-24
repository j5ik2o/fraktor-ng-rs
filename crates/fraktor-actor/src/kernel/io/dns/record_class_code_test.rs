//! Unit tests for [`RecordClassCode`](super::RecordClassCode).

use super::RecordClassCode;
use crate::kernel::io::dns::RecordClass;

#[test]
fn a_code_keeps_its_value() {
  assert_eq!(RecordClassCode::new(4).value(), 4);
}

#[test]
fn a_record_class_converts_to_its_code() {
  assert_eq!(RecordClassCode::from(RecordClass::IN), RecordClassCode::new(1));
}
