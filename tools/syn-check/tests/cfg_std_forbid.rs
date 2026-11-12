//! Cases for `cfg-std-forbid` that the old UI fixtures do not cover, and the
//! scope in the ng layout.

use std::path::PathBuf;

use syn_check::{layout::Layout, options::Options};

const ROOT: &str = "tests/ui/synthetic/cfg-std-forbid";

fn violations(layout: Layout, fixture: &str) -> Vec<(usize, String)> {
  let options = Options { layout, rules: Some(vec!["cfg-std-forbid".to_string()]) };
  let report = syn_check::run(&[PathBuf::from(ROOT).join(fixture)], &options).expect("run the checker");
  assert!(report.parse_failures.is_empty(), "{:?}", report.parse_failures);
  report.violations.into_iter().map(|violation| (violation.position.line, violation.message)).collect()
}

fn lines(layout: Layout, fixture: &str) -> Vec<usize> {
  violations(layout, fixture).into_iter().map(|(line, _)| line).collect()
}

const LEGACY_CORE: &str = "legacy/modules/actor-core/src";

#[test]
fn legacy_branches_follow_the_old_lint() {
  // 3〜6: `std` を名前、先頭 `::`、別名、途中のセグメントに持つ use。
  // 8: any(test, ..) は cfg(test) の例外にならない。21: フィールドの属性。
  // 26: 関数本体の cfg 属性と use（同じ行）。
  // 36: cfg(test) の impl メソッドはアイテムではないので、本体の use は例外にならない。
  // `use` 以外の位置の `std::` パス（29、30 行）は旧 lint と同じく見ない。
  assert_eq!(lines(Layout::Legacy, &format!("{LEGACY_CORE}/branches.rs")), vec![3, 4, 5, 6, 8, 21, 26, 26, 36]);
}

#[test]
fn messages_name_what_was_found() {
  let found = violations(Layout::Legacy, &format!("{LEGACY_CORE}/branches.rs"));
  assert!(found[0].1.starts_with("`use std::...` の使用が検出されました"));
  assert!(found[4].1.starts_with("`#[cfg(feature = \"std\")]` の使用が検出されました"));
}

#[test]
fn legacy_scope_comes_from_the_crate_root_lint_level() {
  assert_eq!(lines(Layout::Legacy, &format!("{LEGACY_CORE}/clock/system.rs")), vec![1]);
  assert_eq!(lines(Layout::Legacy, "legacy/modules/actor-adaptor-std/src/runtime.rs"), Vec::<usize>::new());
}

#[test]
fn legacy_scope_comes_from_the_file_lint_level() {
  assert_eq!(lines(Layout::Legacy, "legacy/standalone/warn_level.rs"), vec![3]);
  assert_eq!(lines(Layout::Legacy, "legacy/standalone/forbid_level.rs"), vec![2]);
  // 外側属性の `#[deny]` はアイテムのレベルで、ファイルの適用範囲は決めない。
  assert_eq!(lines(Layout::Legacy, "legacy/standalone/outer_deny.rs"), Vec::<usize>::new());
}

#[test]
fn a_cfg_test_file_attribute_exempts_the_file() {
  assert_eq!(lines(Layout::Legacy, "legacy/standalone/cfg_test_file.rs"), Vec::<usize>::new());
}

#[test]
fn files_declared_under_cfg_test_are_exempt() {
  for fixture in ["tests.rs", "clock/tests.rs", "clock/tests/nested.rs"] {
    assert_eq!(lines(Layout::Legacy, &format!("{LEGACY_CORE}/{fixture}")), Vec::<usize>::new(), "{fixture}");
  }
}

#[test]
fn ng_kernel_sources_are_checked() {
  assert_eq!(lines(Layout::Ng, "crates/actor/src/kernel.rs"), vec![4]);
  let found = violations(Layout::Ng, "crates/actor/src/kernel/clock.rs");
  assert_eq!(found.len(), 1);
  assert_eq!(found[0].0, 4);
  assert!(found[0].1.contains("adaptor モジュール"), "{}", found[0].1);
}

#[test]
fn ng_kernel_tests_declared_under_cfg_test_are_exempt() {
  assert_eq!(lines(Layout::Ng, "crates/actor/src/kernel/clock/tests.rs"), Vec::<usize>::new());
}

#[test]
fn ng_a_comment_suppresses_the_next_element_only() {
  assert_eq!(lines(Layout::Ng, "crates/actor/src/kernel/suppressed.rs"), vec![9]);
}

#[test]
fn ng_code_outside_the_kernel_is_not_checked() {
  for fixture in ["crates/actor/src/lib.rs", "crates/actor/src/adaptor/std_clock.rs", "crates/actor/src/kernel_util.rs"]
  {
    assert_eq!(lines(Layout::Ng, fixture), Vec::<usize>::new(), "{fixture}");
  }
}

#[test]
fn ng_does_not_need_a_lint_level() {
  assert_eq!(lines(Layout::Ng, "legacy/standalone/warn_level.rs"), Vec::<usize>::new());
}

#[test]
fn ng_sibling_test_hook_files_are_exempt() {
  // `#[cfg(test)] #[path = "timer_test.rs"] mod tests;` で宣言されたテストファイルは対象外。
  assert_eq!(lines(Layout::Ng, "crates/actor/src/kernel/timer_test.rs"), Vec::<usize>::new());
  // cfg(test) の無い `#[path]` 宣言は例外にならない。
  assert_eq!(lines(Layout::Ng, "crates/actor/src/kernel/ungated_test.rs"), vec![1]);
}
