# ゴール G0issue99：ActorSystem がスコープ終了時に終了する

Pekko v1.7.1 の classic ActorSystem のテストを1ケース移植する。原典では `Using(ActorSystem())` を抜けた直後に `whenTerminated.isCompleted` が真になることを確かめている。Rust ではブロックが所有する `ActorSystem` の `Drop` で同じタイミングを検証する。 [Issue #99](https://github.com/j5ik2o/fraktor-ng-rs/issues/99) · [Pekko v1.7.1 の原典](https://github.com/apache/pekko/blob/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/actor-tests/src/test/scala-2.13%2B/org/apache/pekko/actor/ActorSystemScala213PlusSpec.scala#L28-L35)

## 対象ノード

| id | Rust の統合テスト | 実ケース数 |
|:---|:---|---:|
| `actor-tests/src/test/scala-2.13+/org/apache/pekko/actor/ActorSystemScala213PlusSpec.scala` | `crates/fraktor-actor/tests/actor_system_scala213_plus_spec.rs` | 1 |

台帳の `extends-pekko-spec`、`pekko-spec`、`implicit-sender` は G0b で充足している。原典のケース本体はこれらのフィクスチャから提供されるシステムを使わず、`Using` の中で新しいシステムを作る。Rust のテストも観測対象のシステムをブロックの中で作る。除外は0件。

## スコープを抜けた直後の終了を観測する

テストはシステムが使う `ChannelBoundary` と、そこにメッセージを送る `ActorRef` を保持する。ブロック内では送ったメッセージがキューにあり、アクター本体が残っていることを確かめる。ブロックの直後にはアクター本体の解放とキューの排出を確認する。さらに同じ参照から送ってもキューが空のままであることを確認する。

現行実装では `Drop` が `terminate` を呼び、`Executor::stop_all` が同期的に境界を閉じ、アクター本体を解放し、キューを空にする。テスト用アクターのデストラクタは解放の有無を記録する。境界の `has_pending` は状態を読み、`ActorRef::tell` は境界に送信する。テストはこの3つを使い、終了処理の明示呼び出しやブロック外での実行器の駆動を省く。

## 検証結果

| 確認 | 結果 |
|:---|:---|
| 本番の差分がない状態で対象テストを初回実行 | 1件成功、終了コード0 |
| 別の一時チェックアウトで `ActorSystem::drop` の `self.terminate()` だけを外す | ブロック直後のキュー排出のアサーションで1件失敗、終了コード101 |
| 一時チェックアウトで本番実装を戻し、同じテストを再実行 | 1件成功、終了コード0。一時チェックアウトは破棄済み |
| 既存の `verify_red.py` | `unexpected_pass`、`original_cases: null`、`passed: 1`、`excluded: 0` |

原典には `"Scala's Using automatically terminates ActorSystem" in` が1件ある。自動カウンターは外側の `must` も名前付きブロックに数えるため、実ケース数を確定できない。独立レビューではスコープ終了を観測する方法と負の対照を原典に照合し、境界の変化に加えてアクター本体の解放も確かめることとした。台帳の件数は原典と実測に基づく。

## 回収時の台帳更新

準備の TAKT 実行を停止したため、監督者は #99 に限り、回収担当が検証と独立レビューの結果に基づいて台帳を更新する手順を承認した。独立レビューの指摘を受けてアクター本体の解放を検査に加え、対象テストと負の対照を再実行した。本番実装で1件成功し、`Drop` の終了処理を無効にするとスコープ直後のアサーションで1件失敗した。

`verify_red.py` の結果は `unexpected_pass` のまま保存した。actor と testkit は、既存の `verify_green.py --crate <crate>` でテスト、Clippy、フォーマット、syn-check、aislop を確認した。台帳の `green` は、この検証、独立レビュー、原典の1ケースと Rust の1件の通過を根拠とする。

対象テストは `cargo test -p fraktor-actor --test actor_system_scala213_plus_spec` で実行する。最終確認には actor と testkit の回帰、G0issue99 の `check_needs.py` と `rollup.py`、G0a〜G0f の `rollup.py`、PR 最終 HEAD の `Fraktor CI Gate` と `Check unresolved comments` を含める。
