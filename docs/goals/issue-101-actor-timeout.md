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

## 検証

本番の変更前に、実アクターへの到達を観測する両ケースを実行し、出力と原典の対応を保存する。呼び出し側の待機上限に達した場合とは異なる結果で ask の期限切れを判定する。既存実装で通る場合は、期限処理を無効にした隔離環境で負の対照も確認する。本番を変更する場合は classic ask の期限に範囲を絞り、既存の `ask` と `await_result` の呼び出しを維持する。

独立した等価性確認後、対象2ケース、actor と testkit の回帰、G0issue101 の `check_needs.py` と `rollup.py`、および G0a〜G0f の `rollup.py` を確認する。
