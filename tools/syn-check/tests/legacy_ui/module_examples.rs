use crate::spec::LintSpec;

pub(crate) const SPEC: LintSpec = LintSpec {
    dir: "module-examples",
    rules: &["module-examples"],
    adaptations: &[],
    differences: &[],
};
