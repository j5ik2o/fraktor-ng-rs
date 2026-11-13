//! A port that runs a unit of work and reports whether it failed.

/// Runs a piece of work that may fail and reports whether it completed.
///
/// Pekko catches the exception that a failing actor throws. The kernel cannot
/// catch a panic without the standard library, so it asks a port to do it and
/// keeps only the answer. `true` means the work completed; `false` means it
/// failed, and the parent applies the supervisor strategy.
pub trait FailureCatcher: Send + Sync {
  /// Runs `body` and returns whether it completed without failing.
  fn run(&self, body: &mut dyn FnMut()) -> bool;
}
