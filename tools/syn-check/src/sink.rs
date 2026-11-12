use crate::position::Position;

/// Collects what one rule finds in one file.
#[derive(Default)]
pub struct Sink {
  findings: Vec<(Position, String)>,
}

impl Sink {
  pub fn new() -> Self {
    Self::default()
  }

  /// Reports a violation at `at`. The checker adds the rule id and the path.
  pub fn report(&mut self, at: Position, message: impl Into<String>) {
    self.findings.push((at, message.into()));
  }

  pub(crate) fn into_findings(self) -> Vec<(Position, String)> {
    self.findings
  }
}
