use crate::spec::LintSpec;

pub(crate) const SPEC: LintSpec = LintSpec {
    dir: "cfg-std-forbid",
    rules: &["cfg-std-forbid"],
    adaptations: &[(
        "core_allow.rs",
        "`#[allow(cfg_std_forbid)]` の直前の空行を `// syn-check: allow(cfg-std-forbid)` に置き換えた",
    )],
    differences: &[],
};
