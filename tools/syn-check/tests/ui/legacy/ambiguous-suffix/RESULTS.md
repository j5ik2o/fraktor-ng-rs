# ambiguous-suffix: 旧 UI テストとの対応

旧 `ambiguous-suffix-lint` の UI テストを、新チェッカーのルール `ambiguous-suffix` で判定した結果である。
期待は旧 lint の `<ケース>.stderr` から読む。`.stderr` が無いか空のケースはクリーンが期待である。
このファイルは `SYN_CHECK_BLESS=1 cargo test --test legacy_ui` が生成する。手で編集しない。

| ケース | 旧 lint の期待 | 新チェッカーの結果 | 判定 | 備考 |
|---|---|---|---|---|
| `allow_attribute.rs` | クリーン | クリーン | 一致 | フィクスチャ変更 `allow_attribute.rs`: `#[allow(ambiguous_suffix)]` の直前の空行を `// syn-check: allow(ambiguous-suffix)` に置き換えた |
| `clean_names_ok.rs` | クリーン | クリーン | 一致 |  |
| `forbidden_all_names.rs` | 違反 27 件（`forbidden_all_names.rs:7`、`forbidden_all_names.rs:9`、`forbidden_all_names.rs:10`、`forbidden_all_names.rs:12`、`forbidden_all_names.rs:16`、`forbidden_all_names.rs:16`、`forbidden_all_names.rs:16`、`forbidden_all_names.rs:17`、`forbidden_all_names.rs:23`、`forbidden_all_names.rs:24`、`forbidden_all_names.rs:28`、`forbidden_all_names.rs:29`、`forbidden_all_names.rs:30`、`forbidden_all_names.rs:30`、`forbidden_all_names.rs:34`、`forbidden_all_names.rs:35`、`forbidden_all_names.rs:37`、`forbidden_all_names.rs:37`、`forbidden_all_names.rs:38`、`forbidden_all_names.rs:44`、`forbidden_all_names.rs:46`、`forbidden_all_names.rs:46`、`forbidden_all_names.rs:47`、`forbidden_all_names.rs:48`、`forbidden_all_names.rs:53`、`forbidden_all_names.rs:53`、`forbidden_all_names.rs:54`） | 違反 27 件（`forbidden_all_names.rs:7`、`forbidden_all_names.rs:9`、`forbidden_all_names.rs:10`、`forbidden_all_names.rs:12`、`forbidden_all_names.rs:16`、`forbidden_all_names.rs:16`、`forbidden_all_names.rs:16`、`forbidden_all_names.rs:17`、`forbidden_all_names.rs:23`、`forbidden_all_names.rs:24`、`forbidden_all_names.rs:28`、`forbidden_all_names.rs:29`、`forbidden_all_names.rs:30`、`forbidden_all_names.rs:30`、`forbidden_all_names.rs:34`、`forbidden_all_names.rs:35`、`forbidden_all_names.rs:37`、`forbidden_all_names.rs:37`、`forbidden_all_names.rs:38`、`forbidden_all_names.rs:44`、`forbidden_all_names.rs:46`、`forbidden_all_names.rs:46`、`forbidden_all_names.rs:47`、`forbidden_all_names.rs:48`、`forbidden_all_names.rs:53`、`forbidden_all_names.rs:53`、`forbidden_all_names.rs:54`） | 一致 |  |
| `forbidden_file_name_runtime.rs` | 違反 1 件（`forbidden_file_name_runtime.rs:5`） | 違反 1 件（`forbidden_file_name_runtime.rs:5`） | 一致 |  |
| `forbidden_module_names.rs` | 違反 2 件（`forbidden_module_names.rs:5`、`forbidden_module_names.rs:9`） | 違反 2 件（`forbidden_module_names.rs:5`、`forbidden_module_names.rs:9`） | 一致 |  |
| `forbidden_suffix.rs` | 違反 6 件（`forbidden_suffix.rs:5`、`forbidden_suffix.rs:7`、`forbidden_suffix.rs:12`、`forbidden_suffix.rs:16`、`forbidden_suffix.rs:18`、`forbidden_suffix.rs:20`） | 違反 6 件（`forbidden_suffix.rs:5`、`forbidden_suffix.rs:7`、`forbidden_suffix.rs:12`、`forbidden_suffix.rs:16`、`forbidden_suffix.rs:18`、`forbidden_suffix.rs:20`） | 一致 |  |
| `helper_names_test.rs` | クリーン | クリーン | 一致 |  |

一致 7 件、差異あり（意図どおり） 0 件、不一致 0 件。
