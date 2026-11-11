# rustdoc: 旧 UI テストとの対応

旧 `rustdoc-lint` の UI テストを、新チェッカーのルール `rustdoc` で判定した結果である。
期待は旧 lint の `<ケース>.stderr` から読む。`.stderr` が無いか空のケースはクリーンが期待である。
このファイルは `SYN_CHECK_BLESS=1 cargo test --test legacy_ui` が生成する。手で編集しない。

| ケース | 旧 lint の期待 | 新チェッカーの結果 | 判定 | 備考 |
|---|---|---|---|---|
| `ignore_fence.rs` | 違反 1 件（`ignore_fence.rs:3`） | 違反 1 件（`ignore_fence.rs:3`） | 一致 |  |
| `non_english.rs` | 違反 1 件（`non_english.rs:3`） | 違反 1 件（`non_english.rs:3`） | 一致 |  |

一致 2 件、差異あり（意図どおり） 0 件、不一致 0 件。
