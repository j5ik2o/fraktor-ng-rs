use std::{collections::BTreeMap, fmt::Write as _, time::Duration};

use serde_json::{Value, json};

use crate::{
    layout::Layout, parse_failure::ParseFailure, suppression_record::SuppressionRecord,
    violation::Violation,
};

/// The result of one run.
#[derive(Debug)]
pub struct Report {
    pub layout: Layout,
    /// The ids of the rules that ran.
    pub rules: Vec<&'static str>,
    pub files_checked: usize,
    pub violations: Vec<Violation>,
    pub suppressions: Vec<SuppressionRecord>,
    pub parse_failures: Vec<ParseFailure>,
    pub elapsed: Duration,
}

impl Report {
    /// Whether the run found nothing to fix. A file that cannot be parsed
    /// cannot be vouched for, so it makes the run unclean.
    pub fn is_clean(&self) -> bool {
        self.violations.is_empty() && self.parse_failures.is_empty()
    }

    pub fn to_json(&self) -> Value {
        let mut by_rule = BTreeMap::new();
        for violation in &self.violations {
            *by_rule.entry(violation.rule.as_str()).or_insert(0usize) += 1;
        }
        json!({
            "tool": "syn-check",
            "version": env!("CARGO_PKG_VERSION"),
            "layout": self.layout.name(),
            "rules": self.rules,
            "clean": self.is_clean(),
            "summary": {
                "files_checked": self.files_checked,
                "violations": self.violations.len(),
                "violations_by_rule": by_rule,
                "suppressions": self.suppressions.len(),
                "parse_failures": self.parse_failures.len(),
                "elapsed_ms": u64::try_from(self.elapsed.as_millis()).unwrap_or(u64::MAX),
            },
            "violations": self.violations.iter().map(|violation| json!({
                "rule": violation.rule,
                "path": violation.path.to_string_lossy(),
                "line": violation.position.line,
                "column": violation.position.column,
                "message": violation.message,
            })).collect::<Vec<_>>(),
            "suppressions": self.suppressions.iter().map(|record| json!({
                "path": record.path.to_string_lossy(),
                "line": record.line,
                "scope": if record.file_scope { "file" } else { "next" },
                "rules": record.rules,
                "reason": record.reason,
                "used": record.used,
            })).collect::<Vec<_>>(),
            "parse_failures": self.parse_failures.iter().map(|failure| json!({
                "path": failure.path.to_string_lossy(),
                "message": failure.message,
            })).collect::<Vec<_>>(),
        })
    }

    /// One line per violation, in the form compilers use.
    pub fn to_text(&self) -> String {
        let mut text = String::new();
        for violation in &self.violations {
            let _ = writeln!(
                text,
                "{}:{}:{}: {}: {}",
                violation.path.display(),
                violation.position.line,
                violation.position.column,
                violation.rule,
                violation.message
            );
        }
        for failure in &self.parse_failures {
            let _ = writeln!(
                text,
                "{}: parse failure: {}",
                failure.path.display(),
                failure.message
            );
        }
        for record in &self.suppressions {
            let _ = writeln!(
                text,
                "{}:{}: suppression ({}{}): {} -- {}",
                record.path.display(),
                record.line,
                if record.file_scope { "file" } else { "next" },
                if record.used { "" } else { ", unused" },
                record.rules.join(", "),
                record.reason
            );
        }
        let _ = writeln!(
            text,
            "{} files, {} violations, {} suppressions, {} parse failures, {} ms",
            self.files_checked,
            self.violations.len(),
            self.suppressions.len(),
            self.parse_failures.len(),
            self.elapsed.as_millis()
        );
        text
    }
}
