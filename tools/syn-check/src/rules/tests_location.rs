use std::{ops::Range, path::Path};

use crate::{check_context::CheckContext, position::Position, rule::Rule, sink::Sink, source_file::SourceFile};

/// Requires tests to live in sibling `*_test.rs` files. Ported from the old
/// `tests-location-lint` (`separate_tests`).
///
/// The old lint scanned the source text, not the syntax tree, and so does
/// this port: a match inside a comment or a string literal counts too.
pub(crate) struct TestsLocation;

const INLINE_TEST_MODULE: &str = "`mod tests { ... }` は兄弟ファイル `<module>_test.rs` に置いてください。テストモジュールを `<module>_test.rs` へ丸ごと移し、親ファイルには `#[cfg(test)] #[path = \"<module>_test.rs\"] mod tests;` だけを残してください";
const CFG_TEST_ITEM: &str = "本番モジュールに `#[cfg(test)]` のアイテムは置けません。テスト専用コードは `<module>_test.rs` へ移し、親ファイルに `#[cfg(test)] #[path = \"<module>_test.rs\"] mod tests;` を宣言してください。構造体のメソッドなら、テストファイル側で `impl 型名 { ... }` を定義し `use super::*;` で型やフィールドに触ってください";
const DIRECT_TEST_ATTRIBUTE: &str = "テスト関数は専用の `<module>_test.rs` に移してください。依存するヘルパーも一緒に移し、親ファイルに `#[cfg(test)] #[path = \"<module>_test.rs\"] mod tests;` を宣言してください";
const INNER_CFG_TEST: &str = "テストファイル内で `#![cfg(test)]` は使えません。この属性を削除し、親ファイルに `#[cfg(test)] #[path = \"<module>_test.rs\"] mod tests;` を宣言してテストモジュールを有効化してください";

impl Rule for TestsLocation {
  fn id(&self) -> &'static str {
    "tests-location"
  }

  fn check(&self, file: &SourceFile, _cx: &CheckContext<'_>, sink: &mut Sink) {
    let source = file.text();
    if is_tests_path(file.path()) {
      report_all(sink, source, find_inner_cfg_test_attributes(source), INNER_CFG_TEST);
      return;
    }
    report_all(sink, source, find_inline_test_modules(source), INLINE_TEST_MODULE);
    report_all(sink, source, find_cfg_test_items(source), CFG_TEST_ITEM);
    report_all(sink, source, find_direct_test_attributes(source), DIRECT_TEST_ATTRIBUTE);
  }
}

fn report_all(sink: &mut Sink, source: &str, ranges: Vec<Range<usize>>, message: &str) {
  for range in ranges {
    sink.report(position_of(source, range.start), message);
  }
}

/// The 1-based line and character column of the byte offset `offset`.
fn position_of(source: &str, offset: usize) -> Position {
  let before = &source[..offset];
  let line_start = before.rfind('\n').map_or(0, |newline| newline + 1);
  Position::new(before.matches('\n').count() + 1, before[line_start..].chars().count() + 1)
}

fn is_tests_path(path: &Path) -> bool {
  if let Some(file_name) = path.file_name().and_then(|name| name.to_str())
    && (file_name == "tests.rs" || file_name.ends_with("_test.rs") || file_name.ends_with("_tests.rs"))
  {
    return true;
  }
  path.components().any(|component| component.as_os_str() == "tests")
}

fn find_inline_test_modules(src: &str) -> Vec<Range<usize>> {
  const PATTERN: &str = "mod tests";
  let mut matches = Vec::new();
  let mut cursor = 0;

  while let Some(offset) = src[cursor..].find(PATTERN) {
    let absolute = cursor + offset;
    let after = &src[absolute + PATTERN.len()..];
    let skipped = leading_whitespace_len(after);
    if after[skipped..].starts_with('{') {
      matches.push(absolute..absolute + PATTERN.len() + skipped + 1);
    }
    cursor = absolute + PATTERN.len();
  }

  matches
}

fn find_cfg_test_items(src: &str) -> Vec<Range<usize>> {
  const ATTR: &str = "#[cfg(test)]";
  let mut matches = Vec::new();
  let mut cursor = 0;

  while let Some(offset) = src[cursor..].find(ATTR) {
    let absolute = cursor + offset;
    let rest = &src[absolute + ATTR.len()..];
    let after = &rest[leading_whitespace_len(rest)..];
    cursor = absolute + ATTR.len();
    if is_sibling_test_module_hook(after) {
      continue;
    }
    // `mod tests { ... }` は find_inline_test_modules が報告するので、二重に報告しない。
    if is_tests_module_declaration(after) {
      continue;
    }
    matches.push(absolute..absolute + ATTR.len());
  }

  matches
}

fn is_tests_module_declaration(after_cfg_test: &str) -> bool {
  let after = strip_visibility_prefix(after_cfg_test);
  let Some(rest) = after.strip_prefix("mod tests") else {
    return false;
  };

  match rest.chars().next() {
    | Some(';') | Some('{') => true,
    | Some(c) => c.is_whitespace(),
    | None => false,
  }
}

fn strip_visibility_prefix(text: &str) -> &str {
  let text = text.trim_start();
  let Some(rest) = text.strip_prefix("pub") else {
    return text;
  };

  match rest.chars().next() {
    | Some(c) if c.is_whitespace() => rest.trim_start(),
    | Some('(') => {
      let Some(closing) = rest.find(')') else {
        return text;
      };
      rest[closing + 1..].trim_start()
    },
    | _ => text,
  }
}

fn is_sibling_test_module_hook(after_cfg_test: &str) -> bool {
  let after = after_cfg_test.trim_start();
  if !after.starts_with("#[") {
    return false;
  }

  let Some(closing) = find_attribute_end(&after[2..]) else {
    return false;
  };

  let attr_body = &after[2..2 + closing];
  let normalized: String = attr_body.chars().filter(|c| !c.is_whitespace()).collect();
  if !normalized.starts_with("path=\"") || !normalized.ends_with("_test.rs\"") {
    return false;
  }

  let attr_end = 2 + closing + 1;
  let after_path = after[attr_end..].trim_start();
  let after_module = strip_visibility_prefix(after_path);
  after_module.starts_with("mod tests;")
}

fn find_direct_test_attributes(src: &str) -> Vec<Range<usize>> {
  let mut matches = Vec::new();
  let mut cursor = 0;

  while let Some(start) = src[cursor..].find("#[") {
    let absolute = cursor + start;
    let remaining = &src[absolute + 2..];
    let Some(closing) = find_attribute_end(remaining) else {
      break;
    };
    let attr_body = &remaining[..closing];
    let attr_name = attr_body.trim().split(|c: char| c == '(' || c.is_whitespace()).next().unwrap_or("");

    if attr_name.ends_with("::test") || attr_name == "test" {
      matches.push(absolute..absolute + 2 + closing + 1);
    }

    cursor = absolute + 2 + closing + 1;
  }

  matches
}

/// The byte offset of the `]` that closes an attribute whose body starts at
/// the beginning of `segment`.
fn find_attribute_end(segment: &str) -> Option<usize> {
  let mut depth = 0usize;
  for (idx, ch) in segment.char_indices() {
    match ch {
      | '[' => depth += 1,
      | ']' => {
        if depth == 0 {
          return Some(idx);
        }
        depth -= 1;
      },
      | _ => {},
    }
  }
  None
}

fn find_inner_cfg_test_attributes(src: &str) -> Vec<Range<usize>> {
  let mut matches = Vec::new();
  let mut cursor = 0;

  while let Some(start) = src[cursor..].find("#![") {
    let absolute = cursor + start;
    let remaining = &src[absolute + 3..];
    let Some(closing) = find_attribute_end(remaining) else {
      break;
    };
    let attr_body = &remaining[..closing];
    let normalized: String = attr_body.chars().filter(|c| !c.is_whitespace()).collect();
    if normalized == "cfg(test)" {
      matches.push(absolute..absolute + 3 + closing + 1);
    }
    cursor = absolute + 3 + closing + 1;
  }

  matches
}

fn leading_whitespace_len(text: &str) -> usize {
  text.chars().take_while(|c| c.is_whitespace()).map(char::len_utf8).sum()
}
