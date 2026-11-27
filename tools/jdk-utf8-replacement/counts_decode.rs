// SPDX-License-Identifier: Apache-2.0
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// #190: `ByteString::decode_string` as the crate has it, for the count check of
// tools/jdk-utf8-replacement/check_counts.py. This file reads byte rows on
// stdin (`hex` per line, empty for no bytes) and writes the decoded code points
// (`U+XXXX` separated by spaces, `-` for none), one line per row:
//
//   rustc -O --edition 2021 -o /tmp/counts-decode tools/jdk-utf8-replacement/counts_decode.rs
//   ... | /tmp/counts-decode
//
// The function below is copied here, unchanged but for the indentation, so that
// the check builds nothing of the crate and a change to the crate shows up as a
// difference between the two copies. The file must never change the function:
// the copy is of the shipped code, and the crate is what runs in production.

use std::io::{self, BufRead, Write};

/// Decodes `bytes` as UTF-8, replacing malformed sequences with U+FFFD.
fn decode_string(bytes: &[u8]) -> String {
  // The JDK replaces a UTF-8 encoded surrogate as one malformed unit:
  // `0xED`, the byte after it in `0xA0..=0xBF`, and one more continuation
  // byte when one follows, become a single U+FFFD. Every other malformed
  // unit follows the maximal subpart rule, which `from_utf8_lossy` already
  // implements, so the bytes are decoded in segments around those units.
  // The bytes are flattened first, so a unit that spans two chunks is
  // treated the same as one inside a chunk.
  // The bytes of the value, as `self.to_vec()` flattens them.
  let mut out = String::new();
  let mut start = 0_usize;
  let mut index = 0_usize;
  while index < bytes.len() {
    if bytes[index] == 0xED && bytes.get(index + 1).is_some_and(|byte| (0xA0..=0xBF).contains(byte)) {
      out.push_str(&String::from_utf8_lossy(&bytes[start..index]));
      out.push('\u{FFFD}');
      index += 2;
      if bytes.get(index).is_some_and(|byte| (0x80..=0xBF).contains(byte)) {
        index += 1;
      }
      start = index;
    } else {
      index += 1;
    }
  }
  out.push_str(&String::from_utf8_lossy(&bytes[start..]));
  out
}

fn parse(line: &str) -> Vec<u8> {
  line.split_whitespace().map(|part| u8::from_str_radix(part, 16).expect("two hex digits")).collect()
}

fn code_points(text: &str) -> String {
  if text.is_empty() {
    return String::from("-");
  }
  text.chars().map(|c| format!("U+{:04X}", u32::from(c))).collect::<Vec<_>>().join(" ")
}

fn main() {
  let stdin = io::stdin();
  let stdout = io::stdout();
  let mut out = io::BufWriter::new(stdout.lock());
  for line in stdin.lock().lines() {
    let line = line.expect("a line");
    let bytes = parse(&line);
    let string = decode_string(&bytes);
    writeln!(out, "{}", code_points(&string)).expect("a write");
  }
}
