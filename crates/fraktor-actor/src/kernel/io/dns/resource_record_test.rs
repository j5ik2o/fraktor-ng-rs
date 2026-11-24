//! Unit tests for [`ResourceRecord`](super::ResourceRecord).

use alloc::vec::Vec;
use core::time::Duration;

use super::ResourceRecord;
use crate::kernel::io::dns::{RecordClass, RecordClassCode, RecordType, RecordTypeCode, WireFormatError, WireReader};

fn record_bytes(record_type: u16, ttl: i32, data_length: u16, data: &[u8]) -> Vec<u8> {
  let mut bytes = Vec::from([3, b'w', b'w', b'w', 0]);
  bytes.extend_from_slice(&record_type.to_be_bytes());
  bytes.extend_from_slice(&1_u16.to_be_bytes());
  bytes.extend_from_slice(&ttl.to_be_bytes());
  bytes.extend_from_slice(&data_length.to_be_bytes());
  bytes.extend_from_slice(data);
  bytes
}

#[test]
fn a_record_keeps_its_envelope_and_data() {
  let bytes = record_bytes(1, 300, 4, &[192, 0, 2, 1]);
  let mut reader = WireReader::new(&bytes);
  let record = ResourceRecord::parse(&mut reader).unwrap();
  assert_eq!(record.name().as_str(), "www");
  assert_eq!(record.record_type_code(), RecordTypeCode::from(RecordType::A));
  assert_eq!(record.record_class_code(), RecordClassCode::from(RecordClass::IN));
  assert_eq!(record.ttl(), Duration::from_secs(300));
  assert_eq!(record.data(), [192, 0, 2, 1]);
  assert_eq!(reader.position(), bytes.len());
}

#[test]
fn a_record_of_a_type_without_a_constant_is_kept() {
  let record = ResourceRecord::parse(&mut WireReader::new(&record_bytes(99, 1, 1, &[7]))).unwrap();
  assert_eq!(record.record_type_code(), RecordTypeCode::new(99));
  assert_eq!(record.record_type_code().record_type(), None);
  assert_eq!(record.data(), [7]);
}

// The original reads a zero time to live as `Ttl.never`.
#[test]
fn a_zero_time_to_live_is_zero() {
  let record = ResourceRecord::parse(&mut WireReader::new(&record_bytes(1, 0, 0, &[]))).unwrap();
  assert_eq!(record.ttl(), Duration::ZERO);
}

// The original's `Ttl.fromPositive` throws for a negative time to live.
#[test]
fn a_negative_time_to_live_is_rejected() {
  assert_eq!(
    ResourceRecord::parse(&mut WireReader::new(&record_bytes(1, -1, 0, &[]))),
    Err(WireFormatError::NegativeTtl(-1))
  );
}

// The original's `take` and `drop` stop at the end of the message.
#[test]
fn data_that_runs_past_the_end_is_cut_at_the_end() {
  let bytes = record_bytes(99, 1, 10, &[1, 2, 3]);
  let mut reader = WireReader::new(&bytes);
  let record = ResourceRecord::parse(&mut reader).unwrap();
  assert_eq!(record.data(), [1, 2, 3]);
  assert_eq!(reader.position(), bytes.len());
}

// The original's data length is a `Short`; from `0x8000` up it is negative,
// and `take` and `drop` then read nothing.
#[test]
fn a_data_length_from_0x8000_up_reads_no_data() {
  let bytes = record_bytes(99, 1, 0x8000, &[1, 2, 3]);
  let mut reader = WireReader::new(&bytes);
  let record = ResourceRecord::parse(&mut reader).unwrap();
  assert!(record.data().is_empty());
  assert_eq!(reader.position(), bytes.len() - 3);
}

#[test]
fn a_record_cut_inside_its_fixed_fields_is_truncated() {
  let bytes = record_bytes(1, 300, 4, &[]);
  assert_eq!(ResourceRecord::parse(&mut WireReader::new(&bytes[..12])), Err(WireFormatError::Truncated));
}
