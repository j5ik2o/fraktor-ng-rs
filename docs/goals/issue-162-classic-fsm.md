# ゴール G0issue162：AbstractFSMActorTest と AbstractFSMWithStashActorTest

状態：**green**。2026-11-25 に run 25 経由で副元帥が候補 A を承認した。原典 2 ケースは pass、除外 0。

[Issue #162](https://github.com/j5ik2o/fraktor-ng-rs/issues/162) と [Issue #163](https://github.com/j5ik2o/fraktor-ng-rs/issues/163) を、同じ FSM 基盤を使う 1 つの移植単位として扱う。原典は Pekko v1.7.1（commit `9a54d1294c3ac7821bbed3c940769c3d5027d1ff`）の [AbstractFSMActorTest.java](https://github.com/apache/pekko/blob/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/actor-tests/src/test/java/org/apache/pekko/actor/AbstractFSMActorTest.java) と [AbstractFSMWithStashActorTest.java](https://github.com/apache/pekko/blob/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/actor-tests/src/test/java/org/apache/pekko/actor/AbstractFSMWithStashActorTest.java)。

## 対象ノード

| id | Rust の統合テスト | 原典のケース数 | 状態 |
|:---|:---|---:|:---|
| `actor-tests/src/test/java/org/apache/pekko/actor/AbstractFSMActorTest.java` | `crates/fraktor-actor/tests/abstract_fsm_actor_test.rs` | 1 | green（1/1/1） |
| `actor-tests/src/test/java/org/apache/pekko/actor/AbstractFSMWithStashActorTest.java` | `crates/fraktor-actor/tests/abstract_fsm_with_stash_actor_test.rs` | 1 | green（1/1/1） |

原典のケースは `canCreateFSM` と `canCreateFSMWithStash` の 2 件で、除外は 0 件。needs（`pekko-spec`、`expect-msg`、`test-probe`）は G0b と G0c で充足している（`check_needs.py --goal G0issue162` は「needs 閉性 OK」）。

## 既存の公開面との照合

起点は `origin/main` の `c7ba2f783dc2451b70c826c9a91ee9a8839ff0be`。

- `crates/` に FSM と stash の実装はない（`grep -rli "fsm\|stash" crates` は 0 件）。
- `Actor` はトレイトで、振る舞いの切り替えは `ActorContext` の `change_behavior`、`push_behavior`、`pop_behavior`（`BehaviorStack`）が担う。状態名、状態データ、遷移の通知はない。
- 受信箱はアクターごとではない。シャードごとの局所キュー（`ShardQueue`）が 1 本あり、末尾に積む操作しかない。Pekko の `unstashAll` は受信箱の先頭に戻す（`DequeBasedMessageQueue.enqueueFirst`）が、それに当たる操作はない。
- `ActorContext` は処理中のメッセージを保持していない。Pekko の `stash()` は引数なしで、処理中のメッセージを退避する。

このため、原典の実行時契約は既存の型だけでは満たせない。新しい公開面（FSM の型、状態遷移の値、stash の操作）と、executor の内部変更（先頭への戻し）が要る。原典と一対一に決まる形がないため、製品の変更を確定させる前に判断を受けた（下の「判断の記録」）。

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

## 判断の記録

2026-11-25、run 25 経由で副元帥が候補 A を承認した。条件は次のとおり。

1. 既存の API を維持し、公開名と配置は `kernel::actor` の命名規則に従う。トレイト化は別の判断とする。
2. `unstash_all` は保留の順を保って受信箱の先頭へ戻す。既に並んでいる通常のメッセージより先に戻ること、保留内の FIFO、再起動、停止、既存の経路を回帰で確かめる。
3. 停止時の dead letters への配送は、小さく実装できるなら同じ PR で実装して確かめる。
4. 上限なしを既定とし、容量の設定やタイマーなど残りの範囲は差異として記録し、原典の該当 suite は台帳で後続に回す。未承認のケース除外や do_not_port は作らない。

### 検討した候補

### 候補 A（推奨）：値として組み立てる `Fsm` と、`ActorContext` の stash

- `kernel::actor::Fsm<S, D>`：`Actor` を実装する。`Props` の工場の中で `start_with`、`when`、`on_transition`、`initialize` を呼んで組み立てる。工場は原典のコンストラクタ本体に当たる。状態関数は `FnMut(&mut ActorContext, &Message, &D) -> Option<FsmState<S, D>>`、遷移ハンドラは `FnMut(&mut ActorContext, &S, &S)`。`initialize` の `start -> start` の通知は、context が使える最初の時点の `pre_start` で行う。
- `kernel::actor::FsmState<S, D>`：`goto`、`stay`、`using`。`goto` は同じ状態でも通知し、`stay` は通知しない（Pekko の `notifies` と同じ）。
- `ActorContext::stash` と `ActorContext::unstash_all`：stash はアクターごとに executor が持つ。`unstash_all` は退避した順のまま局所キューの先頭に戻す。再起動では旧インスタンスの `pre_restart` の前に戻す（`UnrestrictedStash.preRestart`）。停止では、各メッセージを元の送り手のまま、退避した順に dead letters へ送る（承認後に実装。承認前の試作では捨てていた）。
- 既存の公開 API は変えない。追加のみ。kernel は no_std のまま（`alloc` だけを使う）。
- 試作：ブランチ `j5ik2o/issue-162-163-classic-fsm-candidate` の `d131c29`（+283/−1、6 ファイル）。承認後、このブランチへ取り込んだ。

### 候補 B：`Fsm` をトレイトにする

`trait FsmActor { type State; type Data; ... }` を実装させ、状態関数をメソッドで返す形。Java のサブクラスに近く見えるが、原典の登録の順序（コンストラクタで `when` を重ねる）とは一対一にならない。Java/Scala の継承 ABI は範囲外なので、近さの利点が無い。型が増え、試作もない。推奨しない。

### 候補 C：stash を `Fsm` の中だけに持ち、`unstash_all` で自分に送り直す

executor を変えずに済む。ただし送り直したメッセージは受信箱の末尾に付くため、すでに並んでいるメッセージより後に処理される。Pekko の「先頭に戻す」と順序が違う。今回の 2 ケースは通るが、原典の契約からずれるので推奨しない。

### 候補 A で原典と違う点（範囲外として残す）

| 項目 | Pekko | 候補 A |
|:---|:---|:---|
| 処理されないイベント | `whenUnhandled` の既定は警告を記録して `stay` | 記録せずに `stay` |
| 存在しない状態への `goto` | `terminate` で理由 `Failure` を記録してログに出し、`onTermination` を呼ぶ。アクターは止めず、状態も変えない | 状態とデータを変えずに動き続ける（Pekko と同じ）。ログと `onTermination` は無い |
| 停止時の stash | 受信箱に戻し、受信箱の後始末で dead letters へ送る。dead letters は `DeadLetter` を event stream に流す | dead letters へ送る（元の送り手、退避した順）。停止フックや置き換えに失敗した再起動が局所キューへ戻した分も、停止時に局所キューから取り出して送る。port の dead letters は `DeadLetter` をまだ流さないため、利用者からは観測できない。破棄を保証とはしない |
| 停止したアクター宛てで boundary に残る封筒 | 受信箱の後始末で dead letters へ | 従来どおり、後の配送で受け手が見つからず捨てられる（局所キューの分だけを dead letters へ送る） |
| 先頭へ戻す範囲 | アクターごとの受信箱の先頭 | シャードに 1 本の局所キューの先頭。同じアクター宛ての順は Pekko と同じだが、同じシャードの他のアクター宛てのメッセージも追い越す（Pekko はアクター間の順を約束しない） |
| `DeadLetter` の受け手 | 元の受け手を `DeadLetter.recipient` に残す | dead letters の handle に渡るのは送り手とメッセージだけ |
| stash の容量と mailbox の要件 | `stash-capacity`、`StashOverflowException`、`DequeBasedMessageQueueSemantics` の要求 | 上限なし（既定）。要件の検査なし |
| 同じメッセージを 2 回 stash | 現在の配送が stash の末尾と同じなら `IllegalStateException`。`unstashAll` で空にした後は同じ配送でも再び stash できる | 同じ条件で panic。`unstash_all` の後の再 stash は Pekko と同じく受け付ける |
| 同じ監視対象の `Terminated` を複数戻す | `terminatedQueued` は対象ごとに 1 件で、最初に処理した 1 件だけが届く | 同じ（対象ごとに 1 件の通知記録を共有する） |
| タイマー、状態のタイムアウト、遷移の購読 | あり | 無い（今回の原典は使わない） |

### 影響と戻し方

- 影響：公開面に `Fsm`、`FsmState`、`ActorContext::stash`、`ActorContext::unstash_all` が増える。executor にはアクターごとの stash 表、局所キューの先頭への戻し、停止時の dead letters への送り出しが増える。既存の API は変わらない。既存テストはすべて pass のまま。
- 戻し方：追加のみなので、製品のコミットを revert すれば元に戻る。台帳の 2 ノードは `red` に戻す。

### 後続に回す原典の suite

今回の 2 ケースが使わない FSM と stash の機能は、次の台帳ノードが受け持つ。どれも `unported`・`triage: pending` のままで、ゴールのタグは付けていない（タグ付けは発行時に行う）。除外や do_not_port は作っていない。

| 台帳ノード | 受け持つ範囲 |
|:---|:---|
| `actor-tests/src/test/scala/org/apache/pekko/actor/FSMActorSpec.scala` | `whenUnhandled`、`onTermination`、存在しない状態への `goto`、ログ |
| `actor-tests/src/test/scala/org/apache/pekko/actor/FSMTimingSpec.scala` | タイマー、状態のタイムアウト |
| `actor-tests/src/test/scala/org/apache/pekko/actor/FSMTransitionSpec.scala` | 遷移の購読（`SubscribeTransitionCallBack`） |
| `actor-tests/src/test/scala/org/apache/pekko/actor/ActorWithStashSpec.scala` | stash の全般（Scala の `Stash` トレイト） |
| `actor-tests/src/test/scala/org/apache/pekko/actor/ActorWithBoundedStashSpec.scala` | `stash-capacity` と `StashOverflowException` |
| `actor-tests/src/test/scala/org/apache/pekko/actor/DeadLetterSupressionSpec.scala`、`DeadLetterSuspensionSpec.scala` | `DeadLetter` の event stream への発行 |

`DeadLetter` の発行は、停止時の stash だけでなく dead letters 全体に欠けている機能で、上の 2 ノードが既に追跡している。重複する issue は起こしていない。

## 実装

| ファイル | 内容 |
|:---|:---|
| `crates/fraktor-actor/src/kernel/actor/fsm.rs` | `Fsm<S, D>`。`Actor` を実装し、`pre_start` で初期遷移を通知し、`receive` で現在の状態の状態関数を登録順に試す |
| `crates/fraktor-actor/src/kernel/actor/fsm_state.rs` | `FsmState<S, D>`（`goto`、`stay`、`using`） |
| `crates/fraktor-actor/src/kernel/actor/actor_context.rs` | 処理中のメッセージを保持し、`stash` と `unstash_all` を公開する |
| `crates/fraktor-actor/src/kernel/executor/executor_value.rs` | アクターごとの stash 表と配送番号。再起動の前に戻し、`Terminated` は対象ごとの通知記録として積み直す。停止時は stash と局所キューの残りを dead letters へ送り、system の終了で空にする |
| `crates/fraktor-actor/src/kernel/executor/shard_queue.rs` | `prepend_locally`（局所キューの先頭へ順序を保って戻す）、`take_addressed_to`（停止したアクター宛ての封筒を順に取り出す） |

## 回帰のテスト（原典外、台帳の件数に含めない）

| テスト | 確かめること |
|:---|:---|
| `context_stash_test::unstash_all_puts_the_stash_ahead_of_messages_waiting_at_the_boundary` | 外から送られ boundary で待つ `c` より先に、stash した `a`、`b` が届く |
| `context_stash_test::unstash_all_puts_the_stash_ahead_of_messages_in_the_local_queue` | 同じシャードの actor が送り局所キューで待つ `c`、`d` より先に `a`、`b` が届く |
| `context_stash_test::unstash_all_keeps_the_stash_first_in_first_out_across_rounds` | 5 件の stash が退避した順に届く |
| `context_stash_test::a_restart_returns_the_stash_to_the_new_instance_before_later_messages` | 失敗による再起動の後、新しいインスタンスが `a`、`b` を `c` より先に受ける |
| `context_stash_test::a_stop_releases_the_stash_and_a_new_actor_does_not_inherit_it` | `ping` への `pong` で退避を確かめてから止める。停止で退避した payload が解放され、同じ recipe の新しいアクターには渡らない |
| `context_stash_test::terminating_the_system_releases_the_stash_while_the_system_is_held` | system の値を持ったまま `terminate` すると、退避した payload が解放される |
| `context_stash_test::an_unstashed_terminated_is_received_again` | 退避して戻した `Terminated` をもう一度受ける（ActorWithStashSpec の「re-receive unstashed Terminated messages」の実行時契約。ノードは unported のまま） |
| `context_stash_test::an_unwatch_after_unstash_all_cancels_the_unstashed_terminated` | `unstash_all` の直後に `unwatch` すると、戻した `Terminated` は届かない（Pekko の `enqueueFirst` が `terminatedQueuedFor` で通知として積み直すため） |
| `context_stash_test::two_stashed_terminated_for_one_target_are_received_again_once` | 同じ対象の `Terminated` を 2 件 stash して戻すと、再び届くのは 1 件 |
| `context_stash_test::stashed_terminated_for_different_targets_are_each_received_again_once` | 2 つの対象でそれぞれ 2 件ずつ戻すと、各対象 1 件ずつ届く |
| `context_stash_test::a_delivery_may_stash_again_after_unstash_all` | 同じ配送で `stash → unstash_all → stash → unstash_all` が通り、メッセージは 2 回再び届く（Pekko の Java 実測と同じ） |
| `context_stash_test::a_stash_returned_by_the_stop_hook_is_released_by_the_stop` | 停止フックで `unstash_all` した payload が、停止の時点で解放される |
| `fsm_test::stay_using_replaces_the_data_silently_and_goto_the_same_state_notifies` | `stay().using` はデータだけを替えて通知せず、同じ状態への `goto` は通知する |
| `fsm_test::a_goto_into_an_unknown_state_keeps_the_machine_running_in_its_state` | 存在しない状態への `goto` の後も動き続け、`next` で `start->next` を通知する |
| `executor_value::tests::a_stopped_actor_sends_its_stash_to_dead_letters_with_the_original_senders_in_order` | 停止時に stash が元の送り手のまま、退避した順に dead letters へ届く（dead letters を記録用の handle に差し替えて観測） |
| `executor_value::tests::a_stash_returned_by_the_stop_hook_reaches_dead_letters_and_other_actors_keep_their_queue` | 停止フックで戻した stash が停止時に dead letters へ届き、局所キューで待つ他のアクター宛てのメッセージはそのまま届く |
| `executor_value::tests::a_stash_returned_by_a_restart_whose_replacement_fails_reaches_dead_letters` | 再起動が stash を戻した後に置き換えが失敗して止まると、戻した分が dead letters へ届く |
| `executor_value::tests::stashing_the_newest_stashed_delivery_again_is_refused_and_another_delivery_is_not` | stash の末尾と同じ配送の 2 回目は拒まれ、別の配送や `unstash_all` の後は受け付ける |

## 最終の検証

| コマンド | 終了値 | 結果 |
|:---|---:|:---|
| `cargo test -p fraktor-actor --test abstract_fsm_actor_test --test abstract_fsm_with_stash_actor_test` | 0 | 各 1 passed、0 failed（原典 2 件） |
| `cargo test -p fraktor-actor --test context_stash_test --test fsm_test` | 0 | 12 passed と 2 passed、0 failed |
| `cargo test --workspace` | 0 | 848 passed、0 failed、0 ignored（75 結果） |
| `cargo check -p fraktor-actor --lib --no-default-features` | 0 | 成功（kernel の追加は `alloc` と `core` のみ） |
| `git diff --check` | 0 | 指摘なし |
| `cargo clippy --workspace --all-targets -- -D warnings` | 0 | 警告なし |
| `cargo fmt --check` | 0 | 差分なし |
| syn-check（CI と同じく `crates/*` ごと、Release/locked） | 0 | fraktor-actor 256 ファイル、fraktor-testkit 46 ファイル、violations 0、parse failures 0 |
| `npm run check:aislop` | 0 | accepted と一致 |
| `verify_green.py --crate fraktor-actor` | 0 | `all_pass: true` |
| CI の python テスト 5 本（`goal_complete_gate_test.py` ほか） | 0 | すべて成功 |

負の対照（1 つずつ一時的に入れ、毎回元に戻した。最終の差分には含めない）：

| 変異 | 結果 |
|:---|:---|
| 遷移ハンドラを呼ばない | `can_create_fsm` が `test_kit_base.rs:166` で失敗（exit 101） |
| `initialize` の初期遷移を行わない | `can_create_fsm` が `test_kit_base.rs:166` で失敗 |
| `unstash_all` で stash を捨てる | `can_create_fsm_with_stash` が `test_kit_base.rs:156` で失敗 |
| `goto` で状態を変えない | `can_create_fsm_with_stash` が `test_kit_base.rs:156` で失敗 |
| `prepend_locally` を局所キューの末尾への追加にする | `unstash_all_puts_the_stash_ahead_of_messages_in_the_local_queue` が失敗 |
| `prepend_locally` で順序を逆にする | 順序を見る 4 件が失敗 |
| `unstash_all` を自分への再送（候補 C）にする | boundary、局所キュー、再起動の 3 件が失敗 |
| 再起動の前に stash を戻さない | `a_restart_returns_the_stash_to_the_new_instance_before_later_messages` が失敗 |
| 停止時に stash を捨てる | `a_stopped_actor_sends_its_stash_to_dead_letters_with_the_original_senders_in_order` が失敗 |
| 停止時に stash を残す | `a_stop_releases_the_stash_and_a_new_actor_does_not_inherit_it` が失敗 |
| `stop_all` で stash を空にしない | `terminating_the_system_releases_the_stash_while_the_system_is_held` が失敗 |
| `unstash_all` で `Terminated` を通知として積み直さない | `an_unwatch_after_unstash_all_cancels_the_unstashed_terminated` が失敗 |
| 停止時に局所キューの分を dead letters へ送らない | 停止フックと置き換えに失敗した再起動の unit 2 件、`a_stash_returned_by_the_stop_hook_is_released_by_the_stop` が失敗 |
| 同じ対象の通知記録を共有しない | 対象 1 つと 2 つの `Terminated` の 2 件が失敗 |
| 再 stash を処理中のメッセージの取り出しで拒む（修正前） | `a_delivery_may_stash_again_after_unstash_all` が失敗 |
| 末尾と同じ配送の再 stash を拒まない | `stashing_the_newest_stashed_delivery_again_is_refused_and_another_delivery_is_not` が失敗 |
| 存在しない状態への `goto` でアクターを止める（修正前） | `a_goto_into_an_unknown_state_keeps_the_machine_running_in_its_state` が失敗 |

## G0 と対象ゴールの確認

| コマンド | 結果 |
|:---|:---|
| `rollup.py --goal G0issue162` | 2 件とも green、「完了: 全ノードが green か do_not_port」 |
| `check_needs.py --goal G0issue162` | needs 閉性 OK |
| `rollup.py --goal G0a`〜`G0f` | 各ゴール「完了: 全ノードが green か do_not_port」 |
| `check_needs.py --goal G0a`〜`G0f` | 各ゴール「needs 閉性 OK」 |

## PR #164 のレビュー対応（修正 1）

基準 HEAD は `d64980157d2b90ce70bfa145e97018920b024e6e`。

| 指摘 | 分類 | 対応 |
|:---|:---|:---|
| `terminate`（`stop_all`）が executor の stash を解放しない（Devin、Codex、Cursor、CodeRabbit のレビュー本文で重複） | 直す | `stop_all` で `stashes` を空にした。先行の回帰 `terminating_the_system_releases_the_stash_while_the_system_is_held` は修正なしで失敗する。個別停止の dead letters への送り出しは変えていない |
| 停止のテストが退避の前に止めている（Devin） | 直す | `ping` に `pong` を返させて退避を確かめてから止め、payload の解放も確かめる形にした。停止時に stash を残す変異で失敗する |
| 退避した `Terminated` が `unwatch` で取り消されない（Devin） | 直す | Pekko の `UnrestrictedStash.enqueueFirst` は `Terminated` を `terminatedQueuedFor` で積み直すので、その後の `unwatch` で取り消される。`unstash_all` で同じく DeathWatch の通知として積み直した。既存の `unwatch` が待ち中の通知を取り消す契約に揃えただけで、公開 API は増やしていない |

## PR #164 の独立レビュー対応（修正 2）

基準 HEAD は `e3af16b89e3ea459d931706e0d507114740aedc2`。§2.6 の独立レビュー（OpenAI Codex、差し戻し）の指摘と処理。

| 指摘 | 分類 | 対応 |
|:---|:---|:---|
| 停止フックで `unstash_all` した stash が dead letters に届かない。置き換えに失敗した再起動で戻した分も同じ | 直す | 停止の処理で、stash 表を dead letters へ送った後に、局所キューに残る停止したアクター宛ての封筒を順に取り出して dead letters へ送る（Pekko の受信箱の後始末）。他のアクター宛ての封筒と、既存の終了の契約は変えていない。unit 2 件と統合 1 件で確かめた |
| 同じ監視対象の `Terminated` を複数戻すと全件が再び届く | 直す | Pekko の `terminatedQueued` は対象ごとに 1 件なので、`unstash_all` で積み直すとき、同じ監視者と対象の通知記録が待っていればそれを共有する。最初に処理した 1 件だけが届く。単発の再受信と `unwatch` の回帰はそのまま通る |
| 同じ配送の再 stash が Pekko と違い、rustdoc の説明も不正確 | 直す | Pekko は現在の配送と stash の末尾を比べる。配送ごとに番号を付けて同じ比較にした。`unstash_all` の後の再 stash は通る。公開 API の形は変えていない |
| 存在しない状態への `goto` の説明（レビューの照合で判明） | 直す | Pekko の `makeTransition` は `terminate` で失敗を記録するだけで、アクターを止めない。port もアクターを止めず状態を保つ形に揃え、差異表を正した |
| `git diff --check` がゴール文書の末尾の空行を検出 | 直す | 末尾の空行を削除した |

## Pekko v2.0.0 との照合（2026-11-25 追記）

基準を Pekko v2.0.0（`9fbd2283b1cf701c90a1223bf621942c191db61f`）に切り替えた後に照合した。v1.7.1 は v2.0.0 の祖先ではないので、共通祖先 `0ca804c7af` から両方の系統を比べた。上の本文は v1.7.1 で照合したときの記録なので、そのまま残す。ここでは v2.0.0 の位置と判定だけを書く。台帳の `verified_against` は v2.0.0 にした（第3まとまり、`docs/baseline/pekko-v2.0.0-migration/batch-3.md`。第1・第2まとまりで扱ったノードは、それぞれの記録にある）。

- テストの変更は JUnit 5 化と Java の構文だけで、2 ケースの観測は変わらない。照合の記録は `docs/baseline/pekko-v2.0.0-migration/batch-1.md` にある。
- 後続の suite の要件：`63d22b60fe` #3096 で、`SubscribeTransitionCallBack` と `Listen` で登録した listener を FSM が `watchWith` で見張り、止まると自動で外すようになった（すでに watch している相手なら警告を出し、自動では外さない）。`FSMTransitionSpec` を移植するときは、これが観測の対象になる。`2323878ae1` で非推奨の `setTimer` が消えたので、`FSMTimingSpec` は v2.0.0 のケースを数え直してから移植する。
- 記録済みの差異（処理されないイベント、存在しない状態への `goto`、停止時の stash、先頭へ戻す範囲、`DeadLetter` の受け手、stash の容量、同じメッセージの再 stash、`Terminated` の積み直し）は維持する。
