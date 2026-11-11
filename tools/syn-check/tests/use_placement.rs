//! Cases for `use-placement` that the old UI fixtures do not cover.

use std::path::PathBuf;

use syn_check::{layout::Layout, options::Options};

#[test]
fn nested_modules_and_impl_bodies_are_checked() {
    let options = Options {
        layout: Layout::Ng,
        rules: Some(vec!["use-placement".to_string()]),
    };
    let path = PathBuf::from("tests/ui/synthetic/use-placement/nested_module.rs");
    let report = syn_check::run(&[path], &options).expect("run the checker");
    let lines: Vec<usize> = report
        .violations
        .iter()
        .map(|violation| violation.position.line)
        .collect();
    // 6: インラインモジュールの中で fn の後にある use。11: メソッド本体の use。
    // 19: impl の後にある use。type alias と extern crate は順序に影響しない。
    assert_eq!(lines, vec![6, 11, 19]);
}
