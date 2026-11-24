//! A parser for `resolv.conf` lines.

use alloc::{
  string::{String, ToString},
  vec,
  vec::Vec,
};

use super::{ResolvConf, ResolvConfParseError};

#[cfg(test)]
#[path = "resolv_conf_parser_test.rs"]
mod tests;

// The initial `ndots` before any `options` line sets it.
const DEFAULT_NDOTS: i32 = 1;

/// Parses `resolv.conf` lines into a [`ResolvConf`].
///
/// Mirrors `org.apache.pekko.io.dns.internal.ResolvConfParser.parseLines`: it
/// reads `domain`, `search` and `options` lines in order and ignores the rest.
pub struct ResolvConfParser;

impl ResolvConfParser {
  /// Parses the given lines, from the first to the last.
  ///
  /// Each line is trimmed of its `U+0000`..`U+0020` characters, and blank or
  /// comment lines (starting with `#` or `;`) are skipped. The line is split at
  /// the first character Java's `Character.isWhitespace` accepts; the word before
  /// it selects the rule:
  ///
  /// - `domain` replaces the search list with its whole trimmed argument run.
  /// - `search` replaces the search list with its trimmed arguments split on
  ///   ASCII whitespace; an empty run gives one empty element.
  /// - `options` splits its untrimmed arguments on ASCII whitespace and reads the
  ///   `i32` after each `ndots:`, the last one winning.
  ///
  /// Returns [`ResolvConfParseError::InvalidNdots`] when a `ndots:` value is not
  /// an `i32`.
  pub fn parse_lines<'a, I: IntoIterator<Item = &'a str>>(lines: I) -> Result<ResolvConf, ResolvConfParseError> {
    let mut search = Vec::new();
    let mut ndots = DEFAULT_NDOTS;
    for line in lines {
      let trimmed = java_trim(line);
      if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with(';') {
        continue;
      }
      let (label, args) = split_label(trimmed);
      match label {
        | "domain" => search = vec![java_trim(args).to_string()],
        | "search" => search = split_search_arguments(java_trim(args)),
        | "options" => {
          for option in split_ascii_whitespace(args) {
            if let Some(value) = option.strip_prefix("ndots:") {
              ndots = value.parse::<i32>().map_err(|_| ResolvConfParseError::InvalidNdots)?;
            }
          }
        },
        | _ => {},
      }
    }
    Ok(ResolvConf::new(search, ndots))
  }
}

// Java's `String.trim`: strip the `U+0000`..`U+0020` characters at both ends.
fn java_trim(text: &str) -> &str {
  text.trim_matches(|character: char| character <= ' ')
}

// Splits at the first Java-whitespace character; `args` keeps the rest as is.
fn split_label(line: &str) -> (&str, &str) {
  match line.find(is_java_whitespace) {
    | Some(index) => line.split_at(index),
    | None => (line, ""),
  }
}

// Java's `Character.isWhitespace`: `\t`, `\n`, `U+000B`, `U+000C`, `\r`,
// `U+001C`..=`U+001F`, and the Unicode spaces that `char::is_whitespace` also
// accepts except `U+00A0`, `U+2007`, `U+202F` and `U+0085`.
fn is_java_whitespace(character: char) -> bool {
  match character {
    | '\t' | '\n' | '\u{0B}' | '\u{0C}' | '\r' | '\u{1C}'..='\u{1F}' => true,
    | '\u{00A0}' | '\u{2007}' | '\u{202F}' | '\u{0085}' => false,
    | other => other.is_whitespace(),
  }
}

// Java's `split("\\s+")` over ASCII `\s`: split on ` `, `\t`, `\n`, `U+000B`,
// `U+000C`, `\r`, dropping the empty elements a run of separators leaves.
fn split_ascii_whitespace(text: &str) -> impl Iterator<Item = &str> {
  text.split([' ', '\t', '\n', '\u{0B}', '\u{0C}', '\r']).filter(|part| !part.is_empty())
}

// Java's `trimmed.split("\\s+")` for a `search` line: as above, except that the
// empty string yields one empty element, matching `"".split(...)` in Java.
fn split_search_arguments(trimmed: &str) -> Vec<String> {
  if trimmed.is_empty() {
    return vec![String::new()];
  }
  split_ascii_whitespace(trimmed).map(ToString::to_string).collect()
}
