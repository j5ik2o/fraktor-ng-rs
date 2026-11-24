//! Unit tests for [`ResolvConfParser`](super::ResolvConfParser).

use alloc::{string::String, vec, vec::Vec};

use super::{ResolvConfParseError, ResolvConfParser};

fn parse(text: &str) -> Vec<String> {
  ResolvConfParser::parse_lines(text.lines()).expect("the resolv.conf must parse").search().to_vec()
}

fn ndots(text: &str) -> i32 {
  ResolvConfParser::parse_lines(text.lines()).expect("the resolv.conf must parse").ndots()
}

// The initial values when nothing is found: an empty search list and ndots 1.
// A default of 0 for ndots, or a `[""]` search, would show up here.
#[test]
fn an_empty_input_defaults_to_no_search_and_ndots_one() {
  let conf = ResolvConfParser::parse_lines([]).expect("an empty input parses");
  assert_eq!(conf.search(), [] as [String; 0]);
  assert_eq!(conf.ndots(), 1);
}

// A `search` line with no arguments keeps Java's `"".split(...)` result of one
// empty element, not an empty list. Mapping the empty argument to `[]` would
// fail this.
#[test]
fn a_search_line_without_arguments_holds_one_empty_element() {
  assert_eq!(parse("search"), vec![String::new()]);
}

// `domain` and `search` are case sensitive (the original matches the exact
// words), so `Search` is an unknown line and leaves the search list empty.
#[test]
fn an_unknown_label_in_a_different_case_is_ignored() {
  assert_eq!(parse("Search example.com"), [] as [String; 0]);
}

// `domain` keeps its whole argument run as one element; splitting on spaces
// would yield two elements.
#[test]
fn a_domain_line_keeps_its_arguments_as_one_element() {
  assert_eq!(parse("domain a b"), vec![String::from("a b")]);
}

// A vertical tab (`U+000B`) is in the ASCII `\s` class of Java's
// `split("\\s+")`, so it separates search elements. Rust's
// `split_ascii_whitespace` does not split on it and would leave `a\u{0B}b`
// as one element.
#[test]
fn a_vertical_tab_separates_labels_and_search_elements() {
  assert_eq!(parse("search a\u{0B}b"), vec![String::from("a"), String::from("b")]);
}

// `U+001C` is whitespace to Java's `Character.isWhitespace` but not to Rust's
// `char::is_whitespace`, so it separates the label from its arguments:
// `search\u{1C}example.com` is a `search` line. A parser that used
// `char::is_whitespace` for the label would see one unknown label and ignore it.
#[test]
fn an_information_separator_separates_the_label() {
  assert_eq!(parse("search\u{1C}example.com"), vec![String::from("example.com")]);
}

// A no-break space (`U+00A0`) is not Java whitespace, so it does not separate
// the label: `search\u{A0}example.com` is an unknown label and is ignored. A
// parser that used `char::is_whitespace` would emit `["example.com"]`.
#[test]
fn a_no_break_space_is_not_a_label_separator() {
  assert_eq!(parse("search\u{A0}example.com"), [] as [String; 0]);
}

// The last `ndots` wins. Reading only the first value would keep 1.
#[test]
fn the_last_ndots_option_wins() {
  assert_eq!(ndots("options ndots:1 ndots:4"), 4);
}

// A negative `ndots` parses, matching Java's `Integer.parseInt`. A parser that
// rejected a leading `-`, or parsed into an unsigned type, would fail.
#[test]
fn a_negative_ndots_option_parses() {
  assert_eq!(ndots("options ndots:-1"), -1);
}

// A leading `+` parses, matching Java's `Integer.parseInt`.
#[test]
fn a_plus_prefixed_ndots_option_parses() {
  assert_eq!(ndots("options ndots:+5"), 5);
}

// A `ndots` value that is not an integer is reported, not silently dropped.
#[test]
fn a_non_numeric_ndots_option_is_an_error() {
  assert_eq!(ResolvConfParser::parse_lines(["options ndots:x"]), Err(ResolvConfParseError::InvalidNdots));
}

// The arguments of `options` are not trimmed before splitting, so leading and
// trailing whitespace produce empty elements that are not `ndots:` entries and
// are ignored, while a real `ndots` in the middle still lands.
#[test]
fn options_are_split_without_trimming_around_the_ndots_entry() {
  assert_eq!(ndots("options  ndots:3  rotate "), 3);
}

// A line is trimmed at both ends with the `U+0000`..`U+0020` set before the
// leading `#` is checked, so an indented comment is ignored.
#[test]
fn a_comment_after_leading_whitespace_is_ignored() {
  assert_eq!(parse("search example.com\n   #search foobar.com"), vec![String::from("example.com")]);
}

// `#` and `;` only start a comment at the start of a line; inside a `search`
// argument run they are ordinary characters. Truncating at an inline `#` would
// drop the second element.
#[test]
fn an_inline_hash_is_not_a_comment() {
  assert_eq!(parse("search example.com #foobar.com"), vec![String::from("example.com"), String::from("#foobar.com")]);
}

// A later `search` line replaces the earlier list instead of appending to it.
// Appending would yield both elements.
#[test]
fn a_later_search_line_replaces_the_earlier_one() {
  assert_eq!(parse("search a.com b.com\nsearch c.com"), vec![String::from("c.com")]);
}

// A `domain` line after a `search` line replaces the search list, as the
// original does, rather than being ignored or appended.
#[test]
fn a_domain_line_after_a_search_line_replaces_it() {
  assert_eq!(parse("search a.com b.com\ndomain c.com"), vec![String::from("c.com")]);
}

#[test]
fn the_display_text_names_the_failure() {
  assert!(!ResolvConfParseError::InvalidNdots.to_string().is_empty());
}

// The original's `toInt` is `java.lang.Integer.parseInt`, which reads each
// UTF-16 unit with `Character.digit(ch, 10)`. Measured on Temurin 21.0.13,
// 25.0.5 and 26.0.2.1 over every code point, it accepts exactly these 37 runs of
// ten Unicode decimal digits (general category Nd), all in the BMP. Each entry
// is the run's zero.
const JDK_DECIMAL_DIGIT_ZEROS: [char; 37] = [
  '\u{0030}', '\u{0660}', '\u{06F0}', '\u{07C0}', '\u{0966}', '\u{09E6}', '\u{0A66}', '\u{0AE6}', '\u{0B66}',
  '\u{0BE6}', '\u{0C66}', '\u{0CE6}', '\u{0D66}', '\u{0DE6}', '\u{0E50}', '\u{0ED0}', '\u{0F20}', '\u{1040}',
  '\u{1090}', '\u{17E0}', '\u{1810}', '\u{1946}', '\u{19D0}', '\u{1A80}', '\u{1A90}', '\u{1B50}', '\u{1BB0}',
  '\u{1C40}', '\u{1C50}', '\u{A620}', '\u{A8D0}', '\u{A900}', '\u{A9D0}', '\u{A9F0}', '\u{AA50}', '\u{ABF0}',
  '\u{FF10}',
];

fn ndots_result(value: &str) -> Result<i32, ResolvConfParseError> {
  ResolvConfParser::parse_lines([alloc::format!("options ndots:{value}").as_str()]).map(|conf| conf.ndots())
}

// A full-width `５` is a decimal digit to `Integer.parseInt`, so the original
// reads 5. Rust's `str::parse::<i32>` rejects it.
#[test]
fn a_full_width_ndots_digit_parses() {
  assert_eq!(ndots_result("５"), Ok(5));
  assert_eq!(ndots_result("１２"), Ok(12));
}

#[test]
fn a_signed_full_width_ndots_value_parses() {
  assert_eq!(ndots_result("-５"), Ok(-5));
  assert_eq!(ndots_result("+５"), Ok(5));
}

// Digits from other scripts, and digits from different scripts in one value.
#[test]
fn decimal_digits_of_other_scripts_parse() {
  assert_eq!(ndots_result("٣"), Ok(3));
  assert_eq!(ndots_result("१२"), Ok(12));
  assert_eq!(ndots_result("1５٣"), Ok(153));
}

// Every run the JDK accepts gives 0 at its zero and 9 at its nine. A table that
// missed a run, or was off by one at either end, fails here.
#[test]
fn every_decimal_digit_run_the_jdk_accepts_parses() {
  for zero in JDK_DECIMAL_DIGIT_ZEROS {
    let nine = char::from_u32(zero as u32 + 9).expect("a digit nine");
    assert_eq!(ndots_result(&String::from(zero)), Ok(0), "zero {:04X}", zero as u32);
    assert_eq!(ndots_result(&String::from(nine)), Ok(9), "nine {:04X}", zero as u32);
  }
}

// The code points just before each zero and just after each nine are not
// digits of that run; where they are not the next run's digit, they fail.
#[test]
fn the_code_points_around_each_run_are_not_digits() {
  for zero in JDK_DECIMAL_DIGIT_ZEROS {
    for outside in [zero as u32 - 1, zero as u32 + 10] {
      let character = char::from_u32(outside).expect("a scalar value");
      let in_another_run =
        JDK_DECIMAL_DIGIT_ZEROS.iter().any(|other| (*other as u32..*other as u32 + 10).contains(&outside));
      if !in_another_run {
        assert_eq!(ndots_result(&String::from(character)), Err(ResolvConfParseError::InvalidNdots), "{outside:04X}");
      }
    }
  }
}

// `Integer.parseInt` reads UTF-16 units, and a surrogate is never a digit, so a
// decimal digit outside the BMP such as `U+1D7D3` (MATHEMATICAL BOLD DIGIT FIVE)
// is rejected.
#[test]
fn a_decimal_digit_outside_the_bmp_is_an_error() {
  assert_eq!(ndots_result("\u{1D7D3}"), Err(ResolvConfParseError::InvalidNdots));
  assert_eq!(ndots_result("\u{104A5}"), Err(ResolvConfParseError::InvalidNdots));
}

// Numbers that are not decimal digits (No and Nl) are rejected.
#[test]
fn numbers_that_are_not_decimal_digits_are_errors() {
  for value in ["²", "¹", "Ⅴ", "①", "五", "٫"] {
    assert_eq!(ndots_result(value), Err(ResolvConfParseError::InvalidNdots), "{value}");
  }
}

#[test]
fn a_sign_without_digits_is_an_error() {
  for value in ["", "+", "-", "++5", "+-5", "-+5", "5-", "+＋5", "－5", "＋5"] {
    assert_eq!(ndots_result(value), Err(ResolvConfParseError::InvalidNdots), "{value}");
  }
}

#[test]
fn other_non_digit_text_is_an_error() {
  for value in ["5a", "0x5", "5.0", "5\u{200B}", "\u{00A0}5", "5\u{3000}"] {
    assert_eq!(ndots_result(value), Err(ResolvConfParseError::InvalidNdots), "{value}");
  }
}

#[test]
fn leading_zeros_parse() {
  assert_eq!(ndots_result("007"), Ok(7));
  assert_eq!(ndots_result("-０００"), Ok(0));
}

#[test]
fn the_ends_of_the_i32_range_parse() {
  assert_eq!(ndots_result("2147483647"), Ok(i32::MAX));
  assert_eq!(ndots_result("+2147483647"), Ok(i32::MAX));
  assert_eq!(ndots_result("-2147483648"), Ok(i32::MIN));
  assert_eq!(ndots_result("２１４７４８３６４７"), Ok(i32::MAX));
  assert_eq!(ndots_result("-２１４７４８３６４８"), Ok(i32::MIN));
  assert_eq!(ndots_result("0000000000002147483647"), Ok(i32::MAX));
}

#[test]
fn values_past_the_i32_range_are_errors() {
  for value in [
    "2147483648",
    "-2147483649",
    "２１４７４８３６４８",
    "-２１４７４８３６４９",
    "99999999999",
    "-99999999999",
    "18446744073709551616",
  ] {
    assert_eq!(ndots_result(value), Err(ResolvConfParseError::InvalidNdots), "{value}");
  }
}

// Over every scalar value, exactly the 370 code points the JDK measurement
// accepted (37 runs of ten) parse as one-digit values.
#[test]
fn exactly_the_jdk_digit_count_parses_over_all_code_points() {
  let accepted = (0..=0x10_FFFF_u32)
    .filter_map(char::from_u32)
    .filter(|character| super::parse_java_int(character.encode_utf8(&mut [0; 4])).is_some())
    .count();
  assert_eq!(accepted, 370);
}
