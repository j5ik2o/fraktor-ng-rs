use std::path::PathBuf;

use crate::position::Position;

/// One rule violation that no suppression comment covers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Violation {
  pub rule: String,
  pub path: PathBuf,
  pub position: Position,
  pub message: String,
}
