//! Failures observed when waiting for an ask.

/// Distinguishes an ask's own deadline from the limit of one wait.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AskError {
  /// The ask expired without a reply; its result cannot change afterwards.
  AskTimedOut,
  /// This wait expired; the ask can still complete on a later wait.
  WaitTimedOut,
}
