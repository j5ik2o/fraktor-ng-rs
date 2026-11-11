//! Cases for `module-examples` that the old UI fixtures do not cover.
//!
//! Each case is a directory that holds the layout's root (`modules` or
//! `crates`). The test runs the checker from inside that directory, so the
//! checked paths start with `modules/` or `crates/`, as they do when the
//! checker runs from a repository root.

use std::{
    env,
    path::{Path, PathBuf},
    sync::{Mutex, PoisonError},
};

use syn_check::{layout::Layout, options::Options, report::Report};

/// The working directory is shared by the whole test process.
static WORKING_DIRECTORY: Mutex<()> = Mutex::new(());

/// Restores the previous working directory, also when the test panics.
struct Restore(PathBuf);

impl Drop for Restore {
    fn drop(&mut self) {
        env::set_current_dir(&self.0).expect("restore the working directory");
    }
}

fn run_in(case: &str, root: &str, layout: Layout) -> Report {
    let _lock = WORKING_DIRECTORY
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/ui/synthetic/module-examples")
        .join(case);
    let _restore = Restore(env::current_dir().expect("read the working directory"));
    env::set_current_dir(directory).expect("enter the case directory");
    let options = Options {
        layout,
        rules: Some(vec!["module-examples".to_string()]),
    };
    let report = syn_check::run(&[PathBuf::from(root)], &options).expect("run the checker");
    assert!(
        report.parse_failures.is_empty(),
        "{:?}",
        report.parse_failures
    );
    report
}

/// The path and the line of each violation.
fn violations(case: &str, root: &str, layout: Layout) -> Vec<(String, usize)> {
    run_in(case, root, layout)
        .violations
        .iter()
        .map(|violation| {
            (
                violation.path.to_string_lossy().replace('\\', "/"),
                violation.position.line,
            )
        })
        .collect()
}

fn at(path: &str) -> (String, usize) {
    (path.to_string(), 1)
}

#[test]
fn ng_layout_flags_files_under_crate_examples() {
    // crates/<crate>/src/examples と crates/examples は、examples が <crate> の直下ではないので対象外。
    assert_eq!(
        violations("ng_violation", "crates", Layout::Ng),
        vec![
            at("crates/actor-core/examples/nested/deep/main.rs"),
            at("crates/actor-core/examples/ping.rs"),
        ]
    );
}

#[test]
fn ng_layout_does_not_widen_the_rule() {
    // modules/ と showcases/ は ng の対象ではない。
    assert_eq!(
        violations("ng_violation", "modules", Layout::Ng),
        Vec::<(String, usize)>::new()
    );
    assert_eq!(
        violations("ng_violation", "showcases", Layout::Ng),
        Vec::<(String, usize)>::new()
    );
}

#[test]
fn a_suppression_comment_clears_a_file() {
    let report = run_in("ng_violation", "crates", Layout::Ng);
    let suppressed: Vec<_> = report
        .suppressions
        .iter()
        .map(|record| {
            (
                record.path.to_string_lossy().into_owned(),
                record.file_scope,
                record.used,
            )
        })
        .collect();
    assert_eq!(
        suppressed,
        vec![(
            "crates/actor-core/examples/suppressed.rs".to_string(),
            true,
            true
        )]
    );
}

#[test]
fn a_file_without_items_is_not_flagged() {
    assert_eq!(
        violations("ng_no_items", "crates", Layout::Ng),
        Vec::<(String, usize)>::new()
    );
}

#[test]
fn legacy_layout_flags_files_under_module_examples() {
    assert_eq!(
        violations("legacy_violation", "modules", Layout::Legacy),
        vec![
            at("modules/actor-adaptor/examples/nested/main.rs"),
            at("modules/actor-adaptor/examples/ping.rs"),
        ]
    );
}

#[test]
fn legacy_layout_does_not_flag_crates_or_showcases() {
    assert_eq!(
        violations("legacy_violation", "crates", Layout::Legacy),
        Vec::<(String, usize)>::new()
    );
    assert_eq!(
        violations("legacy_violation", "showcases", Layout::Legacy),
        Vec::<(String, usize)>::new()
    );
}

#[test]
fn the_result_does_not_depend_on_the_form_of_the_path() {
    let from_a_relative_root = violations("legacy_violation", "modules", Layout::Legacy);
    let from_the_current_directory = violations("legacy_violation", ".", Layout::Legacy);
    let stripped: Vec<_> = from_the_current_directory
        .iter()
        .map(|(path, line)| (path.trim_start_matches("./").to_string(), *line))
        .filter(|(path, _)| path.starts_with("modules/"))
        .collect();
    assert_eq!(from_a_relative_root, stripped);

    let absolute = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/ui/synthetic/module-examples/legacy_violation/modules");
    let report = run_in(
        "legacy_violation",
        absolute.to_str().expect("utf-8 path"),
        Layout::Legacy,
    );
    assert_eq!(report.violations.len(), from_a_relative_root.len());
}
