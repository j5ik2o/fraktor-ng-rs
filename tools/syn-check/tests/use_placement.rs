//! Cases for `use-placement` that the old UI fixtures do not cover.

use std::path::PathBuf;

use syn_check::{layout::Layout, options::Options};

const SAMPLE_CRATE: &str = "tests/ui/synthetic/use-placement/crates/sample";

fn violation_lines(path: &str) -> Vec<usize> {
    let options = Options {
        layout: Layout::Ng,
        rules: Some(vec!["use-placement".to_string()]),
    };
    let report = syn_check::run(&[PathBuf::from(path)], &options).expect("run the checker");
    report
        .violations
        .iter()
        .map(|violation| violation.position.line)
        .collect()
}

#[test]
fn nested_modules_and_impl_bodies_are_checked() {
    // 6: インラインモジュールの中で fn の後にある use。11: メソッド本体の use。
    // 19: impl の後にある use。type alias と extern crate は順序に影響しない。
    assert_eq!(
        violation_lines("tests/ui/synthetic/use-placement/nested_module.rs"),
        vec![6, 11, 19]
    );
}

/// `tests/` の下、`benches/` の下、`*_test.rs` は対象外。3 つのファイルは
/// `src/tests.rs` と同じ内容で、対象にすると 3 行目と 6 行目が違反になる。
#[test]
fn test_and_bench_files_are_out_of_scope() {
    for file in [
        "tests/port.rs",
        "benches/throughput.rs",
        "src/widget_test.rs",
    ] {
        assert_eq!(
            violation_lines(&format!("{SAMPLE_CRATE}/{file}")),
            Vec::<usize>::new(),
            "{file}"
        );
    }
}

/// 対象外にするのは上の 3 つのパスだけ。`tests.rs` という名前のファイルは対象のまま。
/// 3: fn の後にある use。6: 関数本体の use。
#[test]
fn a_file_named_tests_rs_is_still_checked() {
    assert_eq!(
        violation_lines(&format!("{SAMPLE_CRATE}/src/tests.rs")),
        vec![3, 6]
    );
}

/// lib 側のファイルにある `#[cfg(test)]` のコードは対象のまま（旧 lint も syn で
/// ファイル全体を読み直していたので、検査していた）。
/// 18: `#[cfg(test)]` のインラインモジュールの中にある、関数本体の use。
/// 24: 型と impl の後に置かれた `#[cfg(test)] mod late_hook;`。属性の行で報告する。
/// 冒頭の `#[cfg(test)] #[path = "widget_test.rs"] mod tests;` は違反にならない。
#[test]
fn cfg_test_code_in_a_lib_file_is_still_checked() {
    assert_eq!(
        violation_lines(&format!("{SAMPLE_CRATE}/src/widget.rs")),
        vec![18, 24]
    );
}
