# 設計：#198 FSMTimingSpec／ActorRefSpec の移植

[Issue #198](https://github.com/j5ik2o/fraktor-ng-rs/issues/198) の第1段（設計だけ）の記録である。実装は、別の担当がこの設計をレビューして通ってから行う。第2段では、テストは Claude Opus の担当が先に red で書き、実装の担当はそのテストを書き換えずに通す実装だけを書く。この文書は実装を含まない。

対象は Pekko v2.0.0（commit `9fbd2283b1cf701c90a1223bf621942c191db61f`）の actor-tests にある `FSMTimingSpec` と `ActorRefSpec` である。原典の本体は、actor の `FSM.scala`（名前付きタイマ、状態タイムアウト、`whenUnhandled` の動的な差し替え）と `ActorRef`／`ActorCell`／`LocalActorRefProvider`（生成の失敗、入れ子の生成、子の存在確認、直列化）である。移植先は `crates/fraktor-actor` である。依存の [Issue #180](https://github.com/j5ik2o/fraktor-ng-rs/issues/180)（`receiveWhile`、`TestLatch`）は main に入っている。依存する issue はほかに無い。

進め方の規約は `.takt/facets/knowledge/fraktor-porting-rules.md`、設計文書の前例は [PR #205](https://github.com/j5ik2o/fraktor-ng-rs/pull/205) の `docs/design/issue-177-frequency-sketch.md`、直近の `docs/design/issue-194-specs.md`（FSMTransitionSpec ほか）と `docs/design/issue-181-testfsmref.md`（TestFSMRef、FSM のタイマの表・世代・状態タイマのスロット）に従う。オーナーの指針（`docs/decisions/pekko-semantics-first.md`、main）は、速さより Pekko の意味論を優先することである。Scala・Java の書き方の対応は `docs/decisions/scala-java-to-rust-correspondence.md` を先に引いた（§2.3）。新しい Rust ファイルには、自作のものも含めて Apache-2.0 のヘッダーを付ける（決まりの見直しは [Issue #241](https://github.com/j5ik2o/fraktor-ng-rs/issues/241) で進行中であり、この件では現行の決まりに従う）。

## 1. 何を移植するか

### 1.1 件数の数え直し

原典を `" in {"` の数で数え直した。`ignore` と `pending` はどちらにも無い。

| 原典 | 目安（issue／台帳） | 数え直し |
|:---|---:|---:|
| `actor-tests/src/test/scala/org/apache/pekko/actor/FSMTimingSpec.scala` | 10／0 | **10** |
| `actor-tests/src/test/scala/org/apache/pekko/actor/ActorRefSpec.scala` | 12／12 | **12** |
| 合計 | 22／12 | **22** |

台帳の `FSMTimingSpec` ノードの `test_cases_approx: 0` は目安が入っていない値であり、数え直しは 10 件で issue の目安と一致する。第2段で台帳を更新するときに `cases.total: 10` として記録する。`ActorRefSpec` は issue、台帳、数え直しがすべて 12 件で一致する。

台帳の対象ノードは次の 2 件で、どちらも `status: unported`、`triage.kind: pending` である。

- `actor-tests/src/test/scala/org/apache/pekko/actor/FSMTimingSpec.scala`（needs: extends-pekko-spec、pekko-spec、test-kit、test-actor、implicit-sender、expect-msg、expect-no-message、event-filter、within、await-cond、receive-while、ignore-msg。premise_flags: thread-sleep）
- `actor-tests/src/test/scala/org/apache/pekko/actor/ActorRefSpec.scala`（needs: extends-pekko-spec、pekko-spec、test-actor、default-timeout、expect-msg、expect-terminated、event-filter、test-latch。premise_flags: java-serialization、thread-sleep）

移す先はどちらも `crates/fraktor-actor/tests/` で、ノード 1 件＝ファイル 1 件、ファイルの冒頭に出典（Pekko v2.0.0 のファイルパス）をコメントで残す。

- `crates/fraktor-actor/tests/fsm_timing_spec.rs`
- `crates/fraktor-actor/tests/actor_ref_spec.rs`

### 1.2 対応表：FSMTimingSpec（10 件）

原典の機械は 1 つである。testActor へ遷移と Tick／Tock を送る `StateMachine`（状態は `Initial`、`TestStateTimeout`、`TestStateTimeoutOverride`、`TestSingleTimer`、`TestSingleTimerResubmit`、`TestRepeatedTimer`、`TestUnhandled`、`TestCancelTimer`、`TestCancelStateTimerInNamedTimerMessage`、`TestCancelStateTimerInNamedTimerMessage2`、データは `Int`）と、停止する `StoppingActor` である。移植の機械は本体の `Fsm`（§2.1）を `Props` の factory の中で組む形にし、原典のコンストラクタの呼び出し（`startWith`、`when`、`onTransition`、`initialize`）を写す。ただし原典の `when(state, stateTimeout = …)`、状態関数の中からの `startSingleTimer`／`startTimerWithFixedDelay`／`cancelTimer`、`goto(…).forMax(…)` は、現行の `Fsm` には無い（§2.1）。§3.1 の追加が入って初めて書ける。

原典は実時間で待つ（`TimingTest` タグ、`within` と `dilated`）。移植も実時間で書き、時間の幅は testkit の `dilated`（`TestKitSettings::dilated`）と `FraktorSpec::within`／`within_range` の前例に合わせる。仮想時間（testkit の `ManualScheduler`）へ置き換える案は採らない。原典が確かめるのは「実際の期限が来たら発火し、来る前は発火しない」ことであり、仮想時間では期限の実測にならないためである。

| # | 原典のケース | Rust のテスト名 | 移す／作らない |
|:---:|:---|:---|:---|
| 1 | receive StateTimeout | `receives_a_state_timeout` | 移す（本体の追加が要る。§3.1）。状態 `TestStateTimeout`（状態タイムアウト 800ms）へ移り、`StateTimeout` のイベントで `Initial` へ戻る。500ms〜2 秒の `within_range` で `Transition` を受ける |
| 2 | cancel a StateTimeout | `cancels_a_state_timeout` | 移す（本体の追加が要る）。`TestStateTimeout` へ移った直後に `Cancel` を送ると、状態タイムアウトより先に処理されて `Initial` へ戻り、その後は黙る。普通のメッセージの受信が張ってある状態タイマを取り消す規則（§4 の I1）を見る |
| 3 | cancel a StateTimeout when actor is stopped | `cancels_a_state_timeout_when_the_actor_is_stopped` | 移す（本体の追加が要る）。`StoppingActor` は `Initial`（状態タイムアウト 200ms）で自分を止める。testActor が DeadLetter を購読し、400ms の間 DeadLetter が来ないことを見る。停止でタイマがすべて消える規則（§4 の I4）を見る |
| 4 | allow StateTimeout override | `allows_a_state_timeout_override` | 移す（本体の追加が要る）。`Initial` から `goto(TestStateTimeout).forMax(Duration.Inf)` で移ると、状態自身の 800ms では発火せず（400ms 黙る）、`Cancel` で戻れる。`forMax` の上書きと無限の扱い（§3.1 の `FsmTimeout`）を見る |
| 5 | receive single-shot timer | `receives_a_single_shot_timer` | 移す（本体の追加が要る）。`startSingleTimer("tester", Tick, 500ms)` を張り、`Tick` が 1 回だけ届いて `Initial` へ戻り、その後は黙る |
| 6 | resubmit single-shot timer | `resubmits_a_single_shot_timer` | 移す（本体の追加が要る）。遷移ハンドラ（`Initial -> TestSingleTimerResubmit`）が単一タイマを張り、発火のたびに同じ名前で張り直して `Tick`、`Tock` の順に届く。張り直しで世代が進み、古い印が通らないこと（§4 の I2）を含む |
| 7 | correctly cancel a named timer | `correctly_cancels_a_named_timer` | 移す（形を変える。本体の追加が要る）。原典は `Tick` の処理の中で 1ms のタイマ「hallo」を張り、メールボックスにメッセージが溜まったことを `awaitCond` で確かめてから取り消し、返信して 500ms で張り直す。移植のアクターの中からはメールボックスの滞留を読めない（§2.1）ため、処理の中で「張る→取り消す→返信する→張り直す」を続けて行う形にする。観測できる振る舞い（最初の `Tock` は届かず、張り直した `Tock` が約 500ms で届く）は同じである。「すでにキューへ入った印を取り消しで捨てる」こと自体は、§5 の PBT（P1）と §6 の Quint が担い、このケースの形の変更で弱めない。次に `Cancel` でタイマを取り消して `Initial` へ戻る |
| 8 | not get confused between named and state timers | —（#234 の段 6 の後。§1.4） | この issue では移さない。名前付きタイマの受信が状態タイマを取り消し、状態タイマの印が名前付きタイマの世代で無効にならないこと（[PR #275](https://github.com/j5ik2o/fraktor-ng-rs/pull/275) のレビューで分けた世代の分離）を見るケースだが、原典は `ActorRefWithCell.suspend()`／`resume()` とメールボックスの滞留の観測を使う。どちらも移植には外向けの口が無く、前提は §1.4 に書く。除外にも作らない区分にもしない。台帳の件数からは外さず、red のまま残す形にする |
| 9 | receive and cancel a repeated timer | `receives_and_cancels_a_repeated_timer` | 移す（本体の追加が要る）。`startTimerWithFixedDelay("tester", Tick, 100ms)` を張り、データ 4 から数えて `Tick` が 5 回届き（`receiveWhile`、[Issue #180](https://github.com/j5ik2o/fraktor-ng-rs/issues/180) で入った `TestProbe::receive_while` を使う）、最後に自分で取り消して `Initial` へ戻る |
| 10 | notify unhandled messages | `notifies_unhandled_messages` | 移す（本体の追加が要る。§3.2）。`TestUnhandled` で処理できない `Tick` と `Unhandled("test")` が、警告「unhandled event … in state TestUnhandled」として `EventFilter.warning` に 1 回ずつ掛かる。途中で `SetHandler` が `whenUnhandled` を差し替えると次の `Tick` はテストへ届き、`Cancel` でハンドラを空に戻してから `Initial` へ戻る |

### 1.3 対応表：ActorRefSpec（12 件）

原典のアクターは、返信役の `ReplyActor`、仕事役の `WorkerActor`、送信役の `SenderActor`（`TestLatch` を数える）、転送役の `OuterActor`、内側の `InnerActor` と、それらのコンストラクタの中で別のアクターを作って失敗する変種（`FailingOuterActor`、`FailingInheritingOuterActor`、`FailingInnerActor`、`FailingInheritingInnerActor`）である。移植ではどれも通常の `Actor` 実装として書き、生成は `Props` の factory の中で行う。原典の設定文字列（`pekko.actor.allow-java-serialization = on`）はケース 3〜5 のためのもので、HOCON は写さない（§1.5、§2.3）。

| # | 原典のケース | Rust のテスト名 | 移す／作らない |
|:---:|:---|:---|:---|
| 1 | not allow Actors to be created outside of an actorOf | `does_not_allow_actors_to_be_created_outside_of_an_actor_of` | 移す（形を変える。直接生成の小ケースだけ作らない提案。§1.5）。原典の本体は、コンストラクタの中で別の `Actor` を `new` すると `ActorInitializationException` になり、生成が失敗し、失敗の後も context のスタックが空（＝系が壊れていない）ことである。移植では「生成」＝`Props` の factory の呼び出しであり、factory の中で失敗（panic）した生成は初期化の失敗として表に出て参照を渡さず、その後も系が通常どおりアクターを作れることを見る。失敗の変種（外側で失敗、内側で失敗、継承した変種、2 回失敗、通常の例外がそのまま伝わる）は、factory の中で panic する対象を入れ替えて写す。factory の panic を実行器がどう表に出すか（捕まえて初期化の失敗にするか、そのまま伝えるか）は現行の実装で確定していないため、第2段の最初に小さな確認テストで実測し、その結果に合わせてアサーションの形を決める（§7） |
| 2 | insert its path in a ActorInitializationException | `inserts_its_path_in_an_actor_initialization_exception` | 移す（本体の追加が要る可能性がある。§3.4）。名前 `failingActor` で factory が失敗する生成を行い、初期化の失敗の表出（エラーか失敗のログ）に `/user/failingActor:` のパスが入ることを `EventFilter` 相当かエラーの文面で見る |
| 3 | be serializable using Java Serialization on local node | — | **作らない・除外の提案**（§1.5）。Java Serialization そのものは JVM の仕組みであり、Rust で同じ振る舞いを作れない |
| 4 | throw an exception on deserialize if no system in scope | — | **作らない・除外の提案**（§1.5）。ケース 3 と同じく Java のデシリアライズの振る舞いである |
| 5 | return EmptyLocalActorRef on deserialize if not present in actor hierarchy (and remoting is not enabled) | `returns_an_empty_local_actor_ref_for_a_path_that_is_not_in_the_hierarchy` | 移す（形を変える。バイト列の部分だけ作らない提案。§1.5）。中身の性質は「階層にいないパスを解決すると、何も答えない参照（Pekko の `EmptyLocalActorRef`）が返る」ことであり、移植には `ActorSystem::resolve_actor_ref` がすでにある（§2.2）。`ReplyActor` を名前 `non-existing` で作って止め、そのパスの文字列表現から解決し直した参照が、元のパスを持ち、知らせても返信が無く dead letters へ流れることを見る。Java のバイト列を経由する部分は写さない。なお Pekko の `EmptyLocalActorRef` はパスで等しいが、移植の `ActorRef` の等価性は id と世代で決まる（所有権モデルの決定）ため、解決し直した参照同士の等価はアサーションに入れない。この違いは形の変更としてここに記録する |
| 6 | support nested actorOfs | `supports_nested_actor_ofs` | 移す（形を変える）。原典はコンストラクタの中で `system.actorOf` して入れ子を作る。移植の factory の中では系を借り直せないため、入れ子は `pre_start` で `ActorContext::actor_of_with_strategy`（名前なし）から作り、受けたメッセージにその参照を返す形にする。外側と入れ子が別の参照であることを見る |
| 7 | support advanced nested actorOfs | `supports_advanced_nested_actor_ofs` | 移す。`OuterActor` が `InnerActor` へ `forward`（送信者を保つ）し、`ask` で「innerself」「self」「msg」が正しく返る。`forward` は `ActorRef::forward`／`ActorContext::forward` がすでにある。`ask` は adaptor の `AskFuture` を使う |
| 8 | support reply via sender | `supports_reply_via_sender` | 移す。`SenderActor` が `complex`／`simple` を送り、`ReplyActor` と `WorkerActor` の応答で `TestLatch(4)` が満ちることを 2 巡見る。`WorkerActor` の 1 秒の仕事は原典どおり実時間の sleep（`dilated`）で写す。`TestLatch` は [Issue #180](https://github.com/j5ik2o/fraktor-ng-rs/issues/180) で入っている |
| 9 | support actorOfs where the class of the actor isn't public | `supports_actor_ofs_where_the_type_of_the_actor_is_not_public` | 移す（形を変える）。原典は非公開クラスの `createProps()` 経由で作る。移植では、テストファイルの中の非公開モジュールにアクター型を置き、そのモジュールが公開する factory 関数が返す `Props` で作る形にする。「pigdog」を知らせて同じ文が返ることを見る。リフレクションでコンストラクタを探す仕組みそのものは写さない（対応表の前例、§2.3） |
| 10 | stop when sent a poison pill | `stops_when_sent_a_poison_pill` | 移す。`ask(5)` と `ask(0)` を送ってから `PoisonPill` を送ると、先の 2 件の返信（"five" と "null"）が届いてから止まる。実行器は `PoisonPill` をシステムメッセージとしてすでに扱う（§2.2）。`watch` と `expectTerminated` で停止を見る |
| 11 | restart when Kill:ed | `restarts_when_killed` | 移す。親が `OneForOneStrategy`（再試行 2 回、1 秒の範囲）を持ち、子へ `Kill` を送ると子の `pre_restart` と `post_restart` が `TestLatch(2)` を満ちる。実行器は `Kill` を `ActorKilledException` の失敗としてすでに扱い、`Actor` の `pre_restart`／`post_restart` フックもすでにある（§2.2） |
| 12 | be able to check for existence of children | `can_check_for_the_existence_of_children` | 移す（本体の追加が要る。§3.3）。親が子 `child` を作り、受けた名前について `context.child(name)` の有無を返す。`child` は真、`whatnot` は偽であること `ask` で見る。現行の `ActorContext` に `child` は無い |

### 1.4 #234 を前提にするケース（FSMTimingSpec のケース 8）

ケース 8 は、FSM を suspend した間に名前付きタイマ（1ms）と状態タイマ（`forMax(1ms)`）の両方の期限が来てキューに溜まり、resume の後に名前付きタイマの `Tock` が先に処理されて次の状態へ移り、状態タイマの印は捨てられる（名前付きタイマの受信が状態タイマを取り消す）ことを見る。世代の分離そのものは [PR #275](https://github.com/j5ik2o/fraktor-ng-rs/pull/275) で kernel に入っている。

前提になるのはタイマではなく、セルの外からの suspend／resume と、メールボックスにメッセージが溜まったことの観測（`mailbox.hasMessages` と `TestKit.awaitCond`）である。現行の fraktor-ng-rs では、メールボックスの suspend／resume は `MailboxStatus` の中の数えだけがあり、`ActorRef` や `ActorContext` から呼ぶ口が無い。[Issue #234](https://github.com/j5ik2o/fraktor-ng-rs/issues/234) の設計（`docs/design/issue-234-multithread-executor.md` §1.2）は、外からの suspend／resume を「dispatcher の観測口」と一緒に要るものとして挙げ、その実装を段 6（dispatcher のライフサイクル）に置いている。したがってこのケースは「#234 の段 6 の後」と分けて書き、この issue では移さない。

- ケース 8 は除外（`cases.excluded`）にも作らない区分（`cases.not_built`）にもしない。台帳の `FSMTimingSpec` ノードは `status: unported` のまま、`cases.total: 10`、`ported: 9`、`passing: 9` とし、最上位の `verified_against` は書かない形を第2段に提案する（[Issue #194](https://github.com/j5ik2o/fraktor-ng-rs/issues/194) の `LocalActorRefProviderSpec` の扱いと同じ）。
- 残りの 21 件は #234 に依存しない。FSM の残り 9 件は単一シャードの実行器と実時間のドライバで確かめられ、ActorRefSpec の 12 件も同じである。

### 1.5 作らない・除外の提案（決めるのはオーナー）

次の提案は、この文書では提案として書くだけにし、台帳へは書かない。承認は `cases.excluded`／`cases.not_built` への `approved_by` と `approved_at` の記録で受ける（移植の規約）。

| 対象 | 提案 | 理由 |
|:---|:---|:---|
| ActorRefSpec ケース 3（Java Serialization で直列化できる） | 除外 | 確かめる対象が Java の `ObjectOutputStream`／`ObjectInputStream` と `JavaSerializer` そのものであり、Rust には対応する仕組みが無い。`ActorRef` の等価・パス・停止の観測といった、Rust で同じ振る舞いを作れる部分はケース 5（形を変えた形）と既存の `actor_ref_value_test.rs` が担う |
| ActorRefSpec ケース 4（系がスコープに無いデシリアライズで例外） | 除外 | ケース 3 と同じく、Java のデシリアライズの入口の振る舞いである。直列化そのものを除外すれば、このケースの前提も無くなる |
| ActorRefSpec ケース 5 のバイト列の部分 | 作らない（ケース自体は形を変えて移す） | `SerializedActorRef` のバイト列化は Java Serialization の一部である。パスの文字列表現からの解決で同じ性質（階層にいないパスは何も答えない参照になる）を確かめる |
| ActorRefSpec ケース 1 の直接生成の小ケース（`new Actor { … }` を actorOf の外で作ると例外） | 作らない | Rust では `Actor` を実装した値を作ること自体はただの値の構築であり、セルも自己参照も生まれない。作ることを禁じる仕組みは言語に無く、禁じなくても観測できる振る舞いは何も変わらない。ケースの残り（生成の中での失敗）は形を変えて移す |

HOCON の設定を写す部分は、ActorRefSpec の設定文字列（`allow-java-serialization = on`）だけであり、上の除外提案と一緒に落ちる。前例（[Issue #154](https://github.com/j5ik2o/fraktor-ng-rs/issues/154) の D1、G0b の決定 E）どおり、設定は型付きの入力にするという判断は、本件では「設定でしか選べない振る舞いをケースの前提にしない」形で効く。FSMTimingSpec に設定文字列は無い。

## 2. 対応する本体が fraktor-ng-rs にすでにあるか

2026-11-27 の main（`c0f94f0`、[PR #275](https://github.com/j5ik2o/fraktor-ng-rs/pull/275) のマージ直後）で確かめた。

### 2.1 FSM 側

| 原典の本体 | fraktor-ng-rs の現状 |
|:---|:---|
| `FSM` の状態遷移（`goto`、`stay`、`using`、`replying`、遷移の通知、購読） | ある。`crates/fraktor-actor/src/kernel/actor/fsm.rs` の `Fsm<S, D>`、`fsm_state.rs` の `FsmState<S, D>`、`fsm_view.rs` の `FsmView<S>`（[Issue #194](https://github.com/j5ik2o/fraktor-ng-rs/issues/194)） |
| FSM のタイマの表、世代、状態タイマのスロット | **test-support の下にだけある**。`Fsm` の `timers` と `state_timer`、`test_support/fsm_handle.rs` の `FsmHandle`（タイマの記録と状態タイマの記録、世代の数え）が [Issue #181](https://github.com/j5ik2o/fraktor-ng-rs/issues/181) で入った。ただしこれは `TestFSMRef` が外から駆動するためのもので、機械自身が状態関数の中からタイマを張る口は無い。名前付きタイマと状態タイマの世代を分ける判定は `Fsm::handle_control` の `TimerMark`／`StateTimeoutMark` にある（[PR #275](https://github.com/j5ik2o/fraktor-ng-rs/pull/275)） |
| 状態ごとのタイムアウト（`when(state, stateTimeout = …)`） | 無い。`Fsm::when` は状態関数だけを登録する。状態に入ったときに状態タイマを張る処理も、通常の機械（ハンドルなし）には無い |
| `forMax`（遷移ごとのタイムアウトの上書き、`Duration.Inf` を含む） | 無い。`FsmState` は `goto`／`stay`／`using`／`replying` だけを持つ |
| 状態関数の中からのタイマ操作（`startSingleTimer`、`startTimerWithFixedDelay`、`cancelTimer`、`isTimerActive`） | 無い。状態関数は `&FsmView`（読み取り専用。`listener_count` と `state_name` だけ）を受け取る |
| タイマの発火を運ぶ仕組み（通常の機械向け） | 無い。`TestFSMRef` には testkit のドライバのスレッド（`crates/fraktor-testkit/src/test_fsm_ref/test_fsm_ref_value.rs` の `drive_timers`。1ms ごとに記録を見て印を送る）があるが、これはハンドルを持つ機械専用である。kernel に Scheduler の port は無く（`kernel/clock.rs` の `Clock` だけがある）、`ActorContext` からスケジューラを取る口も無い |
| `StateTimeout` のイベント型 | ある。`crates/fraktor-actor/src/kernel/actor/state_timeout.rs` の `StateTimeout`（[Issue #181](https://github.com/j5ik2o/fraktor-ng-rs/issues/181)） |
| `whenUnhandled` | 一部ある。`Fsm::when_unhandled` で構築時に 1 つ登録できる。ただし (a) 処理できなかったイベントへの警告の発行が無い（現行は黙って現状態を保つ。#194 の設計が記録したとおり警告の文面も Pekko と違う）、(b) 実行中に差し替える口が無い（原典のケース 10 は状態関数の中で差し替え、空に戻す） |
| セルの suspend／resume、メールボックスの滞留の観測 | 無い（外向けには）。`kernel/dispatch/mailbox_status.rs` に suspend の数えはあるが、`ActorRef`／`ActorContext` から呼ぶ口と、`hasMessages` に当たる観測口は無い。#234 の段 6 の対象である（§1.4） |
| テストの土台（`within`、`expectMsg`、`expectNoMessage`、`ignoreMsg`、`receiveWhile`、`EventFilter`） | ある。`FraktorSpec::within`／`within_range`、`TestProbe::expect_msg`／`expect_no_message`／`ignore_msg`／`receive_while`、`EventFilter::warning`。台帳の needs のうち満たされていないものは無い |

### 2.2 ActorRef 側

| 原典の本体 | fraktor-ng-rs の現状 |
|:---|:---|
| `ActorRef`（等価、ハッシュ、パス、`tell`、`forward`） | ある。`crates/fraktor-actor/src/kernel/actor_ref/actor_ref_value.rs` の `ActorRef`。等価とハッシュは id と世代で決まる（所有権モデルの決定）。`path()`、`tell`、`forward` がある |
| 生成の入口（系の直下、親の下） | ある。`ActorSystem::actor_of`／`actor_of_named`／`try_actor_of`（[Issue #278](https://github.com/j5ik2o/fraktor-ng-rs/issues/278) で `actor_of_named` に一本化）、`ActorContext::actor_of`（名前つき）、`ActorContext::actor_of_with_strategy`（名前なし）、`WeakActorSystem::try_spawn_child`（[Issue #181](https://github.com/j5ik2o/fraktor-ng-rs/issues/181)） |
| 生成の失敗（初期化の失敗） | 一部ある。実行器に `ActorInitializationException` の原因名と `FailureKind::Initialization` があり、既定の監督は初期化の失敗を Stop にする。ただし factory の panic の扱いと、失敗した生成の名前の解放、失敗へのパスの同梱は、この設計では未確認として残す（§3.4、§7） |
| 入れ子の生成（`pre_start` での子の生成） | ある。`ActorContext::actor_of_with_strategy` と `actor_of` が `pre_start` の中から呼べる（ほかの Spec の移植で使用済み） |
| `PoisonPill`、`Kill` | ある。実行器が文字列のシステムメッセージとして扱い、`Kill` は `ActorKilledException` の失敗になる。`Actor::pre_restart`／`post_restart` のフックもある |
| 監督（`OneForOneStrategy`） | ある。`crates/fraktor-actor/src/kernel/supervision/supervisor_strategy.rs`。`Actor::supervisor_strategy` で親が上書きする |
| `ask` | ある。adaptor の `AskFuture`（`ActorRef::ask`／`ask_with_timeout`） |
| 子の存在確認（`context.child(name)`） | **無い**。実行器は親ごとに子の名前と参照の表（`ParentEntry.children`）を持つが、`ActorContext` から引く口が無い |
| 階層にいないパスの解決（`EmptyLocalActorRef` 相当） | ある。`ActorSystem::resolve_actor_ref` が、生きていないパスには「パスを保ち、何も答えない」参照を返すと文書に書いている |
| 直列化（Java Serialization、`SerializedActorRef`） | 無い。作らない（§1.5 の提案） |
| テストの土台（`DefaultTimeout`、`expectTerminated`、`TestLatch`、`EventFilter`） | ある。`TestProbe::expect_msg_terminated` 系、`fraktor-testkit` の `TestLatch`（[Issue #180](https://github.com/j5ik2o/fraktor-ng-rs/issues/180)）、`EventFilter` |

### 2.3 対応表（scala-java-to-rust-correspondence.md）を引いた結果

- `Duration.Inf` の代数を持つ汎用型は作らない。「上限が無い」は使う場所ごとの型で表し、FSM と ReceiveTimeout では `Option` を比較に使わず専用の enum にする（`docs/design/issue-185-util-specs.md:123` の行）。§3.1 の `FsmTimeout` はこの行に従う。
- `classOf[…]` とリフレクションでコンストラクタを探す組み立ては、引数を捕まえたクロージャを `Props::new` へ渡して写す（`docs/design/issue-193-specs.md:46` の行）。ActorRefSpec のケース 9 はこの行に従い、factory 関数経由の形にする。
- HOCON の `Config` で設定を受ける部分は型付きの入力にする（`docs/goals/issue-154-id-generator-and-dns-settings.md:26` の行ほか）。本件の設定文字列は直列化の許可だけであり、§1.5 のとおりケースごと提案に回す。
- 伺う基準：Pekko どおりに写せるものは伺わずに写し、伺うのは Rust で同じ振る舞いを作れず、数から外す・弱めるときだけである。§1.5 の 4 件だけがこの基準に当たる。

## 3. 新しく足すものと、公開 API の形

新しい型は、syn チェッカーの型ごとに 1 ファイルの決まりに従い、1 型 1 ファイルで置く。シグネチャは設計の形であり、名前の細部は実装のときに既存の命名へ合わせる。追加はすべて `crates/fraktor-actor` の kernel と、`crates/fraktor-actor/tests/` の 2 ファイルであり、testkit への追加は無い（ケース 8 を除くすべての土台がすでにある）。

### 3.1 FSM のタイマと状態タイムアウト（通常の機械へ広げる）

方針は、[Issue #181](https://github.com/j5ik2o/fraktor-ng-rs/issues/181) で入った仕組みを重ねて作らず、通常の機械へ広げることである。タイマの表、世代の数え、状態タイマのスロット、印の判定（`TimerMark`／`StateTimeoutMark`）は、現行では `#[cfg(feature = "test-support")]` の下にある。これを feature の外へ出し、`Fsm` が常時持つ形にする。`FsmHandle` はテスト側の読み取り口として残し、表と判定は機械本体が持つ。そうすると `TestFSMRef` のケース（testkit の TestFSMRefSpec、green 済み）は同じ判定を通り続け、FSMTimingSpec の機械は同じ表を使う。

状態関数からタイマを操作する口は、`FsmView` に足す。状態関数のシグネチャ（`&mut ActorContext`、`&FsmView`、`&Message`、`&D`）は [Issue #194](https://github.com/j5ik2o/fraktor-ng-rs/issues/194) と [Issue #181](https://github.com/j5ik2o/fraktor-ng-rs/issues/181) のテストが使う形であり、変えると既存の移植がすべて書き換えになる。`FsmView` は今は値のスナップショットだが、機械のタイマの共有状態（現行の `FsmHandle` と同じ `Arc<spin::Mutex<…>>` の形）への参照を持たせ、コマンドを記録する面を足す。コマンドは状態関数の実行が終わった後、遷移の適用と並んで機械が取り込む（§4 の I1、I3 の順序はこの取り込み位置で決まる）。

```rust
// crates/fraktor-actor/src/kernel/actor/fsm_timeout.rs
/// 状態タイムアウトの長さ。無限は専用の値で表す（対応表の Duration.Inf の行）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FsmTimeout {
  After(Duration),
  Infinite,
}

// crates/fraktor-actor/src/kernel/actor/fsm.rs への追加
impl<S, D> Fsm<S, D> {
  /// 状態タイムアウトつきの状態関数を登録する。原典の `when(state, stateTimeout = …)`。
  /// `None` はタイムアウトなし（`when` と同じ）。
  pub fn when_with_state_timeout<F>(&mut self, state: S, timeout: Option<FsmTimeout>, function: F)
  where F: FnMut(&mut ActorContext<'_>, &FsmView<S>, &Message, &D) -> Option<FsmState<S, D>> + Send + 'static;
}

// crates/fraktor-actor/src/kernel/actor/fsm_state.rs への追加
impl<S, D> FsmState<S, D> {
  /// この遷移に限って状態タイムアウトを上書きする。原典の `State.forMax(timeout)`。
  pub fn for_max(self, timeout: FsmTimeout) -> Self;
}

// crates/fraktor-actor/src/kernel/actor/fsm_view.rs への追加（コマンドの面）
impl<S> FsmView<S> {
  /// 原典の `startSingleTimer`。同じ名前で張ると前のタイマを置き換える。
  pub fn start_single_timer(&self, name: &str, message: Message, delay: Duration);
  /// 原典の `startTimerWithFixedDelay`。
  pub fn start_timer_with_fixed_delay(&self, name: &str, message: Message, delay: Duration);
  /// 原典の `startTimerAtFixedRate`。本 Spec は使わないが、原典の FSM の面として揃える。
  pub fn start_timer_at_fixed_rate(&self, name: &str, message: Message, interval: Duration);
  /// 原典の `cancelTimer`。無い名前の取り消しは何もしない。
  pub fn cancel_timer(&self, name: &str);
  /// 原典の `isTimerActive`。このイベントの開始時点の表で答える。
  pub fn is_timer_active(&self, name: &str) -> bool;
}
```

発火の運搬は、レビューで決める 2 案を置く。どちらの案でも、印の判定（世代が合うときだけ受け付ける）は機械の中にあり、テストの書き方は変わらない。

- **案 A（推奨）：kernel に Scheduler の port を足し、std の adaptor が実時間で運ぶ。** `Clock`（`kernel/clock.rs`）と同じ流儀で、期限の登録と取り消しを受ける port を kernel に置き、`ActorSystem` の生成時に adaptor の実装を差す。機械は `pre_start` で自分のタイマの共有状態を port へ登録し、期限が来たら adaptor が印（現行の `FsmControl::TimerMark`／`StateTimeoutMark` と同じ形）を対象の `ActorRef` へ送る。FSMTimingSpec の対象は通常の機械の振る舞いであり、testkit 専用のドライバでは「製品の FSM が時間を守る」ことを確かめたことにならないため、こちらを推奨する。代償は、`ActorSystem` の生成の形と adaptor への追加がこの issue の範囲に入ることである。
- **案 B：#181 のドライバを testkit の汎用の仕組みへ広げる。** `fraktor-testkit` に、ハンドルつきで機械を起動し `drive_timers` と同じスレッドで運ぶ入口（`TestFSMRef` の汎用化）を足し、FSMTimingSpec の機械をその入口で起動する。範囲は小さいが、確かめる対象が「testkit にぶら下げた機械」になり、通常の `actor_of` で作った機械のタイマは無いまま残る。FSM のタイマを製品の機能として持つかどうか自体が論点になる場合は、こちらが暫定形になる。

どちらの案でも、期限を過ぎた印が遅れて届くこと、ドライバの起床の粒度（現行の testkit ドライバは 1ms）は、意味論ではなく運搬の性質として扱う。テストの時間の幅は `dilated` で取る（§1.2）。

no_std の観点：表、世代、`FsmTimeout`、コマンドの記録は `core` と `alloc`（`Vec`、`String`、`Duration` は `core::time`）だけで書ける。実時間の待ちとスレッドは案 A では adaptor（std）に閉じ、案 B では testkit（std）に閉じる。kernel に `std` を持ち込まない。`FsmView` のコマンドの面は、ハンドルなしの機械でもタイマの共有状態を持つため、`Fsm::new` の時点で小さな共有状態を 1 つ確保する。これは既存の利用者へのコストとして §8 に書く。

### 3.2 未処理イベントの警告と、`whenUnhandled` の差し替え

- 警告：状態関数も `whenUnhandled` も答えなかったイベントについて、機械は Pekko の文面「unhandled event {event} in state {state}」の警告を、自分のパスを source として発行する。発行の経路は、実行器が `UnhandledMessage` を event stream へ出す既存の仕組みとは分ける。原典の警告は FSM 自身のログであり、`EventFilter.warning(source = fsm.path)` が見る対象である。イベントの表示は、ケース 10 が使う 2 件（`Tick` と `Unhandled("test")`）について、型名と中身が Pekko の文字列表現と対応する形をテスト側の型の `Debug` で決める。文面の細部はレビューで見てほしい点に挙げる（§9）。
- 差し替え：`FsmView` のコマンドの面に、未処理ハンドラの差し替えを足す。

```rust
impl<S> FsmView<S> {
  /// 原典の、状態関数の中からの `whenUnhandled { … }` の差し替え。
  pub fn set_unhandled_handler<F>(&self, function: F)
  where F: FnMut(&mut ActorContext<'_>, &FsmView<S>, &Message, &D) -> Option<FsmState<S, D>> + Send + 'static;
  /// 原典の `whenUnhandled(NullFunction)`。既定の（警告を出す）扱いに戻す。
  pub fn clear_unhandled_handler(&self);
}
```

ただし `FsmView<S>` はデータ型 `D` を持たないため、この形のままでは書けない。実装では、コマンドの記録先に型消去したハンドラ（現行の `StateFunction<S, D>` と同じ `Box<dyn FnMut …>`）を持たせるため、`FsmView` に `D` の型引数を足すか、差し替え専用の面を状態関数の第 2 引数とは別に用意するかのどちらかが要る。どちらの形でも状態関数のシグネチャが変わる場合は、[Issue #194](https://github.com/j5ik2o/fraktor-ng-rs/issues/194) と [Issue #181](https://github.com/j5ik2o/fraktor-ng-rs/issues/181) の既存テストの書き換え範囲を第2段の前に数えて報告する。これもレビューで見てほしい点に挙げる（§9）。

### 3.3 `ActorContext::child`

```rust
// crates/fraktor-actor/src/kernel/actor/actor_context.rs への追加
impl ActorContext<'_> {
  /// 原典の `ActorContext.child(name)`。自分の生きている子がいれば、その参照を返す。
  pub fn child(&self, name: &str) -> Option<ActorRef>;
}
```

実行器の親ごとの子の表（`ParentEntry.children`）を引くだけであり、新しい状態は足さない。止まった子は表から外れているので `None` になる（原典と同じ）。kernel の中だけで閉じ、`core` と `alloc` で書ける。

### 3.4 生成の失敗へのパスの同梱

ケース 2 が要るのは、初期化の失敗の表出に失敗したアクターのパスが入ることである。第2段の最初に、現行の実行器が factory／`pre_start` の失敗をどう記録するか（原因名、文面、パスの有無）を実測する。パスが入っていない場合は、失敗の記録へ対象のパスを足す追加をこの issue の範囲に入れる。公開 API の追加は想定しない（失敗の記録は event stream のログ側の話である）。

### 3.5 その他の追加

無い。ActorRefSpec の残りのケース（入れ子、`forward`、`ask`、`PoisonPill`、`Kill`、監督、解決）は §2.2 のとおり既存の本体で足りる。ケース 5 の解決は `ActorSystem::resolve_actor_ref` をそのまま使う。

## 4. 守る性質（不変条件）

| 番号 | 不変条件 |
|:---|:---|
| I1 | 状態タイマの張り直し：普通のメッセージを 1 件受けるたびに、張ってある状態タイマは取り消され（世代が進む）、処理の結果の状態について、その状態のタイムアウトか、この遷移の `forMax` の上書きのどちらか一方が張られる。`forMax(FsmTimeout::Infinite)` と、タイムアウトの無い状態では張られない。`goto` で同じ状態へ移る場合も入り直しとして張り直す。購読・購読解除・リスナーの終了の受信は原典どおり例外であり、取り消さない（現行の `Fsm::receive` の分岐と同じ） |
| I2 | 世代の分離：名前付きタイマの世代と状態タイマの世代は別々に進む。名前付きタイマを張っても状態タイマの印は無効にならず、状態タイマの取り消しは名前付きタイマの印を無効にしない（[PR #275](https://github.com/j5ik2o/fraktor-ng-rs/pull/275) の決定） |
| I3 | 名前付きタイマ：同じ名前で張ると置き換わり、古い印は捨てられる。単一タイマは高々 1 回だけ発火し、発火で表から消える。繰り返し（fixed delay）は取り消されるまで続き、取り消しの後に印が届いても捨てられる。無い名前の取り消しは何もしない |
| I4 | 停止：アクターが止まると、名前付きタイマと状態タイマはすべて消え、その後に期限が来ても何も届かず、DeadLetter にもならない（ケース 3） |
| I5 | 未処理の警告：状態関数も未処理ハンドラも答えなかったイベントは、警告が 1 回だけ発行され、状態とデータは変わらない。差し替えたハンドラは次のイベントから効き、空に戻すと警告の扱いに戻る |
| I6 | 子の参照：`child(name)` が `Some` を返すのは、その名前の生きている子が自分の下にいるときだけであり、返る参照は生成時に渡した参照と等しい |
| I7 | 生成の失敗の後始末：factory の中で失敗した生成は参照を渡さず、失敗した名前は解放され（同じ名前で作り直せる）、系はその後も通常どおり生成できる（原典の「context のスタックが空」の写し）。ケース 1 と 2 で確かめる |
| I8 | 等価の不変：この issue は `ActorRef` の等価（id と世代）と比較の規則を変えない。ケース 5 の形の変更（§1.3）はこの不変条件の帰結である |

## 5. PBT を足すか

足す。原典外であり、台帳の件数には含めない。外部の依存は足さず、既存の移植と同じ固定 seed の SplitMix64 で書く（`proptest` の採否は [Issue #212](https://github.com/j5ik2o/fraktor-ng-rs/issues/212) で決める話であり、この件では触れない）。置き場所は `crates/fraktor-actor/tests/fsm_timing_property_test.rs` と `crates/fraktor-actor/tests/actor_ref_property_test.rs` を想定する。

| 番号 | 性質 | 確かめる不変条件 |
|:---|:---|:---|
| P1 | 名前付きタイマの表のモデル照合：張る・取り消す・張り直す・印を受けるのランダムな列について、イベントとして処理されるのは「表にあり、世代が合い、単一ならまだ発火していない」印だけであり、処理されるメッセージは常に対象の行が持つメッセージである | I2、I3 |
| P2 | 状態タイムアウトの選択：状態のタイムアウト（なし／`After`／`Infinite` 相当の入り直し）と `forMax` の上書き（なし／`After`／`Infinite`）と遷移の種類（`goto`／`stay`／同じ状態への `goto`）の組み合わせについて、処理の後に張られるのは常に I1 が選ぶ 1 つだけであり、古い印は決して通らない | I1、I2 |
| P3 | 生成の失敗の後始末：factory が失敗する生成と成功する生成のランダムな列について、失敗した名前は常にもう一度使え、成功した参照はすべて相異なり、失敗は成功した子の集合を変えない | I6、I7 |

## 6. 状態遷移と Quint を書くかどうか

**書く。** ただし、タイマの世代の判定そのものは `crates/fraktor-actor/specs/test_fsm_ref.qnt`（[Issue #181](https://github.com/j5ik2o/fraktor-ng-rs/issues/181)）がすでにモデルにしている（印の発行と受信を分け、間に取り消し・張り直し・再起動を挟む形）。重ねて書かない。この issue で新しいのは、状態タイムアウトの**選び方**（状態自身の値、`forMax` の上書き、無限、入り直し）であり、固定のテストでは組み合わせを取りこぼしうる（原典のケースが通るのは、状態の値だけ、上書きだけの 2 組み合わせである）。

仕様は `crates/fraktor-actor/specs/fsm_timing.qnt`（本体）と、負の対照を含むテスト側を想定する。

- 状態：現在の状態名、張ってある状態タイマの種類（なし／期限あり／無限で張らない）とその世代、最後に処理した遷移の種類。
- 遷移：`enter`（入り直しを含む）、`receiveOrdinary`（取り消してから処理し、結果で張り直す）、`stateTimerDue`（印の発行）、`receiveStateMark`（世代で判定する受信）。
- 不変条件：`armedMatchesSelection`（張ってあるのは I1 の選択と常に一致する）、`staleMarkNeverFires`（世代の合わない印は状態を変えない）、`infiniteNeverArms`（`Infinite` の上書きと、タイムアウトの無い状態では決して張られない）。
- 負の対照：`bugForMaxIgnored`（上書きを無視する）、`bugStayKeepsOldTimer`（`stay` で古いタイマを残す）、`bugSameStateGotoDoesNotRearm`（同じ状態への `goto` で張り直さない）。それぞれ不変条件が落ちることを確かめる。
- モデルにしないもの：実時間そのもの（期限が来たかどうかは非決定的に選ぶ。`test_fsm_ref.qnt` と同じ）、名前付きタイマの世代（既存の仕様が担う）、警告の文面。

## 7. 壊しうる入力

| 入力 | 何が起きうるか | 確かめているもの |
|:---|:---|:---|
| タイマの名前が空文字 | 原典は名前を検証せずキーとして使う。移植も空文字を許し、ほかの名前と区別する | P1（名前の抽選に空文字を含める） |
| タイマの名前の連続した張り直し（同じ名前を何度も） | 世代が進み続け、古い印が新しい行に通る事故が起きうる | P1、ケース 6、ケース 7 |
| ワイルドカード的な名前（`*` など） | 移植の表は文字列の完全一致だけであり、パターンの意味は持たない。原典も同じ | P1（名前は不透明な文字列として扱う） |
| 極端に長い名前、非 ASCII の名前 | 表のキーは `String` であり、長さと符号化に制限を足さない | P1 |
| 期限が `Duration::ZERO` | 張った直後に期限が来る。処理の順序（コマンドの取り込み位置）で、そのイベント自身の処理中に発火しないことを決める | §3.1 の取り込み位置、単体テストを第2段で足す |
| 期限が非常に大きい（`Instant` に足せない） | 運搬側で溢れる。現行の testkit ドライバは `checked_add` で「決して発火しない」に倒す。案 A でも同じ扱いにする | 範囲の外としてここに記録する |
| 境界の時間（期限ちょうどにメッセージが届く） | 実時間の競合であり、勝敗は実行ごとに変わりうる。テストは `dilated` の幅で避け、意味論としては「受信が先なら取り消し、印が先なら発火」のどちらも正しい | ケース 1、2 の時間の幅。Quint は時間を持たないので対象外 |
| 状態タイムアウトが `Duration::ZERO` の状態 | 入るたびに即座に `StateTimeout` になりうる。無限ループにならないこと（1 回の発火は 1 件のメッセージ処理である） | P2 |
| `child` へ渡す名前が空文字、存在しない名前、止まった子の名前 | すべて `None`。原典も `None`（`Option` が空）である | ケース 12、P3 |
| factory の中で panic する生成、panic のペイロードが文字列でない生成 | 初期化の失敗の表出と、名前の解放が崩れうる | ケース 1、2、P3。第2段の最初に現行の扱いを実測する |
| 再起動をまたぐタイマ | 通常の機械では、再起動で新しい個体が作られ、タイマの表は新しい個体のものになる（`TestFSMRef` のハンドルが再起動をまたぐのは testkit の設計であり、混同しない）。状態タイマは `initialize` の入り直しで張り直される | 範囲の外としてここに記録する。本 Spec に再起動のケースは無い |

## 8. 影響

- **公開 API**：追加は `FsmTimeout`、`Fsm::when_with_state_timeout`、`FsmState::for_max`、`FsmView` のタイマと未処理ハンドラの面、`ActorContext::child` である。既存の型と関数のシグネチャは変えない方針だが、§3.2 の差し替えの面だけは形しだいで `FsmView` の型引数が変わりうる（§9）。
- **test-support の形**：タイマの表と判定を feature の外へ出すと、`fraktor-testkit` の `TestFSMRef` と green 済みの TestFSMRefSpec が同じ判定を通る。判定の意味は変えない（I2 は [PR #275](https://github.com/j5ik2o/fraktor-ng-rs/pull/275) の決定のまま）。
- **既存の利用者へのコスト**：`Fsm::new` がタイマの共有状態を 1 つ確保するようになる（案 A／B のどちらでも）。ハンドルなしの機械でも払う小さな確保であり、ほかのコスト（ロックはイベント処理の前後だけ）は §3.1 の取り込み位置で閉じる。
- **台帳**：第2段で 2 ノードを更新する。FSMTimingSpec は §1.4 の形（total 10、ported 9、passing 9、`verified_against` なし）、ActorRefSpec は §1.5 の提案が承認された場合に total 12、ported 10、passing 10、excluded 2（ケース 3、4）として green にする。提案が承認されない場合は、その判断を受けてから台帳の形を決める。
- **保存済みのデータ・上流**：無し。

## 9. レビューで見てほしい点

- **発火の運搬の案 A／案 B（§3.1）**：推奨は案 A（kernel の Scheduler の port と adaptor の実時間の運搬）である。案 A は `ActorSystem` の生成と adaptor に触れるため、この issue の範囲を広げる。その判断がレビューの中心である。
- **未処理ハンドラの差し替えの形（§3.2）**：`FsmView` へ型引数を足す案と、差し替え専用の面を分ける案とで、既存の移植テストの書き換え範囲が変わる。あわせて、警告の文面（「unhandled event {event} in state {state}」）とイベントの表示の対応づけを妥当と見るか。
- **作らない・除外の提案 4 件（§1.5）**：決めるのはオーナーである。特にケース 5 を「形を変えて移す」とし、等価のアサーションを落とす扱い（I8 の帰結）を妥当と見るか。
- **ケース 8 を #234 の段 6 の後に分ける扱い（§1.4）**：台帳を total 10、ported 9 のまま残す形が、[Issue #194](https://github.com/j5ik2o/fraktor-ng-rs/issues/194) の前例と揃うか。
- **ケース 1 の形（§1.3）**：factory の panic の扱いを第2段の最初に実測してからアサーションを決める進め方と、直接生成の小ケースを作らない提案。
- **ケース 7 の形の変更（§1.2）**：メールボックスの滞留を処理の中で待つ部分を落とし、捨てられる印の性質は PBT と Quint が担うとする割り切りを妥当と見るか。
