# ゴール G0issue105：classic StatusReplySpec

[GitHub #105](https://github.com/j5ik2o/fraktor-ng-rs/issues/105) の対象は、Pekko v1.7.1 の [Scala classic StatusReplySpec](https://raw.githubusercontent.com/apache/pekko/v1.7.1/actor-tests/src/test/scala/org/apache/pekko/pattern/StatusReplySpec.scala) の9件。台帳ノードは `actor-tests/src/test/scala/org/apache/pekko/pattern/StatusReplySpec.scala`。Rust 側は `crates/fraktor-actor/tests/status_reply_spec.rs` の9件で、除外は0件。基点は `cbfb4a118fa81ea0eb2dac4ec7fa5c07f35679a7`。

## 対象ノード

| id | Rust の統合テスト | 原典のケース数 |
| --- | --- | --- |
| `actor-tests/src/test/scala/org/apache/pekko/pattern/StatusReplySpec.scala` | `crates/fraktor-actor/tests/status_reply_spec.rs` | 9 |

## 原典のケース対応

| Scala 原典 | Rust | 観測する結果 |
| --- | --- | --- |
| `pattern match success` | `success_payload_downcasts_to_the_stored_string_type_and_rejects_int` | 成功値の型消去、`i32` downcast が失敗し、`&'static str` downcast と値 `"woho!"` が一致 |
| `pattern match success (Ack)` | `ack_success_payload_downcasts_to_the_ack_unit_struct` | `StatusReply::ack()` の成功ペイロードが公開 `StatusReplyAck` に downcast し単位値と一致 |
| `not throw exception if null` | `null_reply_is_neither_success_nor_error` | `StatusReply::match_reply(None)` が成功・失敗のどちらにも一致しない |
| `pattern match error with text` | `text_error_reply_carries_a_concrete_error_message` | 失敗状態、具象 `ErrorMessage` と本文 `"boho!"` |
| `pattern match error with exception` | `typed_error_reply_carries_the_original_exception` | 失敗状態、元の `TestException` と本文 `"boho!"` |
| `flatten a Future[StatusReply]` | `flatten_status_future_unwraps_value_ack_and_both_error_kinds` | 1テスト内で原典の成功値・Ack・文字列エラー・型付きエラーの4分岐を検査。受信前の失敗も同じエラーを返す |
| `askWithStatus unwrap success` | `ask_with_status_unwraps_success_value` | 実配送、保存された送信元への返信、展開された成功値 |
| `askWithStatus unwrap Error with message` | `ask_with_status_unwraps_text_error` | 同じ配送と返信経路、`StatusAskError::Reply` 内の `ErrorMessage` と本文 |
| `askWithStatus unwrap Error with exception` | `ask_with_status_unwraps_original_exception` | 同じ配送と返信経路、`StatusAskError::Reply` 内の元の `TestException` と本文 |

Scala の null は Rust の `Option<&StatusReply>` の `None` に対応する。`StatusReply::match_reply` に空入力、成功、文字列エラー、型付きエラーを渡し、抽出結果の型と値を検査する。平坦化テストは完了済み Future を `poll_ready` で直接 poll する。返信前に Future が失敗する場合は、入力と出力の `Arc` が同じエラーを指すことを確認する。ask の3件は `FraktorSpec` の実 ActorSystem に `TestProbe` を登録する。3秒の期限付き ask の要求を `expect_msg` で受信し、保存された送信元に返信して `try_await_result` で結果を取得する。

## 本番変更前の採証

基点 `cbfb4a118fa81ea0eb2dac4ec7fa5c07f35679a7` の本番コードを変更せず、9件のテストを追加し、追加指示2の整形を反映した状態で再実行した。`git diff HEAD -- crates/fraktor-actor/src` は空。

### 実行コマンドと結果

| 項目 | 値 |
| --- | --- |
| 実行コマンド | `cargo test -p fraktor-actor --test status_reply_spec` |
| 基点SHA | `cbfb4a118fa81ea0eb2dac4ec7fa5c07f35679a7` |
| 終了コード | 101（compile-red） |
| 宣言件数 | 9 |
| 実行件数 | 0（コンパイル失敗のため未実行） |
| red 根拠 | 本番 API 不在による診断7件。テストの誤記・fixture 欠陥・環境障害は含まない |

診断7件はすべて、当時の `adaptor` に後続工程で実装予定の本番 API が存在しなかったことによる。

- E0432 `unresolved import fraktor_actor::adaptor::StatusReplyAck`（`status_reply_spec.rs:18`）
- E0599 `ack` not found（`status_reply_spec.rs:51`、`status_reply_spec.rs:97`）
- E0599 `flatten_status_future` not found（`status_reply_spec.rs:92`、`status_reply_spec.rs:97`、`status_reply_spec.rs:101`、`status_reply_spec.rs:106`）

### 生出力（stdout/stderr）

整形後の新ログは `.takt/runs/20261122-173751-github-105-orca-73-1-no-ha-rus-rdr2n3/reports/.takt-report-internal/status_reply_spec.formatted.out`。stdout/stderr を同じファイルへ保存し、その全文を以下へ無加工で挿入した。7418 bytes・144行、SHA-256 は `cfcc30fe162e11bef457fc8993dd8d2750df05c2c57824dca2cf2e9c9001ccb8`。コードフェンス内を抽出して保存ログとのバイト一致を確認した。終了コード101は同ディレクトリの `status_reply_spec.formatted.exit-code` に別途記録した。

```text
   Compiling fraktor-actor v0.0.0 (/Users/j5ik2o/Sources/github.com/j5ik2o/takt-worktrees/20261122T1737-github-105-orca-73-1-no-ha-rus-57702a36e2a5aa7f/crates/fraktor-actor)
error[E0432]: unresolved import `fraktor_actor::adaptor::StatusReplyAck`
  --> crates/fraktor-actor/tests/status_reply_spec.rs:18:5
   |
18 | use fraktor_actor::adaptor::StatusReplyAck;
   |     ^^^^^^^^^^^^^^^^^^^^^^^^--------------
   |                             |
   |                             no `StatusReplyAck` in `adaptor`
   |
help: a similar name exists in the module
   |
18 - use fraktor_actor::adaptor::StatusReplyAck;
18 + use fraktor_actor::adaptor::StatusReply;
   |

error[E0599]: no associated function or constant named `ack` found for struct `StatusReply` in the current scope
  --> crates/fraktor-actor/tests/status_reply_spec.rs:51:28
   |
51 |   let reply = StatusReply::ack();
   |                            ^^^ associated function or constant not found in `StatusReply`
   |
note: if you're trying to build a new `StatusReply` consider using one of the following associated functions:
      StatusReply::success
      StatusReply::error_message
      StatusReply::error
  --> crates/fraktor-actor/src/adaptor/status_reply.rs:21:3
   |
21 |   pub fn success(value: impl Any + Send + Sync) -> Self {
   |   ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
...
26 |   pub fn error_message(message: impl Into<String>) -> Self {
   |   ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
...
31 |   pub fn error(error: impl Error + Send + Sync + 'static) -> Self {
   |   ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

error[E0599]: no associated function or constant named `flatten_status_future` found for struct `StatusReply` in the current scope
  --> crates/fraktor-actor/tests/status_reply_spec.rs:92:39
   |
92 |   let value = poll_ready(StatusReply::flatten_status_future(core::future::ready(StatusReply::success("woho"))))
   |                                       ^^^^^^^^^^^^^^^^^^^^^ associated function or constant not found in `StatusReply`
   |
note: if you're trying to build a new `StatusReply` consider using one of the following associated functions:
      StatusReply::success
      StatusReply::error_message
      StatusReply::error
  --> crates/fraktor-actor/src/adaptor/status_reply.rs:21:3
   |
21 |   pub fn success(value: impl Any + Send + Sync) -> Self {
   |   ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
...
26 |   pub fn error_message(message: impl Into<String>) -> Self {
   |   ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
...
31 |   pub fn error(error: impl Error + Send + Sync + 'static) -> Self {
   |   ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

error[E0599]: no associated function or constant named `flatten_status_future` found for struct `StatusReply` in the current scope
  --> crates/fraktor-actor/tests/status_reply_spec.rs:97:29
   |
97 |     poll_ready(StatusReply::flatten_status_future(core::future::ready(StatusReply::ack()))).expect("flattened ack");
   |                             ^^^^^^^^^^^^^^^^^^^^^ associated function or constant not found in `StatusReply`
   |
note: if you're trying to build a new `StatusReply` consider using one of the following associated functions:
      StatusReply::success
      StatusReply::error_message
      StatusReply::error
  --> crates/fraktor-actor/src/adaptor/status_reply.rs:21:3
   |
21 |   pub fn success(value: impl Any + Send + Sync) -> Self {
   |   ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
...
26 |   pub fn error_message(message: impl Into<String>) -> Self {
   |   ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
...
31 |   pub fn error(error: impl Error + Send + Sync + 'static) -> Self {
   |   ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

error[E0599]: no associated function or constant named `ack` found for struct `StatusReply` in the current scope
  --> crates/fraktor-actor/tests/status_reply_spec.rs:97:84
   |
97 |     poll_ready(StatusReply::flatten_status_future(core::future::ready(StatusReply::ack()))).expect("flattened ack");
   |                                                                                    ^^^ associated function or constant not found in `StatusReply`
   |
note: if you're trying to build a new `StatusReply` consider using one of the following associated functions:
      StatusReply::success
      StatusReply::error_message
      StatusReply::error
  --> crates/fraktor-actor/src/adaptor/status_reply.rs:21:3
   |
21 |   pub fn success(value: impl Any + Send + Sync) -> Self {
   |   ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
...
26 |   pub fn error_message(message: impl Into<String>) -> Self {
   |   ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
...
31 |   pub fn error(error: impl Error + Send + Sync + 'static) -> Self {
   |   ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

error[E0599]: no associated function or constant named `flatten_status_future` found for struct `StatusReply` in the current scope
   --> crates/fraktor-actor/tests/status_reply_spec.rs:101:29
    |
101 |     poll_ready(StatusReply::flatten_status_future(core::future::ready(StatusReply::error_message("boo"))))
    |                             ^^^^^^^^^^^^^^^^^^^^^ associated function or constant not found in `StatusReply`
    |
note: if you're trying to build a new `StatusReply` consider using one of the following associated functions:
      StatusReply::success
      StatusReply::error_message
      StatusReply::error
   --> crates/fraktor-actor/src/adaptor/status_reply.rs:21:3
    |
 21 |   pub fn success(value: impl Any + Send + Sync) -> Self {
    |   ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
...
 26 |   pub fn error_message(message: impl Into<String>) -> Self {
    |   ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
...
 31 |   pub fn error(error: impl Error + Send + Sync + 'static) -> Self {
    |   ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

error[E0599]: no associated function or constant named `flatten_status_future` found for struct `StatusReply` in the current scope
   --> crates/fraktor-actor/tests/status_reply_spec.rs:106:29
    |
106 |     poll_ready(StatusReply::flatten_status_future(core::future::ready(StatusReply::error(TestException("boo")))))
    |                             ^^^^^^^^^^^^^^^^^^^^^ associated function or constant not found in `StatusReply`
    |
note: if you're trying to build a new `StatusReply` consider using one of the following associated functions:
      StatusReply::success
      StatusReply::error_message
      StatusReply::error
   --> crates/fraktor-actor/src/adaptor/status_reply.rs:21:3
    |
 21 |   pub fn success(value: impl Any + Send + Sync) -> Self {
    |   ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
...
 26 |   pub fn error_message(message: impl Into<String>) -> Self {
    |   ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
...
 31 |   pub fn error(error: impl Error + Send + Sync + 'static) -> Self {
    |   ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

Some errors have detailed explanations: E0432, E0599.
For more information about an error, try `rustc --explain E0432`.
error: could not compile `fraktor-actor` (test "status_reply_spec") due to 7 previous errors
```

## 期待される red の説明

第1工程では、後続工程で実装する本番 API（`StatusReply::ack()`、公開 `StatusReplyAck`、`StatusReply::flatten_status_future`）を対象にテストを作成した。基点時点の本番にはこれらが存在しなかったため、テストにローカル定義を置かず、本番 API の不在によるコンパイル失敗を期待成果として採証した。不足は `StatusReplyAck` の未解決 import 1件と、`ack` および `flatten_status_future` の未実装メソッド呼び出し6件で、合計7件だった。当時の実行件数は0で、9件の実行時合否は未確認だった。第1工程はテスト追加と採証のみを対象とし、本番実装は後述の工程で行った。

通常 Future の直接 poll への修正版（整形前）の採証前に、`flatten_status_future` を ask 専用 `.try_await_result` から通常 Future の直接 poll へ変更する前の初期版の生出力も保全している。初期版も同じ本番未変更・同じ基点で、終了コード101、診断7件（`StatusReplyAck` E0432 1件、`ack` E0599 2件、`flatten_status_future` E0599 4件）だった。診断の種類と件数は修正版と同じで、位置だけが編集前の行番号（`status_reply_spec.rs:20`、`:53`、`:94`、`:99`、`:99`、`:104`、`:109`）である。整形前の修正版は同じ未変更本番で再採証し、当時の編集後の行番号（`:18`、`:51`、`:92`、`:96`、`:96`、`:100`、`:105`）で同じ7診断を得た。

第1工程の整形後の再採証では、診断位置は `:18`、`:51`、`:92`、`:97`、`:97`、`:101`、`:106` となった。不足APIによる7診断・終了101・宣言9件・実行0件は変わらなかった。上記の生出力は本番変更前の最終実行を示す。当時の `rustfmt --check crates/fraktor-actor/tests/status_reply_spec.rs` と `cargo fmt --check` はともに終了0で整形差分なし（既存設定のnightly専用項目についてstableから警告あり）だった。旧ログ `status_reply_spec.v1.out`、`status_reply_spec.rev2.out`、`status_reply_spec.verify.out` は同じ保存ディレクトリで上書きせず保持した。

## 第1工程での未確認範囲

- 9件の各 assertion の実行時合否は、当時は本番 API 不在のため未確認（実行0件）だった。API 実装後の再実行で9件が green になることの確認は、後続工程に残していた。
- 台帳更新、既存 #103 テスト・文書の変更、PR 作成、外部公開、canonical main checkout の変更は第1工程の対象外であり、当時は実施しなかった。

第1工程で未確認だった実行時合否と、後続工程で実施した台帳更新の結果は以下に記録する。既存 #103 テスト・文書、PR、外部公開、canonical main checkout は変更していない。

## 実装と検証（本番変更後）

最小実装を行い、対象テスト9件と既存テスト6件の成功を確認した。その後に actor・testkit の全体テストと品質チェックを実行し、台帳更新、needs 閉性と7ゴールの rollup の検証、本ゴール文書への結果記録の順で進めた。

### 最小実装

- `crates/fraktor-actor/src/adaptor/status_reply_ack.rs`（新規）: 公開単位構造体 `StatusReplyAck` を定義（`Clone, Copy, Debug, PartialEq, Eq`）。
- `crates/fraktor-actor/src/adaptor.rs`: 非公開モジュール `status_reply_ack` を宣言し `pub use` で再公開。
- `crates/fraktor-actor/src/adaptor/status_reply.rs`:
  - `StatusReply::ack()` を `Self::success(StatusReplyAck)` として追加。
  - `StatusReply::match_reply` は、返信があれば値か具象エラーへの参照を返す。空入力には `None` を返す。
  - `StatusReply::flatten_status_future` は `Result<StatusReply, Arc<dyn Error + Send + Sync>>` を出力する Future を受け取る。返信が届けば内側の結果を取り出し、Future 自体が失敗すれば元のエラーを返す。
- 既存の `success`/`error_message`/`error`、`get_value`/`get_error`、`ask_with_status`、`StatusAskFuture`、期限付き ask、`result(&self)` は変更していない。

### 実測結果

| コマンド | 成功（passed） | 失敗（failed） | 除外（ignored） | 結果 |
| --- | --- | --- | --- | --- |
| `cargo test -p fraktor-actor --test status_reply_spec --test status_reply_test` | 15（spec 9、test 6） | 0 | 0 | 終了0 |
| `cargo test -p fraktor-actor` | 267 | 0 | 0 | 終了0 |
| `cargo test -p fraktor-testkit` | 61 | 0 | 0 | 終了0 |
| `cargo fmt --check` | — | — | — | 終了0（nightly専用設定の警告のみ） |
| `cargo clippy --workspace --all-targets -- -D warnings` | — | — | — | 終了0 |

全体テストの件数は、各コマンドの全 `test result` 行（doc-test を含め actor 30行、testkit 7行）の passed・failed・ignored をそれぞれ合計した。対象テスト15件による代用ではない。

Ack の rustdoc 英語化後に `cargo run --quiet --release --locked --manifest-path tools/syn-check/Cargo.toml -- --format json crates/fraktor-actor` を実行し、終了0、`clean: true`、109ファイル、違反0件、parse failure 0件を確認した。実行された規則には `rustdoc` が含まれる。

台帳コマンド:

- `check_needs.py --goal G0issue105`: ノード1件、needs 閉性 OK、終了0。

`uv run --with pyyaml python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal <goal>` を次の7ゴールに個別実行した。green 件数は全体集計ではなく、各出力の「ゴール <goal>」行の status 内訳から記録した。

| ゴール | ノード数 | green 件数 | 判定 | 終了コード |
| --- | --- | --- | --- | --- |
| `G0issue105` | 1 | 1 | 完了：全ノードが green か do_not_port | 0 |
| `G0a` | 2 | 2 | 完了：全ノードが green か do_not_port | 0 |
| `G0b` | 4 | 4 | 完了：全ノードが green か do_not_port | 0 |
| `G0c` | 2 | 2 | 完了：全ノードが green か do_not_port | 0 |
| `G0d` | 2 | 2 | 完了：全ノードが green か do_not_port | 0 |
| `G0e` | 2 | 2 | 完了：全ノードが green か do_not_port | 0 |
| `G0f` | 2 | 2 | 完了：全ノードが green か do_not_port | 0 |

### 台帳更新

発行時に TAKT ゴール `68f64af2-9718-4e70-b4ce-dd80fe805e50` へ対象の Scala ノードを記録した。ゴールタグを発行時に台帳へ付ける手順は、`fraktor-porting-ng` が移植対象を選ぶためのもの。今回は本番変更前の採証を独立したタスクに分けるため `simple` を使用し、第2工程で移植と回帰の結果を確認してからタグと実測を台帳に記録した。

`ledger/nodes.yaml` の Scala `StatusReplySpec.scala` 1ノードのみを `status: green`、`cases: total/ported/passing 各9, excluded []`、`rust.crate: fraktor-actor`、`rust.test_file: crates/fraktor-actor/tests/status_reply_spec.rs`、`goal: G0issue105` に更新した。Java 側ノード・既存ゴール・capabilities は変更していない。除外は0件。

### 変更前生出力の位置づけ

採証欄の生出力は第1工程のテストを本番変更前に実行した記録。PR レビュー後は同じ9件の中で、本番の照合 API を通した空入力と、平坦化の返信前失敗も検証した。
