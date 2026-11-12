/// How one old lint's UI fixtures map to the new checker.
pub(crate) struct LintSpec {
  /// The fixture directory under `tests/ui/legacy`.
  pub dir: &'static str,
  /// The rule ids that replace the old lint. Empty while the lint is not ported.
  pub rules: &'static [&'static str],
  /// Fixture edits made after the verbatim import: the path under `dir`, and what changed.
  pub adaptations: &'static [(&'static str, &'static str)],
  /// Cases where the new checker intentionally differs from the old `.stderr`.
  pub differences: &'static [Difference],
}

/// An intended difference from the old lint for one case.
pub(crate) struct Difference {
  /// The case file name, for example `foo.rs`.
  pub case: &'static str,
  /// What the new checker reports instead: the path under the fixture directory, and the line.
  pub expected: &'static [(&'static str, usize)],
  pub reason: &'static str,
}
