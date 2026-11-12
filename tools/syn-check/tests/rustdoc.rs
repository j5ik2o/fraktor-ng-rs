//! Cases for `rustdoc` that the old UI fixtures do not cover.

use std::{fs, path::PathBuf};

use syn_check::{layout::Layout, options::Options};

/// What a finding is about, read from the message.
#[derive(Debug, PartialEq, Eq)]
enum Kind {
  Ignore,
  English,
}

/// The line, the column and the kind of each violation, for one layout.
fn findings_in(fixture: &str, layout: Layout) -> Vec<(usize, usize, Kind)> {
  let path = PathBuf::from("tests/ui/synthetic/rustdoc").join(fixture);
  let options = Options { layout, rules: Some(vec!["rustdoc".to_string()]) };
  let report = syn_check::run(&[path], &options).expect("run the checker");
  assert!(report.parse_failures.is_empty(), "{:?}", report.parse_failures);
  report
    .violations
    .iter()
    .map(|violation| {
      let kind = if violation.message.contains("`ignore`") { Kind::Ignore } else { Kind::English };
      (violation.position.line, violation.position.column, kind)
    })
    .collect()
}

/// The result is the same in both layouts: the rule has no path predicate.
fn findings(fixture: &str) -> Vec<(usize, usize, Kind)> {
  let ng = findings_in(fixture, Layout::Ng);
  assert_eq!(ng, findings_in(fixture, Layout::Legacy), "{fixture}");
  ng
}

fn english(line: usize, column: usize) -> (usize, usize, Kind) {
  (line, column, Kind::English)
}

fn ignore(line: usize, column: usize) -> (usize, usize, Kind) {
  (line, column, Kind::Ignore)
}

#[test]
fn outer_and_inner_doc_comments_are_both_checked() {
  assert_eq!(
    findings("line_and_inner.rs"),
    vec![
      english(1, 5),
      english(7, 5),
      english(10, 9),
      // `é` は 2 バイトだが 1 桁として数える。
      english(13, 35),
      english(17, 9),
      english(18, 9),
    ]
  );
}

#[test]
fn only_the_first_japanese_line_of_a_block_is_reported() {
  assert_eq!(
    findings("block_grouping.rs"),
    vec![english(2, 5), english(6, 5), english(8, 5), english(11, 5), english(13, 5), english(17, 5), english(19, 5),]
  );
}

#[test]
fn lines_are_scanned_as_text() {
  assert_eq!(findings("text_scan.rs"), vec![english(4, 5), english(7, 6), english(10, 5)]);
}

#[test]
fn japanese_inside_a_code_fence_is_allowed() {
  assert_eq!(findings("fences.rs"), vec![english(12, 5), english(19, 5), english(28, 5), english(42, 5),]);
}

#[test]
fn ignore_is_found_among_the_fence_flags_and_nowhere_else() {
  assert_eq!(
    findings("ignore_fence.rs"),
    vec![
      ignore(1, 5),
      ignore(6, 5),
      ignore(11, 5),
      ignore(16, 5),
      ignore(21, 5),
      ignore(26, 5),
      ignore(64, 5),
      ignore(70, 9),
    ]
  );
}

#[test]
fn the_japanese_range_edges_are_exact() {
  let path = "tests/ui/synthetic/rustdoc/japanese_boundaries.rs";
  // 範囲の中の端 (U+3000 から U+303F など) と、そのすぐ外。
  const INSIDE: &[u32] = &[
    0x3000, 0x3001, 0x303F, 0x3040, 0x309F, 0x30A0, 0x30FF, 0x31F0, 0x31FF, 0x3400, 0x4DBF, 0x4E00, 0x9FFF, 0xF900,
    0xFAFF, 0xFF66, 0xFF9F, 0xFFE0, 0xFFEE,
  ];
  const OUTSIDE: &[u32] =
    &[0x2FFF, 0x3100, 0x31EF, 0x3200, 0x33FF, 0x4DC0, 0x4DFF, 0xA000, 0xF8FF, 0xFB00, 0xFF65, 0xFFA0, 0xFFDF, 0xFFEF];

  let mut expected = Vec::new();
  let mut seen = Vec::new();
  for (index, line) in fs::read_to_string(path).expect("read the fixture").lines().enumerate() {
    let Some(label) = line.strip_prefix("/// U+") else {
      continue;
    };
    let (code, character) = label.split_once(' ').expect("label and character");
    let code = u32::from_str_radix(code, 16).expect("hex label");
    assert_eq!(
      character.chars().collect::<Vec<_>>(),
      vec![char::from_u32(code).expect("scalar value")],
      "line {}: the character does not match its label",
      index + 1
    );
    seen.push(code);
    if INSIDE.contains(&code) {
      // `/// U+XXXX ` の 11 文字のあとに文字がある。
      expected.push(english(index + 1, 12));
    }
  }
  let mut listed: Vec<u32> = INSIDE.iter().chain(OUTSIDE).copied().collect();
  listed.sort_unstable();
  seen.sort_unstable();
  assert_eq!(seen, listed, "the fixture and the lists disagree");
  assert_eq!(findings("japanese_boundaries.rs"), expected);
}

#[test]
fn a_suppression_comment_covers_the_next_element() {
  assert_eq!(findings("suppressed.rs"), vec![english(5, 5), english(15, 5)]);
  assert_eq!(findings("suppressed_file.rs"), Vec::new());
}

#[test]
fn a_file_without_items_is_not_scanned() {
  assert_eq!(findings("no_items.rs"), Vec::new());
}

#[test]
fn a_macro_definition_counts_as_an_item() {
  assert_eq!(findings("only_macro_rules.rs"), vec![english(1, 5)]);
}

#[test]
fn a_macro_call_counts_as_an_item() {
  assert_eq!(findings("only_macro_call.rs"), vec![english(1, 5)]);
}
