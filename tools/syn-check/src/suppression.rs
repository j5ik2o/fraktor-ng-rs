//! Suppression comments.
//!
//! ```text
//! // syn-check: allow(<rule>[, <rule>...]) -- <reason>
//! // syn-check: allow-file(<rule>[, <rule>...]) -- <reason>
//! ```
//!
//! `allow` covers the syntactic element that starts on the next line that is
//! not a plain comment, through the end of that element. It replaces the old
//! `#[allow(...)]` on an item, which also covered everything inside the item.
//! `allow-file` covers the whole file. The reason is mandatory.

use std::collections::HashMap;

use syn::visit::{self, Visit};

use crate::{ast, source_file::SourceFile};

const PREFIX: &str = "syn-check:";

/// The rule id under which a malformed suppression comment is reported.
pub(crate) const SUPPRESSION_RULE: &str = "suppression";

pub(crate) struct Directive {
  pub line: usize,
  pub rules: Vec<String>,
  pub reason: String,
  pub used: bool,
  /// `None` covers the file. `Some` is the inclusive line range covered.
  pub range: Option<(usize, usize)>,
}

#[derive(Default)]
pub(crate) struct Directives {
  pub items: Vec<Directive>,
  /// Malformed comments: the line and what is wrong.
  pub errors: Vec<(usize, String)>,
}

impl Directives {
  pub fn parse(file: &SourceFile, known_rules: &[&'static str]) -> Self {
    let lines: Vec<&str> = file.text().lines().collect();
    let mut directives = Self::default();
    let mut extents = None;
    for (index, line) in lines.iter().enumerate() {
      let Some(body) = plain_comment(line).and_then(|comment| comment.strip_prefix(PREFIX)) else {
        continue;
      };
      match parse_body(body.trim(), known_rules) {
        | Ok((file_scope, rules, reason)) => {
          let range = (!file_scope).then(|| {
            let target = next_code_line(&lines, index);
            let extents = extents.get_or_insert_with(|| Extents::of(file));
            (target, extents.end_of(target))
          });
          directives.items.push(Directive { line: index + 1, rules, reason, used: false, range });
        },
        | Err(message) => directives.errors.push((index + 1, message)),
      }
    }
    directives
  }

  /// Whether a comment covers `rule` at `line`. Marks the comment as used.
  pub fn suppresses(&mut self, rule: &str, line: usize) -> bool {
    let mut suppressed = false;
    for directive in &mut self.items {
      let covers = directive.range.is_none_or(|(first, last)| (first..=last).contains(&line));
      if covers && directive.rules.iter().any(|allowed| allowed == rule) {
        directive.used = true;
        suppressed = true;
      }
    }
    suppressed
  }
}

/// The text of a `//` comment line. Doc comments are documentation, not comments.
fn plain_comment(line: &str) -> Option<&str> {
  let comment = line.trim_start().strip_prefix("//")?;
  if comment.starts_with('/') || comment.starts_with('!') {
    return None;
  }
  Some(comment.trim_start())
}

fn parse_body(body: &str, known_rules: &[&'static str]) -> Result<(bool, Vec<String>, String), String> {
  let (file_scope, rest) = if let Some(rest) = body.strip_prefix("allow-file(") {
    (true, rest)
  } else if let Some(rest) = body.strip_prefix("allow(") {
    (false, rest)
  } else {
    return Err("`allow(<rule>) -- <理由>` か `allow-file(<rule>) -- <理由>` の形で書いてください".to_string());
  };
  let Some((list, rest)) = rest.split_once(')') else {
    return Err("ルール名の一覧を `)` で閉じてください".to_string());
  };
  let rules: Vec<String> =
    list.split(',').map(|rule| rule.trim().to_string()).filter(|rule| !rule.is_empty()).collect();
  if rules.is_empty() {
    return Err("抑制するルール名を 1 つ以上書いてください".to_string());
  }
  if let Some(unknown) = rules.iter().find(|rule| !known_rules.contains(&rule.as_str())) {
    return Err(format!("`{unknown}` というルールはありません"));
  }
  let reason = rest.trim().strip_prefix("--").map(str::trim).unwrap_or_default();
  if reason.is_empty() {
    return Err("抑制の理由を `-- <理由>` の形で書いてください".to_string());
  }
  Ok((file_scope, rules, reason.to_string()))
}

/// The 1-based number of the first line after `index` that is not a plain comment.
fn next_code_line(lines: &[&str], index: usize) -> usize {
  let mut next = index + 1;
  while lines.get(next).is_some_and(|line| plain_comment(line).is_some()) {
    next += 1;
  }
  next + 1
}

/// For each line where a syntactic element starts, the last line of the
/// longest element that starts there.
struct Extents {
  last_line_by_first: HashMap<usize, usize>,
}

impl Extents {
  fn of(file: &SourceFile) -> Self {
    let mut extents = Self { last_line_by_first: HashMap::new() };
    if let Some(ast) = file.ast() {
      extents.visit_file(ast);
    }
    extents
  }

  fn end_of(&self, first_line: usize) -> usize {
    self.last_line_by_first.get(&first_line).copied().unwrap_or(first_line)
  }

  fn record<T: quote::ToTokens>(&mut self, node: &T) {
    let first = ast::start(node).line;
    let last = self.last_line_by_first.entry(first).or_insert(first);
    *last = (*last).max(ast::end_line(node));
  }
}

impl<'ast> Visit<'ast> for Extents {
  fn visit_item(&mut self, node: &'ast syn::Item) {
    self.record(node);
    visit::visit_item(self, node);
  }

  fn visit_impl_item(&mut self, node: &'ast syn::ImplItem) {
    self.record(node);
    visit::visit_impl_item(self, node);
  }

  fn visit_trait_item(&mut self, node: &'ast syn::TraitItem) {
    self.record(node);
    visit::visit_trait_item(self, node);
  }

  fn visit_foreign_item(&mut self, node: &'ast syn::ForeignItem) {
    self.record(node);
    visit::visit_foreign_item(self, node);
  }

  fn visit_field(&mut self, node: &'ast syn::Field) {
    self.record(node);
    visit::visit_field(self, node);
  }

  fn visit_variant(&mut self, node: &'ast syn::Variant) {
    self.record(node);
    visit::visit_variant(self, node);
  }

  fn visit_stmt(&mut self, node: &'ast syn::Stmt) {
    self.record(node);
    visit::visit_stmt(self, node);
  }

  fn visit_arm(&mut self, node: &'ast syn::Arm) {
    self.record(node);
    visit::visit_arm(self, node);
  }
}
