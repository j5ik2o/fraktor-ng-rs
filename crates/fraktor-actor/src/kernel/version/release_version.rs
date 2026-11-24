//! The parts of a release version that a compatibility check reads.

use super::pre_release_kind::PreReleaseKind;

#[cfg(test)]
#[path = "release_version_test.rs"]
mod tests;

/// The numbers and optional milestone of a `major.minor.patch[-QUALIFIER]`
/// version. Every number keeps its original digit text so the compatibility
/// check can tell `02` from `2`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ReleaseVersion<'a> {
  major: &'a str,
  minor: &'a str,
  patch: &'a str,
  milestone: Option<(PreReleaseKind, &'a str)>,
}

impl<'a> ReleaseVersion<'a> {
  /// Reads `text` as a whole release version, or returns `None` when the text
  /// does not match `(\d+)\.(\d+)\.(\d+)(?:-(M|RC)(\d+))?`.
  pub(crate) fn parse(text: &'a str) -> Option<Self> {
    let mut parts = text.splitn(4, '.');
    let major = parts.next()?;
    let minor = parts.next()?;
    let rest = parts.next()?;
    if parts.next().is_some() || !is_digits(major) || !is_digits(minor) {
      return None;
    }
    let (patch, milestone) = read_patch_and_milestone(rest)?;
    Some(Self { major, minor, patch, milestone })
  }

  /// Decides whether this version, as the required one, is satisfied by `current`.
  ///
  /// Mirrors `PekkoVersion.require` once both versions parse: the major must
  /// match textually; a different minor only compares minors; a different patch
  /// only compares patches; identical numbers compare the milestone.
  pub(crate) fn satisfies(&self, current: &Self) -> bool {
    if self.major != current.major {
      return false;
    }
    if self.minor != current.minor {
      return !digits_less(current.minor, self.minor);
    }
    if self.patch != current.patch {
      return !digits_less(current.patch, self.patch);
    }
    self.satisfies_milestone(current)
  }

  fn satisfies_milestone(&self, current: &Self) -> bool {
    let parsed = (self.milestone, current.milestone);
    let (Some((required_kind, required_number)), Some((current_kind, current_number))) = parsed else {
      return current.milestone.is_none();
    };
    match (required_kind, current_kind) {
      | (PreReleaseKind::Milestone, PreReleaseKind::Milestone)
      | (PreReleaseKind::ReleaseCandidate, PreReleaseKind::ReleaseCandidate) => {
        !digits_less(current_number, required_number)
      },
      | (PreReleaseKind::Milestone, PreReleaseKind::ReleaseCandidate) => true,
      | (PreReleaseKind::ReleaseCandidate, PreReleaseKind::Milestone) => false,
    }
  }
}

fn read_patch_and_milestone(rest: &str) -> Option<(&str, Option<(PreReleaseKind, &str)>)> {
  let Some(dash) = rest.find('-') else {
    return is_digits(rest).then_some((rest, None));
  };
  let patch = &rest[..dash];
  let qualifier = &rest[dash + 1..];
  if !is_digits(patch) {
    return None;
  }
  let (kind, number) = PreReleaseKind::split(qualifier)?;
  if !is_digits(number) {
    return None;
  }
  Some((patch, Some((kind, number))))
}

fn is_digits(text: &str) -> bool {
  !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit())
}

/// Reports whether `left < right` as decimal numbers read from digit strings,
/// without converting to a native integer.
fn digits_less(left: &str, right: &str) -> bool {
  let left = left.trim_start_matches('0');
  let right = right.trim_start_matches('0');
  let left = if left.is_empty() { "0" } else { left };
  let right = if right.is_empty() { "0" } else { right };
  match left.len().cmp(&right.len()) {
    | core::cmp::Ordering::Less => true,
    | core::cmp::Ordering::Greater => false,
    | core::cmp::Ordering::Equal => left < right,
  }
}
