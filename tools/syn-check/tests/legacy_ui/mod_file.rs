use crate::spec::LintSpec;

pub(crate) const SPEC: LintSpec = LintSpec {
  dir: "mod-file",
  rules: &["mod-file"],
  adaptations: &[(
    "auxiliary/suppressed/mod.rs",
    "クレート単位の `#![allow(no_mod_rs)]` はコメント指定に写せないので、このファイルの先頭に `// syn-check: allow-file(mod-file)` を足した",
  )],
  differences: &[],
};
