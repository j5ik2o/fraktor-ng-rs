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

//! Unit tests for [`WireReader`](super::WireReader).

use super::WireReader;
use crate::kernel::io::dns::WireFormatError;

#[test]
fn a_new_reader_starts_at_offset_zero() {
  assert_eq!(WireReader::new(&[1, 2]).position(), 0);
}

#[test]
fn a_start_past_the_end_is_clamped_to_the_end() {
  let mut reader = WireReader::starting_at(&[1, 2], 5);
  assert_eq!(reader.position(), 2);
  assert_eq!(reader.read_u8(), Err(WireFormatError::Truncated));
}

#[test]
fn a_u16_is_read_big_endian() {
  let mut reader = WireReader::new(&[0x83, 0x80, 0x01]);
  assert_eq!(reader.read_u16(), Ok(0x8380));
  assert_eq!(reader.position(), 2);
}

// Pekko's `getShort` consumes the one byte that is left before it throws.
#[test]
fn a_u16_cut_by_the_end_fails_and_leaves_the_reader_at_the_end() {
  let mut reader = WireReader::new(&[0x01]);
  assert_eq!(reader.read_u16(), Err(WireFormatError::Truncated));
  assert_eq!(reader.position(), 1);
}

#[test]
fn an_i32_is_read_big_endian_and_signed() {
  assert_eq!(WireReader::new(&[0, 0, 1, 0]).read_i32(), Ok(256));
  assert_eq!(WireReader::new(&[0xFF, 0xFF, 0xFF, 0xFF]).read_i32(), Ok(-1));
}

#[test]
fn reading_up_to_a_length_stops_at_the_end_of_the_message() {
  let mut reader = WireReader::new(&[1, 2, 3]);
  assert_eq!(reader.read_up_to(2), [1, 2]);
  assert_eq!(reader.read_up_to(5), [3]);
  assert_eq!(reader.position(), 3);
  assert!(reader.read_up_to(1).is_empty());
}
