use std::path::PathBuf;

/// A file that could not be read or that syn could not parse.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseFailure {
  pub path: PathBuf,
  pub message: String,
}
