# 設計：#234 1 つの ActorSystem を複数のスレッドで回す実行器（dispatcher の移植の前提）

[Issue #234](https://github.com/j5ik2o/fraktor-ng-rs/issues/234) の設計である。親は
[Issue #179](https://github.com/j5ik2o/fraktor-ng-rs/issues/179)（台帳の unported を 0 件にする）。原典は
Pekko v2.0.0（commit `9fbd2283b1cf701c90a1223bf621942c191db61f`）。

状態：設計の案（未承認）。設計の独立レビューはまだ。この issue では実装しない。実装は §6 の段ごとに別の issue に分ける。

この PR に入れるものは 2 つである。

| ファイル | 中身 |
|:---|:---|
| `docs/design/issue-234-multithread-executor.md` | この文書 |
| `crates/fraktor-actor/specs/mailbox_scheduling.qnt` | §5 の Quint 仕様。CI の `quint` ジョブが typecheck・test・run を流す |

## 1. 目的と範囲

### 1.1 目的

1 つの `ActorSystem` の中のアクターを、複数のスレッドで同時に動かせるようにする。そのうえで、次の Spec を移植できる状態にする。

- #179 の順番表の 38〜46：SystemMessageListSpec、DispatcherShutdownSpec、ControlAwareDispatcherSpec、
  PriorityDispatcherSpec、StablePriorityDispatcherSpec、MailboxConfigSpec、ExecutionContextSpec、
  BalancingDispatcherSpec、DispatcherActorsSpec、PinnedActorSpec、DispatcherActorSpec、ActorModelSpec
  （同じファイルの DispatcherModelSpec と BalancingDispatcherModelSpec を含む）、DispatchersSpec、
  CallingThreadDispatcherModelSpec
- [Issue #235](https://github.com/j5ik2o/fraktor-ng-rs/issues/235) の ConsistencySpec

### 1.2 最小限、要るもの

上の Spec が実際に使う機能から逆算した。§8 の対応表が、ケースごとの根拠である。

| 要るもの | 使う Spec |
|:---|:---|
| アクターごとのメールボックスと、状態（idle、scheduled、running、suspended、closed） | すべて |
| 複数スレッドの実行器（固定のスレッド数、fork-join 相当の並列度の決め方） | ConsistencySpec、DispatcherActorsSpec、ActorModelSpec ほか |
| throughput と throughput の期限 | DispatcherActorSpec、ConsistencySpec（throughput = 1） |
| Pinned（アクター 1 つにスレッド 1 本） | PinnedActorSpec、ActorModelSpec（boss）、DispatchersSpec |
| Balancing（共有のキューを複数のアクターで分け合う） | BalancingDispatcherSpec、BalancingDispatcherModelSpec |
| CallingThread（送った側のスレッドでそのまま処理する） | CallingThreadDispatcherModelSpec |
| メールボックスの種類：unbounded、bounded（容量と push の待ち時間）、priority、stable priority、control-aware | Priority／StablePriority／ControlAware の各 Spec、MailboxConfigSpec |
| dispatcher の登録表（id での参照、別名、既定の dispatcher）。設定はコードで与える | DispatchersSpec、ほかの Spec の `withDispatcher` |
| dispatcher の観測口（register、unregister、suspend、resume、dispatch、shutdown の回数） | ActorModelSpec と、それを継承する 3 クラス |
| 外からの suspend／resume（内部 API） | ActorModelSpec「not process messages for a suspended actor」 |
| 住人がいなくなった dispatcher の自動停止と、再利用での再起動 | ActorModelSpec「dynamically handle its own life cycle」 |
| dispatcher を、クロージャを実行する場所として使う口（`Future { … }(dispatcher)` 相当） | ActorModelSpec、ExecutionContextSpec の一部 |
| システムの終了で、dispatcher のスレッドがすべて終わること | DispatcherShutdownSpec |
| testkit の待ち方：テストのスレッドがシステムを回さずに、届くのを待つ | 複数スレッドで動く Spec のすべて |

### 1.3 後回しにするもの

- 設定ファイル（HOCON）からの読み込み。dispatcher とメールボックスの設定は、コードで組み立てて渡す
  （G0b の承認済みの判断 E と同じ方針）。DispatchersSpec の「get config for dispatcher」などの 4 ケースは、
  設定の層を作るまで待つ（§8）
- deployment の設定と router（DispatchersSpec の deployment の 4 ケース。#179 の 47 以降の router の仕事）
- work stealing。fork-join 相当は、並列度の決め方（`parallelism-min`、`factor`、`max`）だけを写し、
  実行器は 1 本の共有キューを N 本のスレッドで取り合う形から始める（§3.5）。work stealing は計測で要ると
  分かってから
- 仮想スレッド、affinity、`blocking {}` による一時的なスレッドの追加（JVM と Scala の機能。§8）
- Thread の interrupt（JVM の機能。Rust に同じものは無い。§8）
- 遠隔（remote）と cluster の dispatcher

## 2. 今の形の問題

### 2.1 どこが !Send なのか

試作（§10 の 1）で `assert_send` と `assert_sync` を `cargo check` した。E0277 が 9 件出る。原因は 3 つの型である。

| 型 | 場所 | 何が !Send か |
|:---|:---|:---|
| `ActorSystem` | `kernel/actor_system/actor_system_value.rs:46` | `executor: Rc<RefCell<Executor>>`。`event_stream` の中の 2 つも |
| `WeakActorSystem` | `kernel/actor_system/weak_actor_system.rs:29` | `executor: rc::Weak<RefCell<Executor>>` |
| `EventStream` | `kernel/event/event_stream.rs:63` | `setup: Rc<RefCell<StreamSetup>>` と、中の `WeakActorSystem` |

外側の `Rc` を外しても、`Executor` 自体が Send ではない（試作 2）。中身の 4 か所である。

| 中身 | 場所 |
|:---|:---|
| `Box<dyn Actor>`（`Actor` に `Send` が無い） | `kernel/actor/actor_trait.rs:19`、`executor/actor_cell.rs` |
| `become` の枠 `Box<dyn FnMut(&mut ActorContext, Message)>` | `kernel/actor/behavior_stack.rs:11`、`actor_context.rs:90-105` |
| 終了の通知 `Box<dyn Fn(&ActorRef)>` | `executor/executor_value.rs:116`、`:201` |
| FSM の状態関数と遷移の関数（`Box<dyn FnMut …>`） | `kernel/actor/fsm.rs:13`、`:16` |

`ActorRef`、`Props`、`Message`、`Envelope` は、すでに `Send + Sync` である（`ActorRef` は試作 1 で通る）。

### 2.2 !Send より深い問題

Send にしただけでは、アクターは同時には動かない。今の実行器は、次の 3 点で 1 本のスレッドを前提にしている。

1. **1 つのシャードに、ローカルのキューが 1 本だけある**（`executor/shard_queue.rs`）。メールボックスは
   アクターごとに無い。配送のたびに、`cells` を先頭から探して宛先を見つける（`executor_value.rs:1231`、
   アクターの数に比例する）
2. **アクターの文脈が、実行器全体を `&mut` で借りている**（`ActorContext { executor: &'a mut Executor }`、
   `actor_context.rs:30`）。子を作る、watch する、止めるといった操作は、実行器の表をその場で書き換える。
   子の `pre_start` は、親の `receive` の中で同期して走る（入れ子の枠、`enter_frame`／`finish_boundary`）
3. **アクターの間の操作が、メッセージではなく関数の呼び出しで進む**。監督の判断、`Terminated` の通知、
   子の停止の待ち合わせは、`pending_*` の表と枠の深さで、Pekko の順序をまねている

Pekko では、この 3 つは次の形である。アクターごとにメールボックスがある。アクターの間の操作は system message
（Create、Suspend、Resume、Recreate、Terminate、Supervise、Watch、Unwatch、Failed、DeathWatchNotification）で、
宛先のアクターを実行しているスレッドが処理する。複数スレッドにするには、この形に寄せる必要がある。

### 2.3 Rc と RefCell を、どこからどこまで置き換えるか

| 置き換える（段 1） | 置き換え方 |
|:---|:---|
| `ActorSystem.executor`、`WeakActorSystem.executor` | `Arc<spin::Mutex<Executor>>` と `sync::Weak`。`spin` はすでに依存にある |
| `EventStream.setup` | `Arc<spin::Mutex<StreamSetup>>` |
| `ActorEventBus.subscriptions`（`kernel/event/actor_event_bus.rs:49`。終了の通知の中から触る） | `Arc<spin::Mutex<Subscriptions>>` |
| 上の 4 か所の中身 | `Actor: Send`、枠と FSM の関数に `+ Send`、`Fsm<S, D>` に `S: Send, D: Send`、終了の通知に `+ Send` |
| 単体テスト `actor_system_value_test.rs`（3 か所）と `actor_event_bus_test.rs`（2 か所） | `borrow` を `lock` に |

| 置き換えない | 理由 |
|:---|:---|
| `adaptor/ask_driver.rs` の `thread_local!` の `RefCell<Vec<Step>>` | スレッドごとの登録表で、作りとしてスレッドをまたがない |
| `adaptor/random_id_generator.rs` の `thread_local!` の `Cell` | 同上 |
| testkit の `TestKitBase`、`TestKit`、`FraktorSpec` の `Rc<RefCell<Option<Instant>>>`（`within` の期限） | テストのスレッドだけが使う。ActorModelSpec でも、別のスレッドへ渡すのは `ActorRef` だけである |
| `kernel/event/scanning_event_bus.rs` の `Box<dyn Fn>` | システムが持たない、利用者の値である |

## 3. 型と API の形

### 3.1 移り方の方針：実行器は 1 つのまま、中身を段ごとに入れ替える

既存の `Executor` を残したまま、新しい実行器を横に作る形は取らない。監督、DeathWatch、stash、`become`、FSM、
event stream の意味論は、今の 1767 行と、それを確かめる既存のテスト 1077 件にある。2 つの実行器を並べると、
意味論が 2 か所に分かれ、新しい側は既存のテストで確かめられない期間が長くなる。

そこで、今の実行器の中身を、§6 の段に分けて入れ替える。どの段の終わりでも、既存のテストがすべて同じ実行器で
通ることを完了の条件にする。決定論的に回す今のやり方（テストのスレッドが `run_one` でシステムを回す）は、
「手で回す dispatcher」（`MessageDispatcher` と `ManualExecutor`）として残す。`ActorSystem::new(name, boundary)` で
作ったシステムの既定の dispatcher はこれにする。複数スレッドの dispatcher は、dispatcher を登録して作ったシステムで使う。
そのシステムでは、既定の dispatcher も複数スレッドにできる（DispatcherActorsSpec は `withDispatcher` を使わず、
既定の dispatcher の上で遅いアクターと速いアクターを並べて動かす）。

この方針は §9 の D1 で上司に決めてもらう。

### 3.2 kernel（no_std）と adaptor（std）の分け方

crate に `#![no_std]` は無い。kernel を no_std に保つのは、規約と syn-check の `cfg-std-forbid` による
（kernel では `use std::` を使わない）。この設計でも同じに守る。

| 置き場所 | 型（名前は案） | 中身 |
|:---|:---|:---|
| kernel `dispatch/mailbox_status.rs` | `MailboxStatus` | `AtomicU32` 1 つ。bit 0 が closed、bit 1 が scheduled、bit 2 から上が suspend の数（Pekko `Mailbox.scala:47-55` と同じ配置）。`set_as_scheduled`、`set_as_idle`、`suspend`、`resume`、`become_closed` はどれも compare-and-set の繰り返し |
| kernel `dispatch/message_queue.rs` | trait `MessageQueue: Send + Sync` | `enqueue(Envelope) -> Result<(), Envelope>`（閉じていれば返す）、`dequeue`、`has_messages`、`number_of_messages`、`close_and_drain` |
| kernel `dispatch/…` | `UnboundedQueue`、`BoundedQueue`（待たない）、`PriorityQueue`（stable の有無）、`ControlAwareQueue` | `spin::Mutex<VecDeque<Envelope>>` と、閉じたかどうかを同じロックの中に持つ（§4 の I6） |
| kernel `dispatch/system_message.rs` | enum `SystemMessage` | Create、Suspend、Resume、Recreate、Terminate、Supervise、Watch、Unwatch、Failed、DeathWatchNotification、ChildTerminated |
| kernel `dispatch/mailbox.rs` | `Mailbox` | 状態、通常のキュー（`Arc<dyn MessageQueue>`。Balancing では共有）、system message のキュー、`spin::Mutex<Option<ActorCell>>`、dispatcher への参照 |
| kernel `dispatch/message_dispatcher.rs` | `MessageDispatcher` | id、throughput、期限、住人の数、観測口、`register_for_execution`（Pekko `Dispatcher.scala:146` の形）、Balancing の team |
| kernel `dispatch/task_executor.rs` | trait `TaskExecutor: Send + Sync`（port） | `execute(Task) -> Result<(), Rejected>`、`shutdown`。`Task` はメールボックスの 1 回の実行か、`Box<dyn FnOnce() + Send>` |
| kernel `dispatch/manual_executor.rs` | `ManualExecutor` | 実行待ちのメールボックスを `VecDeque` に積むだけ。`run_one` が 1 つ取り出して回す。今の決定論的な回し方の後継 |
| kernel `dispatch/dispatchers.rs` | `Dispatchers`、`DispatcherConfig` | id から dispatcher への表、別名、既定の id。設定はコードで組む |
| kernel `dispatch/dispatcher_observer.rs` | trait `DispatcherObserver: Send + Sync` | 既定は何もしない `registered`、`unregistered`、`suspended`、`resumed`、`dispatched`、`shut_down` |
| kernel（既存） | `Clock` | throughput の期限を測る。今の port をそのまま使う |
| adaptor `thread_pool_executor.rs` | `ThreadPoolExecutor` | 固定の N 本のスレッド、`Mutex<VecDeque<Task>>` と `Condvar`。スレッドの名前は `<システム名>-<dispatcher id>-<番号>` |
| adaptor | `ForkJoinConfig::scaled_pool_size(min, factor, max)` | `ceil(コア数 × factor)` を `[min, max]` に収める（`ThreadPoolConfig.scaledPoolSize` と同じ式。ConsistencySpec が使う） |
| adaptor | `PinnedExecutor` | スレッド 1 本。持ち主のアクター以外の登録を断る（Pekko `PinnedDispatcher.scala` の `register`） |
| adaptor | `BlockingBoundedQueue` | 満杯のとき push の待ち時間だけ待つ bounded キュー（`Condvar` が要るので std） |
| adaptor | `CondvarSignal`（kernel の port `WakeSignal` の実装） | 手で回す実行器に仕事が来た、または test actor にメッセージが来た、をテストのスレッドへ知らせる |
| testkit | `CallingThreadDispatcher` | 送った側のスレッドで処理する。スレッドごとの再入のキューは `thread_local!` なので std の testkit に置く（Pekko でも testkit にある） |

kernel のロックの決まり：kernel の `spin::Mutex` は、キューの出し入れや表の読み書きのような短い区間だけで持つ。
利用者のコード（`receive` など）をまたいで持つのは、`Mailbox` の中の `ActorCell` のロックだけである。このロックは、
scheduled の bit を持つスレッドしか取りに行かないので、取り合いは起きない。`try_lock` が失敗したら、
「1 つのアクターを 2 つのスレッドが処理した」という不変条件（I1）の破れとして panic する。待ち（スレッドの休止、
bounded の push の待ち、testkit の待ち）は、どれも adaptor の `Condvar` で行う。

### 3.3 Send と Sync の境界

| 種類 | 型 |
|:---|:---|
| `Send + Sync` | `ActorSystem`、`WeakActorSystem`、`EventStream`（段 1 から）。`ActorRef`、`Props`、`Message`（今も）。`Mailbox`、`MessageDispatcher`、`Dispatchers`、システム全体で共有する表 |
| `Send` だけ | `Box<dyn Actor>`、`ActorCell`、`become` の枠、`Envelope`。一度に 1 つのスレッドだけが触る |
| Send にしない | `ActorContext<'a>`（1 回の実行の間だけ借りる）、testkit の期限の `Rc` |

`Actor: Send` は、公開 API を狭める変更である。`Rc` や `RefCell` を持つアクターは作れなくなる。試作 3 では、
リポジトリの中でコンパイルが通らなくなったのは単体テスト 2 ファイルの 5 か所だけで、どれも `borrow` から `lock` への
書き換えで済んだ。ほかの既存のテストは、書き換えなしでそのまま通った（§10 の 3）。Pekko のアクターも、
どのスレッドで動くかを選ばない。

### 3.4 ActorRef とメールボックス

段 3 から、`ActorRef` が持つ `boundary: Arc<dyn ShardBoundary>` を、送り先を表す値に替える。

```rust
// 案。名前は段 3 で決める。
enum Endpoint {
  Mailbox(Arc<Mailbox>),             // セルのアクター
  Boundary(Arc<dyn ShardBoundary>),  // function ref、no-op、ほかのシステム
}
```

`tell` は、`Mailbox` なら `enqueue` してから `register_for_execution` を呼ぶ。等しさとハッシュは、今と同じく
id と世代だけで決める。宛先を探す線形の検索（§2.2 の 1）は無くなる。

所有の形：`Arc<Mailbox>` は、送り端としてハンドルと dispatcher のキューが共有する。アクターの中身（`ActorCell`）は
`Mailbox` の中の `Option` が 1 つだけ持ち、アクターが止まったら捨てる。古いハンドルが残っても、残るのは小さな
メールボックスの殻だけである。所有権モデル（`docs/decisions/ownership-model-arc-overhead.md`）は、Arc を置いてよい
場所に「スレッド境界を越える送信端」を挙げている。メールボックスは、複数スレッドの実行器での送信端である。
ただし「ActorRef は id＋世代の値」という既定から一歩出るので、§9 の D2 で確かめてもらう。

### 3.5 Dispatcher の種類とスケジューリング

| 種類 | 作り | 原典 |
|:---|:---|:---|
| Dispatcher（既定） | `MessageDispatcher` と `ThreadPoolExecutor`。throughput（既定 5）と期限（既定 0ms、つまりなし。`reference.conf:622-625`）。1 回の実行で最大 throughput 件を処理し、期限を過ぎたら次のメッセージを取らずに手放す | `Dispatcher.scala`、`Mailbox.scala:277` の `processMailbox` |
| fork-join 相当 | 上と同じ実行器で、スレッドの数を `scaled_pool_size` で決める。work stealing はしない（§1.3） | `ForkJoinExecutorConfigurator.scala` |
| Pinned | アクターごとに dispatcher を 1 つ作る。throughput は `u32::MAX`、期限なし、スレッドは 1 本 | `PinnedDispatcher.scala` |
| Balancing | team の全員が 1 本の `MessageQueue` を共有する。`dispatch` は共有のキューに積み、宛先のメールボックスを予約し、だめなら team のほかの 1 人を予約する（teamWork） | `BalancingDispatcher.scala:106-125` |
| CallingThread | testkit に置く。送った側のスレッドで、そのまま処理する。そのスレッドがすでにそのアクターを処理中なら、終わってから処理する | testkit の `CallingThreadDispatcher.scala` |
| Manual（fraktor 独自） | `ManualExecutor`。テストのスレッドが `run_one`／`run_until_idle` で回す。`ActorSystem::new` の既定 | なし（今の G0b の回し方の後継） |

1 回の実行（Pekko `Mailbox.run`、`Mailbox.scala:262`）は次の順に進む。

1. 閉じていなければ、system message をすべて処理する
2. 通常のメッセージを 1 件取り、`receive` に渡す。そのあと、また system message をすべて処理する。
   suspended でなく、throughput と期限が残っていれば、2 を繰り返す
3. scheduled の bit を下ろす（`set_as_idle`）
4. もう一度 `register_for_execution` を呼ぶ。3 と 4 の間に届いたメッセージを、ここで拾う（§5 の LostWakeup）

### 3.6 既存のテストと testkit

- `ActorSystem::new(name, boundary)` の既定の dispatcher は Manual にする。今と同じく、テストのスレッドが回す。
  `run_one`、`run_until_idle`、`await_envelope` の名前と意味は変えない
- 未登録の dispatcher の id：今は `Props::with_dispatcher` の名前を記録するだけで、使わない
  （`tests/supervisor_misc_spec.rs:144` が `pinned-dispatcher` などを未登録のまま使う）。`ActorSystem::new` で
  作ったシステムでは、未登録の id は既定の dispatcher に回す（今の振る舞いを保つ）。dispatcher を登録して作った
  システムでは、Pekko と同じく、未登録の id で `actor_of` を断る
- testkit の待ち方：今の `TestKitBase::receive_one` は、`run_one` で 1 件回し、無ければ `await_envelope` で境界を
  待つ（`test_kit_base.rs:493-532`）。複数スレッドのアクターは、テストのスレッドが回さなくても進む。そこで
  `await_one` を、kernel の port `WakeSignal` を待つ形に替える。手で回す実行器に仕事が積まれたときと、
  test actor にメッセージが届いたときに、どちらも同じ合図を鳴らす。手で回すアクターだけのテストでは、今と同じ
  順序で回る
- `ask` の待ち（`adaptor/ask_driver.rs`）は変えない。手で回すシステムは登録した driver が回し、複数スレッドの
  アクターへの `ask` は、今の条件変数の待ちで返りを待つ（登録の無い宛先の経路で、すでにある）

## 4. 不変条件

### 4.1 メールボックスとスケジューリング

| 番号 | 不変条件 | 何が守るか | Quint の名前 |
|:---|:---|:---|:---|
| I1 | 1 つのアクターのメッセージを、同時に 2 つのスレッドが処理しない | scheduled の bit の compare-and-set に勝った 1 人だけが、プールにメールボックスを出す。`ActorCell` のロックの `try_lock` の失敗は panic | `atMostOneRunner` |
| I2 | メールボックスはプールに高々 1 回だけ入る。閉じていない間、scheduled の bit は「プールにある、または実行中」とちょうど一致する | 同上 | `scheduledMatchesTheRun` |
| I3 | 仕事のある開いたメールボックスは、予約されているか、予約しに行く誰かがいる（起こし忘れが無い） | §3.5 の 4 | `noStrandedMessage` |
| I4 | suspended の間は `receive` を呼ばない。suspend の数を変えるのは、そのアクターを実行中のスレッドだけ（外からの suspend は Suspend の system message にする） | `should_process_message` を取り出しの直前に読む。system message は `receive` の外で処理する | `noInvokeWhileSuspended` |
| I5 | 閉じたあとは `receive` を呼ばない。closed は抜けられない | Terminate を処理した実行の中で閉じ、以後 `set_as_scheduled` は失敗する | `noInvokeAfterClose` |
| I6 | 閉じたメールボックスは何も持たない。閉じたあとのメッセージは dead letters へ行く | 閉じたかどうかと積むことを、キューの同じロックの中で行う。閉じるときに残りを dead letters へ移す | `closedHoldsNothing` |
| I7 | 1 回の実行で処理するのは throughput 件まで。期限を過ぎたら次を取らない | 実行のループの数え方 | `throughputRespected` |
| I8 | 受け付けたメッセージは、処理済み、キューの中、処理中、dead letters のどれかにある | I6 と、`close_and_drain` | `messagesConserved` |
| I9 | 同じ送り手から同じ宛先への順序は保つ（FIFO のキューで）。priority のキューは優先度の順、stable なら同じ優先度の中で FIFO | キューの実装 | Quint には無い。PBT（§7.3） |
| I10 | あるメッセージの処理で書いた値は、次のメッセージの処理から見える。次の処理が別のスレッドでも見える | scheduled の bit の操作を AcqRel、キューのロックと `ActorCell` のロックで acquire と release がつながる | Quint は逐次一貫なので扱えない。ConsistencySpec と stress テスト（§7.3） |
| I11 | system message は、実行の始めと、通常のメッセージ 1 件ごとのあとに、届いている分をすべて処理する | §3.5 の 1 と 2 | 一部だけ（`processSystem`、`decideFinish` の前提） |

### 4.2 状態の名前と、bit との対応

| 状態 | bit | 抜け方 |
|:---|:---|:---|
| idle | closed なし、scheduled なし、suspend 0 | 送り手か system message の予約で scheduled へ |
| scheduled | scheduled あり、プールのキューにある | スレッドが取り出して running へ |
| running | scheduled あり、スレッドが `run` の中にいる | 終わりに idle へ戻り、仕事があれば scheduled へ。Terminate を処理すれば closed へ |
| suspended | suspend の数が 1 以上（scheduled とは独立） | system message だけのために scheduled にはなれる。Resume の数が揃えば idle へ |
| closed | closed あり（ほかの bit は意味を持たない） | 抜けない |

Pekko の bit は scheduled と running を区別しない。running は「scheduled の bit を持ち、スレッドが `run` の中にいる」
ことで、Quint では作業者の段階（`Running`、`Invoking`、`Finishing`）で区別する。

### 4.3 停止と終了の順序

アクター 1 つが止まるとき（Pekko `FaultHandling.scala:247` の `finishTerminate`）：

| 番号 | 順序 |
|:---|:---|
| T1 | Terminate を受けたアクターは、まず子のすべてに Terminate を送り、子の ChildTerminated がすべて揃うまで待つ。待つ間、通常のメッセージは処理しない |
| T2 | 子が揃ったら `post_stop` を呼ぶ。そのあと function ref を閉じる |
| T3 | dispatcher から外す（住人の数を 1 減らす）。メールボックスを閉じ、残りの通常のメッセージと stash を dead letters へ移す |
| T4 | 親へ ChildTerminated を、watcher へ Terminated（DeathWatchNotification）を送る。親が watcher でもあるときは、この順 |
| T5 | 終了の通知（`on_termination`）を呼ぶ |

したがって、watcher が Terminated を受けたとき、`post_stop` は終わっていて、そのアクターのメールボックスは閉じている。

システムの終了：

| 番号 | 順序 |
|:---|:---|
| T6 | `/user` の下を止め（T1 で深い方から）、次に `/system` の下を止める |
| T7 | 終わったら、登録した dispatcher のすべてを止める。新しい仕事を受け付けず、走っている仕事は最後まで走らせ、スレッドを終わらせる |
| T8 | `is_terminated` が true になるのは、T6 が終わったときである。スレッドを join で待ち切るのは `terminate` の呼び出し元ではない（アクターのスレッドから `terminate` を呼ぶと、自分を join して止まるため）。DispatcherShutdownSpec のように「いずれスレッドが 0 本になる」ことは、プールが数える生きたスレッドの数で確かめる |

dispatcher の住人と自動停止（Pekko `AbstractDispatcher.scala:196-262`）：

| 番号 | 順序 |
|:---|:---|
| T9 | 住人は、付いているアクターの数と、走っているクロージャの数の和である。0 になったら、停止までの猶予（`shutdown-timeout`）を数え始める |
| T10 | 猶予の間に住人が増えたら、止めない（UNSCHEDULED、SCHEDULED、RESCHEDULED の 3 状態） |
| T11 | 止めた dispatcher に、あとで仕事や住人が来たら、実行器を作り直す（Pekko は `executorServiceDelegate.copy()` で作り直す）。止めた回数は観測口の `shut_down` で数える |

T9〜T11 は状態遷移なので、実装する段（§6 の段 6）で `dispatcher_lifecycle.qnt` を書く。案は §5.4。

## 5. Quint

### 5.1 仕様のファイル

`crates/fraktor-actor/specs/mailbox_scheduling.qnt`（この PR に入れる）。1 つのアクターのメールボックスを、
送り手 2 人、プールのスレッド 2 本、外からの操作（Suspend、Resume、Terminate）、親の監督の判断で動かす。

| 仕様の動作 | 実装で当たるところ |
|:---|:---|
| `send`、`register` | `ActorRef::tell` → `Mailbox::enqueue` → `MessageDispatcher::register_for_execution` |
| `control`、`decide` | system message を積んで予約する（外からの suspend、親の Resume） |
| `take` | プールのスレッドがキューから 1 つ取る |
| `processSystem` | `Mailbox::process_all_system_messages`（Terminate で閉じて dead letters へ移す） |
| `startInvoke`、`endInvoke`、`failInvoke` | 1 件の `receive`。失敗したら自分を suspend し、親に Failed を送る |
| `decideFinish`、`finishIdle`、`finishRegister` | `run` の終わり。`set_as_idle` とその後の予約 |

### 5.2 CI の決まりへの合わせ方

- 不変条件は `invariant`（§4.1 の I1〜I8 の論理積）
- `run` は 14 本で、名前はどれも `Test` で終わる。`Test` で終わる名前は `run` にしか使っていない
- 定数は `pure val`（`SENDERS`、`WORKERS`、`SENDS_PER_SENDER`、`THROUGHPUT`、`CONTROL_BUDGET`）。`const` は使わない
- 負の対照は、別のファイルや `--main` に分けず、同じモジュールの `mode` で切り替える。`init` は
  `initWith(Fixed)` だけなので、CI の `quint run --invariant=invariant` は正しい形だけを探す。誤りの形は、
  `initWith(<誤り>)` から始めて `.expect(not(<不変条件>))` で終わる `run` で、`quint test` が毎回確かめる

### 5.3 負の対照（直す前の誤り）

どれも、設計で避ける誤りである。誤りの形では不変条件が破れること、正しい形では破れないことを、両方確かめる。

| モード | 誤り | 破れる不変条件 | 確かめる `run` |
|:---|:---|:---|:---|
| `LostWakeup` | 実行の終わりに bit を下ろしたあと、もう一度予約しない | `noStrandedMessage` | `lostWakeupStrandsAMessageTest`（正しい形は `lateMessageIsRescheduledTest`） |
| `CheckThenAct` | bit を読んでから立てるまでが 2 手（compare-and-set でない） | `atMostOneRunner`、`scheduledMatchesTheRun` | `checkThenActRunsTheActorTwiceTest`（正しい形は `racingSendersSubmitOnceTest`、`secondWorkerFindsNothingToTakeTest`） |
| `ExternalSuspend` | 外からの suspend が数を直接変える | `noInvokeWhileSuspended` | `externalSuspendInterruptsReceiveTest`（正しい形は `suspendWaitsForTheRunnerTest`） |
| `LateEnqueue` | closed を見るのと積むのが 2 手 | `closedHoldsNothing` | `lateEnqueueLeavesAMessageInAClosedMailboxTest`（正しい形は `sendAfterCloseIsADeadLetterTest`） |

`LateEnqueue` は、Pekko でも起きうる取りこぼし（閉じた直後に積まれたメッセージが、dead letters にならずに消える）
である。この設計では、閉じたかどうかをキューと同じロックで持つので、起きない（I6）。

手で並べた `run` だけでなく、誤りのモードから始めた `quint run`（30 手、2 万本）でも、4 つとも違反が見つかった
（§10 の 6）。正しい形は、CI と同じ条件（20 手、1 万本）でも、60 手・10 万本でも違反が無かった。Apalache による 12 手までの有界の検証でも、違反は無かった（§10 の 8）。

### 5.4 この仕様で扱わないもの（後の段で書く仕様）

| 仕様（案） | 段 | 状態と不変条件 | 負の対照 |
|:---|:---|:---|:---|
| `dispatcher_lifecycle.qnt` | 段 6 | 住人の数、停止の予定（UNSCHEDULED／SCHEDULED／RESCHEDULED）、実行器の生死、止めた回数。不変条件：住人がいる間は止めない、止めたあとの仕事は断らずに実行器を作り直す | 作り直さない実行器（止めたあとの仕事が断られる） |
| `balancing_team.qnt` | 段 7a | 共有のキュー、team の各メールボックス。不変条件：1 件のメッセージを処理するのは 1 人だけ、同じアクターが 2 スレッドで動かない、共有のキューに仕事があれば誰かが予約されているか予約しに行く | 仲間が抜けるとき（`unregister`）に teamWork をしない。抜けた仲間が予約していた分が共有のキューに残り、誰も予約されない（原典 `BalancingDispatcher.scala:100-104` が teamWork を呼ぶ理由） |
| `actor_termination.qnt` | 段 4a | T1〜T5。子の集合、post_stop、閉じたか、通知の順 | `post_stop` の前に Terminated を送る |

priority の順序、メモリの見え方（I10）、CallingThread の再入は、Quint ではなく PBT と stress テストで確かめる（§7）。

## 6. 段に分けた実装の計画

大きさは相対の見積もりである。基準は [PR #217](https://github.com/j5ik2o/fraktor-ng-rs/pull/217)（#207：kernel の
API 3 つと単体テスト、8 ファイル）を 2 とする。どの段も、設計の差分があれば先に設計の独立レビューを通す。
どの段も「既存のテストがすべて通る」を完了の条件に含む。

| 段 | issue にする単位 | 中身 | 完了の条件 | 大きさ |
|:---|:---|:---|:---|:---|
| 1 | Send にする（挙動は変えない） | §2.3 の置き換え。`spin::Mutex` は `try_lock` で取り、取れなければ「再入または同時の利用」で panic する（今の `RefCell` の二重借用の panic と同じ振る舞いを保つ。同時に使える形は段 3 以降）。`ActorSystem`、`WeakActorSystem`、`EventStream` が `Send + Sync` であることのコンパイル時の検査を `tests/` に足す | 検査を先に足して E0277（9 件）で落ちることを確かめる（red）。置き換えのあと、検査と既存のテストのすべてが通る。§10 の 5 の計測で、性能の差が無いことを確かめる | 2 |
| 2 | kernel のメールボックス（配線しない） | `MailboxStatus`、`MessageQueue` と 4 種類のキュー、`SystemMessage` とそのキュー。どこからも使わない | §7.3 の P1〜P3 の PBT。状態の操作が Quint の状態の部分と一致する | 3 |
| 3 | アクターごとのメールボックスと Manual dispatcher | `ShardQueue` の 1 本のキューを、アクターごとの `Mailbox` と `ManualExecutor` の実行待ちの列に替える。`ActorRef` の送り先を §3.4 の形にする。実行器の大きなロックと `&mut Executor` の文脈は、まだ残す | 既存のテストのすべてが通る。全体の配送の順序が変わる（全体の FIFO から、アクターごとの順番へ）。先に既存のテストを流して、順序に頼るテストを洗い出す（fail first） | 5 |
| 4 | アクターの間の操作を system message に（3 つに分ける） | 4a：生成と停止（Create、Terminate、子の名前、T1〜T5、閉じるときの dead letters）。4b：失敗と監督（Failed、Suspend、Resume、Recreate、all-for-one）。4c：DeathWatch と終了の通知、stop-and-reply、stash。文脈から `&mut Executor` を外し、自分のセルとシステム全体の共有の表を借りる形にする | 各段で既存のテストのすべてが通る。4a で終了の順序の Quint を足す。子の `pre_start` が親の `receive` の中で同期して走らなくなる（Pekko と同じ非同期）。これに頼る既存のテストを先に洗い出す | 5、5、3 |
| 5 | std のスレッドプール | `ThreadPoolExecutor`、fork-join 相当の並列度、Pinned、`Dispatchers` の登録表、スレッドの名前、testkit の `WakeSignal` の待ち方、`BlockingBoundedQueue`、dispatcher ごとのメールボックスの種類、プールが数える「同時に実行中のメールボックスの最大数」（ConsistencySpec の記録に使う。§8） | §7.3 の P4（stress）。ConsistencySpec（#235）、DispatcherActorsSpec、PinnedActorSpec、DispatcherActorSpec が移植できる状態 | 5 |
| 6 | dispatcher の寿命 | 住人の数、自動停止と作り直し、システムの終了で全部を止める（T6〜T11）、観測口、dispatcher でクロージャを実行する口、外からの suspend／resume | `dispatcher_lifecycle.qnt`。DispatcherShutdownSpec、DispatcherModelSpec が移植できる状態 | 3 |
| 7 | Balancing と CallingThread | 7a：Balancing（共有のキュー、teamWork）と `balancing_team.qnt`。7b：testkit の `CallingThreadDispatcher` | BalancingDispatcherSpec、BalancingDispatcherModelSpec、CallingThreadDispatcherModelSpec が移植できる状態 | 3、3 |
| 8 | 後始末 | 使われなくなった `ShardQueue` と、`ShardBoundary` の役目の整理。文書の更新 | 既存のテストのすべてが通る | 2 |

最初の段（段 1）だけで、既存の挙動を変えずに Send にするところまで進む。試作 3 で、その形の変更が既存のテスト
1077 件を通すことを確かめた（§10 の 3）。

Spec の移植そのもの（#179 の 38〜46 と #235）は、上の段のあとに、今までどおりノードごとの issue で行う。§8 に、
どの段のあとでどのケースが移せるかを書いた。

## 7. 壊しうる入力、性能、PBT

### 7.1 壊しうる入力

| 区分 | 入力 | 期待 |
|:---|:---|:---|
| 境界 | throughput が 1、5、`u32::MAX`（Pinned）、0（Pekko は `max(throughput, 1)`） | 0 は 1 として扱う |
| 境界 | 期限が無い、0、1 件の処理より短い | 1ms 未満は「無い」と同じ（`AbstractDispatcher.scala:372` の `toMillis > 0`）。短い期限でも、1 件は必ず処理する |
| 境界 | bounded の容量 0、1、満杯ちょうど、push の待ち時間 0 と負 | 満杯で待ち時間が 0 以上なら、その時間だけ待って dead letters（`DeadLetter`）。負なら空くまで待つ（原典 `Mailbox.scala:583-591`） |
| 境界 | コア数 1 での `scaled_pool_size(1, 1.5, 2000)` | 2（`ceil(1.5)`）。0 本にはならない |
| 並行 | 最後の取り出しと bit を下ろす間に届いたメッセージ | 実行の終わりの再予約で拾う（I3、`lateMessageIsRescheduledTest`） |
| 並行 | 2 人の送り手が同時に予約する | 1 回だけプールに入る（I2） |
| 並行 | 閉じている最中の送信 | dead letters（I6） |
| 並行 | `receive` の最中の外からの suspend | `receive` の後で効く（I4） |
| 並行 | Resume が Suspend より多い、Suspend の入れ子 | 数は 0 で止まる。入れ子は同じ数の Resume で戻る（Pekko `Mailbox.scala:160-196`） |
| 再入 | `receive` の中で自分に送る、`pre_start` で自分に送る、CallingThread での再入 | 自分が実行中なので、後で処理する |
| 失敗 | プールのスレッドでの panic（`receive`、`post_stop`、`pre_start`） | スレッドは死なない。複数スレッドの dispatcher を持つシステムは、catcher（既定 `UnwindCatcher`）を必ず持つ |
| 停止 | 2 回の `context.stop(self)`（ActorModelSpec「not double-deregister」） | 登録の解除は 1 回 |
| 停止 | Create を処理する前の停止、子が作られている途中の親の停止 | T1 の待ち合わせで、子の終わりを待つ |
| 停止 | アクターのスレッドからの `terminate`、`Drop` がプールのスレッドで起きる | 自分を join しない（T8） |
| 停止 | 永遠に待つアクターがいるときの終了 | `terminate` は返る。スレッドはそのアクターが返るまで残る（原典と同じ） |
| 再利用 | 止まった dispatcher への新しいアクターやクロージャ | 実行器を作り直す（T11） |
| 量 | 1 万のアクターの生成と停止を 3 回（ActorModelSpec「handle waves of actors」） | 段 5 で時間とメモリを測る |
| 誤用 | 2 つのアクターに同じ Pinned を使う | 2 つ目の登録を断る（原典の `IllegalArgumentException`） |
| 誤用 | 未登録の dispatcher の id | §3.6 |
| 名前 | システム名が Unicode、dispatcher の id に `.` や `-` を含む | スレッドの名前に、そのまま入れる |

### 7.2 性能への影響

- 段 1：`Rc` から `Arc` への置き換えは、システムのハンドルの複製だけに効く（制御の側で、メッセージごとではない）。
  `RefCell` の借用が `spin::Mutex` の `try_lock` に替わり、`ActorSystem` の呼び出しごとに競合の無い CAS が 1 回増える。
  試作で 20 万件のメッセージを 7 回ずつ流した中央値は、外からの `tell` で 97.9 → 99.5 ns、自分への `tell` を
  `run_until_idle` で 83.6 → 84.3 ns、`run_one` の繰り返しで 88.1 → 85.0 ns だった（§10 の 5）。差は ±3% の中で、
  向きもそろわないので、差は無いと見る。試作は `lock` で測った。段 1 で使う `try_lock` も、競合が無いときは同じく CAS 1 回である
- 段 3：宛先の線形の検索（アクターの数に比例）が無くなり、送り先に直接積む。アクターが多いほど速くなる見込み。
  代わりに、送るたびにキューのロックを 1 回取り、メールボックスが idle なら状態の CAS を 1 回行う
- 段 5：スレッドをまたぐと、起こすのに `Condvar` の通知が要る（マイクロ秒の単位）。throughput で、1 回の起床あたりの
  処理の件数を増やして薄める。kernel の `spin::Mutex` は、競合が多いとスピンで CPU を使う。区間は出し入れだけに
  限るが、段 5 で計測し、問題なら std の `Mutex` を port 越しに使う
- メモリ：アクターごとに `Arc<Mailbox>` 1 つ（状態、キュー 2 本、セルのロック）。段 5 の 1 万のアクターの計測で確かめる

### 7.3 PBT は要る

| 性質 | 段 | 入力の領域 | 除外 |
|:---|:---|:---|:---|
| P1 `MailboxStatus` の操作列が、参照の模型（Quint の `st` の状態の部分を Rust で書いたもの）と同じ結果と状態になる | 2 | `set_as_scheduled`、`set_as_idle`、`suspend`、`resume`、`become_closed`、`should_process_message`、`can_be_scheduled` の長さ 0〜64 の列。suspend の数は 0〜5 を網羅 | なし |
| P2 キューの出す順：FIFO は `VecDeque` の模型と一致。priority は優先度の順（stable は同じ優先度の中で積んだ順）。control-aware は control を先に FIFO、残りを FIFO。bounded は容量を超えない | 2 | 長さ 0〜容量＋2、優先度の重複を含む、積むと出すの交互 | なし |
| P3 閉じたあとの `enqueue` は必ず返され、`close_and_drain` が返す件数と合わせて、積んだ件数と一致する | 2 | 積む、出す、閉じるの列 | なし |
| P4 stress：N 人の送り手が K のアクターに M 件ずつ送る。各アクターで送り手ごとの順が保たれ、件数が一致し、同じアクターの実行が重ならない（Pekko の `DispatcherActor` の `busy` と同じ見張り） | 5 | スレッド 1、2、4、8。throughput 1、5、`u32::MAX`。seed を記録 | なし。乱数の stress で、縮小はしない |

P4 は、縮小のできる PBT ではなく、seed を記録する stress テストである。I10（メモリの見え方）は、P4 と
ConsistencySpec で確かめる。段 2 では、`MailboxStatus` に限って loom での検査も選べる（kernel の atomics を
`cfg(loom)` で差し替える）。要るかは段 2 の設計で決める。

### 7.4 上流・保存済みデータ・公開 API・守りの仕組みへの影響

- 公開 API：`ActorSystem`、`WeakActorSystem`、`EventStream` が `Send + Sync` になる（足すだけ）。`Actor: Send`、
  `change_behavior`／`push_behavior` の関数に `Send`、`on_termination` の関数に `Send`、`Fsm<S, D>` に
  `S: Send, D: Send` が要る（狭める変更。リポジトリの中での影響は §3.3）。`ActorSystem` の既存のメソッドの名前と
  意味は変えない
- 保存済みのデータ：無し
- 上流：無し（このリポジトリの中だけ）
- 守りの仕組み：CI の `quint` ジョブの対象が 4 本になる。手元でスクリプト全体が 3.9 秒から 6.4 秒になった。syn-check の規約
  （`type-per-file`、`cfg-std-forbid`、`port-adaptor-boundary`）に合わせて、§3.2 の型は 1 ファイル 1 型、
  std を使うものは adaptor に置く

## 8. 原典の Spec のうち、何が確かめられるようになるか

「段」は、そのケースを移植できるようになる §6 の段である。「人間の判断」は除外や `not_built` の候補で、決めるのは
人間である。この設計は提案までで、台帳には書かない。

| Spec（ケースの数） | ケース | 段 | 備考 |
|:---|:---|:---|:---|
| ConsistencySpec（1） | provide memory consistency | 5 | アクターの数は原典の式（`scaled_pool_size(1, 1.5, 2000) + 1`）。throughput = 1。元帥の条件（同時に 2 つ以上のスレッドで動いたこと）は、プールが数える「同時に実行中だったメールボックスの最大数」が 2 以上であることで記録する。数えるのは Relaxed の原子操作で、アクターのメッセージごとには触らない（#230 の再レビューの指摘：測る道具が見え方の誤りを隠さないように） |
| DispatcherActorsSpec（1） | not block fast actors by slow actors | 5 | |
| PinnedActorSpec（2） | support tell、support ask/reply | 5 | |
| DispatcherActorSpec（4） | support tell、support ask/reply、respect the throughput setting、respect throughput deadline | 5 | スレッド 1 本の固定プール |
| ControlAwareDispatcherSpec（2） | unbounded、bounded | 5 | キューは段 2 |
| PriorityDispatcherSpec（2） | unbounded、bounded | 5 | 同上 |
| StablePriorityDispatcherSpec（2） | unbounded、bounded | 5 | 同上 |
| MailboxConfigSpec（複数クラス） | MailboxSpec の 8 ケース × Default、Priority、StablePriority、ControlAware、SingleConsumerOnly。CustomMailboxSpec の 1、SingleConsumerOnlyMailboxVerificationSpec の 2 | 2、5 | キューの単体のケースは段 2。bounded の push の待ち、溢れたときの DeadLetter、複数の消費者は段 5。SingleConsumerOnly は Pekko の `AbstractNodeQueue` を前提にする。同じ種類のキューを作るか、`not_built` の候補にするかは人間の判断 |
| DispatcherShutdownSpec（1） | eventually shutdown when used after system terminate | 6 | 原典は JVM の ThreadMXBean でスレッドの名前を数える。Rust には同じものが無いので、プールが数える生きたスレッドの数に置き換える。これは前提の違いで、人間の判断 |
| ActorModelSpec の共通のケース（9 定義） | dynamically handle its own life cycle | 6 | 観測口の `shut_down` の回数 |
| | process messages one at a time、handle queueing from multiple threads | 6 | 観測口 |
| | not process messages for a suspended actor | 6 | 外からの suspend は内部 API。移植の規約どおり単体テストに置き、台帳の `rust.cases_outside_test_file` に書く |
| | handle waves of actors | 6 | boss は Pinned |
| | continue to process messages when exception is thrown | 6 | 段 4b の再起動 |
| | not double-deregister | 6 | |
| | continue to process messages when a thread gets interrupted and throws an exception | — | JVM の Thread の interrupt。人間の判断（除外か `not_built` の候補） |
| | continue to process messages without failure when a thread gets interrupted and doesn't throw an exception | — | 同上 |
| DispatcherModelSpec（共通 9 ＋ 1） | process messages in parallel | 6 | |
| BalancingDispatcherModelSpec（共通 9 ＋ 1） | process messages in parallel | 7a | 共通のケースも 7a |
| BalancingDispatcherSpec（1） | have fast actor stealing work from slow actor | 7a | 原典はアクターの中身（`invocationCount`）を読む。共有のカウンタで写す |
| CallingThreadDispatcherModelSpec（共通 9） | ActorModelSpec の共通のケース | 7b | interrupt の 2 ケースは上と同じ |
| DispatchersSpec（22） | use specific id、have only one default dispatcher、provide lookup of dispatchers by id、provide lookup of aliased dispatchers、complain about missing aliased dispatchers、complain about missing config、get the correct types of dispatchers、use defined properties | 5 | 設定はコードで与える |
| | include system name and dispatcher id in thread names（fork-join、thread-pool、default、pinned、balancing の 5） | 5、7a | balancing は 7a |
| | throw ConfigurationException if type does not exist | — | コードで組む設定では、無い種類を書けない（enum で選ぶ）。人間の判断（`not_built` の候補） |
| | get config for dispatcher、aliased、return empty config ×2（4） | 後回し | 設定の層を作るまで（§1.3） |
| | use dispatcher in deployment config ×2、pool-dispatcher router、balancing-pool router（4） | 後回し | deployment と router のあと |
| ExecutionContextSpec（14） | SerializedSuspendableExecutionContext の 4 ケース | 6 のあと | その型を作れば移せる。作るかは移植の issue で決める |
| | be instantiable、Batching、reentrant batch、starvation、`blocking{}` ×2、parasitic、same-thread ×3（10） | — | Scala の ExecutionContext、Future、BlockContext の機能。人間の判断（`do_not_port` の候補） |
| SystemMessageListSpec（6） | 6 ケース | — | 原典の `LatestFirstSystemMessageList` と `EarliestFirstSystemMessageList`（侵入型の連結リスト）を対象にする。この設計の system message のキューはロックつきの `VecDeque` で、同じ型を作らない。同じ型を作るか、`not_built` の候補にするかは、§9 の D4 |

## 9. 上司に決めてもらう点

| 番号 | 問い | 選択肢 | 推奨 | 戻し方 |
|:---|:---|:---|:---|:---|
| D1 | 実行器をどう移すか | (A) 今の実行器の中身を段ごとに入れ替える（§3.1）。(B) 新しい実行器を横に作り、最後に既存のテストを移して古い方を消す | (A)。どの段でも既存の 1077 件のテストが同じ実行器で走り、意味論が 1 か所にとどまる。(B) は新しい側を既存のテストで確かめられない期間が長い | 段 3 までは小さく、どちらにも戻れる |
| D2 | `ActorRef` が `Arc<Mailbox>` を指すことを、所有権モデルの「送信端」として認めるか | (A) 認める（§3.4）。(B) `ActorRef` は id＋世代のままにし、配送のたびに並行の表（id → メールボックス）を引く | (A)。今も `ActorRef` は `Arc<dyn ShardBoundary>` を持っていて、持つ Arc の数は変わらない。(B) は配送ごとに表のロックと検索が増える | 段 3 の中で閉じる |
| D3 | dispatcher の移植の Spec で、人間の判断が要るケース（§8 の「—」）を、いつ誰に上げるか | (A) 各 Spec の移植の issue で、その場で提案として上げる。(B) 今まとめて上げる | (A)。段 5 より前に決める必要は無い | — |
| D4 | SystemMessageListSpec の扱い | (A) system message のキューは `VecDeque` にし、この Spec は `not_built` の候補として人間に出す。(B) 原典と同じ侵入型の連結リスト（compare-and-set の Treiber stack）を kernel に作り、Spec を移す | (A)。キューの区間は短く、ロックつきで足りる。連結リストは unsafe と原子操作の検査が増える。性能で要ると分かったら (B) へ | 段 2 の中で閉じる |
| D5 | 段 1 で、ロックを取れなかったときの振る舞い | (A) panic する（今の `RefCell` の二重借用と同じ）。(B) 待つ（再入では止まったままになる） | (A)。段 1 は挙動を変えない段なので、今と同じ panic を保つ。同時に使える形は段 3 以降で作る | 段 1 の中で閉じる |

D1、D2 は、この設計の骨組みを決める。D3〜D5 は、推奨で進めてもあとで戻せる。

## 10. 動かしたコマンドと結果

試作はすべて scratchpad の写しで行った。3 は main の `d842842` の写しで行った。1、2、5 は 1 つ前の `d435d7b` の写しで行った。その間の差分は kernel に `glob`、`pretty_duration`、`switch` を足しただけで、`executor`、`actor_system`、`event`、`actor` には触れていない。作業ツリーで変えたのは、この PR の 2 ファイルだけである。

| 番号 | コマンド | 結果 |
|:---|:---|:---|
| 1 | 写しの `tests/` に `assert_send::<ActorSystem>()`、`WeakActorSystem`、`EventStream`、`assert_sync::<ActorSystem>()`、`ActorRef`、`Props` を置き、`cargo check -p fraktor-actor --test zz_send_probe` | E0277 が 9 件。原因は `Rc<RefCell<Executor>>`、`rc::Weak<RefCell<Executor>>`、`Rc<RefCell<StreamSetup>>` の 3 つ。`ActorRef` と `Props` は通る |
| 2 | 写しの `executor_value.rs` に `assert_send::<Executor>()` などを置いて `cargo check -p fraktor-actor --lib` | E0277 が 8 件。`dyn Actor`、`become` の枠、終了の通知、`EventStream` の中身 |
| 3 | 写しに §2.3 の置き換え（`Arc<spin::Mutex<_>>`、`Send` の境界）を入れて `cargo test --workspace` | main の d842842 の写しで、92 個のテストのバイナリの 1078 件が通り、失敗は 0。元のコードは 91 個で 1077 件。差の 1 件は 1 の検査で、置き換えのあとは通る。書き換えたテストは単体テストの 2 ファイル（5 か所の `borrow` を `lock` に）だけ |
| 4 | `bash scripts/quint-check-specs.sh`（リポジトリの写しに quint 0.32.0 を `npm ci` で入れて） | exit 0。`mailbox_scheduling.qnt` は 14 passing、`[ok] No violation found`、`Trace length statistics: max=21`。既存の 3 本も通る |
| 5 | 元のコードと、3 と同じ置き換えを入れた写し（どちらも `d435d7b`）で、`cargo test --release --test zz_stage1_bench`（20 万件 × 7 回の中央値） | 外からの `tell`：97.9 → 99.5 ns。自分への `tell` を `run_until_idle`：83.6 → 84.3 ns。`run_one` の繰り返し：88.1 → 85.0 ns |
| 6 | 仕様の写しに `initLostWakeup` などを足し、`quint run --init=init<モード> --invariant=invariant --max-steps=30 --max-samples=20000` | 4 つのモードとも `[violation] Found an issue`（seed は LostWakeup `0x1b40dfc978df975a`、CheckThenAct `0xf34588dfe384ca9a`、ExternalSuspend `0xcf8dec869c26a400`、LateEnqueue `0x92d5e3a0950a5673`） |
| 7 | 正しい形を `quint run --invariant=invariant --max-steps=60 --max-samples=100000` | `[ok] No violation found`、`max=45` |
| 8 | `timeout 1500 quint verify --invariant=invariant --max-steps=12`（Apalache 0.56.1、正しい形） | `The outcome is: NoError`（12 手までのすべての状態で 8 つの不変条件が成り立つ）。約 25 分かかった。結果を出した直後に `timeout` の打ち切りに当たり、終了コードは 124。CI には入れない（`quint-in-ci.md` の方針） |

429、rate limit、overloaded は出なかった。
