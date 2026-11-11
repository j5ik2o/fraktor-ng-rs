# module-wiring: 旧 UI テストとの対応

旧 `module-wiring-lint` の UI テストを、新チェッカーのルール `module-wiring` で判定した結果である。
期待は旧 lint の `<ケース>.stderr` から読む。`.stderr` が無いか空のケースはクリーンが期待である。
このファイルは `SYN_CHECK_BLESS=1 cargo test --test legacy_ui` が生成する。手で編集しない。

| ケース | 旧 lint の期待 | 新チェッカーの結果 | 判定 | 備考 |
|---|---|---|---|---|
| `alias_violation.rs` | 違反 1 件（`alias_violation.rs:9`） | 違反 1 件（`alias_violation.rs:9`） | 一致 |  |
| `allow_comment_ok.rs` | 違反 1 件（`allow_comment_ok.rs:6`） | 違反 1 件（`allow_comment_ok.rs:6`） | 一致 | フィクスチャ変更 `allow_comment_ok.rs`: 12 行目の旧コメント `// allow module_wiring::no_parent_reexport` を `// syn-check: allow(module-wiring)` に置き換えた。15 行目は行末の旧コメントで許可していたので、直前の空行（14 行目）を同じコメント指定に置き換えた。15 行目の行末コメントは残したが、新チェッカーは読まない |
| `brace_non_leaf_violation.rs` | 違反 2 件（`brace_non_leaf_violation.rs:6`、`brace_non_leaf_violation.rs:11`） | 違反 2 件（`brace_non_leaf_violation.rs:6`、`brace_non_leaf_violation.rs:11`） | 一致 |  |
| `brace_ok.rs` | クリーン | クリーン | 一致 |  |
| `glob_non_leaf_violation.rs` | 違反 2 件（`glob_non_leaf_violation.rs:6`、`glob_non_leaf_violation.rs:11`） | 違反 2 件（`glob_non_leaf_violation.rs:6`、`glob_non_leaf_violation.rs:11`） | 一致 |  |
| `glob_ok.rs` | クリーン | クリーン | 一致 |  |
| `grandparent_reexport.rs` | 違反 2 件（`grandparent_reexport.rs:6`、`grandparent_reexport.rs:11`） | 違反 2 件（`grandparent_reexport.rs:6`、`grandparent_reexport.rs:11`） | 一致 |  |
| `include_macro_violation.rs` | 違反 1 件（`include_macro_violation.rs:4`） | 違反 1 件（`include_macro_violation.rs:4`） | 一致 |  |
| `leaf_reexport_ok.rs` | クリーン | クリーン | 一致 |  |
| `mod_leaf_ok.rs` | クリーン | クリーン | 一致 |  |
| `non_direct_parent.rs` | 違反 2 件（`non_direct_parent.rs:6`、`non_direct_parent.rs:11`） | 違反 2 件（`non_direct_parent.rs:6`、`non_direct_parent.rs:11`） | 一致 |  |
| `other_test.rs` | クリーン | クリーン | 一致 |  |
| `path_attribute_directory.rs` | 違反 1 件（`path_attribute_directory.rs:4`） | 違反 1 件（`path_attribute_directory.rs:4`） | 一致 |  |
| `path_attribute_missing_cfg.rs` | 違反 1 件（`path_attribute_missing_cfg.rs:1`） | 違反 1 件（`path_attribute_missing_cfg.rs:1`） | 一致 |  |
| `path_attribute_missing_cfg_test.rs` | クリーン | クリーン | 一致 |  |
| `path_attribute_non_test_module.rs` | 違反 1 件（`path_attribute_non_test_module.rs:4`） | 違反 1 件（`path_attribute_non_test_module.rs:4`） | 一致 |  |
| `path_attribute_non_test_module_test.rs` | クリーン | クリーン | 一致 |  |
| `path_attribute_stem_mismatch.rs` | 違反 1 件（`path_attribute_stem_mismatch.rs:4`） | 違反 1 件（`path_attribute_stem_mismatch.rs:4`） | 一致 |  |
| `path_attribute_violation.rs` | 違反 1 件（`path_attribute_violation.rs:3`） | 違反 1 件（`path_attribute_violation.rs:3`） | 一致 |  |
| `prelude_leaf_ok.rs` | クリーン | クリーン | 一致 |  |
| `prelude_reexport_ok.rs` | 違反 1 件（`prelude_reexport_ok.rs:7`） | 違反 1 件（`prelude_reexport_ok.rs:7`） | 一致 |  |
| `pub_crate_violation.rs` | 違反 2 件（`pub_crate_violation.rs:6`、`pub_crate_violation.rs:11`） | 違反 2 件（`pub_crate_violation.rs:6`、`pub_crate_violation.rs:11`） | 一致 |  |
| `pub_mod_leaf_violation.rs` | 違反 1 件（`pub_mod_leaf_violation.rs:5`） | 違反 1 件（`pub_mod_leaf_violation.rs:5`） | 一致 |  |
| `sibling_test_hook_compound_cfg_ok.rs` | クリーン | クリーン | 一致 |  |
| `sibling_test_hook_compound_cfg_ok_test.rs` | クリーン | クリーン | 一致 |  |
| `sibling_test_hook_ok.rs` | クリーン | クリーン | 一致 |  |
| `sibling_test_hook_ok_test.rs` | クリーン | クリーン | 一致 |  |
| `special_prefix_violation.rs` | 違反 1 件（`special_prefix_violation.rs:9`） | 違反 1 件（`special_prefix_violation.rs:9`） | 一致 |  |

一致 28 件、差異あり（意図どおり） 0 件、不一致 0 件。
