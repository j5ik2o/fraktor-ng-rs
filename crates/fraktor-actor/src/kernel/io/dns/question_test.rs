// SPDX-License-Identifier: Apache-2.0
//
// Licensed to the Apache Software Foundation (ASF) under one or more
// license agreements; and to You under the Apache License, version 2.0:
//
//   https://www.apache.org/licenses/LICENSE-2.0
//
// This file is part of the Apache Pekko project, which was derived from Akka.
//
// Copyright (C) 2018-2022 Lightbend Inc. <https://www.lightbend.com>
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.

//! Unit tests for [`Question`](super::Question).

use alloc::vec::Vec;

use super::Question;
use crate::kernel::io::dns::{RecordClass, RecordType, WireFormatError, WireReader};

fn question_bytes(record_type: u16, record_class: u16) -> Vec<u8> {
  let mut bytes = Vec::from([3, b'w', b'w', b'w', 0]);
  bytes.extend_from_slice(&record_type.to_be_bytes());
  bytes.extend_from_slice(&record_class.to_be_bytes());
  bytes
}

#[test]
fn a_question_is_a_name_a_type_and_a_class() {
  let bytes = question_bytes(1, 1);
  let mut reader = WireReader::new(&bytes);
  let question = Question::parse(&mut reader).unwrap();
  assert_eq!(question.name().as_str(), "www");
  assert_eq!(question.record_type(), RecordType::A);
  assert_eq!(question.record_class(), RecordClass::IN);
  assert_eq!(reader.position(), 9);
}

#[test]
fn an_unknown_record_type_fails_the_question() {
  assert_eq!(
    Question::parse(&mut WireReader::new(&question_bytes(17, 1))),
    Err(WireFormatError::UnknownRecordType(17))
  );
}

#[test]
fn an_unknown_record_class_fails_the_question() {
  assert_eq!(Question::parse(&mut WireReader::new(&question_bytes(1, 4))), Err(WireFormatError::UnknownRecordClass(4)));
}

#[test]
fn a_question_cut_after_its_name_is_truncated() {
  assert_eq!(Question::parse(&mut WireReader::new(&[3, b'w', b'w', b'w', 0, 0])), Err(WireFormatError::Truncated));
}
