# ゴール G0issue108：MarkerLoggingSpec

[Issue #108](https://github.com/j5ik2o/fraktor-ng-rs/issues/108) の対象は Pekko v1.7.1 のマーカー付きログ。原典は `MarkerLoggerAdapter` の 2 ケースで、マーカー付き `info` が本文と SECURITY マーカーを購読先へ届けること、マーカー付き `error` が原因例外の `getMessage` を本文と分離して届けることを確かめる。[Pekko v1.7.1 の原典](https://github.com/apache/pekko/blob/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/actor-tests/src/test/scala/org/apache/pekko/event/MarkerLoggingSpec.scala)

## 対象ノード

| id | Rust の統合テスト | 原典のケース数 |
|:---|:---|---:|
| `actor-tests/src/test/scala/org/apache/pekko/event/MarkerLoggingSpec.scala` | `crates/fraktor-actor/tests/marker_logging_spec.rs` | 2 |

原典のケース名は `add markers to logging` と `allow cause exceptions in error messages`。除外は 0 件。台帳の `extends-pekko-spec` と `pekko-spec` は G0b、`event-filter` は G0d、`implicit-sender` は G0b、`expect-msg` は G0b で充足している。

## write_scope

- 対象の統合テストと、それを通すためのマーカー付き発行口・`LogMarker`・`Info`/`Error` のマーカーと原因メッセージ保持。
- 対象ノードの台帳欄とこのゴール文書。

## 原典とテストの個別照合

Pekko v1.7.1 commit `9a54d1294c3ac7821bbed3c940769c3d5027d1ff` の `MarkerLoggingSpec.scala` 全文と、統合済みの Rust コードを照合した。

| 原典のケース | Rust のケース | 購読先と受信 | 直接観測する契約 |
|:---|:---|:---|:---|
| `add markers to logging` | `add_markers_to_logging` | 同じ `ActorSystem` の `event_stream` に `TestProbe` を `EventClass::of::<Info>` で購読し、`probe.expect_msg_type::<Info>` で受信 | 実配送された `Info` の `message()` が `"This is a security problem"`、`marker()` が `SECURITY` |
| `allow cause exceptions in error messages` | `allow_cause_exceptions_in_error_messages` | 同じ `ActorSystem` の `event_stream` に `TestProbe` を `EventClass::of::<Error>` で購読し、`probe.expect_msg_type::<Error>` で受信 | 実配送された `Error` の `message()` が `"this is a security crash"`、`cause_message()` が `Some("Security Exception")`、`EventFilter::typed(MyException).with_message("this is a security crash").apply(&error)` が真 |

`add_markers_to_logging` は本文比較と、マーカー名 `SECURITY` の比較を維持する。`allow_cause_exceptions_in_error_messages` は本文比較と、原典のミュート `EventFilter[MyException]("this is a security crash")` を、受信済み `Error` への既存 `EventFilter` 適用として維持し、加えて error に付けた `LogMarker::Security` が受信 `Error` の `marker()` に保持されることを確認する。原典の `cause.getMessage` は `cause_message()` で読む。

`LogMarker` と `LogMarkerAdapter` は既存 `Logging` 配下（`Logging::LogMarker` / `Logging::LogMarkerAdapter`）に公開入口を集約した。原因メッセージは adapter が原因の `Display` から直接生成し、任意の `String` を別渡しする公開 API は持たない。組み立て用の `Error::with_cause_and_message` は crate 内に閉じ、既存 `Error::with_cause` の `Debug` 限定も維持する。

## 原典との意図的な差異

| 項目 | 原典 | Rust の扱い |
|:---|:---|:---|
| info のミュート | `system.eventStream.publish(TestEvent.Mute(EventFilter.info()))` を実行 | `EventFilter` に `Info` 用の変種がないため省略し、購読で受信した `Info` から本文とマーカーを直接観測する。除外ではない |
| 原因の表現 | `Throwable` オブジェクトを保持し `cause.getMessage` を読む | 原因名は `Debug`（`MyException`）、原因メッセージは `Display`（`Security Exception`）として別フィールドに保持する |
| ロガー・フィルター構造 | `DefaultLoggingFilter` を渡す | 既存の `EventStream` へ直接発行する。公開 `EventFilter` enum と既存ロガーは変更しない |

## 初回 red の採証

| コマンド | 終了値 | 証拠 | 診断 |
|:---|---:|:---|:---|
| `cargo test -p fraktor-actor --test marker_logging_spec -- --nocapture` | 101 | `.takt/issue-108-evidence/stage1-testprobe-target.log`、`stage1-testprobe-summary.md`（正規 checkout） | 製品 API 不足のみ 4 件。`LogMarkerAdapter`（E0432）、`property::LogMarker`（E0432）、`Info::marker()`（E0599）、`Error::cause_message()`（E0599） |

実テストの実行数は 0 である。実行時に 2 件が失敗したという意味ではなく、製品 API 不足による意図したコンパイル失敗である。[Rust テスト](../../crates/fraktor-actor/tests/marker_logging_spec.rs) の初回 SHA-256 は第 1 工程の記録では `dea9af5c63591fbdb377400be701ad32c6b0291a3d83f85b4c438b8b0be0cc23`。初回ファイルからの変更は、公開入口を `Logging` 配下に集約する import の調整、Info のマーカーを `Option` として観測する比較への調整、原典第2ケースへの Error のマーカー保持チェックの追加である。

## 最終の実行結果と再現コマンド

作業ツリー（HEAD `14626a9cf2f7459b285f2e457ca9e227e59c7efc`、第 1 工程統合済み）で、Info/Error の `with_marker` を crate 内に限定した修正後に再実測した。ログは `.takt/issue-108-evidence/stage2-fix-*` に保存し、終了値は各 `.exit` に記録した。

| コマンド | 終了値 | 実測 |
|:---|---:|:---|
| `cargo test -p fraktor-actor --test marker_logging_spec` | 0 | 2 passed、0 failed |
| `cargo test -p fraktor-actor` | 0 | 270 passed、0 failed |
| `cargo test -p fraktor-testkit` | 0 | 61 passed、0 failed |
| `cargo test -p fraktor-actor --test log_marker_contract_test` | 0 | 1 passed、0 failed |
| `cargo fmt --check` | 0 | 差分なし |
| `cargo clippy --workspace --all-targets -- -D warnings` | 0 | 警告なし |
| syn-check（`crates/fraktor-actor`、Release/locked） | 0 | violations 0、parse_failures 0 |
| syn-check（`crates/fraktor-testkit`、Release/locked） | 0 | violations 0、parse_failures 0 |

| 台帳コマンド | 終了値 | 実測 |
|:---|---:|:---|
| `uv run --with pyyaml python3 ledger/tools/check_needs.py --ledger ledger/nodes.yaml --capabilities ledger/capabilities.yaml --goal G0issue108` | 0 | 対象 1 ノード、needs 閉性 OK |
| `uv run --with pyyaml python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal G0issue108` | 0 | 1 ノード、green 1、完了 |
| `uv run --with pyyaml python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal G0a` | 0 | 2 ノード、green 2、完了 |
| `uv run --with pyyaml python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal G0b` | 0 | 4 ノード、green 4、完了 |
| `uv run --with pyyaml python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal G0c` | 0 | 2 ノード、green 2、完了 |
| `uv run --with pyyaml python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal G0d` | 0 | 2 ノード、green 2、完了 |
| `uv run --with pyyaml python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal G0e` | 0 | 2 ノード、green 2、完了 |
| `uv run --with pyyaml python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal G0f` | 0 | 2 ノード、green 2、完了 |

needs と各 rollup は、既定 uv キャッシュへの一時ファイル作成が拒否される場合に備え `UV_CACHE_DIR=.takt/issue108-uv-cache` を設定して実行した。台帳・検証ロジックは変更していない。

対象 2 件は actor 回帰に含まれるため重複加算しない。対象台帳 1 ノードの `status: green`、`cases.total/ported/passing: 2`、`excluded: []`、`rust.crate: fraktor-actor`、`rust.test_file: crates/fraktor-actor/tests/marker_logging_spec.rs`、`goal: G0issue108` を記録した。対象外ノードと能力定義も変更していない。

## 補助の互換テスト

`crates/fraktor-actor/tests/log_marker_contract_test.rs` は原典 2 ケースとは別に、既存回帰では漏れる 1 点だけを確かめる補助テスト 1 件である。台帳の移植件数には加算しない。

| 契約 | 観測する不変条件 |
|:---|:---|
| 無印イベント | `Info::new` / `Error::new` がマーカーを持たない（原典 2 ケースはマーカー付きイベントしか観測せず、既定でマーカーが付く誤実装を検出できないため） |

`Error::with_cause` の `Debug` 限定と、既存コンストラクターの本文・source・class・MDC 保持は、既存回帰（`cargo test -p fraktor-testkit` の `test_event_listener_spec` など）で既に確認されるため、重複するケースは追加しない。

## 採証の出典と確認限界

初回 red は正規 checkout の保存採証（`.takt/issue-108-evidence/stage1-testprobe-target.log`、`stage1-testprobe-summary.md`）を参照し、この作業ツリーでは再実行していない。最終の実測は作業ツリーで実行し、`stage2-fix-target.log`、`stage2-fix-actor.log`、`stage2-fix-testkit.log`、`stage2-fix-contract.log`、`stage2-fix-fmt.log`、`stage2-fix-clippy.log`、`stage2-fix-syn-check-actor.log`、`stage2-fix-syn-check-testkit.log`、`stage2-fix-check-needs.log`、`stage2-fix-rollup-<goal>.log` と各 `.exit` に対応する。実行コマンドと終了値の一覧は `stage2-fix-commands.json` に記録した。ログは `.takt/.gitignore` により追跡対象外である。
