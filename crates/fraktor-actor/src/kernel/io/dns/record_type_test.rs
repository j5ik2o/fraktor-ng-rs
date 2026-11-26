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

//! Unit tests for [`RecordType`](super::RecordType).

use super::RecordType;
use crate::kernel::io::dns::{WireFormatError, WireReader};

#[test]
fn a_registered_code_returns_its_record_type() {
  assert_eq!(RecordType::from_code(28), Some(RecordType::AAAA));
  assert_eq!(RecordType::from_code(1), Some(RecordType::A));
  assert_eq!(RecordType::from_code(255), Some(RecordType::WILDCARD));
}

#[test]
fn an_unregistered_code_has_no_record_type() {
  assert_eq!(RecordType::from_code(0), None);
  assert_eq!(RecordType::from_code(17), None);
  assert_eq!(RecordType::from_code(256), None);
}

#[test]
fn every_registered_type_is_found_by_its_own_code() {
  for record_type in RecordType::REGISTERED {
    assert_eq!(RecordType::from_code(record_type.code()), Some(record_type), "{record_type}");
  }
}

#[test]
fn a_record_type_has_its_code_and_name() {
  assert_eq!(RecordType::AAAA.code(), 28);
  assert_eq!(RecordType::AAAA.name(), "AAAA");
  assert_eq!(alloc::format!("{}", RecordType::SRV), "SRV");
}

#[test]
fn parsing_reads_a_big_endian_code() {
  let mut reader = WireReader::new(&[0, 28, 0xAB]);
  assert_eq!(RecordType::parse(&mut reader), Ok(RecordType::AAAA));
  assert_eq!(reader.position(), 2);
}

#[test]
fn parsing_an_unregistered_code_is_an_error() {
  assert_eq!(RecordType::parse(&mut WireReader::new(&[0, 17])), Err(WireFormatError::UnknownRecordType(17)));
}

#[test]
fn parsing_a_cut_code_is_truncated() {
  assert_eq!(RecordType::parse(&mut WireReader::new(&[0])), Err(WireFormatError::Truncated));
}
