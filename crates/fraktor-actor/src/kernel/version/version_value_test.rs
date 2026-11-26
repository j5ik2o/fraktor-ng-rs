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

//! Unit tests for [`Version`](super::Version).

use std::{
  collections::hash_map::DefaultHasher,
  hash::{Hash, Hasher},
};

use super::Version;
use crate::kernel::version::VersionParseError;

fn version(text: &str) -> Version {
  Version::parse(text).expect("the input parses")
}

fn hash_text(text: &str) -> u64 {
  let mut hasher = DefaultHasher::new();
  version(text).hash(&mut hasher);
  hasher.finish()
}

// Splitting mirrors Java's `String.split`, which drops a trailing empty
// component, so both spellings parse to the same numbers and qualifier.
#[test]
fn drops_trailing_empty_components_when_splitting() {
  assert_eq!(version("1.2."), version("1.2"));
  assert_eq!(version("1.2.3.."), version("1.2.3"));
  assert_eq!(Version::parse("."), Err(VersionParseError::UnsupportedSegmentCount));
}

#[test]
fn classifies_empty_short_and_overlong_inputs() {
  assert_eq!(Version::parse(""), Err(VersionParseError::Empty));
  assert_eq!(Version::parse(".1"), Err(VersionParseError::InvalidNumber));
  assert_eq!(Version::parse("1..2"), Err(VersionParseError::InvalidNumber));
  assert_eq!(Version::parse("1.2.3.4"), Err(VersionParseError::UnsupportedSegmentCount));
}

// The qualifier split takes the first `-` or `+`; a trimmed sign leaves an
// empty qualifier, so `1.2-` equals `1.2`.
#[test]
fn splits_qualifier_on_the_first_sign() {
  assert_eq!(version("1.2-"), version("1.2"));
  assert_eq!(Version::parse("1.-M1"), Err(VersionParseError::InvalidNumber));
  assert_eq!(version("+1.2"), version("1.2"));
  assert!(version("-1.2") < version("0.2"));
}

#[test]
fn rejects_numbers_that_do_not_fit_in_i32() {
  assert_eq!(Version::parse("2147483648.0"), Err(VersionParseError::InvalidNumber));
}

// A single component whose number overflows stays a qualifier; the parse
// succeeds because the numeric branch is only taken when the number reads.
#[test]
fn keeps_an_out_of_range_single_component_as_a_qualifier() {
  let parsed = version("2147483648");
  assert_eq!(parsed, version("2147483648"));
  assert_ne!(parsed, version("1"));
}

// The dynver number is read without failing the parse; when it does not fit,
// the remainder stays with the qualifier instead of becoming `InvalidNumber`.
#[test]
fn keeps_an_unreadable_dynver_number_in_the_qualifier() {
  let parsed = version("1.2+3000000000-1234abcd");
  assert_eq!(parsed, version("1.2+3000000000-1234abcd"));
  assert_ne!(parsed, version("1.2"));
  assert_ne!(parsed, version("1.2+0-abc"));
  assert_ne!(parsed, version("1.2+3000000000-xyz"));
  assert_ne!(parsed, version("1.2+1234abcd"));
}

#[test]
fn displays_and_returns_the_original_text() {
  assert_eq!("1.2", version("1.2").to_string());
  assert_eq!("1.2.0", version("1.2.0").to_string());
  assert_eq!("1.2", version("1.2").as_str());
  assert_eq!("1.2.0", version("1.2.0").as_str());
  assert_eq!("foo", version("foo").as_str());
}

// `compareTo` in the original leaves out the `else`, so `1.2.0` would report
// less than `1.2-M1` in one direction. The port keeps `cmp` antisymmetric, so
// both directions agree that the unqualified release is the greater one.
#[test]
fn orders_an_unqualified_release_above_the_same_number_with_a_qualifier() {
  assert!(version("1.2.0") > version("1.2-M1"));
  assert!(version("1.2-M1") < version("1.2.0"));
  assert_eq!(version("1.2.0").cmp(&version("1.2-M1")), core::cmp::Ordering::Greater);
  assert_eq!(version("1.2-M1").cmp(&version("1.2.0")), core::cmp::Ordering::Less);
}

#[test]
fn orders_qualifierless_versions_between_qualified_ones() {
  assert!(version("1.2-M1") < version("1.2.0"));
  assert!(version("1.2.0") < version("1.2.1-M1"));
  assert!(version("bar") < version("foo"));
  assert!(version("foo") < version("1.2.3"));
}

// The original computes `i1 - i2`, which overflows for the i32 extremes. The
// port compares the numbers directly, so the order stays correct.
#[test]
fn orders_i32_extremes_without_overflow() {
  assert!(version("2147483647") > version("-1.0"));
  assert!(version("-1.0") < version("2147483647"));
}

// `PartialEq`, `Ord`, and `Hash` are derived from the same numbers and
// qualifier, so equality, ordering, and hashing never disagree.
#[test]
fn keeps_equality_ordering_and_hashing_in_agreement() {
  const INPUTS: &[&str] = &[
    "1",
    "1.0",
    "1.0.0",
    "1.2.3",
    "1.2.3-M1",
    "1.2.3-M2",
    "1.2-M1",
    "1.2.0-M1",
    "1.0.10+3-1234abcd",
    "1.0.10+10-1234abcd",
    "1.0.10+3a-1234abcd",
    "1.2+3-1234abcd",
    "1.2+10-1234abcd",
    "HEAD+20140707-1030",
    "HEAD+20140707-1130",
    "foo",
    "bar",
    "1-foo",
    "01-foo",
    "1.2.",
    "1.2.3..",
    "+1.2",
    "-1.2",
    "2147483648",
    "1.0.0+99999999999-x",
  ];

  let versions: Vec<(&str, Version)> = INPUTS.iter().map(|text| (*text, version(text))).collect();
  for (left_text, left) in &versions {
    for (right_text, right) in &versions {
      let equal = left == right;
      assert_eq!(equal, left.cmp(right) == core::cmp::Ordering::Equal, "{left_text} vs {right_text}");
      assert_eq!(left.cmp(right), right.cmp(left).reverse(), "{left_text} vs {right_text} is not antisymmetric");
      if equal {
        assert_eq!(hash_text(left_text), hash_text(right_text), "{left_text} and {right_text} hash differently");
      }
    }
  }
}
