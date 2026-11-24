# ゴール G0issue162：AbstractFSMActorTest と AbstractFSMWithStashActorTest

状態：**判断待ち**。先行テストは red（コンパイル失敗）。製品の変更は確定していない。

[Issue #162](https://github.com/j5ik2o/fraktor-ng-rs/issues/162) と [Issue #163](https://github.com/j5ik2o/fraktor-ng-rs/issues/163) を、同じ FSM 基盤を使う 1 つの移植単位として扱う。原典は Pekko v1.7.1（commit `9a54d1294c3ac7821bbed3c940769c3d5027d1ff`）の [AbstractFSMActorTest.java](https://github.com/apache/pekko/blob/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/actor-tests/src/test/java/org/apache/pekko/actor/AbstractFSMActorTest.java) と [AbstractFSMWithStashActorTest.java](https://github.com/apache/pekko/blob/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/actor-tests/src/test/java/org/apache/pekko/actor/AbstractFSMWithStashActorTest.java)。

## 対象ノード

| id | Rust の統合テスト | 原典のケース数 | 状態 |
|:---|:---|---:|:---|
| `actor-tests/src/test/java/org/apache/pekko/actor/AbstractFSMActorTest.java` | `crates/fraktor-actor/tests/abstract_fsm_actor_test.rs` | 1 | red |
| `actor-tests/src/test/java/org/apache/pekko/actor/AbstractFSMWithStashActorTest.java` | `crates/fraktor-actor/tests/abstract_fsm_with_stash_actor_test.rs` | 1 | red |

原典のケースは `canCreateFSM` と `canCreateFSMWithStash` の 2 件で、除外は 0 件。needs（`pekko-spec`、`expect-msg`、`test-probe`）は G0b と G0c で充足している（`check_needs.py --goal G0issue162` は「needs 閉性 OK」）。

## 既存の公開面との照合

起点は `origin/main` の `c7ba2f783dc2451b70c826c9a91ee9a8839ff0be`。

- `crates/` に FSM と stash の実装はない（`grep -rli "fsm\|stash" crates` は 0 件）。
- `Actor` はトレイトで、振る舞いの切り替えは `ActorContext` の `change_behavior`、`push_behavior`、`pop_behavior`（`BehaviorStack`）が担う。状態名、状態データ、遷移の通知はない。
- 受信箱はアクターごとではない。シャードごとの局所キュー（`ShardQueue`）が 1 本あり、末尾に積む操作しかない。Pekko の `unstashAll` は受信箱の先頭に戻す（`DequeBasedMessageQueue.enqueueFirst`）が、それに当たる操作はない。
- `ActorContext` は処理中のメッセージを保持していない。Pekko の `stash()` は引数なしで、処理中のメッセージを退避する。

このため、原典の実行時契約は既存の型だけでは満たせない。新しい公開面（FSM の型、状態遷移の値、stash の操作）と、executor の内部変更（先頭への戻し）が要る。原典と一対一に決まる形がないため、製品の変更は確定させず、判断を受ける。

## 先行テストと red の証拠

テストは提案する API（下の候補 A）で書き、製品より先に置いた。実際の actor を `ActorSystem::actor_of` で起動し、`TestProbe` で観測する。テスト専用の FSM は作っていない。

| テスト | 原典のケース | 観測する契約 |
|:---|:---|:---|
| `can_create_fsm` | `canCreateFSM` | 起動直後に `Transitioning from start to start.`、`next` の配送後に `Transitioning from start to next.` が probe に届く |
| `can_create_fsm_with_stash` | `canCreateFSMWithStash` | `work`、`go` の順に配送する。`start` 状態は `work` を stash し、`go` で `unstashAll` して `next` に移る。`next` 状態が受けた `work` が probe に届く |

red の確認（テストの SHA `80f1ca97e8e117d3ec104713def914d3449d1b27`、製品は未変更）：

| コマンド | 終了値 | 結果 |
|:---|---:|:---|
| `cargo test -p fraktor-actor --test abstract_fsm_actor_test` | 101 | E0432（`kernel::actor::Fsm`、`FsmState` が無い）。実行 0 件 |
| `cargo test -p fraktor-actor --test abstract_fsm_with_stash_actor_test` | 101 | E0432 と E0599 2 件（`ActorContext::stash`、`unstash_all` が無い）。実行 0 件 |

これはコンパイル失敗であり、実行した 1 件の失敗ではない。already_green ではない。

## 判断を求める点と候補

### 候補 A（推奨）：値として組み立てる `Fsm` と、`ActorContext` の stash

- `kernel::actor::Fsm<S, D>`：`Actor` を実装する。`Props` の工場の中で `start_with`、`when`、`on_transition`、`initialize` を呼んで組み立てる。工場は原典のコンストラクタ本体に当たる。状態関数は `FnMut(&mut ActorContext, &Message, &D) -> Option<FsmState<S, D>>`、遷移ハンドラは `FnMut(&mut ActorContext, &S, &S)`。`initialize` の `start -> start` の通知は、context が使える最初の時点の `pre_start` で行う。
- `kernel::actor::FsmState<S, D>`：`goto`、`stay`、`using`。`goto` は同じ状態でも通知し、`stay` は通知しない（Pekko の `notifies` と同じ）。
- `ActorContext::stash` と `ActorContext::unstash_all`：stash はアクターごとに executor が持つ。`unstash_all` は退避した順のまま局所キューの先頭に戻す。再起動では旧インスタンスの `pre_restart` の前に戻し（`UnrestrictedStash.preRestart`）、停止では捨てる。
- 既存の公開 API は変えない。追加のみ。kernel は no_std のまま（`alloc` だけを使う）。
- 試作：ローカルブランチ `j5ik2o/issue-162-163-classic-fsm-candidate` の `d131c29`（+283/−1、6 ファイル）。下の検証を通っている。

### 候補 B：`Fsm` をトレイトにする

`trait FsmActor { type State; type Data; ... }` を実装させ、状態関数をメソッドで返す形。Java のサブクラスに近く見えるが、原典の登録の順序（コンストラクタで `when` を重ねる）とは一対一にならない。Java/Scala の継承 ABI は範囲外なので、近さの利点が無い。型が増え、試作もない。推奨しない。

### 候補 C：stash を `Fsm` の中だけに持ち、`unstash_all` で自分に送り直す

executor を変えずに済む。ただし送り直したメッセージは受信箱の末尾に付くため、すでに並んでいるメッセージより後に処理される。Pekko の「先頭に戻す」と順序が違う。今回の 2 ケースは通るが、原典の契約からずれるので推奨しない。

### 候補 A で原典と違う点（範囲外として残す）

| 項目 | Pekko | 候補 A |
|:---|:---|:---|
| 処理されないイベント | `whenUnhandled` の既定は警告を記録して `stay` | 記録せずに `stay` |
| 存在しない状態への `goto` | 理由 `Failure` で終了し、`onTermination` を呼ぶ | アクターを止める。`onTermination` は無い |
| 停止時の stash | 受信箱に戻し、dead letters へ送る | 捨てる |
| stash の容量と mailbox の要件 | `stash-capacity`、`DequeBasedMessageQueueSemantics` の要求 | 上限なし。要件の検査なし |
| タイマー、状態のタイムアウト、遷移の購読 | あり | 無い（今回の原典は使わない） |

### 影響と戻し方

- 影響：公開面に `Fsm`、`FsmState`、`ActorContext::stash`、`ActorContext::unstash_all` が増える。executor にはアクターごとの stash 表と、局所キューの先頭への戻しが増える。既存の API と既存テストの結果は変わらない（試作で workspace 830 件 pass）。
- 戻し方：追加のみなので、製品のコミットを revert すれば元に戻る。台帳の 2 ノードは `red` に戻す。

## 試作（候補 A）の検証

試作ブランチ `d131c29` で実行した。

| コマンド | 終了値 | 結果 |
|:---|---:|:---|
| `cargo test -p fraktor-actor --test abstract_fsm_actor_test --test abstract_fsm_with_stash_actor_test` | 0 | 各 1 passed、0 failed |
| `cargo test --workspace` | 0 | 830 passed、0 failed、0 ignored（73 結果） |
| `cargo clippy --workspace --all-targets -- -D warnings` | 0 | 警告なし |
| `cargo fmt --check` | 0 | 差分なし |
| syn-check（`crates/fraktor-actor`、`crates/fraktor-testkit`、Release/locked） | 0 | 300 ファイル、violations 0、parse failures 0 |
| `npm run check:aislop` | 0 | accepted と一致 |

負の対照（1 つずつ一時的に入れ、毎回元に戻した。最終の差分には含めない）：

| 変異 | 結果 |
|:---|:---|
| 遷移ハンドラを呼ばない | `can_create_fsm` が `test_kit_base.rs:166` で失敗（exit 101） |
| `initialize` の初期遷移を行わない | `can_create_fsm` が `test_kit_base.rs:166` で失敗 |
| `unstash_all` で stash を捨てる | `can_create_fsm_with_stash` が `test_kit_base.rs:156` で失敗 |
| `goto` で状態を変えない | `can_create_fsm_with_stash` が `test_kit_base.rs:156` で失敗 |

## G0 と対象ゴールの確認（テストのブランチ）

| コマンド | 結果 |
|:---|:---|
| `rollup.py --goal G0a`〜`G0f` | 各ゴール「完了: 全ノードが green か do_not_port」 |
| `check_needs.py --goal G0a`〜`G0f` | 各ゴール「needs 閉性 OK」 |
| `rollup.py --goal G0issue162` | 2 件とも red、未完了 2 件 |
| `check_needs.py --goal G0issue162` | needs 閉性 OK |
