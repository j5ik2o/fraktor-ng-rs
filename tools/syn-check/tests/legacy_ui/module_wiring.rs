use crate::spec::LintSpec;

pub(crate) const SPEC: LintSpec = LintSpec {
  dir: "module-wiring",
  rules: &["module-wiring"],
  adaptations: &[(
    "allow_comment_ok.rs",
    "12 行目の旧コメント `// allow module_wiring::no_parent_reexport` を `// syn-check: allow(module-wiring)` に置き換えた。15 行目は行末の旧コメントで許可していたので、直前の空行（14 行目）を同じコメント指定に置き換えた。15 行目の行末コメントは残したが、新チェッカーは読まない",
  )],
  differences: &[],
};
