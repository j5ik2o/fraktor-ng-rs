# ゴール G0issue101：classic ActorTimeoutSpec

[Issue #101](https://github.com/j5ik2o/fraktor-ng-rs/issues/101) の対象は Pekko v1.7.1 の classic ask。原典では、`Props.empty` で作った実アクターに `"hallo"` を送り、返信がないため ask に設定した期限で `AskTimeoutException` が発生することを確かめる。ask には 200 ms、呼び出し側の待機にはさらに 500 ms の余裕を与える。警告のミュートはフィクスチャの設定に含まれる。 [Pekko v1.7.1 の原典](https://github.com/apache/pekko/blob/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/actor-tests/src/test/scala/org/apache/pekko/actor/ActorTimeoutSpec.scala#L25-L46)

## 対象ノード

| id | Rust の統合テスト | 原典のケース数 |
|:---|:---|---:|
| `actor-tests/src/test/scala/org/apache/pekko/actor/ActorTimeoutSpec.scala` | `crates/fraktor-actor/tests/actor_timeout_spec.rs` | 2 |

原典のケース名は `use implicitly supplied timeout` と `use explicitly supplied timeout`。前者はフィクスチャ側の既定値を受け取る経路、後者は呼び出し側が期限を明示する経路に対応する。除外は0件。台帳の `extends-pekko-spec` と `pekko-spec` は G0b、`event-filter` は G0d で充足している。

## write_scope

- 対象の統合テストと、それを通すために必要な classic ask の期限処理。
- 対象ノードの台帳欄とこのゴール文書。

後続の検証担当は Rust 本番・テストを編集せず、原典違反があれば報告して停止する。台帳は実測と一致すれば再書き込み不要。PR・issue 操作、最終 CI・マージは同担当の対象外とする。

## 原典と実行経路の独立照合

Pekko v1.7.1 commit `9a54d1294c3ac7821bbed3c940769c3d5027d1ff` の `ActorTimeoutSpec.scala` 全文と、統合済み commit `0cbb76cd1169d1fc753a4dba7d76a355bb61d326` のコードを照合した。

| 原典 | Rust の入口 | 直接観測する契約 |
|:---|:---|:---|
| L34–39 `use implicitly supplied timeout` | `TimeoutFixture::ask` → `ActorRef::ask_with_timeout` | fixture が保持する期限を使い、実配送・無返信・ask 固有期限を確認 |
| L41–45 `use explicitly supplied timeout` | caller が `explicit_timeout` を生成 → `ActorRef::ask_with_timeout` | caller の明示期限を使い、同じ契約を確認 |

対応する [Rust テスト](../../crates/fraktor-actor/tests/actor_timeout_spec.rs) は2件、除外なし。`NonReplyingActor::receive` は `hallo` の受信だけを `AtomicBool` に記録し、返信を送らない。受信フラグの assertion により、未配送のまま期限切れを返す経路を成功として扱わない。

fixture は `FraktorSpec::test_kit_settings()` の `dilated` を通して200 msと500 msを個別に生成する。[TestKitSettings](../../crates/fraktor-testkit/src/test_kit_extension/test_kit_settings.rs) の現在の time factor は1.0なので、ask の期限は200 ms、呼び出し側の待機上限は200 + 500 = 700 msとなる。500 msは待機余裕であり、ask の期限へ足さない。fixture 所有の200 msは、このテストの既定値であり、TestKitSettings の汎用 default timeout（5秒）とは別である。

[ask の実装](../../crates/fraktor-actor/src/adaptor/ask_future.rs) は `ask_with_timeout` の送信時点で期限を生成し、`start_ask` が共有 `AskState` に保持して `tell` で配送する。`try_await_result` は別の待機上限を生成する。`AskState::outcome_at` は期限に達した未完了の ask を失敗として確定する。`result_for_wait` は単一の driver ステップが両期限を過ぎても、先に達した期限を `AskError::AskTimedOut` または `AskError::WaitTimedOut` として返す。

[FraktorSpec](../../crates/fraktor-testkit/src/fraktorspec/fraktor_spec.rs) は system 作成時に `attach_wait_driver` を呼ぶ。[待機 driver](../../crates/fraktor-actor/src/adaptor/ask_driver.rs) は宛先と同じ shard のキーを使い、`try_await_result` → `drive_until` → `WeakActorSystem::run_one` → executor の `deliver` を通じてアクターを実行する。原典の background dispatcher に対し、この移植では待機スレッドが system を駆動する。単に待機して配送を省略するテストではない。

両テストの結果型 assertion は `Err(AskError::AskTimedOut)` だけを受け入れ、返信成功や `WaitTimedOut` を拒否する。経過時間が設定した ask 期限以上であることも assertion で確認する。個別の経過時間は未記録であり、下記の実行時間は対象テスト全体の表示値であって、実行環境による遅延の上限保証ではない。

原典の警告 mute は fixture 設定であり、警告件数を検証するケースではない。Rust も `EventFilter::warning()` を登録し、fixture の `Drop` で system 終了とフィルタ解除を行う。原典のメッセージ pattern と Rust の警告フィルタは同一の絞り込みではないが、警告内容・件数を ask の成功条件には使っていない。

旧 API の `ActorRef::ask` は期限指定なしで送信し、`AskFuture::await_result` は返信を返すか待機上限で panic する入口を保持する。actor 回帰には `pipe_to` の返信配送、既存の `await_result` による返信取得、返信がない場合の待機上限を確認するテストを含む。

## 初回失敗と隔離候補の採証

| 採証 | 確認した内容 |
|:---|:---|
| `accepted-test-only.rs`、`first-source-equivalent-cargo.log`、`test-report.md` | 原典対応2件を本番編集前に実行しようとしたが、`AskError`、`ask_with_timeout`、`try_await_result` の不足によりコンパイル失敗。診断4件、終了コード101、実テスト実行数0。実行時に2件が失敗したという意味ではない |
| `verify-red.json` | `classification: expected_compile_error`、`red_recorded: true`、`green_recorded: false`、`original_cases: null`、`passed: null`、`excluded: 0`。機械判定を変更せず、原典の2つの `in` と実テストで件数2を確認 |
| `legacy-ask.log` | 旧 API の別の補助実験。`delivered=true`、`elapsed=701.4975ms`、`wait_limit=700ms`、caller の待機上限による panic、`future_completed=false`。ask 固有の200 ms期限を検証した成功ではない |
| `candidate-edge.log` | 初期候補の補助3件中2件成功、1件失敗。単一 driver ステップが両期限を過ぎる場合、短い待機上限より ask 期限を優先して `Err(AskTimedOut)` とした誤分類を記録 |
| `candidate-fixed-edge.log` | 修正後候補の原典2件と補助3件が成功。補助ケースは、期限前の返信を保持すること、短い待機と遅延返信を分離すること、単一ステップが両期限を過ぎても先着期限を分類することを確認 |

採用した compile-red は run `20261122-152852-github-issue-101-https-github-8ctk3b` の原典対応版。先行の EchoActor と返信成功を使う非等価な差分は却下され、本番差分なしで停止したものとして区別する。

## 期限無効化の否定対照

| 実行対象とログ | 共通する観測結果 | 報告された終了コード |
|:---|:---|---:|
| 隔離候補：`candidate-no-deadline-control.log`／統合済み commit の隔離展開：`no-deadline-0cbb76c.log` | ask の期限指定だけを無効化すると原典2件が両方 `Err(WaitTimedOut)` で失敗、表示時間0.70秒 | 各101 |

統合済み commit では期限指定の復元後に同じ2件が exit 0で再び成功したと報告されている。

## 検証結果と再現コマンド

対象は commit `0cbb76cd1169d1fc753a4dba7d76a355bb61d326`。隔離展開の保存採証と、2026-11-23（JST）の同 HEAD 作業ツリーでの独立実行を比較した。作業ツリーの Rust 本番・テストに変更はない。

| コマンド | 共通する観測結果 | 統合 commit の隔離展開：ログ／報告終了コード | 作業ツリー：ログ／実測終了コード |
|:---|:---|:---|:---|
| `cargo test -p fraktor-actor --test actor_timeout_spec` | 2 passed、0 failed、0 ignored、0 filtered out、表示時間0.20秒 | `target-0cbb76c.log`／0 | `target-current.log`／0 |
| `cargo test -p fraktor-actor` | 合計249 passed、0 failed、0 ignored、0 filtered out | `actor-0cbb76c.log`／0 | `actor-current.log`／0 |
| `cargo test -p fraktor-testkit` | 合計61 passed、0 failed、0 ignored、0 filtered out | `testkit-0cbb76c.log`／0 | `testkit-current.log`／0 |

| 作業ツリーのコマンド | 終了コード | 実測 |
|:---|---:|:---|
| `uv run --with pyyaml python3 ledger/tools/check_needs.py --ledger ledger/nodes.yaml --capabilities ledger/capabilities.yaml --goal G0issue101` | 0 | 対象1ノード、needs 閉性 OK |
| `uv run --with pyyaml python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal G0issue101` | 0 | 1ノード、green 1、完了 |
| `uv run --with pyyaml python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal G0a` | 0 | 2ノード、green 2、完了 |
| `uv run --with pyyaml python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal G0b` | 0 | 4ノード、green 4、完了 |
| `uv run --with pyyaml python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal G0c` | 0 | 2ノード、green 2、完了 |
| `uv run --with pyyaml python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal G0d` | 0 | 2ノード、green 2、完了 |
| `uv run --with pyyaml python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal G0e` | 0 | 2ノード、green 2、完了 |
| `uv run --with pyyaml python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal G0f` | 0 | 2ノード、green 2、完了 |
| `git diff --check` | 0 | 差分の空白エラーなし |

needs と各 rollup は、既定 uv キャッシュへの一時ファイル作成が拒否されて各 exit 2となった後、`UV_CACHE_DIR=.takt/issue101-uv-cache` を設定して再実行した。上表は再実行の結果であり、台帳・検証ロジックは変更していない。

対象2件は actor 回帰に含まれるため重複加算しない。補助プローブ、needs、rollup も原典件数に加算しない。対象台帳1ノードの `status: green`、`cases.total/ported/passing: 2`、`excluded: []`、`rust.crate: fraktor-actor`、`rust.test_file: crates/fraktor-actor/tests/actor_timeout_spec.rs`、`goal: G0issue101` は実測と一致したため保持した。対象外ノードと能力定義も変更していない。

## 採証の出典と確認限界

過去採証は指定 evidence の保存ファイル、作業ツリーの採証は run の Report Directory に保存されている。後者の終了コードは `cargo-current-results.json`、`ledger-current-results.json`／`ledger-current-retry-results.json`、`diff-check-current-results.json` に記録し、needs と各 rollup の初回・再実行は `needs-current.log`／`needs-current-retry.log`、`rollup-<goal>-current.log`／`rollup-<goal>-current-retry.log`、差分確認は `diff-check-current.log` に対応する。

過去の終了コード（初回 compile-red は保存レポート、否定対照と統合 commit の隔離展開はユーザー報告）、期限無効化・復元の操作順序、採用 run の識別と外側の `must` による自動計数 null の説明、先行差分の却下・停止履歴は、ログ本文から独立に実証した情報と区別する。新たな負の対照や補助プローブは実行していない。

## PR レビューで確認した回帰

期限付き ask では、共有状態が最初の返信の受理時刻を記録する。呼び出し側の待機上限を過ぎてから ask の期限前に返信が届くと、その待機は `WaitTimedOut` を返し、次の待機は保存された返信を受け取れる。期限指定のない既存の `ask` は従来の返信処理を維持する。

追加の単体テストでは、この返信の再取得と、200 ms と異なる明示値が使われることを確認する。実装の期限を200 msに固定すると、後者は `WaitTimedOut` を受けて失敗した。台帳の原典対応は2件、除外0件。

`Duration::MAX` を指定した単体テストは、修正前には期限の計算で panic した。`Instant` で表現できない ask 期限は時計が表せる範囲内では満了せず、呼び出し側の待機上限は適用する。修正後は実アクターに配送され、最初の待機が期限切れになった後、次の待機で返信を取得できた。
