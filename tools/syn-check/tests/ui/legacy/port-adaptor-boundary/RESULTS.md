# port-adaptor-boundary: 旧 UI テストとの対応

旧 `port-adaptor-boundary-lint` の UI テストを、新チェッカーのルール `port-adaptor-boundary` で判定した結果である。
期待は旧 lint の `<ケース>.stderr` から読む。`.stderr` が無いか空のケースはクリーンが期待である。
このファイルは `SYN_CHECK_BLESS=1 cargo test --test legacy_ui` が生成する。手で編集しない。

| ケース | 旧 lint の期待 | 新チェッカーの結果 | 判定 | 備考 |
|---|---|---|---|---|
| `concrete_api_field.rs` | 違反 2 件（`auxiliary/modules/cluster-adaptor-std/src/concrete_api_field.rs:4`、`auxiliary/modules/cluster-adaptor-std/src/concrete_api_field.rs:7`） | 違反 2 件（`auxiliary/modules/cluster-adaptor-std/src/concrete_api_field.rs:4`、`auxiliary/modules/cluster-adaptor-std/src/concrete_api_field.rs:7`） | 一致 |  |
| `port_trait_ok.rs` | クリーン | クリーン | 一致 |  |
| `same_name_wrapper.rs` | 違反 3 件（`auxiliary/modules/cluster-adaptor-std/src/same_name_wrapper.rs:5`、`auxiliary/modules/cluster-adaptor-std/src/same_name_wrapper.rs:9`、`auxiliary/modules/cluster-adaptor-std/src/same_name_wrapper.rs:13`） | 違反 3 件（`auxiliary/modules/cluster-adaptor-std/src/same_name_wrapper.rs:5`、`auxiliary/modules/cluster-adaptor-std/src/same_name_wrapper.rs:9`、`auxiliary/modules/cluster-adaptor-std/src/same_name_wrapper.rs:13`） | 一致 |  |

一致 3 件、差異あり（意図どおり） 0 件、不一致 0 件。
