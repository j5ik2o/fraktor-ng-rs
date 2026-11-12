//! Cases for `type-per-file` that the old UI fixtures do not cover.

use std::path::PathBuf;

use syn_check::{layout::Layout, options::Options};

fn violation_lines(fixture: &str) -> Vec<usize> {
  let path = PathBuf::from("tests/ui/synthetic/type-per-file").join(fixture);
  let options = Options { layout: Layout::Ng, rules: Some(vec!["type-per-file".to_string()]) };
  let report = syn_check::run(&[path], &options).expect("run the checker");
  assert!(report.parse_failures.is_empty(), "{:?}", report.parse_failures);
  report.violations.iter().map(|violation| violation.position.line).collect()
}

#[test]
fn cfg_switched_aliases_are_not_counted() {
  assert_eq!(violation_lines("cfg_alias.rs"), Vec::<usize>::new());
}

#[test]
fn cfg_exclusive_types_with_one_name_count_once() {
  assert_eq!(violation_lines("cfg_struct.rs"), Vec::<usize>::new());
  assert_eq!(violation_lines("cfg_module.rs"), Vec::<usize>::new());
}

#[test]
fn another_type_next_to_a_cfg_pair_is_still_reported() {
  assert_eq!(violation_lines("cfg_struct_and_enum.rs"), vec![11]);
}

#[test]
fn one_name_in_two_scopes_counts_twice() {
  assert_eq!(violation_lines("same_name_other_scope.rs"), vec![6]);
}

#[test]
fn types_in_inline_modules_and_blocks_count() {
  assert_eq!(violation_lines("nested_and_block.rs"), vec![4, 8]);
}

#[test]
fn only_plain_pub_structs_enums_and_traits_count() {
  assert_eq!(violation_lines("restricted_visibility.rs"), Vec::<usize>::new());
}

#[test]
fn test_files_are_ignored() {
  assert_eq!(violation_lines("tests/helpers.rs"), Vec::<usize>::new());
  assert_eq!(violation_lines("two_types_test.rs"), Vec::<usize>::new());
}
