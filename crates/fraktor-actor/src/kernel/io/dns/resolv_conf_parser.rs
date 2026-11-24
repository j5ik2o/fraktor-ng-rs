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
  /// an `i32`. The value is read as the original's `toInt`
  /// (`Integer.parseInt`) reads it: an optional ASCII `+` or `-`, then one or
  /// more decimal digits from any of the BMP's runs of Unicode decimal digits,
  /// such as the full-width `５` or the Arabic-Indic `٣`. Digits outside the
  /// BMP, other numbers such as `²`, and values past the `i32` range are
  /// errors.
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
              ndots = parse_java_int(value).ok_or(ResolvConfParseError::InvalidNdots)?;
            }
          }
        },
        | _ => {},
      }
    }
    Ok(ResolvConf::new(search, ndots))
  }
}

// The zero of each run of ten decimal digits that `Character.digit(ch, 10)`
// accepts. Measured over every code point on Temurin 21.0.13, 25.0.5 and
// 26.0.2.1, the JDK 11 and later set (Unicode 10.0 and later): all general
// category Nd runs in the BMP. JDK 8 (Unicode 6.2) lacks `U+0DE6` and `U+A9F0`.
const DECIMAL_DIGIT_ZEROS: [u32; 37] = [
  0x0030, 0x0660, 0x06F0, 0x07C0, 0x0966, 0x09E6, 0x0A66, 0x0AE6, 0x0B66, 0x0BE6, 0x0C66, 0x0CE6, 0x0D66, 0x0DE6,
  0x0E50, 0x0ED0, 0x0F20, 0x1040, 0x1090, 0x17E0, 0x1810, 0x1946, 0x19D0, 0x1A80, 0x1A90, 0x1B50, 0x1BB0, 0x1C40,
  0x1C50, 0xA620, 0xA8D0, 0xA900, 0xA9D0, 0xA9F0, 0xAA50, 0xABF0, 0xFF10,
];

// Java's `Integer.parseInt(text)`: an optional ASCII sign, then at least one
// decimal digit, within the `i32` range. Java reads UTF-16 units and never
// treats a surrogate as a digit, so a digit outside the BMP is rejected here
// because no zero in the table lies outside it. The value is built negative,
// as Java does, so `i32::MIN` does not overflow.
fn parse_java_int(text: &str) -> Option<i32> {
  let (negative, digits) = match text.as_bytes().first() {
    | Some(b'-') => (true, &text[1..]),
    | Some(b'+') => (false, &text[1..]),
    | _ => (false, text),
  };
  if digits.is_empty() {
    return None;
  }
  let mut value: i32 = 0;
  for character in digits.chars() {
    let digit = decimal_digit_value(character)?;
    value = value.checked_mul(10)?.checked_sub(digit)?;
  }
  if negative {
    Some(value)
  } else {
    value.checked_neg()
  }
}

// `Character.digit(character, 10)`: the digit's value, or `None`.
fn decimal_digit_value(character: char) -> Option<i32> {
  let code = u32::from(character);
  DECIMAL_DIGIT_ZEROS.iter().find_map(|zero| {
    let offset = code.checked_sub(*zero)?;
    if offset < 10 {
      i32::try_from(offset).ok()
    } else {
      None
    }
  })
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
