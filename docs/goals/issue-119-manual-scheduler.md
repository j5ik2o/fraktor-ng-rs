# ゴール G0issue119：ExplicitlyTriggeredSchedulerSpec

[Issue #119](https://github.com/j5ik2o/fraktor-ng-rs/issues/119) の対象は Pekko v1.7.1 の `ExplicitlyTriggeredSchedulerSpec`。原典は、実時間を見ずに手動で時刻を進めるテスト用スケジューラ `ExplicitlyTriggeredScheduler` の振る舞いを8ケースで確かめる。 [Pekko v1.7.1 の原典テスト](https://github.com/apache/pekko/blob/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/testkit/src/test/scala/org/apache/pekko/testkit/ExplicitlyTriggeredSchedulerSpec.scala)

## 対象ノード

| id | Rust の統合テスト | 原典のケース数 |
|:---|:---|---:|
| `testkit/src/test/scala/org/apache/pekko/testkit/ExplicitlyTriggeredSchedulerSpec.scala` | `crates/fraktor-testkit/tests/explicitly_triggered_scheduler_spec.rs` | 8 |

原典のケース数は8、除外は0件。台帳の `extends-pekko-spec`、`pekko-spec` は G0b で充足している。

## 原典と実行経路の独立照合

Pekko v1.7.1 commit `9a54d1294c3ac7821bbed3c940769c3d5027d1ff` の `ExplicitlyTriggeredScheduler.scala` と `ExplicitlyTriggeredSchedulerSpec.scala` を照合した。原典実装の要点は次のとおり。

- 登録単位の `Item` がコールバック（`Runnable`）と反復間隔（`Option[FiniteDuration]`）を持ち、登録表は `Item` をキー、実行時刻（ミリ秒）を値とする。
- `timePasses(amount)` は新しい時刻を計算し、期限がそれ以下の Item を期限順に処理する。各実行の前に現在時刻をその期限へ進め、反復は `実行期限 + interval` で再登録する。コールバック中に新規登録があれば再帰的に処理する。
- 初期遅延が 0 以下なら `schedule` の呼び出し中に `executeTasks(currentTime)` を実行する。
- 取消は `Cancellable` が保持する `Item` に作用し、コールバックの同一性ではなく登録を単位とする。`cancel()` は登録表からその `Item` を除けたときだけ true を返す。
- 原典は `Thread.sleep(100)` を含むが、これは実時間待機であり、本移植の要件（時間は手動で進め、実時間 sleep なし）に反するため移植しない。

| 原典のケース（実名） | 入力と期待値 | Rust のテスト名 |
|:---|:---|:---|
| "execute a scheduled task" | 初期0秒、間隔5秒、12秒前進で3回 | `execute_a_scheduled_task` |
| "execute a scheduled task with initial delay" | 初期5秒、間隔5秒、12秒前進で2回 | `execute_a_scheduled_task_with_initial_delay` |
| "execute a scheduled task only once" | 遅延5秒、100秒前進で1回 | `execute_a_scheduled_task_only_once` |
| "schedule multiple identical tasks" | 同一コールバックを遅延1秒で3登録、2秒前進で3回 | `schedule_multiple_identical_tasks` |
| "cancel scheduled task" | 7秒で1回、取消が true、さらに4秒で1回のまま | `cancel_scheduled_task` |
| "cancel one out of many scheduled tasks" | 3登録、7秒で3回、1登録の取消が true、さらに4秒で計5回 | `cancel_one_out_of_many_scheduled_tasks` |
| "not execute task if cancelled immediately" | 登録直後に取消、7秒前進で0回 | `not_execute_task_if_cancelled_immediately` |
| "allow to move in time many times" | 同じスケジューラを7秒、1秒、10秒前進し累計1、1、3回 | `allow_to_move_in_time_many_times` |

## 実際に使った TestKit needs と前提

Rust 版は `ManualScheduler::new()` で生成し、`schedule`、`schedule_once`、`time_passes` を呼ぶ。コールバックは `Arc<dyn Fn() + Send + Sync>` で、件数は `AtomicUsize` で数える。原典の `extends PekkoSpec` と `PekkoSpec` は G0b で充足済みだが、Rust 版の統合テストは `FraktorSpec` を継承せず、`ManualScheduler` を直接使う。実時間待機とスレッドは使わない。

原典由来の `Thread.sleep`（前提 `thread-sleep`）は要件により移植しないため、台帳の前提として使っていない。

## 公開 API と設計

- `crates/fraktor-testkit/src/manual_scheduler.rs` に公開モジュール `manual_scheduler` を置く。`ManualScheduler` は現在時刻と登録表を1つの内部ミューテックス越しに共有し、`ManualSchedulerTask` は弱参照と登録 ID で同じ登録表に届く cancel ハンドルである。原典で dispatcher はコールバックに使われないため、`ActorSystem` 依存の公開入口は設けず `ManualScheduler::new()` だけで生成する。
- `schedule(initial_delay, interval, callback)` は反復登録を、`schedule_once(delay, callback)` は単発登録を返す。初期遅延が 0 なら登録呼び出し中に、呼び出しスレッドで実行する。
- `time_passes(amount)` は前進先の時刻まで、期限がその時刻以下のコールバックを、呼び出しスレッドで復帰前にすべて実行する。1回の前進の中では、各コールバック実行後に登録表から最短の due を取得し直すため、自己再登録する反復登録や、コールバックが新規登録した due task も同じ前進内で実行される。コールバックは表のロック外で実行し、先のコールバックが取消した登録は実行せずに飛ばす。
- 反復登録の再登録は登録 ID を保ったまま期限だけを更新する。これにより最初の cancel ハンドルが反復の各回に有効であり続ける。
- 取消は登録単位で作用する。`cancel()` は未実行・未取消の登録に対して true、二重取消・実行後の取消・スケジューラ破棄後は false を返す。
- ゼロ反復間隔、初回期限・前進先の算術オーバーフローは、状態を変更する前に拒否する。`time_passes` は開始時点の登録表について、前進先までの最後の反復期限と、その次の再登録期限を剰余計算で事前検証する。後続反復でオーバーフローする場合も、コールバック実行・時計・登録の更新前に拒否する。実行対象は固定せず、コールバックが後から作る登録には実行直前の検証も維持する。既に実行したコールバックの副作用は巻き戻さない。
- `time_passes` の再入は拒否する（コールバックから `time_passes` を呼ぶと panic）。取得成功時だけ作るRAIIガードが正常終了・unwind時に進行状態を解放し、拒否された内側呼び出しは外側の状態を解放しない。コールバックのpanicは伝播し、その反復を再登録しないが、次の前進は可能になる。コールバックから `schedule`・`schedule_once` を呼ぶことは許される。
- 公開 API の保証と制約（同期実行・共有時刻と登録表・単一の進行主体・拒否条件・取消範囲・コールバックの panic）は rustdoc に英語で記述する。

## 補助単体テスト

原典8件では観測しない退行だけを最小限の単体テストで捕捉する。原典と重複するケースは置かず、台帳のケース数（8）には算入しない。

- ゼロ初期遅延の即時実行と期限と同値での実行は、API復帰直後の件数と呼び出しスレッドIDを確認する。期限直前では未実行。
- 先のコールバックが同一前進内の別の due を取消しても安全に飛ばすこと。
- 二重取消が false になること。
- ゼロ間隔と、初回期限・前進先・反復再登録の各オーバーフローの拒否（拒否時に実行0、既存登録の維持）。
- 時計MAX−10秒、初期1秒・間隔6秒で、6秒前進は1回実行、10秒前進は実行0・時計と登録ID・期限・間隔を保持して拒否する。
- 算術拒否後の取消・再前進、コールバックpanic後の非再登録・再前進、内側前進の連続2回拒否と外側終了後の再取得。
- 親期限1秒から子を新規登録し、子遅延1秒なら外側3秒前進の復帰前に期限2秒で1回実行する。子遅延3秒なら期限4秒に残り、追加1秒前進で1回実行する。

通常反復の自己再登録だけを観測する重複補助1件は削除した。通常反復と同一IDの取消は原典相当の統合8件で検証する。現行の補助は13件で、台帳には算入しない。

## 初回実装の検証結果と再現コマンド（履歴）

| コマンド | 実測 |
|:---|:---|
| `cargo test -p fraktor-testkit --test explicitly_triggered_scheduler_spec` | 8 passed、0 failed |
| `cargo test -p fraktor-testkit --lib manual_scheduler` | 10 passed、0 failed（補助単体テスト。台帳の8には算入しない） |
| `cargo test --workspace` | 全バイナリと doc-test が成功（0 failed） |
| `cargo fmt --check` | exit 0（下記「fmt 修正の根拠」参照） |
| `cargo clippy --workspace --all-targets -- -D warnings` | 警告なし |
| `cargo run --release --locked --manifest-path tools/syn-check/Cargo.toml -- --format json crates/fraktor-testkit` | `violations: 0` |
| `uv run --with pyyaml python3 ledger/tools/check_needs.py --ledger ledger/nodes.yaml --capabilities ledger/capabilities.yaml --goal G0issue119` | 対象1ノード、needs 閉性 OK（exit 0） |
| `uv run --with pyyaml python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal G0issue119` | 1ノード、green 1、完了（exit 0） |

G0a〜G0f の check_needs と rollup も各々 exit 0 を確認した。

## レビュー修正後の実測

実行時HEADは `5955225ff7e49fd153b22732cebd284b4f8e1f94`。未コミットの修正版を検証した。Rustは `rustc 1.100.0 (0ef1bc386 2026-11-09)`、rustfmtは `1.11.0-stable`、crate editionは2021。

| コマンド | 実測 |
|:---|:---|
| `cargo build -p fraktor-testkit` | exit 0 |
| `cargo test -p fraktor-testkit --lib manual_scheduler` | exit 0、13 passed、0 failed |
| `cargo test -p fraktor-testkit --test explicitly_triggered_scheduler_spec` | exit 0、8 passed、0 failed |
| `cargo test --workspace` | exit 0、全テストとdoc-test成功 |
| `cargo fmt --check` | exit 0（stableでは未対応のrustfmt設定の警告あり、整形差分なし） |
| `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 |
| `cargo run --release --locked --manifest-path tools/syn-check/Cargo.toml -- --format json crates/fraktor-testkit` | exit 0、43ファイル、違反0、抑制0 |
| `uv run --with pyyaml python3 ledger/tools/check_needs.py --ledger ledger/nodes.yaml --capabilities ledger/capabilities.yaml --goal GOAL` | G0issue119・G0a〜G0fすべてexit 0 |
| `uv run --with pyyaml python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal GOAL` | 同7ゴールすべてexit 0 |

元出力・コマンド・終了コード・SHAは、この作業ツリー内の `.takt/issue-119-evidence/phase2/fix-1/` の各 `.log`・`.json` に保存した。指定された別checkoutの証跡ディレクトリはプロジェクト外編集禁止のため変更せず、以前の実測を保持している。台帳検査の初回はuvの既定キャッシュへの書込み制限でexit 2となったため、`*-initial.log`・`*-initial.json`を保持し、`UV_CACHE_DIR=.takt/issue-119-evidence/uv-cache`で全件再実行した。

修正前の `before.log` は13件中3件失敗（exit 101）。後続反復のoverflowで1回の部分実行と時計更新が残り、最初の反復の拒否後・callback panic後の正常前進も再入扱いで拒否された。修正後は同条件を含む13件が成功した。初回実装の補助10件成功は上の履歴として保持し、今回の成功件数へ書き換えていない。

第1工程確定SHAとの本文比較も再実行し、import順の入れ替えだけで完全一致することを確認した。原典相当の8関数・14アサートと台帳の8/8/8・除外なしは維持している。

## fmt 修正の根拠

第1工程で追加した `explicitly_triggered_scheduler_spec.rs` の `use` 並びが、`cargo fmt --check`（PR 必須 CI）の期待とずれていた。原因は初回のファイル整形チェックが edition 2024 の順序で行われたことで、クレートは `edition = "2021"`（`crates/fraktor-testkit/Cargo.toml`）。edition 2021 の `cargo fmt` に従い、`atomic::{AtomicUsize, Ordering}` を `Arc` より先に戻す整形だけを行った。

- 第1工程の確定コミットは `7567828`、当該ファイルの blob は `e9bd3a1579179308c540de973b78ced9a2be9479`（前の版として履歴に残す）。HEAD の blob も同値。整形後の作業ツリーブロブは `ca21f851b5ad1e3e07da6a7f40c00673ba60e3d5`。
- `git diff 7567828 -- …/explicitly_triggered_scheduler_spec.rs` は `use` の2行（`Arc` と `atomic::{…}` の入れ替え）だけ。`#[test]` は8件、`assert` は14件で、第1工程と件数・本体・アサートが同一。差分は整形のみ。
- 初回 red の確定（第1工程の統合 SHA `5955225…` で未実装によりコンパイル失敗）は前の版の記録として保持し、成功扱いしていない。最終 fmt（exit 0）と実測は上表のとおり取り直した。

対象は1ノードのみ `status: green`、`cases.total/ported/passing: 8`、`excluded: []`、`goal: G0issue119` とした。他ノード、能力定義、CI・設定は変更していない。PR・issue 操作、最終 CI・マージは親担当の対象外とする。

## 確定した製品未変更の初期 red

第1工程で確定した状態（統合 SHA `5955225ff7e49fd153b22732cebd284b4f8e1f94`）では、`fraktor_testkit::manual_scheduler` が未実装のため統合テストはコンパイルに失敗した（exit 101、`E0432 unresolved import`、宣言8件は実行0件）。この red は実行時に8ケースが失敗したことを意味せず、実行時の成功や全型整合の証拠にならない。第2工程の実装後は上表のとおり8/8が成功しており、この red を成功扱いしていない。
