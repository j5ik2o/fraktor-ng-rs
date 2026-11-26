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
  /// The scan keeps the resume position of the most recent `*` as a byte
  /// offset into `input` (from `str::char_indices`), so matching allocates
  /// nothing and never collects the input.
  pub fn matches(&self, input: &str) -> bool {
    let pattern = &self.pattern;
    let mut p = 0usize;
    let mut rest = input;
    let mut consumed = 0usize;
    // Where to resume if the run consumed by the most recent `*` turns out
    // to be too short: the pattern index just after the `*`, and the byte
    // offset in `input` the `*` has consumed up to.
    let mut star: Option<(usize, usize, &str)> = None;

    loop {
      let current = rest.chars().next();
      match current {
        | Some(c) => {
          if p < pattern.len() && (pattern[p] == '?' || pattern[p] == c) {
            p += 1;
            consumed += c.len_utf8();
            rest = &rest[c.len_utf8()..];
          } else if p < pattern.len() && pattern[p] == '*' {
            star = Some((p, consumed, rest));
            p += 1;
          } else if let Some((star_p, star_consumed, star_rest)) = star {
            // Let the most recent `*` consume one more character and try
            // again from there.
            let first = star_rest.chars().next().expect("the star resume point is within the input");
            let new_consumed = star_consumed + first.len_utf8();
            let new_rest = &star_rest[first.len_utf8()..];
            star = Some((star_p, new_consumed, new_rest));
            consumed = new_consumed;
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
