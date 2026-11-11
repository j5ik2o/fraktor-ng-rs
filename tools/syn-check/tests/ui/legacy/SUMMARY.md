# 旧 UI テストとの対応（まとめ）

旧 fraktor-rs の dylint 9 本の UI テストを、新チェッカーで判定した結果である。行は、ケースと、フィクスチャを変更した補助ファイルを数える。
tests-location の UI テストは同梱されていなかったので、ここには無い。
このファイルは `SYN_CHECK_BLESS=1 cargo test --test legacy_ui` が生成する。手で編集しない。

| 旧 lint | 新ルール | 一致 | 差異あり（意図どおり） | 不一致 | 変更したフィクスチャ |
|---|---|---|---|---|---|
| [`mod-file`](mod-file/RESULTS.md) | `mod-file` | 2 | 0 | 0 | 1 |
| [`module-examples`](module-examples/RESULTS.md) | `module-examples` | 2 | 0 | 0 | 0 |
| [`type-per-file`](type-per-file/RESULTS.md) | `type-per-file` | 6 | 0 | 0 | 1 |
| [`use-placement`](use-placement/RESULTS.md) | `use-placement` | 7 | 0 | 0 | 0 |
| [`ambiguous-suffix`](ambiguous-suffix/RESULTS.md) | `ambiguous-suffix` | 7 | 0 | 0 | 1 |
| [`rustdoc`](rustdoc/RESULTS.md) | `rustdoc` | 2 | 0 | 0 | 0 |
| [`module-wiring`](module-wiring/RESULTS.md) | `module-wiring` | 28 | 0 | 0 | 1 |
| [`port-adaptor-boundary`](port-adaptor-boundary/RESULTS.md) | `port-adaptor-boundary` | 3 | 0 | 0 | 0 |
| [`cfg-std-forbid`](cfg-std-forbid/RESULTS.md) | `cfg-std-forbid` | 8 | 0 | 0 | 1 |
| 合計 | | 65 | 0 | 0 | 5 |
