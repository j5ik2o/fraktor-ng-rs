use crate::spec::LintSpec;

// 未移植。移植したら rules にルール id を入れ、必要なら adaptations と differences を書く。
pub(crate) const SPEC: LintSpec = LintSpec {
    dir: "port-adaptor-boundary",
    rules: &[],
    adaptations: &[],
    differences: &[],
};
