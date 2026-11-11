# use-placement: 旧 UI テストとの対応

旧 `use-placement-lint` の UI テストを、新チェッカーのルール `use-placement` で判定した結果である。
期待は旧 lint の `<ケース>.stderr` から読む。`.stderr` が無いか空のケースはクリーンが期待である。
このファイルは `SYN_CHECK_BLESS=1 cargo test --test legacy_ui` が生成する。手で編集しない。

| ケース | 旧 lint の期待 | 新チェッカーの結果 | 判定 | 備考 |
|---|---|---|---|---|
| `cfg_mod_ok.rs` | クリーン | クリーン | 一致 |  |
| `cfg_test_mod_warn.rs` | 違反 1 件（`cfg_test_mod_warn.rs:7`） | 違反 1 件（`cfg_test_mod_warn.rs:7`） | 一致 |  |
| `extern_crate_ok.rs` | クリーン | クリーン | 一致 |  |
| `function_local_use.rs` | 違反 1 件（`function_local_use.rs:4`） | 違反 1 件（`function_local_use.rs:4`） | 一致 |  |
| `inline_mod_ok.rs` | クリーン | クリーン | 一致 |  |
| `scatter.rs` | 違反 3 件（`scatter.rs:5`、`scatter.rs:8`、`scatter.rs:10`） | 違反 3 件（`scatter.rs:5`、`scatter.rs:8`、`scatter.rs:10`） | 一致 |  |
| `type_alias_between.rs` | クリーン | クリーン | 一致 |  |

一致 7 件、差異あり（意図どおり） 0 件、不一致 0 件。
