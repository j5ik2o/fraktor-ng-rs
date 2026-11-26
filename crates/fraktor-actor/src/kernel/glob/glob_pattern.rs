// SPDX-License-Identifier: Apache-2.0
//
// Licensed to the Apache Software Foundation (ASF) under one or more
// contributor license agreements. See the NOTICE file distributed with
// this work for additional information regarding copyright ownership.
// The ASF licenses this file to You under the Apache License, Version 2.0
// (the "License"); you may not use this file except in compliance with
// the License. You may obtain a copy of the License at
//
//    http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.

//! A glob pattern: `?` matches exactly one character, `*` matches any run
//! of characters, and every other character is literal.

use alloc::vec::Vec;

/// A glob pattern, compiled once from its text.
///
/// Mirrors `org.apache.pekko.util.Glob`, which matches without going
/// through a regular expression: a chain of `*` in a regular expression
/// backtracks exponentially on inputs that cannot match, while this
/// matcher revisits a decision at most once per input position.
///
/// The port counts Unicode scalar values (`char`), where the original
/// counts UTF-16 code units with `charAt`. The two agree except for
/// characters outside the BMP, where the original's `?` can match half of
/// a surrogate pair and this port's `?` matches the whole character.
///
/// Like the original, the matcher checks literal equality before the `*`
/// branch, so an input `*` that meets a pattern `*` is consumed as a
/// literal character and no resume point is recorded for that `*`. A
/// pattern `*` therefore does not match an input that starts with `*`
/// through that position; this is the original's behavior, kept as is.
pub struct GlobPattern {
  pattern: Vec<char>,
}

impl GlobPattern {
  /// Builds a pattern from its text. Every string is a valid pattern, so
  /// this never fails.
  pub fn new(pattern: &str) -> Self {
    Self { pattern: pattern.chars().collect() }
  }

  /// Returns whether `input` matches this pattern as a whole.
  ///
  /// The scan keeps the resume position of the most recent `*` as a
  /// subslice of `input`, so matching allocates nothing and never
  /// collects the input.
  pub fn matches(&self, input: &str) -> bool {
    let pattern = &self.pattern;
    let mut p = 0usize;
    let mut rest = input;
    // Where to resume if the run consumed by the most recent `*` turns out
    // to be too short: the pattern index of the `*` and the remaining input
    // slice starting where the `*` resumes.
    let mut star: Option<(usize, &str)> = None;

    loop {
      let current = rest.chars().next();
      match current {
        | Some(c) => {
          if p < pattern.len() && (pattern[p] == '?' || pattern[p] == c) {
            p += 1;
            rest = &rest[c.len_utf8()..];
          } else if p < pattern.len() && pattern[p] == '*' {
            star = Some((p, rest));
            p += 1;
          } else if let Some((star_p, star_rest)) = star {
            // Let the most recent `*` consume one more character and try
            // again from there.
            let first = star_rest.chars().next().expect("the star resume point is within the input");
            let new_rest = &star_rest[first.len_utf8()..];
            star = Some((star_p, new_rest));
            rest = new_rest;
            p = star_p + 1;
          } else {
            return false;
          }
        },
        | None => break,
      }
    }

    // Trailing `*`s may match nothing; anything else left over means no
    // match.
    while p < pattern.len() && pattern[p] == '*' {
      p += 1;
    }
    p == pattern.len()
  }
}
