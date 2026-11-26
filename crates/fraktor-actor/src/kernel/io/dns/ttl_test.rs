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

//! Unit tests for [`Ttl`](super::Ttl).

use core::time::Duration;

use super::{NonPositiveTtlError, Ttl};

// The original's `Ttl.fromPositive` rejects zero, so a zero `Duration` is the
// only value it can reject and must not pass through.
#[test]
fn a_zero_duration_is_rejected() {
  assert_eq!(Ttl::from_positive(Duration::ZERO), Err(NonPositiveTtlError));
}

// One nanosecond is the smallest positive `Duration`, so accepting it and
// returning the same value is the accepted side of the zero boundary.
#[test]
fn one_nanosecond_is_accepted_unchanged() {
  let ttl = Ttl::from_positive(Duration::from_nanos(1)).expect("a positive duration");
  assert_eq!(ttl.value(), Duration::from_nanos(1));
}

// A larger duration is kept as given, not rounded or clamped.
#[test]
fn a_larger_duration_is_accepted_unchanged() {
  let ttl = Ttl::from_positive(Duration::from_secs(10)).expect("a positive duration");
  assert_eq!(ttl.value(), Duration::from_secs(10));
}

#[test]
fn the_display_text_names_the_failure() {
  assert!(!NonPositiveTtlError.to_string().is_empty());
}
