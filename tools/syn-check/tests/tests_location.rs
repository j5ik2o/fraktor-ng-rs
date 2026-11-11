//! Synthetic cases for `tests-location`.
//!
//! The old `tests-location-lint` shipped no UI tests. These fixtures are
//! written from a line-by-line reading of its `lib.rs`; each test names the
//! old function and branch it covers.

use std::{path::Path, process::Command};

use serde_json::Value;

const FIXTURES: &str = "tests/ui/synthetic/tests-location";

const INLINE_MODULE: &str = "inline-module";
const CFG_TEST_ITEM: &str = "cfg-test-item";
const TEST_ATTRIBUTE: &str = "test-attribute";
const INNER_CFG_TEST: &str = "inner-cfg-test";

type Finding = (usize, usize, &'static str);

/// The line, column, and kind of each violation in `fixture`.
///
/// The fixtures live under this crate's `tests` directory, and `is_tests_path`
/// treats any path with a `tests` component as a test file. So the checker is
/// started inside the fixture directory and given a path relative to it.
/// Both layouts must agree, since the rule does not look at the layout.
fn findings(fixture: &str) -> Vec<Finding> {
    let ng = run(fixture, "ng");
    assert_eq!(ng, run(fixture, "legacy"), "{fixture}: layouts disagree");
    ng
}

fn run(fixture: &str, layout: &str) -> Vec<Finding> {
    let report = check(fixture, layout);
    assert_eq!(report["parse_failures"], Value::Array(Vec::new()));
    report["violations"]
        .as_array()
        .expect("violations")
        .iter()
        .map(|violation| {
            assert_eq!(violation["rule"], "tests-location");
            (
                as_usize(&violation["line"]),
                as_usize(&violation["column"]),
                kind(violation["message"].as_str().expect("message")),
            )
        })
        .collect()
}

fn check(fixture: &str, layout: &str) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_syn-check"))
        .current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join(FIXTURES))
        .args(["--layout", layout, "--rules", "tests-location", "--format"])
        .args(["json", fixture])
        .output()
        .expect("start syn-check");
    assert!(
        matches!(output.status.code(), Some(0 | 1)),
        "{fixture}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("JSON report")
}

fn as_usize(value: &Value) -> usize {
    usize::try_from(value.as_u64().expect("number")).expect("fits in usize")
}

fn kind(message: &str) -> &'static str {
    [
        ("`mod tests { ... }`", INLINE_MODULE),
        ("本番モジュールに", CFG_TEST_ITEM),
        ("テスト関数は", TEST_ATTRIBUTE),
        ("テストファイル内で", INNER_CFG_TEST),
    ]
    .into_iter()
    .find(|(prefix, _)| message.starts_with(prefix))
    .map(|(_, kind)| kind)
    .unwrap_or_else(|| panic!("unknown message: {message}"))
}

// find_inline_test_modules: `mod tests` の後の空白（改行を含む）を飛ばして `{` があれば一致。
// 可視性と cfg 属性は見ないので、範囲の先頭は `mod` になる。`mod tests;` と、
// `tests` の後に識別子が続く `mod tests_util` と `mod testsuite` は一致しない。
#[test]
fn find_inline_test_modules_reports_mod_tests_with_a_body() {
    assert_eq!(
        findings("src/inline_module.rs"),
        vec![
            (3, 1, INLINE_MODULE),
            (7, 12, INLINE_MODULE),
            (13, 5, INLINE_MODULE),
        ]
    );
}

// find_cfg_test_items の push 分岐: hook でも tests モジュールの宣言でもない `#[cfg(test)]` は、
// 付いている先（use、関数、フィールド、メソッド、別名のモジュール）を問わず違反。
// 最後の `#[cfg(test)] #[allow] mod tests {}` は、宣言の判定が直後の `#[allow]` で外れるので、
// cfg(test) と inline module の二重報告になる（旧実装のまま）。
#[test]
fn find_cfg_test_items_reports_every_other_cfg_test() {
    assert_eq!(
        findings("src/cfg_test_items.rs"),
        vec![
            (3, 1, CFG_TEST_ITEM),
            (6, 1, CFG_TEST_ITEM),
            (10, 5, CFG_TEST_ITEM),
            (15, 5, CFG_TEST_ITEM),
            (19, 1, CFG_TEST_ITEM),
            (22, 1, CFG_TEST_ITEM),
            (24, 1, INLINE_MODULE),
        ]
    );
}

// is_sibling_test_module_hook が真になる形（空白の有無、`pub`、`pub(crate)`、`pub(in ...)`、
// パスにディレクトリを含む `*_test.rs`）と、is_tests_module_declaration が真になる形
// （`#[path]` なしの `mod tests;`、`pub(super)`、`#[path]` が `#[cfg(test)]` より前）は違反にならない。
// 末尾の `#[cfg(test)] mod tests {}` は宣言の判定で cfg(test) 側が飛ばされ、inline module だけが残る。
#[test]
fn sibling_hooks_and_tests_module_declarations_are_exempt() {
    assert_eq!(findings("src/hook.rs"), vec![(28, 1, INLINE_MODULE)]);
}

// is_sibling_test_module_hook が偽になる形: パスが `_tests.rs` で終わる、モジュール名が tests でない、
// `mod tests ;`（`mod tests;` の前方一致に失敗）、`#[path]` が cfg(test) の直後でない、
// `mod tests_util;`。どれも is_tests_module_declaration も偽なので cfg(test) として報告される。
#[test]
fn hooks_that_do_not_match_exactly_are_cfg_test_items() {
    assert_eq!(
        findings("src/hook_mismatch.rs"),
        vec![
            (2, 1, CFG_TEST_ITEM),
            (6, 1, CFG_TEST_ITEM),
            (10, 1, CFG_TEST_ITEM),
            (14, 1, CFG_TEST_ITEM),
            (19, 1, CFG_TEST_ITEM),
        ]
    );
}

// find_cfg_test_items は `#[cfg(test)]` の完全一致だけを探す。`all(test, ..)`、`any(test, ..)`、
// `not(test)`、`cfg_attr(test, ..)`、`cfg( test )` は一致しない。本番ファイルの `#![cfg(test)]` は
// analyze_source_file のどの検出にもかからない。複合 cfg の hook（`#[cfg(all(test, unix))]
// #[path = "..._test.rs"] mod tests;`）もそのまま通る。
#[test]
fn compound_cfg_is_not_a_cfg_test_item() {
    assert_eq!(findings("src/composite_cfg.rs"), Vec::<Finding>::new());
}

// find_direct_test_attributes: 属性名（`(` か空白の前まで）が `test` か `::test` で終われば一致。
// `test_case`、`rstest`、`should_panic`、`testing`、`my::test_util` は一致しない。
#[test]
fn find_direct_test_attributes_reports_test_and_path_test() {
    assert_eq!(
        findings("src/direct_attribute.rs"),
        vec![
            (3, 1, TEST_ATTRIBUTE),
            (6, 1, TEST_ATTRIBUTE),
            (9, 1, TEST_ATTRIBUTE),
            (12, 1, TEST_ATTRIBUTE),
            (15, 1, TEST_ATTRIBUTE),
        ]
    );
}

// 旧実装の癖: テキスト走査なので doc コメント、通常コメント、文字列の中も拾う。
// 文字列の中の `#[cfg(test)] mod tests {` は、宣言の判定で cfg(test) 側が飛ばされる。
// find_direct_test_attributes は一致しなかった属性の本体を飛ばすので、
// `#[doc = "#[test]"]` の中の `#[test]` は拾わない。
#[test]
fn text_scan_matches_inside_comments_and_strings() {
    assert_eq!(
        findings("src/text_scan.rs"),
        vec![
            (4, 5, TEST_ATTRIBUTE),
            (8, 4, INLINE_MODULE),
            (9, 4, CFG_TEST_ITEM),
            (10, 37, INLINE_MODULE),
        ]
    );
}

// find_direct_test_attributes の break 分岐: find_attribute_end が閉じ括弧を見つけられないと、
// その後ろの属性は見ない。
#[test]
fn unclosed_attribute_stops_the_test_attribute_scan() {
    assert_eq!(findings("src/unbalanced.rs"), Vec::<Finding>::new());
}

// span_for_range の先頭: 列は行頭からの文字数で数える（マルチバイト文字を 1 と数える）。
#[test]
fn report_position_is_the_start_of_the_matched_range() {
    assert_eq!(findings("src/column.rs"), vec![(2, 15, TEST_ATTRIBUTE)]);
}

// is_tests_path が偽になる名前: `test.rs`、`tests_util.rs`、`tests` と一致しないディレクトリ。
#[test]
fn near_miss_test_paths_are_production_files() {
    for fixture in ["src/test.rs", "src/tests_util.rs", "src/my_tests/helper.rs"] {
        assert_eq!(findings(fixture), vec![(2, 1, TEST_ATTRIBUTE)], "{fixture}");
    }
}

// is_tests_path が真になる 4 通り（`tests.rs`、`*_test.rs`、`*_tests.rs`、`tests` ディレクトリ）では、
// analyze_tests_file の find_inner_cfg_test_attributes だけが働く。
#[test]
fn every_test_path_form_reports_inner_cfg_test() {
    for fixture in [
        "src/tests.rs",
        "src/hook_tests.rs",
        "src/tests/helper.rs",
        "tests/integration.rs",
    ] {
        assert_eq!(findings(fixture), vec![(1, 1, INNER_CFG_TEST)], "{fixture}");
    }
}

// analyze_tests_file: 空白を除いて `cfg(test)` に一致する内部属性だけが違反。
// `all(test, ..)`、`not(test)`、`allow` は違反にならない。インラインの `mod tests {}`、
// `#[cfg(test)]` のアイテム、`#[test]` はテストファイルでは検査しない。
#[test]
fn find_inner_cfg_test_attributes_in_a_test_file() {
    assert_eq!(
        findings("src/hook_test.rs"),
        vec![(2, 1, INNER_CFG_TEST), (3, 1, INNER_CFG_TEST)]
    );
    assert_eq!(findings("src/clean_test.rs"), Vec::<Finding>::new());
}

// fraktor-ng-rs の配置: `crates/<crate>/tests/<node>.rs` は `tests` ディレクトリなのでテストファイル、
// `crates/<crate>/src/**` は本番ファイル。述語を変えずにそのまま当てはまる。
#[test]
fn ng_crate_layout_needs_no_other_predicate() {
    assert_eq!(
        findings("crates/node/tests/actor_cell.rs"),
        vec![(2, 1, INNER_CFG_TEST)]
    );
    assert_eq!(
        findings("crates/node/src/actor_cell.rs"),
        vec![(8, 1, INLINE_MODULE), (9, 5, TEST_ATTRIBUTE)]
    );
}

// 抑制コメントは次の要素全体にかかり、モジュールの中の報告も消す。要素の外は消さない。
#[test]
fn allow_comment_covers_the_whole_module() {
    assert_eq!(findings("src/suppressed.rs"), vec![(12, 1, TEST_ATTRIBUTE)]);
    let report = check("src/suppressed.rs", "ng");
    assert_eq!(report["suppressions"][0]["used"], true);
}

#[test]
fn allow_file_comment_covers_the_file() {
    assert_eq!(findings("src/suppressed_file.rs"), Vec::<Finding>::new());
}

// ルールは `#[allow]` 属性を読まない。旧 lint 名を書いても抑制にならない。
#[test]
fn allow_attribute_does_not_suppress() {
    assert_eq!(
        findings("src/allow_attribute.rs"),
        vec![(2, 1, TEST_ATTRIBUTE)]
    );
}
