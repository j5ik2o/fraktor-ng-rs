# cfg-std-forbid: 旧 UI テストとの対応

旧 `cfg-std-forbid-lint` の UI テストを、新チェッカーのルール `cfg-std-forbid` で判定した結果である。
期待は旧 lint の `<ケース>.stderr` から読む。`.stderr` が無いか空のケースはクリーンが期待である。
このファイルは `SYN_CHECK_BLESS=1 cargo test --test legacy_ui` が生成する。手で編集しない。

| ケース | 旧 lint の期待 | 新チェッカーの結果 | 判定 | 備考 |
|---|---|---|---|---|
| `core_allow.rs` | クリーン | クリーン | 一致 | フィクスチャ変更 `core_allow.rs`: `#[allow(cfg_std_forbid)]` の直前の空行を `// syn-check: allow(cfg-std-forbid)` に置き換えた |
| `core_cfg_not.rs` | クリーン | クリーン | 一致 |  |
| `core_cfg_test.rs` | クリーン | クリーン | 一致 |  |
| `core_feature_list.rs` | 違反 1 件（`core_feature_list.rs:3`） | 違反 1 件（`core_feature_list.rs:3`） | 一致 |  |
| `core_feature_only.rs` | 違反 1 件（`core_feature_only.rs:3`） | 違反 1 件（`core_feature_only.rs:3`） | 一致 |  |
| `core_forbid.rs` | クリーン | クリーン | 一致 |  |
| `core_use_std.rs` | 違反 1 件（`core_use_std.rs:3`） | 違反 1 件（`core_use_std.rs:3`） | 一致 |  |
| `non_core_ok.rs` | クリーン | クリーン | 一致 |  |

一致 8 件、差異あり（意図どおり） 0 件、不一致 0 件。
