# module-examples: 旧 UI テストとの対応

旧 `module-examples-lint` の UI テストを、新チェッカーのルール `module-examples` で判定した結果である。
期待は旧 lint の `<ケース>.stderr` から読む。`.stderr` が無いか空のケースはクリーンが期待である。
このファイルは `SYN_CHECK_BLESS=1 cargo test --test legacy_ui` が生成する。手で編集しない。

| ケース | 旧 lint の期待 | 新チェッカーの結果 | 判定 | 備考 |
|---|---|---|---|---|
| `allowed_showcases_example.rs` | クリーン | クリーン | 一致 |  |
| `forbidden_module_example.rs` | 違反 1 件（`auxiliary/modules/actor-adaptor/examples/classic_logging.rs:1`） | 違反 1 件（`auxiliary/modules/actor-adaptor/examples/classic_logging.rs:1`） | 一致 |  |

一致 2 件、差異あり（意図どおり） 0 件、不一致 0 件。
