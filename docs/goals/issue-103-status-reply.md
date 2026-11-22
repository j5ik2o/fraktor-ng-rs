# G0issue103 — classic StatusReplyTest

[GitHub #103](https://github.com/j5ik2o/fraktor-ng-rs/issues/103) の対象は、Pekko v1.7.1 の [Java classic StatusReplyTest](https://github.com/apache/pekko/blob/v1.7.1/actor-tests/src/test/java/org/apache/pekko/pattern/StatusReplyTest.java) の6件。台帳ノードは `actor-tests/src/test/java/org/apache/pekko/pattern/StatusReplyTest.java`。Rust 側は `crates/fraktor-actor/tests/status_reply_test.rs` の6件で、除外は0件。

| Java 原典 | Rust | 観測する結果 |
| --- | --- | --- |
| `successReplyThrowsExceptionWhenGetErrorIsCalled` | `success_reply_returns_value_and_rejects_error_access` | 成功状態、値、エラー側に値がないこと |
| `failedReplyThrowsExceptionWhenGetValueIsCalled` | `text_error_reply_returns_error_message_on_value_access` | 失敗状態、エラー本文、値の取得で返る具体的な `ErrorMessage` とその本文 |
| `failedReplyThrowsOriginalExceptionWhenGetValueIsCalled` | `typed_error_reply_returns_original_error_on_value_access` | 失敗状態、エラー本文、値の取得で返る元の `TestException` とその本文 |
| `askWithStatusSuccessReturnsValue` | `ask_with_status_success_returns_value` | 実配送、保存された送信元への返信、展開された成功値 |
| `askWithStatusErrorReturnsErrorMessageExceptionForText` | `ask_with_status_returns_text_error_message` | 同じ配送と返信経路、具体的な `ErrorMessage` とその本文 |
| `askWithStatusErrorReturnsOriginalException` | `ask_with_status_returns_original_error` | 同じ配送と返信経路、元の `TestException` とその本文 |

Java の誤ったエラー側アクセスは Rust の `Option::None` に、値の取得時の例外は `Result::Err` に対応する。classic の成功値は既存の型消去された `Message` として扱う。ask の3件は `FraktorSpec` の実システムに `TestProbe` を登録し、3秒の期限付き ask で送った `request` を `expect_msg` で確認する。probe が保存した送信元に返信し、返信側の送信元には原典の `noSender` に対応する dead letters を指定する。

## 本番変更前の採証

ベース `a02fc0c98e3627927881ffa5140c144babd27d97` の本番コードを使い、6件のテストを追加した状態で `cargo test -p fraktor-actor --test status_reply_test` を実行した。終了コード101。公開入口 `ErrorMessage`、`StatusAskError`、`StatusReply` の未解決 import 3件と `ActorRef::ask_with_status` の未実装1件で compile-red となった。テストの実行件数は0。

TAKT ゴール `bf2902f1-b46e-57c5-a901-732b76667792` の第1工程 `issue-103/01-source-red` は、指定範囲外の本番試作を繰り返したため停止した。ベースから実装差分を含まない状態で、GPT-6.2 Sol の既存 Orca 担当が上記の6件と compile-red を採証し直した。TAKT の停止済みタスクは再投入していない。

## 実装と回帰確認

採証を確定した後、同じ担当が実装と台帳更新を別工程で行った。`ActorRef::ask_with_status` は既存の `ask_with_timeout` に配送と期限を委ねる。待機結果の `StatusReply` から成功値を取り出し、エラーなら元のエラーを保持した `StatusAskError::Reply` を返す。既存の ask の戻り値と期限処理を変更していない。

| コマンド | 実測結果 |
| --- | --- |
| `cargo test -p fraktor-actor --test status_reply_test` | 6 passed、0 failed、0 ignored |
| `cargo test -p fraktor-actor` | 258 passed、0 failed、0 ignored。上記6件と #101 の期限付き ask を含む |
| `cargo test -p fraktor-testkit` | 61 passed、0 failed、0 ignored |
| `uv run --with pyyaml python3 ledger/tools/check_needs.py --ledger ledger/nodes.yaml --capabilities ledger/capabilities.yaml --goal G0issue103` | 1ノード、needs 閉性 OK |
| `uv run --with pyyaml python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal <goal>` | G0issue103 は1ノードが green。G0a〜G0f は順に2、4、2、2、2、2件がすべて green |

actor の合計に対象の6件を含む。台帳は対象ノードだけを更新し、原典6件に対して実測6件、除外0件を記録した。
