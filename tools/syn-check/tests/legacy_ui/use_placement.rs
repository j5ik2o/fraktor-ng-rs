use crate::spec::LintSpec;

pub(crate) const SPEC: LintSpec = LintSpec {
    dir: "use-placement",
    rules: &["use-placement"],
    adaptations: &[],
    differences: &[],
};
