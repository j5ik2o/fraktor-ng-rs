# ゴール G0issue121：classic TestTimeSpec

[Issue #121](https://github.com/j5ik2o/fraktor-ng-rs/issues/121)（Orca 項目94）の対象は Pekko v1.7.1 の classic `TestTimeSpec`。原典は、時間倍率 2.0 の spec 上で 3 ケースを実行する。`awaitCond` の常に false な待機が倍率適用後の期限で時間切れになり、経過時間が `[2s, 3s)` に入ること、`awaitAssert` が最終 assertion の失敗を再送出すること、`within` の外で `remaining` を呼ぶと契約違反になることを確かめる。 [Pekko v1.7.1 の原典](https://github.com/apache/pekko/blob/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/testkit/src/test/scala/org/apache/pekko/testkit/TestTimeSpec.scala#L20-L49)

## 対象ノード

| id | Rust の統合テスト | 原典のケース数 |
|:---|:---|---:|
| `testkit/src/test/scala/org/apache/pekko/testkit/TestTimeSpec.scala` | `crates/fraktor-testkit/tests/test_time_spec.rs` | 3 |

原典のケース名は `correctly dilate times`（TimingTest）、`awaitAssert must throw correctly`、``throw if `remaining` is called outside of `within` ``。除外は0件。台帳の `test-kit` / `test-probe` / `within` / `await-assert` は既存ゴールで充足済みで、`await-cond` は本ゴールで実装済みに更新した。

原典の `test_cases_approx` は `2` のまま（概算値であり、実測の3件とは区別する）。

## write_scope

- 対象の統合テストと、それを通すために必要な時間倍率・条件待機・within 下限／残り時間の API。
- 対象ノードの台帳欄、`await-cond` の capability、このゴール文書。

第2工程では `crates/fraktor-testkit` の製品5ファイル（`test_kit_settings.rs` / `test_kit_value.rs` / `test_kit_base.rs` / `test_probe.rs` / `fraktor_spec.rs`）を更新し、補助の統合テスト `tests/test_time_contract.rs` を追加した。PRレビューでは時間倍率の専用型を追加し、原典相当テスト `tests/test_time_spec.rs` の設定方法を合わせた。

## 原典と実行経路の独立照合

Pekko v1.7.1 commit `9a54d1294c3ac7821bbed3c940769c3d5027d1ff` の `TestTimeSpec.scala` と `TestKit.scala`（`awaitCond` / `awaitAssert` / `remaining` / `within`）を照合した。

| 原典 | Rust の入口 | 直接観測する契約 |
|:---|:---|:---|
| L24–34 `correctly dilate times` | `FraktorSpec::with_time_factor` → `test_kit_settings().dilated` / `TestProbe::with_settings` → `await_cond` | 倍率 2.0 の設定が probe へ届き、常に false な条件が `[2s, 3s)` で timeout する |
| L36–43 `awaitAssert must throw correctly` | `FraktorSpec::await_assert` / `within_range` | 成功値の返却と最終 assertion（foo/bar）の再送出、within 予算超過での置換をしないこと |
| L45–49 ``throw if `remaining` is called outside of `within` `` | `FraktorSpec::remaining` | within 外で `remaining` と `within` を含む契約違反 |

第1工程で確定した公開APIは `FraktorSpec::with_time_factor`、`TestProbe::with_settings`、`TestProbe::await_cond`、`FraktorSpec::within_range`、`FraktorSpec::remaining`。従来の `within(max, body)` も同じ実装を使う。倍率は原典 `TestKitExtension.Settings` と同様に有限の正値だけを受理する。0・負・無限・NaN は `TestTimeFactor::new` が拒否する。

経路：倍率は `FraktorSpec::with_time_factor` → `TestKitSettings::with_time_factor` → `TestKit::with_settings` → `TestKitBase::new` で保持し、`TestKit::test_kit_settings` は保持済み設定を返す（システムから作り直さない）。`TestProbe::with_settings` は受け取った設定をそのまま base へ渡す。`await_cond` は probe 側で `max` を1回だけ膨張し、base が条件を即時評価してから、false の間は `drive_for` で actor を運転しながら単調時計の期限で再評価する。`within_range` は min を膨張せず、外側残り時間で min を満たせない場合は本文の前に拒否し、終了時に未膨張の min と（no-message 以外で）上限を検査し、`ScopeReset` で外側期限を復元する。`remaining` は同じ scope を読み、期限経過後は 0 に飽和し、scope 外では `remaining` と `within` を含む契約違反を返す。

原典の actor 全体設定機構は採らず、既存の FraktorSpec / TestKitSettings / TestProbe / TestKit / TestKitBase を再利用する。環境変数・グローバル可変状態は導入していない。既定倍率は 1.0 のまま。

対応する [Rust 統合テスト](../../crates/fraktor-testkit/tests/test_time_spec.rs) は3件、除外なし。補助の [契約テスト](../../crates/fraktor-testkit/tests/test_time_contract.rs) は修正後13件で、倍率の伝播と無効値拒否、`await_cond` の即時評価／早期復帰／待機中の配送、`within` の min 非膨張（未達拒否と正常終了・本文値返却）・入れ子／panic 後の期限復元・実現不能 min の事前拒否・no-message 上限例外、`remaining` の scope 内値と scope 外契約違反を観測する。補助テストは原典3件の件数に加算しない。

## 対照測定と実装後の実測

対照測定（製品未変更）は、統合 SHA `05a23daf3d010f5ba3effc1427e70f9fcf53afae`（tree `41fc5fc2f17ef29bd329f760f8bd2d29817c385a`）で `4f5d28cd28452ad37680f8d4380196a07ec9d92a` と同一ツリーを再実行したもの。test blob `5f2b3805fa5aadd2a3c600ba8ecdbaa4b747810b`。分類は compile-red、`cargo` 終了値 101、宣言3・実行0・除外0、未実装 API の E0599 5件、製品差分0。これは実装後の失敗ではなく、本番編集前のコンパイル不能状態を記録したもの。

実装後の実測（初回実装、補助12件）。実行時 HEAD は `05a23daf3d010f5ba3effc1427e70f9fcf53afae` のままで、変更は作業ツリー（未コミット）にある。HEAD 単独では実装コミットを識別しないため、初回測定時の対象ファイルの `git hash-object` を併記する。

| ファイル | git hash-object |
|:---|:---|
| `crates/fraktor-testkit/src/test_kit_extension/test_kit_settings.rs` | `8710cd556cc7b10d0a6f00dd4add0cdadf35dcc2` |
| `crates/fraktor-testkit/src/test_kit/test_kit_value.rs` | `4abe8e2d969a4af261b94a739835af319526d6d5` |
| `crates/fraktor-testkit/src/test_kit/test_kit_base.rs` | `ae5692aa1e216514d29f9744c18d64d71e247b6e` |
| `crates/fraktor-testkit/src/test_kit/test_probe.rs` | `13d39ad2dd4bc6b773eda91e0a99ccbabf6cc566` |
| `crates/fraktor-testkit/src/fraktorspec/fraktor_spec.rs` | `f97205004b9e9dcc42cd563eef56fbba0c4fdecc` |
| `crates/fraktor-testkit/tests/test_time_contract.rs`（新規） | `ef266ef8bbb38ceb73baf1c31f417a02ae9c3e41` |
| `crates/fraktor-testkit/tests/test_time_spec.rs`（不変） | `5f2b3805fa5aadd2a3c600ba8ecdbaa4b747810b` |
| `ledger/nodes.yaml` | `e1d0eddd27ca3ec3536bbf7adc8d5c5723de4e5d` |
| `ledger/capabilities.yaml` | `0389dcf7b614b609bf076f1e5eb4c4ce956369b4` |

原典相当テストの blob `5f2b3805fa5aadd2a3c600ba8ecdbaa4b747810b` は対照測定と一致しており、テストソースの相違はない。

初回実装後のコマンドと結果（修正後13件の測定とは区別する）。

| コマンド | 終了値 | 成功数 |
|:---|---:|---:|
| `cargo test -p fraktor-testkit --test test_time_spec -- --test-threads=1` | 0 | 3 passed / 0 failed / 0 ignored |
| `cargo test -p fraktor-testkit --test test_time_contract -- --test-threads=1` | 0 | 12 passed / 0 failed / 0 ignored |
| `cargo test -p fraktor-testkit` | 0 | 48 unit + 統合すべて成功 |
| `cargo test --workspace` | 0 | 全テスト成功（失敗0） |
| `cargo fmt --check` | 0 | 差分なし |
| `cargo clippy --workspace --all-targets -- -D warnings` | 0 | 警告なし |
| `cargo run --release --locked --manifest-path tools/syn-check/Cargo.toml -- --format json crates/fraktor-testkit` | 0 | `clean: true`、violations 0 |

台帳は `G0issue121` と `G0a`〜`G0f` の各々について `check_needs.py`（needs 閉性 OK、終了値 0）と `rollup.py`（全ノード green で完了、終了値 0）を実行し、すべて成功した。

## min 非膨張の正常終了を追加検証した修正後の実測

`CODE-NEW-test-time-contract-L195` に対応して補助テストを1件追加した。既存の即時終了拒否テストだけでは、未膨張 min を満たす有効な本文の受理を識別できなかったため、公開入口から正常終了と本文値返却を直接観測する。製品コード、原典相当3ケース、台帳は今回変更していない。

追加テスト `within_accepts_a_body_between_the_raw_and_dilated_minimum_and_returns_its_value` は、倍率2.0・min1秒・max2秒を固定し、即時終了が拒否されることを確認した後、同じ入力で本文を1.5秒待機させる。本文の実測が1秒以上、呼び出し全体の実測が2秒未満、本文値が期待値と一致することを assert する。呼び出し全体が膨張 min の2秒未満なので、本文も2秒未満かつ膨張 max の4秒以内である。時間境界の緩和や成功するまでの自動再試行は追加していない。

修正後も実行時 HEAD は `05a23daf3d010f5ba3effc1427e70f9fcf53afae`。補助テストの blob は `fa481a6c3dec23efa89832213880eb88bbe5babf`（初回 `ef266ef8bbb38ceb73baf1c31f417a02ae9c3e41` から変更）、原典相当テストの blob は `5f2b3805fa5aadd2a3c600ba8ecdbaa4b747810b` のまま。製品5ファイルと台帳2ファイルの blob は上記初回測定表と一致する。

| コマンド | 終了値 | 修正後の結果 |
|:---|---:|:---|
| `cargo test -p fraktor-testkit --test test_time_contract within_accepts_a_body_between_the_raw_and_dilated_minimum_and_returns_its_value -- --exact --nocapture --test-threads=1` | 0 | 1 passed / 0 failed / 0 ignored / 12 filtered out。本文 1.506845833秒、呼び出し全体 1.506849583秒。即時終了は下限検査で拒否 |
| `cargo test -p fraktor-testkit --test test_time_contract -- --test-threads=1` | 0 | 13 passed / 0 failed / 0 ignored / 0 filtered out |
| `cargo test -p fraktor-testkit --test test_time_spec -- --test-threads=1` | 0 | 3 passed / 0 failed / 0 ignored / 0 filtered out |
| `cargo test --workspace` | 0 | 全51スイート、430 passed / 0 failed / 0 ignored / 0 filtered out |
| `cargo build -p fraktor-testkit` | 0 | ビルド成功 |
| `cargo fmt --check` | 0 | 整形差分なし（stableで未対応の設定に警告あり） |
| `cargo clippy --workspace --all-targets -- -D warnings` | 0 | 成功 |
| `cargo run --quiet --release --locked --manifest-path tools/syn-check/Cargo.toml -- --format json crates/fraktor-actor` | 0 | `clean: true`、137 files、違反・parse failure 0 |
| 同コマンドの対象を `crates/fraktor-testkit` に変更 | 0 | `clean: true`、45 files、違反・parse failure 0 |
| `npm ci --cache target/issue-121-fix/npm-cache` → `npm run check:aislop` | 各0 | 依存導入後、warn以上の指摘が既存の受理済み一覧と一致 |

台帳の原典件数は3件のまま。修正後にも `G0issue121` と `G0a`〜`G0f` の各々で `check_needs.py` と `rollup.py` を実行し、14コマンドすべて終了値0、needs閉性OK・全対象ノードgreenで完了した。

通常のPython検証も `uv run --with pyyaml python3 ledger/tools/<検証器>.py` で実行した。

| 検証器 | 終了値 | 結果 |
|:---|---:|:---|
| `goal_complete_gate_test` | 0 | 26 passed / 0 failed |
| `verify_red_test` | 0 | 48 passed / 0 failed |
| `verify_green_test` | 0 | 全5ケース成功 |
| `verify_green_test_aislop` | 0 | 全4ケース成功。検証器自身のPATH制限を変更せず成功 |
| `verify_green_test_crate` | 0 | 全15ケース成功 |

検証時のUVキャッシュは `target/issue-121-fix/uv-cache`。最初の `verify_green_test` は一時ディレクトリをリポジトリ内の `target/issue-121-fix` に指定したため、生成されたスタブのfmtゲートが失敗して終了値1となった。同じソースの検証器を、一時ディレクトリ指定を外して標準の一時ディレクトリで実行すると全5ケースが成功した。製品の `cargo fmt --check` は終了値0。時間条件を持つ追加テストの再試行はしていない。

## コミットで再検証した結果

第2工程の成果コミット `d12291bc9ecb22f4b789a3b167a70340cd4ee0b6` で原典3件、ワークスペース回帰、対象と G0a〜G0f の台帳検査14件が成功した。

統合後は、条件の評価間隔を原典の100msに合わせた。メッセージ配送には既存の待機処理を使う。入れ子の `within` は、下限と残り時間を `Duration` のまま比較する。設定を指定した probe にも既定の `testProbe` 名を使う。

これらの修正と最新の main を取り込んだ `f2ffd8476ea65ef433f327c8a9e05f60b0fba28c` で、原典3件、ワークスペース434件、対象と G0a〜G0f の台帳検査14件がすべて成功した。整形、Clippy、両クレートの構文検査も成功した。個別実行のテストハーネス表示は、TimingTest が2.00秒、成功と失敗を検証する `await_assert` ケースが0.96秒だった。

## PRレビュー後の実測

修正コミット `6d137a667c9334ad14e5ed09fd29c25b69fca51d` で時間倍率を `TestTimeFactor` にまとめた。また、内側の `within` で発生したパニックを捕捉した直後に、外側の残り時間を確認するよう補助テストを直した。

原典3ケースの変更は `TestTimeFactor` のインポートと倍率2.0を渡す設定処理に限る。パニックの判定、待機の上限・間隔、経過時間の許容幅は第1工程で検証したとおり。修正後のテストファイルの blob は `cea60fa2ad497920257bfcaa53aa2970efbe6549`。

修正コミットで原典3件、補助13件、ワークスペース434件、対象と G0a〜G0f の台帳検査14件が成功した。整形、Clippy、`fraktor-testkit` の構文検査も成功した。

## 再現手順

```text
cargo test -p fraktor-testkit --test test_time_spec -- --test-threads=1
cargo test -p fraktor-testkit --test test_time_contract -- --test-threads=1
cargo test --workspace
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
uv run --with pyyaml python3 ledger/tools/check_needs.py --ledger ledger/nodes.yaml --capabilities ledger/capabilities.yaml --goal G0issue121
uv run --with pyyaml python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal G0issue121
```
