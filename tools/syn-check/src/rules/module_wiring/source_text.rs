//! Text scans that the old lint ran on the source, ported as text scans.

use crate::position::Position;

/// The byte offset of `at` in `text`. A position past the end of its line
/// maps to the line break.
pub(super) fn byte_offset(text: &str, at: Position) -> usize {
    let mut line_start = 0;
    for _ in 1..at.line {
        match text[line_start..].find('\n') {
            Some(index) => line_start += index + 1,
            None => return text.len(),
        }
    }
    text[line_start..]
        .char_indices()
        .nth(at.column.saturating_sub(1))
        .map_or(text.len(), |(index, _)| line_start + index)
}

/// The position of the byte at `offset` in `text`.
pub(super) fn position(text: &str, offset: usize) -> Position {
    let before = &text[..offset];
    let line_start = before.rfind('\n').map_or(0, |index| index + 1);
    Position::new(
        before.matches('\n').count() + 1,
        before[line_start..].chars().count() + 1,
    )
}

/// The byte offset of the first `include!(` in `snippet`, as the old
/// `find_include_invocation_span` found it. An occurrence is skipped when the
/// character before it (whitespace aside) is a letter, a digit, `_`, `#`, or
/// `/`. Other comments and string literals are not skipped.
pub(super) fn find_include(snippet: &str) -> Option<usize> {
    const TARGET: &str = "include!";
    let mut search_from = 0;
    while let Some(found) = snippet[search_from..].find(TARGET) {
        let at = search_from + found;
        search_from = at + TARGET.len();
        let before = snippet[..at].chars().rev().find(|ch| !ch.is_whitespace());
        if before.is_some_and(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '#' | '/')) {
            continue;
        }
        let after = snippet[search_from..]
            .chars()
            .find(|ch| !ch.is_whitespace());
        if after == Some('(') {
            return Some(at);
        }
    }
    None
}

/// Whether the last non-blank line of `before` is `#[cfg(...)]` with a
/// predicate that requires `test`. The old `has_cfg_test_attr_before_path`.
pub(super) fn ends_with_cfg_test_line(before: &str) -> bool {
    let Some(line) = before.lines().rev().find(|line| !line.trim().is_empty()) else {
        return false;
    };
    let normalized = without_whitespace(line);
    normalized
        .strip_prefix("#[cfg(")
        .and_then(|rest| rest.strip_suffix(")]"))
        .is_some_and(requires_test)
}

/// Whether the `cfg` predicate `expr` (the text inside `cfg(...)`) holds only
/// when `test` is set: `test`, or `all(...)` with such an argument.
pub(super) fn cfg_requires_test(expr: &str) -> bool {
    requires_test(&without_whitespace(expr))
}

fn requires_test(expr: &str) -> bool {
    if expr == "test" {
        return true;
    }
    let Some(inner) = expr
        .strip_prefix("all(")
        .and_then(|rest| rest.strip_suffix(')'))
    else {
        return false;
    };
    split_top_level_args(inner).into_iter().any(requires_test)
}

fn split_top_level_args(expr: &str) -> Vec<&str> {
    let mut args = Vec::new();
    let mut start = 0;
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for (index, ch) in expr.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                args.push(expr[start..index].trim());
                start = index + ch.len_utf8();
            }
            _ => {}
        }
    }
    args.push(expr[start..].trim());
    args
}

fn without_whitespace(text: &str) -> String {
    text.chars().filter(|ch| !ch.is_whitespace()).collect()
}
