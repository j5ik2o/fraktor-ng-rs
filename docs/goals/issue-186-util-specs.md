# ゴール G0issue186：util の PrettyDurationSpec／GlobSpec／SwitchSpec の移植

## 目的

Pekko v2.0.0 の `actor-tests` の util の3つの Spec（PrettyDurationSpec、GlobSpec、SwitchSpec）を Rust へ移植し、対応する本体（PrettyDuration、Glob、Switch）を `fraktor-actor` の kernel に新しく足して、台帳の対象3ノードを green にする。

設計は [docs/design/issue-186-util-specs.md](../design/issue-186-util-specs.md) にあり、独立レビュー（要修正、高2件・中5件・低4件）と再レビュー（承認、低5件）を通った。再レビューの低い指摘5件は、設計への反映と実装の両方で対応した：S5 を Quint の不変条件に入れる履歴の変数（`onAtLock`、`lastChanger`）、戻しをロックの保持中に CAS で行うこと、照合でバイト位置を使い確保しないこと、ファイル表の抜けと置き場所の食い違いの修正、期待値を作った JDK の版（JDK 26）の明記である。

## 対象ノード

| id | Rust の統合テスト | 原典のケース数 |
|:---|:---|---:|
| `actor-tests/src/test/scala/org/apache/pekko/util/PrettyDurationSpec.scala` | `crates/fraktor-actor/tests/pretty_duration_spec.rs` | 13 |
| `actor-tests/src/test/scala/org/apache/pekko/util/GlobSpec.scala` | `crates/fraktor-actor/tests/glob_spec.rs` | 6 |
| `actor-tests/src/test/scala/org/apache/pekko/util/SwitchSpec.scala` | `crates/fraktor-actor/tests/switch_spec.rs` | 6 |

3ノードとも green である。

PrettyDurationSpec の件数は、原典の `cases.foreach` が10件の表を展開するため、台帳の概数（3件）と issue の目安（4件）ではなく実数の13件である。除くケースは無い。

Switch ノードの前提フラグ（`thread-sleep`）は外した。原典のケース6の `Thread.sleep(500)` はほかにスレッドが居ない場面で結果に効かず、移植は sleep を使わず `join()` で確かめるため、移植がその前提に頼らないからである（設計の §1.3 と §5）。

## 成果物

- 本体：`crates/fraktor-actor/src/kernel/pretty_duration/`（`PrettyDurationInput` とその `impl`、整形は `pretty_duration_format.rs`）、`crates/fraktor-actor/src/kernel/glob/`（`GlobPattern`）、`crates/fraktor-actor/src/kernel/switch/`（`Switch`、`LockedSwitch`、drop ガード）
- 移植テスト：`crates/fraktor-actor/tests/pretty_duration_spec.rs`（13件）、`glob_spec.rs`（6件）、`switch_spec.rs`（6件）
- 原典の外のテスト：`pretty_duration_format_test.rs`（`%g` の HALF_UP の固定ケースと i64 の範囲の外）、`pretty_duration_property_test.rs`、`glob_property_test.rs`、`switch_model_test.rs`、`switch_extra_test.rs`（S5、panic のときの戻し、引数なし版）、`crates/fraktor-actor/src/kernel/switch/switch_value_test.rs`（drop ガードの単体テスト）
- Quint：`crates/fraktor-actor/specs/switch.qnt`
- 台帳：対象3ノードを green にし、PrettyDurationSpec の件数を実数13件にした。あわせて `docs/baseline/v2.0.0-undocumented-nodes.tsv` を作り直し、対象3ノードを未記載（no-report）として載せた（`report_ledger_1to1.py list-undocumented`）

## 進め方

red（失敗するテスト、`E0432` で本体が無いことによる失敗）→ green の順に進め、red のコミットを残した。

## 検証

Mac mini（`~/muse-work/fraktor-ng-rs`）で実行した。

| コマンド | 終了値 | 結果 |
|:---|---:|:---|
| `cargo fmt --check` | 0 | 差分なし |
| `cargo clippy --workspace --all-targets -- -D warnings` | 0 | 警告なし |
| `cargo test --workspace` | 0 | 1075件成功、0件失敗 |
| `cargo test -p fraktor-actor --test pretty_duration_spec --test glob_spec --test switch_spec` | 0 | 13件、6件、6件が成功 |
| `scripts/quint-check-specs.sh` | 0 | 全 spec が通過（switch は run 4件が成功し、不変条件の違反なし、トレース最大長21） |
| 台帳の Python テスト（`uv run --with pyyaml python3 ledger/tools/<name>_test.py`） | — | `goal_complete_gate`、`verify_red`、`verify_green`、`verify_green_test_crate`、`pekko_baseline`、`retired_lineage`、`reverify_against`、`relocate_nodes`、`report_ledger_1to1` が通過。`verify_green_test_aislop` は、この実行環境では fixture の中の `cargo` と `bash` が見つからず失敗する。変更前のコードでも同じ失敗になることを確かめたため、環境側の失敗であり、この変更による失敗ではない |
| `python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal G0issue186` | 0 | ゴール G0issue186 はノード3件すべて green で完了 |
