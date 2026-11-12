//! Cases for `ambiguous-suffix` that the old UI fixtures do not cover.

use std::path::PathBuf;

use syn_check::{layout::Layout, options::Options};

/// The line and the leading "`name` (kind)" of each violation.
fn findings(fixture: &str) -> Vec<(usize, String)> {
  let path = PathBuf::from("tests/ui/synthetic/ambiguous-suffix").join(fixture);
  let options = Options { layout: Layout::Ng, rules: Some(vec!["ambiguous-suffix".to_string()]) };
  let report = syn_check::run(&[path], &options).expect("run the checker");
  assert!(report.parse_failures.is_empty(), "{:?}", report.parse_failures);
  report
    .violations
    .iter()
    .map(|violation| {
      let label = violation.message.split_once(" は曖昧").map_or(violation.message.as_str(), |(label, _)| label);
      (violation.position.line, label.to_string())
    })
    .collect()
}

fn expect(fixture: &str, expected: &[(usize, &str)]) {
  let expected: Vec<(usize, String)> = expected.iter().map(|(line, label)| (*line, label.to_string())).collect();
  assert_eq!(findings(fixture), expected, "{fixture}");
}

fn lines(fixture: &str) -> Vec<usize> {
  findings(fixture).into_iter().map(|(line, _)| line).collect()
}

#[test]
fn camel_names_match_the_suffix_case_sensitively() {
  expect(
    "style_boundaries.rs",
    &[
      (1, "`Manager` (struct)"),
      (2, "`TaskManager` (struct)"),
      (8, "`task_manager` (function)"),
      (9, "`Task_Manager` (function)"),
      (10, "`manager` (function)"),
      (12, "`service` (function)"),
      (13, "`TASK_ENGINE` (const)"),
    ],
  );
}

#[test]
fn bindings_are_labelled_by_where_they_appear() {
  expect(
    "binding_kinds.rs",
    &[
      (2, "`job_manager` (parameter)"),
      (3, "`job_service` (variable)"),
      (9, "`item_engine` (variable)"),
      (10, "`item_util` (variable)"),
      (14, "`inner_runtime` (variable)"),
      (17, "`ref_manager` (variable)"),
      (20, "`Worker_service` (variable)"),
      (24, "`FIXED_SERVICE` (const)"),
      (27, "`Manager` (variant)"),
    ],
  );
}

#[test]
fn foreign_functions_are_not_checked_but_pointer_parameters_inside_are() {
  expect("foreign_and_pointers.rs", &[(2, "`inner_engine` (function pointer parameter)")]);
}

#[test]
fn generics_fields_and_associated_items_are_checked() {
  expect(
    "generics_and_members.rs",
    &[
      (1, "`'job_service` (lifetime)"),
      (1, "`SlotRuntime` (generic parameter)"),
      (1, "`SLOT_UTIL` (const parameter)"),
      (2, "`cache_manager` (field)"),
      (11, "`OutputEngine` (associated type)"),
      (12, "`LIMIT_FACADE` (associated const)"),
      (13, "`run_service` (trait method)"),
      (16, "`'call_manager` (lifetime)"),
    ],
  );
}

#[test]
fn items_in_modules_and_function_bodies_are_checked() {
  expect(
    "nested_items.rs",
    &[
      (1, "`inner_util` (module)"),
      (2, "`RawEngine` (union)"),
      (7, "`JobEngine` (trait alias)"),
      (10, "`LocalManager` (struct)"),
      (11, "`LocalService` (type alias)"),
      (12, "`nested_engine` (function)"),
      (13, "`make_util` (macro)"),
    ],
  );
}

#[test]
fn tokens_inside_a_macro_invocation_are_not_seen() {
  assert_eq!(lines("macro_arguments.rs"), Vec::<usize>::new());
}

#[test]
fn a_comment_covers_the_next_element_and_everything_inside_it() {
  expect(
    "suppressed.rs",
    &[(4, "`OtherManager` (struct)"), (9, "`other_manager` (field)"), (20, "`request_runtime` (variable)")],
  );
}

#[test]
fn a_file_name_is_reported_at_the_first_item() {
  assert_eq!(lines("file_name/job_runtime.rs"), vec![2]);
  assert_eq!(lines("file_name/runtime.rs"), vec![3]);
  assert_eq!(lines("file_name/attrs_service.rs"), vec![3]);
}

#[test]
fn a_file_name_is_matched_case_sensitively_on_a_word_boundary() {
  assert_eq!(lines("file_name/HttpRuntime.rs"), Vec::<usize>::new());
  assert_eq!(lines("file_name/runtime_extension.rs"), Vec::<usize>::new());
}

#[test]
fn a_file_without_items_has_no_file_name_violation() {
  assert_eq!(lines("file_name/empty_manager.rs"), Vec::<usize>::new());
}

#[test]
fn allow_file_suppresses_the_file_name() {
  assert_eq!(lines("file_name/suppressed_engine.rs"), Vec::<usize>::new());
}

#[test]
fn test_and_target_paths_are_ignored() {
  for fixture in [
    "file_name/tests.rs",
    "file_name/cache_manager_test.rs",
    "file_name/cache_manager_tests.rs",
    "file_name/tests/helper_manager.rs",
    "file_name/target/engine.rs",
  ] {
    assert_eq!(lines(fixture), Vec::<usize>::new(), "{fixture}");
  }
}
