# ゴール G0issue188：util の ImmutableIntMapSpec／TokenBucketSpec の移植

## 目的

Pekko v2.0.0 の `actor-tests` の util の2つの Spec（ImmutableIntMapSpec、TokenBucketSpec）を Rust へ移植し、対応する本体（ImmutableIntMap、TokenBucket）を `fraktor-actor` の kernel に新しく足して、台帳の対象2ノードを green にする。

設計は [docs/design/issue-188-specs.md](../design/issue-188-specs.md) にあり、独立レビュー（要修正、高1件・中4件・低4件）と再レビュー（承認、中1件・低3件）を通った。レビューの指摘はすべて設計へ反映し、実装もその形にした：到着分の加算の `saturating_add`（F1）、`last_update` と時刻の算術はすべて wrapping（F2）、Quint は `crates/fraktor-actor/specs/token_bucket.qnt` の1モジュールで CI の対象（F3）、不変条件 `delayIsEarliest` と負の対照 `bugExtraPeriod`（F4）、時刻は既存の `kernel::Clock` を使う（F5）、性質2の PBT は private のフィールドを読むため単体テストへ置く（N1）、テストの時計は複製できる `ManualClock(Arc<AtomicU64>)`（N2）。

## 対象ノード

| id | Rust の統合テスト | 原典のケース数 |
|:---|:---|---:|
| `actor-tests/src/test/scala/org/apache/pekko/util/ImmutableIntMapSpec.scala` | `crates/fraktor-actor/tests/immutable_int_map_spec.rs` | 10 |
| `actor-tests/src/test/scala/org/apache/pekko/util/TokenBucketSpec.scala` | `crates/fraktor-actor/tests/token_bucket_spec.rs` | 12 |

2ノードとも green である。除くケースは無い。TokenBucketSpec のケース12は、432通りの組み合わせを1件のテストで確かめる。

## 成果物

- 本体：`crates/fraktor-actor/src/kernel/immutable_int_map/`（`ImmutableIntMap`、`Vec<[i32; 2]>` の組の列と二分探索）、`crates/fraktor-actor/src/kernel/token_bucket/`（`TokenBucket<C: Clock>` と `TokenBucketError`、単体テスト `token_bucket_value_test.rs`）
- 移植テスト：`crates/fraktor-actor/tests/immutable_int_map_spec.rs`（10件）、`token_bucket_spec.rs`（12件）
- 原典の外のテスト：`immutable_int_map_property_test.rs`（BTreeMap のモデルとの照合）、`token_bucket_property_test.rs`（性質1、理想のバケットとの照合）、`token_bucket_value_test.rs`（性質2の T1、T2、T3、T9 と、構築とコストのエラー、F1 のあふれ）
- Quint：`crates/fraktor-actor/specs/token_bucket.qnt`
- 台帳：対象2ノードを green にした。あわせて `docs/baseline/v2.0.0-undocumented-nodes.tsv` を作り直し、対象2ノードを未記載（no-report）として載せた（`report_ledger_1to1.py list-undocumented`）

## 進め方

red（失敗するテスト、`E0432` で本体が無いことによる失敗）→ green の順に進めた。

## 検証

Mac mini（`~/muse-work/fraktor-ng-rs-188`）で実行した。

| コマンド | 結果 |
|:---|:---|
| `cargo fmt --check` | 通過 |
| `cargo clippy --workspace --all-targets -- -D warnings` | 通過 |
| `cargo test --workspace` | 1057件、失敗0 |
| `bash scripts/quint-check-specs.sh` | token_bucket は 5 passing、`No violation found` |
| syn-check（`crates/*/`） | 違反0 |
| `npm run check:aislop` | 受理済みの指摘と一致 |
| `uv run --with pytest --with pyyaml python -m pytest ledger/tools` | 全件通過 |
| `uv run --with pyyaml python3 ledger/tools/report_ledger_1to1.py check` | 検査1から5がすべて OK |
