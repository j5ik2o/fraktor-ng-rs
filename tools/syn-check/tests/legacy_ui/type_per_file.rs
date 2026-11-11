use crate::spec::LintSpec;

pub(crate) const SPEC: LintSpec = LintSpec {
    dir: "type-per-file",
    rules: &["type-per-file"],
    adaptations: &[(
        "allow_attribute.rs",
        "`#[allow(multiple_type_definitions)]` の直前の空行を `// syn-check: allow(type-per-file)` に置き換えた",
    )],
    differences: &[],
};
