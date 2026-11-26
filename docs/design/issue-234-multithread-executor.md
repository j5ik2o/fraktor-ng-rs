# 設計：#234 1 つの ActorSystem を複数のスレッドで回す実行器（dispatcher の移植の前提）

[Issue #234](https://github.com/j5ik2o/fraktor-ng-rs/issues/234) の設計である。親は
[Issue #179](https://github.com/j5ik2o/fraktor-ng-rs/issues/179)（台帳の unported を 0 件にする）。原典は
Pekko v2.0.0（commit `9fbd2283b1cf701c90a1223bf621942c191db61f`）。

状態：設計の案。設計の独立レビューは実施済み（2026-11-26、固定 `e702bbe`、判定は要修正。高 2・中 8・低 8）。
指摘の処理と、それで変わった箇所は §11 にある。D1 は決定済み（元帥、2026-11-26）。D2 と、段 1 の実装
（main に入れること）は、オーナーの判断待ちである。この issue では実装しない。実装は §6 の段ごとに別の issue に分ける。

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

### 2.4 段 1 で Arc が増える場所

段 1 で `Rc` を `Arc` に替えるのは、次の 4 か所だけである。どれも、システムの制御の側の値（システムそのもの、
event stream の設定、バスの購読表）を共有するためのハンドルである。セル、メールボックス、`ActorRef`、配送の経路には
入らない。所有権モデルの言葉では、「スレッドの境界」の側の変更である。

| 場所 | 今 | 段 1 | 数 | 参照の数が動くとき |
|:---|:---|:---|:---|:---|
| `ActorSystem.executor`（`actor_system_value.rs:46`、`:66`） | `Rc<RefCell<Executor>>` | `Arc<spin::Mutex<Executor>>` | システム 1 つに 1 つ | 作るときと捨てるときだけ。`ActorSystem` のメソッドは参照の数を変えない（ロックの `try_lock` で CAS が 1 回） |
| `WeakActorSystem.executor`（`weak_actor_system.rs:29`） | `rc::Weak` | `sync::Weak` | `downgrade` のたびに 1 つ。今の呼び出し元は `actor_system_value.rs:67`（event stream の分）、`:193`、`adaptor/ask_driver.rs:41`、testkit の `test_kit_value.rs:52`、`test_probe.rs:62`、`test_latch.rs:123` | メソッドを呼ぶたびの `upgrade`（原子操作の増減 1 組）。testkit は `WeakActorSystem::run_one` でシステムを回すので、testkit が回すときは 1 メッセージごとに 1 組になる（§7.2 で測った） |
| `EventStream.setup`（`event_stream.rs:63`、`:72`） | `Rc<RefCell<StreamSetup>>` | `Arc<spin::Mutex<StreamSetup>>` | システム 1 つに 1 つ。`EventStream` を複製するたびに参照が 1 増える | 複製と、設定を読み書きするとき（購読、ロガーと unsubscriber の開始）。`publish` の経路では触らない |
| `ActorEventBus.subscriptions`（`actor_event_bus.rs:49`、`:63`、`:65`） | `Rc<RefCell<Subscriptions>>` と `rc::Weak` | `Arc<spin::Mutex<Subscriptions>>` と `sync::Weak` | バス 1 つに 1 つ。終了の通知の中に `Weak` が 1 つ | 購読、解除、終了の通知のとき |

この変更も、Arc をシステムの中に広く入れることになり、所有権モデルの決定に触れる。そのため、段 1 の実装
（main に入れること）は、D2 とあわせてオーナーの答えを待つ（元帥の訂正、2026-11-26）。

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

この方針は D1 として決まった（元帥、2026-11-26）。条件が 2 つ付いている（§9.1）。1 つ目は、段 3 の設計の前に、
順序に頼っている既存のテストの一覧と、その直し方を作ること。2 つ目は、段 2 の型の形を、Balancing と、std の
ロックも使える形に合わせることである。

### 3.2 kernel（no_std）と adaptor（std）の分け方

crate に `#![no_std]` は無い。kernel を no_std に保つのは、規約と syn-check の `cfg-std-forbid` による
（kernel では `use std::` を使わない）。この設計でも同じに守る。

| 置き場所 | 型（名前は案） | 中身 |
|:---|:---|:---|
| kernel `dispatch/mailbox_status.rs` | `MailboxStatus` | `AtomicU32` 1 つ。bit 0 が closed、bit 1 が scheduled、bit 2 から上が suspend の数（Pekko `Mailbox.scala:47-55` と同じ配置）。`set_as_scheduled`、`set_as_idle`、`suspend`、`resume`、`become_closed` はどれも compare-and-set の繰り返し |
| kernel `dispatch/message_queue.rs` | trait `MessageQueue: Send + Sync` | `enqueue(Envelope)`、`dequeue`、`has_messages`、`number_of_messages`、`drain`。**閉じたかどうかは持たない**（下の「閉じる門」） |
| kernel `dispatch/…` | `UnboundedQueue<R>`、`BoundedQueue<R>`（待たない）、`PriorityQueue<R>`（stable の有無）、`ControlAwareQueue<R>` | `VecDeque<Envelope>` を、型引数 `R: lock_api::RawMutex` のロックで守る（下の「ロックは port にする」） |
| kernel `dispatch/system_message.rs` | enum `SystemMessage` | Create、Suspend、Resume、Recreate、Terminate、Supervise、Watch、Unwatch、Failed、DeathWatchNotification、ChildTerminated |
| kernel `dispatch/mailbox.rs` | `Mailbox` | 状態、閉じる門、通常のキュー（`Arc<dyn MessageQueue>`。Balancing では team で共有）、system message のキュー、`ActorCell` を入れる場所、dispatcher への戻りの辺（§3.4 の条件 1 により `Weak` か id） |
| kernel `dispatch/message_dispatcher.rs` | `MessageDispatcher` | id、throughput、期限、住人の数、観測口、`register_for_execution`（Pekko `Dispatcher.scala:146` の形）、Balancing の team |
| kernel `dispatch/task_executor.rs` | trait `TaskExecutor: Send + Sync`（port） | `execute(Task) -> Result<(), Rejected>`、`shutdown`。`Task` はメールボックスの 1 回の実行か、`Box<dyn FnOnce() + Send>`。断られたときの扱いは §3.5 |
| kernel `dispatch/manual_executor.rs` | `ManualExecutor` | 実行待ちのメールボックスを `VecDeque` に積むだけ。`run_one` が 1 つ取り出して回す。今の決定論的な回し方の後継 |
| kernel `dispatch/dispatchers.rs` | `Dispatchers`、`DispatcherConfig` | id から dispatcher への表、別名、既定の id。設定はコードで組む |
| kernel `dispatch/dispatcher_observer.rs` | trait `DispatcherObserver: Send + Sync` | 既定は何もしない `registered`、`unregistered`、`suspended`、`resumed`、`dispatched`、`shut_down` |
| kernel（既存） | `Clock` | throughput の期限を測る。今の port をそのまま使う |
| kernel（新しい port） | trait `Scheduler: Send + Sync` | 一度だけ後で動かす（`schedule_once(delay, task) -> Cancellable`）。dispatcher の停止の猶予（T9〜T11）に使う。原典は `prerequisites.scheduler.scheduleOnce`。adaptor はタイマーのスレッドで、testkit は今の `ManualScheduler` で実装できる |
| kernel `dispatch/wake_signal.rs` | trait `WakeSignal: Send + Sync`（port） | `notify`、`wait(max)`。手で回す実行器に仕事が積まれたとき、または test actor にメッセージが届いたときに鳴らす。段 3 から使う（§3.6） |
| adaptor `thread_pool_executor.rs` | `ThreadPoolExecutor` | 固定の N 本のスレッド、`Mutex<VecDeque<Task>>` と `Condvar`。スレッドの名前は `<システム名>-<dispatcher id>-<番号>`。生きたスレッドの join の完了を数える（T8、§8 の DispatcherShutdownSpec） |
| adaptor | `ForkJoinConfig::scaled_pool_size(min, factor, max)` | `ceil(コア数 × factor)` を `[min, max]` に収める（`ThreadPoolConfig.scaledPoolSize` と同じ式。ConsistencySpec が使う） |
| adaptor | `PinnedExecutor` | スレッド 1 本。持ち主のアクター以外の登録を断る（Pekko `PinnedDispatcher.scala` の `register`） |
| adaptor | `BlockingBoundedQueue` | 満杯のとき push の待ち時間だけ待つ bounded キュー（`Condvar` が要るので std） |
| adaptor | `CondvarSignal` | `WakeSignal` の std の実装 |
| adaptor | std のロック | `lock_api::RawMutex` の実装で、待つときにスレッドを休ませるもの（下の「ロックは port にする」） |
| testkit | `CallingThreadDispatcher` | 送った側のスレッドで処理する。スレッドごとの再入のキューは `thread_local!` なので std の testkit に置く（Pekko でも testkit にある） |

**閉じる門（設計レビュー M4）**：閉じたかどうかは、キューではなく `Mailbox` が持つ。送るときは、門のロックの中で
「閉じているか」を見てから積む。閉じるときは、同じロックの中で閉じる。こうして I6（閉じたかを見ることと積むことを
分けない）を得る。閉じたあとの扱いは、キューが自分専用か共有かで分ける。

| キュー | 閉じるときにすること |
|:---|:---|
| 自分専用（Balancing 以外） | 残りの通常のメッセージを dead letters へ流す |
| Balancing の共有のキュー | 流さない。team の残りが処理する（原典 `BalancingDispatcher.scala:75-89` の `SharingMailbox.cleanUp`）。止まった一員宛てに後から来たものも共有のキューに積む（`:106-109`） |
| system message のキュー（どちらでも） | 残りを dead letters へ流す。閉じたあとに来たものも dead letters へ回す（§4.1 の I12） |

自分専用のキューでは、門とキューを同じロックで守ってよい（送るたびのロックは 1 回で済む）。

**ロックは port にする（設計レビュー M5）**：kernel のキューと表のロックは、型引数 `R: lock_api::RawMutex` で
受け取る。kernel の既定は `spin` のロック（no_std で使える）で、std の adaptor は、待つときにスレッドを休ませる
ロックを渡す。ConsistencySpec はわざとコア数より多いスレッドを使う（`scaledPoolSize(1, 1.5, 2000)`）。既定の
dispatcher も `parallelism-min = 8` である（`reference.conf:478`）。スレッドがコアより多いと、ロックを持った
スレッドが OS に止められ、待つ側はスピンし続ける。`spin` のロックは公平でないので、送り手が多いと取り出しが飢える。
そのため、複数スレッドの dispatcher では spin を使わない。std の側で何を渡すか（`parking_lot::RawMutex` を
依存に足すか、std の `Mutex` と `Condvar` で書いた自前の `RawMutex` か）は、段 2 の設計で決める。

**ロックの決まり**：

| 順 | ロック | 持ってよい間 |
|:---|:---|:---|
| 1 | `ActorCell` のロック | 1 回の実行の間（`receive` などの利用者のコードをまたぐ）。scheduled の bit を持つスレッドしか取りに行かないので、取り合いは起きない。`try_lock` が失敗したら I1 の破れとして panic する |
| 2 | システム全体で共有する表（function ref、終了の通知、dispatcher の登録表など） | 読み書きの間だけ |
| 3 | メールボックスの閉じる門とキュー | 出し入れの間だけ |
| 4 | event bus の購読表 | 読み書きの間だけ |

- 取る順は上から下である。2〜4 は、同時に 2 つ以上持たない。
- 2〜4 を持ったまま、利用者のコードを呼ばない。`tell` もしない。宛先を写してからロックを外し、外したあとで送る。
  今の `EventStreamBus::publish_through`（`event_stream_bus.rs:118-121`）がこの形である。段 4 で共有の表を足すときも、
  これを守る。
- 待ち（スレッドの休止、bounded の push の待ち、testkit の待ち）は、どれも adaptor の `Condvar` で行う。

### 3.3 Send と Sync の境界

| 種類 | 型 |
|:---|:---|
| `Send + Sync` | `ActorSystem`、`WeakActorSystem`、`EventStream`（段 1 から）。`ActorRef`、`Props`、`Message`（今も）。`Mailbox`、`MessageDispatcher`、`Dispatchers`、システム全体で共有する表 |
| `Send` だけ | `Box<dyn Actor>`、`ActorCell`、`become` の枠、`Envelope`。一度に 1 つのスレッドだけが触る |
| Send にしない | `ActorContext<'a>`（1 回の実行の間だけ借りる）、testkit の期限の `Rc` |
| Send にならないまま | `ActorEventBus`。段 1 で購読表を `Arc` にしても、`classify: Box<dyn Fn(&E) -> ActorRef>` を持つので !Send のままである。システムが持つ値ではないので、害は無い（設計レビュー L8） |

`Actor: Send` は、公開 API を狭める変更である。`Rc` や `RefCell` を持つアクターは作れなくなる。試作 3 では、
リポジトリの中でコンパイルが通らなくなったのは単体テスト 2 ファイルの 5 か所だけで、どれも `borrow` から `lock` への
書き換えで済んだ。ほかの既存のテストは、書き換えなしでそのまま通った（§10 の 3）。Pekko のアクターも、
どのスレッドで動くかを選ばない。

### 3.4 ActorRef とメールボックス（D2：オーナーの判断待ち）

段 3 から、`ActorRef` が持つ `boundary: Arc<dyn ShardBoundary>` を、送り先を表す値に替える案である。

```rust
// 案。名前は段 3 で決める。
enum Endpoint {
  Mailbox(Arc<Mailbox>),             // セルのアクター
  Boundary(Arc<dyn ShardBoundary>),  // function ref、no-op、ほかのシステム
}
```

`tell` は、`Mailbox` なら門の中で `enqueue` してから `register_for_execution` を呼ぶ。等しさとハッシュは、今と同じく
id と世代だけで決める。宛先を探す線形の検索（§2.2 の 1）は無くなる。

**これは、承認済みの所有権モデルを改める変更である。**決定文書（`docs/decisions/ownership-model-arc-overhead.md`、
accepted 2026-11-11）は、次のように決めている。

- 「Arc / Rc は … ホットパス（cell / mailbox / ref / 配送）には置かない」（`:106`）
- スレッド境界を越える送信端の Arc は認めるが、「『境界』に閉じ込められ、シャード内のホットパスには入らない」（`:96-97`）
- 移植の規約の要約（`.takt/facets/knowledge/fraktor-porting-rules.md` の「所有権モデル（accepted）」）も、
  「ActorRef は id＋世代番号の値」「親子・DeathWatch・購読は … 所有テーブル／登録表」としている。矛盾したら決定文書が優先する

この案では、アクターごとに `Arc<Mailbox>` があり、その中に `ActorCell` を入れ、`ActorRef` がそれを指す。メールボックスと
ref と配送の経路に Arc が入る。所有権モデルのスパイクで「実装 1 Arc 全面型」として測られ、単一スレッドで 7〜15 倍
遅かった形に近い（同じ文書の `:132-155`）。持つ Arc の数が今と同じでも、ホットパスで実体を指すことに変わりはない。
そこで §9.2 の D2 は、「送信端として認めるか」の確認ではなく、「決定文書を改めるか」の判断としてオーナーに出している
（元帥経由、2026-11-26）。答えが出るまで、段 3 には入らない。

案 (A) を採る場合は、次の 4 つを条件にする（設計の独立レビューの推奨）。

| 条件 | 中身 | 確かめ方 |
|:---|:---|:---|
| 1 | `Mailbox` から dispatcher、システム、dead letters へ戻る辺は、`Weak` か id にする。強い循環を作らない | 段 3 の設計の型で見る |
| 2 | 閉じるとき（I6）に、自分専用のキューを必ず dead letters へ流す。Envelope の送り手（`ActorRef` → `Arc<Mailbox>`）による循環を断つため。今の `stop_all` の doc（`executor_value.rs:1113-1119`）が、境界について同じ理由を書いている | 段 3 の完了条件に、終了したあとにメールボックスが解放されること（`Arc::strong_count` か、`Weak::upgrade` の失敗）のテストを入れる |
| 3 | 段 3 の完了条件に、計測の関門を入れる。今の 1 メッセージあたり約 80〜100 ns（§7.2）と比べ、アクターが多い場合も含めて測る | 段 3 の PR に、同じ計測の前後の数字を載せる。悪くなる条件があれば、段 3 を止めて上に上げる |
| 4 | 決定文書の「Arc を置いてよい場所」に、複数スレッドの dispatcher のメールボックスと、その予算（アクター 1 つに Arc 1 つ）を書き足す | オーナーの承認のあと、決定文書の改訂を段 3 より先に入れる |

案 (B) は、`ActorRef` を id＋世代のままにし、配送のたびに並行の表（id → メールボックス）を引く形である。決定文書は
変えずに済むが、`tell` のたびに表のロックと検索が要る。止まったアクター宛てを見分けるのにも、表の世代の検査が要る。

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

期限は、2 の繰り返しのたびに `Clock` で測る。期限を過ぎても、取り出した 1 件は最後まで処理する。

**実行器に断られたとき（設計レビュー M2）**：`register_for_execution` が compare-and-set に勝ったあと、
`TaskExecutor::execute` が `Rejected` を返すことがある（T7 で止めた直後や、T11 で作り直す前）。原典
`Dispatcher.scala:146-172` と同じく、1 回だけ試し直す。それも断られたら、scheduled の bit を下ろし（`set_as_idle`）、
event stream に `Error` を出して、呼び出し元にエラーを返す。bit を立てたままにすると、そのメールボックスは
二度と予約されない（I2 と I3 が破れる。§4.1 の I15）。

### 3.6 既存のテストと testkit

- `ActorSystem::new(name, boundary)` の既定の dispatcher は Manual にする。今と同じく、テストのスレッドが回す。
  `run_one`、`run_until_idle`、`await_envelope` の名前と意味は変えない
- 未登録の dispatcher の id：今は `Props::with_dispatcher` の名前を記録するだけで、使わない
  （`tests/supervisor_misc_spec.rs:144` が `pinned-dispatcher` などを未登録のまま使う）。`ActorSystem::new` で
  作ったシステムでは、未登録の id は既定の dispatcher に回す（今の振る舞いを保つ）。dispatcher を登録して作った
  システムでは、Pekko と同じく、未登録の id で `actor_of` を断る
- testkit の待ち方：今の `TestKitBase::receive_one` は、`run_one` で 1 件回し、無ければ `await_envelope` で境界を
  待つ（`test_kit_base.rs:493-532`）。段 3 で、cell のアクター宛ての `tell` は境界を通らなくなる。そのままだと
  `await_envelope` は何も受け取らず、待ちは `POLL_INTERVAL` の眠りになる。複数スレッドのアクターは、テストの
  スレッドが回さなくても進む。そこで段 3 で、`await_one` を kernel の port `WakeSignal` を待つ形に替える。手で回す実行器に仕事が積まれたときと、
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
| I5 | 閉じたあとは `receive` を呼ばない。closed は抜けられない | 閉じるのは `finishTerminate` → `detach` の中（子のいないアクターなら Terminate を処理した実行、子がいれば最後の ChildTerminated を処理した実行。T1〜T3）。以後 `set_as_scheduled` は失敗する | `noInvokeAfterClose` |
| I6 | 閉じたメールボックスは何も持たない。閉じたあとの通常のメッセージは dead letters へ行く | 閉じる門（§3.2）の中で見てから積む。閉じるときに、自分専用のキューの残りを dead letters へ移す（Balancing の共有のキューは流さない） | `closedHoldsNothing` |
| I7 | 1 回の実行で処理するのは throughput 件まで。期限を過ぎたら次を取らない | 実行のループの数え方 | `throughputRespected`。仕様が見るのは throughput だけで、期限は段 2 と段 5 の単体テストで見る（時間を仕様に入れない） |
| I8 | 受け付けたメッセージは、処理済み、キューの中、処理中、dead letters のどれかにある | I6 と、閉じるときに dead letters へ流すこと | `messagesConserved` |
| I9 | 同じ送り手から同じ宛先への順序は保つ（FIFO のキューで）。priority のキューは優先度の順、stable なら同じ優先度の中で FIFO | キューの実装 | Quint には無い。PBT（§7.3） |
| I10 | あるメッセージの処理で書いた値は、次のメッセージの処理から見える。次の処理が別のスレッドでも見える | scheduled の bit の操作を AcqRel、キューのロックと `ActorCell` のロックで acquire と release がつながる | Quint は逐次一貫なので扱えない。ConsistencySpec と stress テスト（§7.3） |
| I11 | system message は、実行の始めと、通常のメッセージ 1 件ごとのあとに、届いている分をすべて処理する | §3.5 の 1 と 2 | 一部だけ（`processSystem`、`decideFinish` の前提） |
| I12 | system message も失われない。処理済み、キューの中、dead letters のどれかにある。閉じるときの残りと、閉じたあとに来たものは dead letters へ行く（DeathWatch のため。dead letters は Watch に DeathWatchNotification を返す） | 原典 `Mailbox.scala:318-338`、`:503-505` と同じ | `systemMessagesConserved` |
| I13 | プールに入ったメールボックスには、その実行でできる仕事が必ずある（suspend 中を通常のメッセージで予約しない） | `can_be_scheduled` | `submittedOnlyWithWork` |
| I14 | 何も進行中でない（送り手が途中にいない、実行も予約も無い、親の判断も待っていない）とき、開いたメールボックスは、できる仕事をやり終えている。suspend されているのは、外から頼まれたときだけ | I3 と、失敗のたびに親が Resume を返すこと | `settledMeansIdle` |
| I15 | 実行器に断られたメールボックスに、scheduled の bit は残らない | §3.5 の「実行器に断られたとき」 | `dispatcher_lifecycle.qnt`（段 6） |

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
| T1 | Terminate を受けたアクターは、まず自分が watch している相手をすべて unwatch する（原典 `FaultHandling.scala:185`。dead letters に Terminated が溜まらないように）。次に子のすべてに Terminate を送り、子の ChildTerminated がすべて揃うまで待つ。待つ間、通常のメッセージは処理しない |
| T2 | 子が揃ったら `post_stop` を呼ぶ。そのあと function ref を閉じる |
| T3 | dispatcher から外す（住人の数を 1 減らす）。メールボックスを閉じ、残りの通常のメッセージ、stash、system message を dead letters へ移す（I6、I12。Balancing の共有のキューは流さない） |
| T4 | 親へ ChildTerminated を、watcher へ Terminated（DeathWatchNotification）を送る。親が watcher でもあるときは、この順 |
| T5 | 終了の通知（`on_termination`）を呼ぶ |

したがって、watcher が Terminated を受けたとき、`post_stop` は終わっていて、そのアクターのメールボックスは閉じている。

システムの終了：

| 番号 | 順序 |
|:---|:---|
| T6 | `/user` の下を止め（T1 で深い方から）、次に `/system` の下を止める |
| T7 | 終わったら、登録した dispatcher のすべてを止める。新しい仕事を受け付けず、走っている仕事は最後まで走らせ、スレッドを終わらせる |
| T8 | `is_terminated` が true になるのは、T6 が終わったときである。スレッドを join で待ち切るのは `terminate` の呼び出し元ではない（アクターのスレッドから `terminate` を呼ぶと、自分を join して止まるため）。DispatcherShutdownSpec のように「いずれスレッドが 0 本になる」ことは、プールが作ったスレッドの join がすべて終わったことで確かめる。プールが自分で数える「生きている数」では、プールの勘定の外に漏れたスレッドを見逃すからである（設計レビュー L7） |

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

仕様の `Terminate` は、**子のいないアクターの停止のすべて**を表す。それを処理した実行が `finishTerminate` まで進み、
dispatcher から外して、メールボックスを閉じる。子のいるアクターは、子が全員いなくなるまで「開いていて、
suspend されている」状態で待つ（T1）。この状態はこの仕様には無く、`actor_termination.qnt`（段 4a、§5.4）で扱う。

| 仕様の動作 | 実装で当たるところ |
|:---|:---|
| `send`、`register` | `ActorRef::tell` → 門の中で `Mailbox::enqueue` → `MessageDispatcher::register_for_execution` |
| `control`、`decide` | system message を積んで予約する（外からの suspend、親の Resume）。閉じていれば dead letters へ回す |
| `take` | プールのスレッドがキューから 1 つ取る |
| `processSystem` | `Mailbox::process_all_system_messages`。`Terminate` は、子のいない場合の `finishTerminate` → `detach` → 閉じる。残りの通常のメッセージも system message も dead letters へ移す |
| `startInvoke`、`endInvoke`、`failInvoke` | 1 件の `receive`。失敗したら自分を suspend し、親に Failed を送る |
| `decideFinish`、`finishIdle`、`finishRegister` | `run` の終わり。`set_as_idle` とその後の予約 |

### 5.2 CI の決まりへの合わせ方

- 不変条件は `invariant`。§4.1 の I1〜I8、I12、I13、I14 の論理積である
- `run` は 17 本で、名前はどれも `Test` で終わる。`Test` で終わる名前は `run` にしか使っていない
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
| `DropSystemOnClose` | 閉じるときに残りの system message を捨て、閉じたあとの system message も捨てる。この仕様の最初の版（`e702bbe`）の誤りである（設計レビュー H2） | `systemMessagesConserved` | `dropSystemOnCloseLosesAWatchTest`（正しい形は `remainingSystemMessagesAreDeadLettersTest`、`systemMessageAfterCloseIsADeadLetterTest`） |

`LateEnqueue` は、Pekko でも起きうる取りこぼし（閉じた直後に積まれたメッセージが、dead letters にならずに消える）
である。この設計では、閉じる門の中で見てから積むので、起きない（I6）。

`DropSystemOnClose` を避ける理由は DeathWatch である。原典の `Mailbox.scala:318-338`（`processAllSystemMessages`）は、
「if we closed the mailbox, we must dump the remaining system messages to deadLetters (this is essential for
DeathWatch)」としている。閉じたあとの `systemEnqueue`（`:503-505`）も dead letters のメールボックスへ回す。
dead letters が Watch を受けると、DeathWatchNotification を返す。捨ててしまうと、Terminate と同じ実行の中で後ろに
入った Watch が消え、watcher には Terminated が永遠に届かない（`expect_terminated` が時間切れになる）。

手で並べた `run` だけでなく、ランダムな探索でも違反が見つかることを確かめた（§10 の 6、10）。誤りの 5 つのモードから
始めた `quint run`（20 手、1 万本）は、5 つとも違反を見つけた。設計レビューで探索が見逃した変異 3 つ（suspend 中の
予約、Resume を失う、親に Failed を伝えない）も、I13 と I14 を足した版では探索が見つけた。正しい形は、CI と同じ
条件（20 手、1 万本）でも、60 手・10 万本でも違反が無かった。最初の版は、Apalache による 12 手までの有界の検証でも
違反が無かった（§10 の 8。直した版では流していない）。

### 5.4 この仕様で扱わないもの（後の段で書く仕様）

| 仕様（案） | 段 | 状態と不変条件 | 負の対照 |
|:---|:---|:---|:---|
| `actor_termination.qnt` | 段 4a | T1〜T5。子の集合、「停止中で、開いていて、suspend されている」状態、post_stop、閉じたか、通知の順 | `post_stop` の前に Terminated を送る。子が残っているのに閉じる（子の ChildTerminated が閉じたメールボックスに届く） |
| `dispatcher_lifecycle.qnt` | 段 6 | 住人の数、停止の予定（UNSCHEDULED／SCHEDULED／RESCHEDULED）、実行器の生死、止めた回数。不変条件：住人がいる間は止めない、止めたあとの仕事は断らずに実行器を作り直す、断られたメールボックスの scheduled の bit は残らない | 作り直さない実行器（止めたあとの仕事が断られる）。断られたあとで bit を下ろさない（設計レビュー M2） |
| `balancing_team.qnt` | 段 7a | 共有のキュー、team の各メールボックス。不変条件：1 件のメッセージを処理するのは 1 人だけ、同じアクターが 2 スレッドで動かない、共有のキューに仕事があれば誰かが予約されているか予約しに行く、一員が止まっても共有のキューは流さない | 仲間が抜けるとき（`unregister`）に teamWork をしない。抜けた仲間が予約していた分が共有のキューに残り、誰も予約されない（原典 `BalancingDispatcher.scala:100-104` が teamWork を呼ぶ理由）。一員が止まるときに共有のキューを dead letters へ流す（M4） |

priority の順序、メモリの見え方（I10）、throughput の期限（I7 の後半）、CallingThread の再入は、Quint ではなく
単体テスト、PBT、stress テストで確かめる（§7）。

## 6. 段に分けた実装の計画

大きさは相対の見積もりである。基準は [PR #217](https://github.com/j5ik2o/fraktor-ng-rs/pull/217)（#207：kernel の
API 3 つと単体テスト、8 ファイル）を 2 とする。どの段も、設計の差分があれば先に設計の独立レビューを通す。
どの段も「既存のテストがすべて通る」を完了の条件に含む。

始める前に要るもの：段 1 と段 3 は、D2（§9.2）のオーナーの答えを待つ。段 2 は D2 に依らない（メールボックスの
型を作るだけで、`ActorRef` には触れない）。段 2 を先に始めるかどうかは、上司が決める。

| 段 | issue にする単位 | 中身 | 完了の条件 | 大きさ |
|:---|:---|:---|:---|:---|
| 1 | Send にする（挙動は変えない）。**オーナーの答えを待つ** | §2.3 の置き換え。Arc が増える場所は §2.4 の 4 か所だけ。`spin::Mutex` は `try_lock` で取り、取れなければ「再入または同時の利用」で panic する（今の `RefCell` の二重借用の panic と同じ振る舞いを保つ）。`ActorSystem`、`WeakActorSystem`、`EventStream` が `Send + Sync` であることのコンパイル時の検査を `tests/` に足す。`ActorSystem` と `WeakActorSystem` の rustdoc に「段 5 より前は、別のスレッドから同時に使うと panic する」と書く | 検査を先に足して E0277（9 件）で落ちることを確かめる（red）。置き換えのあと、検査と既存のテストのすべてが通る。`receive` の中から `WeakActorSystem` を呼ぶ再入のテストが、止まらずに panic で返る（設計レビュー M3。base でも段 1 でも通ることを、レビューが試作で確かめた）。§7.2 の計測で、性能の差が無いことを確かめる | 2 |
| 2 | kernel のメールボックス（配線しない） | `MailboxStatus`、`MessageQueue` と 4 種類のキュー、`SystemMessage` とそのキュー、閉じる門、ロックの型引数（§3.2）。どこからも使わない。D1 の条件 2 により、型の形は Balancing の共有のキューと std のロックを先に入れて決める | §7.3 の P1〜P3 の PBT。状態の操作が Quint の状態の部分と一致する。共有のキューを持つ 2 つのメールボックスで、片方を閉じても共有のキューが残ることの単体テスト | 3 |
| 3 の前 | 順序の洗い出し（D1 の条件 1） | 段 3（全体の FIFO から、アクターごとの順へ）と段 4a（子の `pre_start` が非同期になる）で変わる順序に、頼っている既存のテストを洗い出す。試作の写しで段 3 と段 4a の振る舞いを入れて既存のテストを流し、落ちたものを一覧にする。一覧の各行に、原典ではどう振る舞うか（同期か非同期か、順序を保証するか）を書く | 一覧が issue に載っている。各行について「実装で原典どおりに直す」「テストが原典より強い前提に頼っているので直す」のどちらかが決まっている。テストを直すものは、移植の規約（実装ステップはテストファイルを編集しない、移植したテストを弱めない）に従い、triage への差し戻しとして扱う。原典より弱めることになるものは、人間の判断にかける | 2 |
| 3 | アクターごとのメールボックスと Manual dispatcher。**D2 の答えを待つ** | `ShardQueue` の 1 本のキューを、アクターごとの `Mailbox` と `ManualExecutor` の実行待ちの列に替える。`ActorRef` の送り先を §3.4 の形にする（D2 が (A) のとき）。testkit の待ち方を `WakeSignal` に替える（cell のアクター宛ての `tell` が境界を通らなくなり、`await_envelope` が何も受け取らなくなるため。設計レビュー L3）。実行器の大きなロックと `&mut Executor` の文脈は、まだ残す | 既存のテストのすべてが通る（洗い出しの一覧で決めた直しを含む）。D2 が (A) なら §3.4 の条件 2（終了後にメールボックスが解放されるテスト）と条件 3（計測の関門）。testkit の待ちが `POLL_INTERVAL` の眠りに落ちていないこと（待ちの時間を測る） | 5 |
| 4 | アクターの間の操作を system message に（3 つに分ける） | 4a：生成と停止（Create、Terminate、子の名前、T1〜T5、閉じるときの dead letters）。4b：失敗と監督（Failed、Suspend、Resume、Recreate、all-for-one）。4c：DeathWatch と終了の通知、stop-and-reply、stash。文脈から `&mut Executor` を外し、自分のセルとシステム全体の共有の表を借りる形にする。共有の表は §3.2 のロックの決まりに従う | 各段で既存のテストのすべてが通る（洗い出しの一覧で決めた直しを含む）。4a で `actor_termination.qnt` を足す | 5、5、3 |
| 5 | std のスレッドプール | `ThreadPoolExecutor`、fork-join 相当の並列度、Pinned、`Dispatchers` の登録表、スレッドの名前、std のロック、`BlockingBoundedQueue`、dispatcher ごとのメールボックスの種類、プールが数える「同時に実行中のメールボックスの最大数」（ConsistencySpec の記録に使う。§8）。`execute` が断られたときの扱い（§3.5） | §7.3 の P4（stress）。メッセージの経路の計測（スレッドをまたぐ `tell`、アクターの数を増やした場合。設計レビュー L1）。ConsistencySpec（#235）、DispatcherActorsSpec、PinnedActorSpec、DispatcherActorSpec が移植できる状態 | 5 |
| 6 | dispatcher の寿命 | 住人の数、自動停止と作り直し、システムの終了で全部を止める（T6〜T11）、観測口、dispatcher でクロージャを実行する口、外からの suspend／resume、kernel の `Scheduler` の port（停止の猶予を数える。設計レビュー L4） | `dispatcher_lifecycle.qnt`（断られたあとで bit を下ろさない誤りを負の対照に含む）。DispatcherShutdownSpec、DispatcherModelSpec が移植できる状態 | 3 |
| 7 | Balancing と CallingThread | 7a：Balancing（共有のキュー、teamWork）と `balancing_team.qnt`。7b：testkit の `CallingThreadDispatcher` | BalancingDispatcherSpec、BalancingDispatcherModelSpec、CallingThreadDispatcherModelSpec が移植できる状態 | 3、3 |
| 8 | 後始末 | 使われなくなった `ShardQueue` と、`ShardBoundary` の役目の整理。文書の更新 | 既存のテストのすべてが通る | 2 |

最初の段（段 1）だけで、既存の挙動を変えずに Send にするところまで進む。試作で、その形の変更が既存のテスト
1077 件を書き換えなしで通すことを確かめた（§10 の 3、9）。

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
| 並行 | Terminate と同じ実行の中で、system message のキューの後ろに Watch が入る。閉じたあとに Watch が来る | どちらも dead letters へ行き、watcher に Terminated が返る（I12） |
| 停止 | 子のいるアクターへの Terminate | 開いたまま suspend して、最後の ChildTerminated で閉じる（I5、T1） |
| 並行 | 止めた直後の dispatcher、作り直す前の dispatcher への予約 | 1 回試し直し、だめなら bit を下ろしてエラー（I15） |
| Balancing | team の 1 人が止まる | 共有のキューは流さない。止まった一員宛てのメッセージは team の残りが処理する |
| 並行 | コア数より多いスレッドが同じキューを取り合う | std のロックで待つ（§3.2）。spin で回り続けない |

### 7.2 性能への影響

- 段 1：`Rc` から `Arc` への置き換えは、システムのハンドルの複製と `WeakActorSystem` の `upgrade` に効く（§2.4）。
  `RefCell` の借用が `spin::Mutex` の `try_lock` に替わり、`ActorSystem` の呼び出しごとに競合の無い CAS が 1 回増える。
  ロックを取るのは呼び出しごとで、メッセージごとではない。外からの `tell` は executor のロックを通らない。そのため、
  最初の計測（§10 の 5）の ±3% は「差が出ない構造」を確かめただけである（設計レビュー L1）。メッセージごとに
  増えるのは、testkit が `WeakActorSystem::run_one` で回すときの `upgrade`（原子操作の増減 1 組）とロックの CAS で、
  この経路を `try_lock` の形で測り足した（§10 の 11）。中央値は、元が 80.2〜81.0 ns、段 1 が 81.5〜82.2 ns で、
  1 メッセージあたり 1〜2 ns（1〜2%）増える。1 回目だけは段 1 が 92.6 ns だったが、同じ回はロックを通らない経路も
  含めて段 1 の側がすべて約 13% 遅く、機械の揺れと見て外した。段 3 と段 5 では、メッセージの経路（スレッドを
  またぐ `tell`、アクターの数を増やした場合）を完了の条件として測る（§6）
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
| P3 閉じたあとの送信は閉じる門で断られて dead letters へ行く。それと、閉じるときに流した件数と、処理した件数を合わせると、送った件数と一致する。共有のキューは、一員が閉じても流れない | 2 | 積む、出す、閉じるの列。キューが自分専用の場合と共有の場合 | なし |
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
- 守りの仕組み：CI の `quint` ジョブの対象が 1 本増える。ほかの仕様が 3 本のときに手元で測ると、スクリプト全体が 4.3 秒から 8.6〜10.4 秒になった（レビューのあとの版。2 回測った）。syn-check の規約
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
| MailboxConfigSpec（複数クラス） | MailboxSpec の 8 ケース × Default、Priority、StablePriority、ControlAware、SingleConsumerOnly。CustomMailboxSpec の 1、SingleConsumerOnlyMailboxVerificationSpec の 2 | 2、5 | キューの単体のケースは段 2。bounded の push の待ち、溢れたときの DeadLetter、複数の消費者は段 5。SingleConsumerOnly は Pekko の `AbstractNodeQueue` を前提にする。同じ種類のキューを作るか、`do_not_port` かケースの除外の候補にするかは人間の判断（Pekko の内部のデータ構造なので `not_built` には当たらない。設計レビュー L2） |
| DispatcherShutdownSpec（1） | eventually shutdown when used after system terminate | 6 | 原典は JVM の ThreadMXBean でスレッドの名前を数える。Rust には同じものが無いので、プールが作ったスレッドの join がすべて終わることで確かめる。これは前提の違いで、人間の判断。出すときは「プール自身が数える生きている数では、勘定の外に漏れたスレッドを見逃す」ことも添える（設計レビュー L7） |
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
| | be able to use Batching、continue the outer batch after reentrant batch execution、be able to avoid starvation when Batching is used and Await/blocking is called（3） | 6 のあと | Pekko 自身の `BatchingExecutor`（`dispatch/BatchingExecutor.scala`。`MessageDispatcher` が継承する。`AbstractDispatcher.scala:139-142`）を検査する。その型を作れば移せる（設計レビュー M6） |
| | run parasitic future callbacks on the dispatcher thread completing the promise（1） | 6 | `system.dispatcher.execute` が呼んだスレッドの外で走ることを見る。段 6 の「クロージャを実行する口」で移せる |
| | work with same-thread dispatcher plus blocking、work with same-thread dispatcher as executor with blocking（2） | 7b | 中身は CallingThread の再入と、CallingThreadDispatcher をまとめる実行器として使うこと |
| | be instantiable、work with tasks that use blocking{} multiple times、work with tasks that block inside blocking、work with same-thread executor plus blocking（4） | — | Scala の `ExecutionContext.fromExecutor`、`blocking {}`、`ExecutionContext.parasitic`。人間の判断（`do_not_port` の候補） |
| SystemMessageListSpec（6） | 6 ケース | — | 原典の `LatestFirstSystemMessageList` と `EarliestFirstSystemMessageList`（侵入型の連結リスト）を対象にする。この設計の system message のキューはロックつきの `VecDeque` で、同じ型を作らない。同じ型を作るか、`do_not_port` かケースの除外の候補にするかは、§9.3 の D4（`not_built` には当たらない。設計レビュー L2） |

## 9. 決めること

### 9.1 決まったこと

**D1：実行器の移し方は (A)。今の実行器の中身を段ごとに入れ替える（§3.1）。**元帥が決めた（2026-11-26）。
設計の独立レビューの推奨どおり、条件が 2 つ付いている。

| 条件 | 中身 | どこに書いたか |
|:---|:---|:---|
| 1 | 段 3 の設計の前に、順序に頼っている既存のテストの一覧と、その直し方を作る | §6 の段 3 の前の「順序の洗い出し」 |
| 2 | 段 2 の型の形を、Balancing の共有のキューと、std のロックも使える形に合わせる | §3.2 の「閉じる門」と「ロックは port にする」、§6 の段 2 |

選ばなかった (B) は、新しい実行器を横に作り、最後に既存のテストを移して古い方を消す形である。新しい側を既存の
テストで確かめられない期間が長くなるので、採らなかった。

### 9.2 オーナーの判断待ち

**D2：所有権モデルの決定文書を改めるか。**ホットパスに置く Arc の例外として、複数スレッドの dispatcher の
メールボックスを足すかどうかである。決めるのはオーナーで、元帥が伺っている（2026-11-26）。経緯と、案 (A) の
4 つの条件は §3.4 にある。

| 案 | 中身 | 設計の独立レビューの推奨 |
|:---|:---|:---|
| (A) | 決定文書を改める。`ActorRef` が `Arc<Mailbox>` を指す。§3.4 の条件 1〜4 を付ける | 推奨。理由は 3 つ。(B) は `tell` のたびに表のロックと検索が要る。止まったアクター宛ての扱いにも表の世代の検査が要る。Pekko の意味論（どのスレッドでも動く、Balancing）に近いのは (A) |
| (B) | 決定文書を変えない。`ActorRef` は id＋世代のままにし、配送のたびに並行の表を引く | — |

段 1 の実装（main に入れること）も、この答えを待つ（元帥の訂正、2026-11-26）。段 1 は、`Rc` を `Arc` にして
システムの中に入れる変更で、決定文書に触れるからである。段 1 で Arc が増える場所は §2.4 にある。答えが出るまでに
進めるのは、設計の直しと、試作での計測だけである。

### 9.3 推奨のまま進めてよいもの

どれも、あとで戻せる。

| 番号 | 問い | 推奨 | 付けた条件 |
|:---|:---|:---|:---|
| D3 | dispatcher の Spec で人間の判断が要るケース（§8 の「—」）を、いつ上げるか | 各 Spec の移植の issue で、その場で提案として上げる | 提案の前提になる事実を今のうちに直しておく。ExecutionContextSpec の分け方（M6）、区分の取り違え（L2）、DispatcherShutdownSpec の測り方（L7）。どれも §8 に入れた |
| D4 | SystemMessageListSpec の扱い | system message のキューは `VecDeque` にする。この Spec は、`do_not_port` かケースの除外の候補として人間に出す | `not_built` ではない（L2）。台帳の定義（`ledger-schema-draft.md` の「作らない区分」）は「原典の言語（Java、Scala）のための補助そのもの」で、SystemMessageList は Pekko の内部のデータ構造だからである。性能で要ると分かったら、原典と同じ侵入型の連結リストに替える |
| D5 | 段 1 で、ロックを取れなかったときの振る舞い | panic する（今の `RefCell` の二重借用と同じ） | 段 1 の完了条件に、再入が止まらずに panic することのテストを足す。`ActorSystem` と `WeakActorSystem` の rustdoc に、「複数スレッドで使える形になる段 5 より前は、別のスレッドから同時に使うと panic する」と書く（M3） |

## 10. 動かしたコマンドと結果

試作はすべて scratchpad の写しで行った。1〜8 はレビューの前、9〜12 はレビューのあとに流した。3 は main の `d842842` の写しで行った。1、2、5 は 1 つ前の `d435d7b` の写しで行った。その間の差分は kernel に `glob`、`pretty_duration`、`switch` を足しただけで、`executor`、`actor_system`、`event`、`actor` には触れていない。作業ツリーで変えたのは、この PR の 2 ファイルだけである。

| 番号 | コマンド | 結果 |
|:---|:---|:---|
| 1 | 写しの `tests/` に `assert_send::<ActorSystem>()`、`WeakActorSystem`、`EventStream`、`assert_sync::<ActorSystem>()`、`ActorRef`、`Props` を置き、`cargo check -p fraktor-actor --test zz_send_probe` | E0277 が 9 件。原因は `Rc<RefCell<Executor>>`、`rc::Weak<RefCell<Executor>>`、`Rc<RefCell<StreamSetup>>` の 3 つ。`ActorRef` と `Props` は通る |
| 2 | 写しの `executor_value.rs` に `assert_send::<Executor>()` などを置いて `cargo check -p fraktor-actor --lib` | E0277 が 8 件。`dyn Actor`、`become` の枠、終了の通知、`EventStream` の中身 |
| 3 | 写しに §2.3 の置き換え（`Arc<spin::Mutex<_>>`、`Send` の境界）を入れて `cargo test --workspace` | main の d842842 の写しで、92 個のテストのバイナリの 1078 件が通り、失敗は 0。元のコードは 91 個で 1077 件。差の 1 件は 1 の検査で、置き換えのあとは通る。書き換えたテストは単体テストの 2 ファイル（5 か所の `borrow` を `lock` に）だけ |
| 4 | `bash scripts/quint-check-specs.sh`（リポジトリの写しに quint 0.32.0 を `npm ci` で入れて） | exit 0。`mailbox_scheduling.qnt` は 14 passing、`[ok] No violation found`、`Trace length statistics: max=21`。既存の 3 本も通る |
| 5 | 元のコードと、3 と同じ置き換えを入れた写し（どちらも `d435d7b`）で、`cargo test --release --test zz_stage1_bench`（20 万件 × 7 回の中央値） | 外からの `tell`：97.9 → 99.5 ns。自分への `tell` を `run_until_idle`：83.6 → 84.3 ns。`run_one` の繰り返し：88.1 → 85.0 ns |
| 6 | 仕様の写しに `initLostWakeup` などを足し、`quint run --init=init<モード> --invariant=invariant --max-steps=30 --max-samples=20000` | 4 つのモードとも `[violation] Found an issue`（seed は LostWakeup `0x1b40dfc978df975a`、CheckThenAct `0xf34588dfe384ca9a`、ExternalSuspend `0xcf8dec869c26a400`、LateEnqueue `0x92d5e3a0950a5673`） |
| 7 | 正しい形を `quint run --invariant=invariant --max-steps=60 --max-samples=100000` | `[ok] No violation found`、`max=45` |
| 8 | （最初の版 `e702bbe` の仕様で）`timeout 1500 quint verify --invariant=invariant --max-steps=12`（Apalache 0.56.1、正しい形） | `The outcome is: NoError`（12 手までのすべての状態で 8 つの不変条件が成り立つ）。約 25 分かかった。結果を出した直後に `timeout` の打ち切りに当たり、終了コードは 124。CI には入れない（`quint-in-ci.md` の方針） |


レビューのあとに流したもの（9〜12）：

| 番号 | コマンド | 結果 |
|:---|:---|:---|
| 9 | main `d842842` の写しに、段 1 の置き換えを `try_lock` の形（取れなければ `already borrowed: re-entrant or concurrent use` で panic）で入れて `cargo test --workspace` | 92 個のバイナリの 1078 件が通り、失敗は 0（レビューの試作の 2 と同じ結果） |
| 10 | 直した仕様で `quint test`、`bash scripts/quint-check-specs.sh`。誤りの 5 つのモードから `quint run --init=init<モード> --invariant=invariant --max-steps=20 --max-samples=10000`。レビューの変異 M4（suspend 中も予約する）、M5（Resume で数が減らない）、M6（失敗を親に伝えない）を入れた写しで `quint run`（20 手、1 万本）。正しい形を 60 手・10 万本 | `quint test` は 17 passing。スクリプトは exit 0（`No violation found`、`max=21`。ほかの 3 本も通る）。5 つのモードはどれも `[violation]`。M4〜M6 はどれも `[violation]`（レビューの版では M4〜M6 とも `[ok]` で見逃していた）。M4 は、I13 を `canBeScheduled` で書いた途中の版では `[ok]` で、`hasWork` で書き直して見つかった。正しい形は `[ok]`、`max=53` |
| 11 | 元のコードと 9 の写し（どちらも `d842842`）で、`cargo test --release --test zz_stage1_bench`（20 万件 × 7 回の中央値）を、順番を入れ替えて 4 回ずつ。`WeakActorSystem::run_one` で回す経路を足した | `WeakActorSystem::run_one`：元 80.3、80.2、81.0、80.6 ns、段 1 は 92.6、81.8、82.2、81.5 ns。1 回目は、段 1 の側だけ全経路が約 13% 遅かった（ロックを通らない `run_until_idle` も 79 → 90 ns）。ほかの経路の差は、向きがそろわない揺れの中（外からの `tell` は、元が 95.7〜104.6 ns、段 1 が 96.0〜109.8 ns） |
| 12 | 直した仕様で `quint verify` | 流していない（最初の版で 25 分かかった。CI にも入れない方針） |

429、rate limit、overloaded は出なかった。

## 11. 設計の独立レビューの指摘の処理

レビュー：固定 `e702bbe`、Claude Opus（設計にも実装にも関わっていない）、2026-11-26。判定は要修正（高 2・中 8・低 8）。
レポートは bot の記録 `pr236-design-review.md` にある。どの指摘も、一次情報（原典、決定文書、コード）で確かめてから
処理した。

| 指摘 | 処理 | 変わった箇所 |
|:---|:---|:---|
| H1 D2 は確認ではなく、承認済みの所有権モデルを覆す判断である | 直す | §3.4 を書き直した（決定文書の該当箇所、スパイクとの関係、案 (A) の 4 つの条件、案 (B)）。§9.2 の D2 を「決定文書を改めるか」の判断にし、オーナーの判断待ちと書いた。§2.4 に、段 1 で Arc が増える場所を足した |
| H2 閉じるときに残りの system message を捨てている（DeathWatch を壊す） | 直す | 仕様：`Terminate` で残りの system message を `sysDeadLettered` へ、閉じたあとの `control` も dead letters へ。不変条件 `systemMessagesConserved`（I12）、負の対照 `DropSystemOnClose`、`run` を 3 本足した。設計：§3.2 の閉じる門の表、§4.1 の I6 と I12、§5.1、§5.3 |
| M1 「Terminate を処理した実行の中で閉じる」は T1 と原典に反する | 直す | 仕様の冒頭と §5.1 に、`Terminate` は子のいないアクターの `finishTerminate` に当たると書いた。§4.1 の I5 を「`finishTerminate` で閉じる」に直した。§5.4 の `actor_termination.qnt` に、停止中で開いていて suspend された状態と、その負の対照を書いた |
| M2 `execute` が断られたとき、scheduled の bit が立ったまま残る | 直す | §3.5 に、1 回だけ試し直し、だめなら bit を下ろしてエラーを出すと書いた。§4.1 に I15、§5.4 の `dispatcher_lifecycle.qnt` の不変条件と負の対照、§7.1 に入力を足した。実装は段 5 |
| M3 段 1 は、コンパイルで止まっていた同時の利用を、実行時の panic に変える | 直す | §6 の段 1 の完了条件に再入のテストを、中身に rustdoc の注意書きを足した。§9.3 の D5 の条件にした。§7.2 の計測は `try_lock` の形で測り直した |
| M4 閉じたかどうかをキューに持たせると、Balancing の共有のキューと両立しない | 直す | §3.2：`MessageQueue` から「閉じた」を外し、`Mailbox` の閉じる門にした。キューが自分専用か共有かで、閉じるときの扱いを分けた。§6 の段 2 に単体テストを足した |
| M5 kernel のキューに `spin` を焼き込んでいる。ロックの順序も無い | 直す | §3.2：ロックを `lock_api::RawMutex` の型引数にし、std の adaptor が休むロックを渡す形にした。ロックの順序の表と、持ったまま `tell` しない決まりを書いた |
| M6 ExecutionContextSpec の 10 ケースを一括で「Scala の機能」とした | 直す | §8 を、BatchingExecutor を作れば移せる 3、段 6／7b で移せる 3、Scala の機能 4、SerializedSuspendable 4 に分けた |
| M7 順序に頼るテストの扱いが、移植の規約と結びついていない | 直す | §6 に「3 の前：順序の洗い出し」の段を足した。一覧を作ること、直す道（triage への差し戻し、弱めるものは人間の判断）を書いた。D1 の条件 1 でもある |
| M8 Quint の不変条件の穴（予約したのに仕事が無い、進まないこと） | 直す | 仕様に `submittedOnlyWithWork`（I13）と `settledMeansIdle`（I14、幽霊の変数 `externalSuspendIssued` を使う）を足した。I13 は `canBeScheduled` を使わずに書いた（同じ定義を使うと、予約の規則の誤りが検査そのものも弱める。最初はそう書いて、変異 M4 を見逃した）。レビューの変異 M4〜M6 は、探索で見つかるようになった（§10 の 10） |
| L1 計測が、変えた経路を測っていない | 直す | §7.2 に明記した。testkit が回す経路（`WeakActorSystem::run_one`。段 1 でメッセージごとに `upgrade` が走る）を測り足した。段 3 と段 5 の完了条件に、メッセージの経路の計測を入れた |
| L2 D4 の `not_built` は台帳の定義に当たらない | 直す | §9.3 の D4 と §8 を、`do_not_port` かケースの除外の候補に直した。MailboxConfigSpec の SingleConsumerOnly も同じ理由で直した |
| L3 段 3 で `await_envelope` が何も受け取らなくなり、testkit の待ちが眠りになる | 直す | `WakeSignal` を段 3 へ前倒しした（§3.2、§3.6、§6 の段 3） |
| L4 自動停止に Scheduler の port が要る | 直す | §3.2 に kernel の `Scheduler` の port を、§6 の段 6 に依存を書いた |
| L5 I7 は期限も不変条件にしているが、仕様は throughput だけ | 直す | §4.1 の I7 に、仕様が見るのは throughput だけで、期限は単体テストで見ると書いた |
| L6 T1 に、最初の `unwatchWatchedActors` が無い | 直す | §4.3 の T1 に足した（原典 `FaultHandling.scala:185`） |
| L7 DispatcherShutdownSpec を、プール自身の数え方で確かめると漏れを見逃す | 直す | §4.3 の T8 と §8 を、スレッドの join の完了で確かめる形に直し、人間に出すときに添える点として書いた |
| L8 `ActorEventBus` は段 1 のあとも !Send のまま | 直す | §3.3 に書いた |

D1 は、元帥が (A) に決めた（条件 2 つ。§9.1）。D2 はオーナーの判断待ちである（§9.2）。
