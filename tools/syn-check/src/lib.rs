//! Static checker for the fraktor-ng-rs structure rules.
//!
//! The checker parses each file once with syn and runs every selected rule on
//! it. It never compiles the code it checks, so it has no type information,
//! does not expand macros, and does not resolve names.

use std::{path::PathBuf, time::Instant};

pub mod ast;
pub mod check_context;
pub mod layout;
pub mod options;
pub mod parse_failure;
mod paths;
pub mod position;
pub mod report;
pub mod rule;
mod rules;
pub mod run_error;
pub mod sink;
pub mod source_file;
mod suppression;
pub mod suppression_record;
pub mod violation;
pub mod workspace;

use check_context::CheckContext;
use options::Options;
use parse_failure::ParseFailure;
use position::Position;
use report::Report;
use rule::Rule;
use run_error::RunError;
use sink::Sink;
use suppression::{Directives, SUPPRESSION_RULE};
use suppression_record::SuppressionRecord;
use violation::Violation;
use workspace::Workspace;

/// The id of every rule, and whether it runs by default.
pub fn rule_ids() -> Vec<(&'static str, bool)> {
  rules::all().iter().map(|rule| (rule.id(), rule.default_enabled())).collect()
}

/// Checks every `.rs` file under `roots`.
pub fn run(roots: &[PathBuf], options: &Options) -> Result<Report, RunError> {
  let started = Instant::now();
  let all = rules::all();
  let known: Vec<&'static str> = all.iter().map(|rule| rule.id()).collect();
  if let Some(unknown) = options.rules.iter().flatten().find(|id| !known.contains(&id.as_str())) {
    return Err(RunError::UnknownRule(unknown.clone()));
  }
  let selected: Vec<&dyn Rule> = all
    .iter()
    .map(Box::as_ref)
    .filter(|rule| match &options.rules {
      | Some(ids) => ids.iter().any(|id| id == rule.id()),
      | None => rule.default_enabled(),
    })
    .collect();

  let files = Workspace::discover(roots)?;
  let workspace = Workspace::new();
  let cx = CheckContext { layout: options.layout, workspace: &workspace };
  let mut report = Report {
    layout: options.layout,
    rules: selected.iter().map(|rule| rule.id()).collect(),
    files_checked: files.len(),
    violations: Vec::new(),
    suppressions: Vec::new(),
    parse_failures: Vec::new(),
    elapsed: started.elapsed(),
  };

  for path in files {
    let Some(file) = workspace.source(&path) else {
      report.parse_failures.push(ParseFailure { path, message: "the file cannot be read as UTF-8 text".to_string() });
      continue;
    };
    if let Some(message) = file.parse_error() {
      report.parse_failures.push(ParseFailure { path: path.clone(), message: message.to_string() });
    }

    let mut directives = Directives::parse(&file, &known);
    for rule in &selected {
      let mut sink = Sink::new();
      rule.check(&file, &cx, &mut sink);
      for (position, message) in sink.into_findings() {
        if !directives.suppresses(rule.id(), position.line) {
          report.violations.push(Violation { rule: rule.id().to_string(), path: path.clone(), position, message });
        }
      }
    }
    for (line, message) in directives.errors {
      report.violations.push(Violation {
        rule: SUPPRESSION_RULE.to_string(),
        path: path.clone(),
        position: Position::new(line, 1),
        message,
      });
    }
    for directive in directives.items {
      report.suppressions.push(SuppressionRecord {
        path: path.clone(),
        line: directive.line,
        file_scope: directive.range.is_none(),
        rules: directive.rules,
        reason: directive.reason,
        used: directive.used,
      });
    }
  }

  report
    .violations
    .sort_by(|left, right| (&left.path, left.position, &left.rule).cmp(&(&right.path, right.position, &right.rule)));
  report.elapsed = started.elapsed();
  Ok(report)
}
