//! The kind of pre-release a release version may carry.

#[cfg(test)]
#[path = "pre_release_kind_test.rs"]
mod tests;

/// The pre-release kinds that `-(M|RC)` accepts. A release candidate is newer
/// than a milestone of the same numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PreReleaseKind {
  /// `M`, a milestone.
  Milestone,
  /// `RC`, a release candidate.
  ReleaseCandidate,
}

impl PreReleaseKind {
  /// Splits `qualifier` into its kind and the text after the kind's letters,
  /// or returns `None` when it starts with neither `RC` nor `M`.
  pub(crate) fn split(qualifier: &str) -> Option<(Self, &str)> {
    match qualifier.strip_prefix("RC") {
      | Some(number) => Some((Self::ReleaseCandidate, number)),
      | None => Some((Self::Milestone, qualifier.strip_prefix('M')?)),
    }
  }
}
