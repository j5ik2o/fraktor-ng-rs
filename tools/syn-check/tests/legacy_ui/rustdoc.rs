use crate::spec::LintSpec;

pub(crate) const SPEC: LintSpec = LintSpec {
    dir: "rustdoc",
    rules: &["rustdoc"],
    adaptations: &[],
    differences: &[],
};
