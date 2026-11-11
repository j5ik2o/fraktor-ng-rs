# type-per-file: 旧 UI テストとの対応

旧 `type-per-file-lint` の UI テストを、新チェッカーのルール `type-per-file` で判定した結果である。
期待は旧 lint の `<ケース>.stderr` から読む。`.stderr` が無いか空のケースはクリーンが期待である。
このファイルは `SYN_CHECK_BLESS=1 cargo test --test legacy_ui` が生成する。手で編集しない。

| ケース | 旧 lint の期待 | 新チェッカーの結果 | 判定 | 備考 |
|---|---|---|---|---|
| `allow_attribute.rs` | クリーン | クリーン | 一致 | フィクスチャ変更 `allow_attribute.rs`: `#[allow(multiple_type_definitions)]` の直前の空行を `// syn-check: allow(type-per-file)` に置き換えた |
| `private_structs_ok.rs` | クリーン | クリーン | 一致 |  |
| `public_and_private_ok.rs` | クリーン | クリーン | 一致 |  |
| `public_helpers_test.rs` | クリーン | クリーン | 一致 |  |
| `single_struct_ok.rs` | クリーン | クリーン | 一致 |  |
| `struct_and_enum.rs` | 違反 1 件（`struct_and_enum.rs:7`） | 違反 1 件（`struct_and_enum.rs:7`） | 一致 |  |

一致 6 件、差異あり（意図どおり） 0 件、不一致 0 件。
