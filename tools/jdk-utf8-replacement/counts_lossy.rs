// SPDX-License-Identifier: Apache-2.0
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// #190: `String::from_utf8_lossy` on the same rows as `counts_decode.rs`, so
// tools/jdk-utf8-replacement/check_counts.py can report how many of the inputs
// the exception of the design's section 8.2 changes. See that file for the
// invocation.

use std::io::{self, BufRead, Write};

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
    let string = String::from_utf8_lossy(&bytes);
    writeln!(out, "{}", code_points(&string)).expect("a write");
  }
}
