# ゴール G0issue113：ListenerSpec

[Issue #113](https://github.com/j5ik2o/fraktor-ng-rs/issues/113) の対象は Pekko v1.7.1 の `routing.Listeners`。原典は 1 ケースで、broadcaster に 3 名を登録し 1 名を解除した上で、現在の集合への操作で `foo` を届け、別の `foo` の受信を契機に `bar` を配信する。[Pekko v1.7.1 の原典](https://github.com/apache/pekko/blob/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/actor-tests/src/test/scala/org/apache/pekko/actor/routing/ListenerSpec.scala)、管理本体は [Listeners.scala](https://github.com/apache/pekko/blob/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/actor/src/main/scala/org/apache/pekko/routing/Listeners.scala)。

## 対象ノード

| id | Rust の統合テスト | 原典のケース数 |
|:---|:---|---:|
| `actor-tests/src/test/scala/org/apache/pekko/actor/routing/ListenerSpec.scala` | `crates/fraktor-actor/tests/listener_spec.rs` | 1 |

原典のケース名は `listen`。除外は 0 件。台帳の `extends-pekko-spec` と `pekko-spec` は G0b、`test-actor` は G0b、`test-probe`・`fish-for-message`・`expect-no-message` は G0c で充足している（`ledger/capabilities.yaml` の `provided_by` と再照合済み）。最終テストは `expect_msg*` を使わず、順不問の受信を `fish_for_message`、無配送を `expect_no_message` で観測するため、needs は `fish-for-message` を記す。

## 原典とテストの個別照合

Pekko v1.7.1 commit `9a54d1294c3ac7821bbed3c940769c3d5027d1ff` の `ListenerSpec.scala` 全文と `Listeners.scala` 全文を、統合済みの Rust コードと照合した。

| 原典のケース | Rust のケース | 観測する契約 |
|:---|:---|:---|
| `listen` | `listen` | 3 名登録・1 名解除、`WithSubscribers(_ ! "foo")` による現在集合への `foo` 配送、別 `"foo"` 契機の `gossip("bar")` 配送、残る各 2 名の両受信、解除者の無配送、`bar` 総数 2 |
| （なし。補助） | `subscribe_same_listener_twice_delivers_once_per_path` | 同一 actor の重複登録が 1 エントリに留まり、各経路 1 回だけ配送される |

原典は `TestLatch` と `AtomicInteger` で 2 件の `foo` と 2 件の `bar` を待ち、`bar` 総数を数える。Rust は各リスナーの実アクターのメールボックスを、自分の `TestProbe` で有界に観測する。`a1` と `a2` は各自 `foo` と `bar` を期限付きで受信し、`a3` は生存したまま期限付きで何も受信せず、各リスナーは共有カウンタで `bar` を数える。

## 原典との意図的な差異

| 項目 | 原典 | Rust の扱い |
|:---|:---|:---|
| 待ち合わせ | `TestLatch` で 2 件ずつ待つ | 各受信先 `TestProbe` の期限付き観測に置換。順不問の 1 件ずつ受信は `fish_for_message` で次メッセージを取り出して検証し、`expect_no_message` は毎ループで executor の `run_one` を駆動して空なら期限まで有界に待つ。TestLatch を Rust へ移植せず、台帳の `test-latch` 依存を残さない |
| 集合順 | `TreeSet`（ActorRef 順） | 登録順の `Vec`。原典ケースは集合順をアサートしないため、順序一致を契約にしない |
| 受信順 | `fooLatch`/`barLatch` は別々に 2 件ずつ待つだけで順序をアサートしない | 2 経路は別キュー（`WithSubscribers` は broadcaster の `receive` 内で callback 送信、`"foo"` は `ActorContext::tell` で `gossip`）を通るため `Executor::run_one` の取り込み順で `bar` が `foo` を追い越し得る。各 probe は「`foo` と `bar` を順不問で 1 件ずつ」受けたことを検証する。固定 sleep は加えない |
| sender | `gossip` は Pekko `Actor.scala` の暗黙 `self` を修飾子にする | `Actor::receive` 内から `gossip` を呼ぶため配信 sender は通常 broadcaster 自身。原典ケースは sender をアサートしないため、一致を契約にしない |
| 管理 API 名 | `Listeners`、`Listen`、`Deafen`、`WithListeners` | 製品命名規則に従い `Subscribers`、`Subscribe`、`Unsubscribe`、`WithSubscribers` へ改名した |

## 実装

`crates/fraktor-actor/src/kernel/broadcast.rs` が子モジュールを宣言し、直属の公開型を再公開する。集合 `Subscribers` は broadcaster が保持し、`handle_management` が管理メッセージを識別して集合を更新する。`handle_management` は `ActorContext` を受け取らず、戻り値の `bool` は「そのメッセージを管理メッセージとして処理したか」だけを表す（重複 `Subscribe` や存在しない actor への `Unsubscribe` でも集合が変わらず `true` を返す）。`gossip` が現在の集合へ配信する。既存の `ActorRef::tell`、`ActorContext::tell`、`Message` の共有 payload を再利用し、system 全体の共有表・監視・永続化・再試行は追加していない。未使用の公開マーカートレイトは追加していない。

## 初回 red の採証

第 1 工程（issue-113/01-source-red）で、製品未変更のまま `cargo test -p fraktor-actor --test listener_spec` が E0432（`kernel::routing` 不在）、exit 101、実行 0 となることを確認済み。証拠は第 1 工程の run 報告 `.takt/issue-113-evidence/source-red/listener_spec.cargo.log`（相対の証拠名）。これはコンパイル失敗であり、実行 1 件の失敗ではない。入力テスト SHA は `715652d1466e54f197ebd709326c048446b3c184`、統合後ベースは `51a020f1db9711dfbf903005b3c9ab810eab2eef`。

## 負の対照

`crates/fraktor-actor/src/kernel/broadcast/subscriber_registry.rs` に次の変異を一つずつ一時適用し、`cargo test -p fraktor-actor --test listener_spec listen -- --exact`（原典ケース 1 件のみ。補助ケースは実行しない）の失敗を観測して毎回復元した。各実行は 1 件・0 passed/1 failed・exit 101。最終差分に変異は含めていない。

| 変異 | 診断（exit 101、実行 1 件、0 passed/1 failed） |
|:---|:---|
| 解除の無効化（`remove` を no-op） | `listen` が `test_kit_base.rs:255` で「received unexpected message \"foo\"」— a3 への `expect_no_message` で検出 |
| 操作配送の無効化（`for_each` を no-op） | `listen` が `test_kit_base.rs:289` で「timeout (5s) during fishForMessage」— path1 の `foo` が届かない |
| broadcast 配送の無効化（`gossip` を no-op） | `listen` が `test_kit_base.rs:289` で「timeout (5s) during fishForMessage」— path2 の `bar` が届かず 2 件目待ちでタイムアウト |

件数の区分: 台帳 `cases` は原典ケース 1 件（1/1/1、除外 0）を数え、補助ケース `subscribe_same_listener_twice_delivers_once_per_path` は原典外として台帳件数に含めない。統合テストファイルは原典 1 件＋補助 1 件の計 2 件を実行する。

## 最終の実行結果と再現コマンド

| コマンド | 終了値 | 実測 |
|:---|---:|:---|
| `cargo test -p fraktor-actor --test listener_spec` | 0 | 2 passed、0 failed |
| `cargo test --workspace` | 0 | 363 passed、0 failed、0 ignored（44 結果） |
| `cargo fmt --check` | 0 | 差分なし |
| `cargo clippy --workspace --all-targets -- -D warnings` | 0 | 警告なし |
| syn-check（`crates/fraktor-actor`、Release/locked） | 0 | violations 0、parse_failures 0 |
| `cargo test -p fraktor-actor --test listener_spec listen -- --exact` | 0 | 1 passed、0 failed、1 filtered out（原典 1 件のみ） |
| `python3 ledger/tools/check_needs.py --ledger ledger/nodes.yaml --capabilities ledger/capabilities.yaml --goal G0issue113` | 0 | `ゴール G0issue113: needs 閉性 OK` |
| `python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal G0issue113` | 0 | 対象ノード 1 件 green、`完了: 全ノードが green か do_not_port` |
| `python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal G0a`〜`G0f` | 0 | 各ゴール `完了: 全ノードが green か do_not_port` |
