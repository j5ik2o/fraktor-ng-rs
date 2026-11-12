use std::path::PathBuf;

/// One suppression comment found in the checked files, for audit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SuppressionRecord {
  pub path: PathBuf,
  pub line: usize,
  /// `true` for `allow-file`, `false` for `allow`.
  pub file_scope: bool,
  pub rules: Vec<String>,
  pub reason: String,
  /// Whether the comment suppressed at least one violation in this run.
  pub used: bool,
}
