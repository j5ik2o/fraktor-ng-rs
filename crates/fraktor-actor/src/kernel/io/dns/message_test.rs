// SPDX-License-Identifier: Apache-2.0
//
// Licensed to the Apache Software Foundation (ASF) under one or more
// license agreements; and to You under the Apache License, version 2.0:
//
//   https://www.apache.org/licenses/LICENSE-2.0
//
// This file is part of the Apache Pekko project, which was derived from Akka.
//
// Copyright (C) 2009-2022 Lightbend Inc. <https://www.lightbend.com>
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.

//! Unit tests for [`Message`](super::Message).

use alloc::vec::Vec;
use core::time::Duration;

use super::Message;
use crate::kernel::io::dns::{RecordType, WireFormatError};

fn header(flags: u16, counts: [u16; 4]) -> Vec<u8> {
  let mut bytes = Vec::from(7_u16.to_be_bytes());
  bytes.extend_from_slice(&flags.to_be_bytes());
  for count in counts {
    bytes.extend_from_slice(&count.to_be_bytes());
  }
  bytes
}

fn question(record_class: u16) -> Vec<u8> {
  let mut bytes = Vec::from([1, b'a', 0, 0, 1]);
  bytes.extend_from_slice(&record_class.to_be_bytes());
  bytes
}

fn a_record() -> Vec<u8> {
  // name: a pointer to the question's name at offset 12
  Vec::from([0xC0, 12, 0, 1, 0, 1, 0, 0, 0, 60, 0, 4, 192, 0, 2, 1])
}

const ANSWER: u16 = 0x8180;
const TRUNCATED_ANSWER: u16 = 0x8380;

#[test]
fn a_complete_response_keeps_every_section() {
  let bytes = [header(ANSWER, [1, 1, 1, 1]), question(1), a_record(), a_record(), a_record()].concat();
  let message = Message::parse(&bytes).unwrap();
  assert_eq!(message.id(), 7);
  assert!(!message.flags().is_truncated());
  assert_eq!(message.questions().len(), 1);
  assert_eq!(message.questions()[0].record_type(), RecordType::A);
  assert_eq!(message.answer_records().len(), 1);
  assert_eq!(message.authority_records().len(), 1);
  assert_eq!(message.additional_records().len(), 1);
  let record = &message.answer_records()[0];
  assert_eq!(record.name().as_str(), "a");
  assert_eq!(record.ttl(), Duration::from_secs(60));
  assert_eq!(record.data(), [192, 0, 2, 1]);
}

#[test]
fn a_response_without_the_tc_bit_fails_on_a_missing_record() {
  let bytes = [header(ANSWER, [1, 2, 0, 0]), question(1), a_record()].concat();
  assert_eq!(Message::parse(&bytes), Err(WireFormatError::Truncated));
}

#[test]
fn a_response_with_the_tc_bit_leaves_out_the_missing_records() {
  let bytes = [header(TRUNCATED_ANSWER, [1, 2, 3, 4]), question(1), a_record()].concat();
  let message = Message::parse(&bytes).unwrap();
  assert_eq!(message.questions().len(), 1);
  assert_eq!(message.answer_records().len(), 1);
  assert!(message.authority_records().is_empty());
  assert!(message.additional_records().is_empty());
}

// With the TC bit, a question that cannot be read is left out and the next
// one is read from where the failed one stopped, as in the original.
#[test]
fn a_response_with_the_tc_bit_reads_on_after_a_rejected_question() {
  let bytes = [header(TRUNCATED_ANSWER, [2, 0, 0, 0]), question(4), question(1)].concat();
  let message = Message::parse(&bytes).unwrap();
  assert_eq!(message.questions().len(), 1);
  assert_eq!(message.questions()[0].name().as_str(), "a");
}

#[test]
fn a_response_without_the_tc_bit_fails_on_a_rejected_question() {
  let bytes = [header(ANSWER, [2, 0, 0, 0]), question(4), question(1)].concat();
  assert_eq!(Message::parse(&bytes), Err(WireFormatError::UnknownRecordClass(4)));
}

#[test]
fn a_message_cut_inside_its_header_is_truncated() {
  assert_eq!(Message::parse(&[0, 4, 0x83, 0x80, 0, 1]), Err(WireFormatError::Truncated));
}

// The original's counts are `Short`s; from `0x8000` up they are negative and
// `0 until count` reads nothing.
#[test]
fn a_section_count_from_0x8000_up_reads_no_entries() {
  let bytes = header(ANSWER, [0x8000, 0xFFFF, 0, 0]);
  let message = Message::parse(&bytes).unwrap();
  assert!(message.questions().is_empty());
  assert!(message.answer_records().is_empty());
}
