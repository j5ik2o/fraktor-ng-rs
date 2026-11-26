// SPDX-License-Identifier: Apache-2.0
//
// Licensed to the Apache Software Foundation (ASF) under one or more
// license agreements; and to You under the Apache License, version 2.0:
//
//   https://www.apache.org/licenses/LICENSE-2.0
//
// This file is part of the Apache Pekko project, which was derived from Akka.
//
// Copyright (C) 2015-2022 Lightbend Inc. <https://www.lightbend.com>
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.

//! A parsed version compared by its numbers and qualifier.

use alloc::string::{String, ToString};
use core::{cmp::Ordering, fmt, hash::Hash};

use super::version_parse_error::VersionParseError;

#[cfg(test)]
#[path = "version_value_test.rs"]
mod tests;

/// A version parsed from text, compared by its numbers and qualifier.
///
/// Mirrors `org.apache.pekko.util.Version`. The original parses at
/// construction and throws; the port returns the outcome from [`Version::parse`].
/// Equality and ordering compare only the parsed numbers and qualifier, so
/// `1.2` and `1.2.0` are equal even though their text differs, while
/// [`Version::as_str`] and [`Display`](fmt::Display) keep the original text.
///
/// Differences from the original: the original's `compareTo` omits its final
/// `else`, so an unqualified version against a qualified one of the same
/// numbers compares inconsistently depending on the direction. This port keeps
/// ordering antisymmetric, so an unqualified release sorts above the qualified
/// version of the same numbers in both directions.
#[derive(Debug, Clone)]
pub struct Version {
  text: String,
  numbers: [i32; 4],
  qualifier: String,
}

impl Version {
  /// Parses `text` the way the original parser does.
  ///
  /// Splits on `.` the way Java's `String.split` does, dropping trailing empty
  /// components, then reads one, two, or three segments. A one-segment number
  /// that fits `i32` becomes the first number; otherwise it is the qualifier.
  /// The dynver number is read without failing the parse: when it does not fit,
  /// the remainder stays with the qualifier.
  pub fn parse(text: &str) -> Result<Self, VersionParseError> {
    let segments: Vec<&str> = if text.contains('.') {
      let mut segments: Vec<&str> = text.split('.').collect();
      while segments.last() == Some(&"") {
        segments.pop();
      }
      segments
    } else {
      vec![text]
    };
    let mut numbers = [0i32; 4];
    let mut qualifier = "";
    match segments.as_slice() {
      | [] => return Err(VersionParseError::UnsupportedSegmentCount),
      | [single] => {
        if single.is_empty() {
          return Err(VersionParseError::Empty);
        }
        if single.starts_with(|character: char| character.is_ascii_digit()) {
          match to_int(single) {
            | Some(number) => numbers[0] = number,
            | None => qualifier = single,
          }
        } else {
          qualifier = single;
        }
      },
      | [major, rest] => {
        let (last, dynver, remainder) = last_parts(rest)?;
        numbers[0] = to_int_checked(major)?;
        numbers[1] = last;
        numbers[2] = dynver;
        qualifier = remainder;
      },
      | [major, minor, rest] => {
        let (last, dynver, remainder) = last_parts(rest)?;
        numbers[0] = to_int_checked(major)?;
        numbers[1] = to_int_checked(minor)?;
        numbers[2] = last;
        numbers[3] = dynver;
        qualifier = remainder;
      },
      | _ => return Err(VersionParseError::UnsupportedSegmentCount),
    }
    Ok(Self { text: text.to_string(), numbers, qualifier: qualifier.to_string() })
  }

  /// Returns the original text this version was parsed from.
  pub fn as_str(&self) -> &str {
    &self.text
  }
}

fn to_int(text: &str) -> Option<i32> {
  text.parse::<i32>().ok()
}

fn to_int_checked(text: &str) -> Result<i32, VersionParseError> {
  to_int(text).ok_or(VersionParseError::InvalidNumber)
}

fn last_part(text: &str) -> Result<(i32, &str), VersionParseError> {
  if text.is_empty() {
    return Ok((0, ""));
  }
  match text.find(['-', '+']) {
    | None => Ok((to_int_checked(text)?, "")),
    | Some(position) => Ok((to_int_checked(&text[..position])?, &text[position + 1..])),
  }
}

fn dynver_part(text: &str) -> (i32, &str) {
  if text.is_empty() || !text.starts_with(|character: char| character.is_ascii_digit()) {
    return (0, text);
  }
  let Some(position) = text.find('-') else {
    return (0, text);
  };
  match to_int(&text[..position]) {
    | Some(number) => (number, &text[position + 1..]),
    | None => (0, text),
  }
}

fn last_parts(text: &str) -> Result<(i32, i32, &str), VersionParseError> {
  let (last, rest) = last_part(text)?;
  if rest.is_empty() {
    return Ok((last, 0, ""));
  }
  let (dynver, remainder) = dynver_part(rest);
  Ok((last, dynver, remainder))
}

impl PartialEq for Version {
  fn eq(&self, other: &Self) -> bool {
    self.numbers == other.numbers && self.qualifier == other.qualifier
  }
}

impl Eq for Version {}

impl PartialOrd for Version {
  fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
    Some(self.cmp(other))
  }
}

impl Ord for Version {
  fn cmp(&self, other: &Self) -> Ordering {
    self.numbers.cmp(&other.numbers).then_with(|| match (self.qualifier.is_empty(), other.qualifier.is_empty()) {
      | (true, true) => Ordering::Equal,
      | (true, false) => Ordering::Greater,
      | (false, true) => Ordering::Less,
      | (false, false) => self.qualifier.cmp(&other.qualifier),
    })
  }
}

impl Hash for Version {
  fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
    self.numbers.hash(state);
    self.qualifier.hash(state);
  }
}

impl fmt::Display for Version {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str(&self.text)
  }
}
