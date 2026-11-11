//! Cases for `port-adaptor-boundary` that the old UI fixtures do not cover,
//! and the scope in the ng layout.

use std::path::PathBuf;

use syn_check::{layout::Layout, options::Options};

const ROOT: &str = "tests/ui/synthetic/port-adaptor-boundary";

fn violations(layout: Layout, fixture: &str) -> Vec<(usize, String)> {
    let options = Options {
        layout,
        rules: Some(vec!["port-adaptor-boundary".to_string()]),
    };
    let report =
        syn_check::run(&[PathBuf::from(ROOT).join(fixture)], &options).expect("run the checker");
    assert!(
        report.parse_failures.is_empty(),
        "{:?}",
        report.parse_failures
    );
    report
        .violations
        .into_iter()
        .map(|violation| (violation.position.line, violation.message))
        .collect()
}

fn lines(layout: Layout, fixture: &str) -> Vec<usize> {
    violations(layout, fixture)
        .into_iter()
        .map(|(line, _)| line)
        .collect()
}

const LEGACY_ADAPTOR: &str = "legacy/modules/cluster-adaptor-std/src";

#[test]
fn legacy_full_paths_aliases_and_generic_arguments_are_classified() {
    let found = violations(Layout::Legacy, &format!("{LEGACY_ADAPTOR}/branches.rs"));
    let summary: Vec<(usize, bool, &str)> = found
        .iter()
        .map(|(line, message)| {
            let type_name = message.split('`').nth(1).unwrap_or_default();
            (*line, message.contains("同名 wrapper"), type_name)
        })
        .collect();
    // 7: フルパスの Api 型。11: 別名の import は元の名前で判定する。
    // 15: `Core<struct>` の別名で、元の名前から Api を除くと struct 名になる。
    // 19: ジェネリクスの引数の中も見る。`Clean` のフィールドはどれも対象外。
    assert_eq!(
        summary,
        vec![
            (7, false, "ClusterApi"),
            (11, false, "GrainApi"),
            (15, true, "ClusterApi"),
            (19, false, "ClusterApi"),
        ]
    );
}

#[test]
fn legacy_only_top_level_public_structs_are_checked() {
    assert_eq!(
        lines(Layout::Legacy, &format!("{LEGACY_ADAPTOR}/not_checked.rs")),
        Vec::<usize>::new()
    );
}

#[test]
fn a_field_is_reported_at_its_first_attribute() {
    assert_eq!(
        lines(Layout::Legacy, &format!("{LEGACY_ADAPTOR}/doc_field.rs")),
        vec![4]
    );
}

#[test]
fn a_comment_before_the_struct_suppresses_all_its_fields() {
    assert_eq!(
        lines(Layout::Legacy, &format!("{LEGACY_ADAPTOR}/suppressed.rs")),
        vec![10]
    );
}

#[test]
fn legacy_code_outside_adaptor_std_is_not_checked() {
    assert_eq!(
        lines(
            Layout::Legacy,
            "legacy/modules/cluster-core/src/not_adaptor.rs"
        ),
        Vec::<usize>::new()
    );
}

#[test]
fn ng_adaptor_root_file_is_checked() {
    let found = violations(Layout::Ng, "crates/cluster/src/adaptor.rs");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].0, 4);
    assert!(
        found[0]
            .1
            .starts_with("adaptor の public struct が kernel concrete API `ClusterApi`"),
        "{}",
        found[0].1
    );
}

#[test]
fn ng_imports_from_this_and_other_kernels_are_core() {
    // 5, 9, 13: 同じクレートの kernel の同名 wrapper（別名とフルパス）。
    // 16: ほかのクレートの kernel の Api 型（import とフルパスの 2 フィールド）。
    assert_eq!(
        lines(Layout::Ng, "crates/cluster/src/adaptor/kernel_imports.rs"),
        vec![5, 9, 13, 16, 16]
    );
}

#[test]
fn ng_port_traits_adaptor_types_and_old_core_crates_are_allowed() {
    assert_eq!(
        lines(Layout::Ng, "crates/cluster/src/adaptor/clean.rs"),
        Vec::<usize>::new()
    );
}

#[test]
fn ng_kernel_code_is_not_checked() {
    assert_eq!(
        lines(Layout::Ng, "crates/cluster/src/kernel/same_content.rs"),
        Vec::<usize>::new()
    );
}

#[test]
fn the_layouts_do_not_share_scopes() {
    assert_eq!(
        lines(Layout::Ng, &format!("{LEGACY_ADAPTOR}/doc_field.rs")),
        Vec::<usize>::new()
    );
    assert_eq!(
        lines(Layout::Legacy, "crates/cluster/src/adaptor.rs"),
        Vec::<usize>::new()
    );
}
