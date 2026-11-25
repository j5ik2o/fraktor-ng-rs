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
| `listen` | `listen` | 3 名を登録し、1 名を解除する。`WithSubscribers` で `foo` を送り、broadcaster が別の `foo` を受けて `broadcast` で `bar` を送る。残る 2 名は両方を受け取り、解除した 1 名にはどちらも届かない。`bar` は計 2 件 |
| （なし。補助） | `subscribe_same_listener_twice_delivers_once_per_path` | 同一 actor の重複登録が 1 エントリに留まり、各経路 1 回だけ配送される |

原典は `TestLatch` と `AtomicInteger` で 2 件の `foo` と 2 件の `bar` を待ち、`bar` の総数を数える。Rust では実アクターのリスナーごとに `TestProbe` を使う。`a1` と `a2` は期限内に `foo` と `bar` を各 1 件受け取り、共有カウンタで `bar` の総数を確認する。`a3` はどちらも受け取らず、その確認後に直接送ったメッセージには応答できる。

## 原典との意図的な差異

| 項目 | 原典 | Rust の扱い |
|:---|:---|:---|
| 待ち合わせ | `TestLatch` で 2 件ずつ待つ | 各受信先 `TestProbe` の期限付き観測に置換。順不問の 1 件ずつ受信は `fish_for_message` で次メッセージを取り出して検証し、`expect_no_message` は毎ループで executor の `run_one` を駆動して空なら期限まで有界に待つ。TestLatch を Rust へ移植せず、台帳の `test-latch` 依存を残さない |
| 集合順 | `TreeSet`（ActorRef 順） | 登録順の `Vec`。原典ケースは集合順をアサートしないため、順序一致を契約にしない |
| 受信順 | `fooLatch` と `barLatch` がそれぞれ 2 件の到着を待つ | 各 probe が `foo` と `bar` を 1 件ずつ受信したことを確認する。受信順は問わない |
| sender | `gossip` を Actor 内から呼ぶと暗黙の `self` が使われる | `broadcast` は `ActorContext::tell` で broadcaster 自身を sender にする |
| API 名 | `Listeners`、`Listen`、`Deafen`、`WithListeners`、`gossip` | `Subscribers`、`Subscribe`、`Unsubscribe`、`WithSubscribers`、`broadcast` |

## 実装

`Subscribers` は broadcaster が保持する。`handle_management` は管理メッセージを処理すると `true` を返す。重複した `Subscribe` や未登録の actor を指定した `Unsubscribe` もこれに含まれる。`broadcast` は `ActorContext::tell` で現在の購読先すべてに配信する。各配送は `Message` のペイロードを共有する。

## 初回 red の採証

第 1 工程（issue-113/01-source-red）で、製品未変更のまま `cargo test -p fraktor-actor --test listener_spec` が E0432（`kernel::routing` 不在）、exit 101、実行 0 となることを確認済み。証拠は第 1 工程の run 報告 `.takt/issue-113-evidence/source-red/listener_spec.cargo.log`（相対の証拠名）。これはコンパイル失敗であり、実行 1 件の失敗ではない。入力テスト SHA は `715652d1466e54f197ebd709326c048446b3c184`、統合後ベースは `51a020f1db9711dfbf903005b3c9ab810eab2eef`。

## 負の対照

`crates/fraktor-actor/src/kernel/broadcast/subscriber_registry.rs` に次の変異を一つずつ一時適用し、`cargo test -p fraktor-actor --test listener_spec listen -- --exact`（原典ケース 1 件のみ。補助ケースは実行しない）の失敗を観測して毎回復元した。各実行は 1 件・0 passed/1 failed・exit 101。最終差分に変異は含めていない。

| 変異 | 診断（exit 101、実行 1 件、0 passed/1 failed） |
|:---|:---|
| 解除の無効化（`remove` を no-op） | `listen` が `test_kit_base.rs:255` で「received unexpected message \"foo\"」— a3 への `expect_no_message` で検出 |
| 操作配送の無効化（`for_each` を no-op） | `listen` が `test_kit_base.rs:289` で「timeout (5s) during fishForMessage」— path1 の `foo` が届かない |
| `broadcast` による配送の無効化 | 2 件目の `bar` の待機が `test_kit_base.rs:289` でタイムアウトする |

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

## Pekko v2.0.0 との照合（2026-11-25 追記）

基準を Pekko v2.0.0（`9fbd2283b1cf701c90a1223bf621942c191db61f`）に切り替えた後に照合した。v1.7.1 は v2.0.0 の祖先ではないので、共通祖先 `0ca804c7af` から両方の系統を比べた。上の本文は v1.7.1 で照合したときの記録なので、そのまま残す。ここでは v2.0.0 の位置と判定だけを書く。台帳の `verified_against` は v2.0.0 にした（第3まとまり、`docs/baseline/pekko-v2.0.0-migration/batch-3.md`。第1・第2まとまりで扱ったノードは、それぞれの記録にある）。

- テスト `ListenerSpec.scala` は v1.7.1 と v2.0.0 で blob が同じである。実装 `routing/Listeners.scala` の変更は `@DoNotInherit`（`2214215b7b`）だけである。
- 記録済みの差異 5 件（待ち合わせ、集合の順、受信の順、sender、API 名）は、すべて維持する。
