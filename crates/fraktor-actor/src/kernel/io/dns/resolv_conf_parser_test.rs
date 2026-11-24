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

// A vertical tab (`U+000B`) separates the label from its arguments, because
// Java's `Character.isWhitespace` is true for it while Rust's
// `char::is_whitespace` is not. Using `split_whitespace` for the label would
// leave `a\u{0B}b` unsplit.
#[test]
fn a_vertical_tab_separates_labels_and_search_elements() {
  assert_eq!(parse("search a\u{0B}b"), vec![String::from("a"), String::from("b")]);
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
