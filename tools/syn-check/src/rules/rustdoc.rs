use crate::{check_context::CheckContext, position::Position, rule::Rule, sink::Sink, source_file::SourceFile};

/// Requires English `///` and `//!` comments and forbids `ignore` code fences.
/// Ported from the old `rustdoc-lint` (`rustdoc_lint`).
///
/// Like the old lint, this scans the source text line by line. It does not ask
/// syn where the doc comments are, so the result matches the old one on every
/// line that starts with `///` or `//!`.
pub(crate) struct Rustdoc;

impl Rule for Rustdoc {
  fn id(&self) -> &'static str {
    "rustdoc"
  }

  fn check(&self, file: &SourceFile, _cx: &CheckContext<'_>, sink: &mut Sink) {
    let Some(ast) = file.ast() else {
      return;
    };
    // 旧 lint はアイテムを訪れたときにそのファイルを 1 度だけ解析した。アイテムの無いファイルは見ない。
    if ast.items.is_empty() {
      return;
    }
    let lines: Vec<&str> = file.text().lines().collect();
    for block in extract_doc_blocks(&lines) {
      analyze_doc_block(&block, sink);
    }
  }
}

/// One line of a doc comment block, with the text after the marker.
struct DocSegment<'a> {
  line: &'a str,
  /// 0-based.
  line_index: usize,
  /// Byte offset of the text after `///` or `//!` and one optional space.
  content_column: usize,
}

impl<'a> DocSegment<'a> {
  fn new(line: &'a str, line_index: usize, marker: &str) -> Self {
    let indent = line.len() - line.trim_start().len();
    let mut content_column = indent + marker.len();
    if let Some(b' ') = line.as_bytes().get(content_column) {
      content_column += 1;
    }
    Self { line, line_index, content_column }
  }

  fn text(&self) -> &'a str {
    &self.line[self.content_column..]
  }

  /// The position of the byte `offset` into the text, with a character column as rustc prints it.
  fn position_at(&self, offset: usize) -> Position {
    let column = self.line[..self.content_column + offset].chars().count() + 1;
    Position::new(self.line_index + 1, column)
  }
}

/// Runs of consecutive lines that start with the same marker.
/// A blank line, a plain comment, or the other marker ends the run.
fn extract_doc_blocks<'a>(lines: &[&'a str]) -> Vec<Vec<DocSegment<'a>>> {
  let mut blocks = Vec::new();
  let mut index = 0;
  while index < lines.len() {
    let trimmed = lines[index].trim_start();
    let Some(marker) = ["///", "//!"].into_iter().find(|marker| trimmed.starts_with(marker)) else {
      index += 1;
      continue;
    };
    let mut segments = Vec::new();
    while index < lines.len() && lines[index].trim_start().starts_with(marker) {
      segments.push(DocSegment::new(lines[index], index, marker));
      index += 1;
    }
    blocks.push(segments);
  }
  blocks
}

fn analyze_doc_block(block: &[DocSegment<'_>], sink: &mut Sink) {
  let mut inside_fence = false;
  let mut reported_non_english = false;

  for segment in block {
    if let Some(info) = segment.text().trim_start().strip_prefix("```") {
      if !inside_fence && fence_has_ignore(info) {
        sink.report(
                    segment.position_at(0),
                    "rustdoc のコードフェンスでは `ignore` を使用できません。サンプルコードは ```rust としてコンパイル可能な形で記述してください",
                );
      }
      inside_fence = !inside_fence;
      continue;
    }

    // 英語の確認は 1 ブロックにつき最初の 1 件だけ。フェンスの中は見ない。
    if !inside_fence
      && !reported_non_english
      && let Some(offset) = first_japanese_offset(segment.text())
    {
      sink.report(
                segment.position_at(offset),
                "rustdoc は英語で記述してください。日本語などの全角文字はドキュメントコメントでは使わず、別資料に移動してください",
            );
      reported_non_english = true;
    }
  }
}

fn fence_has_ignore(info: &str) -> bool {
  info
    .split(|ch: char| ch == ',' || ch.is_whitespace())
    .filter(|part| !part.is_empty())
    .any(|part| part.eq_ignore_ascii_case("ignore"))
}

fn first_japanese_offset(text: &str) -> Option<usize> {
  text.char_indices().find(|(_, ch)| is_japanese_char(*ch)).map(|(offset, _)| offset)
}

fn is_japanese_char(ch: char) -> bool {
  matches!(
      ch,
      '\u{3000}'
          | '\u{3001}'..='\u{303F}'
          | '\u{3040}'..='\u{309F}'
          | '\u{30A0}'..='\u{30FF}'
          | '\u{31F0}'..='\u{31FF}'
          | '\u{3400}'..='\u{4DBF}'
          | '\u{4E00}'..='\u{9FFF}'
          | '\u{F900}'..='\u{FAFF}'
          | '\u{FF66}'..='\u{FF9F}'
          | '\u{FFE0}'..='\u{FFEE}'
  )
}
