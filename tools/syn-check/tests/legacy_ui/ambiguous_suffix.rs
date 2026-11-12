use crate::spec::LintSpec;

pub(crate) const SPEC: LintSpec = LintSpec {
  dir: "ambiguous-suffix",
  rules: &["ambiguous-suffix"],
  adaptations: &[(
    "allow_attribute.rs",
    "`#[allow(ambiguous_suffix)]` の直前の空行を `// syn-check: allow(ambiguous-suffix)` に置き換えた",
  )],
  differences: &[],
};
