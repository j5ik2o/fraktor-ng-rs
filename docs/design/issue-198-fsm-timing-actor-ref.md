# 設計：#198 FSMTimingSpec／ActorRefSpec の移植

[Issue #198](https://github.com/j5ik2o/fraktor-ng-rs/issues/198)（[PR #288](https://github.com/j5ik2o/fraktor-ng-rs/pull/288)）の第 1 段（設計と設計に付く Quint 仕様のみ）の記録である。実装とテストは、この設計が独立レビューを通ってから第 2 段で行う。第 2 段では、テストを Claude Opus 5.5 の担当が先に red で書き、実装の担当はそのテストを書き換えずに通す実装だけを書く。この文書と `crates/fraktor-actor/specs/fsm_timing.qnt` の段では実装・テスト・台帳を触らない。

対象は Pekko v2.0.0（commit `9fbd2283b1cf701c90a1223bf621942c191db61f`）の actor-tests にある `FSMTimingSpec` と `ActorRefSpec` である。原典の本体は、actor の `FSM.scala`（名前付きタイマ、状態タイムアウト、`onTransition` からのタイマ操作、`stay()`・未処理イベントでの状態タイムアウト張り直し、`whenUnhandled` の動的差し替え）と `ActorRef.scala`／`Actor.scala`／`ActorCell.scala`／`LocalActorRefProvider`（非同期生成と `ActorInitializationException` のパス同梱、入れ子の生成、`context.child`、`EmptyLocalActorRef`、直列化）である。移植先は `crates/fraktor-actor`（および不足している `EventFilter` 拡張について `crates/fraktor-testkit`）である。依存の [Issue #180](https://github.com/j5ik2o/fraktor-ng-rs/issues/180)（`receiveWhile`、`TestLatch`）および [Issue #200](https://github.com/j5ik2o/fraktor-ng-rs/issues/200)（[PR #283](https://github.com/j5ik2o/fraktor-ng-rs/pull/283)、`Executor` の期限索引と `await_envelope` の期限カット）は `main`（`53014ed`）に入っている。

進め方の規約は `.takt/facets/knowledge/fraktor-porting-rules.md`、設計文書の前例は `docs/design/issue-181-testfsmref.md`（[PR #275](https://github.com/j5ik2o/fraktor-ng-rs/pull/275)）、`docs/design/issue-200-receive-timeout.md`（[PR #283](https://github.com/j5ik2o/fraktor-ng-rs/pull/283)）、`docs/design/issue-234-multithread-executor.md`（段 6：[Issue #257](https://github.com/j5ik2o/fraktor-ng-rs/issues/257)）に従う。方針は `docs/decisions/pekko-semantics-first.md`（速さより Pekko の意味論を優先する）と `docs/decisions/scala-java-to-rust-correspondence.md`（§2.3）に従う。新しい Rust・Quint ファイルには Apache-2.0 のヘッダーを付ける。

## 1. 何を移植するか

### 1.1 件数の数え直し

原典を `" in {"` の数で数え直した。`ignore` と `pending` はどちらにも無い。

| 原典 | 目安（issue／台帳） | 数え直し |
|:---|---:|---:|
| `actor-tests/src/test/scala/org/apache/pekko/actor/FSMTimingSpec.scala` | 10／0 | **10** |
| `actor-tests/src/test/scala/org/apache/pekko/actor/ActorRefSpec.scala` | 12／12 | **12** |
| 合計 | 22／12 | **22** |

台帳の対象ノードは次の 2 件で、どちらも `status: unported`、`triage.kind: pending` である。

- `actor-tests/src/test/scala/org/apache/pekko/actor/FSMTimingSpec.scala`（全 10 件に `taggedAs TimingTest`。needs: `extends-pekko-spec`, `pekko-spec`, `test-kit`, `test-actor`, `implicit-sender`, `expect-msg`, `expect-no-message`, `event-filter`, `within`, `await-cond`, `receive-while`, `ignore-msg`。premise_flags: `thread-sleep`）
- `actor-tests/src/test/scala/org/apache/pekko/actor/ActorRefSpec.scala`（needs: `extends-pekko-spec`, `pekko-spec`, `test-actor`, `default-timeout`, `expect-msg`, `expect-terminated`, `event-filter`, `test-latch`。premise_flags: `java-serialization`, `thread-sleep`）

移す先は `crates/fraktor-actor/tests/` にノード 1 件＝ファイル 1 件で置き、冒頭に Pekko v2.0.0 の原典パスをコメントで残す。

- `crates/fraktor-actor/tests/fsm_timing_spec.rs`
- `crates/fraktor-actor/tests/actor_ref_spec.rs`

### 1.2 対応表：FSMTimingSpec（10 件）

原典は 1 つの `StateMachine` インスタンスを 10 ケースで順次使い回すが、Rust の `#[test]` は並列実行されるため、**ケースごとに独立した `FraktorSpec` と `StateMachine` を起動する**構成にする（台帳の件数は原典の 10 件のまま変えない。テストの本数が増えるだけである）。共通ヘルパ `start_fsm(spec)` が次を毎回行う：

1. `spec.test_kit_settings()` を `Props` の factory へ渡し、`StateMachine`（`Fsm<State, i32>`）を起動する。**`initialize()` は呼ばない。** 原典の `StateMachine`（`FSMTimingSpec.scala:198-290`）も `StoppingActor`（`:292-300`）も `initialize()` を呼ばず、最初のイベントを処理する `make_transition` で初めて状態タイマを張る（§3.1(a) の「`initialize()`（`pre_start`）」）。`startWith(Initial, 0)` にタイムアウトが無いので、呼んでも張られるものは無い。どちらにせよ原典に無い呼び出しは足さない。
2. `SubscribeTransitionCallBack(spec.test_actor())` を送り、`expect_msg_within(Duration::from_secs(1), ...)` 相当で `CurrentState { fsm, state: State::Initial }` を受ける（`FSMTimingSpec.scala:26-27`）。
3. `spec.test_kit_mut().ignore_msg(|msg| matches!(msg.downcast_ref::<Transition<State>>(), Some(t) if t.from == State::Initial))` を登録する（`FSMTimingSpec.scala:29-31`）。`FraktorSpec` 自身には `ignore_msg`／`expect_no_message` の直接メソッドが無いため、いずれも `spec.test_kit_mut()` 経由で呼ぶ。`TestKit::expect_no_message(max)` は原典の `expectNoMessage(max)` と同じく `max` を dilate しない。原典の 2 つの形は別の長さを待つので、写し方を分ける：
   - `expectNoMessage(remainingOrDefault)`（ケース 1・2・3）：`within` の中なので `remainingOrDefault` は `remaining` と同じ。`spec.test_kit_mut().expect_no_message(spec.remaining())` と書く。
   - 引数なしの `expectNoMessage()`（ケース 4・5・6）：原典 `TestKit.scala:733-734` は `ExpectNoMessageDefaultTimeout.dilated`（`pekko.test.expect-no-message-default = 100ms`、`testkit/src/main/resources/reference.conf:26`）を待つ。scope の残りは待たない。`let settings = spec.test_kit_settings(); spec.test_kit_mut().expect_no_message(settings.dilated(settings.expect_no_message_default_timeout()))` と書く（`TestProbe::expect_no_message` の doc が示す形。testkit に引数なしの口は足さない）。

時間の扱い（`dilated` と CI 揺れ対策）：

- 原典 `FSMTimingSpec.scala` に合わせ、`StateMachine` の遅延（`800ms`, `500ms`, `100ms`, `1ms`：`FSMTimingSpec.scala:207, 210, 216, 226, 231, 239, 243, 266, 268`）は `TestKitSettings::dilated` で伸ばす。一方 `StoppingActor` の `when(Initial, 200.millis)`（`FSMTimingSpec.scala:295`）は原典どおり dilate しない。
- `FraktorSpec::within_range(min, max)` は `max` のみ dilate し `min` は伸ばさない（原典 `TestKitBase.within(min, max)` と同一）。
- 発火の運搬は §3.1 の案 C（`#200` 型の `Executor` 期限索引＋`Clock`＋`await_envelope` の期限カット）を採る。1ms ポーリングスレッドとスレッド間起床遅延が無いので、実時間テストの CI 揺れを最小限に抑えられる。仮想時間（`ManualScheduler`）への置き換えは行わない（実時間の発火確認は Spec が担い、決定論的な境界・順序の網羅は §5 の PBT が `ActorSystem::set_clock` で担う）。

| # | 原典のケース | Rust のテスト名 | 移す／保留と具体的な写し方 |
|:---:|:---|:---|:---|
| 1 | receive StateTimeout | `receives_a_state_timeout` | 移す（§3.1）。`TestStateTimeout`（`when_with_state_timeout` で 800ms dilated）へ移り、`StateTimeout` で `Initial` へ戻る。`within(2s)` の内側で `within_range(500ms, 2s)` に `Transition(fsm, TestStateTimeout, Initial)` を受け、その後 `expect_no_message(spec.remaining())` |
| 2 | cancel a StateTimeout | `cancels_a_state_timeout` | 移す（§3.1）。`TestStateTimeout` へ移った直後に `Cancel` を送り、`within(1s)` 内で `Cancel` と `Transition(fsm, TestStateTimeout, Initial)` を受け、`expect_no_message(spec.remaining())`。通常メッセージ受信で状態タイマが取り消されること（I1）を見る |
| 3 | cancel a StateTimeout when actor is stopped | `cancels_a_state_timeout_when_the_actor_is_stopped` | 移す（§3.1）。`StoppingActor` は `initialize()` を呼ばず、`when_with_state_timeout(Initial, Some(200ms), ...)` で `context.stop(&context.self_ref()); Some(FsmState::stay())` を返す。`make_transition(stay())` が 200ms の状態タイマを張り、直後の `post_stop_with_context`（および停止処理の `clear_fsm_timers`）がそれを取り消す原典の構造（`FSMTimingSpec.scala:292-300`）をそのまま写す。`test_actor` が `DeadLetter` を購読し、`within(400ms)` で `expect_no_message(spec.remaining())`（I4） |
| 4 | allow StateTimeout override | `allows_a_state_timeout_override` | 移す（§3.1）。`Initial` から `FsmState::goto(TestStateTimeout).for_max(FsmTimeout::Infinite)` で移ると、状態自身の 800ms は張られず、`within(400ms)` の中で既定の長さ（100ms dilated）の `expect_no_message` を通る。続く `within(1s)` で `Cancel` を送り `Cancel` と `Transition(fsm, TestStateTimeout, Initial)` を受ける |
| 5 | receive single-shot timer | `receives_a_single_shot_timer` | 移す（§3.1）。状態関数内で `view.start_single_timer("tester", Message::new(Tick), dilated(500ms))` を張り、`within(2s)` 内側の `within_range(500ms, 1s)` で `Tick` と `Transition(fsm, TestSingleTimer, Initial)` を受け、既定の長さ（100ms dilated）の `expect_no_message` |
| 6 | resubmit single-shot timer | `resubmits_a_single_shot_timer` | 移す（§3.1）。`on_transition_with_view` の `Initial -> TestSingleTimerResubmit` で `view.start_single_timer("blah", Message::new(Tick), dilated(500ms))` を張り、発火時に同じ名前 `"blah"` で `Tock` を張り直す。`within(2.5s)` の内側で `within_range(500ms, 1s)` に `Tick`、続く `within(1s)` に `Tock` と `Transition(fsm, TestSingleTimerResubmit, Initial)`、最後に既定の長さ（100ms dilated）の `expect_no_message` |
| 7 | correctly cancel a named timer | —（#234 段 6 #257 の後。§1.4） | この issue では移さない（#257 後の後続 issue へ回す）。原典（`FSMTimingSpec.scala:237-251`）は `Tick` 処理の中で 1ms タイマ `"hallo"` を張り、`TestKit.awaitCond(context.asInstanceOf[ActorCell].mailbox.hasMessages, 1.second.dilated)`（`:240`）で印がメールボックスに積まれるのを待ってから取り消す。**この待ちが観察の中心なので、待ちを落として移すと、確かめる中身が別物になる。** アクターが自分のメールボックスを読む口（`mailbox.hasMessages`）が無いので今は写せない。ケース 8 と同じく、除外・作らない区分にはせず台帳は `total: 10, ported: 8, passing: 8, status: unported`（`verified_against` なし）のまま残す。代わりの確かめ（§5 P1 と §6 の Quint `staleNamedNeverFires`）は、正しさを補うための裏付けであって、台帳の件数には数えない（決定：`pr288-decisions.md` の 5・6） |
| 8 | not get confused between named and state timers | —（#234 段 6 #257 の後。§1.4） | この issue では移さない（#257 後の後続 issue へ回す）。外からの `suspend`／`resume` と `mailbox.hasMessages` が必要。ケース 7 と同じ扱いにする。除外・作らない区分にはせず、台帳は `total: 10, ported: 8, passing: 8, status: unported`（`verified_against` なし）のまま残す |
| 9 | receive and cancel a repeated timer | `receives_and_cancels_a_repeated_timer` | 移す（§3.1）。`view.start_timer_with_fixed_delay("tester", Message::new(Tick), dilated(100ms))` を張り、データ 4 から数えて 5 回 `Tick` を `FraktorSpec::receive_while`（2s）で受け（長さ 5）、0 で `cancel_timer("tester")` して `Initial` へ戻り、`within(500ms)` で `Transition(fsm, TestRepeatedTimer, Initial)` を受ける |
| 10 | notify unhandled messages | `notifies_unhandled_messages` | 移す（§3.2、§3.5）。`FraktorSpec::filter_events`（§3.5 の (a)。ブロックの中で `spec.within` と `spec.test_kit_mut().expect_msg` を呼ぶ必要があるため、`EventFilter` の関連関数ではなく `FraktorSpec` の口にする）に 2 本の `EventFilter::warning()`（`"unhandled event Tick in state TestUnhandled"` と `"unhandled event Unhandled(test) in state TestUnhandled"`、どちらも `with_source(fsm.path().to_string())`、`with_occurrences(1)`）を同時に張り、`TestUnhandled`（`State` として送り `TestUnhandled` へ移る。`FSMTimingSpec.scala:149`）→ `within(3s) { Message::with_display(Tick)`（`TestUnhandled` の状態関数が未処理にする）→ `SetHandler`（`FsmState::stay().with_unhandled(...)`）→ `Message::with_display(Tick)`（`Unhandled(Tick)` を受信）→ `Message::with_display(Unhandled("test"))` → `Cancel`（`FsmState::goto(Initial).clear_unhandled()`）→ `expect_msg(Transition(fsm, TestUnhandled, Initial))` }` を実行する。原典の `fsm ! TestUnhandled` は `within` の外（`:149`）、`within(3.second)` の内側は `Tick` から遷移の受信まで（`:150-158`）なので、その順に写す |

### 1.3 対応表：ActorRefSpec（12 件）

原典のアクター群（`ReplyActor`, `WorkerActor`, `SenderActor`, `OuterActor`, `InnerActor` および失敗変種）は通常の `Actor` 実装として書く。HOCON の `pekko.actor.allow-java-serialization = on`（`ActorRefSpec.scala:122-125`）はケース 3〜5 のための設定であり写さない（§2.3）。

現行の `fraktor-ng-rs` の生成モデル（`crates/fraktor-actor/src/kernel/executor/executor_value.rs:1236-1330, 1591-1624`）を先に確認しておく：

- `ActorSystem::actor_of`／`actor_of_named`・`ActorContext::actor_of`／`actor_of_with_strategy` は**非同期**であり、即座に `ActorRef` を返して `SystemMessage::Create` を積む。
- `Create` の実行（`Executor::create_now`）は `UnwindCatcher` の下で `Props` factory と `Actor::pre_start` を呼び、panic すれば `FailureKind::Initialization`（原因名 `"ActorInitializationException"`）を発生させ、既定の監督戦略が `Error` ログを発行して `Directive::Stop` で停止・名前解放する。
- **`Props::new` クロージャの中から `system.actor_of`（や `WeakActorSystem::spawn_child`／`try_spawn_child`）を呼ぶと、`Executor::lend` の排他ロックがすでに保持されているため再入 panic（`"already borrowed: re-entrant or concurrent use"`）で壊れる。** したがって、アクターの**実行中**の入れ子生成（`pre_start` から作る子など）は `context.actor_of`／`context.actor_of_with_strategy` を使う。`WeakActorSystem` に `try_actor_of` という口は無い（`weak_actor_system.rs:77, 112` の `spawn_child`／`try_spawn_child`）。
- **外側と内側の監督の形（ケース 1）**：原典 `ActorRefSpec.scala:148-149` の `import system.actorOf` により、外側も内側も `/user` の子（兄弟）として作られる。原典では外側の `Props` の creator を処理するときに `system.actorOf(Props(...))` が呼ばれ、内側の `Create` がそこで積まれる（内側の `Create` が全体の先頭に積まれるわけではない）。Rust では外側の factory の中から `system.actor_of` を呼べない（上の再入 panic）。そこで**テストの本体が先に内側を `system.actor_of_named` で作り、その参照を外側の factory に閉じ込めてから外側を `system.actor_of_named` で作る**。こうすると両方 `/user` の子になり、両方の失敗がそれぞれ親 `/user` の `Create` 処理の中で起きる。積まれる順は原典と変わるが、`occurrences` の数え方には影響しない（失敗の扱いは `Create` ごとに独立している）。
  - レビュー H4 の直し方にある「`pre_start` での入れ子の子の生成」は、ケース 1 では採らない。内側を外側の子として `pre_start` で作ると、外側が失敗して suspend している間に内側の失敗が `entry.held` に保留され、外側の停止で捨てられる（`executor_value.rs:2901-2916`）。双方失敗の `occurrences = 2` が 1 になり、原典の監督の形とも変わる。
- **`pre_start` での入れ子の生成は、親が失敗しないケースだけに使う**：ケース 12（`ActorRefSpec.scala:466-473` の `context.actorOf`）と ActorRefSpec の説明用の親が使う。ケース 6・7（`ActorRefSpec.scala:367-377`、`:379-388`）は原典が**コンストラクタの中で `system.actorOf`** を呼ぶ形（内側は `/user` の兄弟）であり、ケース 1 と同じく「テストの本体が先に `nested`／`inner` を作り、参照を外側の factory に渡す」形で写す。親が失敗しないので、`pre_start` を使う理由が無い。

| # | 原典のケース | Rust のテスト名 | 移す／除外と具体的な写し方 |
|:---:|:---|:---|:---|
| 1 | not allow Actors to be created outside of an actorOf | `does_not_allow_actors_to_be_created_outside_of_an_actor_of` | 移す（13 の小ケースの写し方は下の §1.3.1。**直接の `new Actor` を `ActorCell.contextStack` で禁じる性質は作らない（オーナー決定。§1.5 決定 4）**）。1 つの `FraktorSpec` の中で、原典と同じ順に小ケースを続けて流す |
| 2 | insert its path in a ActorInitializationException | `inserts_its_path_in_an_actor_initialization_exception` | 移す（§3.4、§3.5）。factory が panic するアクターを `system.actor_of_named(props, "failingActor")` で起こし、`EventFilter::typed("ActorInitializationException").contains("/user/failingActor:").with_occurrences(1).intercept(spec.system_mut(), ...)` で包む（フィルタは 1 本だけなので、既存の `EventFilter::intercept`（`&mut ActorSystem` を取る形）で足りる）。原典と同じく、この文字列は原因の文面（`"pekko://ActorRefSpec/user/failingActor: exception during creation, root cause message: [...]"`）に当たる。原典の `intercept[java.lang.IllegalStateException]`（`:279-283`。`Promise` で呼び手まで届くこと）は、§1.3.1 の中4 の `promise_intercept` で写す。原因に文面が無い（`throw new IllegalStateException`。`:282`）ので、届く panic の文面は空であることを確かめる |
| 3 | be serializable using Java Serialization on local node | — | **除外（オーナー決定：直列化の設計まで延ばす。§1.5 決定 1）**。Java Serialization 固有 |
| 4 | throw an exception on deserialize if no system in scope | — | **除外（オーナー決定：直列化の設計まで延ばす。§1.5 決定 2）**。`JavaSerializer.currentSystem` 固有 |
| 5 | return EmptyLocalActorRef on deserialize if not present in actor hierarchy (and remoting is not enabled) | `returns_an_empty_local_actor_ref_for_a_path_that_is_not_in_the_hierarchy` | ケース本体を移す（バイト列往復は**直列化の設計まで延ばす（§1.5 決定 3）**）。`ReplyActor` を `"non-existing"` で作って `PoisonPill` で停止確認（`watch`＋`expect_msg_terminated`）した後、そのパス文字列を `ActorPath::from_string` → `system.resolve_actor_ref` で解決した参照について、`EmptyLocalActorRef` の 4 性質すべてを検証する：① `resolved.path() == stopped.path()`、② `system.is_actor_terminated(&resolved) == true`、③ メッセージ送信で返信が無く `DeadLetter`（`recipient.path() == resolved.path()`）が発行される、④ `watch(&resolved)` で `Terminated { existence_confirmed: false, .. }` が即座に届く（原典 `ActorRefSpec.scala:341-365`）。原典の `==`（`:363`）は、`(id, generation)` の等価の決まり（I8）に写し、上の 4 点を数に入れる（決定：`pr288-decisions.md` の 3）。原典の等価は「パスと uid」の比較（`ActorRef.scala:181-183`）なので、パスの一致（①）が最も近い代わりである |
| 6 | support nested actorOfs | `supports_nested_actor_ofs` | 移す（形を変えない）。原典（`ActorRefSpec.scala:367-377`）はコンストラクタ内で `system.actorOf`（親 `/user`）を呼ぶ。テストの本体が先に `nested` を `system.actor_of` で作り、その参照を外側の factory に渡す（§1.3 の「外側と内側の監督の形」と同じ。親は `/user` のまま）。外側は受けた任意のメッセージに `nested` を返す。`ask` で受けた `nested` が `a` と別参照（`a != nested`）であることを検証する |
| 7 | support advanced nested actorOfs | `supports_advanced_nested_actor_ofs` | 移す（形を変えない）。原典（`ActorRefSpec.scala:379-388`）は `OuterActor(system.actorOf(Props(new InnerActor)))` なので、ケース 6 と同じくテストの本体が先に `inner` を `/user` の子として作り、外側の factory に渡す。`OuterActor` が `InnerActor` へ `ActorRef::forward(&inner, message, context)`（`actor_ref_value.rs:101`。`ActorContext::forward` は `pub(crate)` なので `ActorRef::forward` を使う）で転送し、`ask` で `"innerself"`（`!= a`）、`a`（`== a`）、`"self"`（`== a`）、`"msg"`（`== "msg"`）を検証する |
| 8 | support reply via sender | `supports_reply_via_sender` | 移す。`SenderActor` が `"complex"`／`"simple"` と `"complex2"`／`"simple"` の 2 巡を送り、`ReplyActor` と `WorkerActor`（`1s` を `TestKitSettings::dilated` で sleep）の応答で `TestLatch(4)` が 2 巡とも満ちることを確認し、最後に `system.stop` する |
| 9 | support actorOfs where the class of the actor isn't public | `supports_actor_ofs_where_the_type_of_the_actor_is_not_public` | 移す（形を変える。対応表 §2.3）。テストファイル内の非公開モジュール `mod non_public` に非公開のアクター型を置き、公開関数 `create_props() -> Props` 経由で `system.actor_of` し、`"pigdog"` を送って同じ文が返ることを確認し `system.stop` する |
| 10 | stop when sent a poison pill | `stops_when_sent_a_poison_pill` | 移す。`ask(5)` と `ask(0)` の後に `"PoisonPill"`（または定数）を送り、`"five"` と `"null"` の 2 返信が届いてから停止することを `watch`＋`TestKit::expect_msg_terminated`（または `TestProbe::expect_msg_terminated_within`）で確認する |
| 11 | restart when Kill:ed | `restarts_when_killed` | 移す。原典（`ActorRefSpec.scala:442-463`）の `filterException[ActorKilledException]`（`testkit/package.scala:50-51`）に合わせ、`EventFilter::typed("ActorKilledException").intercept(spec.system_mut(), ...)` で包む（フィルタ 1 本なので既存の `EventFilter::intercept` を使う）。ブロックの中で `TestLatch` を待つ処理は `spec.system_mut()` を通さないので、借用は衝突しない。親が `OneForOneStrategy(max_nr_of_retries = 2, within = 1s)` を持ち、子へ `Kill` を送って子の `pre_restart`・`post_restart` が `TestLatch(2)` を 5s 以内に満たすことを確認する |
| 12 | be able to check for existence of children | `can_check_for_the_existence_of_children` | 移す（§3.3）。親 `"parent"` が `pre_start` で子 `"child"` を作り、受けた名前に対し `context.child(name).is_some()` を返す。`ask("child") == true`、`ask("whatnot") == false` を確認する |

### 1.3.1 ActorRefSpec ケース 1 の 13 の小ケース

原典の小ケースは 2 つの性質を同時に見ている。

- **Rust で作れない性質**：`trait Actor` のコンストラクタが `ActorCell.contextStack`（`Actor.scala:500-506`、`ActorCell.scala:640-659`）を見て、`actorOf` の外で直接 `new` された `Actor` を `ActorInitializationException` で拒む。小ケース 1 はこれだけを見る。小ケース 2〜12 の `Failing*` の失敗の原因も、すべてコンストラクタの中の `val fail = new InnerActor`（`ActorRefSpec.scala:89, 98, 109, 118`）、または creator の中の 2 回目の `new`（`:257`）である。Rust の `Actor` は構造体に付けるトレイトなので、値を作るときにセルも検査も通らない。この性質は作らない（オーナー決定。§1.5 決定 4）。Pekko がこの禁止で守る「文脈の無いアクターが動かない」は、Rust では `actor_of` を通らないと動けないことで成り立つ。
- **Rust で写せる性質**：初期化に失敗したアクターごとに `ActorInitializationException` のエラーログが 1 件出る（`occurrences = 1`／`2`）。失敗の原因の文面がそのログに残る（小ケース 13）。**失敗そのものが呼び手まで届く**（原典の `promiseIntercept`／`wrap`）。失敗のあとも系は壊れず、次の生成が普通に動く（`contextStackMustBeEmpty()`）。

写し方：失敗の原因は「直接の `new`」の代わりに **`Props` の factory の中の panic** で起こす（オーナー決定：`pr288-decisions.md` の 4。前例：`classOf` → クロージャ）。panic の文面は原典の `contextStack` の文面（`Actor.scala:502-504` の `"You cannot create an instance of [...] explicitly using the constructor (new). ..."`）をテスト側の定数にして使う。Rust には継承が無いので、`FailingOuterActor` と `FailingInheritingOuterActor`、`FailingInnerActor` と `FailingInheritingInnerActor` は同じ形になる。内側と外側の作り方は §1.3 の「外側と内側の監督の形」に従う。各小ケースは `EventFilter::typed("ActorInitializationException").with_occurrences(n).intercept(spec.system_mut(), ...)` で包む。

**失敗を呼び手まで届ける（原典の `Promise` の写し）**：原典は `promiseIntercept(f)(to)`（`ActorRefSpec.scala:128-137`）で creator の中で例外を捕まえて `Promise` を失敗させ、`wrap`（`:139-144`）がテストのスレッドで `Await.result` して受け取り、挙げ直す。Pekko の `actorOf` も参照を先に返すので、「参照を先に返すから呼び手は受け取れない」は原典にも当てはまり、原典は `Promise` で越えている。Rust でも同じことをする。

```rust
// crates/fraktor-actor/tests/actor_ref_spec.rs（テスト側のヘルパ）
fn promise_intercept(tx: std::sync::mpsc::Sender<String>, make: impl FnOnce() -> Props) -> Props {
  // 原典 promiseIntercept：creator の中で失敗を捕まえて送り、そのまま投げ直す。
  // 投げ直した panic は今の Create の捕捉（executor_value.rs:1605-1624）にそのまま届き、
  // エラーログの件数も Terminated も今の設計のまま変わらない。
  Props::new(move || {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(make)) {
      | Ok(props) => {
        let _ = tx.send(String::new());
        props
      },
      | Err(payload) => {
        let text = panic_text(&payload); // &str / String を取り出す
        let _ = tx.send(text.clone());
        std::panic::resume_unwind(payload)
      },
    }
  })
}
```

- `FraktorSpec` は `UnwindCatcher` を入れており（`fraktor_spec.rs:87`）、`resume_unwind` で投げ直した panic は今の `Create` の捕捉に届く。エラーログの件数・`Terminated`・名前の解放は、panic を直接上げたときと同じになる。
- テストは `spec.system_mut().run_until_idle()` のあとに `rx.recv_timeout(..)` を読み、失敗が届いたこと（空でない文面）と、小ケース 13・ケース 2 では文面そのものを確かめる。
- 小ケース 2 の「外側の creator の中の入れ子」は、原典では内側の匿名アクターに `Promise` を渡している（`:160`）。Rust では外側の factory が内側を作る工程もテスト側にあるので、外側の factory から直接送る。
- **届く失敗の種類**：小ケース 2〜12 では、原典の `intercept[ActorInitializationException]` の代わりに届くのが factory の panic（`ActorInitializationException` ではない）になる。この点だけは写せない（決定 4 の「factory の中の panic に写す」に含まれる）。**それ以外（届くこと自体）は原典どおりに写すので、伺わずに数に入れる。**

| 小ケース | 原典の行 | 原典の形 | 原典の `occurrences` | `Promise` を渡す先（原典） | Rust の形 |
|:---:|:---|:---|:---:|:---|:---|
| 1 | `:150-152` | `actorOf` の外の `new Actor` | （フィルタ無し） | 無し | **作らない**（オーナー決定。§1.5 決定 4） |
| 2 | `:156-166` | 外側のコンストラクタの中の `new Actor` | 1 | 入れ子の匿名のアクター（`:160`） | 外側の factory が `panic_intercept` で panic。内側は作らない。届いた失敗があることを確かめる |
| 3 | `:168-175` | `FailingOuterActor(actorOf(InnerActor))` | 1 | 外側（`:171`） | 内側は正常、外側の factory が panic。届いた失敗があることを確かめる |
| 4 | `:177-184` | `OuterActor(actorOf(FailingInnerActor))` | 1 | 内側（`:180`） | 内側の factory が panic、外側は正常。届いた失敗があることを確かめる |
| 5 | `:186-193` | `FailingInheritingOuterActor(actorOf(InnerActor))` | 1 | 外側（`:189`） | 小ケース 3 と同じ形 |
| 6 | `:195-203` | `FailingOuterActor(actorOf(FailingInheritingInnerActor))` | 2 | 内側（`:199`） | 内側と外側の両方の factory が panic。届いた失敗があることを確かめる（両方あればよい） |
| 7 | `:205-213` | `FailingInheritingOuterActor(actorOf(FailingInheritingInnerActor))` | 2 | 内側（`:209`） | 小ケース 6 と同じ形 |
| 8 | `:215-223` | `FailingInheritingOuterActor(actorOf(FailingInnerActor))` | 2 | 内側（`:219`） | 小ケース 6 と同じ形 |
| 9 | `:225-234` | `OuterActor(actorOf(new InnerActor { val a = new InnerActor }))` | 1 | 内側の本文（`:229`） | 小ケース 4 と同じ形。届いた失敗があることを確かめる |
| 10 | `:236-244` | 小ケース 6 と同じ式（原典の重複） | 2 | 内側（`:240`） | 小ケース 6 と同じ形（原典に合わせて重複のまま流す） |
| 11 | `:246-253` | `OuterActor(actorOf(FailingInheritingInnerActor))` | 1 | 内側（`:249`） | 小ケース 4 と同じ形。届いた失敗があることを確かめる |
| 12 | `:255-263` | `OuterActor(actorOf({ new InnerActor; new InnerActor }))` | 1 | 内側の本文（`:259`） | 小ケース 4 と同じ形（2 回目の `new` が失敗する形）。届いた失敗があることを確かめる |
| 13 | `:265-274` | `OuterActor(actorOf(throw new IllegalStateException("Ur state be b0rked")))` | 1 | 内側の本文（`:268-270`） | 内側の factory が `panic!("Ur state be b0rked")`。フィルタを `with_message("Ur state be b0rked")` で絞る。原典の `logFailure` の文面は原因の `getMessage` そのもの（§3.4）なので、完全一致で原典の `getMessage should ===("Ur state be b0rked")` と同じことを確かめられる。**届いた panic の文面も `"Ur state be b0rked"` に完全一致すること**を確かめる（原典 `:271`） |

各小ケースのあとに、原典の `intercept[ActorInitializationException]` と `contextStackMustBeEmpty()` の代わりとして次を確かめる。

- **失敗が呼び手まで届いている**：上の `promise_intercept` の受け口が、空でない文面を受け取っている（小ケース 13・ケース 2 は文面の完全一致）。
- 失敗したアクターは監督に止められている：作る前に `watch` しておき、`expect_msg_terminated` で `Terminated` を受ける。失敗しなかった側（小ケース 3 の内側、小ケース 4 の外側など）は止まっていない（`system.is_actor_terminated(&r) == false`）。
- 名前が空いている：失敗したアクターと同じ名前で、正常なアクターを `system.actor_of_named` で作り直せる（`ActorRefSpec` の `ReplyActor` を使い、`ask` に返事が返る）。原典の `contextStackMustBeEmpty()`（`:154`）はこれで代える。

### 1.4 `#234` 段 6（`#257`）を前提にするケース（FSMTimingSpec ケース 7・8）

決定（`pr288-decisions.md` の 5・6）により、FSMTimingSpec のケース 7 とケース 8 は**どちらも `#257` の後へ延ばす**。扱いを両方で同じにする。

- **ケース 8**（`FSMTimingSpec.scala:115-129`、`:262-277`）は、FSM を外部から `suspend` した間に名前付きタイマ（1ms）と状態タイマ（`forMax(1ms)`）の両方の期限が来てメールボックスへ積まれ、`resume` の後に名前付きタイマ `Tock` が先に処理されて状態タイマをキャンセルし、続いて届く古い `StateTimeout` の印が捨てられることを見る。テストが依存する「セルの外からの `suspend`／`resume`」と「`mailbox.hasMessages` の観測口」は [Issue #234](https://github.com/j5ik2o/fraktor-ng-rs/issues/234) 段 6（[Issue #257](https://github.com/j5ik2o/fraktor-ng-rs/issues/257)）の実装範囲である。
- **ケース 7**（`FSMTimingSpec.scala:102-113`、`:237-251`）は、1ms のタイマ `"hallo"` を張り、`TestKit.awaitCond(context.asInstanceOf[ActorCell].mailbox.hasMessages, 1.second.dilated)`（`:240`）で印がメールボックスに積まれるのを待ってから取り消す。この待ちが観察の中心なので、待ちを落として移すと確かめる中身が別物になる。アクターが自分のメールボックスを読む口が無い点がケース 8 と同じである。
- 世代分離そのものは [PR #275](https://github.com/j5ik2o/fraktor-ng-rs/pull/275) および本設計 §3.1（`state_generation` と `named_timer_generation` の分離）で kernel に入る。
- ケース 7・8 は除外（`cases.excluded`）にも作らない区分（`cases.not_built`）にもしない。台帳の `FSMTimingSpec` ノードは `status: unported`、`cases.total: 10, ported: 8, passing: 8`、`verified_against` なしのまま残し、**`#257` 完了後にケース 7・8 を移植する後続 issue を切る**（[Issue #194](https://github.com/j5ik2o/fraktor-ng-rs/issues/194) の `LocalActorRefProviderSpec`・後続 [Issue #265](https://github.com/j5ik2o/fraktor-ng-rs/issues/265) および [Issue #200](https://github.com/j5ik2o/fraktor-ng-rs/issues/200) の `ReceiveTimeoutSpec` ケース 9 と同一の運用）。後続 issue には「`#257` で戻して数に入れる」と書く（決定の「延ばすものは、台帳と PR の本文にどの段で戻すかを書く」）。
- 代わりの裏付け（§5 P1・P2、§6 の Quint `staleNamedNeverFires`）は、`#257` を待つ間に世代分離と印の破棄の正しさを支えるためのもので、**ケース 7・8 の移植の代わりに数には入れない**。
- 残る 20 件は `#234` 段 6 に依存しない。

### 1.5 決定（作れないもの・延ばすもの。元帥・オーナーの判断）

`docs/decisions/pekko-semantics-first.md` の伺う基準（「Pekko どおりに写せるものは伺わずに写す。伺うのは Rust で同じ振る舞いを作れず、数から外す・弱めるときだけ」）に従い、6 件を上長とオーナーへ伺った。**6 件とも答えが出ている**（`pr288-decisions.md`）。以下は決定の内容と、原典・実物で確かめた裏付けである。この設計の段では台帳へは書かない。

| 項 | 決定 | 反映先 |
|:---:|:---|:---|
| 1・2・3 のバイト列往復（ActorRefSpec ケース 3・4・5 の直列化の部分） | **直列化の設計まで延ばす。** #190 の群 7 と同じ扱いで、今は数に入れない。直列化の段で `PrimitivesSerializationSpec` とまとめて設計する | §1.3（ケース 3・4 は除外、ケース 5 はバイト列を延ばして 4 性質を移す）、§8 |
| 3 の `==`（`EmptyLocalActorRef` との等しさ） | **`(id, generation)` の等しさの決まりに写し、代わりの 4 つの確かめで数に入れる** | §1.3 ケース 5、§4 I8 |
| 4 の小ケース 2〜12 | **数に入れる。**構築子の失敗は factory の中の panic に写す（前例：`classOf` → クロージャ、項目 441 の 2 問目でオーナー承認済み） | §1.3.1 |
| 4 の小ケース 1（`ActorRefSpec.scala:150-152`、`actorOf` の外で直接 `new Actor` すると拒む） | **作らない**（JVM 固有の除外。分類は「対象が Rust に無い（言語の違い）」） | §1.3.1 小ケース 1、§8 台帳 |
| 5・6（FSMTimingSpec ケース 8、ケース 7 のメールボックス待ちの省略） | **`#257` の後に延ばす。**`#257` が入ったら戻して数に入れる。台帳と PR の本文に、どの段で戻すかを書く | §1.2 ケース 7・8、§1.4、§8 台帳 |

- 小ケース 1 を作らない理由（決定の文面）：Pekko がこの禁止で守る「文脈の無いアクターが動かない」は、Rust では `actor_of` を通らないと動けないことで成り立つ。「作った時点で失敗」まで写すと、全アクターの書き方が変わる。
- **「届く失敗の種類」**（小ケース 2〜12 で `ActorInitializationException` ではなく factory の panic が届く）は、決定の 4「構築子の失敗は factory の中の panic に写す」に含まれる。**届くこと自体は中4 のとおり原典どおりに写す。**

#### 決定 1：ActorRefSpec ケース 3（`be serializable using Java Serialization on local node`）を直列化の設計まで延ばす

- **原典の行**：`actor-tests/src/test/scala/org/apache/pekko/actor/ActorRefSpec.scala:288-317`
- **作れない理由**：`java.io.ObjectOutputStream`／`ObjectInputStream` と `JavaSerializer`（`ActorRef.writeReplace` → `SerializedActorRef` → `readResolve`）という JVM 固有の直列化機構そのものを検査しており、Rust に対応する仕組みが無い。
- **原典の Serialization 経路の調査結果（grep 実測）**：
  - `actor-tests/src/test/scala/org/apache/pekko/serialization/PrimitivesSerializationSpec.scala`：`ActorRef`／`SerializedActorRef`／`serializedActorPath` の検査は **0 件**（`Long`/`Int`/`Boolean`/`String`/`ByteString` のプリミティブのみ）。
  - `actor-tests/src/test/scala/org/apache/pekko/serialization/SerializeSpec.scala:520-537`：`"serialize DeadLetterActorRef"` があるが、これも同じ `ObjectOutputStream`／`ObjectInputStream`＋`JavaSerializer.currentSystem.withValue` の JVM 固有検査であり、一般の `LocalActorRef` の非 Java 直列化経路ではない。
  - `remote/src/test/scala/org/apache/pekko/remote/serialization/MiscMessageSerializerSpec.scala:91, 112, 152-160` および `SerializationTransportInformationSpec.scala:39-40, 63-64, 115`：protobuf ベースの `MiscMessageSerializer` と `Serialization.serializedActorPath` による `ActorRef` 直列化検査は `pekko-remote` モジュール側に置かれており、`actor-tests`（`pekko-actor`）の範囲外である。
  - なお原典の本体 `actor/src/main/scala/org/apache/pekko/actor/ActorRef.scala:445-462` の `SerializedActorRef` の実体は、`Serialization.serializedActorPath(actorRef)` のパス文字列を保存し、`readResolve` で `provider.resolveActorRef(path)` を呼ぶだけである。
- **Rust で代わりに確かめること**：`SerializedActorRef` の非 JVM な本質である「パス文字列を経由した解決（`ref.path().to_string()` → `ActorPath::from_string` → `ActorSystem::resolve_actor_ref`）で同一の生存中 `ActorRef` が返ること」は、既存の `crates/fraktor-actor/tests/local_actor_ref_provider_spec.rs:92-95` と `crates/fraktor-actor/src/kernel/actor_ref/actor_ref_value_test.rs` で確かめている。
- **扱い**：この issue では数に入れず、直列化の設計の段で戻す。台帳には直列化の設計まで未移植として残す（「直列化が入ったら見直す」の但し書きつき。前例：FunctionRefSpec の「not be found」の Java 直列化による除外、`ledger/nodes.yaml:3008-3011`）。

#### 決定 2：ActorRefSpec ケース 4（`throw an exception on deserialize if no system in scope`）を直列化の設計まで延ばす

- **原典の行**：`actor-tests/src/test/scala/org/apache/pekko/actor/ActorRefSpec.scala:319-339`
- **作れない理由**：`JavaSerializer.currentSystem`（スレッドローカル `DynamicVariable`）を設定せずに `ObjectInputStream.readObject` したときの `IllegalStateException` を検証する JVM 固有のケースである。Rust の `ActorSystem::resolve_actor_ref(&self, path: &ActorPath)` は `&ActorSystem` を型で必須としており、「系がスコープに無い状態で解決する」こと自体が型上作れない。
- **Rust で代わりに確かめること**：型システムにより静的に排除される（動的な代替テストは無い）。
- **扱い**：ケース 3 と同じく、直列化の設計の段で戻す。同じ但し書きを付ける。

#### 決定 3：ActorRefSpec ケース 5 のバイト列往復を直列化の設計まで延ばし、`EmptyLocalActorRef` 同士の等価は `(id, generation)` の決まりに写す

- **原典の行**：`actor-tests/src/test/scala/org/apache/pekko/actor/ActorRefSpec.scala:341-365`（バイト列往復は `:344-354, 361-363`、`EmptyLocalActorRef` の `===` 等価比較は `:363`）
- **作れない理由**：`SerializedActorRef` を `ObjectOutputStream`／`ObjectInputStream` でバイト列化する部分は JVM 固有で作れない。また解決結果同士を `==` で比較する部分は、所有権モデルの決定により `fraktor-ng-rs` の `ActorRef` の等価性が `(id, generation)` で決まり（I8）、未存在パスの解決ごとに別 id が採番される仕様と両立しない。
- **Rust で代わりに確かめること**：ケース本体は移し、停止済みの `"non-existing"` のパスを `ActorPath::from_string` → `ActorSystem::resolve_actor_ref` で解決した参照に対し、`EmptyLocalActorRef` の 4 つの観測可能な振る舞い（① `path` 一致、② `is_actor_terminated == true`、③ メッセージ送信で返信せず `DeadLetter`（`recipient.path() == ref.path()`）を発行、④ `watch` で `Terminated(existence_confirmed = false)` を即時返却）をすべて検証する。
- **扱い**：バイト列往復（`:344-354, 361-363`）は決定 1・2 と同じく直列化の設計の段で戻す。`==`（`:363`）は `(id, generation)` の等価の決まりに写し、上の 4 点を数に入れる。原典の等価（`ActorRef.scala:181-183`）は「パスと uid」の比較なので、①が最も近い代わりである。

#### 決定 4：ActorRefSpec ケース 1 の直接 `new Actor` 禁止（`ActorCell.contextStack` 検査）の小ケース

- **原典の行**：`actor-tests/src/test/scala/org/apache/pekko/actor/ActorRefSpec.scala:148-263`（サブケース 1：`:150-152`、サブケース 2〜12：`:156-263`。原典の検査本体は `Actor.scala:500-506`・`ActorCell.scala:374-376, 640-659` の `ThreadLocal` `contextStack`）
- **作れない理由**：Pekko では `trait Actor` のコンストラクタが `ActorCell.contextStack` を検査し、`actorOf` の外での `new Actor`（サブケース 1）だけでなく、コンストラクタ内で別の `Actor` を直接 `new` する行為（サブケース 2〜12 の `FailingOuterActor`・`FailingInnerActor` 等にある `val fail = new InnerActor` および `{ new InnerActor; new InnerActor }`）もすべて `ActorInitializationException` で禁止する。Rust では `Actor` は通常の構造体に実装するトレイトであり、構造体値の構築時にセルも自己参照も作られず、`contextStack` のようなコンストラクタフックを置けないため、直接値構築そのものを禁止する性質（サブケース 1〜12 の `contextStack` 検査）は作れない。
- **Rust で代わりに確かめること**：小ケース 1 は作らない（決定）。小ケース 2〜12 は、失敗の原因（直接の `new`）を `Props` の factory の中の panic に置き換え、それ以外は原典どおりに写す（§1.3.1 の表）。確かめるのは次のとおり：① 失敗したアクターごとに `ActorInitializationException` のエラーログが出て、監督が止める（`Terminated` を受ける）、② 外側だけ・内側だけの失敗は `occurrences = 1`、両方の失敗（小ケース 6・7・8・10）は `occurrences = 2`、③ 小ケース 13 で根の原因の文面 `"Ur state be b0rked"` がログの文面に完全一致で残る、④ **失敗が呼び手まで届く（`promise_intercept`。中4）**、⑤ 失敗のあとに同じ名前で正常なアクターを作り直せて返事が返る（`contextStackMustBeEmpty()` の代わり）。
- **代わりの失敗の種類**：小ケース 2〜12 で届くのは `ActorInitializationException` ではなく factory の panic である。これは決定の「構築子の失敗は factory の中の panic に写す」に含まれる。それ以外は原典どおりなので、伺いは残っていない。

#### 決定 5：FSMTimingSpec ケース 8 を `#234` 段 6（`#257`）の後へ回す

- **原典の行**：`actor-tests/src/test/scala/org/apache/pekko/actor/FSMTimingSpec.scala:115-129`、`:262-277`
- **今移せない理由**：外からの `ActorRefWithCell.suspend()`／`resume()` と `mailbox.hasMessages` の観測口が未実装であり、それらは [Issue #234](https://github.com/j5ik2o/fraktor-ng-rs/issues/234) 段 6（[Issue #257](https://github.com/j5ik2o/fraktor-ng-rs/issues/257)）の範囲である。
- **決定と扱い**：`#257` の後に延ばす。`#257` が入ったら戻して数に入れる。除外・作らない区分にはせず、台帳 `cases.total: 10, ported: 8, passing: 8, status: unported` のまま残し、#257 完了後の後続 issue で移植する（§1.4）。
- **代わりの裏付け**：名前付きタイマ受信時の状態タイマキャンセルと世代分離は、単体テスト・PBT（P1・P2：手動時計 `ActorSystem::set_clock` でメールボックス滞留順序を確定的に再現）・Quint（`crates/fraktor-actor/specs/fsm_timing.qnt`）で先行検証する。ただしこれは裏付けであり、ケース 8 を今数に入れる理由にはしない。

#### 決定 6：FSMTimingSpec ケース 7 も `#257` の後へ回す

- **原典の行**：`actor-tests/src/test/scala/org/apache/pekko/actor/FSMTimingSpec.scala:102-113`（テスト本体）、`:237-251`（`when(TestCancelTimer)`、滞留待ちは `:240` の `TestKit.awaitCond(context.asInstanceOf[ActorCell].mailbox.hasMessages, 1.second.dilated)`）
- **同じ形で作れない理由**：原典は状態関数の中で 1ms タイマを張り、その印がメールボックスに入るまで待ってから `cancelTimer` する。fraktor-ng-rs には、アクターが自分のメールボックスの滞留を見る口が無い（`mailbox.hasMessages` は #257 の範囲で、ケース 8 と同じ）。
- **決定と扱い**：待ちを落とした形で今写すと、原典が確かめる「すでに届いた印が捨てられる」を通らないために確かめる中身が別物になる（案 C では、期限の検査は `Executor` の `run_one`／`run_until_idle` の境目と `await_envelope` の前後で走るので、状態関数が走っている間に印がメールボックスへ入ることは無い）。そこでケース 8 と同じく `#257` の後に延ばす。台帳は `ported: 8, passing: 8`（§1.2、§1.4）。
- **代わりの裏付け**：「滞留した古い印は取り消し・張り直しのあとに捨てられる」と「古い印は再起動のあとに捨てられる」は、§5 P1 の決まった手順（手動時計で期限を過ぎさせ、`Cancel` が処理される前に印を積ませる）と §6 の Quint（`staleNamedNeverFires`）で確かめる。ただしこれは裏付けであり、ケース 7 を今数に入れる理由にはしない。

## 2. fraktor-ng-rs の現状（`main` `53014ed` 基準）

[PR #275](https://github.com/j5ik2o/fraktor-ng-rs/pull/275)（#181）および [PR #283](https://github.com/j5ik2o/fraktor-ng-rs/pull/283)（#200 `ReceiveTimeoutSpec`）がマージ済みの `main`（`53014ed`）の実物を確認した。

### 2.1 FSM・タイマ・TestKit 側

| 原典の本体 | fraktor-ng-rs の現状（実物確認済み） |
|:---|:---|
| `FSM` の状態遷移（`goto`, `stay`, `using`, `replying`, 通知, 購読） | ある（`crates/fraktor-actor/src/kernel/actor/fsm.rs`、`fsm_state.rs`、`fsm_view.rs`）。ただし `Fsm<S, D>` の境界は `S: Clone + PartialEq + Send + Sync + 'static` で `Display`/`Debug` 境界は無い。`FsmView<S>` は 9 ファイル 27 箇所で型注釈付きで使われている |
| FSM のタイマの表、世代、状態タイマのスロット | **`#[cfg(feature = "test-support")]` の下にだけある**（`fsm.rs:106-111`、`test_support/fsm_handle.rs`、`test_support/fsm_control.rs`）。世代カウンタが `FsmHandle` 側にあり、通常の機械（ハンドルなし）にはタイマ表も状態タイマも無い。また `FsmControl` にテスト制御と発火印（`TimerMark`／`StateTimeoutMark`）が混在している |
| 状態ごとのタイムアウト（`when(state, stateTimeout = …)`）と `forMax` | 無い。さらに現行 `fsm.rs:587-590` は `apply_event` が `None`（未処理）のとき `make_transition` を呼ばないため、未処理イベント後に既定の `stateTimeout` が張り直されない |
| `onTransition` からのタイマ操作 | 無い。`Fsm::on_transition`（`fsm.rs:221`）と `on_transition_with_data`（`fsm.rs:239`）は `&FsmView` を受け取らない |
| 状態関数からのタイマ操作（`startSingleTimer` 等）と `isTimerActive` | 無い。`FsmView<S>` は `listener_count` と `state_name` の読み取り専用 |
| タイマの発火を運ぶ仕組み | **`Executor` に `#200`（[PR #283](https://github.com/j5ik2o/fraktor-ng-rs/pull/283)）の期限索引（`BTreeMap`）＋`Clock`＋`await_envelope` 期限カットがある**（`executor_value.rs:432, 883-1064, 2321-2500`）。一方、`TestFsmRef` は `test_fsm_ref_value.rs:340-394` に独自の 1ms ポーリングスレッド `drive_timers` を持つ。kernel の汎用 `Scheduler` port は `#234` 段 6（[Issue #257](https://github.com/j5ik2o/fraktor-ng-rs/issues/257)）の持ち物 |
| `StateTimeout` のイベント型 | ある（`crates/fraktor-actor/src/kernel/actor/state_timeout.rs`、`kernel::actor::StateTimeout` で既公開） |
| `whenUnhandled` と未処理警告 | `Fsm::when_unhandled` の構築時登録のみある。実行中の差し替え・解除が無く、未処理警告も発行していない。`ActorContext` にも任意の `Warning` を `event_stream`/`event_sink` へ出す口が無い（`publish_unhandled` は `UnhandledMessage` と `"unhandled message from ..."` 固定） |
| `Message` の文字列化 | `message_value.rs:43-56, 117-127` の `Payload` トレイトは `Any + Send + Sync` のみで `Display`/`Debug` を保持せず、`Debug for Message` は `&'static str` と `i32` 以外を `"<message of an unknown type>"` と出力する（`unhandled_notification_spec.rs:132` がこの出力を検証している） |
| テストの土台（`within`, `ignoreMsg`, `expectNoMessage`, `receiveWhile`, `EventFilter`） | `FraktorSpec::within`／`within_range`／`remaining`／`receive_while`（`fraktor_spec.rs:196, 235-287`）、`TestKit::ignore_msg`／`expect_no_message`／`expect_msg_terminated`（`test_kit_value.rs:149, 242, 285`）、`TestProbe::expect_msg_terminated_within` がある。**不足**：`EventFilter::Warning` がユニットバリアントで `with_message` を黙殺しており（`event_filter_value.rs:43, 117`）、`source` 照合・`pattern` 部分一致・`occurrences` 指定・複数フィルタ同時登録 `filter_events` がいずれも未実装 |

### 2.2 ActorRef 側

| 原典の本体 | fraktor-ng-rs の現状（実物確認済み） |
|:---|:---|
| `ActorRef`（等価、ハッシュ、パス、`tell`、`forward`） | ある（`actor_ref_value.rs`）。等価・ハッシュは `(id, generation)`。`ActorRef::forward(&self, message, context)` が公開（`actor_ref_value.rs:101`）。`ActorContext::forward` は `pub(crate)`（`actor_context.rs:502`） |
| 生成と初期化失敗（`ActorInitializationException`） | 非同期生成モデル（`executor_value.rs:1236-1330, 1591-1624`）。`actor_of` は即座に `ActorRef` を返し、`SystemMessage::Create`（`create_now`）が `UnwindCatcher` 下で factory＋`pre_start` を呼ぶ。panic 時は `FailureKind::Initialization`（原因名 `"ActorInitializationException"`）となり、既定監督が `Stop` して停止・名前解放する。**不足**：`executor_value.rs:1591-1624, 2872-2891, 1620` はパスを `log_source` にしか入れず `message` には生の panic 文字列しか入っていない（原典 `Actor.scala:203-204` の `enrichedMessage` は `s"${actor.path}: $message"`） |
| `PoisonPill`、`Kill` | ある。通常メッセージ（`&'static str`）として `Executor::invoke`（`executor_value.rs:2408-2470`）で特別扱いし、`PoisonPill` が `SystemMessage::Terminate` を積み、`Kill` が `ActorKilledException` の失敗になる |
| 監督（`OneForOneStrategy`）・`ask`・`TestLatch` | ある（`supervisor_strategy.rs`, `AskFuture`, `TestLatch`） |
| 子の存在確認（`context.child(name)`） | **無い**。`ParentEntry.children: Vec<(String, ActorRef)>`（`executor_value.rs:255`）はすでにあるが `ActorContext` から引く口が無い |
| 階層にいないパスの解決（`EmptyLocalActorRef` 相当） | ある（`executor_value.rs:1146-1164, 2855`、`actor_system_value.rs:397-405`）。未存在パスに返す参照は ① パスを保持、② `is_actor_terminated == true`、③ 送信で `DeadLetter`（`recipient.path() == ref.path()`）を発行、④ `watch` で `Terminated(existence_confirmed = false)` を即時返却、の 4 点を満たす |
| Java Serialization | 無い（§1.5 決定 1〜3） |

### 2.3 対応表（`docs/decisions/scala-java-to-rust-correspondence.md`）を引いた結果

- `Duration.Inf` の汎用代数は作らず、FSM のタイムアウトには `Option<Duration>` を比較に使わない専用 enum `FsmTimeout` を使う（`docs/design/issue-185-util-specs.md:123`）。
- `classOf[…]`／非公開クラスのリフレクション生成は factory 関数が返す `Props` で写す（`docs/design/issue-193-specs.md:46`）。ActorRefSpec ケース 9 が従う。
- HOCON の設定文字列は写さない（`docs/goals/issue-154-id-generator-and-dns-settings.md:26`）。
- 伺う基準：Pekko どおりに写せるものは伺わずに写し、作れない／弱める／保留する §1.5 の 6 件だけを伺った（答えは §1.5 に書いた）。

## 3. 新しく足すものと、公開 API の形

新しい型は syn-check の 1 ファイル 1 型・平坦モジュール規則に従い、`crates/fraktor-actor/src/kernel/actor/` 直下に置く（入れ子のサブディレクトリは作らない）。追加は `crates/fraktor-actor` の kernel と `crates/fraktor-testkit` の `event_filter`（および `TestFsmRef` の二重ドライバ解消）である。既存の公開 API のシグネチャ・ジェネリック境界は壊さない。

### 3.1 FSM のタイマ・状態タイムアウト・発火運搬（案 C 採用）

#### (a) 原典 `FSM.scala:511-513, 615-644, 755-763, 768-772, 817-914` との 1 対 1 対応

1. **状態登録（`when` / `when_with_state_timeout`、`FSM.scala:475-476, 755-763`）**：
   - 同一状態 `S` に複数回登録した場合、状態関数は登録順に試行し（`orElse`）、状態の既定タイムアウトは **`timeout.or(existing)`** で合成する（新しい登録が `Some(d)` なら上書き、`None` なら既存の既定タイムアウトを保持する）。
2. **`onTransition` からのタイマ操作（`FSMTimingSpec.scala:225-227`）**：
   - 既存の `Fsm::on_transition`（`(&mut ActorContext<'_>, &S, &S)`）と `Fsm::on_transition_with_data`（`(&mut ActorContext<'_>, &S, &S, &D, &D)`）のシグネチャは変えない。
   - 新たに **`Fsm::on_transition_with_view`**（`(&mut ActorContext<'_>, &FsmView<S>, &S, &S)`）を追加し（内部の `TransitionHandler<S, D>` に `&FsmView<S>` を通す）、遷移ハンドラの中から `view.start_single_timer` 等を呼べるようにする。`FSM.scala:893-899` と同じく `handleTransition` は `currentState = nextState` の前に走るため、`view.state_name()` は遷移前の状態名を返し、直後にタイマ操作を `Executor` へ反映する。
3. **`FsmView` のタイマ操作の即時反映（`FSM.scala:596-625`）**：
   - 状態関数のシグネチャ（`&mut ActorContext<'_>, &FsmView<S>, &Message, &D -> Option<FsmState<S, D>>`）は変えない。
   - `Fsm` は名前付きタイマのライブ表を `Arc<spin::Mutex<FsmTimerState>>` で持ち、`FsmView<S>` にその参照を貸す。`FsmTimerState` は「ライブな名前付きタイマ表 `timers`」「名前付きタイマ専用の単調増加カウンタ `named_timer_gen: u64`」「`Executor` 期限索引への同期キュー `pending_ops`」「（`test-support` 時のみ）`FsmHandle` への同期フック」を持つ。
   - 状態関数または `on_transition_with_view` の中で `view.start_single_timer`／`start_timer_with_fixed_delay`／`start_timer_at_fixed_rate`／`cancel_timer` を呼ぶと、**即座に `timers` を更新して `named_timer_gen` を進め、`pending_ops` に積む**。したがって、**同一ハンドラ内で直後に `view.is_timer_active(name)` を呼べば直前の開始・取り消しを即座に反映した真偽値が返る**（原典 `FSM.scala:625` と一致）。複数登録された状態関数が `None` でフォールバックした先や `whenUnhandled` からも同じライブ状態が見える。
   - 状態関数・各遷移ハンドラの終了直後に `Fsm` が `pending_ops` を `context` 経由で `Executor` の期限索引へ反映する（1 ターン中に `start` → `cancel` されたタイマは索引に残らず、最終的にライブ表に存在する行のみ最新世代で期限登録される）。また `test-support` 有効時は `FsmHandle` のタイマ記録へも同期し、`TestFsmRef::is_timer_active` が内側から張ったタイマも見える約束（#181 設計 line 219）を満たす。
4. **`receive` と `make_transition` の完全な手順（`FSM.scala:817-914`）**：
   - `SubscribeTransitionCallBack`／`UnsubscribeTransitionCallBack`／`ListenerTerminated`：状態タイマを取り消さず、`state_generation` も進めない。
   - **通常メッセージ**：
     1. 張ってある状態タイマを取り消し（`Executor` 期限索引から削除し `state_timer = None`、`handle` 同期）、`state_generation += 1`。
     2. 状態関数を順に試行し、すべて `None` なら `unhandled_function` を試行する。
     3. いずれも `None` の場合（未処理）：§3.2 の警告 `"unhandled event {event} in state {state}"`（`source = self.path()`）を発行し、**`FsmState::stay()` として `make_transition` を通す**（原典 `FSM.scala:768-772, 868-871`）。
     4. `Some(next)` の場合：`pending_ops` を反映し、`next` に載った `whenUnhandled` の差し替え／解除（§3.2）を適用した上で `make_transition` を通す。
   - **名前付きタイマの印（`FsmTimerMark::Named { instance_id, name, generation }`）**：
     1. `instance_id == self.instance_id` かつ `timers` に `name` があり `entry.generation == generation` のときのみ受理（原典 `FSM.scala:823` の `owner eq this` を含む 3 条件）。不一致は黙って破棄する。
     2. 受理した場合：`Single` なら `timers` から削除（`handle` 同期）し、通常メッセージと同じく状態タイマを取り消して `state_generation += 1` したうえで、そのタイマの `message` をイベントとして処理する（状態関数 → 未処理なら警告＋`stay()` → `make_transition`）。
   - **状態タイムアウトの印（`FsmTimerMark::StateTimeout { instance_id, generation }`）**：
     1. `instance_id == self.instance_id` かつ `self.state_timer == Some(generation)` かつ `self.state_generation == generation` のときのみ受理。不一致は黙って破棄する。
     2. 受理した場合：原典 `FSM.scala:818-821` どおり **`state_generation` は進めない**。`StateTimeout` をイベントとして処理し `make_transition` を通す。次の遷移が新しい状態タイマを張らない場合（例：`for_max(Infinite)` または既定タイムアウト無しの状態への遷移）、`Executor` の期限行は発火時に消費済みのため二度と発火しない一方、`is_state_timer_active`（`self.state_timer.is_some()`）は原典どおり次の通常メッセージまで `true` のまま残る（#181 設計 I5 を維持）。次の遷移が新しい状態タイマを張る場合（`for_max(After(d))` または既定タイムアウトありの状態への `stay()`/`goto`）は、現在の `state_generation` のまま `Executor` に新しい期限を登録する。
   - **`make_transition` での状態タイムアウト選択（`FSM.scala:901-912`）**：
     - 遷移先が存在しない状態なら遷移せず、状態タイマも張らない（現行どおり）。さらに、原典 `FSM.scala:886-888` はこのとき `terminate` を呼び、`terminate`（`FSM.scala:933-939`）は**名前付きタイマをすべて取り消して表を空にし、状態タイマも取り消す**。この issue で通常の機械にも名前付きタイマが入るので、存在しない状態への遷移でも名前付きタイマの表と `Executor` の期限索引の行を空にする（§6 の Quint `gotoNonExistentClearsTimersTest`）。原典はこのとき `logTermination` でエラーログ（`"Next state X does not exist"`）も出し、`onTermination` を呼ぶが、port には `onTermination` が無く、ログも今は出していない（`fsm.rs:293-306` の doc）。これはタイマと別の話なので、この issue では足さない（§8 の別件）。
     - 遷移が成立したら、`replies` 送信 → `onTransition` 実行（直後に `pending_ops` を反映）→ リスナー通知 → `current` 更新（`handle` スナップショット同期）の順で進めた後、`next_state.timeout` に従って状態タイマを張る：
       1. `Some(FsmTimeout::Infinite)` → **張らない**（既定タイムアウトを無効化）。
       2. `Some(FsmTimeout::After(d))` → `d` で張る（`self.state_timer = Some(self.state_generation)`、`Executor` 期限索引へ登録、`handle` 同期）。
       3. `None`（`goto`／`stay()`／未処理既定のすべてがここに入る）→ `state_timeouts[next_state]` が `Some(d)` なら `d` で張る、`None` なら張らない。
   - **`initialize()`（`pre_start`）**：`initialized == true` なら初期状態への `make_transition` を実行し、初期状態に既定タイムアウトがあれば張る。`StoppingActor` のように `initialize()` を呼ばない機械では `pre_start` で張らず、最初のイベント処理の `make_transition(stay())` で初めて張られる（原典どおり）。
   - **停止（`post_stop_with_context`）**：`timers`・`state_timer` を空にし、`context` 経由で `Executor::clear_fsm_timers(self_key)` を呼んで期限索引から全行を即時削除する。さらに `Executor` 側の停止完了・`faultCreate`・`stop_all` でも `clear_fsm_timers(key)` を呼び、panic で `post_stop` が抑止されても期限が残らないようにする。

#### (b) 再起動をまたぐ `instance_id` 分離と `test-support` の公開面最小化（レビュー M1）

- `Fsm::new()` ごとに `portable_atomic::AtomicU64` の静的カウンタから一意の `instance_id: u64` を採番する（`no_std` 対応）。
- `state_generation: u64` と `named_timer_gen: u64` を別カウンタにし、名前付きタイマの開始が状態タイマ世代を動かさないことを構造で保証する（[PR #275](https://github.com/j5ik2o/fraktor-ng-rs/pull/275) の I2 をさらに明確化）。
- 再起動で新しい `Fsm` 個体が作られローカル世代が `0` から始まっても、古い個体の印は古い `instance_id` を持つため必ず破棄される（原典 `FSM.scala:823` の `owner eq this` に対応）。
- 発火印は **`pub(crate) enum FsmTimerMark`**（`Named { instance_id, name, generation }`／`StateTimeout { instance_id, generation }`）として `crates/fraktor-actor/src/kernel/actor/fsm_timer_mark.rs` に独立させ、`test_support::FsmControl`（`SetState`／`StartTimer`／`CancelTimer`／`Sentinel`）は引き続き `#[cfg(feature = "test-support")]` の下に閉じ込める。`FsmTimerKind` は今のまま、非公開のモジュール `test_support/fsm_timer_kind.rs` に `pub enum FsmTimerKind` として置き、`test_support.rs` の `pub use fsm_timer_kind::FsmTimerKind;` で `pub mod test_support`（`#[cfg(feature = "test-support")]` つき）から出す（`crates/fraktor-actor/src/kernel/actor/test_support.rs:36`、`actor.rs:60`）。kernel の側では `pub(crate)` として使い、**crate の外の公開面には出さない**（`pub(crate)` の項目は `pub use` で外へ出せず E0364 になる、というのが前の版の誤りだった）。

#### (c) 発火の運搬：案 C（`#200` 型の `Executor` 期限索引＋`Clock`＋`await_envelope` 期限カット）を採る

| 案 | 評価 | 結論 |
|:---|:---|:---|
| 旧案 A（kernel に FSM 専用の共有状態登録 `Scheduler` port＋std タイマスレッド） | `#234` 段 6（[Issue #257](https://github.com/j5ik2o/fraktor-ng-rs/issues/257)）の汎用 `Scheduler` port（`schedule_once(delay, task) -> Cancellable`、`docs/design/issue-234-multithread-executor.md:219`）と形・責務が真っ向から衝突する。`#181` の判断にも反し、`terminate` 時のスレッド停止・`set_clock` 整合・PBT 決定論性・`TestFsmRef::drive_timers` との二重発火（Bugbot 指摘）を破綻させる | **却下** |
| 旧案 B（`#181` の testkit `drive_timers` を汎用化） | 通常の `actor_of` で作る製品 `Fsm` にタイマ発火が無いまま残る | **却下** |
| **案 C（採用：`#200` 型の `Executor` 期限索引＋`Clock`＋`await_envelope` 期限カット）** | `main`（[PR #283](https://github.com/j5ik2o/fraktor-ng-rs/pull/283)）に既に入っている `ReceiveTimeout` の期限索引（`BTreeMap`＋`Clock`＋`await_envelope` 期限カット）と同じ仕組みに FSM タイマの期限を載せる。`#257` の `Scheduler` port を先取り・二重化せず、`no_std` 完全対応、新スレッド不要で CI 揺れ・スレッドリークなし、`ActorSystem::set_clock` で決定論的 PBT が書ける。`TestFsmRef::drive_timers` スレッドを廃止して単一ドライバに統一できるため二重発火も構造上起きない | **採用** |

案 C の具体設計（`executor_value.rs` への追加）：

- `Executor` に FSM タイマの期限索引を置く。**FSM の表は FSM 専用であり、汎用のタイマの表ではない**（`#200` §13-3 の条件 (b)）。
  - `fsm_timers: BTreeMap<(ActorKey, FsmTimerSlot), FsmScheduledTimer>`（`FsmTimerSlot` は `State` または `Named(String)`）
  - `fsm_timer_index: BTreeMap<(Duration, u64), FsmTimerIndexEntry>`。キーは **`(期限, seq)`** である（中3）。
- **同じ期限の撃つ順（中3）**：索引の第 2 のキーは `ActorKey` や `FsmTimerSlot` ではなく、**予約するたびに単調に増える番号 `seq: u64`** にする。
  - 原典 `LightArrayRevolverScheduler` は期限ごとのバケツに入れた順（FIFO）に実行するので、同じ期限なら**予約した順**に撃つ。
  - FSM では、状態関数の中の `startSingleTimer` が先に予約され、状態タイムアウトは `make_transition` の最後（`FSM.scala:906-912`）で予約される。手動の時計はハンドラの中で進まないので、同じハンドラの中で張った名前付きタイマと状態タイマは同じ長さなら同じ期限になり、原典では**名前付きタイマが先**である。`(Duration, seq)` のキーはこれをそのまま表す（ケース 8 の「古い `StateTimeout` が捨てられる」道もこの順に頼る）。
  - **受信タイムアウトの索引との間の順**：`Executor` は 1 つの `seq` のカウンタを FSM タイマと受信タイムアウトで共有し、発火の入口（`fire_due_timers`）で**両方の索引を合わせて `(期限, seq)` の小さい順に撃つ**。`#200` の受信タイムアウトの索引（`executor_value.rs:432` の `BTreeMap<(Duration, ActorKey), u32>`）は第 2 のキーが `ActorKey` なので、同じ入口で撃つために受信タイムアウトの側にも `seq` を足す。受信タイムアウトは繰り返さないので、意味は変わらない（同時刻の複数のアクターの間の順だけが予約順になる。原典も同じ）。
  - P1・P2 の参照モデルも同じ規則（同じ期限なら `seq` の順）で発火列を作る、と §5 に書く。
- 登録・置き換え・取り消し・停止のたびに旧行を索引から即時削除し、索引に古い行を残さない（`#200` の `sync_receive_timeout_index` と同じ方針）。
- 期限の計算は `Executor::now_opt()`（`self.clock` 優先、無ければ `boundary.monotonic_time()`）を使う。時計が無い状態で有限の FSM タイマを張ろうとした場合は、FSM タイマ専用の理由を付けて panic する（`#200` と同じ契約）。`now.checked_add(delay)` が `Duration::MAX` をあふれた場合は「発火しない」として索引に積まず panic しない（`#181`・`#200` と同じ）。
- **繰り返しタイマの間隔は 0 より大きいこと（中2）**：原典 `LightArrayRevolverScheduler.scala:242-246` の `checkPeriod` は、`delay.length <= 0` の繰り返し（`scheduleWithFixedDelay`・`scheduleAtFixedRate`。呼び出しは `:129, :145`）を `IllegalArgumentException`（「creating an infinite loop」）で拒む。1 回きりの 0 は即時に実行し（`:223-227`）、遅延は tick（既定 10ms）に切り上げる（`:93-97`）。
  - 移植では、`FsmView::start_timer_with_fixed_delay`／`start_timer_at_fixed_rate` に `Duration::ZERO` を渡したら **panic する**。状態関数の中の失敗になるので、原典の例外と同じく監督へ渡る（原典の例外は `startTimer` の中で投げられ、状態関数の失敗として監督に届く）。文面は `"FSM timer {name}: a repeating timer needs a delay greater than zero (Pekko rejects it as creating an infinite loop)"` とする。`start_single_timer` の `Duration::ZERO` は原典どおり即時発火である（次の発火の入口で撃つ）。
  - **発火のループは `now` を 1 回だけ読む**。`fire_due_timers` は入口で `now` を 1 回読み、`deadline <= now` の行だけを `(期限, seq)` の順に撃つ。撃った行を「今の `now` で」再登録しても、その回のループでは見ない（`#200` の `fire_due_receive_timeouts` は毎回 `now_opt()` を読み直す作りだが、繰り返さない受信タイムアウトには足りるだけで、繰り返す FSM タイマにそのまま使うと、間隔 0 でループが終わらない。手動の時計でも、実時間の 1ns のような短い間隔でも同じである）。再登録した行は次の発火の入口で見る。
- `await_envelope` の待ち上限は `min(earliest_receive_timeout_deadline, earliest_fsm_timer_deadline)` で切る。期限検査（`fire_due_timers`）は `#200` と同じ 4 入口（`run_one`/`run_until_idle` 先頭、`Run` 後、`await_envelope` 待ち前後）で実行し、期限が来た行を `(期限, seq)` の順に消費して `FsmTimerMark` を対象のメールボックスへ送る。
  - `State`／`Single`：発火時に**`Executor` 側の索引**（と `fsm_timers` の行）から消費する。FSM 側のライブ表 `timers` からは、原典どおり**印を受け取ったときに初めて** `Single` を消す（`FSM.scala:823-831`。それまで `is_timer_active` は真）。
  - `FixedDelay`：発火後に `now.checked_add(delay)` で次期限を索引へ再登録する（原典 `FixedDelayMode`）。
  - `FixedRate`：発火後に `deadline.checked_add(delay)` で次期限を索引へ再登録する（原典 `FixedRateMode`）。**時計を大きく進めると、間に挟まる回数だけ撃つ（追いつく）**。これは原典の `FixedRateMode`（`getAndAdd` で期限を積む、`LightArrayRevolverScheduler.scala` の `schedule` の `getAndAdd`）と同じ振る舞いなので残す。ただし 1 回の発火の入口で撃つ回数は「`now` 以下の期限の数」で決まり、`now` は読み直さない。
  - **自動で受け取るメッセージをタイマで送る場合（低7）**：原典 `FSM.scala:141-145` は、タイマのメッセージが `AutoReceivedMessage`（`PoisonPill`・`Kill` など）のときは `Timer` で包まずにそのまま送る。移植では `Executor::invoke`（`executor_value.rs:2408-2470`）が `"PoisonPill"`／`"Kill"` を自動受信として扱うので、`FsmTimerMark::Named` で包むのは**それ以外のメッセージだけ**にする。`"PoisonPill"`／`"Kill"` をタイマに張った場合は、印を包まずにそのまま送る（原典どおり、`PoisonPill` は止まり、`Kill` は失敗になる）。このとき、FSM のライブ表から `Single` を消すのは送った時点ではなく「印を受け取る」という原典の道が無くなるので、送った時点で消す（`AutoReceivedMessage` は FSM の状態関数に届かないので、原典の「受信時に消す」と観測上同じ）。このテストはケースに無いが、`Message` を `&'static str` で作れるので写せる。
- **単一ドライバ化（Cursor Bugbot「Dual timer drivers would double-fire」への対応）**：`TestFsmRef`（`crates/fraktor-testkit/src/test_fsm_ref/test_fsm_ref_value.rs`）の `drive_timers` スレッドを廃止する。`FsmControl::SetState { timeout: Some(d) }` と `FsmControl::StartTimer` も `Fsm::handle_control` から同じ `Executor` 期限索引へ登録する形に統一し、全 FSM（通常機械・`TestFsmRef` 機械）を単一の `Executor` 期限索引が駆動する。`FsmHandle` の既存読み取りメソッド（`is_timer_active`, `is_state_timer_active` 等）は互換のため維持し同期し続ける。`TestFsmRef` の `stop_driver` フィールドも無くなる（低6）。
- **`#200` の条件（`issue-200-receive-timeout.md:364` の §13-3）と食い違わないこと（低8）**：
  - (b)「汎用のタイマの表は作らないこと」：FSM の索引は `FsmTimerSlot`（`State` と `Named(String)`）と `FsmTimerMark` しか持たない FSM 専用の表であり、ほかの用途の予約を載せない。受信タイムアウトの索引（`receive_timeout_index`）はそのまま別に残す。`Scheduler` の port は作らない。
  - (c)「段 5 で `Scheduler` の port へ作り直し、段 5 の複数スレッドの dispatcher では未対応である制約を書く」：**段 5 のプールでは、アイドルの FSM のタイマが撃たれない**（`#200` と同じく、誰も `run_one`／`await_envelope` を回さないアクターの期限は検査されない）。この制約を `#257` だけでなく `#256`（段 5）へも転記する（§8）。
- **`#234` 段 5・段 6（[Issue #257](https://github.com/j5ik2o/fraktor-ng-rs/issues/257)）への引き継ぎ**：`#200` の転記（`#257` コメント）と同じく、この期限索引は段 3〜4 の `ManualExecutor` に限る。段 5〜6 で複数スレッド dispatcher と汎用 `Scheduler` port が入るときに、`ReceiveTimeout` と FSM タイマをまとめて `Scheduler` port へ差し替える。

#### (d) FSM 関連の公開 API（`fraktor-actor`）

```rust
// crates/fraktor-actor/src/kernel/actor/fsm_timeout.rs（新規、kernel::actor から pub use）
/// 遷移ごとの状態タイムアウト上書き。無限は専用の値で表す（対応表 Duration.Inf の行）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FsmTimeout {
  After(Duration),
  Infinite,
}

// crates/fraktor-actor/src/kernel/actor/fsm.rs への追加（既存メソッドは変更なし）
impl<S, D> Fsm<S, D>
where
  S: Clone + PartialEq + Send + Sync + 'static,
  D: Send + 'static,
{
  /// 原典 `when(stateName, stateTimeout = ...)`。
  /// 同一状態への複数回登録は状態関数を `orElse` 合成し、タイムアウトは `timeout.or(existing)`。
  pub fn when_with_state_timeout<F>(&mut self, state: S, timeout: Option<Duration>, function: F)
  where
    F: FnMut(&mut ActorContext<'_>, &FsmView<S>, &Message, &D) -> Option<FsmState<S, D>> + Send + 'static;

  /// 原典 `onTransition` のうち、`FsmView` 経由でタイマ操作を行う形。
  /// 既存の `on_transition`／`on_transition_with_data` と同じリストへ追加され登録順に走る。
  pub fn on_transition_with_view<F>(&mut self, handler: F)
  where
    F: FnMut(&mut ActorContext<'_>, &FsmView<S>, &S, &S) + Send + 'static;

  /// 未処理イベントの警告で状態名 `S` を `Display` 表示するための登録。
  /// `Fsm<S, D>` 自体に `Display` 境界を足さないことで既存の `TestFsmRef<S, D>` 等を壊さない。
  pub fn with_state_display(&mut self)
  where
    S: core::fmt::Display;
}

// crates/fraktor-actor/src/kernel/actor/fsm_state.rs への追加
impl<S, D> FsmState<S, D> {
  /// 原典 `State.forMax(timeout)`。
  pub fn for_max(self, timeout: FsmTimeout) -> Self;

  /// 原典の状態関数内からの `whenUnhandled { ... }` 差し替え（§3.2、M2）。
  pub fn with_unhandled<F>(self, function: F) -> Self
  where
    F: FnMut(&mut ActorContext<'_>, &FsmView<S>, &Message, &D) -> Option<FsmState<S, D>> + Send + 'static;

  /// 原典の `whenUnhandled(NullFunction)`。既定（警告＋`stay()`）へ戻す（§3.2、M2）。
  pub fn clear_unhandled(self) -> Self;
}

// crates/fraktor-actor/src/kernel/actor/fsm_view.rs への追加
impl<S> FsmView<S> {
  /// 原典 `startSingleTimer`。即座にライブ表へ反映し、同一ハンドラ内の `is_timer_active` から見える。
  pub fn start_single_timer(&self, name: &str, message: Message, delay: Duration);
  /// 原典 `startTimerWithFixedDelay`。
  pub fn start_timer_with_fixed_delay(&self, name: &str, message: Message, delay: Duration);
  /// 原典 `startTimerAtFixedRate`。
  pub fn start_timer_at_fixed_rate(&self, name: &str, message: Message, interval: Duration);
  /// 原典 `cancelTimer`。無い名前は何もしない。即座にライブ表へ反映する。
  pub fn cancel_timer(&self, name: &str);
  /// 原典 `isTimerActive`。ライブ表を見て即座に答える。
  pub fn is_timer_active(&self, name: &str) -> bool;
}
```

no_std の観点：追加する型・表・`FsmTimerMark`・`Executor` 期限索引・`portable_atomic::AtomicU64` はすべて `core` と `alloc` と既存依存（`spin`, `portable-atomic`）だけで書ける。kernel に `std` を一切持ち込まない。

### 3.2 未処理イベントの警告・`Message` と状態 `S` の文字列化・`whenUnhandled` の差し替え

#### (a) `Message` と状態 `S` の文字列化（レビュー H2）

- 現行の `Message::new<M: Any + Send + Sync>` と `impl fmt::Debug for Message`（`&'static str` と `i32` 以外は `"<message of an unknown type>"`、`unhandled_notification_spec.rs:132` が検証済み）は**そのまま維持する**。
- 「`Payload` に既定の実装つきの `fmt_display` を足し、`DisplayPayload<M>` だけ上書きする」形は書けない。今の `Payload` は `impl<T: Any + Send + Sync> Payload for T` の包括の実装（`message_value.rs:43-56`）を持ち、`DisplayPayload<M>` もその `T` に入る。2 つ目の実装は衝突する（stable の Rust に specialization は無い）。そこで包括の実装をやめ、`Payload` を 2 つの非公開の包みにだけ実装する：
  - `struct PlainPayload<M>(M)`：`Message::new` が使う。`as_any()` は `&self.0`、`type_name()` は `type_name::<M>()`、`fmt_display()` は `None`。
  - `struct DisplayPayload<M: fmt::Display>(M)`：`Message::with_display` が使う。`as_any()` と `type_name()` は `PlainPayload` と同じ。`fmt_display()` は `Some(fmt::Display::fmt(&self.0, f))`。
  - どちらも `as_any()` が中の値を返すので、`downcast_ref::<M>()`・`is::<M>()`・`payload_type_id()`・`payload_type_name()` は今と同じに動く。`Message::new` の二重の包み防止（`message_value.rs:82`、`Message` を受けたらそのまま返す）は `PlainPayload(m).as_any()` で同じく判定できる。包括の実装が無くなるので、`message_value.rs:38-42` の doc にある「`Arc<dyn Payload>` に直接メソッドを呼ぶと包括の実装に解決される」という落とし穴も消える。
- `Message` に次の公開メソッドを足す：
  ```rust
  impl Message {
    /// `fmt::Display` 表示を保持した `Message` を作る。
    /// `ActorRef::tell(Message::with_display(v), sender)` は既存の `Message::new` の
    /// 二重ラップ防止（message_value.rs:82-83）によりそのまま運ばれる。
    pub fn with_display<M: Any + Send + Sync + core::fmt::Display>(payload: M) -> Self;

    /// Pekko の `value.toString` 相当の文字列表現を返す。
    /// `with_display` で作られた値はその `Display`、既定の `&'static str`/`String`/整数/`bool` は
    /// 引用符なしの `Display`、それ以外は `payload_type_name()` を返す。
    pub fn display_text(&self) -> alloc::string::String;
  }
  ```
- 状態名 `S` の表示は `Fsm::with_state_display(&mut self) where S: fmt::Display` で設定する（未設定時は `core::any::type_name::<S>()` にフォールバック）。これにより `Fsm<S, D>` の型境界を変えず、`TestFsmRef<S, D>` を含む既存の全ジェネリック呼び出しを壊さない。
- Scala の `case class Unhandled(msg: AnyRef)` の `toString` は引用符なしの `Unhandled(test)` である（Rust の `#[derive(Debug)]` だと `Unhandled("test")` になり一致しない）。テスト側の `Tick` と `Unhandled(&'static str)` に `fmt::Display` を実装して `"Tick"`・`"Unhandled(test)"` を出力し、`Message::with_display(...)` で送ることで、原典 `FSMTimingSpec.scala:144-148` の文面 **`"unhandled event Tick in state TestUnhandled"`** と **`"unhandled event Unhandled(test) in state TestUnhandled"`** に文字単位で完全一致させる。

#### (b) 警告の発行経路

- `ActorContext` に `pub(crate) fn publish_warning(&self, message: &str)` を足し、`Executor::publish_warning(&self, source: &ActorRef, message: &str)` へ委譲する。
- `Executor::publish_warning` は `Warning::new(source.path().to_string(), ACTOR_LOG_CLASS, message)` を作り、`event_stream`（あれば）と `event_sink`（あれば）へ発行する。これは原典 `FSM.scala:770` の `log.warning("unhandled event " + value + " in state " + stateName)` と 1 対 1 に対応し、`UnhandledMessage` を発行する `publish_unhandled` とは別経路である。

#### (c) `whenUnhandled` の動的差し替え（レビュー M2）

- `FsmView<S>` に型引数 `D` を足す案は既存 9 ファイル 27 箇所を壊すため**採らない**。
- すでに `S` と `D` の両方を型引数に持つ **`FsmState<S, D>`** に `with_unhandled(function)` と `clear_unhandled()` を載せる（§3.1 のシグネチャ）。既存テストの書き換えは 0 件である。原典（`FSM.scala:688-689`）は状態関数の中で `whenUnhandled` を呼んだその場で `handleEvent` を差し替える。port は、状態関数が返した `FsmState` を適用するときに `self.unhandled_function` を差し替える（`clear_unhandled` なら既定に戻す。原典の `whenUnhandled(NullFunction)` は `NullFunction.orElse(handleEventDefault)` なので既定と同じ）。差し替えた関数が効くのは次のイベントからなので、どちらでも観測できる違いは無い。

### 3.3 `ActorContext::child`

```rust
// crates/fraktor-actor/src/kernel/actor/actor_context.rs への追加
impl ActorContext<'_> {
  /// 原典 `ActorContext.child(name)`。自分の生きている子がいればその参照を返す。
  pub fn child(&self, name: &str) -> Option<ActorRef>;
}
```

`Executor` がすでに持つ `ParentEntry.children`（`executor_value.rs:255`）から `(self_ref, name)` で引くだけである。停止して `ChildTerminated` を処理済みの子は表から外れているため `None` になる。`core` と `alloc` で完結する。

### 3.4 `ActorInitializationException` へのパス同梱（レビュー H2）

原典で、パスがどこに入るかを先に確かめた。

- `ActorCell.scala:682-700`：creator か `preStart` が投げると、`ActorInitializationException(self, s"exception during creation, root cause message: [$rootCauseMessage]", x)` を投げる（根の原因に文面が無いときは `"No message available"`）。
- `Actor.scala:202-204`：`ActorInitializationException` の `getMessage` は `enrichedMessage` で `s"${actor.path}: $message"` になる。
- `FaultHandling.scala:404-422`：監督の `logFailure` は `Error(cause, child.path.toString, getClass, logMessage)` を出す。`ActorInitializationException` に原因があるとき、`logMessage` は**原因の `getMessage`**（`IllegalStateException("Ur state be b0rked")` なら `"Ur state be b0rked"`）であり、パスは入らない。
- `TestEventListener.scala:332-339`：`ErrorFilter` は `doMatch(src, msg) || doMatch(src, cause.getMessage)` で照合する。`ActorRefSpec.scala:278` の `pattern = "/user/failingActor:"` が当たるのは、ログの文面ではなく、原因（`ActorInitializationException`）の `getMessage` の方である。

したがって、パスはログの文面ではなく原因の文面に入れる。レビュー H2 の直し方の「`message` を `format!("{}: {}", path, message)` にする」は採らない。採るとログの文面が原典の `logMessage` と変わるからである。

- `crates/fraktor-actor/src/kernel/executor/executor_value.rs:1620` の `FailureKind::Initialization` を発行するとき（`failure_reason`、`executor_value.rs:2872-2891`）、`Error::with_cause_name` ではなく既存の `pub(crate) Error::with_cause_and_message`（`Logging/error.rs`）を使う：
  - `cause_name`：`"ActorInitializationException"`（今と同じ）
  - `cause_message`：`format!("{}: exception during creation, root cause message: [{}]", path, root)`。`root` は panic の文面で、空なら `"No message available"`
  - `log_source`：`path.to_string()`（今と同じ）
  - `message`：panic の文面（今と同じ。原典の `logMessage`）
- `Logging::LogEvent` に `fn cause_message(&self) -> Option<&str> { None }` を既定の実装つきで足し、`Error` は持っている `cause_message` を返す（`Error::cause_message` はすでに `pub` で、同じ値を返す）。既定の実装があるので、既存の `LogEvent` の実装は書き換えない。
- ログの文面（`message`）は変わらないので、既存のテストに影響は無い。`crates/` で初期化の失敗のログを見ているテスト（`executor_value_test.rs:548` など）は原因の名前と `log_source` しか見ておらず、どれも今のまま通る。

### 3.5 `fraktor-testkit` の `EventFilter` 拡張（既存の `pub` と重ねない、レビュー H2）

今の公開面（`crates/fraktor-testkit/src/event_filter/event_filter_value.rs`、`event_filter.rs`、`event_sink_registry.rs`）：

- `pub enum EventFilter { Error { cause_name, message }, Warning, Info { start } }`。作る口は `error()`、`warning()`、`info()`、`typed(&str)`。照合は `apply(&dyn LogEvent) -> bool`。絞り込みは `with_message(&str)`（`Error` にだけ効く）と `start(&str)`（`Info` にだけ効く）。検査は `intercept(&self, &mut ActorSystem, impl FnOnce() -> R) -> R`。`intercept` は一致がちょうど 1 件であることを固定で確かめる。
- `pub struct EventSinkRegistry`：`new()`、`arm(EventFilter) -> usize`、`disarm(usize)`、`matched(usize) -> usize`。

**今の不具合（前の担当の気づきを実物で確かめた）**：`EventFilter::Warning` には中身が無い。`with_message` は `if let Self::Error { .. } = self` でしか書き込まないので、`EventFilter::warning().with_message(...)` は引数を黙って捨て、どの文面の警告にも当たる。`crates/fraktor-actor/tests/local_actor_ref_provider_spec.rs:105` と `crates/fraktor-actor/tests/actor_configuration_verification_property_test.rs:143` は `with_message(&format!("unhandled message from {}", dead_letters.path()))` と書いている。ところが実際の文面は `"unhandled message from <path>: \"hello\""`（`executor_value.rs:3389`）である。この 2 本が通っているのは、引数が捨てられているからにすぎない。

原典の `EventFilter.warning(message, source, start, pattern, occurrences)`（`TestEventListener.scala:235-243`）と `doMatch`（`:150-157`）に合わせ、次のように直す。

```rust
// crates/fraktor-testkit/src/event_filter/event_filter_value.rs
// variant を外から見せる enum をやめ、中身を隠した struct にする（公開面を減らす。
// ワークスペースで variant を直接作る・照合する箇所は 0 件を確認済み）。
#[derive(Clone, Debug)]
pub struct EventFilter {
  level: LogLevel,                  // Error / Warning / Info
  cause_name: Option<String>,       // Error のみ。None は Error.NoCause（何の原因でもよい）
  source: Option<String>,           // 原典の source：log_source との完全一致
  text: EventFilterText,            // 下のどれか 1 つ（原典の Either[String, Regex] と complete）
  occurrences: usize,               // 既定 1（今の intercept と同じ）
}
// 非公開の enum：Any（制限なし）／Exact（原典 message、complete = true）／
// Start（原典 start、complete = false）／Contains（下の contains）

impl EventFilter {
  // 既存の口はシグネチャを変えない：error(), warning(), info(), typed(&str), apply(&dyn LogEvent) -> bool
  /// 既存。Error・Warning・Info のすべてで効くようにする（原典 message：完全一致）。
  pub fn with_message(self, message: &str) -> Self;
  /// 既存。Error・Warning・Info のすべてで効くようにする（原典 start：前方一致）。
  pub fn start(self, prefix: &str) -> Self;
  /// 新規。原典 source：`event.log_source() == source`。
  pub fn with_source(self, source: &str) -> Self;
  /// 新規。文面に `fragment` が含まれるかを見る（正規表現ではない。下の注）。
  pub fn contains(self, fragment: &str) -> Self;
  /// 新規。原典 occurrences。
  pub fn with_occurrences(self, occurrences: usize) -> Self;
}

// crates/fraktor-testkit/src/event_filter.rs
impl EventFilter {
  /// 既存。`assert_eq!(matched, self.occurrences)` に変える（既定 1 なので今の動きは変わらない）。
  /// `&mut ActorSystem` を取るので、`body` の中から `spec`（`FraktorSpec`）を通して
  /// `within`／`expect_msg`／`actor_of` を呼ぶことはできない。フィルタ 1 本のケースで使う。
  pub fn intercept<R>(&self, system: &mut ActorSystem, body: impl FnOnce() -> R) -> R;
}

// crates/fraktor-testkit/src/fraktor_spec.rs（新規の中1 の口。`FraktorSpec` 自身が `system_mut` を持つので借用が衝突しない）
impl FraktorSpec {
  /// 既存の `EventFilter::intercept` の `FraktorSpec` 版。ブロックの中で
  /// `self.within`／`self.test_kit_mut().expect_msg`／`self.system_mut().actor_of_named` を呼べる。
  pub fn intercept<R>(&mut self, filter: &EventFilter, body: impl FnOnce(&mut Self) -> R) -> R;
  /// 新規。原典 `filterEvents(filters: EventFilter*)(block)`（testkit `package.scala:25-48`）。
  /// implicit の system の上でブロックを回す原典に合わせ、`&mut FraktorSpec` を受ける。
  /// すべてのフィルタを同時に張り、`body` → `run_until_idle()` のあと、
  /// どのフィルタも一致の数がそれぞれの occurrences と等しいことを確かめて、すべて外す。
  pub fn filter_events<R>(&mut self, filters: &[EventFilter], body: impl FnOnce(&mut Self) -> R) -> R;
}
```

- **なぜ `EventFilter` の関連関数にしないか（中1）**：原典は `filterEvents(...) { fsm ! TestUnhandled; within(3.second) { ...; expectMsg(Unhandled(Tick)) ... } }` と、ブロックの中で `within` と `expectMsg` を呼ぶ（`FSMTimingSpec.scala:143-159`）。ActorRefSpec のケース 1・11 もブロックの中で `actorOf` を呼ぶ（`ActorRefSpec.scala:156-283`、`:443`）。実物の `EventFilter::intercept` は `&mut ActorSystem` を取り、`ActorSystem` は `Clone` を持たない（`actor_system_value.rs:89-95`）ので、ブロックの中で `spec.within`／`spec.test_kit_mut()`／`spec.system_mut()` を呼ぶと借用が衝突する。既存のテストはこれを避けるため `registry.arm`／`matched`／`disarm` を手で書いている（`supervisor_tree_spec.rs:101-127`、`ticket669_spec.rs:126-127`、`actor_timeout_spec.rs:72`）。だから、ブロックの中で `spec` を使うケース（FSMTimingSpec ケース 10、ActorRefSpec ケース 1・2・11）は `FraktorSpec` の口を使う。ブロックの中で `spec` を使わないケースは既存の `EventFilter::intercept` で足りる。どちらを使うかは §1.2・§1.3・§1.3.1 の各ケースに書いた。`run_until_idle` のあとで `occurrences` と照合する手順は今の設計のままである。

- **照合の規則**（原典 `doMatch`）：`level` が合うこと、`source` があれば `log_source` と完全一致すること、`text` が `Exact`／`Start`／`Contains` ならそれに合うこと。`Error` はさらに `cause_name` が合うこと。`Error` の文面は、原典 `ErrorFilter.matches`（`TestEventListener.scala:332-339`）と同じく `message()` と `cause_message()`（§3.4）の**どちらかが合えば**当たる。`with_message`・`start`・`contains` は最後に呼んだもの 1 つが効く（原典も `message`／`pattern`／`start` の 1 つしか持たない）。
- **`pattern` の代わりに `contains` にする理由**：原典の `pattern` は正規表現で、`findFirstIn`（部分一致）で照合する。このテストで要るのは `"/user/failingActor:"` だけで、正規表現の特殊文字を含まない。部分一致の結果は原典と同じになる。正規表現を入れるには、外部の crate（`regex`）を testkit に足す判断が要る。これはこの issue の範囲ではない。正規表現でないものを `with_pattern` と呼ぶと、原典の `pattern` と同じだと読み違えられる。そこで、していることのとおり `contains` と名付ける。正規表現の要る移植（例：`ActorTimeoutSpec.scala:30` の `".*unhandled message from.*hallo"`）が来たときに、別の issue で `pattern` を足す。
- **`occurrences` の既定**：原典の既定は `Int.MaxValue`（数を確かめない）である。今の port の `intercept` は 1 件を固定で確かめていて、既存のテストはそれに頼っている。既定は 1 のままにする。原典で `occurrences` を書いているケース（このテストではすべて）は `with_occurrences` で同じ数を書く。
- **一致の数え方**：原典の `TestEventListener.filter`（`TestEventListener.scala:593-597`）は、張った順の新しいものから `exists` で見る。1 つの事象は最初に当たった 1 本のフィルタにしか数えない。今の `EventSinkRegistry::publish` は、当たったフィルタのすべてに数える。この issue で張るフィルタは重ならない（ケース 10 の 2 本は完全一致の文面が違う。ケース 1・2・11 は 1 本だけ）ので、どちらの数え方でも結果は原典と同じになる。数え方を変えると、張ったままにしている `actor_timeout_spec.rs:72` の `EventFilter::warning()` と後から張るフィルタの関係が変わる。そのため、この issue では変えない。別件として上げる（§8）。
- **`EventSinkRegistry` の中の直し**：`arm` が返す値を、`Vec` の添字から単調に増える ID に変える。`filter_events` で複数を張って外すときや、`intercept` を入れ子にしたときに、`Vec::remove` で添字がずれないようにする。公開のシグネチャは変えない。
- **既存のテスト 2 本（低11）**：`local_actor_ref_provider_spec.rs:105` と `actor_configuration_verification_property_test.rs:143` は、今は `EventFilter::warning().with_message(&format!("unhandled message from {}", dead_letters.path()))` と書き、引数が捨てられていたので通っている。**第 2 段では、いま出る文面の完全一致に書き直しはしない。** そうすると原典と違う今の文面の形（`Actor[...]` の包みと、`"hello"` の引用符）をテストに固めてしまう。次のどちらかにする。
  1. 今は `start(&format!("unhandled message from {}", dead_letters.path()))` に留める（原典の `EventFilter.warning(start = ...)` の形でもあり、元のテストの意図「文面の先頭」を守る）。
  2. この issue で足す `display_text`（§3.2(a)）を使い、`publish_unhandled` の文面を原典の形（`"unhandled message from Actor[{path}]: {msg}"`、`msg` は引用符なし）に直してから完全一致にする。原典 `LocalActorRefProviderSpec.scala:76, 89, 102` の文面は `s"unhandled message from Actor[$deadLettersPath]: $message"`、`TestEventListener.scala:583` も同じ形である。
  - 第 2 段の担当は **1 を既定**とし、2 は実装の担当と相談して文面を直す場合だけ選ぶ。`publish_unhandled` の文面そのものの修正は #198 の前からある別件（§8）であり、この issue の範囲を広げない。テストは変えても「先頭一致」で通り続ける。

## 4. 守る性質（不変条件）

| 番号 | 不変条件 |
|:---:|:---|
| I1 | 状態タイマの選択と張り直し（`FSM.scala:511-513, 755-763, 768-772, 817-914`）：通常メッセージと受理された名前付きタイマ印は、処理前に状態タイマを取り消し `state_generation` を 1 進める。遷移が成立したら、`for_max(Infinite)` なら張らず、`for_max(After(d))` なら `d` で張り、上書き無し（`goto`, `stay()`, 未処理既定の `stay()`）なら遷移先状態の既定タイムアウトがあれば張る（同一状態への `goto` も入り直しとして張り直す）。同一状態への複数回 `when_with_state_timeout` は `timeout.or(existing)` で合成する。`StateTimeout` 印の受信は `state_generation` を進めず、遷移先が新たなタイムアウトを張らない限り `is_state_timer_active` を維持する。購読・購読解除・`ListenerTerminated` は状態タイマを取り消さず世代も進めない |
| I2 | 世代分離と再起動 `instance_id` 分離：`state_generation` と `named_timer_gen` は別カウンタであり、名前付きタイマの開始・取り消しは状態タイマを無効化しない（[PR #275](https://github.com/j5ik2o/fraktor-ng-rs/pull/275)）。また全印は `Fsm` 個体ごとの一意な `instance_id` を運び、再起動で新個体のローカル世代が 0 から始まっても旧個体の印は決して受理されない |
| I3 | 名前付きタイマの即時性と一回性：状態関数・`on_transition_with_view` 内の開始・取り消しは即座にライブ表へ反映され、同一ハンドラ内の `is_timer_active` が最新状態を返す。同名の開始は置き換えであり旧印は破棄される。`Single` は高々 1 回だけ発火して表から消え、`FixedDelay`／`FixedRate` は取り消されるまで続く。無い名前の取り消しは何もしない。存在しない状態への `goto` は、状態を変えず、名前付きタイマをすべて取り消す（`FSM.scala:886-888, 933-939`） |
| I4 | 停止の後始末：アクターが止まると（正常停止・失敗停止・システム終了を問わず）、名前付きタイマ・状態タイマの全期限が `Executor` 期限索引から即時削除され、**停止の後に新しい期限が撃たれない**（ケース 3。原典 `FSM.scala:924-931, 933-946` の `terminate`）。停止の前にメールボックスへ入った印は、原典と同じく（止めたアクターの郵便受けに届くので）`DeadLetter` として流れることがあり、これは I4 の破れではない。また全 FSM タイマは `Executor` 単一ドライバのみで駆動され、二重発火しない |
| I5 | 未処理イベントの警告とハンドラ差し替え：状態関数もカスタム `whenUnhandled` も `None` を返したイベントは、`log_source = self.path()` で `"unhandled event {event} in state {state}"` の `Warning` を 1 回だけ発行し、`FsmState::stay()` として `make_transition` を通る（状態・データは不変、既定 `stateTimeout` があれば再張り直し）。`FsmState::with_unhandled`／`clear_unhandled` の変更は次のイベントから効く |
| I6 | 子の参照：`ActorContext::child(name)` が `Some(ref)` を返すのはその名前の生存中の子がいるときだけであり、返る参照は生成時の参照と等しい。停止完了後は `None` |
| I7 | 非同期生成の失敗の後始末：factory または `pre_start` の panic は `Create` で捕捉され、`ActorInitializationException` のエラーログ（文面は panic の文面、原因の文面は `"{path}: exception during creation, root cause message: [{reason}]"`）が出て監督が `Stop` する。停止完了（`run_until_idle()`）後に名前が解放されて再利用でき、系は健全なまま後続の生成を受け付ける |
| I8 | `ActorRef` 等価の不変：本 issue は `ActorRef` の等価性 `(id, generation)` を変えない。ケース 5 で未存在パス解決結果の 4 性質を検証し `==` 比較を行わないのはこの帰結である |

## 5. PBT（決定論的実行）

外部クレートは足さず、既存の移植（`receive_timeout_property_test.rs` 等）と同じ固定 seed の SplitMix64 で書く。原典外であり台帳の件数には含めない。置き場所は `crates/fraktor-actor/tests/fsm_timing_property_test.rs` と `crates/fraktor-actor/tests/actor_ref_property_test.rs` とする。

| 番号 | 性質と決定論的実行の仕組み | 確かめる不変条件 |
|:---:|:---|:---|
| P1 | 名前付きタイマとキュー滞留印のモデル照合（`fsm_timing_property_test.rs`）：案 C＋`ActorSystem::set_clock`（手動時計）を使い、`receive_timeout_property_test.rs` と同じく決定論的にランダム操作列（開始・置き換え・取り消し・`is_timer_active` 同一ハンドラ内検査・時計進め・通常メッセージ送信・再起動）を流し、Pekko `FSM.scala` の参照モデルと発火イベント列を完全一致比較する。**参照モデルも本実装と同じ規則で発火列を作る**：同じ期限の行は予約した順（`seq`）に撃ち、FSM の索引と受信タイムアウトの索引も `(期限, seq)` の順に合わせて撃つ（§3.1(c)、中3）。**ケース 7 の裏付けとして**「タイマ開始 → `run_until_idle()` → `Cancel` メッセージ送信（未実行） → 時計を期限まで進める → `run_until_idle()`（先頭で期限発火がメールボックス末尾へ印を積み、`Cancel` が先に実行された直後に滞留していた旧印が捨てられる）」の確定的シーケンスと、再起動後に旧 `instance_id` の滞留印が捨てられるシーケンスを明示的に含める。**これはケース 7 の移植の代わりに数えない**（§1.4） | I2、I3、I4 |
| P2 | 状態タイムアウト選択のモデル照合（`fsm_timing_property_test.rs`）：`ActorSystem::set_clock`（手動時計）の下で、状態の既定タイムアウト（なし／有限／複数回 `when_with_state_timeout` 合成）× `for_max` 上書き（なし／`After`／`Infinite`）× 遷移種別（`goto`／同一状態 `goto`／存在しない状態への `goto`／`stay()`／未処理イベント／`StateTimeout` 受信／名前付きタイマ受信）のランダム列を流し、発火時刻・発火回数・**同じ期限の行の撃つ順（予約した順、`seq`）**・古い印の破棄が Pekko 参照モデルと完全一致することを検証する。P1 と同じく、参照モデルは §3.1(c) の `(期限, seq)` の規則を使う。**ケース 8 の移植の代わりには数えない**（§1.4） | I1、I2、I5 |
| P3 | 初期化失敗の後始末と `child` 一貫性（`actor_ref_property_test.rs`）：factory／`pre_start` で失敗する名前付き生成と成功する名前付き生成・停止のランダムな列について、各生成後に `system.run_until_idle()` で非同期 `Create` → 監督 `Stop` → 終了完了まで進めた後、失敗した名前は必ず再利用でき、成功した生存中の子はすべて相異なる参照を持ち、`context.child(name)` の結果が参照モデル（生存集合）と常に一致することを検証する | I6、I7 |

## 6. 状態遷移と Quint（1 ファイル仕様）

**書く（本設計と同時に作成・検査済み）。** 既存の `crates/fraktor-actor/specs/test_fsm_ref.qnt`（#181）は `TestFsmRef` の外部 `set_state` とハンドル中心であり、通常の `Fsm` の状態タイムアウト選択（既定値・`forMax(Infinite)`・`forMax(After)`・`stay()`・未処理イベント・`StateTimeout` 受信時の再張り直し）と再起動時の `instance_id` 分離を持たない。

`scripts/quint-check-specs.sh` の規約（`crates/*/specs/*.qnt` の各ファイルに `init`/`step`/`val invariant`・`*Test` で終わる `run` が 1 本以上・`quint run --invariant=invariant` で `max > 1`）に従い、本体・不変条件・正のテスト・負の対照を **`crates/fraktor-actor/specs/fsm_timing.qnt` の 1 ファイル** にまとめる（レビュー M3）。

- **状態**：`alive`, `curInst`/`nextInst`（`instance_id`）, `state`（既定タイムアウト無しの `SNoTimeout`、既定タイムアウトありの `SWithTimeoutA`/`SWithTimeoutB`、存在しない `SNonExistent`）, `stateGen`, `namedGen`, 状態タイマ枠（`stActive`, `stGen`, `stSid`, `stIssued`）, 名前付きタイマ表 `timers`, 滞留印集合 `namedMarks`/`stateMarks`, 不変条件用ゴースト変数（`liveSid`, `expectedArmed`, `lastOverride`, `deadEids`, `staleStateAccepted`, `staleNamedAccepted`, `acceptedSingle`）。
- **遷移**：`receiveOrdinary(kind, target, ov)`（`Goto`/`Stay`/`Unhandled` × `NoOverride`/`OverrideAfter`/`OverrideInfinite`）, `startNamedTimer`, `cancelNamedTimer`, `namedTimerDue`, `stateTimerDue`, `receiveNamedMark`, `receiveStateMark`, `restartActor`（ローカル世代を 0 へ戻しつつ `curInst` を進める）, `stopActor`。
- **不変条件（`val invariant`）**：`armedMatchesSelection`（I1）, `infiniteNeverArms`（I1）, `staleStateNeverFires`（I1/I2/M1）, `staleNamedNeverFires`（I2/I3/M1）, `stoppedNeverArms`（I4）, `singleLeavesTable`（I3）, `singleFiresOnce`（I3）。
  - **`armedMatchesSelection` の作り直し（レビュー低1）**：前の版は `(liveSid != -1) == expectedArmed` で、`expectedArmed` は `liveSid` と同じ `selectionShouldArm` の呼び出しから作っていた。同じものを自分と比べていたので、`NoOverride => false` と「未処理のときだけ張らない」の 2 つの変異が `quint run` で違反にならなかった。第 2 段では、モデルの変数（`stActive`／`lastTarget`／`lastOverride`）から選択を**書き下し直して**比べる（`selectionShouldArm` は使わない。書き換えると両辺が一緒に動く）。`lastTarget` は直前の遷移の遷移先を残すゴースト変数で、`keepStateMachine` などで更新する。`keptAfterMark` は「`StateTimeout` の印を受けて張り直さなかったので枠を残している」（`FSM.scala:818-821`）を表す。存在しない状態への `goto` は張らない（`FSM.scala:886-888`）ので対象外にする。
  - **`singleLeavesTable` の作り直し（レビュー低1）**：前の版は `acceptedSingle` を受理の直後に `NoName` に戻すため、作りから見て必ず真だった。`acceptedSingle` を「最後に受理した `Single` の名前」として**次の通常メッセージ・開始・取り消し・遷移まで残し**、`singleLeavesTable` はその名前が表に無いことを見る（`Single` の印を受理したら表から消す、`FSM.scala:829-831`）。**I3 の「`Single` は高々 1 回」** は、受理した `Single` の名前を集めた `acceptedSingles: Set[str]` の大きさが `NamedNames` の数を超えないこと（`singleFiresOnce`）で見る。この 2 つで、`Single` の印を受理したあとも表から消さない変異（`timers' = ... else timers`）が `quint run` で違反になることを確かめた。
- **正のテスト（11 本）**：`defaultTimeoutOnEnterAndStayTest`, `unhandledRearmsDefaultTimeoutTest`, `forMaxInfiniteDisablesDefaultTimeoutTest`, `forMaxAfterOverridesNoDefaultTest`, `sameStateGotoRearmsTimeoutTest`, `stateTimeoutRearmsWhenTargetHasDefaultTest`, `namedTimerDoesNotInvalidateStateTimerTest`, `namedTimerMarkCancelsStateTimerTest`, `restartInstanceIdDropsOldMarksTest`, `stopClearsAllTimersTest`, `gotoNonExistentClearsTimersTest`（存在しない状態への `goto` は状態を変えず、名前付きタイマを空にし、そのあと届いた古い印を捨てる。`FSM.scala:886-888, 933-939`）。
- **検査の結果**：`bash scripts/quint-check-specs.sh` が全 spec で通る（`fsm_timing.qnt` は `quint test` 19 本、`quint run --invariant=invariant --max-steps=40 --max-samples=30000` で違反なし、trace の長さの最大 41）。
- **負の対照（8 本、すべて狙った不変条件の破れを確認）**：
  1. `bugForMaxIgnoredTest`（`forMax(Infinite)` を無視して既定タイムアウトを張る）
  2. `bugStayDropsDefaultTimeoutTest`（`stay()` で既定タイムアウトを張り直さない）
  3. `bugUnhandledDoesNotRearmTest`（未処理イベントで `makeTransition(stay())` を通らず既定タイムアウトを落とす）
  4. `bugSameStateGotoDoesNotRearmTest`（同一状態 `goto` で張り直さない）
  5. `bugStayKeepsOldTimerTest`（`stay()` で旧状態タイマをキャンセルせず旧印が通る）
  6. `bugRestartReusesInstanceIdTest`（再起動時に `instance_id` を変えずローカル世代 0 始まりで旧印が誤一致する）
  7. `bugNoOverrideNeverArmsTest`（レビュー低1 の変異 1：`NoOverride` で何も張らない。`quint run` でも違反になることを確かめた）
  8. `bugUnhandledOnlyNeverArmsTest`（レビュー低1 の変異 2：未処理のときだけ張らない。`quint run` でも違反になることを確かめた）
  - 旧版でこの 2 つの変異を入れて `quint run --invariant=invariant` を流すと、どちらも違反なしで通ることを確かめた（直っていることの裏付け）。
- **残る弱点（今回は直さない）**：`namedMarks` は `Set` なので、同じ名前の繰り返しタイマの印が重なるとまとまる。`stopActor` の後に trace が終わるので、trace の長さの平均は 6 程度と浅い。どちらも今回の変異を捕まえることとは別で、直すとモデルの形が大きく変わる。**第 2 段の Quint の見直しでは、`namedMarks` を多重集合（`Set` を `List` にする）へ変えるかと、止めた後の `restart` を許すかを、実装の担当と決める。**
- **モデルにしないもの**：実時間そのもの（期限到来は非決定選択）、リスナー通知順（`fsm_transition.qnt` が担当）、警告文字列の内容。

## 7. 壊しうる入力

| 入力 | 何が起きうるか | 設計での手当て・確かめ先 |
|:---|:---|:---|
| 同一ハンドラ内で `start_single_timer("x", ...)` の直後に `is_timer_active("x")` や `cancel_timer("x")` | 遅延コマンド表だと `false` を返したり順序が崩れる | `FsmView` がライブ表を即時更新（§3.1(a)、P1） |
| `on_transition_with_view` からタイマを開始 | 従来の `on_transition` には `&FsmView` が無い | `on_transition_with_view` を新設し、遷移ハンドラ終了直後に `pending_ops` を `Executor` へ反映（ケース 6） |
| `initialize()` を呼ばない `Fsm` が `when_with_state_timeout` で `stay()` しつつ `context.stop(self)` | `stay()` で状態タイマが張られ、停止時に消し忘れると `DeadLetter` になる | `stay()` は `make_transition` で既定タイムアウトを張り、`post_stop_with_context`＋`Executor::clear_fsm_timers` が即時削除（ケース 3、I4） |
| 既定 `stateTimeout` ありの状態で未処理イベントを受信 | 現行 `fsm.rs:587-590` のままでは状態タイマが消えて戻らない | 未処理時は警告後に `FsmState::stay()` で `make_transition` を通す（I1、Quint `bugUnhandledDoesNotRearmTest`、P2） |
| 通常 `Fsm` のタイマ印がメールボックスに残ったまま再起動 | 新個体の世代が 0 から始まり旧印と世代 1 が誤一致する | `Fsm::new()` で一意な `instance_id` を採番し全印に載せる（I2、Quint `bugRestartReusesInstanceIdTest`、P1） |
| `TestFsmRef` の `drive_timers` と `Executor` 期限索引の両方が動く | 二重発火で `TestFSMRefSpec` や繰り返しタイマが壊れる | `TestFsmRef::drive_timers` を廃止し `Executor` 単一ドライバに統一（§3.1(c)） |
| タイマ名が空文字、ワイルドカード文字（`*`）、極端に長い文字列、非 ASCII | 名前の誤解釈や拒否が起きうる | 完全一致の `String` キーとしてそのまま扱う（P1） |
| 期限が `Duration::ZERO`／`Duration::MAX` 近傍、時計なしの境界 | 即時発火の無限ループ、加算あふれ panic、無音無視 | `checked_add` あふれは発火なし、時計なしは FSM 専用文面で panic。`Duration::ZERO` は次の 2 行に分ける（§3.1(c)、P1、P2） |
| **繰り返しタイマの間隔が 0、または 1 回の発火ループより短い**（`start_timer_with_fixed_delay(.., Duration::ZERO)`、`FsmControl::StartTimer { kind: FixedRate, delay: 1ns }`、手動の時計） | 次の期限が「今」以下になり、`fire_due_timers` のループが終わらず executor が止まり、メールボックスが際限なく伸びる | 原典 `LightArrayRevolverScheduler.scala:242-246` の `checkPeriod` どおり、間隔 `<= 0` の繰り返しは張る時点で panic する（状態関数の失敗として監督へ渡る）。発火のループは `now` を 1 回だけ読み、その `now` 以下の行だけを撃つ。`FixedRate` が追いつくために複数回撃つのは原典どおり（§3.1(c)、P1） |
| 1 回きりのタイマが `Duration::ZERO` | 即時発火 | 原典 `LightArrayRevolverScheduler.scala:223-227` どおり即時に実行する。処理中の再入は無く、1 発火＝1 メッセージ（§3.1(c)、P1、P2） |
| 同じ期限の FSM タイマ（名前付きと状態）を受信タイムアウトと同時に張る | 撃つ順が実装によって変わり、P1・P2 の参照モデルの完全一致が崩れる（ケース 8 の「古い印が捨てられる」道も通らない） | 索引のキーを `(期限, seq)` にし、受信タイムアウトの索引とも 1 つの `seq` で合わせて `(期限, seq)` の順に撃つ（§3.1(c)、P1、P2） |
| タイマのメッセージが `"PoisonPill"`／`"Kill"` | `FsmTimerMark` で包むと状態関数に届き、アクターが止まらない | 原典 `FSM.scala:141-145` どおり自動受信のメッセージは包まずに送る（§3.1(c)） |
| `Props::new` クロージャ内からの `system.actor_of` 呼び出し | `Executor::lend` の二重ロックで再入 panic | アクターの中での入れ子の生成は `pre_start` の `context.actor_of`／`actor_of_with_strategy` を使う（ケース 6・12）。ケース 1 は、テストの本体が先に内側を作り、参照を外側の factory に渡す（§1.3、§1.3.1） |
| 失敗する外側の子として失敗する内側を生成 | 外側が suspend → stop すると保留中の子失敗（`entry.held`）が破棄され `occurrences = 1` になる | 原典と同じく、外側も内側も `/user` の子としてテストの本体から作る（§1.3、§1.3.1） |
| `EventFilter::filter_events` で複数フィルタ同時登録・解除 | `EventSinkRegistry::disarm` の `Vec::remove` で添字がずれる | 安定 ID 方式へ変更する（§3.5） |
| `child` へ空文字・未存在名・停止済みの子名を渡す | 停止済みの子が残ると誤って `Some` を返す | `ParentEntry.children` は停止完了で削除済みのため `None`（ケース 12、P3） |

## 8. 影響

- **公開 API（`fraktor-actor`）**：
  - 追加：`FsmTimeout`, `Fsm::when_with_state_timeout`, `Fsm::on_transition_with_view`, `Fsm::with_state_display`, `FsmState::for_max`, `FsmState::with_unhandled`, `FsmState::clear_unhandled`, `FsmView` のタイマ 5 メソッド、`Message::with_display`, `Message::display_text`, `ActorContext::child`、`Logging::LogEvent::cause_message`（既定の実装つきのトレイトのメソッド）。
  - 既存シグネチャ・トレイト境界への破壊的変更：**0 件**（`FsmView<S>` の型引数変更なし、`Fsm<S, D>` の境界変更なし、`Message::new` と `Debug for Message` 変更なし、`LogEvent` の既存の実装は書き換え不要）。
- **公開 API（`fraktor-testkit`）**：
  - 追加：`EventFilter::with_source`, `contains`, `with_occurrences`、`FraktorSpec::filter_events`, `FraktorSpec::intercept`。既存の `with_message`・`start` は `Error`／`Warning`／`Info` のすべてで効くようにする。
  - 形の変更：`EventFilter` を、variant を見せる `pub enum` から中身を隠した `pub struct` に変える。ワークスペースで variant を直接使う箇所は 0 件。作る口と絞り込みの口のシグネチャは変えない。
  - `TestFsmRef` の公開 API は不変（`stop_driver` は `pub(crate)` の内部フィールドであり、公開 API ではない）。内部の `drive_timers` スレッドと `stop_driver` フィールドのみ廃止し `Executor` 期限索引へ統一。
- **既存テストへの影響**：
  - `crates/fraktor-actor/tests/unhandled_notification_spec.rs:132`：`Message::new` の `Debug` を変えないため影響なし。
  - `ActorInitializationException` のパス（§3.4）：パスは原因の文面に入れ、ログの文面は変えないので影響なし。
  - `crates/fraktor-actor/tests/local_actor_ref_provider_spec.rs:105` と `crates/fraktor-actor/tests/actor_configuration_verification_property_test.rs:143`：`EventFilter::warning().with_message("unhandled message from <path>")` は `Warning` が引数を捨てていたので通っていた。第 2 段では `start(...)`（前方一致）に留める。原典と違う今の文面を完全一致で固めない（§3.5、低11）。
  - **`TestFsmRef` の単体テスト 1 本（低6）**：`crates/fraktor-testkit/src/test_fsm_ref/test_fsm_ref_value_test.rs:41-55` の `the_timer_driver_ends_when_the_machine_stops_for_good` は `fsm.stop_driver` を読むので、`drive_timers` と `stop_driver` を廃止するとコンパイルできない。**実装の担当はテストを編集できないので、red を書く担当が次を行う**：
    1. このテストを消す（`drive_timers` が無くなれば確かめる対象が無い）。
    2. 代わりに「停止のあと、`Executor` の FSM 期限索引にそのアクターの行が残らない」を確かめるテストを書く（`system.stop(&target)` → `run_until_idle()` → 索引が空。`Executor` の索引を読む口は `pub(crate)` なので、`crates/fraktor-actor/src/kernel/executor/` の単体テストに置く）。
    3. `#181` 設計 line 219 の約束（`TestFsmRef::is_timer_active` が内側から張ったタイマも見える）が、`FsmHandle` の同期で保たれることを確かめるテストを足す。
- **別件として上げるもの（この issue では直さない）**：
  - `publish_unhandled` の文面（`executor_value.rs:3389`、`"unhandled message from {path}: {message:?}"`）が、原典 `TestEventListener.scala:583`（`s"unhandled message from $sender: $msg"`。`$sender` は `Actor[path]`、文字列は引用符なし）と違う。
  - `EventSinkRegistry::publish` は、1 つの事象を当たったフィルタのすべてに数える。原典は新しいフィルタから順に見て、最初の 1 本にだけ数える（§3.5）。
  - 正規表現の `pattern`（`regex` を testkit に足すかの判断が要る。§3.5）。
  - 存在しない状態への遷移で、原典はエラーログ `"Next state X does not exist"` を出すが、port は出していない（§3.1(a)）。
- **`test-support` の形**：発火印を `pub(crate) enum FsmTimerMark` に分離し、`FsmControl`・`FsmHandle` は引き続き `#[cfg(feature = "test-support")]` の下に閉じ込める。第 2 段では `cargo clippy -p fraktor-actor --lib -- -D warnings`（`test-support` 無効）で公開面漏れが無いことを検証する。
- **`#234` 段 5・段 6（[Issue #256](https://github.com/j5ik2o/fraktor-ng-rs/issues/256)・[Issue #257](https://github.com/j5ik2o/fraktor-ng-rs/issues/257)）への影響（低8）**：`Scheduler` port に衝突しない。段 5〜6 で `ReceiveTimeout` と一緒に `Executor` 期限索引から `Scheduler` port へ一括移行する旨を `#257` へ転記する。さらに `#200` の §13-3 の条件 (c) と同じ制約、**「段 5 の複数スレッドの dispatcher では、アイドルの FSM のタイマが撃たれない（誰も `run_one`／`await_envelope` を回さないアクターの期限は検査されない）」**を `#256` と `#257` の両方へ転記する（`#200` の受信タイムアウトと同じ性質）。
  - `#200` の条件 (b)「汎用のタイマの表は作らない」に反しない理由：FSM の索引は `FsmTimerSlot`（`State` と `Named(String)`）と `FsmTimerMark` しか持たない FSM 専用の表で、ほかの用途の予約を載せない。受信タイムアウトの索引はそのまま別に残す。`Scheduler` の port は作らない。
- **台帳**：第 2 段で更新する。
  - `FSMTimingSpec`：`total: 10, ported: 8, passing: 8, status: unported`（`verified_against` なし）。ケース 7・8 は `#257` の後に延ばし、`#257` が入ったら戻して数に入れる（決定：`pr288-decisions.md` の 5・6。後続 issue を作る）。
  - `ActorRefSpec`：ケース 3・4 は直列化の設計まで延ばす（「直列化が入ったら見直す」の但し書きつき）。ケース 5 はバイト列を延ばして 4 性質を今数に入れる。ケース 1 の小ケース 1 は「作らない」と確定した（分類「対象が Rust に無い（言語の違い）」、`cases.not_built`。`approved_by`・`approved_at` は元帥の項目 522、2026-11-27）。ケース 1 の小ケース 2〜13 とケース 2・5〜12 は今数に入れる。
  - 延ばすもの（ケース 3・4・5 のバイト列、FSMTimingSpec のケース 7・8）は、どの段で戻すかを PR の本文にも書く（決定の文面）。
- **保存済みデータ・上流**：影響なし。

## 9. 変更の記録（独立設計レビュー・Bugbot 指摘への対応）

独立設計レビュー `/Users/j5ik2o/.orca/bots/fraktor-ng-rs/memory/pr288-design-review.md`（固定 HEAD `4379e3f73ab4df22792fcd4e6228b3033af690f0`、高 4・中 6・低 4）および PR #288 の Cursor Bugbot 指摘（1 件）に対する判断と反映箇所を記録する。15 件すべてを直した。ただし、レビューの「直し方」のうち 3 つは原典を読み直した結果そのままの形では採らず、原典に合う別の形にした（H2 の「ログの文面にパスを入れる」、H2 の「`pattern`」、H4 の「`pre_start` での入れ子の子の生成」。理由は各行）。表の下に、レビューの外で見つけて直したものも記録する。

| 指摘 | 扱い | 対応内容と反映箇所 |
|:---:|:---:|:---|
| H1 | 直す | `Fsm::on_transition_with_view` を追加し、`stay()`・未処理イベント（警告＋`FsmState::stay()`）・`TimeoutMarker`・複数回 `when_with_state_timeout`（`timeout.or(existing)`）の規則を `FSM.scala:511-513, 755-763, 768-772, 817-914` と 1 対 1 で明記した。`FsmView` のタイマ操作はライブ表を即時更新し同一ハンドラ内の `is_timer_active` に即時反映する設計へ改めた（§1.2、§3.1(a)、§4 I1/I3、§6、§7） |
| H2 | 直す（直し方の 2 つは別の形） | `Message::with_display`／`display_text`（`Debug for Message` と `unhandled_notification_spec.rs` を壊さない。包括の実装をやめて `PlainPayload`／`DisplayPayload` の 2 つの包みにする）、`Fsm::with_state_display`（`Fsm<S, D>` の境界を壊さない）、`ActorContext::publish_warning`（`pub(crate)`）で、原典の文面 `"unhandled event {event} in state {state}"` を作る。`EventFilter` は中身を隠した struct にし、`with_message`／`start` を全 level で効かせ、`with_source`／`contains`／`with_occurrences`／`filter_events` を足し、`Error` は原典どおり文面と原因の文面のどちらかで当てる。`EventSinkRegistry` は安定 ID にする。**採らない形 1**：レビューの「`ActorInitializationException` の `message` を `"{path}: {message}"` にする」。原典では、パスは原因の `getMessage` にあり、`logFailure` のログの文面は根の原因の文面のまま（`ActorCell.scala:682-700`、`Actor.scala:202-204`、`FaultHandling.scala:404-422`）。そこでログの文面は変えず、`Error::with_cause_and_message` で原因の文面 `"{path}: exception during creation, root cause message: [..]"` を持たせ、`LogEvent::cause_message` で読めるようにした。**採らない形 2**：レビューの「`pattern`」。原典は正規表現だが、要るのは特殊文字の無い `"/user/failingActor:"` だけで、正規表現には外部 crate の判断が要る。していることのとおり `contains` と名付け、正規表現は別件にした。既存のテスト 2 本（`local_actor_ref_provider_spec.rs:105`・`actor_configuration_verification_property_test.rs:143`）が引数の黙殺で通っていた事実と、その直し方も記録した（§2.1、§2.2、§3.2、§3.4、§3.5、§8） |
| H3 | 直す | 旧案 A（FSM 専用 `Scheduler` port＋std スレッド）と旧案 B を却下し、レビュー第一推奨の案 C（`#200` PR #283 と同じ `Executor` の `BTreeMap` 期限索引＋`Clock`＋`await_envelope` 期限カット）を採用した。`#234` 段 6（[Issue #257](https://github.com/j5ik2o/fraktor-ng-rs/issues/257)）の汎用 `Scheduler` port と衝突せず、段 5〜6 で `ReceiveTimeout` と一括移行できる（§2.1、§3.1(c)、§8） |
| H4 | 直す（直し方の 1 つは別の形） | `executor_value.rs:1236-1330, 1591-1624` の非同期 `Create` を本文に書き、「参照を渡さない」の誤りを消した。ケース 1 の 13 の小ケースを §1.3.1 の表に 1 つずつ書き、「作れない：`ActorCell.contextStack` による直接の `new` の禁止（小ケース 1 と、小ケース 2〜12 の失敗の原因）」と「写せる：外側・内側・両方の失敗の `occurrences` 1・2、根の原因の文面の保持（小ケース 13）、失敗のあとの停止・名前の解放・次の生成」に分けた。`Props` の factory の中から `actor_of` を呼ぶ再入の panic は、テストの本体が先に内側を作って参照を外側の factory に渡す形で避けた。**採らない形**：レビューの「`pre_start` での入れ子の子の生成」。原典では外側も内側も `/user` の兄弟で、内側を外側の子にすると、外側の失敗中に内側の失敗が `entry.held` に保留されて捨てられ、`occurrences = 2` が 1 になるため（§1.3、§1.3.1、§1.5 決定 4、§4 I7、§7） |
| M1 | 直す | `Fsm::new()` ごとに `portable_atomic::AtomicU64` から一意の `instance_id` を採番し、`state_generation` と `named_timer_gen` を分離して全印に `instance_id` を載せる設計にした。発火印は `pub(crate) enum FsmTimerMark` へ分離して `FsmControl` を `test-support` に留め、`FsmView` からのタイマ操作も `FsmHandle` へ同期する経路を明記した（§3.1(a)(b)、§4 I2、§6、§8） |
| M2 | 直す | `FsmView<S>` の破壊的変更を避け、`S` と `D` を既に持つ `FsmState<S, D>` に `with_unhandled`／`clear_unhandled` を置く形に確定した（既存テスト書き換え 0 件）（§3.1(d)、§3.2(c)、§8） |
| M3 | 直す | 2 ファイル分割案を撤回し、`scripts/quint-check-specs.sh` の規約に合わせて `crates/fraktor-actor/specs/fsm_timing.qnt` の 1 ファイルに本体・`val invariant`・正のテスト 11 本・負の対照 8 本（`bugStayDropsDefaultTimeoutTest`, `bugUnhandledDoesNotRearmTest`, `bugRestartReusesInstanceIdTest`, `bugNoOverrideNeverArmsTest`, `bugUnhandledOnlyNeverArmsTest` 等を含む）を作成・検査した（§6） |
| M4 | 直す | `FSMTimingSpec.scala:29-31` の `ignore_msg`、`spec.test_kit_mut().expect_no_message(spec.remaining())`（二重 dilate 回避）、およびケースごとの独立した `FraktorSpec`＋`StateMachine` 起動を明記した（§1.2） |
| M5 | 直す | ActorRefSpec ケース 5 の `EmptyLocalActorRef` 検証を 4 点（`path` 一致、`is_actor_terminated == true`、`DeadLetter` 発行、`watch` の即時 `Terminated(existence_confirmed = false)`）に強化し、§1.5 に「伺うこと」（ケース 3 の原典 Serialization grep 結果を含む）を正確に整理した（§1.3、§1.5） |
| M6 | 直す | 案 C 採用により 1ms ポーリングスレッドを廃して CI 揺れを抑え、`StateMachine` の遅延と `WorkerActor` へ `TestKitSettings::dilated` を適用する一方 `StoppingActor` の 200ms は dilate しない原典どおりの区別を明記した（§1.2、§1.3） |
| L1 | 直す | 基準を `main`（`53014ed`、PR #275・#283 マージ済み）へ更新し、§2 の `Executor` 期限索引、`FraktorSpec::receive_while`、`PoisonPill`/`Kill` の `Executor::invoke` 特別扱い、`TestKit::expect_msg_terminated`／`TestProbe::expect_msg_terminated_within` の事実関係を正確に直した（§2.1、§2.2） |
| L2 | 直す | ケース 8 の後続 issue 作成前提を明記し、ケース 7 の代わりの裏付けとして P1 が手動時計 `ActorSystem::set_clock` で「メールボックス滞留印の取り消し破棄」を確定的に再現する手順を具体化した（§1.2、§1.4、§5）。ケース 7・8 の扱いは再レビュー後の決定（§1.5 決定 5・6）で「どちらも `#257` の後へ延ばす」に確定した |
| L3 | 直す | ケース 6 の親子関係の違い（`/user` → `a`）、ケース 7 の `ActorRef::forward(&inner, message, context)` 使用、ケース 11 の `EventFilter::typed("ActorKilledException")` ラップを明記した（§1.3） |
| L4 | 直す | P1・P2 の決定論的実行手段（案 C＋`ActorSystem::set_clock`）と、P3 の非同期生成・停止完了（`run_until_idle()`）後の名前解放前提を明記した（§5） |
| Bugbot（Dual timer drivers would double-fire） | 直す | 案 C 採用と合わせて `TestFsmRef` の `drive_timers` スレッドを廃止し、通常機械・`TestFsmRef` 機械の双方を `Executor` 期限索引の単一ドライバに統一することで二重発火を構造上排除した（§3.1(c)、§4 I4、§7、§8） |

### 9.0 再レビュー（PR #288、中4・低11）への対応

独立設計再レビュー `/Users/j5ik2o/.orca/bots/fraktor-ng-rs/memory/pr288-design-rereview.md`（`deb6486` 基準、高 0・中 4・低 11）の 15 件への対応である。15 件すべてを本文へ反映した。採らない形は無い（原典を引き直して形を決めた点だけ、各行に書く）。あわせて、元帥・オーナーの決定（`pr288-decisions.md`）を §1.5 に写し、ケース 7・8 の扱いを決定どおりにそろえた。

| # | 重さ | 対応と反映箇所 |
|:---:|:---:|:---|
| 中1 | 中 | `EventFilter::filter_events` を `EventFilter` の関連関数から `FraktorSpec::filter_events(&mut self, ...)`／`FraktorSpec::intercept(&mut self, ...)` に変えた。実物の `EventFilter::intercept` は `&mut ActorSystem` を取り、`ActorSystem` は `Clone` を持たない（`actor_system_value.rs:89-95`）ので、ブロックの中で `spec.within`／`spec.test_kit_mut()` を呼ぶケース 10・ActorRefSpec ケース 1・2・11 は今の口では書けない。既存の `EventFilter::intercept` は残し、フィルタ 1 本で `spec` を使わないケース（ActorRefSpec 2・11）はそちらで足りることを各ケースに書いた（§3.5、§1.2 ケース 10、§1.3 ケース 2・11、§8） |
| 中2 | 中 | 繰り返しタイマの間隔 `<= 0` は、原典 `LightArrayRevolverScheduler.scala:242-246` の `checkPeriod` どおり張る時点で panic する（状態関数の失敗として監督へ渡る）。発火のループは `now` を 1 回だけ読み、その `now` 以下の行だけを撃つ（`#200` の `fire_due_receive_timeouts` は毎回 `now_opt()` を読む作りだが、繰り返さない受信タイムアウトにだけ足りる）。`FixedRate` が追いつくために複数回撃つのは原典どおりに残すと 1 行書いた。§7 の壊しうる入力にも行を足した（§3.1(c)、§7） |
| 中3 | 中 | 索引のキーを `(期限, ActorKey, FsmTimerSlot)` から `(期限, seq)` に変えた。`seq` は予約のたびに単調に増える番号で、原典 LARS の「期限ごとのバケツに入れた順（FIFO）」を表す。FSM では状態関数の中の `startSingleTimer` が先に予約され、状態タイマは `makeTransition` の最後（`FSM.scala:901-904`）で予約されるので、同じ長さなら名前付きタイマが先に撃たれる。受信タイムアウトの索引とも 1 つの `seq` で合わせて撃つと決めた。P1・P2 の参照モデルも同じ規則にする（§3.1(c)、§5、§7） |
| 中4 | 中 | ケース 1 の小ケース 2〜12 とケース 2 の「失敗が呼び手まで届く」検査を、原典の `promiseIntercept`／`wrap`（`ActorRefSpec.scala:128-144`）どおりに写す形にした（`promise_intercept`：factory の中で `catch_unwind` → 送信 → `resume_unwind`）。テストは `run_until_idle()` のあとに受け口を読む。小ケース 13 とケース 2 は届いた文面の完全一致（13 は `"Ur state be b0rked"`、ケース 2 は空）も見る。§1.3.1 の表に「`Promise` を渡す先（原典）」の列を足した。届く失敗の種類が AIE でなく panic になる点だけは決定 4 に含まれる（§1.3.1、§1.3 ケース 2） |
| 低1 | 低 | Quint の `armedMatchesSelection` を作り直した。`expectedArmed` は `liveSid` と同じ `selectionShouldArm` から作っていたので自分自身と比べていた。モデルの変数（`stActive`／`lastTarget`／`lastOverride`）から選択を書き下し直して比べる形にし、`selectionShouldArm` を書き換える変異（両辺が一緒に動く）でも捕まるようにした。`keptAfterMark` は「`StateTimeout` の印を受けて張り直さなかったので枠を残している」（`FSM.scala:818-821`）を表し、存在しない状態への `goto` は対象外にした。`singleLeavesTable` を「最後に受理した `Single` は次の開始・取り消し・遷移まで残す」形にし、`singleFiresOnce`（受理した `Single` の名前の集合が `NamedNames` を超えない）を足した。レビューの 2 つの変異（`NoOverride => false`、未処理のときだけ張らない）は、旧版では `quint run` で違反なし、新版ではどちらも違反になることを確かめた。`namedMarks` の `Set` と trace の浅さは今回直さず、残る弱点として §6 に書いた（§6） |
| 低2 | 低 | §9 の M3 の行を「正のテスト 11 本」に直した（§9）。§6 の本数も実物に合わせた（`quint test` 19 本） |
| 低3 | 低 | ケース 6・7 を「テストの本体が先に `nested`／`inner` を作り、参照を外側の factory に渡す」形に直した（原典はコンストラクタの中の `system.actorOf`。`ActorRefSpec.scala:367-377`、`:379-388`）。`pre_start` での入れ子の生成はケース 12 と説明用の親だけに限ると書いた。「内側の `Create` が先に積まれる」を原典どおり「外側の `Create` を処理するときに creator が走り、そこで内側の `actorOf` が呼ばれる」に直した。`weak.try_actor_of` を実物の `spawn_child`／`try_spawn_child`（`weak_actor_system.rs:77, 112`）に直した（§1.3） |
| 低4 | 低 | `FsmTimerKind` を「非公開のモジュールに `pub enum` として置き、`test-support` のときだけ `#[cfg(feature = "test-support")] pub use` で出す」形に直した（`pub(crate)` の再公開は E0364）（§3.1(b)） |
| 低5 | 低 | I4 の「以降の発火も `DeadLetter` も起きない」を「停止の後に新しい期限が撃たれない」に直し、停止前にメールボックスへ入った印は原典と同じく `DeadLetter` として流れることがあると書いた（§4 I4） |
| 低6 | 低 | `test_fsm_ref_value_test.rs:41-55` の `the_timer_driver_ends_when_the_machine_stops_for_good` が `fsm.stop_driver` を読むのでコンパイルできないことを §8 に書き、red を書く担当が「消すもの」「代わりに確かめること（停止後に索引へ行が残らない）」「`#181` line 219 の約束」を書いた（§8） |
| 低7 | 低 | 原典 `FSM.scala:141-145` どおり、`"PoisonPill"`／`"Kill"` は `FsmTimerMark` で包まずに送ると決めた（`Executor::invoke` の自動受信に委ねる）。`Single` の表からの削除は送った時点で行う（観測上同じ）（§3.1(c)、§7） |
| 低8 | 低 | `#200` の §13-3 の条件 (b)「汎用のタイマの表は作らない」に反しない理由（FSM 専用で `FsmTimerMark` しか作らない）と、条件 (c) の制約「段 5 のプールではアイドルの FSM のタイマが撃たれない」を `#256`・`#257` の両方へ転記することを書いた（§3.1(c)、§8） |
| 低9 | 低 | ケース 10 の手順に、`within` の外の `TestUnhandled`（`FSMTimingSpec.scala:149`）を足した。`start_fsm` に「`initialize()` は呼ばない」（原典 `:198-290`、`:292-300`）を書いた（§1.2 ケース 10、§1.2 冒頭） |
| 低10 | 低 | 「表から消費する」を「`Executor` 側の索引（と `fsm_timers` の行）から消費し、FSM 側のライブ表 `timers` からは印を受け取ったときに初めて `Single` を消す（`FSM.scala:823-831`）」に書き分けた（§3.1(c)） |
| 低11 | 低 | 既存のテスト 2 本は、原典と違う今の文面の完全一致に書き直さず、`start(&format!("unhandled message from {}", dead_letters.path()))`（前方一致）に留めることを既定にした。文面を原典の形に直す案 2 は、実装の担当と相談する場合だけにした（§3.5、§8） |

### 9.1 レビューの外で見つけて直したもの

| 見つけたこと | 直したこと |
|:---|:---|
| FSMTimingSpec ケース 4・5・6 の引数なしの `expectNoMessage()` は、既定の 100ms（dilated。`TestKit.scala:733-734`、`reference.conf:26`）を待つ。scope の残りを待つのではない | §1.2 の写し方を `settings.dilated(settings.expect_no_message_default_timeout())` に直した。`remainingOrDefault` を渡すのはケース 1・2・3 だけ |
| ケース 7 の形の変更（`mailbox.hasMessages` 待ちを落とす）は、Spec の中で滞留した古い印の破棄を通らなくなるので、原典より弱い | 数に入れず、ケース 8 と同じく `#257` の後に延ばすことにした（決定 6）。代わりの裏付けは P1 と Quint |
| `Payload` の包括の実装があるので、`DisplayPayload` だけ上書きする形はコンパイルできない | §3.2(a) を 2 つの包みの形に直した |
| 存在しない状態への `goto` で、原典は `terminate` を呼んで名前付きタイマを空にする（`FSM.scala:886-888, 933-939`） | §3.1(a) に足し、Quint にも入れた（`gotoNonExistentClearsTimersTest`）。エラーログを出さないことは別件（§8） |
| 原典の `filter` は 1 つの事象を最初に当たったフィルタ 1 本にだけ数える。port は当たったすべてに数える | この issue のフィルタは重ならないので結果は同じ。変えると既存テストに響くため、別件にした（§3.5、§8） |
| 基準の `main` が `53014ed` に進んだ | 取り込み、`executor_value.rs` の行番号を `53014ed` の位置に直した |
