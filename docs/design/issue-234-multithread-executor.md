# 設計：#234 1 つの ActorSystem を複数のスレッドで回す実行器（dispatcher の移植の前提）

[Issue #234](https://github.com/j5ik2o/fraktor-ng-rs/issues/234) の設計である。親は
[Issue #179](https://github.com/j5ik2o/fraktor-ng-rs/issues/179)（台帳の unported を 0 件にする）。原典は
Pekko v2.0.0（commit `9fbd2283b1cf701c90a1223bf621942c191db61f`）。

状態：設計の案。設計の独立レビューは実施済み（2026-11-26、固定 `e702bbe`、判定は要修正。高 2・中 8・低 8）。
再レビュー（固定 `7ecb3f5`）の判定は承認（D2 を除く）で、低の指摘 7 件があった。指摘の処理と、それで変わった
箇所は §11 にある。再レビュー 2（固定 `29908ed`）は要修正（文書だけの小さな直し）だった。D1 は決定済み（元帥、2026-11-26）。
D2 は承認済み（オーナー、2026-11-26、元帥の項目 420）で、
所有権モデルの決定文書もこの PR で改めた。tokio と embassy の adaptor を作れる形にするための C1〜C4 は、元帥が決めた
（2026-11-26。§3.2、§3.7、§6 の段 0）。再レビュー 3（固定 `0415893`）の指摘と、元帥の M1・M2 の決定は §11.5 にある。速さより Pekko の意味論を優先する指針（オーナー、2026-11-26）も、決定として
この PR に入れ、§7.5 をそれに合わせた。段 1 の実装は、この PR のマージのあとに別の issue と PR で行う。
この issue では実装しない。実装は §6 の段ごとに別の issue に分ける。

この PR に入れるものは 5 つである。

| ファイル | 中身 |
|:---|:---|
| `docs/design/issue-234-multithread-executor.md` | この文書 |
| `crates/fraktor-actor/specs/mailbox_scheduling.qnt` | §5 の Quint 仕様。CI の `quint` ジョブが typecheck・test・run を流す |
| `docs/decisions/ownership-model-arc-overhead.md` | 所有権モデルの決定文書。D2 の承認を受けて、末尾に「改訂（2026-11-26）」の節を足した。条件 5 は、速さでは止めない形にした |
| `docs/decisions/pekko-semantics-first.md` | 新しい決定。速さより Pekko の意味論を優先する（オーナー、2026-11-26） |
| `docs/decisions/crate-structure-comparison.md` | crate の作りの決定。fraktor-actor の kernel について no_std の要求が確定した節を足した（段 0 の根拠） |

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
| `ActorSystem` | `kernel/actor_system/actor_system_value.rs` の `ActorSystem` | `executor: Rc<RefCell<Executor>>`。`event_stream` の中の 2 つも |
| `WeakActorSystem` | `kernel/actor_system/weak_actor_system.rs` の `WeakActorSystem` | `executor: rc::Weak<RefCell<Executor>>` |
| `EventStream` | `kernel/event/event_stream.rs` の `EventStream` | `setup: Rc<RefCell<StreamSetup>>` と、中の `WeakActorSystem` |

外側の `Rc` を外しても、`Executor` 自体が Send ではない（試作 2）。中身の 4 か所である。

| 中身 | 場所 |
|:---|:---|
| `Box<dyn Actor>`（`Actor` に `Send` が無い） | `kernel/actor/actor_trait.rs` の `trait Actor`、`executor/actor_cell.rs` の `ActorCell::behavior` |
| `become` の枠 `Box<dyn FnMut(&mut ActorContext, Message)>` | `kernel/actor/behavior_stack.rs` の `type Frame`、`actor_context.rs` の `change_behavior`／`push_behavior` |
| 終了の通知 `Box<dyn Fn(&ActorRef)>` | `executor/executor_value.rs` の `type TerminationCallback` と `Executor::on_termination` |
| FSM の状態関数と遷移の関数（`Box<dyn FnMut …>`） | `kernel/actor/fsm.rs` の `type StateFunction` と `type TransitionHandler` |

`ActorRef`、`Props`、`Message`、`Envelope` は、すでに `Send + Sync` である（`ActorRef` は試作 1 で通る）。

### 2.2 !Send より深い問題

Send にしただけでは、アクターは同時には動かない。今の実行器は、次の 3 点で 1 本のスレッドを前提にしている。

1. **1 つのシャードに、ローカルのキューが 1 本だけある**（`executor/shard_queue.rs`）。メールボックスは
   アクターごとに無い。配送のたびに、`cells` を先頭から探して宛先を見つける（`Executor::deliver` の `self.cells.iter().position(…)`、
   アクターの数に比例する）
2. **アクターの文脈が、実行器全体を `&mut` で借りている**（`ActorContext { executor: &'a mut Executor }`、
   `actor_context.rs` の `ActorContext`）。子を作る、watch する、止めるといった操作は、実行器の表をその場で書き換える。
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
| `ActorEventBus.subscriptions`（`kernel/event/actor_event_bus.rs` の `ActorEventBus`。終了の通知の中から触る） | 購読の表と、watch している購読者の集合に分ける。購読の表はロックなしでバスが持ち、集合と外す予定の一覧と印だけを `Arc<spin::Mutex<…>>` で共有する（§2.4） |
| 上の 4 か所の中身 | `Actor: Send`、枠と FSM の関数に `+ Send`、`Fsm<S, D>` に `S: Send, D: Send`、終了の通知に `+ Send` |
| 単体テスト `actor_system_value_test.rs`（3 か所）と `actor_event_bus_test.rs`（2 か所） | `borrow` を `lock` に |

段 1 は段 0 のあとに行うので、この表の `Arc` は `kernel::sync::Arc`（§3.2 の C3）、`spin` は `portable_atomic` の機能つき
（C1）である。実行器全体のロック（`spin::Mutex<Executor>`）は、`try_lock` で再入を panic にするために要る一時のもので、
段 3〜4 で無くなる。そのため、C2 のロックの port には載せない。

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
| `ActorSystem.executor`（`actor_system_value.rs` のフィールドと `ActorSystem::with_setup`） | `Rc<RefCell<Executor>>` | `Arc<spin::Mutex<Executor>>` | システム 1 つに 1 つ | 作るときと捨てるときだけ。`ActorSystem` のメソッドは参照の数を変えない（ロックの `try_lock` で CAS が 1 回） |
| `WeakActorSystem.executor`（`weak_actor_system.rs` のフィールド） | `rc::Weak` | `sync::Weak` | `downgrade` のたびに 1 つ。今の呼び出し元は `ActorSystem::with_setup`（event stream の分）、`ActorSystem::downgrade`、`adaptor/ask_driver.rs` の `attach_wait_driver`、testkit の `TestKit::with_settings`、`TestProbe::with_settings_and_name`、`TestLatch::ready` | メソッドを呼ぶたびの `upgrade`（原子操作の増減 1 組）。メッセージごとに走るのは 2 つの経路だけである。testkit が `WeakActorSystem::run_one` でシステムを回すときと、`attach_wait_driver` で登録した ask の待ち（`AskFuture::try_await_result` → `drive_until` → `step_system` → `WeakActorSystem::run_one`。adaptor の公開 API で、今の呼び出し元は testkit の `FraktorSpec::new` だけ）である。本番の実行の繰り返しは強い参照を持つので、走らない（再レビュー 2 の (b)）。§7.5 の S3 が同じ関数を通るので、速さはそこで測る |
| `EventStream.setup`（`event_stream.rs` のフィールドと `EventStream::new`） | `Rc<RefCell<StreamSetup>>` | `Arc<spin::Mutex<StreamSetup>>` | システム 1 つに 1 つ。`EventStream` を複製するたびに参照が 1 増える | 複製と、設定を読み書きするとき（購読、ロガーと unsubscriber の開始）。`publish` の経路では触らない |
| `ActorEventBus.subscriptions`（`actor_event_bus.rs` のフィールドと `ActorEventBus::new`） | `Rc<RefCell<Subscriptions>>` と `rc::Weak` | 購読の表はロックなしでバスが持つ。watch している購読者の集合、外す予定の一覧、`AtomicBool` の印を `Arc<spin::Mutex<…>>`（印は Mutex の外）と `sync::Weak` で終了の通知と共有する | バス 1 つに 1 つ。終了の通知の中に `Weak` が 1 つ | 購読、解除、終了の通知のとき。`publish` では、印を Acquire で 1 回読むだけで、ロックも参照の数も動かない（下の段落） |

**`ActorEventBus` の作りを替える（再レビュー 2 の (a)）**：今の `ActorEventBus::publish` は、イベント 1 件ごとに
`self.subscriptions.borrow().subscribers_of(&classifier)` で購読表を借りる。そのまま `Arc<spin::Mutex<_>>` にすると、
イベントごとにロックを取ることになり、制御プレーンとは書けない。そこで段 1 では、次の形にする。`ActorEventBus` は、段 1 で作りを替え、`publish` の経路からロックを外す。publish が読む購読の表（分類と購読者の組）は、ロックなしでバスが直に持つ（`publish` は `&mut self` で、バスは 1 か所だけが持つ）。終了の通知と共有するのは、watch している購読者の集合と「外す予定の一覧」と `AtomicBool` の印だけで、`Arc<spin::Mutex<…>>` に入れる。終了の通知は、集合から外し、外したときは debug のイベントを今と同じ時点で出し、外す予定の一覧に積んで印を立てる。`publish`、`subscribe`、`unsubscribe` は、まず印を Acquire で読み、立っているときだけロックを取って一覧を片づける。ふだんの `publish` はロードが 1 回だけで、ロックを取らない。
片づけの時点は変わるが、観測できる振る舞いは変えない。止まった購読者は、次の `publish` が宛先を決める前に表から外れるので、
止まった購読者には今と同じく届かない。止まった直後の `unsubscribe` も、先に片づけるので、今と同じく `false` を返す
（`tests/g0f_review_f_terminated_subscription.rs`）。debug のイベントは、終了の通知の中で今と同じ時点に出すので、
バスを使う前に debug のイベントを待つ `tests/event_bus_spec.rs` の「unsubscribe subscriber when it terminates」も、今と同じく通る。
最初はレビューの案どおり片づけと debug のイベントを両方とも次の操作まで遅らせる形を考えたが、それではこのテストが
壊れるので、watch している集合だけを共有する形にした。

この変更も、Arc をシステムの中に広く入れることになり、所有権モデルの決定に触れる。そのため、段 1 の実装は
D2 とあわせて待っていた（元帥の訂正、2026-11-26）。D2 の承認を受けた決定文書の改訂では、この 4 か所を「制御
プレーンの共有」に当たるものとして書いた。段 1 の実装は、この PR のマージのあとに行う。

## 3. 型と API の形

### 3.1 移り方の方針：実行器は 1 つのまま、中身を段ごとに入れ替える

既存の `Executor` を残したまま、新しい実行器を横に作る形は取らない。監督、DeathWatch、stash、`become`、FSM、
event stream の意味論は、今の `executor_value.rs`（1787 行）と、それを確かめる既存のテスト 1148 件（`7ecb3f5`、
101 個のバイナリ）にある。2 つの実行器を並べると、
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

adaptor には、std のスレッドプールのほかに、tokio や embassy が来ることがある（オーナー、2026-11-26、元帥の項目 420）。
そのため kernel は、std も、CAS（compare-and-set）の命令も前提にしない。今の crate には `#![no_std]` が無く、kernel も
no_std では build できない（std の prelude の `String`、`Vec`、`Box`、`format!` に暗黙に頼り、64 ビットの原子操作も使う）。
これは段 0（§6）で直す。段 0 のあとは、CI が thumbv7em（Cortex-M4。32 ビットの CAS はある）で kernel を `cargo check`
する。thumbv6m（Cortex-M0、RP2040。CAS が無い）は、段 0b（[Issue #243](https://github.com/j5ik2o/fraktor-ng-rs/issues/243)）
のあとに CI へ足す。段 0 の試作（再レビュー 3）では、C3 の別名を入れても thumbv6m に 39 件のエラーが残った。
`Arc<T>` から `Arc<dyn Trait>` への暗黙の変換（unsized coercion）が `portable_atomic_util::Arc` では stable で使えないことと、
公開 trait `EventSink` の `into_any(self: Arc<Self>)` が std の `Arc` 以外では受け手にできないことによる（元帥の M2、2026-11-26）。

kernel が使う原子操作、Arc、ロックは、次の 3 つの決まりで扱う（元帥が決めた、2026-11-26）。

| 番号 | 決まり | 中身 |
|:---|:---|:---|
| C1 | 原子操作は `portable-atomic` 越しにする | API は core と同じで、その型の幅に対応する原子操作がある target では core の命令になる（thumbv7em では 64 ビットは該当せず、portable-atomic の中でスピンロックになる。§3.7、[Issue #263](https://github.com/j5ik2o/fraktor-ng-rs/issues/263)）。CAS の無い target では critical-section で代わりに行い、その実装は adaptor（embassy や HAL）が渡す。`MailboxStatus` と id の数え上げ（今の `NEXT_ID` などの `AtomicU64`）はこれを使う。各ファイルが `portable_atomic` を直接 import し、`kernel::sync` からは再公開しない（段 0 のあとの直し。§11.9）。switch の `AtomicBool` は段 0 では core のままにし、CAS の無い target 向けの置き換えは段 0b（[Issue #243](https://github.com/j5ik2o/fraktor-ng-rs/issues/243)）で行う |
| C2 | ロックは kernel が持つ port 越しにする | 閉じた区間を渡す形にする（下の「ロックの port」）。embassy-sync のロック（`CriticalSectionRawMutex`、`ThreadModeRawMutex`）と同じ形で、std の `Mutex`、`parking_lot`、`spin` でも作れる。ただし embassy-sync のロックは同じスレッドからの再入を許すので、それで作るときは再入を断る印を足す（§11.10 の `RawLock`）。システム全体の表も、この port を使う（再レビュー L6 の「spin のまま」は取りやめた） |
| C3 | Arc は kernel の中の別名 `kernel::sync::Arc` を使う | `cfg(target_has_atomic = "ptr")` なら `alloc::sync::Arc`、無ければ `portable_atomic_util::Arc`。target の cfg で選ぶので、kernel に `feature = "std"` を置かない決まり（syn-check の `cfg-std-forbid`）には触れない。所有権モデルの決定文書の「Arc」は、この別名を含む。CAS の無い target（thumbv6m、RP2040）も対象に入れる。ただし、その target で build できるようにするのは段 0b（#243）で、`Arc<dyn …>` の作り方と、`self: Arc<Self>` を使う公開 API の変更を含む |

| 置き場所 | 型（名前は案） | 中身 |
|:---|:---|:---|
| kernel `sync.rs` | `Arc`、`Weak`（別名だけ。原子操作の型は再公開しない） | C3。原子操作の型は各ファイルが `portable_atomic` を直接 import する（C1。syn-check の module-wiring のため、`kernel::sync` からは再公開しない。段 0 のあとの直し。§11.9）。`sync.rs` は `kernel.rs` で可視性なしの `mod sync;` として宣言する |
| kernel `sync/raw_lock.rs` | trait `RawLock: Send + Sync`（port） | `fn lock(&self, f: &mut dyn FnMut())`。閉じた区間を 1 つ走らせる。dyn で呼べる形なので、型引数が公開 API に伝わらない |
| kernel `sync/lock_cell.rs` | `LockCell<T>` | `Box<dyn RawLock>` と中身を持ち、`with(&self, f: impl FnOnce(&mut T) -> R) -> R` だけを出す。中は `UnsafeCell` で、unsafe はこの型の中に閉じる |
| kernel `sync/lock_factory.rs` | trait `LockFactory: Send + Sync`（port） | `RawLock` を作る。システムを作るときに adaptor が渡す。kernel の既定の工場は `spin`（`portable_atomic` の機能つき）で作る |
| kernel `dispatch/mailbox_status.rs` | `MailboxStatus` | `portable_atomic::AtomicU32` 1 つ。bit 0 が closed、bit 1 が scheduled、bit 2 から上が suspend の数（Pekko `Mailbox.scala:47-55` と同じ配置）。`set_as_idle`、`suspend`、`resume`、`become_closed` は compare-and-set の繰り返し。§3.8 の工夫 2 と 5 を段 2 で入れると、予約は `fetch_or` の 1 手になり、bit 2 が「system message がある」の印になって、suspend の数は bit 3 から上へ移る |
| kernel `dispatch/message_queue.rs` | trait `MessageQueue: Send + Sync` | `enqueue(Envelope)`、`dequeue`、`has_messages`、`number_of_messages`、`drain`。**閉じたかどうかは持たない**（下の「閉じる門」）。§3.8 の工夫 3 により、列そのものは adaptor が差し替えられる |
| kernel `dispatch/…` | `UnboundedQueue`、`BoundedQueue`（待たない）、`PriorityQueue`（stable の有無）、`ControlAwareQueue` | kernel の既定の列。`VecDeque<Envelope>` を `LockCell` で守る |
| kernel `dispatch/message_queue_factory.rs` | trait `MessageQueueFactory: Send + Sync`（port） | dispatcher の設定が持ち、メールボックスごとに列を作る。std と tokio はロックを使わない列、embassy は固定長の列を渡せる（§3.7、§3.8 の工夫 3） |
| kernel `dispatch/system_message.rs` | enum `SystemMessage` | Create、Suspend、Resume、Recreate、Terminate、Supervise、Watch、Unwatch、Failed、DeathWatchNotification、ChildTerminated |
| kernel `dispatch/mailbox.rs` | `Mailbox` | 状態、閉じる門、通常のキュー（`Arc<dyn MessageQueue>`。Balancing では team で共有）、system message のキュー、`ActorCell` を入れる場所（`LockCell<Option<ActorCell>>`）、dispatcher への戻りの辺（§3.4 の条件 2 により `Weak` か id）。1 アクターに 1 つ |
| kernel `dispatch/mailbox_run.rs` | `MailboxRun`（kernel が adaptor に渡す仕事） | `run(self) -> RunOutcome`。1 回の実行で、system message を処理し、通常のメッセージを throughput 件まで処理し、まだ残っているかを返す（§3.8 の工夫 1）。`RunOutcome::Again` は、run が読み直しで予約に勝った（scheduled の bit が立ったままの）状態で返す。adaptor は bit に触らず、受け取った仕事を `execute` で列に積み直すだけにする（予約を取り直さない。取り直すと、自分の立てた bit に阻まれて仕事を失う。Devin の指摘、PR #236）。`Idle` なら何もしない。`Idle` を返してよいのは、`set_as_idle` のあとに予約を読み直し、その予約に負けたとき（ほかの送り手が先に予約した、または仕事が無い）だけである。読み直して勝ったら `Again` を返す。この順を守らないと、`LostWakeup` と同じ取りこぼしになる。段 2 で、仕様の `finishIdle` と `finishRegister` に対応づける（再レビュー 3 の L2）。`Again` を積み直すときに `execute` が `Rejected` を返したら、ほかの予約と同じく §3.5 の扱い（1 回だけ試し直し、だめなら bit を下ろしてエラー）にする（再レビュー 5 の L3。bit を下ろす口は段 5 で I15 と一緒に入れる） |
| kernel `dispatch/message_dispatcher.rs` | `MessageDispatcher` | id、throughput、期限、住人の数、観測口、`register_for_execution`（Pekko `Dispatcher.scala:146` の形）、Balancing の team |
| kernel `dispatch/task_executor.rs` | trait `TaskExecutor: Send + Sync`（port） | `execute(Task) -> Result<(), Rejected>`、`shutdown`。`Task` は `MailboxRun` か、`Box<dyn FnOnce() + Send>`。これが kernel と adaptor の境目の 2 つ目（「このメールボックスを動かしてほしい」）である。断られたときの扱いは §3.5 |
| kernel `dispatch/manual_executor.rs` | `ManualExecutor` | 実行待ちのメールボックスを `VecDeque` に積むだけ。`run_one` が 1 つ取り出して回す。今の決定論的な回し方の後継 |
| kernel `dispatch/dispatchers.rs` | `Dispatchers`、`DispatcherConfig` | id から dispatcher への表、別名、既定の id。設定はコードで組む |
| kernel `dispatch/dispatcher_observer.rs` | trait `DispatcherObserver: Send + Sync` | 既定は何もしない `registered`、`unregistered`、`suspended`、`resumed`、`dispatched`、`shut_down` |
| kernel（既存） | `Clock` | throughput の期限を測る。今の port をそのまま使う |
| kernel（新しい port） | trait `Scheduler: Send + Sync` | 一度だけ後で動かす（`schedule_once(delay, task) -> Cancellable`）。dispatcher の停止の猶予（T9〜T11）に使う。原典は `prerequisites.scheduler.scheduleOnce`。adaptor はタイマーのスレッド（std）、`tokio::time::sleep`、`embassy_time::Timer` で、testkit は今の `ManualScheduler` で実装できる |
| kernel `dispatch/wake_signal.rs` | trait `WakeSignal: Send + Sync`（port） | `notify`、`wait(max)`。手で回す実行器に仕事が積まれたとき、または test actor にメッセージが届いたときに鳴らす。止まって待つ形なので、**std と testkit 用**である。tokio と embassy の adaptor は使わない。段 3 から使う（§3.6） |
| adaptor `thread_pool_executor.rs` | `ThreadPoolExecutor` | 固定の N 本のスレッド、`Mutex<VecDeque<Task>>` と `Condvar`。スレッドの名前は `<システム名>-<dispatcher id>-<番号>`。生きたスレッドの join の完了を数える（T8、§8 の DispatcherShutdownSpec） |
| adaptor | `ForkJoinConfig::scaled_pool_size(min, factor, max)` | `ceil(コア数 × factor)` を `[min, max]` に収める（`ThreadPoolConfig.scaledPoolSize` と同じ式。ConsistencySpec が使う） |
| adaptor | `PinnedExecutor` | スレッド 1 本。持ち主のアクター以外の登録を断る（Pekko `PinnedDispatcher.scala` の `register`） |
| adaptor | `BlockingBoundedQueue` | 満杯のとき push の待ち時間だけ待つ bounded キュー（`Condvar` が要るので std） |
| adaptor | `CondvarSignal` | `WakeSignal` の std の実装 |
| adaptor | std のロックの工場 | `LockFactory` の std の実装。待つときにスレッドを休ませる（std の `Mutex`、または `parking_lot`。どちらにするかは段 2 で決める） |
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

bounded のキューが満杯で、push の待ち時間だけ待つとき（`BlockingBoundedQueue`）は、門のロックを外して待つ。門を持ったまま
待つと、閉じる側も待たされる。門とキューが同じロックなら、取り出しも進まず、止まってしまう。空きができて起きたら、門の中で
閉じたかを見直してから積む。閉じていれば dead letters へ送る（I6）。待ち時間が切れたら、今と同じく dead letters へ送る。
この形は段 5 で入れる（Devin の指摘、PR #236）。§3.8 の工夫 3 で、ロックを
使わない列に替えるときは、門の作りも替える。その形は段 5 の設計で決め、`LateEnqueue` の負の対照が破れることを Quint で
確かめてから入れる。

**ロックの port（C2）**：ConsistencySpec はわざとコア数より多いスレッドを使う（`scaledPoolSize(1, 1.5, 2000)`）。
既定の dispatcher も `parallelism-min = 8` である（`reference.conf:478`）。スレッドがコアより多いと、ロックを持った
スレッドが OS に止められ、待つ側はスピンし続ける。`spin` のロックは公平でないので、送り手が多いと取り出しが飢える。
1 コアの組み込みでは、スレッド側がロックを持ったまま割り込みに入り、割り込みの中の実行器（embassy の InterruptExecutor）が
同じロックを取りに行くと、回り続けて止まる。そのため、ロックは adaptor が選ぶ。

| adaptor | ロックの工場 |
|:---|:---|
| std のスレッドプール | std の `Mutex`、または `parking_lot`（待つときに休む） |
| tokio | std と同じ。`.await` をまたいでロックを持たないことは、下の I16 が保証する |
| embassy | 割り込みからも触るものは `CriticalSectionRawMutex`、スレッドのモードだけで触るものは `ThreadModeRawMutex`。どちらも同じスレッドからの再入を許すので、再入を断る印を足して `RawLock` にする（§11.10） |
| kernel の既定（`ActorSystem::new` の手で回す dispatcher） | `spin`（`portable_atomic` の機能つき）。単一スレッドなので取り合わない |

`RawLock` の実装は、closure が panic で巻き戻っても、ロックを外す（再レビュー 3 の L4）。std の `Mutex` を使う実装は、毒
（poison）を無視して中身を取り出す。今の `ChannelBoundary` の `unwrap_or_else(|poisoned| poisoned.into_inner())` と同じ扱いで
ある。closure の中では値を移すだけ（上の `Drop` の決まり）なので、巻き戻りで中身が半端になることは無い。

**ロックの中で利用者のコードを走らせない（§4.1 の I16）**：`LockCell::with` の closure は、受け取った `&mut T` のほかに
触れない。closure の中で、`receive`、`pre_start` などのライフサイクルのフック、function ref の handler、`tell` を呼ばない。
`LockCell` は `pub(crate)` で、closure の型は `FnOnce(&mut T) -> R` に限り、`ActorContext` や `ActorRef` を渡す口を作らない。

`Drop` も利用者のコードである（再レビュー 3 の M3）。`with` の中で値を捨てると、利用者の型の `Drop` がロックの中で走る。
例えば、キューの `drain` や `clear` で `Envelope` を捨て、その `Message` の中身が最後の参照だった場合や、停止のときに
`Option<ActorCell>` を `None` にして `Box<dyn Actor>` を捨てる場合である。`Drop` の中で `tell` すれば、同じロックに
戻ってくる（spin なら回り続け、std の `Mutex` なら止まる）。そこで、`with` の中では値を外へ移すだけにし、捨てるのは
`with` を出たあとにする（例：`let drained = cell.with(|q| core::mem::take(q)); drop(drained);`）。

`ActorCell` もこの決まりに従う。1 回の実行の初めに、`with` の中で `Option::take` してセルを取り出し、ロックの外で
`receive` を回す。終わったら、`with` の中で戻す。取り出せなかったら（`None`）、ほかのスレッドが同じアクターを実行して
いることになるので、I1 の破れとして panic する（前の版の `try_lock` の検査の代わり）。ロックを取るのは 1 回の実行ごとに
2 回で、メッセージごとではない。今の実行器も、配送のたびにセルを表から外して戻している（`Executor::deliver`）。

セルの出し入れには、次の 2 つの決まりも付ける（再レビュー 3 の M3）。

- **セルを戻すのは、`set_as_idle` より前にする。**順が逆だと、bit を下ろした直後に別のスレッドが予約に勝って実行に入り、
  セルが `None` のまま見つかる。何も壊れていないのに、I1 の panic になる。前の実行と次の実行の間の見え方（I10）も、
  戻すときのロックの解放と、次に取り出すときのロックの取得がつなぐ（§3.5 の実行の順の 4 と 5）
- **`receive` が panic で巻き戻るときも、セルを戻す。**取り出しは drop guard で包み、巻き戻りでも `Drop` の中でセルを
  戻す。catcher の無い `ActorSystem::new` は、今も panic をそのまま外に出す。セルを取り出したまま巻き戻ると、次の実行が
  `None` を見つけ、誤った I1 の panic になるからである。guard がセルを戻すときも、上の `Drop` の決まりに従い、ロックの
  中では戻すだけにする
- CallingThread（段 7b）は、同じスレッドの再入を、セルを取り出す前に thread-local の「処理中」の印で見分け、後回しにする。
  取り出したあとで見分けると、取り出せないことが I1 の panic と区別できない

**ロックの順序**：

| 順 | ロック | 持ってよい間 |
|:---|:---|:---|
| 1 | `ActorCell` を入れる場所 | 取り出すときと戻すときだけ。`receive` の間は持たない |
| 2 | システム全体で共有する表（function ref、終了の通知、dispatcher の登録表など） | 読み書きの間だけ |
| 3 | メールボックスの閉じる門とキュー | 出し入れの間だけ |
| 4 | event bus の購読者の集合（§2.4） | 読み書きの間だけ |

- 1〜4 は、同時に 2 つ以上持たない。
- Balancing（再レビュー L7）：門は一員ごと、通常のキューは team で共有するので、ロックが 2 つに分かれる。送るときは、
  門を見ずに共有のキューに積む（原典 `BalancingDispatcher.scala:106-109` は、止まった一員宛ても共有のキューに積む）。
  門は、その一員の system message のキューと、閉じたかどうかを守るだけに使う。したがって、Balancing でも門と
  共有のキューのロックを同時に持つことはない
- ロックを外してから送る。今の `EventStreamBus::publish_through`（`event_stream_bus.rs`。`state.lock().recipients_of(class)`
  で写してから送る）がこの形である。段 4 で共有の表を足すときも、これを守る。
- 待ち（スレッドの休止、bounded の push の待ち、testkit の待ち）は、どれも adaptor が行う（std は `Condvar`、tokio と
  embassy は `.await`）。

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

### 3.4 ActorRef とメールボックス（D2：承認、オーナー 2026-11-26）

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

**これは、承認済みの所有権モデルを改める変更である。オーナーが承認した（2026-11-26、元帥の項目 420）。**
オーナーの言葉は「つまり Pekko と似たようなスレッドモデルになるってことね。承認」だった。決定文書
（`docs/decisions/ownership-model-arc-overhead.md`）は、この PR で改めた（末尾の「改訂（2026-11-26）」の節）。改める前の
決定は、次のとおりだった。

- 「Arc / Rc は … ホットパス（cell / mailbox / ref / 配送）には置かない」（「原則として言語化」の節）
- スレッド境界を越える送信端の Arc は認めるが、「『境界』に閉じ込められ、シャード内のホットパスには入らない」（「それでも Arc / Rc が残る場所」の節）
- 移植の規約の要約（`.takt/facets/knowledge/fraktor-porting-rules.md` の「所有権モデル（accepted）」）も、
  「ActorRef は id＋世代番号の値」「親子・DeathWatch・購読は … 所有テーブル／登録表」としている。矛盾したら決定文書が優先する

Arc を置いてよいのは、メールボックスの入れ物（`Arc<Mailbox>`）だけで、予算はアクター 1 つに Arc 1 つである。対象は、
セルのアクターすべてのメールボックス（手で回す dispatcher を含む。複数スレッドで動かすために要る形）である。

この形では、アクターごとに `Arc<Mailbox>` があり、その中に `ActorCell` を入れ、`ActorRef` がそれを指す。メールボックスと
ref と配送の経路に Arc が入る。所有権モデルのスパイクで「実装 1 Arc 全面型」として測られ、単一スレッドで 7〜15 倍
遅かった形に近い（同じ文書の「スパイク結果」の節）。そこで、改訂は Arc を置く場所をメールボックスの入れ物だけに絞り、
次の条件を付けた。条件は決定文書の改訂の節と同じである。

| 条件 | 中身 | 確かめ方 |
|:---|:---|:---|
| 1 | 1. メールボックスの Arc は、1件さばくごとには複製しない。複製するのは、予約（idle から scheduled）のたびに1つだけ。送り手のハンドルを Envelope に入れるときの複製は、今の ActorRef の複製と同じ数にとどめる。 2. 1件あたりの原子操作の数は、今（宛先と送り手のハンドルで Arc を4つ複製）を超えない。段3 の複製の回数を数えるテストで、この数を確かめる。 | 段 3 の完了条件：`cfg(test)` の数え上げで、N 件の送受信での Arc の複製の回数を数える。今の `tell` は、宛先のハンドル（path と boundary）と送り手のハンドル（path と boundary）で Arc を 4 つ複製する。段 3 では、送り手のハンドル（path と mailbox）の 2 つと予約の 1 つで、計 3 つになる（再レビュー 2 の M2 の数え方。文言は元帥が決めた、2026-11-26） |
| 2 | `Mailbox` から dispatcher、システム、dead letters へ戻る辺は、`Weak` か id にする。強い循環を作らない | 段 3 の設計の型で見る |
| 3 | 閉じるとき（I6）に、自分専用のキューを必ず dead letters へ流す。Envelope の送り手（`ActorRef` → `Arc<Mailbox>`）による循環を断つため。今の `Executor::stop_all` の doc が、境界について同じ理由を書いている | 段 3 の完了条件 |
| 4 | 終了したあとにメールボックスが解放されることを、テストで確かめる（`Arc::strong_count` か、`Weak::upgrade` の失敗） | 段 3 の完了条件 |
| 5 | 単一スレッドの速さを計り、報告する。速さでは作業を止めない（`docs/decisions/pekko-semantics-first.md`） | §7.5 の測り方と報告の仕方。段 1、3、4、5 で報告する |

選ばなかった案 (B) は、`ActorRef` を id＋世代のままにし、配送のたびに並行の表（id → メールボックス）を引く形だった。
決定文書は変えずに済むが、`tell` のたびに表のロックと検索が要る。止まったアクター宛てを見分けるのにも、表の世代の検査が要る。

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

1. セルを取り出す（§3.2。取り出せなければ I1 の破れとして panic。巻き戻りでも戻す drop guard で包む）
2. 閉じていなければ、system message をすべて処理する
3. 通常のメッセージを 1 件取り、`receive` に渡す。そのあと、また system message をすべて処理する。
   suspended でなく、throughput と期限が残っていれば、3 を繰り返す
4. セルを戻す（`set_as_idle` より前。再レビュー 3 の M3）
5. scheduled の bit を下ろす（`set_as_idle`）
6. もう一度予約を読み直す。5 と 6 の間に届いたメッセージを、ここで拾う（§5 の LostWakeup）。勝てば `RunOutcome::Again`、
   負ければ `RunOutcome::Idle` を返す（§3.2 の `MailboxRun`）

期限は、3 の繰り返しのたびに `Clock` で測る。期限を過ぎても、取り出した 1 件は最後まで処理する。

**実行器に断られたとき（設計レビュー M2）**：`register_for_execution` が compare-and-set に勝ったあと、
`TaskExecutor::execute` が `Rejected` を返すことがある（T7 で止めた直後や、T11 で作り直す前）。原典
`Dispatcher.scala:146-172` と同じく、1 回だけ試し直す。それも断られたら、scheduled の bit を下ろし（`set_as_idle`）、
event stream に `Error` を出して、呼び出し元にエラーを返す。bit を立てたままにすると、そのメールボックスは
二度と予約されない（I2 と I3 が破れる。§4.1 の I15）。

### 3.6 既存のテストと testkit

- `ActorSystem::new(name, boundary)` の既定の dispatcher は Manual にする。今と同じく、テストのスレッドが回す。
  `run_one`、`run_until_idle`、`await_envelope` の名前と意味は変えない
- 未登録の dispatcher の id：今は `Props::with_dispatcher` の名前を記録するだけで、使わない
  （`tests/supervisor_misc_spec.rs` の `counting_worker_props` が `pinned-dispatcher` などを未登録のまま使う）。`ActorSystem::new` で
  作ったシステムでは、未登録の id は既定の dispatcher に回す（今の振る舞いを保つ）。dispatcher を登録して作った
  システムでは、Pekko と同じく、未登録の id で `actor_of` を断る
- testkit の待ち方：今の `TestKitBase::receive_one` は、`run_one` で 1 件回し、無ければ `await_envelope` で境界を
  待つ（`test_kit_base.rs` の `await_one` と `receive_one`）。段 3 で、cell のアクター宛ての `tell` は境界を通らなくなる。そのままだと
  `await_envelope` は何も受け取らず、待ちは `POLL_INTERVAL` の眠りになる。複数スレッドのアクターは、テストの
  スレッドが回さなくても進む。そこで段 3 で、`await_one` を kernel の port `WakeSignal` を待つ形に替える。手で回す実行器に仕事が積まれたときと、
  test actor にメッセージが届いたときに、どちらも同じ合図を鳴らす。手で回すアクターだけのテストでは、今と同じ
  順序で回る
- `ask` の待ち（`adaptor/ask_driver.rs`）は変えない。手で回すシステムは登録した driver が回し、複数スレッドの
  アクターへの `ask` は、今の条件変数の待ちで返りを待つ（登録の無い宛先の経路で、すでにある）

### 3.7 tokio と embassy の adaptor の作り方

kernel と adaptor の境目は、次の 2 つだけである（§3.8 の工夫 1）。

- kernel から adaptor へ：「このメールボックスを動かしてほしい」（`TaskExecutor::execute(Task)`）
- adaptor から kernel へ：「メールボックスを決まった件数（throughput）だけ動かし、まだ残っているかを返す」（`MailboxRun::run`）

アクター 1 つにメールボックス 1 つ、という形は、どの adaptor でも変えない。adaptor が替えるのは、メールボックスを回す
場所（スレッド、タスク）、ロックの工場、通常のメッセージの列、時計、後で動かす仕組みだけである。

| 部品 | std のスレッドプール（段 5） | tokio | embassy |
|:---|:---|:---|:---|
| `TaskExecutor` | `ThreadPoolExecutor`。共有の列と `Condvar` で N 本のスレッドが回す | `TokioTaskExecutor`。予約のたびに `handle.spawn(async move { run.run() })`。割り当てを減らすなら、常駐の N 本のタスクが共有の列と `tokio::sync::Notify` で回す | `EmbassyTaskExecutor`。`static CH: Channel<CriticalSectionRawMutex, Task, N>` を置き、`#[embassy_executor::task(pool_size = W)]` の worker のタスク W 本が `loop { CH.receive().await.run() }` で回す。`execute` は `try_send` で、満杯なら `Rejected` を返す（§3.5 のとおり、bit を下ろしてエラーにする） |
| `RunOutcome::Again` の扱い | 列の後ろへ積み直す | 同じ（`spawn` し直す） | 同じ（`try_send` し直す） |
| Pinned | 専用のスレッド 1 本 | 専用の常駐のタスク 1 本（止まって待つアクターなら `spawn_blocking` の上のスレッド） | 専用の静的なタスク 1 本 |
| 止まって待つアクター | Pinned か、別の dispatcher に置く | `spawn_blocking` で回す dispatcher に置く（Pekko の blocking 用の dispatcher と同じ考え） | 置かない（待つときは `.await` を使うアクターにする話で、この設計の範囲の外） |
| `LockFactory` | std の `Mutex` か `parking_lot` | std と同じ | `CriticalSectionRawMutex`、`ThreadModeRawMutex` に、再入を断る印を足したもの（§11.10 の `RawLock`） |
| 原子操作、Arc | core（`portable-atomic` はそのまま core の命令になる） | 同じ | thumbv7em の 64 ビットは、portable-atomic の中でスピンロックになり、割り込みから再入すると止まる（[Issue #263](https://github.com/j5ik2o/fraktor-ng-rs/issues/263)）。CAS の無い target（thumbv6m、RP2040）は段 0b（#243）のあとで、`portable-atomic` の critical-section と `portable_atomic_util::Arc` |
| `MessageQueueFactory` | kernel の既定の列。段 5 でロックを使わない列に替えられる（§3.8 の工夫 3） | 同じ | 固定長の列（`heapless` の類。満杯は push の待ち時間 0 の bounded と同じく dead letters） |
| `Clock` | `SystemClock`（今のもの） | `tokio::time::Instant` | `embassy_time::Instant` |
| `Scheduler` | タイマーのスレッド | `tokio::time::sleep` を待つタスク | タイマーの列を持つ静的なタスクが `embassy_time::Timer` で待つ |
| testkit の待ち（`WakeSignal`） | `CondvarSignal` | 使わない（テストは `#[tokio::test]` の中で `.await` で待つ） | 使わない |

`MailboxRun::run` は同期のコードで、中で `.await` しない。アクターの `receive` も同期で、Pekko と同じく、止まって待たないことを
利用者に求める。

embassy で前提になるもの：

- アクターの数は動的なので、`alloc` が要る。global allocator（`embedded-alloc` など）を、アプリケーションが用意する
- critical-section の実装は、HAL か `cortex-m` の crate が渡す
- worker のタスクの本数 W と、列の長さ N は、コンパイル時に決める。N は「同時に生きているメールボックスの数と、
  実行を待つクロージャの数の和の最大」以上にする。I2 により、各メールボックスは列に高々 1 回しか入らないので、こうすれば
  混んでいるだけで満杯になることは無い。満杯で `Rejected` になるのは、止めたあとだけにする（原典と同じ。再レビュー 3 の L3）。
  §3.5 の扱い（bit を下ろしてエラー）は、仕事のあるメールボックスを予約されないまま残すので、混雑で起こしてはいけない

embassy の adaptor を作る段の完了条件に、[Issue #263](https://github.com/j5ik2o/fraktor-ng-rs/issues/263) を閉じる（割り込みから送っても止まらないこと）を含める。embassy では割り込みから送るのがふつうの使い方なので、この条件を満たさない限り embassy 対応とは言わない（元帥）。

どの adaptor を作るかは、この設計の範囲の外である（§1.3）。この節は、kernel の port が、どの adaptor でも作れる形に
なっていることを示すためのものである。段 0 の CI（thumbv7em の `cargo check`）と段 0b の CI（thumbv6m）で、kernel が std なしで
build できることを守る。

### 3.8 Rust のアクター実装から取り入れる工夫

元帥の調べ（2026-11-26）から、次の 6 つを、どの段で入れるかを決めた。どれも 1 アクター 1 メールボックスを保つ。
意味論（Pekko のテストと文書が利用者に約束している振る舞い）を変えるものは入れない
（`docs/decisions/pekko-semantics-first.md`）。

根拠にした実装（元帥の調べ役が clone した写し）：

| 実装 | commit | 見たところ |
|:---|:---|:---|
| embassy | `2436d8e` | `embassy-executor/src/raw/state_atomics.rs` の `run_enqueue`（`fetch_or(STATE_RUN_QUEUED)` で印を立て、もとが 0 だった呼び出し元だけが列に積む） |
| riker | `ccd4e5f` | `src/kernel/mailbox.rs`。`set_scheduled(false)` のあと、`is_scheduled()` を読んでから予約する 2 手で、予約の印の読みと書きが分かれている（Quint の `CheckThenAct` と同じ形の誤り） |
| ractor | `064c9fa` | `docs/message-scheduling.md`。アクターは約 1,000 万件／秒（約 100 ns／件）、素の tokio のチャネルは速い場合で約 8,000 万件／秒（約 12 ns／件） |
| tokio | `854013a` | `tokio/src/runtime/scheduler/multi_thread/worker.rs`。同じ worker が起こしたタスクを次に動かす枠（LIFO の枠）。続けて使うのは 3 回まで（`MAX_LIFO_POLLS_PER_TICK = 3`） |
| heph、ector | `3183f48`、`5196572` | メッセージを列の枠に直接入れ、1 件ごとの割り当てを避ける |

| 番号 | 工夫 | 入れるか | 段 | 中身と条件 |
|:---|:---|:---|:---|:---|
| 1 | kernel と adaptor の境目を「決まった件数だけ動かし、まだ残っているかを返す」と「このメールボックスを動かしてほしい」の 2 つにする | 入れる | 段 2（型）、段 3（配線） | `MailboxRun::run(self) -> RunOutcome` と `TaskExecutor::execute`（§3.2）。Pekko の `Mailbox.run` が最後に自分で `registerForExecution` を呼ぶところを、「まだ残っている」を返して adaptor に積み直させる形にする。予約の数の勘定は同じなので、Quint の `finishRegister` の意味は変わらない。tokio と embassy でも同じ形で作れる（§3.7） |
| 2 | 予約の印は `fetch_or` の 1 手で立て、0 から 1 にした送り手だけがプールに頼む | 入れる | 段 2 | `set_as_scheduled` を、`prev = fetch_or(SCHEDULED)` で、`prev` に scheduled も closed も無いときだけ予約する形にする。CAS の繰り返しより短く、CAS の無い target でも `portable-atomic` で書ける。suspend の数は見ない（Pekko の `setAsScheduled` も closed と scheduled の bit しか見ない）。そのため、送り手が「予約してよいか」を読んでから `fetch_or` するまでに、そのアクターが suspend されると、仕事の無い予約が 1 回起きうる（Pekko でも同じ）。その実行は何もせずに終わるので、利用者の見る振る舞いは変わらない。根拠は Pekko 2.0.0 の `Mailbox.scala` である。`setAsScheduled`（`:216`）が見るのは `shouldScheduleMask = 3`（Closed と Scheduled の 2 ビット）だけで、suspend の数（4 以上のビット）は見ない。suspend の確かめは `canBeScheduledForExecution`（`:255`）の別の読みなので、仕事の無い予約は Pekko でも起きる。今の仕様の I13 は、読みと書きを 1 手にまとめているので、この予約を禁じる。I13 を弱めるのは、意味論のずれを直すためではない（どちらの I13 も、Pekko が許す範囲の中で、利用者の見る振る舞いは同じ）。`fetch_or` の 1 手で予約するためである。弱めた I13 で Quint の検査を通したことと、強い I13 でしか捉えられない筋書きが無いことは、§3.10 にある。段 2 で、その形を仕様に入れてから実装する |
| 3 | キューのロックをやめ、adaptor ごとに列を差し替えられるようにする（std と tokio はロックを使わない列、embassy は固定長の列）。取り出す側はまとめて取る | 入れる。ただし kernel の既定の列はロックつきのまま | 段 2（`MessageQueueFactory` の port）、段 5（std のロックを使わない列）、embassy の adaptor（固定長の列） | 列は `MessageQueueFactory` で差し替える。ロックを使わない列では、門の作り（閉じたかを見ることと積むことを分けない。I6）を別の手で得る必要がある。その形は段 5 の設計で決め、Quint で `LateEnqueue` の負の対照が破れることを確かめてから入れる。取り出す側は、1 回の実行で throughput 件までをまとめて取る |
| 4 | メッセージを列の枠に直接入れ、1 件ごとの割り当て（`Box`）を避ける | 今は入れない | —（typed の層のゴールで考え直す） | 今の層は Pekko の classic（untyped）で、メッセージは何の型でもよい（`Message` は `Arc<dyn Any + Send + Sync>`）。型を消すために、1 件ごとの割り当ては残る。heph や ector は、アクターごとにメッセージの型が 1 つに決まる（typed）ので、列の枠に直接入れられる。typed の層を作るゴールで考え直す。`Envelope` そのものは今も `VecDeque` の枠に直接入っている |
| 5 | 「system message がある」の印を状態の語に同居させ、1 件ごとに system message の列を見に行かない | 入れる | 段 2 | `MailboxStatus` の bit 2 を印にし、suspend の数を bit 3 から上へ移す。送り手は、system message を積んでから印を立てる。実行する側は、印を下ろしてから列を空にする。この順を逆にすると、下ろす直前に積まれたものが取り残される。段 2 で、仕様にこの印と、順を逆にした負の対照を足してから実装する |
| 6 | 決まった件数（throughput）をさばいたら手放す。同じスレッドで送った相手は、次に動かす | 1 つ目はすでに入っている。2 つ目は入れる | 1 つ目は §3.5 と I7。2 つ目は段 5 | 1 回の実行で throughput 件まで処理して手放す（`throughputRespected`）。2 つ目は std のスレッドプールの最適化で、worker が `receive` の中で予約したメールボックスを、次に自分で動かす枠に置く。飢えを防ぐため、tokio と同じく続けて使うのは 3 回までにする。アクターの間の処理の順は、Pekko も約束していない（同じ送り手から同じ宛先への順だけを約束する）ので、意味論は変わらない |

### 3.9 1 回の実行で処理する件数（オーナーの訂正）と `run_one`

アクターは 1 回動かすと、throughput の N 件まで処理する（1 件だけではない）。この設計の §3.5、I7、Quint の
`throughputRespected`、§3.8 の工夫 1 と 6 は、どれもこの意味で書いている。

例外は、手で回す dispatcher の `ActorSystem::run_one` である。今の公開 API は「1 件だけ配る」（今の
`Executor::run_one` の doc）で、testkit は 1 件ごとに期限を確かめている（`TestKitBase::receive_one`）。そこで、
`run_one` は今と同じく 1 件だけ配る（手で回す dispatcher は、`run_one` から呼ばれたときだけ throughput を 1 として回す）。
`run_until_idle` は、件数に関係なく、仕事が無くなるまで回す。

`run_one` は、testkit が 1 件ずつ進めるための公開 API で、メールボックスの 1 回の実行（throughput の N 件）とは別のもの
である（元帥、2026-11-26）。段 3 で、`ActorSystem::run_one` と `WeakActorSystem::run_one` の rustdoc を、次の案で書き直す。

```text
Delivers at most one message of the manually driven dispatcher and returns whether it delivered one.

This is a stepping aid for tests that check a deadline between messages; it is not one run of a
mailbox. A run of a mailbox handles up to `throughput` messages; `run_one` hands a single message to
a single actor and then returns, whatever the throughput of its dispatcher is. Use `run_until_idle`
to drive the system the way a dispatcher does.

It keeps running ready mailboxes that only have system messages to handle until it delivers one
message or nothing is ready, so `false` means there was nothing left to drive.
```

`run_one` が `false` を返すのは、実行待ちのメールボックスが無くなったときだけにする。実行待ちの先頭のメールボックスが
system message だけを処理して終わったときは、`false` を返さず、次のメールボックスを続けて回し、通常のメッセージを 1 件
配るか、実行待ちが空になるまで続ける。`TestKitBase::receive_one` は `false` を見ると待ちに入るので、ほかのメールボックスに
通常のメッセージが残ったまま待つことを防ぐためである（段 3。Devin の指摘、PR #236）。

throughput の件数や、1 回の実行の中の振る舞いを確かめるテスト（DispatcherActorSpec の「respect the throughput setting」
と「respect throughput deadline」、ConsistencySpec、§7.3 の P4、§7.5 の測り）は、`run_one` を通さない形で回す。複数スレッドの
dispatcher で回すか、手で回す dispatcher なら `run_until_idle` で回す。`run_one` で回すと throughput が 1 になり、確かめたい
ものが消える。

### 3.10 I13 を Pekko に合わせて弱める（§3.8 の工夫 2 の裏付け）

元帥の条件（2026-11-26）：弱めた I13 で Quint の検査を通し、今の強い I13 でしか通らない筋書き（反例）が無いことを示す。
scratchpad の写しの仕様で確かめた（§10 の 16）。この PR の `mailbox_scheduling.qnt` は変えていない。段 2 で、この形を
仕様に入れてから実装する。

**写しで変えたこと**：

- 送り手の予約を 2 手にした。1 手目の `register` は、予約してよいか（`canBeScheduled`）を読む。2 手目の `fetchOr` は、
  closed と scheduled の 2 ビットだけを見て、どちらも無ければ予約する（Pekko の `shouldScheduleMask = 3`）
- 弱めた I13 を `submittedWithWorkOrStaleRead` とした。「プールに入ったメールボックスに仕事が無いのは、送り手が 1 手目で読んだ
  ときには仕事があり、2 手目までの間に古くなったときだけ」である。幽霊の変数を 2 つ使う。`sawWork` は 1 手目で仕事を見た送り手、
  `staleSubmission` は、その送り手の 2 手目の予約が、仕事の無い状態で起きたことである
- I13 を外しただけの形も試した

**結果**：

| 確かめたこと | 強い I13（今の仕様） | I13 を外しただけ | 弱めた I13 |
|:---|:---|:---|:---|
| 2 手の予約の正しい形（30 手・3 万本、60 手・10 万本） | 違反（仕事の無い予約を捉える。これは Pekko でも起きる正しい振る舞い） | 違反なし | 違反なし |
| I13 以外の 11 の不変条件を 1 つずつ（30 手・2 万本） | — | すべて違反なし | — |
| 誤りの 6 つのモード | — | 6 つとも違反 | 6 つとも違反 |
| R4：suspend 中も通常のメッセージで予約する（空回りで CPU を使い続ける） | 違反 | **見逃す** | 違反 |
| R5：Resume で数が減らない／R6：失敗を親に伝えない／R12：外からの Suspend のあとの外からの Resume を捨てる | 違反 | 違反 | 違反 |
| R13：`fetch_or` が scheduled の bit を見ない（2 重に予約する） | — | 違反 | — |

**結論**：

- I13 を外しただけでは、R4 を見逃す。R4 は、強い I13 でしか捉えられない筋書きだった。
- 弱めた I13（`submittedWithWorkOrStaleRead`）は、R4 を捉え、正しい形（Pekko と同じ、仕事の無い予約を含む）を通す。
- 強い I13 でしか捉えられない筋書きは、この形では残っていない。段 2 では、この形で仕様を直す。

**段 2 で仕様を直して流し直した結果**（[Issue #250](https://github.com/j5ik2o/fraktor-ng-rs/issues/250)、§11.6 の L2）：

`mailbox_scheduling.qnt` そのものを直した。写しとの違いは次のとおりである。

- 予約の経路のすべてを「読む → `fetch_or`」の 2 手にした。送り手（`register` → `fetchOr`）、system message の送り手と親の判断
  （`control`／`decide` → `regRead` → `regFetchOr`）、実行の終わり（`finishRegister` → `finishFetchOr`）である
- `sawWork` は、1 手目の読みのときの `hasWork` で書いた（`canBeScheduled` ではない）。`staleSubmission` は、`sawWork` の呼び出し元の
  `fetch_or` が、仕事の無い状態で予約したことである。プールから取られたときに下ろす
- §3.8 の工夫 5 の印 `sysFlag` を足した。system message の送り手は、門の中で積んで印を立てる（1 手）。実行は印を下ろしてから列を
  空にする。予約の 1 手目は、印ではなくキューを読む（印は、そのメッセージを処理したあとも立ったままのことがあり、印を読むと
  仕事の無い予約が増えて I13 が破れるため）
- 負の対照 `ClearFlagAfterDrain`（列を空にしてから印を下ろす）と、不変条件 `noHiddenSystemMessage` を足した

流したのは scratchpad の写しで、`init<モード>` と比べるための `invariantStrongI13`（`invariant` と強い I13）、`invariantNoI13`
（I13 を外した形）を足しただけである。backend は rust（60 手でも落ちなかった）。正しい形は typescript の backend でも流した。

| 確かめたこと | 強い I13 | I13 を外しただけ | 弱めた I13（`invariant`） |
|:---|:---|:---|:---|
| 正しい形（30 手・3 万本、60 手・10 万本） | 違反（30 手、60 手とも） | 違反なし（`max=31`、`max=61`） | 違反なし（`max=31`、`max=61`）。typescript の backend で 30 手・3,000 本、60 手・2,000 本も違反なし |
| I13 以外の 13 の不変条件を 1 つずつ（30 手・2 万本） | — | すべて違反なし | — |
| 誤りの 8 つのモード（30 手・2 万本） | — | 8 つとも違反 | 8 つとも違反（`ClearFlagAfterDrain` は typescript でも違反） |
| R4：suspend 中も通常のメッセージで予約する | 違反 | **見逃す** | 違反 |
| R5、R6、R12 | 違反 | 違反 | 違反 |
| R13：`fetch_or` が scheduled の bit を見ない | 違反 | 違反 | 違反 |

結論は写しのときと変わらない。強い I13 は正しい形を誤って断り、I13 を外すと R4 を見逃し、弱めた I13 は正しい形を通して、
R4 を含むすべての変異とモードを捉える。

## 4. 不変条件

### 4.1 メールボックスとスケジューリング

| 番号 | 不変条件 | 何が守るか | Quint の名前 |
|:---|:---|:---|:---|
| I1 | 1 つのアクターのメッセージを、同時に 2 つのスレッドが処理しない | scheduled の bit の compare-and-set に勝った 1 人だけが、プールにメールボックスを出す。実行の初めに `ActorCell` を取り出せなければ panic（§3.2） | `atMostOneRunner` |
| I2 | メールボックスはプールに高々 1 回だけ入る。閉じていない間、scheduled の bit は「プールにある、または実行中」とちょうど一致する | 同上 | `scheduledMatchesTheRun` |
| I3 | 仕事のある開いたメールボックスは、予約されているか、予約しに行く誰かがいる（起こし忘れが無い） | §3.5 の 6 | `noStrandedMessage` |
| I4 | suspended の間は `receive` を呼ばない。suspend の数を変えるのは、そのアクターを実行中のスレッドだけ（外からの suspend は Suspend の system message にする） | `should_process_message` を取り出しの直前に読む。system message は `receive` の外で処理する | `noInvokeWhileSuspended` |
| I5 | 閉じたあとは `receive` を呼ばない。closed は抜けられない | 閉じるのは `finishTerminate` → `detach` の中（子のいないアクターなら Terminate を処理した実行、子がいれば最後の ChildTerminated を処理した実行。T1〜T3）。以後 `set_as_scheduled` は失敗する | `noInvokeAfterClose` |
| I6 | 閉じたメールボックスは何も持たない。閉じたあとの通常のメッセージは dead letters へ行く | 閉じる門（§3.2）の中で見てから積む。閉じるときに、自分専用のキューの残りを dead letters へ移す（Balancing の共有のキューは流さない） | `closedHoldsNothing` |
| I7 | 1 回の実行で処理するのは throughput 件まで。期限を過ぎたら次を取らない | 実行のループの数え方 | `throughputRespected`。仕様が見るのは throughput だけで、期限は段 2 と段 5 の単体テストで見る（時間を仕様に入れない） |
| I8 | 受け付けたメッセージは、処理済み、キューの中、処理中、dead letters のどれかにある | I6 と、閉じるときに dead letters へ流すこと | `messagesConserved` |
| I9 | 同じ送り手から同じ宛先への順序は保つ（FIFO のキューで）。priority のキューは優先度の順、stable なら同じ優先度の中で FIFO | キューの実装 | Quint には無い。PBT（§7.3） |
| I10 | あるメッセージの処理で書いた値は、次のメッセージの処理から見える。次の処理が別のスレッドでも見える | scheduled の bit の操作を AcqRel、キューのロックと `ActorCell` のロックで acquire と release がつながる | Quint は逐次一貫なので扱えない。ConsistencySpec と stress テスト（§7.3） |
| I11 | system message は、実行の始めと、通常のメッセージ 1 件ごとのあとに、届いている分をすべて処理する。system message を追い越す通常のメッセージは、多くても 1 件（直前の処理し切りのあとで system message が届き、その直後に取り出したもの） | §3.5 の 1 と 2 | `systemBeforeNormal`（`drainDone` のあとでしか `startInvoke` できない。負の対照は `MessagesBeforeSystem`）と `noSystemSinceDrain`（空を確かめたあとで system message の処理を始めたら、列が空になるのを確かめ直すまで通常のメッセージを取れない。負の対照は `PartialDrain`。Devin の指摘、PR #236）。段 2 から `noHiddenSystemMessage`（開いたメールボックスの system message は、印が立っているか、印を下ろした実行が列を空にしている途中にある。負の対照は `ClearFlagAfterDrain`。§3.8 の工夫 5） |
| I12 | system message も失われない。処理済み、キューの中、dead letters のどれかにある。閉じるときの残りと、閉じたあとに来たものは dead letters へ行く（DeathWatch のため。dead letters は Watch に DeathWatchNotification を返す） | 原典 `Mailbox.scala:318-338`、`:503-505` と同じ | `systemMessagesConserved` |
| I13 | プールに入ったメールボックスには、その実行でできる仕事が必ずある。ただし、予約した呼び出し元が 1 手目で仕事を読み、2 手目の `fetch_or` までに仕事が無くなった場合を除く（Pekko でも起きる空の実行。§3.10）。suspend 中を通常のメッセージで予約しない | `can_be_scheduled`（キューを読む。印は読まない） | `submittedWithWorkOrStaleRead`（段 2 から。強い形の `submittedOnlyWithWork` は `invariant` の外に残し、`staleReadSubmitsARunTest` で破れることを示す） |
| I14 | 何も進行中でない（送り手が途中にいない、実行も予約も無い、親の判断も待っていない、fatal のあとのシステムの終了も負っていない）とき、開いたメールボックスは、できる仕事をやり終えている。suspend されているのは、外からの Suspend がまだ Resume で打ち消されていないときだけ | I3 と、失敗のたびに親が Resume を返すこと | `settledMeansIdle`（幽霊の変数 `externalSuspendsPending` は、外からの Suspend で 1 増え、外からの Resume で 1 減る。再レビュー L5）。段 3 から、「何も進行中でない」に「catcher の無い panic（原典の fatal）のあとの、システムの終了が済んでいる」を含める（§11.11.7。fatal のあとに原典が約束するのはシステムの終了だけである）。catcher が捕まえた失敗は、親の判断が済むまで進行中なので、例外は無い |
| I15 | 実行器に断られたメールボックスに、scheduled の bit は残らない | §3.5 の「実行器に断られたとき」 | 段 5 で、実装より先に `mailbox_scheduling.qnt` に `reject` の動作と負の対照を足す（再レビュー L4）。止めた実行器の作り直しと合わせた形は `dispatcher_lifecycle.qnt`（段 6） |
| I16 | ロックの中で利用者のコードを走らせない。`LockCell::with` の closure の中で、`receive`、ライフサイクルのフック、function ref の handler、`tell` を呼ばない。利用者の型の `Drop` も走らせない（中では値を外へ移すだけにし、捨てるのは出たあと）。`ActorCell` は、実行の初めに取り出し、ロックの外で `receive` を回し、`set_as_idle` より前に戻す。`receive` が panic で巻き戻るときも、drop guard で戻す | §3.2 の「ロックの中で利用者のコードを走らせない」。`LockCell` は `pub(crate)` で、closure に `&mut T` のほかを渡さない | Quint には無い（ロックの区間は仕様の 1 手の中に入る）。段 2 の設計で、型と、セルを取り出せなかったときの panic のテストで確かめる |

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
| `closeOutside`（段 3 から。段 4a からはシステムの終了だけ。§11.12.11） | 実行器の同期の停止と、システムの終了（`stop_all`）。実行の外で閉じ、待っている `Run` を取り除く。外からの操作の 1 つとして `control` と同じ枝（`outside`）から選ぶ（§11.11.7） |
| `yieldRun`（段 3） | `invoke` が `Yield` を返して、実行が早く終わる（§11.11.2） |
| `failSystem`（段 3 から） | catcher が捕まえた system message の失敗。効果を残し、suspend して親に Failed を送り、残りの列を処理し続ける（§11.11.2） |
| `panicInvoke`、`panicSystem`（段 3 から） | catcher の無い panic（原典の fatal）での巻き戻り。guard がセルを戻し、印を立て直し、bit を下ろして予約し直す。そのメッセージは失われ、システムは終了を負う（§11.11.2） |

### 5.2 CI の決まりへの合わせ方

- 不変条件は `invariant`。§4.1 の I1〜I8、I11〜I14 の論理積である（段 2 から、I13 は `submittedWithWorkOrStaleRead`、I11 に `noHiddenSystemMessage` を足した）
- `run` は 38 本（段 3 から。段 2 は 25 本）で、名前はどれも `Test` で終わる。`Test` で終わる名前は `run` にしか使っていない
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
| `MessagesBeforeSystem` | 待っている system message を処理し切らずに、次の通常のメッセージを取る | `systemBeforeNormal` | `messagesBeforeSystemOvertakesTest`（正しい形は `systemMessageGoesBeforeTheNextMessageTest`） |
| `PartialDrain` | 空を確かめたあとに届いた system message を処理し始めたのに、処理し切る前に通常のメッセージを取る（前の版の仕様の Fixed がこの形を許していた。Devin の指摘、PR #236） | `noSystemSinceDrain` | `partialDrainLetsAMessageOvertakeTest`（正しい形は `startedDrainIsFinishedBeforeTheNextMessageTest`） |
| `ClearFlagAfterDrain` | 実行が system message の列を空にしてから印を下ろす（§3.8 の工夫 5 の順を逆にした形。段 2 で足した） | `noHiddenSystemMessage`、`systemBeforeNormal` | `clearFlagAfterDrainHidesASystemMessageTest`（正しい形は `systemMessageDuringTheDrainIsHandledTest`） |
| `DropSystemOnClose` | 閉じるときに残りの system message を捨て、閉じたあとの system message も捨てる。この仕様の最初の版（`e702bbe`）の誤りである（設計レビュー H2） | `systemMessagesConserved` | `dropSystemOnCloseLosesAWatchTest`（正しい形は `remainingSystemMessagesAreDeadLettersTest`、`systemMessageAfterCloseIsADeadLetterTest`） |
| `CloseDuringRun`、`KeepQueuedRun`、`PanicKeepsScheduled`、`PanicDropsSysFlag`、`PanicAsRecoverable`、`SilentSystemFailure` | 段 3 で足した 6 つ（§11.11.7 の表） | `noInvokeAfterClose`、`submittedWithWorkOrStaleRead`、`scheduledMatchesTheRun`、`noHiddenSystemMessage`、`settledMeansIdle`、`lossOnlyWhenFatal` | §11.11.7 の表 |

`LateEnqueue` は、Pekko でも起きうる取りこぼし（閉じた直後に積まれたメッセージが、dead letters にならずに消える）
である。この設計では、閉じる門の中で見てから積むので、起きない（I6）。これは、Pekko のテストと文書が許している振る舞いの
範囲の中で、選べる振る舞いを狭めることで、意味論のずれではない（`docs/decisions/pekko-semantics-first.md` の例）。
I6 は、利用者への約束として、公開 API の文書には書かない（元帥の M1、2026-11-26）。

`DropSystemOnClose` を避ける理由は DeathWatch である。原典の `Mailbox.scala:318-338`（`processAllSystemMessages`）は、
「if we closed the mailbox, we must dump the remaining system messages to deadLetters (this is essential for
DeathWatch)」としている。閉じたあとの `systemEnqueue`（`:503-505`）も dead letters のメールボックスへ回す。
dead letters が Watch を受けると、DeathWatchNotification を返す。捨ててしまうと、Terminate と同じ実行の中で後ろに
入った Watch が消え、watcher には Terminated が永遠に届かない（`expect_terminated` が時間切れになる）。

手で並べた `run` だけでなく、ランダムな探索でも違反が見つかることを確かめた（§10 の 6、10、14）。ただし、誤りのモードの
乱数の探索は CI の対象外で、手で流す確認である（CI の `quint run` は `init`、つまり正しい形だけを探す。誤りのモードを
毎回確かめるのは、`quint test` の決まった筋書きの `run` である）。手で流すには、`action init = initWith(<モード>)` に
書き換えた写しを `quint run --invariant=invariant` で流す。段 3 の時点（§11.11.10）では、20 手・1 万本で 12 のモードが
3 回とも違反を見つけ、`LostWakeup` と `SilentSystemFailure` は種に左右された（30 手・5 万本では 3 回とも見つけた）。段 3 の最初の版では、足した停止の遷移が trace を早く終わらせ、`LostWakeup` と
`PartialDrain` が 20 手・1 万本では見つからなくなっていた（再レビューで、`PartialDrain` は種によって見つかったり
見つからなかったりした）。停止を外からの操作の 1 つにまとめ直して、`PartialDrain` は 20 手・1 万本で安定して見つかるようになった（§11.11.10）。以下は段 2 までの記録である。
誤りの 6 つのモードから始めた `quint run`（20 手、1 万本）は、6 つとも違反を見つけた。再レビューで見逃していた変異 R12（外からの Suspend の
あとの Resume が効かない）も、幽霊の変数を数に替えた版では探索が見つけた。設計レビューで探索が見逃した変異 3 つ（suspend 中の
予約、Resume を失う、親に Failed を伝えない）も、I13 と I14 を足した版では探索が見つけた。正しい形は、CI と同じ
条件（20 手、1 万本）でも、60 手・10 万本でも違反が無かった。最初の版は、Apalache による 12 手までの有界の検証でも
違反が無かった（§10 の 8。直した版では流していない）。

### 5.4 この仕様で扱わないもの（後の段で書く仕様）

| 仕様（案） | 段 | 状態と不変条件 | 負の対照 |
|:---|:---|:---|:---|
| `actor_termination.qnt` | 段 4a、4b | T1〜T5。子の集合、「停止中で、開いていて、suspend されている」状態、post_stop、閉じたか、通知の順。段 4b から、失敗と監督（`Failed`、`Suspend`、`Resume`、`Recreate`、取り置き、all-for-one。§11.13.11） | `post_stop` の前に Terminated を送る。子が残っているのに閉じる（子の ChildTerminated が閉じたメールボックスに届く）。段 4b の負の対照は §11.13.11 |
| `dispatcher_lifecycle.qnt` | 段 6（I15 の部分は段 5 で `mailbox_scheduling.qnt` に先に足す） | 住人の数、停止の予定（UNSCHEDULED／SCHEDULED／RESCHEDULED）、実行器の生死、止めた回数。不変条件：住人がいる間は止めない、止めたあとの仕事は断らずに実行器を作り直す、断られたメールボックスの scheduled の bit は残らない | 作り直さない実行器（止めたあとの仕事が断られる）。断られたあとで bit を下ろさない（設計レビュー M2） |
| `balancing_team.qnt` | 段 7a | 共有のキュー、team の各メールボックス。不変条件：1 件のメッセージを処理するのは 1 人だけ、同じアクターが 2 スレッドで動かない、共有のキューに仕事があれば誰かが予約されているか予約しに行く、一員が止まっても共有のキューは流さない | 仲間が抜けるとき（`unregister`）に teamWork をしない。抜けた仲間が予約していた分が共有のキューに残り、誰も予約されない（原典 `BalancingDispatcher.scala:100-104` が teamWork を呼ぶ理由）。一員が止まるときに共有のキューを dead letters へ流す（M4） |

priority の順序、メモリの見え方（I10）、throughput の期限（I7 の後半）、CallingThread の再入は、Quint ではなく
単体テスト、PBT、stress テストで確かめる（§7）。

## 6. 段に分けた実装の計画

大きさは相対の見積もりである。基準は [PR #217](https://github.com/j5ik2o/fraktor-ng-rs/pull/217)（#207：kernel の
API 3 つと単体テスト、8 ファイル）を 2 とする。どの段も、設計の差分があれば先に設計の独立レビューを通す。
どの段も「既存のテストがすべて通る」を完了の条件に含む。

始める前に要るもの：D2 は承認された（§9.2）。段 0 と段 1 は、この PR の承認とマージのあとに、別の issue と PR で始める。
速さの測り方と報告の仕方は §7.5 で決めた。段 1、3、4、5 では速さを測って報告するが、速さでは作業を止めない
（`docs/decisions/pekko-semantics-first.md`）。止めるのは、各段の完了条件のうち、正しさの条件だけである。

| 段 | issue にする単位 | 中身 | 完了の条件 | 大きさ |
|:---|:---|:---|:---|:---|
| 0 | kernel を no_std で build できるようにする（元帥の C4、2026-11-26） | crate は割らない。lib.rs を `#![cfg_attr(not(feature = "std"), no_std)]` にし、adaptor を `std` の機能（既定で有効）の後ろに置く（lib.rs は kernel ではないので、`cfg-std-forbid` には触れない）。kernel の各ファイルで `alloc` を明示して import する。`AtomicU64` の 3 か所（`NEXT_ID`、`NEXT_SPAWN`、`NEXT_LIST`）と、kernel のほかの原子操作を `portable-atomic` にする（C1。各ファイルが `portable_atomic` を直接 import し、`kernel::sync` からは再公開しない。switch の `AtomicBool` は core のままにし、CAS の無い target 向けの置き換えは段 0b（#243）に回す。段 0 のあとの直し。§11.9）。`AtomicU64::try_update`（`double_linked_list_value.rs`）は portable-atomic に無いので `fetch_update` にする（再レビュー 3 の L5）。kernel の `Arc` を `kernel::sync::Arc` にする（C3。CAS のある target では `alloc::sync::Arc` のまま）。`spin` に `portable_atomic` の機能を足す。`getrandom` を adaptor の側の依存にする。前提：`portable-atomic` と `portable-atomic-util` のライセンスの記録を、段 0 の PR より前に #206 の側へ入れる（§7.4 の追記の案。元帥、2026-11-26） | 既存のテストのすべてが通る。CI に `cargo check -p fraktor-actor --no-default-features --target thumbv7em-none-eabihf` を足し、通る（CI の実装は段 0 の PR で行う。再レビュー 3 の試作では、この段の直しでエラーが 0 になった）。no_std の check には警告が 2 件残る（`executor::next_actor_ref` の使われない import と、`ActorRef::boundary_key` の使われないメソッド。どちらも adaptor だけが使う kernel の `pub(crate)` の口で、std なしでは使う側が無くなる。kernel には `cfg(feature = "std")` を置けず、抑制は人間が決めるため段 0 では残した。片づける先は段 1（[Issue #249](https://github.com/j5ik2o/fraktor-ng-rs/issues/249) のコメント）で、段 1 で無理なら段 0b（#243）で片づけ、その理由を #249 に書く。段 0 のあとの直し。§11.9）。根拠は `docs/decisions/crate-structure-comparison.md` の 2026-11-26 の節 | 3 |
| 0b | CAS の無い target（thumbv6m、RP2040）で build できるようにする（C3 の作り方。[Issue #243](https://github.com/j5ik2o/fraktor-ng-rs/issues/243)。元帥の M2、2026-11-26） | `Arc<dyn …>` を作る所を、どの target でも通る形にそろえる（`Arc::from(Box::new(x) as Box<dyn …>)` か、同じことをする小さなマクロ。CAS の無い target では割り当てと複写が 1 回増える）。場所は、`Props::new`、`FunctionBoundary`、`NoOpBoundary`、`SupervisorStrategy` の decider と hook、`ActorSystemSetup`、`Logging/error.rs` など。`self: Arc<Self>` を受け手にする公開 trait（`EventSink::into_any`）の受け手を替える。この 2 つの作り方は #243 の設計で決める | CI に `cargo check -p fraktor-actor --no-default-features --target thumbv6m-none-eabi` を足し、エラーが 0。公開 API の変更点の一覧（`EventSink` の受け手のほか、変わる型とメソッド）を #243 の設計と PR に載せる。既存のテストのすべてが通る | #243 で見積もる |
| 1 | Send にする（挙動は変えない）。この PR のマージのあと | §2.3 の置き換え。Arc が増える場所は §2.4 の 4 か所だけ。`ActorEventBus` は、§2.4 の形に作りを替え、`publish` の経路にロックを入れない。`spin::Mutex` は `try_lock` で取り、取れなければ「再入または同時の利用」で panic する（今の `RefCell` の二重借用の panic と同じ振る舞いを保つ）。`ActorSystem`、`WeakActorSystem`、`EventStream` が `Send + Sync` であることのコンパイル時の検査を `tests/` に足す。`ActorSystem` と `WeakActorSystem` の rustdoc に「段 5 より前は、別のスレッドから同時に使うと panic する」と書く | 検査を先に足して E0277（9 件）で落ちることを確かめる（red）。置き換えのあと、検査と既存のテストのすべてが通る。`receive` の中から `WeakActorSystem` を呼ぶ再入のテストが、止まらずに panic で返る（設計レビュー M3。base でも段 1 でも通ることを、レビューが試作で確かめた）。§7.5 の S1〜S4 を測って報告する。§7.5 の計測のテストを入れる。`tests/event_bus_spec.rs` と `tests/g0f_review_f_terminated_subscription.rs` が書き換えなしで通る（購読者が止まったときの debug のイベントの時点と、止まった購読者の片づけ） | 2 |
| 2 | kernel のメールボックス（配線しない） | `MailboxStatus`、`MessageQueue` と 4 種類のキュー、`SystemMessage` とそのキュー、閉じる門、ロックの port（`RawLock`、`LockCell`、`LockFactory`）、`MessageQueueFactory`、`MailboxRun`（§3.2）。§3.8 の工夫 1、2、3（port まで）、5 を入れる。工夫 2 と 5 は、実装の前に `mailbox_scheduling.qnt` を直す（予約を「読む」と「`fetch_or`」の 2 手に分け、I13 を直す。system message の印と、印を下ろす順を逆にした負の対照を足す）。どこからも使わない。D1 の条件 2 により、型の形は Balancing の共有のキューと std のロックを先に入れて決める | §7.3 の P1〜P3 の PBT。状態の操作が Quint の状態の部分と一致する。共有のキューを持つ 2 つのメールボックスで、片方を閉じても共有のキューが残ることの単体テスト | 3 |
| 3 の前 | 順序の洗い出し（D1 の条件 1） | 段 3（全体の FIFO から、アクターごとの順へ）と段 4a（子の `pre_start` が非同期になる）で変わる順序に、頼っている既存のテストを洗い出す。試作の写しで段 3 と段 4a の振る舞いを入れて既存のテストを流し、落ちたものを一覧にする。一覧の各行に、原典ではどう振る舞うか（同期か非同期か、順序を保証するか）を書く。今の 1 スレッドの実行器の同期や全体の順は、作りが Pekko と違うことで生まれた強さで、`docs/decisions/pekko-semantics-first.md` により、作りのずれとして直す側に置く | 一覧が issue に載っている。各行について「実装で原典どおりに直す」「テストが原典より強い前提に頼っているので直す」のどちらかが決まっている。テストを直すものは、移植の規約（実装ステップはテストファイルを編集しない、移植したテストを弱めない）に従い、triage への差し戻しとして扱う。原典より弱めることになるものは、人間の判断にかける | 2 |
| 3 | アクターごとのメールボックスと Manual dispatcher | `ShardQueue` の 1 本のキューを、アクターごとの `Mailbox` と `ManualExecutor` の実行待ちの列に替える。`ActorRef` の送り先を §3.4 の形にする。testkit の待ち方を `WakeSignal` に替える（cell のアクター宛ての `tell` が境界を通らなくなり、`await_envelope` が何も受け取らなくなるため。設計レビュー L3）。実行器の大きなロックと `&mut Executor` の文脈は、まだ残す | 既存のテストのすべてが通る（洗い出しの一覧で決めた直しを含む）。§3.4 の条件 1（Arc の複製の回数。満たさないときは数と理由を元帥経由でオーナーへ報告する）と条件 4（終了後にメールボックスが解放されるテスト）。§7.5 の S1〜S7 を測って報告する。testkit の待ちが `POLL_INTERVAL` の眠りに落ちていないこと（待ちの時間を測る） | 5 |
| 4 | アクターの間の操作を system message に（3 つに分ける） | 4a：生成と停止（Create、Terminate、子の名前、T1〜T5、閉じるときの dead letters）。4b：失敗と監督（Failed、Suspend、Resume、Recreate、all-for-one）。4c：DeathWatch と終了の通知、stop-and-reply、stash。文脈から `&mut Executor` を外し、自分のセルとシステム全体の共有の表を借りる形にする。共有の表は §3.2 のロックの決まりに従う | 各段で既存のテストのすべてが通る（洗い出しの一覧で決めた直しを含む）。4a で `actor_termination.qnt` を足す。§7.5 の S1〜S4 を測って報告する | 5、5、3 |
| 5 | std のスレッドプール | `ThreadPoolExecutor`、fork-join 相当の並列度、Pinned、`Dispatchers` の登録表、スレッドの名前、std のロック、`BlockingBoundedQueue`、dispatcher ごとのメールボックスの種類、プールが数える「同時に実行中のメールボックスの最大数」（ConsistencySpec の記録に使う。§8）。`execute` が断られたときの扱い（§3.5）。実装の前に、`mailbox_scheduling.qnt` に `reject` と I15 を足す（再レビュー L4） | §7.3 の P4（stress）。§7.5 の S1〜S7 と P1〜P3 を測って報告する（スレッドをまたぐ `tell`、アクターの数を増やした場合。設計レビュー L1）。ConsistencySpec（#235）、DispatcherActorsSpec、PinnedActorSpec、DispatcherActorSpec が移植できる状態 | 5 |
| 6 | dispatcher の寿命 | 住人の数、自動停止と作り直し、システムの終了で全部を止める（T6〜T11）、観測口、dispatcher でクロージャを実行する口、外からの suspend／resume、kernel の `Scheduler` の port（停止の猶予を数える。設計レビュー L4） | `dispatcher_lifecycle.qnt`（断られたあとで bit を下ろさない誤りを負の対照に含む）。DispatcherShutdownSpec、DispatcherModelSpec が移植できる状態 | 3 |
| 7 | Balancing と CallingThread | 7a：Balancing（共有のキュー、teamWork）と `balancing_team.qnt`。7b：testkit の `CallingThreadDispatcher` | BalancingDispatcherSpec、BalancingDispatcherModelSpec、CallingThreadDispatcherModelSpec が移植できる状態 | 3、3 |
| 8 | 後始末 | 使われなくなった `ShardQueue` と、`ShardBoundary` の役目の整理。文書の更新 | 既存のテストのすべてが通る | 2 |

最初の段（段 1）だけで、既存の挙動を変えずに Send にするところまで進む。試作で、その形の変更が既存のテストを書き換えなしで
通すことを確かめた（§10 の 3、9。再レビューは `7ecb3f5` の写しで、検査 2 件を足した 1150 件が通ることを確かめた）。
件数は、段 1 の issue で、その時点の main で数え直す。

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
  増えるのは、testkit が `WeakActorSystem::run_one` で回すとき（と、`attach_wait_driver` で登録した ask の待ち。§2.4）の
  `upgrade`（原子操作の増減 1 組）とロックの CAS で、
  この経路を `try_lock` の形で測り足した（§10 の 11）。中央値は、元が 80.2〜81.0 ns、段 1 が 81.5〜82.2 ns で、
  1 メッセージあたり 1〜2 ns（1〜2%）増える。1 回目だけは段 1 が 92.6 ns だったが、同じ回はロックを通らない経路も
  含めて段 1 の側がすべて約 13% 遅く、機械の揺れと見て外した。段 3 と段 5 では、メッセージの経路（スレッドを
  またぐ `tell`、アクターの数を増やした場合）を完了の条件として測る（§6）
- 段 3：宛先の線形の検索（アクターの数に比例）が無くなり、送り先に直接積む。アクターが多いほど速くなる見込みで、
  今の値は 100 で約 340 ns、1000 で約 4 µs、1 万で約 39 µs である（§7.5 の S5〜S7）。
  代わりに、送るたびにキューのロックを 1 回取り、メールボックスが idle なら状態の CAS を 1 回行う
- 段 5：スレッドをまたぐと、起こすのに `Condvar` の通知が要る（マイクロ秒の単位）。throughput で、1 回の起床あたりの
  処理の件数を増やして薄める。キューのロックは段 2 から port で、複数スレッドの dispatcher では `spin` を使わず、
  std の adaptor が渡す休むロックを使う（§3.2。再レビュー L3）。段 5 では、その形で §7.5 の P1〜P3 を記録する
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
  意味は変えない。ロックの port（`RawLock`、`LockFactory`）は dyn で呼ぶ形で、型引数は公開 API に出さない（§3.2）。
  dispatcher を登録して作るシステムには、`LockFactory` と `MessageQueueFactory` を渡す口が増える（足すだけ）。
  段 0b（#243）では、`self: Arc<Self>` を受け手にする公開 trait（`EventSink::into_any`）の受け手が変わる（狭める変更。
  一覧は #243 で作る）
- 依存：段 0 で `portable-atomic`（`fallback`、`critical-section` の機能）を足し、`portable-atomic-util` は target の cfg（`not(target_has_atomic = "ptr")`）の依存（`[target.'cfg(not(target_has_atomic = "ptr"))'.dependencies]`）に置き、`spin` に
  `portable_atomic` の機能を足す。`getrandom` は adaptor の側の依存にする。前の版で足すとしていた `lock_api` は足さない
  （C2）。std の側の休むロックに `parking_lot` を使うなら、それも足す（段 2 で決める）
- #206 のライセンスの記録への追記の案：#206 の記録（`docs/design/issue-206-license-policy.md`）には、依存の crate の
  ライセンスを並べる節がまだ無い。次の行を足す案を出す。追記は #206 の担当に頼み、段 0 の PR の前に入れる

  | crate | 使う段 | ライセンス | 使い方 |
  |:---|:---|:---|:---|
  | `portable-atomic` | 段 0 | Apache-2.0 OR MIT | kernel の原子操作（C1） |
  | `portable-atomic-util` | 段 0 | Apache-2.0 OR MIT | CAS の無い target での `Arc`（C3） |

  ライセンスは、どちらも crate の `Cargo.toml` の `license` の欄で、段 0 の PR で `cargo metadata` を見て確かめ直す
- 決定文書：所有権モデルの決定文書を改めた（D2、条件 5）。速さより Pekko の意味論を優先する決定を足した。crate の作りの
  決定に、fraktor-actor の kernel の no_std の要求が確定した節を足した
- 保存済みのデータ：無し
- 上流：無し（このリポジトリの中だけ）
- 守りの仕組み：CI の `quint` ジョブの対象が 1 本増える。ほかの仕様が 3 本のときに手元で測ると、スクリプト全体が 4.3 秒から 8.6〜10.4 秒になった（レビューのあとの版。2 回測った）。syn-check の規約
  （`type-per-file`、`cfg-std-forbid`、`port-adaptor-boundary`）に合わせて、§3.2 の型は 1 ファイル 1 型、
  std を使うものは adaptor に置く

### 7.5 速さの測り方と報告の仕方（実装の前に決める）

オーナーの指針（2026-11-26、元帥の項目 420。`docs/decisions/pekko-semantics-first.md`）により、速さでは作業を止めない。
遅くなっても、今は意味論を重視して、Pekko と同じスレッドモデルにする。速さは毎段で測り、数字と、何が効いているかを
報告する。作業を止めるのは、正しさの条件だけである（下の「止めるもの」）。段ごとに測り方を変えると、後から都合のよい
数字を選べてしまうので、測り方はここで固定する。

**何を測るか**：手で回す dispatcher（`ActorSystem::new` の既定。単一スレッド）で、次の 7 つの場面を測る。値は 1 メッセージ
あたりの時間である。今の値は `7ecb3f5` で測った（§10 の 13）。

| 場面 | 中身 | 今の値（中央値、3 回の幅） |
|:---|:---|:---|
| S1 外からの `tell` | 外から 1 つのアクターへ 20 万件を `tell` し、`run_until_idle` で回す | 97.0〜101.7 ns |
| S2 自分への `tell` | `receive` の中で自分に送り返して、20 万件を回す | 80.2〜82.0 ns |
| S3 testkit の回し方 | S2 を `WeakActorSystem::run_one` の繰り返しで回す（testkit と同じ経路） | 81.1〜87.8 ns |
| S4 2 つのアクターの往復 | 2 つのアクターが `sender` へ送り返して、20 万件を回す | 80.1〜95.5 ns（再レビュー 2 の測りでは、同じ機械で 8 回が 80.1〜82.7 ns に収まった） |
| S5 アクター 100 | 100 のアクターへ順に 20 万件 | 335.8〜358.5 ns |
| S6 アクター 1000 | 1000 のアクターへ順に 20 万件 | 3960.6〜4119.5 ns |
| S7 アクター 1 万 | 1 万のアクターへ順に 10 万件 | 38636.1〜39016.5 ns |

S5〜S7 が大きいのは、今の実行器が宛先を線形に探すためである（アクター 1 つあたり約 3.9 ns）。段 3 ではこの探索が
無くなるので、速くなるはずである。比べる目安として、ractor のアクターは約 100 ns／件、素の tokio のチャネルは速い場合で
約 12 ns／件である（§3.8）。

**どの機械で測るか**：オーナーの手元の機械で測る（Apple M4 Pro、14 コア、64 GB）。CI の機械は共有で揺れるので使わない。
この機械も、ほかの bot やワーカーと共有している。`7ecb3f5` を測ったときの 1 分の load average は 21〜28 で、揺れの大きい
場面では、同じ版の中でも S3 で 8%、S4 で 19% の幅があった。そのため、絶対値ではなく、次の「交互に測る」形の比で報告する。
測った時刻と load average は、毎回記録する。

**何回測るか**：

1. 比べる 2 つの版（基準 A と変更 B）を、同じ `--release` で作る。基準 A は、段に入る直前の main の commit である。
   段をまたいだ積み重なりも見えるように、`7ecb3f5` も毎回一緒に測る（A0）
2. 1 回の実行では、場面ごとに 7 回測り、中央値を取る
3. A、B、B、A、A、B、B、A の順で 8 回実行する（交互に 4 組）。組ごとに比 B／A を出し、場面ごとに 4 つの比の中央値（真ん中の 2 つの平均）を取る
4. 同じ組の中で、実行器を通らない対照（純粋な計算のループ）の値が 5% 以上ずれたら、その組は機械の揺れとして捨て、
   測り直す（`7ecb3f5` の測り直しで、1 回だけ全場面が 13% ずれた回があった。§7.2）。測り直しは同じ段で 2 回まで
   とする。それを超えたら、その時点では測れないこととして報告する（再レビュー 2 の L2）

再レビュー 2 は、同じバイナリを A と B に置いた A/A の 8 回で、S2 と S4 の比の中央値がどちらも 0.994、組ごとには
±2.5% の中だった。段 1 で道具を入れるときに、A/A の比を 1 回記録して、揺れの幅を報告に添える（再レビュー 2 の L3）。

**報告すること**：毎段で、次の 3 つを段の PR に載せ、元帥へ報告する。

- S1〜S7 の A との比と A0 との比（段 5 は、スレッドプールの P1 1 本の送り手から 1 つのアクター、P2 8 本の送り手から 64 の
  アクター、P3 ConsistencySpec の所要時間も）
- 比が揺れの幅（A/A の比）を超えて動いた場面について、何が効いているか（説明のつかない数字のまま出さない）
- 測れなかった組と、その理由

**止めるもの**：作業を止めるのは、正しさの条件だけである。

- 所有権モデルの改訂の条件 2〜4（戻りの辺、閉じるときに dead letters へ流す、メールボックスが解放される）
- Quint の不変条件（`mailbox_scheduling.qnt` と、段ごとに足す仕様）
- 移植した Spec（ConsistencySpec など）と、既存のテスト

改訂の条件 1（Arc の複製の数）は残す。段 3 のテストで数を確かめ、満たさないときは、数と理由を元帥経由でオーナーへ報告する
（条件 1 はオーナーが承認した範囲の一部だから。再レビュー 3 の L1）。

**段とのつながり**：

| 段 | 測るもの | 完了の条件 |
|:---|:---|:---|
| 1 | S1〜S4 | 測って報告する（§7.2 の試作では 1〜2% 増） |
| 3 | S1〜S7。改訂の条件 1（Arc の複製の数）と条件 4（解放のテスト） | S1〜S7 は測って報告する。条件 4 は止めるもの。条件 1 は数を確かめて報告する |
| 4a〜4c | S1〜S4 | 測って報告する |
| 5 | S1〜S7（手で回す dispatcher のまま）と P1〜P3 | 測って報告する |

測る道具：`7ecb3f5` を測った計測は scratchpad の写しにある。段 1 の issue で、`crates/fraktor-actor/tests/` に
`#[ignore]` を付けた計測のテストとして入れる（`cargo test --release -- --ignored` で流す）。交互に流す手順は、
`scripts/` の小さなスクリプトにする。

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
| 2 | 段 2 の型の形を、Balancing の共有のキューと、std のロックも使える形に合わせる | §3.2 の「閉じる門」と「ロックの port」、§6 の段 2 |

選ばなかった (B) は、新しい実行器を横に作り、最後に既存のテストを移して古い方を消す形である。新しい側を既存の
テストで確かめられない期間が長くなるので、採らなかった。

### 9.2 D2：承認（オーナー、2026-11-26）

**D2：所有権モデルの決定文書を改め、セルのアクターすべてのメールボックス（手で回す dispatcher を含む。複数スレッドで動かすために要る形）を、ホットパスに置く Arc の例外として
認める。**オーナーが承認した（2026-11-26、元帥の項目 420）。オーナーの言葉は「つまり Pekko と似たようなスレッド
モデルになるってことね。承認」だった。決定文書の改訂はこの PR に入れた（`docs/decisions/ownership-model-arc-overhead.md`
の「改訂（2026-11-26）」の節）。範囲と条件は §3.4 にある。

| 案 | 中身 | 結果 |
|:---|:---|:---|
| (A) | 決定文書を改める。`ActorRef` が `Arc<Mailbox>` を指す。§3.4 の条件 1〜5 を付ける | 採った（設計の独立レビューの推奨と同じ） |
| (B) | 決定文書を変えない。`ActorRef` は id＋世代のままにし、配送のたびに並行の表を引く | 採らなかった。`tell` のたびに表のロックと検索が要り、止まったアクター宛ての扱いにも表の世代の検査が要るため |

段 1 の実装は、この PR が承認されてマージされたあとに、別の issue と PR で行う（元帥、2026-11-26）。段 1 で Arc が
増える 4 か所（§2.4）は、決定文書の「制御プレーンの共有」に当たると、改訂の節に書いた。

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


再レビューのあとに流したもの（13〜15）：

| 番号 | コマンド | 結果 |
|:---|:---|:---|
| 13 | `git archive 7ecb3f5` の写しで、§7.5 の S1〜S7 を `cargo test --release --test zz_bench` で 3 回（場面ごとに 7 回の中央値）。機械は Apple M4 Pro、14 コア、64 GB | §7.5 の表の値。測ったときの 1 分の load average は 21〜28（ほかの bot やワーカーと共有） |
| 14 | 直した仕様で `bash scripts/quint-check-specs.sh`（main の 6 本の仕様すべて）。誤りの 6 つのモードから `quint run`（20 手、1 万本）。変異 R4〜R6 と R12（外からの Suspend のあとの外からの Resume を捨てる）を入れた写しで `quint run`（20 手、1 万本）と `quint test`。正しい形を 60 手・10 万本 | スクリプトは exit 0（`mailbox_scheduling.qnt` は 20 passing、`No violation found`、`max=21`。ほかの 5 本も通る）。6 つのモードはどれも `[violation]`。R4〜R6、R12 はどれも `[violation]`（`quint test` は 18〜19/20）。正しい形は `[ok]`、`max=53` |
| 15 | `7ecb3f5` で `cargo test --workspace` | 101 個のバイナリの 1148 件が通り、失敗は 0 |
| 16 | `mailbox_scheduling.qnt` の写しで、送り手の予約を `register`（読む）と `fetchOr`（closed と scheduled の 2 ビットだけを見る）の 2 手にし、強い I13、I13 を外した形、弱めた I13（`submittedWithWorkOrStaleRead`）のそれぞれで `quint run --invariant=…`（30 手・3 万本、60 手・10 万本）。I13 以外の不変条件を 1 つずつ（30 手・2 万本）。誤りの 6 つのモードと、変異 R4、R5、R6、R12、R13（30 手・2 万本） | §3.10 の表のとおり。強い I13 だけが正しい形で破れる。I13 を外すと R4 を見逃す。弱めた I13 は正しい形を通し、R4 も含めてすべての変異と 6 つのモードを捉える |
| 17 | Devin の指摘の筋書きを、前の版の仕様（`cc4b4d4`）と直した仕様の写しで `quint test --match=devinRepro`。直した仕様で `bash scripts/quint-check-specs.sh`（main の 7 本の仕様すべて）。誤りの 7 つのモードから `quint run`（30 手・2 万本）。正しい形を 60 手・5 万本 | 前の版：筋書きが通る（Suspend が残ったまま `startInvoke` でき、`invariant` も成り立つ）。直した版：`startInvoke` が止まる（`Tests failed`）。スクリプトは exit 0（`mailbox_scheduling.qnt` は 22 passing、`No violation found`）。7 つのモードはどれも `[violation]`。正しい形は `[ok]`、`max=53`（既定の rust の backend で流した。再レビュー 5 では、同じ版の正しい形を rust の backend で 25 手を超えて流すと `Error: invalid type: null, expected usize` で落ちることがあり、`--backend=typescript` では 30 手・3 万本と 60 手・2 万本で `[ok]` だった。道具の問題で、CI の 20 手には当たらない） |

| 18 | 段 2（#250）で直した仕様で `bash scripts/quint-check-specs.sh`。§3.10 の「段 2 で流し直した結果」の表（写しに `init<モード>` と比べる不変条件を足して `quint run --backend=rust`、`--backend=typescript`） | スクリプトは exit 0（`mailbox_scheduling.qnt` は 25 passing、`No violation found`）。表は §3.10 のとおり |

429、rate limit、overloaded は出なかった。

## 11. 設計の独立レビューの指摘の処理

### 11.1 最初のレビューの指摘の処理

レビュー：固定 `e702bbe`、Claude Opus（設計にも実装にも関わっていない）、2026-11-26。判定は要修正（高 2・中 8・低 8）。
レポートは bot の記録 `pr236-design-review.md` にある。どの指摘も、一次情報（原典、決定文書、コード）で確かめてから
処理した。

| 指摘 | 処理 | 変わった箇所 |
|:---|:---|:---|
| H1 D2 は確認ではなく、承認済みの所有権モデルを覆す判断である | 直す | §3.4 を書き直した（決定文書の該当箇所、スパイクとの関係、案 (A) の 4 つの条件、案 (B)）。§9.2 の D2 を「決定文書を改めるか」の判断にし、オーナーの判断待ちと書いた（その後、オーナーが承認した。§11.2）。§2.4 に、段 1 で Arc が増える場所を足した |
| H2 閉じるときに残りの system message を捨てている（DeathWatch を壊す） | 直す | 仕様：`Terminate` で残りの system message を `sysDeadLettered` へ、閉じたあとの `control` も dead letters へ。不変条件 `systemMessagesConserved`（I12）、負の対照 `DropSystemOnClose`、`run` を 3 本足した。設計：§3.2 の閉じる門の表、§4.1 の I6 と I12、§5.1、§5.3 |
| M1 「Terminate を処理した実行の中で閉じる」は T1 と原典に反する | 直す | 仕様の冒頭と §5.1 に、`Terminate` は子のいないアクターの `finishTerminate` に当たると書いた。§4.1 の I5 を「`finishTerminate` で閉じる」に直した。§5.4 の `actor_termination.qnt` に、停止中で開いていて suspend された状態と、その負の対照を書いた |
| M2 `execute` が断られたとき、scheduled の bit が立ったまま残る | 直す | §3.5 に、1 回だけ試し直し、だめなら bit を下ろしてエラーを出すと書いた。§4.1 に I15、§5.4 の `dispatcher_lifecycle.qnt` の不変条件と負の対照、§7.1 に入力を足した。実装は段 5 |
| M3 段 1 は、コンパイルで止まっていた同時の利用を、実行時の panic に変える | 直す | §6 の段 1 の完了条件に再入のテストを、中身に rustdoc の注意書きを足した。§9.3 の D5 の条件にした。§7.2 の計測は `try_lock` の形で測り直した |
| M4 閉じたかどうかをキューに持たせると、Balancing の共有のキューと両立しない | 直す | §3.2：`MessageQueue` から「閉じた」を外し、`Mailbox` の閉じる門にした。キューが自分専用か共有かで、閉じるときの扱いを分けた。§6 の段 2 に単体テストを足した |
| M5 kernel のキューに `spin` を焼き込んでいる。ロックの順序も無い | 直す | §3.2：ロックを `lock_api::RawMutex` の型引数にし、std の adaptor が休むロックを渡す形にした。ロックの順序の表と、持ったまま `tell` しない決まりを書いた（その後、§11.4 の C2 で、閉じた区間を渡す形の kernel の port に替えた） |
| M6 ExecutionContextSpec の 10 ケースを一括で「Scala の機能」とした | 直す | §8 を、BatchingExecutor を作れば移せる 3、段 6／7b で移せる 3、Scala の機能 4、SerializedSuspendable 4 に分けた |
| M7 順序に頼るテストの扱いが、移植の規約と結びついていない | 直す | §6 に「3 の前：順序の洗い出し」の段を足した。一覧を作ること、直す道（triage への差し戻し、弱めるものは人間の判断）を書いた。D1 の条件 1 でもある |
| M8 Quint の不変条件の穴（予約したのに仕事が無い、進まないこと） | 直す | 仕様に `submittedOnlyWithWork`（I13）と `settledMeansIdle`（I14、幽霊の変数 `externalSuspendIssued` を使う。再レビュー L5 で数に替えた）を足した。I13 は `canBeScheduled` を使わずに書いた（同じ定義を使うと、予約の規則の誤りが検査そのものも弱める。最初はそう書いて、変異 M4 を見逃した）。レビューの変異 M4〜M6 は、探索で見つかるようになった（§10 の 10） |
| L1 計測が、変えた経路を測っていない | 直す | §7.2 に明記した。testkit が回す経路（`WeakActorSystem::run_one`。段 1 でメッセージごとに `upgrade` が走る）を測り足した。段 3 と段 5 の完了条件に、メッセージの経路の計測を入れた |
| L2 D4 の `not_built` は台帳の定義に当たらない | 直す | §9.3 の D4 と §8 を、`do_not_port` かケースの除外の候補に直した。MailboxConfigSpec の SingleConsumerOnly も同じ理由で直した |
| L3 段 3 で `await_envelope` が何も受け取らなくなり、testkit の待ちが眠りになる | 直す | `WakeSignal` を段 3 へ前倒しした（§3.2、§3.6、§6 の段 3） |
| L4 自動停止に Scheduler の port が要る | 直す | §3.2 に kernel の `Scheduler` の port を、§6 の段 6 に依存を書いた |
| L5 I7 は期限も不変条件にしているが、仕様は throughput だけ | 直す | §4.1 の I7 に、仕様が見るのは throughput だけで、期限は単体テストで見ると書いた |
| L6 T1 に、最初の `unwatchWatchedActors` が無い | 直す | §4.3 の T1 に足した（原典 `FaultHandling.scala:185`） |
| L7 DispatcherShutdownSpec を、プール自身の数え方で確かめると漏れを見逃す | 直す | §4.3 の T8 と §8 を、スレッドの join の完了で確かめる形に直し、人間に出すときに添える点として書いた |
| L8 `ActorEventBus` は段 1 のあとも !Send のまま | 直す | §3.3 に書いた |

D1 は、元帥が (A) に決めた（条件 2 つ。§9.1）。D2 は、オーナーが承認した（§9.2）。

### 11.2 再レビューの指摘の処理

再レビュー：固定 `7ecb3f5`、Claude Opus（設計にも実装にも関わっていない）、2026-11-26。判定は承認（D2 を除く）で、
新しい指摘は低の 7 件だった。レポートは bot の記録 `pr236-design-rereview.md` にある。7 件とも「直す」にした。

| 指摘 | 処理 | 変わった箇所 |
|:---|:---|:---|
| L1 main の取り込みで、引いている行の番号がずれた | 直す | リポジトリのコードは、行の番号ではなく関数名、型名、フィールド名で指す形にした（ライセンスの見出しがまた入っても、ずれない）。§2.1、§2.2、§2.3、§2.4、§3.2、§3.4、§3.6。原典（固定の commit）は行の番号のまま |
| L2 テストの件数が古い | 直す | §3.1 を `7ecb3f5` の 1148 件（101 個のバイナリ）にした（§10 の 15）。§6 に、段 1 の issue でその時点の main で数え直すと書いた |
| L3 §7.2 の段 5 の spin の文言が §3.2 と矛盾する | 直す | §7.2 の段 5 を、「キューのロックは段 2 から port で、複数スレッドでは spin を使わない」に直した |
| L4 I15 の実装は段 5、仕様は段 6 | 直す | 段 5 で、実装より先に `mailbox_scheduling.qnt` に `reject` と I15 を足す、と §4.1、§5.4、§6 に書いた |
| L5 I14 の幽霊の変数が R12 を見逃す | 直す | 仕様の幽霊の変数を、真偽値 `externalSuspendIssued` から数 `externalSuspendsPending`（外からの Suspend で 1 増え、外からの Resume で 1 減る）に替えた。`run` を 1 本足した（`externalSuspendThenResumeHandlesTheMessageTest`）。R12 の変異が探索で見つかることを確かめた（§10 の 14） |
| L6 ロックの型引数 `R` をどこで選ぶか、依存の追加 | 直す | §3.2 に、`R` はキューにだけ付けて `dyn MessageQueue` の後ろで消し、dispatcher の設定の工場で選ぶと書いた。システム全体の表は `spin` のまま。足す依存と #206 の記録を §3.2 と §7.4 に書いた（その後、§11.4 の C2 で、表も port を使う形に替えた） |
| L7 Balancing でのロックの重なり | 直す | §3.2 のロックの決まりに、Balancing では送るときに門を見ず、門と共有のキューのロックを同時に持たない、と書いた |

あわせて、指示（元帥、2026-11-26）に従い、次の 3 つを入れた。

- D2 の承認を、§3.4、§9.2、冒頭に書いた。所有権モデルの決定文書に「改訂（2026-11-26）」の節を足した
- 速さの測り方と止める基準を、§7.5 に実装の前に決めて書いた。段 1、3、4、5 の完了条件とつないだ（その後、§11.4 で、
  オーナーの指針により「測って報告する。止めるのは正しさの条件だけ」に替えた）
- 仕様に、system message が通常のメッセージより先にさばかれることの不変条件 `systemBeforeNormal`（I11）と、
  その負の対照 `MessagesBeforeSystem` を足した。前の版の I11 は「一部だけ」で、CI では確かめていなかった

### 11.3 再レビュー 2 の指摘の処理

再レビュー 2：固定 `29908ed`、Claude Opus（設計にも実装にも関わっていない）、2026-11-26。判定は要修正（小さい。文書の直しだけ）。
レポートは bot の記録 `pr236-design-rereview2.md` にある。8 件とも「直す」にした。

| 指摘 | 処理 | 変わった箇所 |
|:---|:---|:---|
| (a)／M1 `ActorEventBus::publish` がイベントごとに購読表のロックを取る | 直す | 段 1 で、publish の経路からロックを外す作りにした（§2.3、§2.4、決定文書の「範囲の外」）。レビューの案は、片づけと debug のイベントを次の操作まで遅らせる形だった。しかし、`event_bus_spec.rs` の「unsubscribe subscriber when it terminates」は、止まった直後に debug のイベントを待つ。そこで、watch している集合だけを終了の通知と共有し、debug のイベントは今と同じ時点で出す形にした |
| (b)／M5 メッセージごとの `upgrade` の経路に ask の待ちが抜けている | 直す | `attach_wait_driver` で登録した ask の待ちを、決定文書の「範囲の外」、設計の §2.4 と §7.2 に書き足した。範囲の外のまま |
| M2 条件 1 の文言が、送り手のハンドルの複製で破れる | 直す | 元帥が決めた 2 文（2026-11-26）を、決定文書の条件 1 と設計の §3.4 の条件 1 に、同じ文字列で入れた。§3.4 の確かめ方に、今の 4 つと段 3 の 3 つの数え方を書いた |
| M3 例外の範囲の書き方が食い違っている | 直す | 「セルのアクターすべてのメールボックス（手で回す dispatcher を含む。複数スレッドで動かすために要る形）」にそろえた（決定文書の冒頭、残る場所の一覧、改訂の節の見出しと範囲、設計の §3.4 と §9.2） |
| M4 S5〜S7 の「1.00 以下」が揺れだけで止まる | 直す | §7.5 を、段 3 は A（＝A0）との比が 1.00 以下、段 4 以降は A との比が 1.05 以下かつ A0 との比が 1.00 以下、にした（その後、§11.4 で、速さの止める基準そのものを外した） |
| L1 4 つの比の中央値 | 直す | §7.5 に「真ん中の 2 つの平均」と書いた |
| L2 測り直しの上限 | 直す | §7.5 に「同じ段で 2 回まで。超えたら測れないこととして元帥へ上げる」と書いた |
| L3 S4 の今の値の幅 | 直す | §7.5 の S4 に再レビュー 2 の値を添え、段 1 で A/A の比を 1 回記録すると書いた |

### 11.4 元帥の決定とオーナーの指針の反映（2026-11-26）

| 項目 | 決めた人 | 変わった箇所 |
|:---|:---|:---|
| C1 原子操作は `portable-atomic` 越し | 元帥 | §3.2 の C1 と `MailboxStatus` の行、§6 の段 0、§7.4 の依存 |
| C2 ロックは閉じた区間を渡す形の kernel の port。システム全体の表もこの port。`with` の中で利用者のコードを走らせない | 元帥 | §3.2 の C2、「ロックの port」「ロックの中で利用者のコードを走らせない」「ロックの順序」、§4.1 の I16。セルは取り出してから `receive` を回す形にした（前の版の `try_lock` の検査の代わり） |
| C3 Arc は `kernel::sync::Arc` の別名。CAS の無い target も対象 | 元帥 | §3.2 の C3、§2.3 の注、所有権モデルの決定文書の「例外の範囲」 |
| C4 段 0（kernel を no_std で build できるようにする）。crate は割らない | 元帥 | §3.2 の冒頭、§6 の段 0、crate の作りの決定の 2026-11-26 の追記 |
| §3.7 tokio と embassy の adaptor の作り方 | — | §3.7 を足した |
| Rust のアクター実装から取り入れる工夫 6 つ | 元帥（承認済みの範囲の中） | §3.8。入れるもの：1（段 2〜3）、2（段 2。仕様を先に直す）、3（段 2 の port、段 5、embassy の adaptor）、5（段 2。仕様を先に直す）、6（1 つ目は済み、2 つ目は段 5）。入れないもの：4（今の層は untyped のため。typed の層で考え直す） |
| 1 回の実行で throughput の N 件を処理する（オーナーの訂正） | オーナー | §3.9。`run_one` だけは今の公開 API どおり 1 件 |
| 速さより Pekko の意味論を優先する | オーナー | 新しい決定 `docs/decisions/pekko-semantics-first.md`。§7.5 を「測って報告する。止めるのは正しさの条件だけ」に書き直した。§3.4 と所有権モデルの決定文書の条件 5 も同じ。§6 の完了条件を「測って報告する」に替えた |
| #206 のライセンスの記録 | 元帥 | §7.4 に、`portable-atomic` と `portable-atomic-util` を足す追記の案を書いた。#206 の文書そのものは書き換えていない |

`.takt/facets/knowledge/fraktor-porting-rules.md` への 1 行は、別の PR #239 で入れるので、この PR では触らない。

### 11.5 再レビュー 3 の指摘と元帥の条件の処理

再レビュー 3：固定 `0415893`、Claude Opus（設計にも実装にも関わっていない）、2026-11-26。判定は要修正（小さい）。レポートは
bot の記録 `pr236-design-rereview3.md` にある。M1 と M2 は元帥が決めた（2026-11-26）。

| 指摘 | 処理 | 変わった箇所 |
|:---|:---|:---|
| M1 `pekko-semantics-first.md` の「Pekko より強い保証も直す」が I6 と矛盾する | 直す（元帥の決定） | 決定の文を、元帥の文に絞った（許している範囲の中で狭めるのはずれにしない。約束を足すことはずれ。作りのずれは直す）。補いの出どころ（元帥、オーナーに報告済み）を書いた。I6 は公開 API の文書に書かない。設計の §5.3、§3.8 の工夫 2、§6 の「3 の前」を合わせた |
| M2 段 0 の thumbv6m は、書かれた直しだけでは通らない | 直す（元帥の決定） | 段 0 の完了条件を thumbv7em だけにした。段 0b（#243）を足した。中身は `Arc<dyn …>` の作り方と `EventSink::into_any` の受け手。完了条件は、thumbv6m の `cargo check` のエラーが 0 であることと、公開 API の変更点の一覧。§3.2、§3.7、§7.4、crate の作りの決定、所有権モデルの決定文書を合わせた。C3 の方針は変えていない |
| M3 I16 とセルの出し入れ | 直す | §3.2：ロックの中の `Drop`、セルを `set_as_idle` より前に戻す、巻き戻りでも drop guard で戻す、CallingThread の再入の見分け方。§3.5 の実行の順に「セルを取り出す」「セルを戻す」を入れた。§4.1 の I16 |
| L1 条件 1 の報告先 | 直す | 「元帥経由でオーナーへ」にした（§7.5、§6 の段 3、所有権モデルの決定文書） |
| L2 `RunOutcome::Idle` を返してよい時点 | 直す | §3.2 の `MailboxRun`、§3.5 の 6 |
| L3 embassy のチャネルの長さ | 直す | §3.7：N を「生きたメールボックスと、待っているクロージャの数の和の最大」以上にする |
| L4 `RawLock` の巻き戻りと毒 | 直す | §3.2 の「ロックの port」 |
| L5 `try_update` | 直す | §6 の段 0 に `fetch_update` にすることを足した |
| 元帥の条件：I13 を Pekko に合わせる | 直す | §3.10（新しい節）。弱めた I13 で検査を通し、強い I13 でしか捉えられない筋書き（R4）を、弱めた I13 の定義で捉えることを確かめた。§3.8 の工夫 2 に、Pekko 2.0.0 の `Mailbox.scala` の根拠を書いた |
| 元帥の条件：`run_one` | 直す | §3.9：`run_one` は testkit が 1 件ずつ進めるための公開 API で、メールボックスの 1 回の実行とは別のもの、と書いた。rustdoc の案も書いた。N 件の振る舞いを確かめるテストは `run_one` を通さない |
| 元帥の条件：portable-atomic の記録 | 直す | §6 の段 0 の前提に、段 0 の PR より前に #206 の側へ入れると書いた |

### 11.6 再レビュー 4 の低の指摘（段の issue を切るときに直す）

再レビュー 4：固定 `17be79b`、2026-11-26。レポートは bot の記録 `pr236-design-rereview4.md` にある。字句の 1 か所（M1 の文を元帥の文と一字一句そろえる）と低 L4（§3.5 の「2 の繰り返し」→「3 の繰り返し」）は、この PR で直した。残りの低の 4 件は、次のとおり段の issue を切るときに直す。

- L1：作りのずれと狭めの見分け方（作りから偶然生まれ、利用者が気づかずに頼りうる強さはずれ。意図して選んだ狭めはずれではない）を、決定か段の設計に 1 行足す。段 3 の前の「順序の洗い出し」の issue を切るときに直す
- L2：段 2 で、すべての予約の経路（送り手、実行の終わり、system message のあと）を「読む → `fetch_or`」の 2 手にし、`sawWork` を `canBeScheduled` ではなく `hasWork` で書いて、§3.10 の表を流し直す。段 2 の issue を切るときに直す
- L3：Terminate を処理して閉じた実行は、§3.5 の 4 でセルを戻さず、`with` を出てからロックの外で捨てる。段 4a の issue を切るときに直す
- L5：巻き戻りの drop guard はセルを戻すが、scheduled の bit は立ったまま残る。guard の中で bit も下ろすか、「catcher の無いシステムで panic が外に出たら、そのシステムは使い続けない」と書く。段 3 の issue を切るときに直す

### 11.7 Devin のレビュー（PR #236、ドラフトから外したあと）

| スレッド | 処理 | 変わった箇所 |
|:---|:---|:---|
| `mailbox_scheduling.qnt:303`（BUG）：空を確かめたあとに届いた system message を処理し始めても、前の確認が有効なまま通常のメッセージを取れる | 直す。指摘の筋書き（`drainDone`、`control(Resume)`、`control(Suspend)`、`processSystem`、`startInvoke`）は、前の版で通り、Suspend が残ったまま `receive` に入れた。原典の `processAllSystemMessages` は列が空になるまで処理するので、仕様の誤りである | 仕様：`processSystem` で `drained` を下ろす。幽霊の `drainMark` と不変条件 `noSystemSinceDrain` を足した。負の対照 `PartialDrain` と、正しい形の `startedDrainIsFinishedBeforeTheNextMessageTest` を足した（run は 22 本）。設計：§4.1 の I11、§5.2、§5.3。I11 の中身（追い越すのは多くても 1 件）は変えていない |
| §3.2 の閉じる門（240 行）：満杯で待つ送信が門を持ったままだと、閉じる側も取り出しも止まる | 直す | §3.2：満杯で待つときは門を外して待ち、起きたら門の中で閉じたかを見直してから積む（段 5） |
| §3.2 の `MailboxRun`（212 行）：`Again` のあとに adaptor が予約を取り直すと、立っている bit に阻まれる | 直す | §3.2：`Again` は予約に勝った状態で返す。adaptor は bit に触らず、列に積み直すだけ |
| §3.9 の `run_one`（494 行）：system message だけの実行で `false` を返すと、testkit が待ちに入る | 直す | §3.9：`false` は実行待ちが空のときだけ。system message だけの実行のあとは、次のメールボックスを続けて回す。rustdoc の案に 1 文足した（1 件だけ配ることは変えていない） |

### 11.8 再レビュー 5 の低の指摘（段の issue を切るときに直す）

再レビュー 5：固定 `b6d15d9`、2026-11-26、判定は承認。レポートは bot の記録 `pr236-design-rereview5.md` にある。低の 3 件は、次のとおり段の issue を切るときに直す（L1 の backend は、§10 の 17 にも書き添えた）。

- L1：仕様を深く流す手順に、`--backend=typescript` を書く（既定の rust の backend は、この仕様で 25 手を超えると quint の実行時エラーで落ちることがある。CI の 20 手には当たらない）。段 2 の issue を切るときに直す
- L2：満杯の待ちから起きた送り手は、門の中で「閉じたか」に加えて「まだ空きがあるか」も見直し、空きが無ければ残りの待ち時間でもう一度待つ。閉じる側は、閉じたときに待っている送り手を起こす。段 5 の issue を切るときに直す
- L3：`Again` の積み直しで `execute` が `Rejected` を返したときも、§3.5 の扱い（1 回だけ試し直し、だめなら bit を下ろしてエラー）にそろえると、`MailboxRun` の行に書く。段 2 の issue を切るときに直す

### 11.9 段 0 のあとの直し

段 0（[PR #262](https://github.com/j5ik2o/fraktor-ng-rs/pull/262)、main にマージ済み）の実装と、この設計のあいだにできたずれ 4 点を、元帥の判断で設計の文書を実装に合わせて直した。変わった点の設計レビューは行わない。[Issue #263](https://github.com/j5ik2o/fraktor-ng-rs/issues/263) の分も、あわせて直した。

| 項目 | 直した内容 | 変わった箇所 |
|:---|:---|:---|
| ずれ 1：原子操作の型を `kernel::sync` から再公開しない | `pub(crate) use portable_atomic::…` は syn-check の module-wiring（直属の子モジュール以外からの再エクスポート）に当たるため、段 0 では各ファイルが `portable_atomic` を直接 import した。`kernel/sync.rs` は `Arc` と `Weak` の別名だけを持ち、`kernel.rs` では可視性なしの `mod sync;` で宣言する | §3.2 の C1、§3.2 の `sync.rs` の行、§6 の段 0 |
| ずれ 2：switch の `AtomicBool` は core のまま | 単体テストが core の `AtomicBool` を作って渡すため、portable-atomic に替えると「既存のテストは書き換えなし」に反する。thumbv7em には 32 ビットの CAS があるので段 0 はこれで通る。CAS の無い target 向けの置き換えは段 0b（[Issue #243](https://github.com/j5ik2o/fraktor-ng-rs/issues/243)）に回す | §3.2 の C1、§6 の段 0 |
| ずれ 3：no_std の check に警告が 2 件残る | `executor::next_actor_ref` の使われない import と、`ActorRef::boundary_key` の使われないメソッドである。どちらも adaptor だけが使う kernel の `pub(crate)` の口で、std なしでは使う側が無くなる。kernel には `cfg(feature = "std")` を置けず（cfg-std-forbid）、抑制は人間が決めるため段 0 では残した。片づける先は段 1（[Issue #249](https://github.com/j5ik2o/fraktor-ng-rs/issues/249) のコメント）で、段 1 で無理なら段 0b（#243）で片づけ、その理由を #249 に書く | §6 の段 0 の完了の条件 |
| ずれ 4：`portable-atomic-util` は target の cfg の依存に置く | `[target.'cfg(not(target_has_atomic = "ptr"))'.dependencies]` に置いた。thumbv7em とホストでは使わないので、ふつうの依存にすると cargo の `unused_dependencies` の警告が出るためである | §7.4 の依存 |
| #263：§3.7 の表の「core と同じ」は誤り | thumbv7em の 64 ビットは、portable-atomic の中でスピンロックになり、割り込みから再入すると止まる（[Issue #263](https://github.com/j5ik2o/fraktor-ng-rs/issues/263)）と直した | §3.7 の「原子操作、Arc」の行の embassy の欄 |
| #263：embassy の adaptor を作る段の完了条件 | embassy の adaptor を作る段の完了条件に「#263 を閉じる（割り込みから送っても止まらないこと）」を入れた。embassy では割り込みから送るのがふつうの使い方なので、この条件を満たさない限り embassy 対応とは言わない（元帥） | §3.7（embassy で前提になるもののあと） |

### 11.10 段 2 の実装で決めたこと

段 2（[Issue #250](https://github.com/j5ik2o/fraktor-ng-rs/issues/250)）で、この設計に書いていなかった形を、次のとおり決めた。
どれも承認済みの不変条件の意味は変えていない（I13 は §3.10 の形にした）。I16 も例外なしで保つ（下の「priority と control-aware の関数」）。独立レビュー（固定 `b5e68e9`、判定は要修正、高 0・中 3・低 3）の指摘は、この表の各行に入れた。低 1（`ThroughputDeadline` の加算のあふれ）は、足すときに `Duration::MAX` で止める形に直した。

| 項目 | 決めた形 | 理由 |
|:---|:---|:---|
| 予約の 1 手目が読むもの | 印ではなく、通常のキューと system message のキューを読む（Pekko の `canBeScheduledForExecution` の hint は使わない） | 印や hint を読むと、仕事の無い予約が、古い読み（§3.10）より広く起きる。仕様の `register` はキューを読み、弱めた I13 はそれを前提にする。状態の語は 1 回だけ読み、閉じていればキューを読まず、suspend 中なら system message のキューだけを読む（独立レビューの低 3。scheduled の bit で読みを省くことはしない。省くと LostWakeup になる） |
| system message の印を立てる時点 | 門のロックの中で、積んだ直後に立てる | キューを見る者は、印より先にメッセージを見ない。仕様の `control` の 1 手に当たる |
| `RawLock` | `unsafe trait` にした。安全の条件は、1 回の呼び出しで区間を 1 度だけ走らせること、同じロックの区間を同時に走らせないこと（別のスレッドや割り込みだけでなく、同じスレッドの再入も入れない。再入した `lock` は panic してもよいし止まってもよいが、区間に入ってはいけない）、前の区間の書き込みを次の区間に見せること（ロックを取るのが acquire、外すのが release）、巻き戻りでロックを外すこと。毒を無視して次の区間を走らせるのは、安全の条件ではなく kernel が求める振る舞い | `LockCell` は区間の中で `&mut T` を渡すので、排他を守らない実装があると安全なコードから未定義の振る舞いになる。`lock_api` の `RawMutex` も unsafe trait である。embassy-sync の `CriticalSectionRawMutex` と `ThreadModeRawMutex`、`critical_section::with` は、同じスレッドからの再入を許す（embassy-sync の `Mutex` が健全なのは、`&T` しか渡さず、借用の検査を中の `RefCell` に任せるため）。そのため embassy の adaptor は、これらにそのまま任せず、ロックごとの「区間の中」の印などで再入を断ってから区間を走らせる（独立レビューの中 2、PR #273） |
| `kernel::sync` の可視性 | `pub mod sync;` にした（§3.2 の「可視性なしの `mod sync;`」から変えた） | adaptor が実装する port（`RawLock`、`LockFactory`）を公開するため。原子操作の型は今までどおり再公開しない |
| `Mailbox` が持つアクター | `Mailbox<C: MailboxCell>` の型引数にした。`MailboxCell` は `invoke` と `system_invoke`（Pekko の `ActorCell` の 2 つの入口） | 段 2 は配線しないので、`ActorCell` をまだ入れられない。段 3 で `C` を `ActorCell` にする |
| 門とキューのロック | 自分専用のキューでは、門のロックの中でキューのロックを取る（逆の順では取らない）。`enqueue` と、閉じるときの `drain` がこの形である。共有のキューでは門を取らない | キューは `Arc<dyn MessageQueue>` で差し替えられるので、門と同じロックにはできない。§3.2 のロックの順序の 3 の中の順として決めた。閉じるときの `drain` を門の中で行うのは、閉じたのに自分専用のキューにメッセージが残っている状態を誰にも見せず、2 度目の `close` が 1 度目より先に空で返らないようにするため（独立レビューの低 2） |
| priority と control-aware の関数 | どのロックの外でも呼ぶ。`MessageQueue` の積む手を 2 つに分けた。`classify`（ロックの外。利用者の関数を呼んでよい）が `ClassifiedEnvelope`（Envelope と優先度）を作り、`enqueue_classified`（キューのロックの中で、自分専用のキューでは門のロックの中でもある）は値を移すだけにする。関数が panic したときの Envelope の `Drop` も、どのロックの外で走る。公開の文書には、Pekko の `PriorityGenerator` と同じく「メッセージだけで決まる純粋な関数」とだけ書き、その中から送ってよいとは書かない | I16 を例外なしで保つため（独立レビューの中 1。前の版は門のロックの中で呼んでいた）。原典の `PriorityBlockingQueue` は比較の関数をキューのロックの中で呼ぶが、Pekko はこの関数の中で何をしてよいかを約束していない。ロックの外で呼ぶのは原典が許す範囲の中の選び方で、約束は足さない（`docs/decisions/pekko-semantics-first.md`） |
| std のロック | `std::sync::Mutex`（`StdLockFactory`） | 依存を足さないため（`parking_lot` は足さない。§7.4） |
| throughput、期限、優先度 | 専用の型 `Throughput`（0 は 1）、`ThroughputDeadline`（1ms 未満は無し）、`MessagePriority`（小さい数が先。型の順序は先に出るものが大きい） | 移植の規約の domain-primitive（特別な扱いを持つ値を基本型のまま渡さない）。Devin の指摘、PR #273 |
| bounded の priority と control-aware | 段 2 では入れない | 満杯の待ちと一緒に段 5 で入れる（dispatcher ごとのメールボックスの種類） |
| dispatch の型の公開 | `Mailbox`、`MailboxCell`、`MailboxRun`、`RunOutcome`、`SystemMessage` などを、段 2 では `pub` にした。公開の rustdoc には、原典が約束しない保証（I6 など）を書かず、門の作りは内部のコメントに書いた | 段 2 は配線しないので、`pub(crate)` だと使う所が無く `dead_code` の警告になる（抑制は人間が決める）。段 3 で `ActorCell` を入れるときに、`Mailbox`、`MailboxCell`、`SystemMessage` は `pub(crate)` に戻し、adaptor へ渡す `MailboxRun` は型引数の無い形にする（[#252 のコメント](https://github.com/j5ik2o/fraktor-ng-rs/issues/252#issuecomment-7193760572)の 4）。`Mailbox::suspend`、`resume`、`close` は、それまで実行中のスレッドだけが呼ぶ前提である（独立レビューの中 3） |

### 11.11 段 3 で決めること（[Issue #252](https://github.com/j5ik2o/fraktor-ng-rs/issues/252)）

段 3 の設計の差分である。実装の前に、設計の独立レビューを通す（§6 の冒頭）。扱うのは、段 2 のレビューから持ち越した
5 点（[#252 のコメント](https://github.com/j5ik2o/fraktor-ng-rs/issues/252#issuecomment-7193760572)）、§11.6 の L5、
元帥の点検で足した「Executor のアクターごとの状態を 1 か所にまとめる」、それと段 3 の中身（§6 の段 3 の行）である。

**変えないこと**：実行器の大きなロック（`ActorSystem` の `spin::Mutex<Executor>` と `try_lock`）、`ActorContext` が持つ
`&mut Executor`、生成と停止と監督と DeathWatch を実行器が同期で行うこと（段 4a〜4c で system message にする）、
`ShardBoundary` の trait の既存のメソッドと `Envelope::new` の形（テストが実装し、呼んでいる）、`run_one`・`run_until_idle`・
`await_envelope` の名前。複数スレッドの dispatcher は段 5 で、この段では入れない。

#### 11.11.1 形の全体

| 項目 | 今（main `7eb9d30`） | 段 3 |
|:---|:---|:---|
| メッセージの置き場所 | 実行器の `ShardQueue`（全体で 1 本の列）と、システムの境界 | アクターごとの `Mailbox<ActorCell>` の通常のキュー（kernel の `UnboundedQueue`）。境界は、セルの無い宛先（ガーディアン、function ref、ほかの境界、解決できなかったパス）と外からの選択（`ActorSelection`）だけに残る |
| 実行待ち | 無い（列の先頭から配る） | `ManualExecutor<C>`（kernel `dispatch/manual_executor.rs`）。`LockCell<VecDeque<ManualTask<C>>>` の FIFO。`ManualTask<C>` は `Run(MailboxRun<C>)` と `Envelope(Envelope)`（境界から来たもの、閉じたメールボックスに断られたもの）の 2 つ。型引数はメールボックスの `Mailbox<C>` と揃えるためのもので（11.11.4 の戻りの辺）、段 3 で作るのは `ManualExecutor<ActorCell>` の 1 つだけである |
| `ActorRef` の送り先 | `boundary: Arc<dyn ShardBoundary>` | `endpoint: Endpoint`。`enum Endpoint { Mailbox(Arc<Mailbox<ActorCell>>), Boundary(Arc<dyn ShardBoundary>) }`（§3.4 の案の名前のまま。`pub(crate)`）。等しさとハッシュは今と同じく id と世代だけ |
| `tell` | 境界へ `post`、実行器の中からは `ShardQueue` へ | `Mailbox` なら `Mailbox::send`：門の中で積み、`register_for_execution`（予約に勝ったら `ManualExecutor` に `Run` を積む）。`Envelope::new` は通らない。メールボックスに積むのはメッセージと送り手だけで、宛先のハンドルは複製しない（条件 1。今の `tell` は `Envelope::new(self.clone(), …)` で宛先を複製するので、段 3 の `tell` はメールボックスの宛先でこの呼び出しを分ける）。断られたら（閉じている、bounded が満杯）、そのときだけ宛先のハンドルを複製して付け、`ManualExecutor` に `Envelope` として積む。実行器がそれを今と同じ dead letters の規則で流す（下の「届けられなかった封筒」。この経路の数は 11.11.7）。`Boundary` なら今と同じく `Envelope::new` を通る |
| `Envelope` | 宛先、メッセージ、送り手 | 宛先は `Option<ActorRef>`。境界を通るとき（`Envelope::new`）と、断られて実行器へ回るときだけ持つ。メールボックスの中では持たない（メールボックスが宛先である。Pekko の `Envelope` も message と sender だけ）。条件 1 のため（下の 11.11.5） |
| 実行器の表 | `cells: Vec<ActorCell>` と、アクターごとの並行の表 | `actors: BTreeMap<(u64, u32), ActorEntry>`（id と世代で引く）。`ActorCell` そのものは `ActorEntry` ではなくメールボックスの中にある（下の 11.11.3） |
| 使わなくなるもの | — | `ShardQueue` を消す（§6 の段 8 から前倒し。段 3 で使う所が無くなり、残すと dead_code になる）。`ShardBoundary` の役目の整理は段 8 のまま |

**届けられなかった封筒**：今は、止まったアクター宛ての封筒も境界か `ShardQueue` に積まれ、実行器が配るときにセルが
見つからず dead letters へ流す（`Executor::deliver`）。段 3 でも、流す時点を同じにする。送り手のスレッドで dead letters へ
流すと、dead letters の購読者が `run_until_idle` の前に通知を受け取り、今と時点が変わるからである。メールボックスから
dead letters へ戻る辺は作らない（条件 2）。

**`run_one` と `run_until_idle`**（§3.9）：どちらも、まず境界から高々 1 件を取り、`ManualTask` の列の後ろへ回す（今の
「境界から 1 件取ってから配る」と同じ。境界が尽きなくても列が飢えない）。そのあと列の先頭から取る。

- `run_one`：`Run` は throughput を 1 にして回す。通常のメッセージを 1 件配ったら `true` を返す。system message だけの実行や、
  空の実行（§3.10 の古い読み）なら、列の次を続けて回す。`Envelope` は 1 件を配って（または dead letters へ流して）`true`。
  列が空になったら `false`
- `run_until_idle`：`Run` は設定の throughput（手で回す dispatcher の既定は Pekko と同じ 5。`reference.conf:622`）で回す。
  境界と列が両方空になるまで続ける
- rustdoc は §3.9 の案に書き直す（`ActorSystem::run_one` と `WeakActorSystem::run_one`）

**アクターの間の順**：今の全体の FIFO は、メールボックスごとの FIFO と、実行待ちの列の FIFO（throughput の件数ずつ）に
変わる。同じ送り手から同じ宛先への順（I9）は保つ。原典が約束するのはこの順だけである
（`message-delivery-reliability.md:109-130`）。#251 の一覧の段 3 の 2 行は、その一覧の直し方で red の工程のコミットに
分けて直す。一覧に無いテストが順の違いで落ちたときは、テストを書き換えずに原典の振る舞いを確かめ、中将へ上げる
（#251 の試作の順は、この段の順と同じではない。試作は「最後に積まれた宛先の、いちばん古い封筒」を選んだ）。

#### 11.11.2 1 回の実行と `&mut Executor` の文脈（持ち越しの 3 と 5、L5）

`MailboxCell` に、実行が貸す文脈の型を足し、`invoke` が「続けてよいか」を返す形にする。

```rust
pub(crate) trait MailboxCell: Send + Sized {
  /// What a run lends the cell: the executor in stage 3.
  type Env: ?Sized;
  fn invoke(&mut self, env: &mut Self::Env, mailbox: &Mailbox<Self>, envelope: Envelope) -> Next;
  fn system_invoke(&mut self, env: &mut Self::Env, mailbox: &Mailbox<Self>, message: SystemMessage);
}

pub(crate) enum Next { Continue, Yield }

impl<C: MailboxCell> MailboxRun<C> {
  /// `budget` overrides the throughput of the mailbox (`run_one` passes 1).
  pub(crate) fn run(self, env: &mut C::Env, budget: Option<Throughput>) -> RunReport<C>;
}

pub(crate) struct RunReport<C> { pub(crate) outcome: RunOutcome<C>, pub(crate) delivered: usize }
```

段 3 では `ActorCell` の `Env` は `Executor` である。`ManualExecutor` から取った `Run` を、実行器が自分を文脈にして回す
（`run.run(self, budget)`）。`ActorCell::invoke` は、今の `Executor::deliver` のうち `receive` を回すところ（特別なメッセージの
見分け、catcher、`ActorContext`、`become` の枠）を行う。

**`Yield`（11.6 L5 とは別の、段 3 の橋渡し）**：今の `deliver` は、`receive` のあとでセルを表に戻してから、同じアクターに
対する後の仕事をする。失敗の処理（再起動か停止）、`stop_and_reply` の停止、`Kill`、`PoisonPill`、`finish_boundary` の
停止と ChildTerminated の通知である。どれもセルが表にあることを前提にする（実行中のセルは表に無いので、
`stop_with_sender` は実行中の相手には何もしない）。段 3 ではセルは実行が持っているので、これらの仕事を `invoke` の中では
できない。そこで、`invoke` は、こうした仕事が残ったときに `Yield` を返し、実行器はその仕事を `ActorEntry` の
`after_run` に記録する。実行はそこで次のメッセージを取らずに終わり（throughput を使い切ったときと同じ終わり方。
§3.5 の 4〜6。待っている system message は処理し切ってから終わる。Quint の `yieldRun`）、`run` が返ったあと、セルが
メールボックスに戻った状態で、実行器が記録した仕事をする。仕事が残らないメッセージ（ふつうの配送）は `Continue` で、
throughput の件数まで続ける。段 4 でこれらが system message になり、実行の中で処理されるようになったら、`Yield` は消す。

`after_run` の値は次の 3 つである。

```rust
/// What a run left for the executor, to do once the run has returned (11.11.2).
enum AfterRun {
  /// `receive` failed under the catcher. `message` is the cause, `sender` the
  /// sender of the failed message, which `apply_failure` hands to the hooks as
  /// `sender()` (its `original_sender`).
  Failed { message: String, sender: ActorRef },
  /// `Kill`: the parent decides for a killed child (`handle_kill`).
  Kill,
  /// A stop: by `stop_and_reply`, or by `PoisonPill`. `sender` is the sender
  /// of the failed message when the message also failed, as today.
  Stop { sender: Option<ActorRef> },
}
```

`invoke` は、1 件のメッセージの結果から、次の表で `Next` と `after_run` を決める。上の行ほど優先する。今の `deliver` の
分岐をそのまま移したもので、時点も同じである。

| メッセージとその結果 | `Next` | `after_run` | 今の `deliver` |
|:---|:---|:---|:---|
| `Kill` | `Yield` | `Kill`（`receive` を呼ばない。下の「失敗の扱い」と同じく suspend する） | `handle_kill` |
| `PoisonPill` | `Yield` | `Stop { sender: None }`（`receive` を呼ばない） | `stop_at_boundary` |
| 失敗し、このアクターが `stop_and_reply` を登録している | `Yield` | `Stop { sender: Some(元の送り手) }`。停止を失敗より優先し、`apply_failure` は呼ばない | `Failed` の分岐の `deferred_stop` |
| 失敗した | `Yield` | `Failed { message, sender }` | `apply_failure` |
| 成功し、`stop_and_reply` を登録している | `Yield` | `Stop { sender: None }` | `Completed` の分岐の `self.stop` |
| 成功し、実行器の境界の仕事が残った | `Yield` | なし | 毎回の `finish_boundary` |
| 成功し、何も残らない | `Continue` | なし | `finish_boundary` は何もしない |

「境界の仕事が残った」は、`invoke` が自分の枠（`exit_frame`）を出たあとで、実行器の `pending_stops`、
`pending_decider_failures`、`pending_child_terminated` のどれかが空でないことである。今の `finish_boundary` は、この 3 つが
すべて空なら何もしない。したがって `Continue` を返すのは、今の `finish_boundary` が何もしない場合とちょうど同じで、
`receive` の中から頼んだ停止（`context.stop`）は、今と同じく次のメッセージの前に効く。throughput が 5 で、ほかの
アクターを止めるメッセージの後ろに 4 件が待っていても、2 件目は、その相手が止まったあとで処理される。

`invoke` が見分けるメッセージは、`Kill`、`PoisonPill` と、DeathWatch の通知（`Arc<PendingTermination>`）である。通知は、
今と同じく実行器の表で引き、表に無ければ（古い通知）配らずに `Continue` を返す。配らなくても、1 件取り出したことは
`delivered` に数える（今の `run_one` は、この場合も `true` を返す）。`ActorSelectionMessage` と `StopDeadLetterListener` は
境界を通るので、`ManualTask::Envelope` の側で今と同じに扱う。

`Kill` と `PoisonPill` の後ろのメッセージは、同じ実行では処理されない（`Yield` で終わるため）。`Kill` なら親の判断のあとに
回る（既定の decider は `Stop` なので、停止の `close` で dead letters へ行く）。`PoisonPill` なら停止の `close` で
dead letters へ行く。どちらも今と同じである。

`after_run` は、1 回の実行に高々 1 つしか生まれない。`Yield` を返したら、実行は次の通常のメッセージを取らないからである。
`Option` 1 つで足りるのはこのためで、失敗と停止の重なりは、上の表のとおり `invoke` が 1 件の中で決める。system message の
失敗（下）は、`after_run` が空のときだけ記録する（すでに `Failed` か `Kill` があれば、原典の `isFailed` と同じく後の失敗は
記録しない。`Stop` があれば停止が勝つ）。

**`run` が返ったあとの実行器の仕事**：実行器は、`ManualExecutor` から取った `Run` を回して返ったら、次の `ManualTask` を
取る前に、次の順に行う。

1. そのアクターの `after_run` を取り出す（`take`）
2. `Kill` なら `handle_kill`、`Failed` なら `apply_failure`、`Stop` なら `stop_with_sender`。セルは `try_take_cell` で
   取り出す（実行は返っているので、必ず取れる。取れなければ I1 と同じく panic する）
3. `Kill` と `Failed` で、判断を当て終えたあとにメールボックスがまだ開いていれば、Resume の system message を 1 つ送る
   （下の「失敗の扱い」）
4. `finish_boundary` をする（いつも。今の `deliver` の最後と同じ）

**失敗の扱い（catcher があるとき。原典の NonFatal）**：原典の `ActorCell.invoke`（`ActorCell.scala:568-581`）は、1 件ごとに
NonFatal な例外を捕まえて `handleInvokeFailure`（`FaultHandling.scala:215-234`）を呼ぶ。`handleInvokeFailure` は、そのアクター
を suspend し、親へ `Failed` を送る。親の判断（Resume、Recreate、Stop）が system message で届くまで、通常のメッセージは
処理されない。段 3 もこの形にする。

- `ActorCell::invoke` は、今の `deliver` と同じく `receive` を catcher で包む。失敗したら、自分のメールボックスを suspend
  する（実行の中なので I4 に合う。段 2 の Suspend の処理と同じ口）。そのあと上の表のとおり `Yield` を返す。suspend
  されているので、実行の終わりの予約は、通常のメッセージだけでは勝たない（Quint の `failInvoke`）
- 実行器は、`run` が返ったあと、今の `apply_failure` で親の判断（Restart、Stop、all-for-one、decider の panic なら
  親の失敗）を当てる。当て終えて、失敗したアクターのメールボックスがまだ開いていれば、Resume の system message を 1 つ
  送る（`Mailbox::system_enqueue` と予約。Quint の `decide`）。Restart なら再起動のあと、Stop で止まっていれば送らない。
  原典では、親の Resume、Recreate の終わり（`finishRecreate` の `resumeNonRecursive`）、escalate の先の判断（`faultResume`
  の `resumeChildren`）が、それぞれ失敗した子の suspend を解く。fraktor の `Directive` には Resume が無い（Restart と
  Stop だけ）ので、段 3 で Resume を送るのは、Restart のあとと、親の失敗（escalate）を当てたあとに子が残った場合である
- 段 3 では判断は `run` が返った直後に同期で済むので、suspend が実際に効くのは、同じ実行の残りと、判断の前の予約の分
  だけである。それでも Quint の `failInvoke` と `decide` と同じ形にしておき、段 4 で判断が system message になったときに
  書き換えずに済むようにする。失敗ごとに、Resume のための実行が 1 回増える（失敗はまれなので、速さの報告には含めない）
- system message も 1 件ごとに catcher で包む（原典の `systemInvoke` の `invokeAll`）。失敗したら、その 1 件の効果は残す
  （メールボックスの数の操作は、利用者のコードより前にある）。上と同じく suspend し、`after_run` が空なら `Failed` を
  記録して、残りの system message を処理し続ける（Quint の `failSystem`）。`Terminate`（段 4a）の `post_stop` の失敗は、
  今の `run_post_stop` と同じく publish するだけで、停止は最後まで進む（原典の `finishTerminate`）。段 3 で実行器が送る
  system message は上の Resume だけで、利用者のコードを走らせないので、この経路を使うのは段 4 からである
- 同じアクターが毎回失敗しても、1 回の実行で失敗するのは 1 件だけで（`Yield` で終わる）、そのあとは親の判断が決める。
  Restart の回数は今の `RestartLimit` で抑えられ、超えれば Stop になる。`run_until_idle` が返らなくなることはない

選ばなかった形：

- 段 3 の手で回す dispatcher の throughput を、いつも 1 にする。後の仕事の時点は保てるが、§3.9 のオーナーの訂正
  （1 回の実行で throughput の N 件）に、段 4 まで反する
- `invoke` の中で、実行器がセルを `&mut` で受け取って後の仕事もする。`apply_failure`、`stop`、`restart_cell`、all-for-one が
  セルを値で出し入れする形を、すべて書き換えることになる。段 4 で system message にするときに、もう一度書き換える
- 失敗しても suspend せず、判断が同期で済むことだけに頼る。段 3 では観測できる違いは無いが、Quint の `failInvoke` と
  形が合わず、段 4 で判断を system message にしたときに、判断の前に次のメッセージを処理する誤り（再レビューの高 2）が
  そのまま残る

**`RunReport::delivered`（持ち越しの 5）**：`run_one` が「通常のメッセージを配ったか」を知るためのもの。

**巻き戻り（catcher が無いとき。原典の fatal。§11.6 L5 と持ち越しの 2）**：catcher が無いシステムでは、`receive`、
system message、ライフサイクルのフック、function ref の handler の panic は捕まえられず、外へ巻き戻る。原典で `invoke` が
捕まえない致命的な例外（fatal）に当たる。原典では、`Mailbox.run` の `finally` が `setAsIdle` と予約をし、例外はスレッドの
uncaught exception handler へ行く。`ActorSystemImpl` の handler は、fatal ならシステムを終える（`jvm-exit-on-fatal-error`
が on なら JVM ごと終える）。回復は約束しない。段 3 もこれに合わせる。

1. メールボックスの guard（`MailboxRun::run`）は、原典の `finally` と同じことをする。セルを戻し（閉じていれば戻さず、
   ロックの外で捨てる。持ち越しの 1）、system message の列が空でなければ印を立て直し（`process_all_system_messages` の
   途中の panic で、印が下りたまま列に残るのを防ぐ）、scheduled の bit を下ろし（`set_as_idle`）、予約を読み直して、勝ったら
   `ManualExecutor` に `Run` を積む（戻りの辺の `Weak` を `upgrade` する。巻き戻りの経路だけ）。メールボックスは、この
   あと何が起きても I2、I3、I11 を保つ（Quint の `panicInvoke`、`panicSystem`）
2. 実行器は、その panic を致命的とみなす。`ActorSystem` と `WeakActorSystem` が実行器を借りる口を、closure を取る 1 つの
   関数にまとめ、closure が正常に返ったときだけ印を立てる。印が立たないままロックを外すとき（巻き戻ったとき）は、
   実行器を「致命的」の状態にし、実行中の枠（`running`）と `boundary_active` を空に戻す。このとき利用者のコードは
   走らせない（取り出した `ActorRef` などは捨てずに退避し、終了の処理で捨てる。巻き戻りの最中に利用者の型の `Drop` を
   走らせないため）。panic は、今と同じく呼び出し元（`run_until_idle` など）へ出る
3. 「致命的」の実行器に入る次の呼び出しは、何よりも先にシステムを終える（`stop_all`。原典の handler の `terminate()`）。
   そのあとは、今の終了したシステムへの呼び出しと同じに扱う（`run_until_idle` は何もしない、生成は断られる）。
   `ActorSystem` の drop は、今と同じく `stop_all` を呼ぶので、次の呼び出しが無くても解放される（条件 4）。
   原典の handler は fatal の時点で `terminate()` を呼ぶが、段 3 の終了は次の呼び出しまで遅れる。巻き戻りの最中には
   実行器を安全に呼べず、手で回す実行器には handler に当たる背景の担い手が無いためである。段 5 のプールでは、この終了を
   どのスレッドのどの呼び出しが担うかを、段 5 の設計に明記する

この形では、guard が予約し直したメールボックスは、もう回らない（次に回すときには、システムを終えている）。保留中の停止や
decider の失敗が止まったまま配送だけが進む状態（再レビューの高 2 の 1）は起きない。同じアクターの panic が繰り返されて
`run_until_idle` が返らないこと（高 2 の 2）も起きない（panic は 1 回で呼び出し元へ出て、そのあとシステムは終わる）。
panic で失われたメッセージと system message は、Quint では `lostToFatal` と `sysLostToFatal` に数え、処理済みには数えない
（I8、I12 で喪失が見える）。

段 5 のプールのスレッドも同じにする。スレッドを巻き戻った panic は、システムを終える（原典の handler と同じ）。前の版は、
「catcher の無いシステムで panic が外に出たら、そのシステムは使い続けない」案を、段 5 のプールでスレッドが 1 本死んでも
ほかのメールボックスは止めない、という理由で退けた。しかし原典も fatal のあとはシステムを終えるので、この理由は原典に無い
保証を足すものだった。取り下げる。

今との違い：今は、catcher の無い panic のあとも、そのアクターのセルを失ったまま、システムを使い続けられる。段 3 からは、
次の呼び出しでシステムが終わる。catcher の無いシステムで panic したあとにシステムを使い続けるテストは無い（`run_until_idle`
を `catch_unwind` で包むテストは、どれも catcher つきで、panic が外へ出ないことを確かめている。2026-11-27 に
`grep -rn catch_unwind crates/` で確かめた）。

**実行の始め（持ち越しの 1）**：`run` は、セルを取り出す前に `is_closed()` を見る。閉じていれば何も処理せず、
§3.5 の 5〜6 だけを行う（閉じたメールボックスの予約は必ず負けるので `Idle`）。

#### 11.11.3 アクターごとの状態を 1 か所にまとめる（元帥の点検、#278 の 4）

今の `Executor` は、アクターごとの事実を、アクターをキーにした並行の表に持つ。段 3 で、アクター 1 つの事実を
`ActorEntry` 1 つにまとめる。

```rust
struct ActorEntry {
  mailbox: Arc<Mailbox<ActorCell>>,            // the actor's mailbox; the cell lives inside it
  stash: Vec<(u64, Envelope)>,                 // was Executor::stashes
  capacities: (CapacityLimit, CapacityLimit),  // was Executor::actor_capacities
  supervision: ParentEntry,                    // was Executor::parents (strategy, children, stopping)
  deferred_replies: Vec<DeferredReply>,        // was Executor::deferred_replies
  after_run: Option<AfterRun>,                 // what a Yield left (11.11.2)
}
```

- 表は `BTreeMap<(u64, u32), ActorEntry>` で、`ActorRef` の id と世代で引く。今の線形の検索（`cells.iter().position`）の
  ほとんどが無くなる（S5〜S7 で効く）。`ActorRef` からメールボックスへは、`Endpoint` が直接指す
- 「実行中のセルは表に無い」という今の意味は保つ。セルを取り出す口は `Mailbox::try_take_cell`（`Option`）で、実行中の
  アクター（セルが実行か外側の枠にある）は `None` になる。今の `cells.iter().position` が `None` を返す場合と同じである。
  エントリは、止まるまで表に残る（`is_live` が `running` を見る必要が無くなる）
- 子の一覧は、表の順（id の順＝生成の順）で引く。今の `Vec` は配送のたびにセルを末尾へ戻すので、順が配送で入れ替わる。
  この順に頼るテストが落ちたら、11.11.1 の「アクターの間の順」と同じく扱う
- まとめないもの：二者の間の関係の表（`watchers`、`watch_messages`、`pending_terminations`、`function_refs`、`function_parents`、
  `function_watching`、`user_children`）。片方が function ref や、ほかのシステムのハンドルで、エントリを持たないことがある。
  原典はこれをセルの DeathWatch（`watching`、`watchedBy`）に持つので、段 4c で DeathWatch を system message にするときに
  セルへ移す
- `ActorEntry` を `ActorCell` の中に置く形（原典の `ActorCell` に近い）は、段 3 では採らない。`stash` などは、そのアクターの
  `receive` の最中（セルが実行の手にあるとき）に実行器が読み書きするので、段 3 では実行器の側に置くしかない。段 4 で
  文脈が自分のセルを借りる形（§6 の段 4）になったら、セルへ移す

#### 11.11.4 型の公開と、メールボックスの戻りの辺（持ち越しの 4、条件 2）

- `Mailbox`、`MailboxCell`、`MailboxRun`、`RunOutcome`、`SystemMessage` を `pub(crate)` にする（段 2 で `pub` にしたのは、
  使う所が無いための dead_code を避けるためだった。§11.10）。`MessageQueue`、`MessageQueueFactory`、4 種類のキュー、
  `RunLimits`、`Throughput`、`ThroughputDeadline`、`MessagePriority`、`ClassifiedEnvelope` は、adaptor が実装・選ぶ port なので
  `pub` のまま
- adaptor へ渡す型引数の無い公開の `MailboxRun` は、段 3 では作らない。段 3 の `run` は `&mut Executor` を文脈に取るので、
  adaptor が呼べる `run` がまだ無いからである。`TaskExecutor` の port と一緒に段 5 で作る（中身は `Arc<Mailbox<ActorCell>>`。
  その時点の文脈は段 4 で決まる）。持ち越しの 4 の「非ジェネリック化」は、この理由で段 5 へ送る
- メールボックスから外への辺は、`Weak<ManualExecutor<C>>` の 1 本だけにする（条件 2。`Mailbox<C>` が持つので型引数は `C` で、段 3 で使うのは `C = ActorCell` だけ。11.11.1）。dispatcher へ `Run` を積むのにも、
  届けられなかった封筒を回すのにも、これを使う。`upgrade` に失敗したら（システムが無い）、積まずに捨てる。システムや
  dead letters への辺は作らない
- 段 2 の `Mailbox::new` と `Mailbox::sharing` は、戻りの辺の無いメールボックスを作る形で残す（段 2 の単体テストが使う）。
  アクターのメールボックスは、戻りの辺を渡す別の口で作る
- `ask` の driver の鍵（今は宛先の境界のアドレス）は、`Mailbox` の宛先では `ManualExecutor` が持つシステムの鍵（今と同じ
  境界のアドレス）を、`upgrade` して読む。同じシステムかどうかの判定（今の `shares_boundary_with`）は、`Weak::as_ptr` を
  比べる（原子操作を使わない）
- `MessageQueue` に `enqueue_first`（既定は「できない」を返す）を足す。`unstash_all` が、今の `ShardQueue::prepend_locally` の
  代わりに使う（Pekko の `DequeBasedMessageQueue.enqueueFirst`）。kernel の `UnboundedQueue` が実装する

#### 11.11.5 停止と解放（持ち越しの 1、条件 3 と 4）

`Arc<Mailbox>` → セル → `self_ref`（`ActorRef`）→ `Arc<Mailbox>` の強い循環が、どのアクターにもできる。ほかに、
キューの中の封筒の送り手、振る舞いが持つ `ActorRef`、実行器の表と `ManualExecutor` の列も、`Arc<Mailbox>` を持つ。
次で断つ。

| 時点 | すること |
|:---|:---|
| アクターの停止（`stop_with_sender`、今の順を保つ） | セルを `try_take_cell` で取り出して `post_stop` を回す（今の `cells.remove` に当たる）。今の `queued_to_dead_letters` の時点で `Mailbox::close` を呼び、返った通常のメッセージを dead letters へ流す（条件 3。今と同じく `PendingTermination` は `Terminated` にする）。`ManualExecutor` から、そのメールボックスの待っている `Run` を取り除く（下の Quint の `closeOutside`）。エントリを表から外し、セルとエントリは、どのロックも持たない所で捨てる（I16） |
| 実行の終わり（guard） | 閉じていればセルを戻さず、ロックの外で捨てる（段 3 では停止は実行の外なので起きないが、段 4a の §11.6 L3 を前倒しして型で守る） |
| システムの終了（`stop_all`） | すべてのメールボックスを閉じ、セルを取り出して捨てる。`ManualExecutor` の列を空にする。境界の扱いは今と同じ。catcher の無い panic のあとの終了（11.11.2 の巻き戻りの 3）も、これを通る |

**セルを取り出せない停止（実行中のアクターへの停止）**：段 3 の停止は、どれも実行の外で行う。実行中（セルが実行の手に
ある間）に頼まれた停止は、今と同じく実行器の `pending_stops` に積まれ（`context.stop`、`stop_and_reply` の後の停止、
decider の失敗の停止も同じ）、`finish_boundary` が行う。`finish_boundary` は実行中の枠が空のときしか動かず、実行は必ず
枠の中にあるので、それが動くのは `run` が返ったあと、セルがメールボックスに戻ってからである（11.11.2 の「`run` が
返ったあとの実行器の仕事」の 4）。自分自身への停止は、`after_run` の `Stop` として同じ時点に行う。実行中の子を止める
`complete_child_stops_of` は、今と同じく実行中の相手を残して `finish_boundary` へ回す。

したがって、`stop_with_sender` が `try_take_cell` で `None` を受けるのは、上の決まりが破れたときだけである。そのときは、
I1 と同じく panic する（停止を捨てると、名前の解放と watcher への `Terminated` が失われ、待つと 1 スレッドでは終わらない
ため）。Quint の `closeOutside` が、作業者が全員 `Free` のときだけ閉じるのは、この決まりを表している（負の対照
`CloseDuringRun`）。段 4a で停止が `Terminate` の system message になったら、停止はそのアクターの実行の中で起き、
この決まりは要らなくなる。

`ActorRef` を外で持ち続ける利用者がいれば、メールボックスそのもの（閉じて、セルもキューも空の入れ物）は残る。中身は
解放されている。条件 4 のテストは、`cfg(test)` の口でメールボックスの `Weak` を取り、(a) アクターを止めてハンドルを捨て、
`run_until_idle` のあとに `upgrade` が失敗すること、(b) 互いのハンドルを持ち合うアクターと、キューに残ったメッセージが
あるままシステムを終えたあとに、すべての `upgrade` が失敗することを確かめる。

#### 11.11.6 testkit の待ち（`WakeSignal`）

- kernel `dispatch/wake_signal.rs` に port `WakeSignal: Send + Sync` を置く。`notify(&self)` と `wait(&self, max) -> bool`。
  `notify` は印を立てて起こし、`wait` は印が立つか時間が切れるまで待って印を下ろす（`wait` の前の `notify` を失わない）
- `ManualExecutor` は、列が空から空でなくなったときに `notify` する（毎回ではない）。function ref の境界
  （`FunctionBoundary`）は、handler を回したあとに `notify` する（test actor にメッセージが届いたとき。§3.6）
- 合図は、`ActorSystem::new(name, boundary)` に渡された境界から受け取る。`ShardBoundary` に既定つきのメソッド
  `wake_signal(&self) -> Option<Arc<dyn WakeSignal>>`（既定は `None`）を足す。adaptor の `ChannelBoundary` は `CondvarSignal`
  （adaptor の `WakeSignal` の実装）を作って返し、自分の `post` でも鳴らす。これで、境界に来たものと、実行待ちと、
  test actor への到着を、1 つの待ちで受ける。`ActorSystem::new` の引数を増やす形は採らない（今のテストと testkit は
  `ActorSystem::new(name, ChannelBoundary)` で作り、書き換えずに合図が付く必要がある）
- `await_envelope(max)`：境界か列に仕事があれば、すぐ `true`。合図があれば、期限（呼んだ時点に `max` を足した時刻）
  まで、`wait(残りの時間)` と見直しを繰り返す。見直して仕事があれば `true`、期限が来たら `false`。合図が別の仕事の
  もの（たとえば、もう片づいた仕事）で、見直しても仕事が無ければ、残りの時間で待ち直す（その時点では `false` を返さない。
  testkit の `expect_msg` が、期限より早く「来なかった」と判定しないため）。合図の無い境界（テストが自作した境界）では、
  今と同じく `poll_within(max)` で境界を待つ。testkit の `await_one` は呼び方を変えない
- 測り方：別のスレッドが 50ms 後にアクターへ送り、`await_envelope(5s)` がすぐ返ることを確かめる単体テスト（合図に落ちて
  いなければ 5 秒待つ）と、testkit の `expect_msg` の待ちの時間の分布（送ってから受け取るまで）を測って報告する

#### 11.11.7 不変条件と Quint

所有権モデルの改訂の条件（§3.4）：

| 条件 | 段 3 の形 | 確かめ方 |
|:---|:---|:---|
| 1 | 下の「条件 1 の数」。届く経路では満たし、断られる経路（閉じている、bounded が満杯）では満たさない | 下の「条件 1 の数え方」 |
| 2 | 11.11.4 の `Weak<ManualExecutor<C>>` の 1 本だけ | 型で見る |
| 3 | 11.11.5 の停止で、`close` が返したものを dead letters へ流す | 既存の stash と dead letters のテストと、11.11.5 の (b) |
| 4 | 11.11.5 | 11.11.5 の (a) と (b) |

**条件 1 の数**：決定文書の改訂の条件 1 は、2 つのことを求める。(1-1) メールボックスの Arc は 1 件さばくごとには複製
せず、複製は予約（idle から scheduled）のたびに 1 つだけ。送り手のハンドルを Envelope に入れるときの複製は、今の
`ActorRef` の複製と同じ数。(1-2) 1 件あたりの原子操作の数は、今（宛先と送り手のハンドルで Arc を 4 つ複製）を超えない。
§3.4 の確かめ方の欄は、段 3 を「送り手のハンドル（path と mailbox）の 2 つと予約の 1 つで、計 3 つ」と数えた（元帥が
決めた文言）。この数え方は変えない。前の版は、戻りの辺の `Weak::upgrade` を足して「最悪 4」と書き直したが、それは
承認済みの数え方を段の設計が差し替えることになるので、取り下げる。ここでは、Arc の複製（1-1 と §3.4 の「計 3 つ」が
数えるもの）と、原子操作（1-2 が数えるもの。複製に `upgrade` の強い参照の数の増加を足したもの）を分けて書く。
数は 1 件あたりの増加だけを数える（捨てるときの減少は、今の 4 つにも同じ数だけあるので、比べる数に入れない）。

| 経路 | 送り手のハンドル | 宛先のハンドル | 予約の `MailboxRun` | 戻りの辺の `upgrade` | Arc の複製 | 原子操作 |
|:---|:---|:---|:---|:---|:---|:---|
| 今（main）、どの経路も | 2 | 2 | — | — | 4 | 4 |
| 届く。送り手は予約に勝たない（宛先が実行中か予約済み。S1 のまとめ送り、S2・S3 の自分への送信） | 2 | 0 | 0 | 0 | 2 | 2 |
| 届く。送り手が予約に勝つ（宛先が idle。S4 の往復、testkit の「外から 1 件送って `run_one`」） | 2 | 0 | 1 | 1 | 3 | 4 |
| 実行の終わりの予約し直し（`RunOutcome::Again`） | — | — | 0（`MailboxRun` を動かす） | 0（回す側が `ManualExecutor` を持つ） | 0 | 0 |
| 断られる（閉じている、bounded が満杯） | 2 | 2（実行器へ回す `Envelope` に宛先が要る） | 0 | 1 | 4 | 5 |

回し方による違い：数を決めるのは、送り手が予約に勝つかどうかで、`run_until_idle` か `run_one` かではない。どちらの
回し方でも、実行の終わりの予約し直しは `MailboxRun` を動かすので、複製は起きない。`run_one`（throughput 1）で自分へ
送り続けるアクター（S3）は 1 件 2 つ、testkit のように外から 1 件送るたびに `run_one` で回す形は、宛先が毎回 idle なので
1 件 4 つ（原子操作）になる。§3.4 の 1 項（メールボックスの Arc は 1 件さばくごとには複製しない）は、予約に勝つ
送り手の複製が「予約のたびに 1 つ」に当たるので、この形でも満たす。

判定：

- 届く経路は、条件 1 を満たす。Arc の複製は最悪 3 で、§3.4 の「計 3 つ」と同じ。原子操作は最悪 4 で、今の 4 を超えない
- 断られる経路は、条件 1 を満たさない。1 件あたり、Arc の複製は 4（宛先のハンドルの複製で、メールボックスの Arc を
  1 件ごとに 1 つ複製する。1-1 に反する）、原子操作は 5（今の 4 を 1 超える。1-2 に反する）。理由は、届けられなかった
  封筒を dead letters へ流す時点を今と同じにするため（11.11.1）に、宛先を付けて実行器へ回すことと、実行器へは戻りの
  辺（条件 2 の `Weak`）しか無いので `upgrade` が要ることである
- 決定文書のとおり、この数と理由は、中将から元帥経由でオーナーへ報告する。段 3 で採らない直し方は次の 2 つで、報告に
  添える
  - path をメールボックスへ移す（`ActorRef` を id、世代、endpoint にする）：届く経路は 1〜3、断られる経路は 3 になる。
    `Mailbox<C>` に path を持たせると、段 2 の型と単体テストの形が広く変わる
  - 断られたものを、送り手のスレッドで dead letters へ流す：Arc の複製は 4（送り手 2＋DeadLetter の宛先 2）、原子操作は 4、`upgrade` は 0。1-2 は満たし、1-1 は宛先の endpoint の複製で満たさない（元帥へ記録）。前の版の「2 になる」は、原典の `DeadLetter(message, sender, receiver)` が宛先のハンドルを持つことを数えていなかった（#286、中将の判断 2026-11-27）。原典の
    `DeadLetterMailbox` も送り手のスレッドで流すので原典には近いが、dead letters の購読者が `run_until_idle` の前に通知を
    受け取り、今の時点が変わる。今の時点に頼るテストを洗い出してからでないと採れない

  上の 4／4・`upgrade` 0 は、数える範囲を限った数である（プルリクエスト #295 のレビューで明記した）。数えるのは、
  数え上げを置いた場所（`ActorRef` と `Endpoint` の複製、予約、`Mailbox::executor` の `upgrade`）で、`tell` から
  送り手のスレッドで dead letters へ流し終えるまでの区間であり、数え上げのテストが購読者の無い形で確かめた数で
  ある。流したあとの EventStream への発行は数に入らない。発行には、購読表の弱参照の `upgrade`
  （`EventStreamBusWeak::publish`）と、購読者 1 件ごとのハンドルの複製があり、抑制されたメッセージを
  `SuppressedDeadLetter` に包むときは dead letters のハンドルの複製も加わる。これらは、実行器が流していた形でも
  発行の側にあったものであり、断られる経路全体の上限ではなく、発行 1 回あたりの数として別に読む。

**条件 1 の数え方**：kernel の `sync::Arc` は標準の `Arc`（CAS の無い target では `portable_atomic_util::Arc`）の別名で、
複製の回数を数える仕掛けが無い。`strong_count` を見るだけでは、見る前に捨てられた複製を数えられない。そこで、複製が
起きる場所を限り、そこに `cfg(test)` の数え上げを置く。

- `Arc<Mailbox<ActorCell>>` は、`Endpoint`、`MailboxRun`、`ActorEntry` の外へ見せない（フィールドを private にする）。
  複製を書いてよいのは、`Endpoint` の `Clone`（手で書く）と、`Mailbox::try_schedule` の予約の 1 か所だけにする。生成の
  ときの複製（表と `ActorRef` へ）は 1 件ごとではないので、送受信の区間の外で起きる
- 数え上げは、スレッドごとの数（`cfg(test)` のビルドは std を使うので `thread_local!`。並行のテストが互いの数を汚さない）
  で、4 つを別々に数える。(a) `ActorRef` の `Clone` での path の複製、(b) `Endpoint` の `Clone`（`Mailbox` と `Boundary` を
  分ける）、(c) 予約の `MailboxRun` の複製、(d) 戻りの辺の `upgrade`（`Mailbox` の 1 つの関数に集める）。ロックの取得
  （`LockCell::with`）の回数も数える（条件 1 は Arc を数えるが、原子操作の数としてはロックも効くため。報告だけで、判定には
  使わない）
- 補助に、送り終えて idle になったあとの `strong_count` が送る前と同じことを確かめる（漏れた複製を見つけるため）
- 数え上げの `cfg(test)` の 8 か所（本番のファイルの 1 行の呼び出し 6 つと、`kernel/sync.rs` の `counts` の宣言）は、syn-check の tests-location と module-wiring に当たる。オーナーの許可（2026-11-27、元帥の項目 472）により、その 8 か所だけに抑制コメント（`// syn-check: allow(<rule>) -- 所有権モデルの条件1の数え上げ（設計 §11.11.7、オーナーの許可 2026-11-27、元帥の項目472）`）を付けた。条件 1 の確かめが要らなくなったら消す（#286 のあと、段 5 の前に見直す）。ほかの場所に抑制を足すには、改めてオーナーの許可が要る
- テストは、上の表の行ごとに、N 件の送受信の区間の数を N で割って確かめる：S1 の形（外から N 件まとめて送って
  `run_until_idle`）、S3 の形（自分へ送るアクターを `run_one` で回す）、testkit の形（外から 1 件送るたびに `run_one`）、
  S4 の形（2 つのアクターの往復）、断られる形（止めたアクターへ N 件）。期待する数は上の表で、表と違えばテストが落ちる。
  断られる形が 5 であることも、そのまま確かめて報告する

I1 は、段 3 では 1 スレッドと大きなロックで守られ、セルを取り出せなければ panic する（今と同じ）。I16 は、セルを
取り出してから `receive` を回し、捨てるのはロックの外、で保つ。I9 は、下の P5 で確かめる。

`mailbox_scheduling.qnt` に、段 3 の遷移を足した（段 4a まで。停止が Terminate の system message になったら
`closeOutside` を消す）。

| 動作 | 実装で当たるところ | 守る不変条件 |
|:---|:---|:---|
| `closeOutside` | 実行器の同期の停止と、システムの終了（`stop_all`。catcher の無い panic のあとの終了も）。どの実行もメールボックスを持っていないとき（大きなロックと、実行中のアクターの停止を後に回すこと。11.11.5）だけ閉じ、待っている `Run` を `ManualExecutor` から取り除く。外からの操作の 1 つとして、`control` と同じ枝から選ぶ（`outside`。下の 11.11.10） | `noInvokeAfterClose`、`submittedWithWorkOrStaleRead`（I13 は変えない） |
| `yieldRun` | 11.11.2 の `Yield` | 全部（`noStrandedMessage` は、終わりの再予約で守る） |
| `failInvoke`（前からある）、`failSystem` | 11.11.2 の「失敗の扱い」。catcher が捕まえた失敗で、suspend して親の判断を待つ。system message の失敗は効果を残し、残りの列を処理し続ける | `settledMeansIdle`（親の判断が済むまでは settled でない）、`systemMessagesConserved` |
| `panicInvoke`、`panicSystem` | 11.11.2 の巻き戻り（catcher が無い、原典の fatal）。guard がセルを戻し、印を立て直し、bit を下ろして予約し直す。そのメッセージは失われ（`lostToFatal`、`sysLostToFatal`）、システムは終了を負う（`fatal`） | `scheduledMatchesTheRun`、`noHiddenSystemMessage`、`noStrandedMessage`、`messagesConserved`、`systemMessagesConserved`、`lossOnlyWhenFatal` |

I14（`settledMeansIdle`）は、例外なしに戻した。前の版は、catcher の無い panic で失われた system message があるときも
suspend を許していた（幽霊の変数 `panickedSystemMessages`）。この例外は、種類を問わず、減らない数で、そのメールボックスの
残りの生涯の suspend を免除していた。Resume を失って永久に suspend のまま、Terminate を失って止まらない、が I14 と I12 を
通ってしまう（再レビューの高 1）。直した形は、原典の 2 つの場合に分ける。

- catcher があるとき（NonFatal）：失敗は `failInvoke` と `failSystem` で、どちらも suspend して親の判断を待つ（`pendingDecision`）。
  判断が済むまでは settled でないので、例外は要らない。system message の効果は失われない
- catcher が無いとき（fatal）：原典はシステムを終えるだけで、回復を約束しない。仕様は、失われたメッセージを処理済みに
  数えず（`lostToFatal`、`sysLostToFatal`。I8 と I12 は、この数を足して保存を見る）、`fatal` を立てる。`settled` は
  「fatal のあとの終了が済んでいる（閉じている）」ことを含むので、fatal のあと閉じるまでは、I14 は何も主張しない。
  原典が約束する「システムが終わる」は、`closeOutside` がメールボックスを閉じることで表す。`lossOnlyWhenFatal` は、
  fatal のほかにメッセージを失う遷移が無いことを見る

仕様には実行器の呼び出しの境界が無い。そのため、`panicInvoke` か `panicSystem` のあとで予約し直されたメールボックスを、
別の作業者が `closeOutside` より先に取って、残りを処理する trace が許される。実装では、次の呼び出しが何よりも先に
システムを終えるので、このメールボックスはもう回らない（11.11.2 の巻き戻りの 3）。不変条件は保たれる（settled は閉じる
まで成り立たず、喪失は数えられている）ので、仕様の健全さの問題ではなく、文章と仕様の抽象のずれである。段 5 で仕様に
実行器の側の状態を足すときに、この境界も表せるかを検討する。

前の版で、例外なしの I14 に `quint run` が見つけた筋書き（Resume の system message が panic で失われ、suspend が残る）は、
直した形でも起きる。ただし、それは fatal のあとで、システムが終わる前の状態であり、settled ではない
（`fatalPanicOwesTheTerminationTest`）。

負の対照を 6 つ足した（`initWith(<誤り>)` から始める `run` が、毎回の `quint test` で確かめる）。

| モード | 誤り | 破れる不変条件 | 確かめる `run` |
|:---|:---|:---|:---|
| `CloseDuringRun` | 実行がセルを持っている間に、外から閉じる | `noInvokeAfterClose` | `closeDuringRunInvokesAClosedActorTest`（正しい形は `closeOutsideWaitsForTheRunTest`） |
| `KeepQueuedRun` | 外から閉じるときに、待っている `Run` を列に残す | `submittedWithWorkOrStaleRead` | `keepQueuedRunSubmitsAClosedMailboxTest`（正しい形は `closeOutsideTakesTheWaitingRunTest`） |
| `PanicKeepsScheduled` | 巻き戻りで、セルは戻すが bit を下ろさない（段 2 の guard） | `scheduledMatchesTheRun` | `panicKeepsTheBitAndStrandsTheMailboxTest`（正しい形は `panicInReceiveReschedulesTheRestTest`） |
| `PanicDropsSysFlag` | system message の途中の巻き戻りで、印を立て直さない | `noHiddenSystemMessage` | `panicDropsTheFlagAndHidesASystemMessageTest`（正しい形は `panicInTheDrainRaisesTheFlagAgainTest`） |
| `PanicAsRecoverable` | catcher の無い panic のあとも、システムが何も無かったように続く（前の版の設計。終了を負わない） | `settledMeansIdle`、`lossOnlyWhenFatal` | `panicAsRecoverableLosesAResumeTest`（正しい形は `fatalPanicOwesTheTerminationTest`） |
| `SilentSystemFailure` | catcher が捕まえた system message の失敗で、その 1 件を効果なしに捨て、suspend も親への通知もしない | `settledMeansIdle` | `silentSystemFailureLeavesTheActorSuspendedTest`（正しい形は `failedSystemMessageAsksTheParentTest`） |

正しい形の `run` は、段 3 で `yieldedRunReschedulesTheRestTest`、`fatalPanicOwesTheTerminationTest`、
`failedSystemMessageAsksTheParentTest` を含めて 7 本足し、`run` は 38 本になった。検査の結果は 11.11.10 にある。

#### 11.11.8 テスト

| 種類 | 中身 |
|:---|:---|
| #251 の一覧の段 3 の 2 行 | `stopping_a_parent_stops_its_child_first_and_tells_the_child_watcher` と `stopping_a_function_ref_stops_its_child_first_and_tells_both_watchers`。今は「子が先に止まった」を、2 人の watcher に `Terminated` が届く順で確かめている。2 人は別の受け手なので、原典はこの順を約束しない。代わりに、子の `post_stop` で印を記録し、その印が、親（または function ref）の watcher に `Terminated` が届くより前にあることを確かめる。親の停止は子の ChildTerminated を待ってから進み、親の `Terminated` はそのあとで送られる（T1〜T4）ので、この順は原典が約束する因果の順である。両方の watcher に 1 回ずつ届くことと `existence_confirmed` は今のまま確かめる（弱めない）。実装より前の、テストだけのコミットにする |
| 段 2 の単体テスト | `MailboxCell` の `Env` と `Next`、`run` の引数の変更に合わせて、形だけ書き換える（`type Env = ()`、`Continue` を返す、`run(&mut (), None)`）。確かめる性質と assert は変えない。実装と別のコミットにする。巻き戻りのテスト（`mailbox_run_test.rs` のセルが戻ること）には、bit が下りて再予約されることと、印の立て直しの assert を足す |
| 新しい単体テスト | 条件 1 の数え上げ（11.11.7 の表の行ごと。断られる形の 5 も確かめて報告する）、条件 4 の (a)(b)、`await_envelope` が合図で返ることと、別の仕事の合図では期限まで待ち直すこと、`run_one` が system message だけの実行で `false` を返さないこと、閉じたメールボックスの `Run` がセルを取らないこと、停止で待っている `Run` が消えること、巻き戻りの bit と印 |
| `Yield` と `after_run` の時点（11.11.2 の表の行ごと） | (1) 失敗：失敗した次のメッセージを、再起動の前に処理しない。失敗のあとメールボックスが suspend され、再起動のあとの Resume で残りが処理される。(2) `Kill`：同じ実行で後ろのメッセージを処理しない。既定の decider では後ろのメッセージが dead letters へ行き、`receive` は呼ばれない。(3) `PoisonPill`：後ろのメッセージが dead letters へ行く。(4) 失敗と `stop_and_reply` の重なり：停止が勝ち、再起動しない。`post_stop` の `sender()` が失敗したメッセージの送り手である。(5) 境界の仕事：throughput 5 で宛先に 5 件あり、1 件目がほかのアクターを `context.stop` で止めると、2 件目を処理するときには相手が止まっている（`is_actor_terminated`）。(6) 何も残らないメッセージでは、throughput の件数まで 1 回の実行で続ける（`Yield` しない） |
| catcher の無い panic（11.11.2 の巻き戻り） | (1) `receive` の panic が `run_until_idle` の呼び出し元へ出る。(2) 出たあと、次の `run_until_idle` は何も配らず、システムが終わっている（`is_terminated`）。(3) 終えたあと、条件 4 の `upgrade` が失敗する。(4) 生成の `pre_start` の panic（実行の外の呼び出し）でも同じになる。(5) 巻き戻りの最中に利用者の型の `Drop` が走らない（`Drop` で数える振る舞い） |
| セルを取り出せない停止（11.11.5） | `receive` の中から自分と、ほかのアクターを止めると、どちらも `run` が返ったあとに止まる。`stop_with_sender` が実行中のアクターに届く経路が無いことは、`try_take_cell` が `None` なら panic する口で守り、その panic を起こすテストは置かない（起こせる経路が無いため） |
| PBT（P5） | 1〜6 のアクターと、外からの送信、`receive` の中からの送信（転送）、`run_one` と `run_until_idle` の混ぜ方を、長さ 0〜8 の列は網羅し、長いものは seed を記録した乱数で作る（段 2 の P1 と同じ作り。依存は足さない）。性質：送り手と宛先の組ごとに、届く順が送った順と同じ（I9）。送ったものは、どれも 1 回だけ届くか dead letters へ行く（I8）。除外なし |
| 既存のテスト | すべて通す。順の違いで一覧に無いものが落ちたら、11.11.1 のとおり上げる |

#### 11.11.9 壊しうる入力と影響

| 区分 | 入力 | 期待 |
|:---|:---|:---|
| 並行 | 別のスレッドからの `tell`（手で回すアクターへ）と、テストのスレッドの待ち | メールボックスに積まれ、合図で待ちが起きる。`ManualExecutor` の列はロックで守る |
| 生成 | `pre_start` の中で自分や子へ送る | メールボックスに積まれ、予約される。実行は外側の配送が終わってから（今と同じく、セルは生成が終わるまでメールボックスに入れない。入れる前の `Run` は回らない：1 スレッドで、`run_one` の外にいるため） |
| 停止 | 止まったアクターへの送信、止まる途中（`post_stop` の中）の送信、待っている `Run` があるときの停止 | 断られたものは実行器へ回って dead letters。止まる途中のものは `close` で dead letters。`Run` は列から消える |
| 再起動 | キューに残ったメッセージがあるときの再起動 | 同じメールボックスとハンドルで、新しいインスタンスが残りを処理する（今と同じ） |
| stash | `unstash_all` で先頭へ戻す、bounded の容量の計算 | `enqueue_first` で先頭へ。容量の計算は `number_of_messages`（今の `count_addressed_to`） |
| 終了 | 実行待ちがあるまま終える、終えたあとにハンドルへ送る | 列は空になる。閉じたメールボックスに断られ、システムが無ければ捨てる（今の、閉じた境界が捨てるのと同じ） |
| panic | catcher の無い `receive` の panic（原典の fatal） | 11.11.2 の巻き戻り。メールボックスは予約し直され、panic は呼び出し元へ出て、次の呼び出しでシステムが終わる |
| 失敗 | catcher がある `receive` の失敗、`Kill`、毎回失敗するアクター | 11.11.2 の「失敗の扱い」。suspend して `Yield`、`run` が返ったあとに親の判断、開いていれば Resume。1 回の実行で失敗するのは 1 件だけ |
| 規模 | 1 万のアクター（S7） | 線形の検索が無くなる |
| ほかのシステム | ほかのシステムのアクターへの `tell` | そのシステムのメールボックスと `ManualExecutor` に直接積む（今は境界を通る） |

公開 API への影響：`Mailbox` などの `pub(crate)` 化（11.11.4。外の利用者はいない）。`ShardBoundary` と `MessageQueue` に
既定つきのメソッドを 1 つずつ足す（実装している側は書き換えなしで通る）。`WakeSignal` を公開の port にする。
`ActorRef` と `Envelope` は中のフィールドだけが変わる。`run_one` の rustdoc を書き直す。catcher の無いシステムで
panic が外へ出たあとは、次の呼び出しでシステムが終わる（11.11.2。今は使い続けられる。そう使うテストは無い）。
上流と保存済みのデータへの影響は無い。

速さ：§7.5 の S1〜S7 を測る。段 1 の計測（`tests/actor_system_send_stage1_bench.rs` と `scripts/stage1-bench.sh`）に
S5〜S7 を足し、§7.5 の交互の手順で、A＝段に入る直前の main、A0＝`7ecb3f5` と比べて報告する。止める基準ではない。

#### 11.11.10 動かしたコマンドと結果（設計の時点、2026-11-26）

直す前の版（`0be3b17`）の記録である。直したあとの記録は、この表の下にある。

| 検査 | 結果 |
|:---|:---|
| `scripts/quint-check-specs.sh`（CI と同じ。typecheck、`run` 34 本、`quint run` 20 手・1 万本） | すべて通った |
| `quint run --invariant=invariant`（正しい形、rust の backend、25 手・5 万本、3 回） | 3 回とも違反なし（trace の長さの最大は 26） |
| `quint run --backend=typescript --invariant=invariant --max-steps=60 --max-samples=20000` | 違反なし（最大 61 手。seed `0x1d835d64a7e5db`）。10 万本では、typescript の backend が trace の統計を出すところで落ちた（`getTraceStatistics` の `Math.max`）。数が多すぎることによる道具の側の落ち方で、仕様の違反ではない |
| 足した 4 つの誤りのモードから始める `quint run`（20 手・1 万本） | 4 つとも違反を見つけた |
| 前からある 7 つの誤りのモード（同じ条件） | `CheckThenAct`、`ExternalSuspend`、`LateEnqueue`、`DropSystemOnClose`、`MessagesBeforeSystem`、`ClearFlagAfterDrain` は見つけた。`LostWakeup` と `PartialDrain` は、20 手・1 万本では見つからなくなり、30 手・5 万本では見つけた（main の仕様では、20 手・1 万本で `LostWakeup` は 3 回中 2 回、`PartialDrain` は 3 回とも見つけた）。足した遷移、とくに `closeOutside` が早い手で閉じて trace を終わらせるため、探索が薄まった。CI の乱数の探索は正しい形だけを見るので、CI の強さは変わらない。誤りのモードは、毎回の `quint test` の `run` が決まった筋書きで確かめる |
| 最初の版（例外なしの I14） | `quint run` が違反を見つけた（Resume の system message が panic で失われ、suspend が残る）。このときは I14 に例外を足して直したが、その直し方はレビューの高 1 で取り下げた（11.11.7、11.11.11） |

設計の独立レビュー（11.11.11）を受けて直したあと（2026-11-27、quint 0.32.0）：

| 検査 | 結果 |
|:---|:---|
| `scripts/quint-check-specs.sh`（CI と同じ。typecheck、`run` 38 本、`quint run` 20 手・1 万本） | exit 0。`mailbox_scheduling.qnt` は 38 passing、`quint run` は違反なし |
| `quint run --invariant=invariant`（正しい形、rust の backend、20 手・1 万本、3 回） | 3 回とも違反なし。trace の長さの平均は 13.2〜13.4（main `7eb9d30` の仕様は 20.8、直す前の版は 9.8） |
| `quint run --invariant=invariant`（正しい形、rust の backend、25 手・5 万本、3 回） | 3 回とも違反なし（trace の長さの最大は 26、平均は 14.7〜15.0） |
| `quint run --backend=typescript --invariant=invariant --max-steps=60 --max-samples=20000` | 違反なし（trace の長さの最大は 61、平均は 20.5。seed `0x5d643e5349a4f`） |
| 14 の誤りのモードから始める `quint run`（20 手・1 万本、モードごとに 3 回） | 12 のモードは 3 回とも違反を見つけた（`PartialDrain` を含む）。`LostWakeup` と `SilentSystemFailure` は、20 手・1 万本では種に左右され、見つからない回がある（設計の再レビューの実測で、`LostWakeup` は 3 回中 2 回、`SilentSystemFailure` は最初の 3 回で 0 回、追加の 5 回で 2 回）。30 手・5 万本では、どちらも 3 回とも見つけた。CI が毎回確かめるのは、決まった筋書きの `quint test` の `run`（`silentSystemFailureLeavesTheActorSuspendedTest` など）で、乱数の探索ではない（§5.3） |

直す前の版で探索が薄まったのは、`closeOutside` が `step` の独立の枝で、開いていて実行の無い状態では毎手有効だった
ためである。多くの trace が早い手で閉じて終わり、平均の長さが main の半分（9.8）になった。停止を外からの操作の 1 つに
まとめ（`outside`。`control` の 3 種類の system message と同じ枝から 4 分の 1 で選ぶ）、平均を 13.3 に戻した。main の
20.8 に届かないのは、閉じたあとに送り手と操作の予算が尽きると trace が終わるためで、閉じる経路が段 3 で増えた分だけ
短い。正しい形の探索の深さは、そのぶん浅い状態に寄っている。25 手・5 万本と 60 手の typescript の探索で、深い状態も
確かめた。

#### 11.11.11 設計の独立レビューの指摘の処理（2026-11-26 のレビュー、`0be3b17` に対して）

レビューの判定は「要修正」で、全体の形（アクターごとのメールボックス、`ManualExecutor`、`ActorEntry`、`pub(crate)`、
停止時の解放）は妥当とされた。指摘ごとの処理は次のとおりである。

| 指摘 | 判断 | 処理 |
|:---|:---|:---|
| 高 1：I14 の例外が広すぎる。panic で失われた system message の効果を仕様が数えず、Resume や Terminate の喪失が I14 と I12 を通る | 直す | 原典の 2 つの場合に分けた（11.11.2、11.11.7）。catcher があるとき（NonFatal）は、`receive` も system message も 1 件ごとに包み、失敗したら suspend して親の判断を待つ。system message の効果は残す（Quint の `failInvoke`、`failSystem`）。catcher が無いとき（fatal）は、原典の handler と同じくシステムを終える。I14 から例外と `panickedSystemMessages` を除き、「何も進行中でない」に fatal のあとの終了を含めた。失われたものは `lostToFatal`、`sysLostToFatal` に数え、処理済みに数えない（`lossOnlyWhenFatal`）。負の対照 `PanicAsRecoverable`（前の版の設計）と `SilentSystemFailure` を足した。レビューの直し方のうち、(b) を catcher がある場合に採り、catcher が無い場合は (c) に近い形を採った。(c) と違ってメールボックスでなくシステムを終えるのは、原典がシステムの単位で終えるからと、閉じるときに dead letters へ流す実行器を、巻き戻りの最中には安全に呼べないからである。(a)（幽霊の変数を Resume の数に絞る）は、喪失を許したまま数を限るだけで、原典が NonFatal の失敗で効果を失わないことに届かないので採らない |
| 高 2：guard の予約し直しが原典と違う。実行中の枠が残って `finish_boundary` が止まる、panic の繰り返しで `run_until_idle` が返らない | 直す | (i) 実行器を借りる口を closure の形にまとめ、巻き戻ったら実行中の枠と `boundary_active` を空に戻し、次の呼び出しでシステムを終える（11.11.2 の巻き戻りの 2、3）。予約し直したメールボックスは、もう回らない。(ii) catcher がある失敗は suspend し、親の判断のあとに Resume を送る（11.11.2 の「失敗の扱い」）。(iii) catcher がある場合、1 回の実行で失敗するのは 1 件だけで、そのあとは親の判断と `RestartLimit` が終わらせる。catcher が無い場合、panic は 1 回で呼び出し元へ出て、システムが終わる。「今と同じく保証しない」の文は消した。前の版が「使い続けない」案を退けた理由（段 5 のプール）も、原典に無い保証なので取り下げた |
| 高 3：`AfterRun` が未定義。失敗と遅らせた停止の優先、Kill と PoisonPill の時点、`finish_boundary` を `Continue` のメッセージで先送りする順序 | 直す | `AfterRun` の型（`Failed`、`Kill`、`Stop`）、運ぶ情報（失敗の原因、元の送り手）、優先の表（今の `deliver` の分岐をそのまま移した）、`Yield` を返す条件（境界の仕事の 3 つの表が空でないこと。今の `finish_boundary` が何もしない場合とちょうど同じ）、`run` が返ったあとの順を 11.11.2 に書いた。`after_run` が `Option` 1 つで足りる理由も書いた。11.11.8 に、表の行ごとの時点のテスト（Kill、PoisonPill、重なり、境界の仕事、`Continue` の続行）を足した。Quint には足さない（仕様は 1 つのメールボックスの予定を表し、実行器の表を持たない。`yieldRun` は、待っている system message を処理し切ってから終わる形のまま） |
| 中 1：条件 1 の数え方を断りなく変えた。断られる経路では 5、`run_one` では 4 が常時、数え方が未定 | 直す（一部は採らない） | §3.4 の「計 3 つ」の数え方に戻し、Arc の複製と原子操作を分けた経路ごとの表を書いた（11.11.7）。判定：届く経路は満たす（複製は最悪 3、原子操作は最悪 4）。断られる経路は満たさない（複製 4、原子操作 5）。数と理由と、段 3 で採らない直し方 2 つを、中将から元帥経由でオーナーへ報告する。数え方（複製の場所を限り、`thread_local!` で 4 種類を別々に数える。`strong_count` は補助）と、表の行ごとのテストを書いた。メールボックスの宛先の `tell` が `Envelope::new` を通らないことを 11.11.1 に明記した。「`run_one` では 4 が常時」は採らない：実行の終わりの予約し直しは `RunOutcome::Again` で `MailboxRun` を動かすので複製せず、数を決めるのは送り手が予約に勝つかどうかである（自分へ送る S3 は 2、外から 1 件ずつ送る testkit の形は 4） |
| 中 2：負の対照の乱数探索が薄まり、§5.3 の記録が成り立たない | 直す | §5.3 を現行に合わせ、誤りのモードの乱数探索は CI の対象外の手作業であることと、その流し方を書いた。加えて、薄まりの原因（`closeOutside` が独立の枝で毎手有効）を仕様の側で直した。20 手・1 万本では 12 のモードが 3 回とも違反を見つけ、`LostWakeup` と `SilentSystemFailure` は種に左右される（30 手・5 万本では 3 回とも見つける。11.11.10。設計の再レビューの中 1 で、前の版の「14 のモードのすべてが 3 回とも」を実測どおりに直した）。正しい形の trace の長さの変化と、深さが浅い状態に寄っていることを 11.11.10 に書いた |
| 中 3：停止でセルを取り出せない場合（実行中のアクターへの停止） | 直す | 11.11.5 に書いた。実行中に頼まれた停止は `pending_stops` か `after_run` の `Stop` に回り、`run` が返ったあとに行う。`stop_with_sender` が `None` を受けるのは決まりが破れたときだけで、I1 と同じく panic する |
| 低：`ManualExecutor` の型引数の置き場所が 11.11.1 と 11.11.4 で合わない | 直す | `ManualExecutor<C>` と `ManualTask<C>` に揃えた。段 3 で作るのは `ManualExecutor<ActorCell>` だけ |
| 低：`ActorEntry` の `mailbox` の行のコメントの誤り | 直す | 「メールボックス。セルはその中にある」に直した |
| 低：`await_envelope` が、別の仕事の合図のあとに待ち直すか | 直す | 期限まで待ち直す形に決めた（11.11.6）。テストも足した（11.11.8） |

#### 11.11.12 設計の再レビューの指摘の処理（2026-11-27 のレビュー、`7c39c48` に対して）

判定は「承認」で、残りは中 1 件と低 2 件だった。どれも設計の形は変えない。

| 指摘 | 判断 | 処理 |
|:---|:---|:---|
| 中 1：11.11.10 の「14 のモードのすべてが 20 手・1 万本で 3 回とも違反を見つけた」は再現しない | 直す | 11.11.10 と 11.11.11 の中 2 の行を実測どおりに直した。`LostWakeup` と `SilentSystemFailure` は 20 手・1 万本では種に左右され、30 手・5 万本では 3 回とも見つける。CI の安全は決まった筋書きの `quint test` が担う |
| 低 1：fatal のあとの終了が次の呼び出しまで遅れる | 記録する | 11.11.2 の巻き戻りの 3 に、遅れる理由と、段 5 の設計でこの終了を担うスレッドと呼び出しを明記することを書いた |
| 低 2：仕様に呼び出しの境界が無く、fatal のあと予約し直されたメールボックスが閉じる前に取られうる | 記録する | 11.11.7 に、文章と仕様の抽象のずれとして書いた。段 5 で実行器の側の状態を足すときに検討する |

#### 11.11.13 段 3 の実装で決めたこと

実装（[PR #284](https://github.com/j5ik2o/fraktor-ng-rs/pull/284)）で、11.11 に書いていなかった形を、次のとおり決めた。承認済みの不変条件の意味は変えていない。

| 項目 | 決めた形 | 理由 |
|:---|:---|:---|
| 選択（`ActorSelection`）の宛先の解決 | 生きているアクターのパスは、`tell` を呼んだ送り手のスレッドで引き、そのアクターのメールボックスへ直接積む。引くのは、実行器の外の、自分のロックを持つ表（`LiveActors`。パスからハンドル）である。それ以外のパス（function ref、ガーディアン、誰もいないパス）は今と同じく境界を通り、実行器が配るときに解決する。送ったときにアクターがいなかったパスは、あとでそのパスにアクターができても届けず、誰もいないパスとして dead letters へ流す（原典では、送ったときに `EmptyLocalActorRef` へ行き、dead letters になる。PR #284 の Devin の指摘） | 原典の `ActorSelection.tell` は、送り手のスレッドで `deliverSelection` を呼び、見つけたアクターへ `tell` する（`ActorSelection.scala:208-260`）。選択で送ったあとに同じ送り手が直接送ったものは、選択のメッセージを追い越さない。境界を通すと、段 3 では直接の送信がメールボックスへ先に入り、`selection_message_is_not_overtaken_by_a_later_direct_send` が落ちた。11.11.2 の「`ActorSelectionMessage` は境界を通る」は、生きていないパスについてだけ残る |
| 「致命的」とする巻き戻り | 実行器を借りる口（`Executor::lend`）から巻き戻ったときのうち、実行中の枠があるとき、メールボックスの実行（`run` と、その後の仕事）の途中、境界の仕事の途中のときだけ、致命的とする | 実行器が自分で投げる拒否（終了したシステムでの生成、止まった親への子の生成など。原典が呼び出し元へ投げる例外に当たる）は、状態を変える前に起きる。これを致命的とすると、拒否のあとも使い続ける既存のテスト（`a_weak_unnamed_child_of_a_stopped_parent_is_refused`、catcher つき）が落ちる。11.11.2 の巻き戻りの 2 の「印が立たないままロックを外すとき」を、この 3 つの場合に絞った |
| 境界から来た、生きているアクター宛ての封筒 | アクターのメールボックスへ積み、予約に勝てば、同じ手でそのメールボックスを回す | 境界が実行器の外でアクター宛ての封筒を作る形（テストが自作する境界）では、今は同じ手で配っていた。積むだけにすると、次の `poll` までに配られず、境界が同じ封筒を作り続ける（`run_until_idle_delivers_from_the_boundary_while_the_queue_keeps_filling`）。満杯で断られたら dead letters へ流す（段 3 のアクターのキューは上限なし） |
| `PoisonPill` の停止 | `after_run` の `PoisonPill` から、今と同じく `stop_at_boundary` で止める（`stop_and_reply` の停止は今と同じく `stop_with_sender`） | 停止の途中で走る子の終了のフックが同じアクターの停止を頼んでも、境界の仕事の印が、最初の停止が終わるまでそれを待たせる。`stop_with_sender` を直に呼ぶと停止が入れ子になり、終了のコールバックが 2 回走った（PR #284 の Devin の指摘） |
| 「致命的」とする範囲の、境界から来た封筒 | 実行待ちの列の仕事は、`Run` も封筒も、回している間を「実行の途中」とする | 封筒の配送の中で function ref の handler が panic しても、`receive` の panic と同じく致命的にする（11.11.2 は function ref の handler の panic も挙げている。PR #284 の Devin の指摘） |
| `after_run` の置き場所 | `ActorEntry` ではなく、実行器の欄 1 つ（アクターの鍵と `AfterRun`） | 1 回の実行に高々 1 つで、`run` が返った直後に取り出す（11.11.2）。アクターごとの事実ではなく、実行ごとの事実である |
| セルの入れ物 | メールボックスの枠は `Option<Box<C>>` | 値で出し入れすると、実行のたびにセル全体を 2 回写す。S3 の計測で、この写しがいちばん大きな割合だった（240 ns から 174 ns） |
| `SystemMessage` の公開 | `pub` のまま（11.11.4 では `pub(crate)`） | 段 3 で送るのは `Resume` だけで、`pub(crate)` にすると、ほかの種類が使われないという dead_code の警告になる（抑制は人間が決める）。段 4 で送るようになったら `pub(crate)` にする |
| system message を catcher で包むこと（11.11.2） | 段 3 では包まない | 段 3 で送る system message は `Resume` だけで、利用者のコードを走らせない。利用者のコードを走らせる system message（`Create`、`Terminate` など）を段 4 で足すときに包む |
| 段 2 の `Mailbox::new`、`Mailbox::sharing` と、テストだけが読む口 | `cfg(test)` にした | `Mailbox` を `pub(crate)` にしたので、使う所が単体テストだけの関数は dead_code になる。製品の動きは変わらない |
| `CondvarSignal` | 印を原子変数に持ち、待っている者がいるときだけロックを取って起こす | 実行器は、実行待ちの列が空でなくなるたびに鳴らす（11.11.6）。待つ者がいないときに毎回ロックを取ると、S3 と S4 で 1 件ごとに効く |
| function ref への到着の合図 | 実行器が数（`ArrivalNotice`）を持ち、function ref の境界は handler を回したあとに数を足して鳴らす。`await_envelope` は、呼んだときより数が増えていれば仕事が来たとみなす | test actor への到着は、境界にも実行待ちにも残らない。数が無いと、11.11.6 の「別の仕事の合図では待ち直す」と見分けられず、到着のたびに期限まで待つ |
| 巻き戻りのテスト（11.11.8 の段 2 の単体テストの行） | 既存のテストに assert を足さず、`mailbox_unwind_test.rs` に新しいテストとして書いた | 既存のテストは書き換えない。確かめる中身（bit が下りて再予約される、印が立て直される）は同じ |
| 境界に封筒を置く手段に、セルのアクター宛ての `tell` を使っていたテスト 7 件 | red の工程の別コミットで、置く手段だけを、メールボックスを持たないハンドル（`resolve_actor_ref` で得る、誰もいないパスのハンドル。原典の `EmptyLocalActorRef` に当たる）からの `tell` に替えた。assert は変えない（中将の判断、2026-11-27） | 段 3 では、セルのアクター宛ての `tell` は境界を通らない（11.11.1）。3 件は落ち、4 件は境界を一度も通らずに通っていた。`actor_system_scala213_plus_spec.rs` の `has_pending` の確かめは fraktor が足したもので、原典の `Scala's Using automatically terminates ActorSystem` は `whenTerminated.isCompleted` だけを確かめる |

#### 11.11.14 段 3 の実装の測り（2026-11-27）

§7.5 の手順（`scripts/stage1-bench.sh`、A＝段に入る直前の main `b591452`、B＝`37d350c`、A0＝`7ecb3f5`）。02:01 JST から、1 分の load average は 26〜32（機械はほかの bot と共有）。4 組とも対照の検査を通った。止める基準ではない。

| 場面 | B/A | B/A0 | A/A の組の比 | 何が効いているか |
|:---|:---|:---|:---|:---|
| S1 | 0.942 | 0.919 | 1.006, 1.003, 1.124 | 揺れの中 |
| S2 | 0.892 | 0.957 | 0.984, 1.008, 1.010 | 自分への送信が共有の列でなくメールボックスに入り、throughput 5 で回る |
| S3 | 1.341 | 1.409 | 0.995, 1.050, 0.949 | 1 件ごとにカーネルのロックの区間が 9 回（実行待ちの列の出し入れ、セルの出し入れ、キューの出し入れ、門、予約の 2 回の読み）。今の経路は 0 回 |
| S4 | 1.528 | 1.509 | 1.009, 1.013, 1.051 | S3 と同じで、1 件ごとに 11 回。相手が毎回 idle なので、予約に勝って実行待ちの列に積む |
| S5 | 0.268 | 0.325 | 1.001, 1.017, 1.070 | 宛先の線形の検索が無くなった |
| S6 | 0.033 | 0.044 | 1.006, 1.022, 0.990 | 同上 |
| S7 | 0.003 | 0.004 | 1.013, 0.997, 1.032 | 同上（約 53 µs から約 160 ns） |

ロックの回数は、条件 1 の数え上げのテストが数えた値（`LockCell::with`）である。計測の途中で、実行のたびにセル全体を値で 2 回写していたのを箱に入れて直した（S3 で 240 ns から 174 ns。11.11.13）。

testkit の待ち（`crates/fraktor-testkit/tests/wait_latency_bench.rs`、別のスレッドからの送信から `expect_msg` が返るまで、200 回）：test actor へ直接は中央値 10 µs、p99 59 µs、最大 0.15 ms。アクターを経て test actor へは中央値 14 µs、p99 0.22 ms、最大 0.39 ms。`POLL_INTERVAL`（10 ms）の眠りには落ちていない。

### 11.12 段 4a で決めること（[Issue #253](https://github.com/j5ik2o/fraktor-ng-rs/issues/253)）

段 4a の設計の差分である。実装の前に、実装と別の系統の設計の独立レビューを通す（§6 の冒頭）。扱うのは、§6 の段 4a の行
（Create、Terminate、子の名前、T1〜T5、閉じるときの dead letters）、§11.6 の L3、§11.11.13 で段 4 に回した「利用者のコードを
走らせる system message を catcher で包む」、#251 の一覧で 4a に回した回復の重なりの深さ 1 の 2 件、段 3 のレビューから回した
2 点（#253 のコメント）である。

**変えないこと**：失敗と監督の判断（`receive` の失敗、`Kill`、all-for-one、decider の panic）は、段 3 と同じく実行のあとに
実行器が行う（段 4b で `Failed` と `Recreate` の system message にする）。DeathWatch と終了の通知の表、stash、`stop_and_reply`
の返事は、実行器の表のまま（段 4c）。`MailboxCell::Env` は `Executor` のままで、`ActorContext` の `&mut Executor` も残す
（段 4 の終わりで外す）。システムの終了（`stop_all`、T6〜T8）は今のまま（段 6）。dispatcher は手で回す 1 スレッドのものだけ
（複数スレッドは段 5）。

#### 11.12.1 形の全体

| 項目 | 今（段 3、main `74d8872`） | 段 4a |
|:---|:---|:---|
| 生成 | `spawn_at` がセルを入れ、その場で `create_initial` が `pre_start` を回す。開始の失敗の判断とやり直しも、そのループの中で同期に済む | `spawn_at` は名前を予約し（原典 `Children.makeChild` の `reserveChild` と `initChild`）、セルを入れ、`Create` を子の system message の列に積んで予約する（`Dispatch.init`、`start`）。`pre_start` は、子の実行が `Create` を処理するときに走る（`ActorCell.create`） |
| 停止 | `stop_with_sender` が、子を先に同期で止め、`post_stop`、閉じる、通知までをその場で行う。実行中のアクターへの停止は `pending_stops` に積み、`finish_boundary` が行う | どの停止も、対象のメールボックスに `Terminate` を積むだけにする（原典 `ActorCell.stop`、`Children.stop`）。`Terminate` は対象の実行が処理する（11.12.3）。function ref だけは、メールボックスが無いので実行器が止める |
| 子の名前 | 開始に成功した子だけを、開始のあとで記録する。止めると `forget_child` がその場で外す | 生成のときに予約し、親が子の `ChildTerminated` を処理したときに外す（原典 `removeChildAndGetStateChange`）。名前の無い子（`$n`）も親の表に入れる |
| 子の停止の知らせ | `run_child_terminated_hook` が親のフックを同期に回す（親が実行中なら `pending_child_terminated` に積む） | `ChildTerminated` を親のメールボックスに積む。親の実行が、名前を外し、フックを回し、待っていた続きを行う（11.12.4）。メールボックスの無い親（ガーディアン、function ref）の分は、子の実行の中で実行器がその場で行う |
| 再起動が子を待つ | `complete_child_stops_of` が、`pre_restart` で止めた子をその場で止め切る | `pre_restart` で止めた子の `ChildTerminated` を待ち、最後の 1 つを処理する実行で `post_restart` を回す（原典の `Recreation`。11.12.5） |
| `AfterRun` | `Failed`、`Kill`、`Stop`、`PoisonPill` | `Failed`、`Kill`、`StartFailed`（開始の失敗）。`Stop` と `PoisonPill` は、自分への `Terminate` になって消える |
| `Yield` を返すとき | 失敗、`Kill`、`PoisonPill`、`stop_and_reply`、境界の仕事（`pending_stops`、`pending_decider_failures`、`pending_child_terminated`） | 失敗、`Kill`、`pending_decider_failures` が空でないとき。ほかは `Continue` |
| 消すもの | — | `create_initial` のループ、`pending_stops`、`request_stop`、`drain_pending_stops`、`complete_child_stops_of`、`pending_child_terminated`、`flush_ready_child_terminated`、`stop_at_boundary`、`AfterRun::Stop`、`AfterRun::PoisonPill`。`finish_boundary` は `pending_decider_failures` だけを扱う形に縮める |
| `SystemMessage` | `pub`。11 種類 | `pub(crate)`。4a が送る 4 種類（`Create`、`Resume`、`Terminate`、`ChildTerminated`）だけを残す（11.12.9） |
| Quint | `mailbox_scheduling.qnt` | `actor_termination.qnt` を足す（11.12.11）。`mailbox_scheduling.qnt` は注釈だけを直す |

#### 11.12.2 生成（Create）

**生成の手順**（原典 `Children.makeChild`、`Dispatch.init`、`start`）：

1. 今と同じ拒否を、今と同じ順で同期に返す（名前の検査、終了したシステム、停止中の親、名前の重複、設定の検査）。原典の
   `actorOf` も、これらは呼び出し元へ同期に投げる
2. 親の表に名前を予約する（`ParentEntry.children` に名前とハンドルを入れる。名前の無い子は、生成した `$n` の名前で入れる。
   トップレベルなら `user_children` にも）。生きている親（エントリがある）かガーディアンに `ParentEntry` が無ければ、ここで
   作る（戦略は既定。親の `Create` が `register_parent_strategy` で書き換え、子の一覧は残す）。止まった親には作らない（今の
   `spawn_named_child_with_strategy` と同じく、停止中として断る）
3. メールボックスとセル（`ActorCell::pending`）を作り、エントリと `LiveActors` に入れる（今と同じ）
4. `Create` を子の system message の列に積んで予約し（`Mailbox::system_send`）、ハンドルを返す

`actor_of` は、`pre_start` を待たずに返る（原典と同じ。#251 の 13 件は、この違いに頼っていた）。

**`Create` の処理**（子の実行の中。原典 `ActorCell.create`、`FaultHandling.faultCreate`、`finishCreate`）：

| セルの状態 | すること |
|:---|:---|
| 停止中（`Terminate` を処理したあと） | 何もしない。原典は、子を待つ間に届いた `Recreate` を取っておき、閉じるときに dead letters へ流す。生成を始めない点は同じ |
| すでに作られている | 何もしない（やり直しの `Create` が 2 通並び、1 通目で作れたときの 2 通目。下の「やり直しの `Create` が 2 通並ぶとき」） |
| 開始に失敗していて、その失敗への判断がまだ（同じ実行の中で、先の `Create` がもう一度失敗した） | 何もしない。判断 1 回につき、作り直しは 1 回である（下の「やり直しの `Create` が 2 通並ぶとき」） |
| 開始に失敗していて、前の試みの子が残っている | その子のすべてに `Terminate` を送り、「子を待つ（理由：Creation）」にする（`faultCreate`）。最後の子の `ChildTerminated` を処理したときに、下の「作る」を行う |
| ほか | 作る。開始に失敗した状態からなら、まず失敗の suspend を 1 つ解く（`finishCreate` の `resumeNonRecursive`）。そのあと catcher で包んで、`Props` からインスタンスを作り、戦略を登録し、`pre_start` を回す（今の `ActorCell::create`。枠と `ActorContext` の作り方は今の `create_initial` と同じ）。失敗したら、メールボックスを suspend し（`handleInvokeFailure` の `suspendNonRecursive`）、セルに「開始に失敗した」を記録し、`after_run` が空なら `StartFailed` を記録する |

**開始の失敗の判断**（実行のあとに実行器が行う。段 4b で親の `Failed` にする）：今の `create_initial` の分岐を、ループにせずに
移す。失敗を publish し（`ActorInitializationException`）、戦略を当てる。判断を当てたら、セルの「開始に失敗した」の記録に
「判断が済んだ」を付ける（上の表と 11.12.5 が読む。原典で `Failed` が親に届いて判断されたことに当たる）。親が suspend されて
いる間は、判断を取っておく（11.12.6 の「親が suspend されている間の子の失敗」）。

| 判断 | すること |
|:---|:---|
| Restart（許された） | 子に `Create` を積む。`Resume` は送らない（suspend を解くのは、その `Create`。上の表） |
| Restart（回数を超えた）、Stop | 子に `Terminate` を積む。suspend のまま止まる（`Terminate` は system message なので処理される）。インスタンスが無いので `post_stop` は走らない |
| all-for-one | 今の `apply_all_for_one` と同じ判断。やり直すなら、失敗した子に `Create` を積み、兄弟は 11.12.5 の再起動。止めるなら、兄弟と一緒に `Terminate` |
| decider の panic | 今と同じく親の失敗にする（`fail_supervisor`）。子は suspend のまま待つ。親が再起動されれば、親の再起動の後半が子に `Create` を送る（11.12.5）。親が止まれば、子は T1 の `Terminate` で止まる |

**やり直しの `Create` が 2 通並ぶとき**：子の `Create` の送り手は 2 つある。子の開始の失敗への Restart の判断（上の表）と、
親の再起動の後半（11.12.5。「開始に失敗した」の記録があり、判断が済んだ子に送る）である。両方が同じ子に効くことがある。子 `c1` の
開始が失敗し、判断が Restart で `c1` に `Create` が積まれたあと、`c1` の実行より先に、親が自分の失敗（または兄弟の decider の
panic）で再起動され、後半が `c1` にもう 1 通 `Create` を送る筋書きである（設計の独立レビューの高 1）。`c1` の次の実行は、system
message を列の残りまで処理するので（11.12.8）、2 通を同じ実行で処理する。何もしなければ、1 通目がもう一度失敗したあと、2 通目が
判断を経ずに作り直し、2 回目の失敗の記録は `after_run` の 1 枠に入らず失われ、再起動の回数の勘定も狂い、1 回目の失敗への判断が
作り直せた子に当たる。

4a は、`Create` の処理の側で閉じる（上の表の「その失敗への判断がまだ」の行）。作り直すのは、判断が済んだ開始の失敗に対して
だけで、判断のまだの失敗に届いた `Create` は何もしない。1 通目が作れたなら、2 通目は「すでに作られている」で何もしない。
1 通目がまた失敗したなら、2 通目は何もせず、2 回目の失敗は実行のあとで判断され、その判断が次の作り直しを決める。どの送り手が
何通送っても、判断 1 回につき作り直しは 1 回で、1 つの実行で記録される開始の失敗は 1 件である（11.12.8）。Quint の
`oneRetryPerDecision` と、負の対照 `RetryUndecided`（判断のまだの失敗にも作り直す形）で確かめる。

原典との関係：原典の作り直しは、`Recreate` を受けた `faultRecreate` が、インスタンスが無ければ `faultCreate` へ落とす形で進む。
同じ重なりでは、原典の子の列にも `Recreate` が 2 通並びうる（親の Restart の判断の `Recreate` と、親の `finishRecreate` が
生き残りの子に送る `Recreate`）。`Recreate` は失敗中に取っておかれない（`StashWhenWaitingForChildren` だけで、`StashWhenFailed`
ではない）ので、原典では 2 通目も作り直す。原典が壊れないのは、失敗ごとに別の `Failed` が親に届いてそれぞれ判断されること、
親の `handleInvokeFailure` が子に `Suspend` を送って suspend の数が合うことによる。4a は失敗の記録が `after_run` の 1 枠で、
`Suspend` も送らない（11.12.9）ので、原典と同じく 2 通目で作り直すと記録が失われる。2 通目を捨てても、原典で起きることのうち
失われるものは無い。2 通目の作り直しは、原典では判断のまだの失敗を追い越す作り直しで、その失敗の `Failed` への判断が、作り直せた
子をあとから再起動するか止める。4a では、その失敗の判断のあとで作り直す。原典より順が 1 つ遅れるだけで、判断を経ない作り直しは
無くなる。

選ばなかった 2 つ（レビューの候補）：

- **親の再起動の後半が、やり直しの `Create` が積まれている子には送らない**（候補 a）：子のセルに「やり直しの `Create` が未処理」を
  持たせ、送り手どうしで調整する形である。送り手が増えるたびに（段 4b で `Recreate` を system message にするときなど）、同じ
  調整が要る。受け手の側で閉じる形なら、送り手の数に頼らない
- **`Create` が失敗した実行では、後ろの `Create` を処理せず列に残す**（候補 c）：実行が system message を列の残りまで処理する形
  （I11、`mailbox_scheduling.qnt` の `systemBeforeNormal` と、段 3 の drain）を、`Create` だけ崩す。残した `Create` は、次の判断の
  あとで、前の判断の分として処理され、判断と作り直しの対応が 1 つずれる

**判断の時点で子が止まっていたとき**：開始の失敗と `Terminate` が同じ実行の列に並ぶと（作ってすぐ止めた子の開始が失敗した
場合など）、実行が返ったときには子は閉じている。原典では、親は子の `Failed` を `DeathWatchNotification` より先に処理するので、
止まった子にも判断を当てる（`FaultHandling.handleFailure` は、子が親の表にある間は判断する）。4a もこれに合わせ、失敗を
publish して判断を当てる。Restart と Stop は止まった子には何もしない。decider の panic は親を失敗させる。判断に要る値（再起動の
回数）はセルから読めなくなるので、`StartFailed` と `Failed` の記録に、失敗した時点の回数を入れておく。

**親の戦略を読む時点**：子の開始の失敗の判断は、親の `ParentEntry` の戦略（子が受け継ぐ戦略）を読む。親の `Create` がまだなら、
既定の戦略を読んでしまう。4a の手で回す dispatcher は、実行待ちを予約の順（FIFO）に回し、親は子より先に予約される（親の
`Create` は、親が子を作れるようになる前に積まれる）。したがって、子の実行より先に親の `Create` が済む。段 4b で判断を親の実行に
移すと、この前提は要らなくなる（親の列では `Create` が先にある）。

**`Supervise` は送らない**：原典の `init` は親に `Supervise` を送るが、同期の `actorOf` では `makeChild` の `initChild` がすでに
子を登録しており、`supervise` は登録を確かめて debug のログを出すだけである（`ActorCell.supervise`。`handleSupervise` が働くのは
非同期の生成のときだけ）。fraktor には非同期の生成が無いので、送らず、`SystemMessage` からも外す。

**インスタンスの無いセルへの通常のメッセージ**：今の `ActorCell::receive` は、インスタンスが無ければ何もしない。4a では、
`Create` が通常のメッセージより先に処理され（I11）、開始に失敗したセルは suspend されたまま（上の判断の表と 11.12.6）なので、
この分岐には届かない。届かないことを Quint の `noReceiveWithoutInstance` と `failedStartStaysSuspended` で確かめる。分岐
そのものは今のまま残す。

#### 11.12.3 停止（Terminate）と T1〜T5

**停止を頼む口**：どれも、対象に `Terminate` を積むだけにする（原典は `ActorCell.stop` が `Terminate` を送る。`context.stop`
も `system.stop` も同じ）。

| 口 | 今 | 段 4a |
|:---|:---|:---|
| `ActorContext::stop`、`ActorSystem::stop`、`WeakActorSystem::stop` | `pending_stops` に積むか、`stop_at_boundary` で同期に止める | 対象に `Terminate` を積む。自分を止めるなら自分の列に積み、いまのメッセージのあとに処理される（I11） |
| `PoisonPill` | `after_run` の `PoisonPill` から `stop_at_boundary` | 自分に `Terminate` を積み、`Continue`（原典 `autoReceiveMessage` の `self.stop()`）。後ろのメッセージは、閉じるときに dead letters へ行く（今と同じ） |
| `stop_and_reply_after_termination` | 返事を記録し、成功したら `after_run` の `Stop` | 返事を記録し、自分に `Terminate` を積む。返事は T5 で送る（今と同じ）。失敗と重なったときは停止が勝ち、判断を当てない（今と同じ） |
| 判断の Stop、all-for-one の Stop、回数を超えた Restart、`Kill` の Stop | `stop_with_sender` を同期に呼ぶ | 対象に `Terminate` を積む |
| T1 の子の停止、`pre_restart` の子の停止（`stop_children`）、開始のやり直しの前の子の停止 | 同期に止める | 子に `Terminate` を積む |
| function ref | 実行器が同期に閉じ、子を止め、通知する | 実行器が今と同じく閉じる。子がいれば子に `Terminate` を送り、最後の子の `ChildTerminated` を実行器が受けたときに watcher へ通知する（T1〜T4 の順を保つ。`stopping_a_function_ref_stops_its_child_first_and_tells_both_watchers`） |

閉じたメールボックスに届いた `Terminate`（止まったアクターをもう一度止める）は、今と同じく何も起こさない（下の「閉じるときの
dead letters」）。停止中のアクターに届いた `Terminate` も何もしない（原典 `terminate` の `wasTerminating`）。したがって、停止は
1 回しか終わらず、段 3 で直した「終了のコールバックが 2 回走る」は起きない
（`a_poison_pill_stops_an_actor_once_when_its_hook_asks_to_stop_it_again`）。

**`Terminate` の処理**（対象の実行の中。原典 `FaultHandling.terminate`）：

1. 理由が `Termination` で子を待っている（停止中）なら何もしない。ほかの理由（`Creation`、`Recreation`）で待っているときは、
   11.12.4 のとおり理由を `Termination` に替え、待つ子をすべての子に広げて、2 から進む（原典 `terminate` の
   `setChildrenTerminationReason`）
2. T1：自分が watch している相手をすべて外す（`watchers`、`watch_messages`、`pending_terminations`、`function_watching` の
   うち、自分が watcher のもの。今の `finish_termination` の先頭の 4 行を、ここへ前に出す）。原典の `unwatchWatchedActors` で、
   待つ間に届く `Terminated` が dead letters に溜まらないようにする
3. 子のすべてに `Terminate` を送る（親の表の子。すでに閉じて `ChildTerminated` が自分の列にある子にも送り、それは閉じた
   メールボックスに断られる）
4. 子がいれば「子を待つ（理由：Termination）」にし、メールボックスを suspend する（`suspendNonRecursive`）。待つ間は通常の
   メッセージを処理しない。子がいなければ、すぐ 5 へ
5. finishTerminate（下の表）

**finishTerminate**（子のいない `Terminate` を処理した実行か、最後の子の `ChildTerminated` を処理した実行の中。原典
`FaultHandling.finishTerminate`。今の `stop_with_sender` の後半を、この順に並べ直す）：

| 順 | すること | 今の関数 |
|:---|:---|:---|
| T2 | インスタンスがあれば、`post_stop` を catcher で包んで回す（失敗は publish して続ける）。`sender()` は、失敗したメッセージの送り手（あれば）か dead letters | `run_post_stop` |
| T2 | function ref を閉じる（今と同じく `post_stop` の前と後の 2 回） | `stop_function_refs_of` |
| T3 | stash を dead letters へ流し、メールボックスを閉じて、残りの通常のメッセージを dead letters へ流す（条件 3）。`LiveActors` から外す。待っている `Run` は無い（自分の実行の中なので） | `stash_to_dead_letters`、`queued_to_dead_letters` |
| T4 | 親に `ChildTerminated` を送る（メールボックスの無い親は、その場で名前を外す。11.12.4）。そのあと watcher に `Terminated` を送る | `notify_watchers` |
| T5 | 終了の通知（`on_termination`）を回し、`stop_and_reply` の返事を送る | `finish_termination` の残り、`flush_deferred_replies` |
| — | エントリを表から外す。セルは、実行の終わりの guard が、ロックの外で捨てる（§11.6 の L3） | — |

今との順の違いは T4 である。今は watcher への通知のあとで親のフックを同期に回すが、4a は親への `ChildTerminated` を先に送る。
親が watcher でもあるときは、親の列で `ChildTerminated`（system message）が `Terminated`（通常のメッセージ）より先に処理される
（原典 finishTerminate の注釈「if a parent is also a watcher then ChildTerminated and Terminated must be processed in this
specific order」）。

**失敗したメッセージの送り手**：今は `stop_with_sender(target, sender)` が、`post_stop` に渡す送り手を引数で運ぶ。4a では停止が
`Terminate` になるので、セルが「失敗したメッセージの送り手」を持ち、`post_stop` と `pre_restart` がそれを `sender()` として読む。
原典の `currentMessage` が、失敗のあとも失敗した封筒を指したままであることに当たる（`ActorCell.invoke` は、失敗したときに
`currentMessage` を戻さない）。次のメッセージを処理するときと、再起動のあとに消す。`Terminate` には送り手を持たせない（原典の
`Terminate` も持たない）。

**§11.6 の L3**：`Terminate` を処理して閉じた実行は、セルを戻さず、`with` を出てからロックの外で捨てる。段 3 の guard
（`RunGuard`）がすでにこの形で、段 3 では届かなかった（停止は実行の外だった）。4a では、`Terminate` で閉じる実行がこの経路を
通る。単体テストで確かめる（11.12.12）。

**閉じるときの dead letters**：

| 残っていたもの | 段 4a |
|:---|:---|
| 通常のメッセージ（`PendingTermination` は `Terminated` にする） | 今と同じく dead letters へ（条件 3） |
| stash | 今と同じく、通常のメッセージより先に dead letters へ |
| system message（閉じたときに残っていたもの、閉じたあとに届いたもの） | 捨てる（下） |
| `stop_and_reply` の返事 | T5 で送る（今と同じ） |

原典は、残った system message と閉じたあとに届いた system message を dead letters のメールボックスへ回す
（`Mailbox.processAllSystemMessages`、`Mailboxes.deadLetterMailbox`）。dead letters は `Watch` に `DeathWatchNotification` を返し、
ほかは `DeadLetter` として publish する。4a で閉じたメールボックスに残りうるのは、`Terminate`（もう一度の停止）、`Resume`
（decider の panic のあとの再開。11.12.6）、`Create`（開始のやり直しが停止と重なった場合）である。`ChildTerminated` は残らない
（親は子の通知を揃えてから閉じる。Quint の `noLostChildTerminated`）。4a ではこれらを、今と同じく捨てる（段 3 の
`queued_to_dead_letters` も捨てている）。今は、止まったアクターをもう一度止めても dead letters に何も出ない。原典の
`DeadLetter(Terminate)` などは、利用者が中を見られない内部の型で、dead letters の配送は best effort である。4a で出し始めると、
dead letters を数える既存のテストの数が変わりうる。`Watch` を送るようになる段 4c で、閉じたメールボックスの system message を
dead letters の規則（`Watch` に通知を返す）へ回すときに、一緒に決める。`actor_termination.qnt` の `sysDead` と `deadLettered` は、
閉じたメールボックスへ行った数を数えるだけで、不変条件は読まない。I12 の「system message も失われない」の保存の主張は、
1 つのメールボックスについて `mailbox_scheduling.qnt` の `systemMessagesConserved` が担う。

#### 11.12.4 子の名前と ChildTerminated

**親の表**（今の `ParentEntry`。原典の `ChildrenContainer`）：

```rust
struct ParentEntry {
  parent: ActorRef,
  strategy: SupervisorStrategy,
  /// Every child, named or not, from its creation until the parent has
  /// handled its `ChildTerminated` (Pekko's ChildrenContainer).
  children: Vec<(String, ActorRef)>,
  /// What the parent waits for its children for, and the children it waits
  /// for (Pekko's TerminatingChildrenContainer).
  waiting: Option<Waiting>,
}

struct Waiting {
  to_die: Vec<ActorRef>,
  reason: WaitReason,
}

/// Pekko's ChildrenContainer.SuspendReason without UserRequest: a child that a
/// user stops leaves nothing to go on with once it has gone.
enum WaitReason {
  Creation,
  Recreation(/* what the second half of the restart needs: the failure */),
  Termination,
}
```

- 今の `stopping: bool` は、待つ理由が `Termination` であることに替える（原典の `isTerminating`）。子の生成を断る条件は今と同じ
- `Termination` は、ほかの理由で待っている間に届いても上書きし、待つ子をすべての子に広げる（原典 `setChildrenTerminationReason`）
- 子の一覧（T1 で止める子、all-for-one の兄弟、`stop_children`）は、この表から引く。今の `children_of`（セルをエントリの表から
  探す）は、実行中の子を落とすので使わない

**`ChildTerminated` の処理**（親の実行の中。原典 `FaultHandling.handleChildTerminated`）：

1. 子を親の表から外す（名前が空く）。親が user guardian なら `user_children` からも外す。外すのは、`ChildTerminated` が持つ
   `ActorRef`（id と世代）が表の登録と一致するときだけで、一致しない通知は表を変えない（原典 `handleChildTerminated` の
   `childrenRefs.getByRef`。Quint の `handleChildTerm` も世代が一致するときだけ名前を空ける）。名前だけで外すと、古い世代の
   通知が、同じ名前で作り直した子の登録を消しうる
2. 親が作られていれば、子の終了のフックを catcher で包んで回す（今の `execute_child_terminated_hook` を、実行が持つセルで回す
   形にする）。失敗と、停止中の親での子の生成の拒否は、今と同じく `IllegalStateException("handleChildTerminated failed")` を
   1 回 publish する（原典はさらに `handleInvokeFailure` で親を失敗させる。監督を system message にする段 4b で扱う）
3. 待つ理由があり、待つ子が空になったら続きを行う。`Termination` なら finishTerminate、`Creation` なら作る（11.12.2）、
   `Recreation` なら再起動の後半（11.12.5）

**メールボックスの無い親**：user guardian、system guardian、root guardian と、function ref（testkit の `child_actor_of` の親）
には、`ChildTerminated` を積む列が無い。子の finishTerminate の中で、実行器がその場で上の 1 を行う（ガーディアンにフックは無い。
function ref の親にもフックは走らない。今の `execute_child_terminated_hook` の `is_created` の検査と同じ）。function ref が自分の
停止で子を待っていれば、3 も行う（11.12.3 の function ref の行）。原典のガーディアンはアクターで、通知を自分の実行で処理するが、
fraktor のガーディアンはセルを持たないので、その場で行う。ガーディアンの下の名前は、子の終了の処理（子の finishTerminate）の
中で空くので、親の実行を待つ形より遅くはならず、原典（ガーディアンが自分の実行で通知を処理してから空く）より早く空くことがある。

**`LiveActors`（選択の表）**：今と同じく、生成で入れ、閉じるときに外す。原典では、閉じた子も親が通知を処理するまで `getChild`
で見つかり、その子の閉じたメールボックスを通って dead letters になる。4a では、閉じた子のパスは解決できず、誰もいないパスとして
dead letters になる（dead letters の受け手が、子のハンドルでなく、そのパスの空のハンドルになる）。この違いは、ワイルドカードの
展開を入れる段 4c で、子の表の持ち方と一緒に見直す（11.12.9）。

#### 11.12.5 再起動が子を待つ（Recreation）

再起動そのもの（`restart_cell`）は、段 3 と同じく、実行のあとに実行器が行う（段 4b で `Recreate` の system message にする）。
4a で停止が非同期になると、`pre_restart` で止めた子は、`post_restart` の時点ではまだ止まっていない。原典は、その子の
`ChildTerminated` が揃うまで `finishRecreate` を待つ（`faultRecreate` の `setChildrenTerminationReason(Recreation(cause))`）。
4a もこの形にする。UidClash の補助のテスト（`a_restart_completes_the_stop_of_the_grandchild_before_post_restart_replaces_it`
など）が、この順に頼っている。

`restart_cell` を 2 つに分ける。

| 半分 | すること | どこで |
|:---|:---|:---|
| 前半（原典 `faultRecreate`） | stash を戻し、`pre_restart` を回す（catcher で包む。今と同じ）。`pre_restart` が止めた子（`Terminate` を送った自分の子で、まだ親の表にいるもの）を待つ子にする | 判断を当てる実行器（今と同じ） |
| 後半（原典 `finishRecreate`） | 新しいインスタンスを作り、`post_restart` を回す（今の `ActorCell::restart`）。suspend を 1 つ解く（11.12.6）。`Kill` の生き残りの子は、今と同じく同期に再起動する（段 4b で `Recreate` にする）。開始に失敗して親の再起動を待っている子（「開始に失敗した」の記録があり、判断が済んでいる子）に `Create` を送る | 待つ子がいなければ、前半に続けてその場で。いれば、最後の子の `ChildTerminated` を処理する、そのアクターの実行の中 |

後半で、作られていない子に `create_initial` を同期に回す今の形はやめ、`Create` を送る。これが回復の重なりを深さ 1 に保つ要
である（11.12.7）。まだ最初の `Create` を処理していない子には送らない（その `Create` が作る）。

**待つ間の suspend**：待つ間、そのアクターは通常のメッセージを処理してはいけない。失敗した本人は、失敗の suspend が残っている。
失敗していないのに再起動されるアクター（all-for-one の兄弟、decider の panic で失敗した親、`Kill` の生き残りの子）は、前半の
始めに実行器が suspend し、後半が解く（原典の all-for-one の `suspendFirst` と、decider の panic を `systemInvoke` で受けた親の
`handleInvokeFailure` の suspend に当たる）。実行の外で suspend の数を変えるのは、I4（数を変えるのは実行中のスレッドだけ）から
外れる。ただし 4a は手で回す 1 スレッドで、前半を回す間そのアクターの実行は無い（実行器がセルを持っている）ので、競合は
起きない。段 4b で `Recreate` を system message にするときに、実行の中へ移す。

**待つ間に届く system message**：原典は、子を待つ間に届いた `Recreate`、`Suspend`、`Resume`、`Failed` を取っておき、待ちが
終わってから処理する（`ActorCell.systemInvoke` の `StashWhenWaitingForChildren`）。4a で待つ間に届きうるのは、`Terminate`
（理由を `Termination` に上書きする。11.12.4）、`ChildTerminated`（処理する）、`Create`（停止中なら何もしない。11.12.2）である。
`Resume` は届かない（11.12.6 で、再起動の判断は `Resume` を送らず、後半が解く形にした）。取っておく仕組みは、`Suspend` などを
送るようになる段 4b で入れる。届けばどうなるかは、Quint の負の対照 `ResumeOnRestart` が示す（待つ間に通常のメッセージを
処理する）。

`complete_child_stops_of` は消える。`pending_stops` の中から子への停止だけを先に行う仕組みは、停止が `Terminate` になると要らない。

#### 11.12.6 suspend と Resume の数え方（段 3 の `resume_if_open` を改める）

段 3 は、失敗と `Kill` の判断を当てたあと、メールボックスが開いていれば必ず `Resume` を送る（`finish_run` の `resume_if_open`）。
判断の効果が同期に終わる段 3 ではこれで合っていたが、4a で停止と再起動の後半が非同期になると、次の 3 つの誤りになる。

- decider の panic で親が止まると、子の `Terminate` は親の T1 から後で届くが、`Resume` はすぐ送られる。子は止まる前に後ろの
  メッセージを処理する（原典では、子は親の判断が来るまで suspend のまま）。判断が Stop のときは、`Resume` が `Terminate` の
  後ろに並ぶので害は出ないが、数え方を 1 つにするため送らない
- 判断が Restart で、再起動が子を待つと、`Resume` が待つ間に処理され、待つ間に通常のメッセージを処理する（Quint の
  `ResumeOnRestart`）
- 開始の失敗に `Resume` を送ると、やり直しの `Create` が解く分と合わせて 2 回解くことになる。2 度目の開始の失敗のあと、
  インスタンスの無いまま suspend が 0 になり、通常のメッセージが届く（Quint の `ResumeAfterStartFailure`）

4a の決まりは、「失敗 1 回につき suspend 1 回。それを解くのは、判断の効果が終わったときの 1 回だけ」である。どこが解くかは
原典に合わせる。

| 失敗 | 判断 | 解くもの | 原典 |
|:---|:---|:---|:---|
| `receive` の失敗、`Kill` | Restart | 再起動の後半 | `finishRecreate` の `resumeNonRecursive` |
| 同上 | Stop | 解かない（suspend のまま `Terminate` を処理して閉じる） | 止まるまで suspend のまま |
| 同上 | decider の panic（親の失敗） | 親が再起動されて残ったときだけ、実行器が `Resume` を送る。親が止まるなら送らない（子は T1 で止まる） | 親の再起動で生き残りの子が `Recreate` を受けるか、親の `faultResume` の `resumeChildren` |
| 開始の失敗 | Restart | やり直しの `Create`（11.12.2） | `finishCreate` の `resumeNonRecursive` |
| 同上 | Stop | 解かない | 同上 |
| 同上 | decider の panic | 親の再起動の後半が送る `Create`（11.12.5） | 親の再起動で、作られていない子が `Recreate` を受けて `faultCreate` |
| 失敗していないのに再起動されるアクター | — | 再起動の後半（前半の始めの suspend を解く） | `suspendFirst` と `finishRecreate` |

「親が残ったか」は、decider の panic を当てた結果（親への判断が Restart だったか）で決める。`fail_supervisor`、`apply_failure`、
`handle_kill` が判断の結果を返す形に直す。

**親が suspend されている間の子の失敗**：4a では、親が suspend されたまま、複数の実行にまたがって残ることがある（自分の失敗の
判断を待つ間、開始に失敗して判断かやり直しを待つ間、子を待つ間）。その間に別の子の失敗の判断を当てると、親の戦略の decider の
panic が、子を待っている親や、インスタンスの無い親を失敗させ、進行中の再起動の中でもう一度再起動する（11.12.7 の深さ 1 が
崩れる）。decider が panic するかは回してみるまで分からないので、どの判断も取っておく。

- 子の失敗（`receive` の失敗、`Kill`、開始の失敗）の判断は、親が suspend されていないときに当てる。親が suspend されていれば、
  実行器は失敗を親の `ParentEntry` に取っておき、子は suspend のまま待つ
- 取っておいた失敗は、親の suspend が 0 に戻ったときに、届いた順に判断する。実行器は、親の実行が返ったときと、親への判断の
  効果を当てたあとに、親の suspend を見る
- 親が閉じたら、取っておいた失敗は捨てる（子は T1 で止まっている。原典でも、閉じる親が取っておいた `Failed` は判断されない）
- 取っておいた失敗を判断するのは、失敗した子が今も親の表にいる（`ActorRef` の id と世代が表の登録と一致する）ときだけである。
  表に無い子の失敗は、判断せずに捨てる（decider も呼ばない）。取っておく間に、親の再起動の `pre_restart` や開始のやり直しの
  前の停止がその子を止め、親がその `ChildTerminated` を処理して表から外していることがある。原典 `FaultHandling.handleFailure`
  （`FaultHandling.scala:305-323`）は、`getChildByRef` で子が親の知る子であり、UID が `Failed` の持つ uid と一致するときだけ
  判断し、ほかは「dropping Failed from unknown child」「dropping Failed from old child」として捨てる（`preRestart` で止めた子からの
  `Failed` を受けないための照合）。取っておいた `Failed` が戻るのは `finishRecreate` のあとで、その時点で止めた子は表に無い。
  11.12.2 の「判断の時点で子が止まっていたとき」は、`Failed` が `DeathWatchNotification` より先に処理され、子がまだ表にいる場合で、
  取り置きを経る判断には及ばない。11.12.4 の `ChildTerminated` の照合と同じ関門を、判断の側にも置く
- 原典の `Failed` が `StashWhenFailed` と `StashWhenWaitingForChildren` で、親が失敗中か子を待つ間は取っておかれることに当たる
  （`ActorCell.systemInvoke` の `shouldStash`）。段 4b で判断を親の `Failed` にすると、これは親の列の取り置きになる
- 段 3 では、判断が実行のすぐあとに同期に終わるので、親が suspend されたまま子の判断を待つ時点は無い。段 3 の振る舞いは変わらない

Quint では `decide` の前提（親が閉じているか、親の suspend が 0）で表す。設計の独立レビューの指摘ではなく、中 1 を直すときに、
直す前の仕様が decider の panic の判断だけを「親が子を待つ間は起きない」として外していた（`canRestart` の前提）のを見て足した。
実装は decider が panic するかを前もって知れないので、判断のすべてを取っておく形にした。

#### 11.12.7 回復の重なりの深さ 1（#251 の 2 件）

`executor_value_test.rs` の `repeated_child_recovery_failures_obey_parent_retry_limit` と
`repeated_child_recovery_failures_do_not_nest_parent_recovery` は、書き換えずに深さ 1 のまま通す（#253 の完了条件、元帥の決定
2026-11-26）。

**#251 の試作が深さ 2 になった理由**：親の `receive` が子を作り、子の `pre_start` が失敗し、子の戦略（親から受け継ぐ）の
decider が panic する。decider の panic は親の失敗になり、親の親（one-for-one）が親を再起動する。親の `pre_restart` は回復の
範囲（テストの `RecoveryScope`）を古い振る舞いの枠に入れ、その枠は再起動が終わるまで生きている。試作では、親の再起動の後半が、
作られていない子に `create_initial` を同期に回した。子の `pre_start` がまた失敗し、decider がまた panic し、その親の失敗が、
まだ終わっていない親の再起動の中で当てられた（`finish_boundary` が、実行中の枠が空になったその時点で走った）。親が自分の
再起動の中でもう一度再起動され、`post_restart` が数えた範囲が 2 になった。原典では、この流れの各段（子の `Create`、親への
`Failed`、親の `Recreate`、子の `Recreate` から `faultCreate`）が別々の system message で、それぞれ別の実行で進むので、入れ子に
ならない。

**4a の決まり**：生成（インスタンスを作り `pre_start` を回すこと）は、その子自身の実行が `Create` を処理するときにだけ行う。
開始のやり直しも、親の再起動の後半が作られていない子を作り直すのも、`Create` を送るだけにする（11.12.2、11.12.5）。これで、
親の再起動の中で子の `pre_start` が走ることは無くなる。子の 2 度目の失敗は、親の再起動が返り、古い振る舞いの枠が捨てられた
あとの、子の次の実行で起きる。

`run_recovery(failures = 3, max_retries = 3)`（`do_not_nest`）の流れ：

1. 親が `"go"` を処理し、子 `bob` を作る（名前を予約し、`Create` を積む）
2. `bob` の実行が `Create` を処理し、`pre_start` が失敗する。suspend して `StartFailed` を記録する
3. 実行が返り、実行器が判断する。decider が panic し、親の失敗になる。親は実行中でないので、その場で親の親の戦略が親を
   再起動する。前半で `pre_restart` が範囲を 1 にする。止めた子は無いので、その場で後半に進み、`post_restart` が深さ 1 を記録し、
   `bob`（開始に失敗し、判断が済んだ子）に `Create` を送る。再起動が返り、古い枠が捨てられて、範囲は 0 に戻る
4. `bob` の次の実行が 2 と 3 を繰り返す（2 回目、3 回目）。4 回目の開始が成功する。試行 4、再起動 3、深さ 1

`run_recovery(failures = 3, max_retries = 2)`（`obey_parent_retry_limit`）では、3 回目の再起動が回数を超え、親への判断が Stop に
なる。親に `Terminate` を積み、`bob` には `Resume` も `Create` も送らない（11.12.6）。親の実行が T1 で `healthy` と `bob` に
`Terminate` を送って待ち、両方の `ChildTerminated` のあとで閉じる。試行 3、再起動 2、深さ 1、親と子が止まり、`bob` についての
終了のフックは 1 回である（親が止まる途中でも、作られた親のフックは走る。原典 `handleChildTerminated` の `actor != null`）。

Quint では、`actor_termination.qnt` の `recoveryNotNested` と、負の対照 `CreateInsideRestart` が、この決まりを表す。各アクターは
進行中の再起動の数（`restarting`。前半の始めで増え、後半の終わりで減る）を持ち、`recoveryNotNested` は、再起動が始まる時点で
同じアクターの再起動が進行中でない（深さが 1 を超えない）ことである。`CreateInsideRestart` は、#251 の試作の遷移そのものを書く。
親の再起動の後半が、開始に失敗した子の作り直しをその場で行い、作り直しが失敗すると、親の再起動は、その子の失敗が判断される
まで進行中のまま残る（試作で、古い振る舞いの枠が生きている間に `finish_boundary` が判断したことに当たる）。その判断の decider が
panic すると、親の再起動が進行中のまま、親の再起動が始まり、深さが 2 になる。正しい形では、再起動が進行中のアクターは子を待って
suspend されており、子の失敗の判断は取っておかれる（11.12.6）ので、同じ数で深さ 1 が保たれる。

#### 11.12.8 利用者のコードを走らせる system message を catcher で包む（§11.11.13 から）

| system message | 利用者のコード | catcher があるとき（原典の NonFatal） | catcher が無いとき（原典の fatal） |
|:---|:---|:---|:---|
| `Create` | `Props` の factory、`supervisor_strategy`、`pre_start` | 失敗したら suspend し、`after_run` が空なら `StartFailed` を記録して、列の残りを処理し続ける（§11.11.2 の `failSystem` の形） | 段 3 の巻き戻り。実行の guard が後始末をし、次の呼び出しでシステムが終わる（`in_run` が立っているので致命的になる） |
| `Terminate`（finishTerminate） | `post_stop` | 今の `run_post_stop` と同じく publish し、停止を最後まで進める | 同上 |
| `ChildTerminated` | 子の終了のフック | 今と同じく `IllegalStateException("handleChildTerminated failed")` を 1 回 publish し、続きを行う | 同上 |
| `Resume` | なし | — | — |

`after_run` が空でないとき（同じ実行で、先に `receive` が失敗している）は、後の失敗を記録しない（原典の `isFailed`。
§11.11.2）。4a では、1 つの実行で記録される失敗は高々 1 件である。失敗したら suspend して通常のメッセージを取らず、`Create` が
もう一度失敗するには、間に実行のあとの判断が要る。同じ実行の列に `Create` が 2 通並んでも、判断のまだの失敗に届いた 2 通目は
何もしない（11.12.2 の「やり直しの `Create` が 2 通並ぶとき」）。

#### 11.12.9 段 3 から回した 2 点

**1. `SystemMessage` を `pub(crate)` にする**（#253 のコメントの 1）。4a が送るのは、`Create`、`Resume`、`Terminate`、
`ChildTerminated` の 4 種類である。ほかの 7 種類は、`pub(crate)` にすると、どこからも作られないという dead_code の警告になる
（抑制は人間が決める）。そこで、4a では送らない種類を enum から外し、送り始める段で足す。

| 種類 | 4a | 足す段 |
|:---|:---|:---|
| `Create`、`Terminate`、`ChildTerminated` | 送る | — |
| `Resume` | 送る（段 3 から。11.12.6） | — |
| `Suspend`、`Recreate`、`Failed` | 外す | 4b |
| `Watch`、`Unwatch`、`DeathWatchNotification` | 外す | 4c |
| `Supervise` | 外す | 足さない（11.12.2。非同期の生成が無い） |

段 2 と段 3 の単体テストは、`Suspend` を 6 か所で使っている（`mailbox_run_test.rs` の `Recorder` と `Fragile` の枝と 2 つの
テスト、`mailbox_test.rs` の 1 つのテストの 2 か所）。これは、実装と別の、テストだけのコミットで形を直す（§11.11.8 の「段 2 の単体テスト」の
行と同じ扱い）。

| テスト | 今 | 直す形 | 確かめる性質 |
|:---|:---|:---|:---|
| `a_suspended_mailbox_handles_only_system_messages_until_resumed` | `Suspend` の system message で suspend する | 1 件目の通常のメッセージの処理の中で suspend する（`Recorder` の `on_message`。I4 のとおり、数を変えるのは実行の中） | suspend されたメールボックスは、`Resume` まで system message だけを処理する（変えない）。記録の文字列は `"suspend"` から `"m0"` に変わる |
| `a_panic_among_system_messages_raises_the_flag_again` と `Fragile` | `Suspend` で panic し、`Resume` が残る | `Create` で panic し、`Resume` が残る | 巻き戻りで印が立て直される（変えない） |
| `closing_a_mailbox_with_its_own_queue_hands_back_what_it_held` | `Suspend` と `Create` を積んで閉じる | `Resume` と `Create` を積んで閉じる | 閉じると、積んだ system message が順に返る（変えない） |
| `Recorder` の `system_invoke` | `Suspend` の枝 | 枝を消す | — |

記録の文字列が 1 か所変わるので、このコミットの形も、設計の独立レビューで見てもらう。

設計の独立レビュー（2026-11-27、`d436bea` に対して）は、この表のとおり triage として扱うのでよいとした。3 件とも確かめる性質は
変わらず、記録の文字列の変化は確かめる対象ではない付随の変化である。実装で避ける道（送らない `Suspend` を enum に残す）は、
dead_code の抑制が要り、#253 の完了条件（4a が送る 4 種類だけを残す）に反する。条件は 2 つで、この直しを実装と別の、テストだけの
コミットに分けることと、表のとおりであることを設計の独立レビューで見たという記録（この段落）を、実装の PR にも載せることである。

**2. ワイルドカードの `ActorSelection`**（#253 のコメントの 2）。今の `ActorSelection` は、パターン（`*`、`?`）を持つパスを
展開しない。生きているアクターの完全なパスだけを送り手のスレッドで引き、ほかは境界を通って、配るときに完全なパスとして
解決するので、ワイルドカードのパスは誰もいないパスとして dead letters になる。原典の `deliverSelection` は、送り手のスレッドで、
アンカーから子の表（`refWithCell.children`、`getSingleChild`）をたどり、パターンに合う子のすべてへ送る
（`ActorSelection.scala:208-260`。合う子が無く、展開の途中でもなければ、空のハンドルへ）。

これは **段 4c** で扱う。理由は 3 つである。

- 展開は、送り手のスレッドから子の表を読む。4a は子の表を実行器の中（`ParentEntry`）に置いたままにし、送り手のスレッドから
  読める表は `LiveActors`（パスからハンドル）だけである。文脈から `&mut Executor` を外し、子の表をセルか共有の表へ移すのは
  段 4 の終わりで、展開はその表を読むのが原典の形に近い
- 4a は生成と停止の意味論の段で、選択の機能を足すと、レビューと洗い出しの範囲が広がる。4c は 3 段の中でいちばん小さい
  （§6 の大きさ 5、5、3）
- 4a が決める子の表の中身（生成から、親が `ChildTerminated` を処理するまで）は、展開が読むものとして、そのまま使える
  （11.12.4）。4c で、11.12.4 の `LiveActors` から外す時点の違いも一緒に見直す

段 4c の issue の完了条件に、「ワイルドカードの選択を送信の時点で展開する（原典 `deliverSelection`）。展開の途中で合う子が
無い枝は dead letters にしない（`wildcardFanOut`）。function ref はパターンに合わない（`children` に入らない）が、名前では
選べる（`getSingleChild`）」を入れるよう、中将へ上げる。

#### 11.12.10 testkit と公開 API への影響

| 何が | 今 | 段 4a |
|:---|:---|:---|
| `ActorSystem::actor_of` など生成の口 | `pre_start` を済ませて返る。開始の失敗の判断も済んでいる | `pre_start` の前に返る。開始と、その失敗の判断は、次に回したとき |
| `ActorSystem::stop` | 止め切って返る | `Terminate` を積んで返る。`is_actor_terminated` は、回すまで `false` |
| 名前の再利用 | 止めた直後から使える | メールボックスを持つ親の下では、親が `ChildTerminated` を処理したあと（`run_until_idle` のあと）から使える（原典と同じ）。ガーディアンと function ref の下（トップレベルの名前など）では、子の終了の処理の中でその場で空く（11.12.4 の「メールボックスの無い親」） |
| 既定の `pre_restart`（`stop_children` のあと `post_stop`） | 子を止め切ってから `post_stop` | 子に `Terminate` を送ってすぐ `post_stop`（原典と同じ。子が止まるのは後で、`post_restart` は子が止まってから） |
| catcher の無い `pre_start` の panic | `actor_of` から出る | `run_until_idle` などから出る。次の呼び出しでシステムが終わるのは同じ |
| testkit の `TestFsmRef` | 生成が同期であることに頼り、開始に失敗した機械を、作った直後の `is_actor_terminated` で見分ける（原典の `TestFSMRef` は CallingThreadDispatcher で同期に作る） | 作ったあと、その機械が作られるか止まるまで手で回す dispatcher を回し（`run_one` を繰り返す。実行待ちの列で前にある仕事も回る）、そのあとで今と同じ検査をする。段 7b で CallingThreadDispatcher を入れたら、それに置き換える |

`ActorContext::stop` の rustdoc のうち「The port records the request and the executor carries it out after the actor returns.」
は、「`Terminate` を対象の列に積む」に直す。`ActorContext::stop_children` の rustdoc（「Completes child stops before …」）も直す。
上流と保存済みのデータへの影響は無い。

#### 11.12.11 不変条件と Quint

**`mailbox_scheduling.qnt`**：遷移は変えない。1 つのアクターの `Terminate`（子のいない場合）は、前から `OSystem(Terminate)` の
system message として、実行の中で閉じる形で書いてある。§11.11.7 は「停止が `Terminate` の system message になったら
`closeOutside` を消す」としたが、`closeOutside` はシステムの終了（`stop_all`）と fatal のあとの終了も表しているので残し、意味を
「システムの終了だけ」に狭める。直したのは注釈だけである（冒頭の段 3 の節、`Outside`、`closeOutside`、`outside`）。負の対照の
`CloseDuringRun` と `KeepQueuedRun` も、システムの終了について、そのまま意味を持つ。

**`actor_termination.qnt`**（§5.4 の案を、この段で書いた）：木の形のアクター（ガーディアン ROOT の下に p、p の下に c1 と c2、
c1 の下に g）を、名前ごとの枠で表す。枠は、名前が空けば次の世代が使う。p は c1 を watch し、外の watcher がすべてを watch
する。各アクターは、system message の列（`Create`、`Terminate`、`Resume`、`ChildTerm`）、通常のメッセージの列、suspend の数、
子の表（名前）、待つ子と理由、判断を待つ失敗を持つ。1 手は、1 つのアクターの実行が system message を 1 件か通常のメッセージを
1 件処理すること、外からの生成・送信・停止、実行器の判断、finishTerminate の 2 つ目の通知のどれかである。別のアクターの手は
自由に混ざる（プールのスレッドに当たる）。1 つのメールボックスの予約と、system message を通常のメッセージより先に処理する
ことは `mailbox_scheduling.qnt` の受け持ちで、ここでは「system message の列が空で、suspend されていないときだけ通常の
メッセージを取る」とだけ書く。

| 動作 | 実装で当たるところ |
|:---|:---|
| `spawn` | 生成の手順（11.12.2）。名前が空いていて、親が停止中でないときだけ |
| `send` | `tell` |
| `stop` | 停止を頼む口（11.12.3）。`Terminate` を積む |
| `runSys` | 実行が system message を 1 件処理する。`Create`、`Terminate`、`Resume`、`ChildTerm` の処理は 11.12.2〜11.12.5 |
| `receive` | 実行が通常のメッセージを 1 件処理する。catcher の下で失敗しうる |
| `decide` | 実行のあとの実行器の判断（11.12.2 の開始の失敗、段 3 の `apply_failure`、11.12.5 の再起動、11.12.6 の数え方）。親が suspend されている間は取っておく（11.12.6 の「親が suspend されている間の子の失敗」） |
| `tellRest` | finishTerminate の 2 つ目の通知。親への `ChildTerminated` と watcher への `Terminated` の間に、ほかのスレッドが親を回せることを表す |

| 不変条件 | 中身 | 何を表すか |
|:---|:---|:---|
| `noCloseBeforeChildren` | 閉じた（`post_stop` を回した）アクターは子を持たない | T1、T2 |
| `noLostChildTerminated` | 子の `ChildTerminated` が閉じた親に届かない | T1、I12 |
| `namesHeldWhileLive`、`heldNamesHaveActors` | 生きているアクターは親の表に名前を持ち、表の名前にはアクターがいる | 子の名前 |
| `noticesAfterTheEnd` | watcher が `Terminated` を受けるのは、そのアクターが閉じたあと | T3、T4 |
| `noticesOnce`、`postStopOnce` | どの世代の終わりも、watcher への `Terminated` と親への `ChildTerminated`（ガーディアンの下では名前を空けること）はそれぞれ 1 回まで。`post_stop` も世代ごとに 1 回まで（通知は世代ごとの数で数える） | T2、T4、二重の停止 |
| `parentLearnsBeforeWatchers` | 親でもある watcher は、子の `ChildTerminated` を、その子の `Terminated` より先に処理する | T4 |
| `noTerminatedToAStoppingWatcher` | 停止中のアクターに `Terminated` を送らない | T1 の unwatch |
| `createBeforeTerminate` | 閉じたアクターは、その前に `Create` を処理している | `Create` が `Terminate` より先 |
| `noReceiveWithoutInstance`、`failedStartStaysSuspended` | インスタンスの無いアクターに通常のメッセージを渡さない。開始に失敗したアクターは、やり直しの `Create` まで suspend のまま | 11.12.2、11.12.6 |
| `oneRetryPerDecision` | 開始の失敗を作り直すのは、その失敗への判断が済んだあとだけ（判断 1 回に作り直し 1 回） | 11.12.2 のやり直しの `Create` が 2 通並ぶとき |
| `noReceiveWhileWaiting` | 子を待つ間は通常のメッセージを処理しない | T1、11.12.5 |
| `retryWithoutTheAttemptsChildren` | 開始のやり直しは、前の試みの子がいなくなってから作る | 11.12.2 |
| `restartAfterStoppedChildren` | `post_restart` は、`pre_restart` で止めた子がいなくなってから | 11.12.5 |
| `recoveryNotNested` | 再起動が始まる時点で、同じアクターの再起動が進行中でない（進行中の再起動の数 `restarting` から求めた深さが 1 を超えない） | 11.12.7 |
| `waitsEnd` | 何も進行中でない（列が空、判断を待つ失敗が無い、通知が残っていない）とき、子を待つアクターはいない | 待ちが必ず終わる |
| `settledMeansResumed` | 何も進行中でないとき、インスタンスのある（または作られる前の）アクターの suspend は 0 である | 11.12.6 の「失敗 1 回に suspend 1 回、解くのは 1 回」。段 3 の I14 を木に広げたもの |

負の対照（`initWith(<誤り>)` から始める `run` が、毎回の `quint test` で確かめる）：

| モード | 誤り | 破れる不変条件 | 確かめる `run` |
|:---|:---|:---|:---|
| `CloseWithChildren` | `Terminate` が子を待たずに閉じる | `noCloseBeforeChildren`、`noLostChildTerminated` | `closeWithChildrenLosesTheChildsNoticeTest` |
| `NotifyBeforePostStop` | `Terminate` が届いた時点で watcher に知らせる | `noticesAfterTheEnd` | `notifyBeforePostStopTellsTheWatcherTooEarlyTest` |
| `WatchersFirst` | finishTerminate が watcher に先に知らせる | `parentLearnsBeforeWatchers` | `watchersFirstLetsTheParentSeeTerminatedFirstTest` |
| `EarlyNameRelease` | `Terminate` が届いた時点で名前を空ける | `namesHeldWhileLive` | `earlyNameReleaseFreesTheNameOfALiveActorTest` |
| `SyncStop` | 実行器が実行の外で止める（段 3 の形）。`Create` の前に閉じる | `createBeforeTerminate` | `syncStopClosesBeforeTheCreateTest` |
| `KeepWatching` | 停止中も watch を外さない | `noTerminatedToAStoppingWatcher` | `keepWatchingSendsTerminatedToAStoppingActorTest` |
| `ResumeAfterStartFailure` | 開始の失敗の判断のあとにも `Resume` を送る（段 3 の `resume_if_open` のまま） | `failedStartStaysSuspended`、`noReceiveWithoutInstance` | `resumeAfterStartFailureWakesAnActorWithoutInstanceTest` |
| `ResumeOnRestart` | 再起動の判断がすぐ `Resume` を送る（段 3 の形） | `noReceiveWhileWaiting` | `resumeOnRestartReceivesWhileWaitingTest` |
| `NoCreationWait` | 開始のやり直しが前の試みの子を待たない | `retryWithoutTheAttemptsChildren` | `noCreationWaitKeepsTheAttemptsChildrenTest` |
| `NoRecreationWait` | `post_restart` が止めた子を待たない | `restartAfterStoppedChildren` | `noRecreationWaitRestartsBeforeTheChildrenGoTest` |
| `CreateInsideRestart` | 親の再起動の後半が子の開始をその場でやり直し、やり直しが失敗すると、その判断まで再起動が進行中のまま残る（#251 の試作の形。11.12.7） | `recoveryNotNested` | `createInsideRestartNestsTheRecoveryTest` |
| `NoTerminateToChildren` | `Terminate` が子に停止を送らずに待つ | `waitsEnd` | `noTerminateToChildrenWaitsForeverTest` |
| `RetryUndecided` | 判断のまだの開始の失敗に届いた `Create` も作り直す | `oneRetryPerDecision` | `retryUndecidedRetriesTwiceForOneDecisionTest` |
| `NotifyTwice` | finishTerminate が親と watcher にその場で知らせ、2 つ目の通知でも watcher に知らせる | `noticesOnce` | `notifyTwiceTellsTheWatcherTwiceTest` |
| `KeepSuspended` | 再起動の後半が失敗の suspend を解かない | `settledMeansResumed` | `keepSuspendedLeavesARestartedActorSuspendedTest` |

正しい形の `run` は 9 本で（`aStopBehindTheCreateRunsAfterPreStartTest`、`aParentClosesInTheRunOfItsLastChildsNoticeTest`、
`theNameStaysTakenUntilTheParentLearnsTest`、`aFailedStartThatIsStoppedRunsNoPostStopTest`、
`aRetriedStartStopsTheAttemptsChildrenFirstTest`、`aRestartWaitsForTheChildrenItStoppedTest`、
`anEscalatedStartFailureIsRetriedAfterTheParentRestartTest`、`twoQueuedCreatesRetryOncePerDecisionTest`、
`aWatchingParentLearnsOfItsChildFirstTest`）、`run` は全部で 24 本である。

仕様の単純化（仕様の外に置いたもの）：

- 生成は、親が子を待っている間には行わない（原典は `Creation` と `Recreation` の待ちの間も子を作れる）。名前の枠の再利用は、
  古い世代の 2 つの通知が済んでから行う（実際には新しいセルなので、古い世代の通知は失われない）
- `pre_restart` は、子をすべて止めるか、1 つも止めないかを選ぶ（部分集合にはしない）
- 監督の判断は 3 つ（Restart、Stop、decider の panic）で、all-for-one と `Kill` は入れない（どちらも 11.12.5 と 11.12.6 の同じ口を
  通る）
- DeathWatch は「p が c1 を watch」と「外の watcher がすべてを watch」に固定する（watch と unwatch の操作は段 4c の仕様）
- T5（`on_termination` と `stop_and_reply` の返事が通知のあと）は仕様に入れない。§5.4 の案（`actor_termination.qnt` は T1〜T5）
  との差で、11.12.12 の単体テストで確かめる
- `sysDead` と `deadLettered` は数えるだけで、不変条件は読まない（11.12.3 の「閉じるときの dead letters」）
- 判断の取り置き（11.12.6）は、`decide` の前提（親が閉じているか、親の suspend が 0）としてだけ表し、取り置きの列そのものは状態に
  持たない。届いた順の解放と、捨てる条件（親が閉じた、子が親の表に無い）の誤りは仕様では捕まえられず、11.12.12 の単体テストが担う
- 「p が c1 を watch」の watcher は名前（`WATCHER`）で選び、世代を見ない。c1 の finishTerminate の 2 つ目の通知（`tellRest`）が
  残る間に、p が閉じて名前の枠が次の世代に使われると、仕様では watch していない新しい p に `Terminated` が届く。実装は通知を
  `ActorRef`（id と世代）に送るので、この誤配は起きない。仕様の不変条件はこの筋書きを読まない（PR #291 の Devin のレビューの指摘）

#### 11.12.12 テスト

| 種類 | 中身 |
|:---|:---|
| #251 の一覧の段 4a の 13 件 | 一覧の直し方で、red の工程のテストだけのコミットに分けて直す（triage への差し戻し。原典より弱めない）。多くは、`actor_of` の直後に `pre_start` の結果を読む前に回す形になる |
| 回復の重なりの深さの 2 件 | 書き換えずに通す（11.12.7） |
| 段 3 の橋渡しを確かめた単体テストのうち、4a で意味が変わるもの | `a_stop_asked_for_in_receive_ends_the_run_and_takes_effect_before_the_next_message`（`context.stop` で `Yield` し、次のメッセージの前に相手が止まっている。4a では `Yield` せず、相手は自分の実行で止まる。原典の停止は非同期で、次のメッセージの時点で止まっていることは約束しない）、`stopping_an_actor_takes_the_run_that_waits_for_it`（停止が待っている `Run` を取り除く。4a では、その `Run` が `Terminate` を処理する）、`a_panic_in_pre_start_without_a_catcher_terminates_on_the_next_call`（11.12.10）。どれも fraktor が段 3 で足したテストで、§11.11.2 が「段 4 で消す」とした `Yield` と同期の停止を確かめていた。red の工程のテストだけのコミットで、4a の形を確かめるテストに直す（直した形は実装の PR に載せる） |
| 段 2・段 3 の単体テストの `Suspend` | 11.12.9 の表 |
| 一覧に無いテストが停止の非同期で落ちたとき | #251 の洗い出しの試作は、停止を同期のまま残し、止める対象の `Create` を先に済ませる形だった（#251 の一覧の「試作の入れ方」）。そのため、停止を `Terminate` にしたことで落ちるテスト（`stop` の直後に止まったことを読む、止めた直後に同じ名前で作る、既定の `pre_restart` の子の停止の順に頼る、など）は、一覧に載っていない。設計の時点で読んで見つけた例は、`a_restart_of_one_child_keeps_the_other_child_stop_waiting` である（`pre_restart` で止めた兄弟が `post_restart` の時点でまだ生きていることを確かめる。4a では、兄弟の `Terminate` は、親の再起動が子を待つ間に兄弟の実行で処理されうる。原典でもこの順は約束しない）。実装の最初の工程として、#251 と同じ作りの洗い出しを行い、中将の判定を受けてから red に入る。テストは書き換えずに上げる（§11.11.1 と同じ）。洗い出しの決まりは次の 3 つである（設計の独立レビューの答え）。(1) 写しは、生成と停止の両方を system message にした形（`Create` と `Terminate` の非同期を両方入れた形）で作る。#251 の試作のように、どちらかを同期のまま残さない。(2) その写しで、テストスイートの全体（testkit を含む `cargo test --workspace`）を流し、一覧に無い落ちを、原典の振る舞いと判定つきで issue に載せる。(3) 落ちずに通るが意味が弱まるテスト（`run_until_idle` がすべてを回すので、生成と停止の順の違いを見ないテストなど）は、流すだけでは拾えない。生成と停止に触れるテストを目で読む分を残し、読んだ範囲と見つけたものも issue に載せる |
| 新しい単体テスト | 生成：`actor_of` の直後は `pre_start` が走っていない／名前は生成の時点で予約される（同じ名前の 2 つ目は断られる）／作ってすぐ止めた子は `pre_start`、`post_stop` の順に走る。停止：子のいる親は子の `ChildTerminated` まで開いていて、待つ間は通常のメッセージを処理しない（T1）／`post_stop` は子の `post_stop` のあと（T2）／watcher が `Terminated` を受けた時点でメールボックスが閉じている（T3）／親でもある watcher は、子の名前が空いたあとで `Terminated` を受ける（T4）／`on_termination` と返事は通知のあと（T5）／止まるアクターが watch していた相手の `Terminated` が dead letters に出ない（T1 の unwatch）／`Terminate` で閉じた実行がセルを戻さず、ロックの外で捨てる（L3。`Drop` で別のアクターへ送る振る舞いで確かめる）／2 度目の `Terminate` は何も起こさない。再起動：待つ間は通常のメッセージを処理しない／all-for-one の兄弟が待つ間も同じ。suspend：11.12.6 の表の行ごと（Stop の判断のあと、後ろのメッセージが `receive` に届かない、など）。開始の失敗：前の試みの子が止まってから作り直す（既存の `a_retried_start_does_not_keep_the_children_of_the_failed_attempt` も確かめる）／開始の失敗と同じ実行で止まった子にも判断が当たり、decider の panic が親を失敗させる／開始のやり直しの `Create` が 2 通並んだとき（子の Restart の判断と、親の再起動の後半）、作り直しは判断 1 回につき 1 回だけで、1 通目がまた失敗したら 2 回目の失敗も判断される（`pre_start` の回数、記録された失敗と判断の回数、再起動の回数で確かめる。11.12.2）。判断の取り置き：親が子を待つ間（`Recreation`）に失敗した子の判断は、親の再起動の後半のあとに当たり、decider の panic が親の再起動の中で親をもう一度再起動しない（11.12.6）／子の失敗が取っておかれ、親の再起動の `pre_restart` がその子を止め、親が `ChildTerminated` を処理して表から外したあと、解放された判断は捨てられ、decider は呼ばれない（decider が panic する戦略でも親は失敗しない。11.12.6） |
| PBT（P5） | `mailbox_order_property_test.rs` は、停止を含むまま通す。停止が非同期になっても、送ったものは 1 回だけ届くか dead letters へ行く（I8） |
| 速さ | §7.5 の S1〜S4 を、A＝段に入る直前の main（`74d8872`）、A0＝`7ecb3f5` と比べて報告する。1 件ごとの送受信の経路は変えないので、揺れの中に収まる見込みである（生成と停止の経路は 1 件ごとには走らない）。止める基準ではない |

#### 11.12.13 壊しうる入力と影響

| 区分 | 入力 | 期待 |
|:---|:---|:---|
| 生成 | 作ってすぐ止める（`Create` の後ろに `Terminate`） | 列の順で `Create` が先。`pre_start` のあと `post_stop`。開始に失敗していれば `post_stop` は走らない |
| 生成 | 作ってすぐ送る | 通常のメッセージは `Create` のあと |
| 生成 | `pre_start` の中で子を作り、そのあと失敗する | 子は親の表にいる。やり直しの前に止まり、名前が空いてから作り直す |
| 生成 | まだ作られていない親の下に、外から子を作る（`WeakActorSystem::spawn_child`） | 親の表を作って名前を予約する。親の `Create` が戦略を書き換える |
| 停止 | 子のいる親を止め、待つ間に親へ送る | 親は suspend され、送ったものは閉じるときに dead letters |
| 停止 | 停止中の親に子を作る | 今と同じく断る（`InvalidActorNameException::terminating`） |
| 停止 | 止まったアクターをもう一度止める | 何も起こらない |
| 停止 | 子の終了のフックが、停止中の親で子を作ろうとする | 今と同じく断られ、`IllegalStateException` を 1 回 publish |
| 名前 | 止めた直後に同じ名前で作る | 親が `ChildTerminated` を処理するまで断られる（原典と同じ） |
| 再起動 | 既定の `pre_restart`（子をすべて止める）で再起動する | 子が止まってから `post_restart`。待つ間は suspend |
| 再起動 | all-for-one で、失敗していない兄弟が子を待つ | 兄弟は前半で suspend され、後半で解かれる |
| 失敗 | 開始の失敗、`receive` の失敗、`Kill` と、各判断 | 11.12.6 の表。インスタンスの無いアクターには通常のメッセージが届かない |
| panic | catcher の無い `pre_start`、`post_stop`、終了のフックの panic | 段 3 の巻き戻り。次の呼び出しでシステムが終わる |
| 終了 | 子を待つ途中でシステムを終える | `stop_all` が今と同じくすべてを閉じる（`post_stop` は走らない。T6 は段 6） |
| function ref | 子を持つ function ref を止める | 子が止まってから watcher へ通知 |

#### 11.12.14 動かしたコマンドと結果（設計の時点、2026-11-27、quint 0.32.0）

| 検査 | 結果 |
|:---|:---|
| `scripts/quint-check-specs.sh`（CI と同じ。typecheck、各仕様の `run`、`quint run` 20 手・1 万本） | exit 0。`actor_termination.qnt` は 20 passing、`mailbox_scheduling.qnt` は 38 passing（遷移は変えていない） |
| `actor_termination.qnt` の `quint run --invariant=invariant`（正しい形、rust の backend、20 手・1 万本、3 回） | 3 回とも違反なし（trace の長さの最大 21、平均 21.0） |
| 同（40 手・5 万本、3 回） | 3 回とも違反なし（最大 41、平均 31.9。予算を使い切って止まる trace が多い） |
| 同（`--backend=typescript`、60 手・5,000 本） | 違反なし（最大 42、平均 31.7。seed `0x18a49f1e90a7d6`） |
| 12 の誤りのモードから始める `quint run`（20 手・1 万本、モードごとに 3 回。`action init` を `initWith(<モード>)` に書き換えた写し） | 12 のモードとも、3 回とも違反を見つけた |
| 探索が届く状態（正しい形、20 手・1 万本。その状態を否定した式を不変条件にして流す） | `Recreation` の待ち、`Creation` の待ち、p と c1 が同時に `Termination` で待つ、名前の 2 世代目、判断の前に閉じた開始の失敗、p が `Recreation` で待つ間に c1 が開始に失敗している、の 6 つとも見つけた |
| `uv run python3 tools/license-headers/license_headers.py --check --manifest docs/design/issue-206-file-origin.tsv` | errors 0（新しい仕様に Apache-2.0 の見出しを付けた） |
| ほかの CI と同じ検査（`cargo fmt --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`、ledger と tools の Python のテストと Pekko baseline の検査、thumbv7em の `cargo check`、syn-check、`npm run check:aislop`） | すべて exit 0。`cargo test --workspace` は 1389 件が通過、失敗 0（Rust のコードは変えていない）。syn-check は違反 0 |

設計の独立レビューを受けて直したあと（2026-11-27、quint 0.32.0。11.12.16）：

| 検査 | 結果 |
|:---|:---|
| `scripts/quint-check-specs.sh` | exit 0。`actor_termination.qnt` は 24 passing（正しい形 9 本、負の対照 15 本）、ほかの仕様は変えていない |
| `actor_termination.qnt` の `quint run --invariant=invariant`（正しい形、rust の backend、20 手・1 万本、3 回） | 3 回とも違反なし（trace の長さの最大 21、平均 20.99） |
| 同（40 手・5 万本、3 回） | 3 回とも違反なし（最大 41、平均 33.8） |
| 同（`--backend=typescript`、60 手・5,000 本） | 違反なし（最大 42、平均 33.6。seed `0x10f8605643e8a0`） |
| 15 の誤りのモードから始める `quint run`（20 手・1 万本、モードごとに 3 回） | 14 のモードは 3 回とも違反を見つけた。`CreateInsideRestart` は `recoveryNotNested`、`NotifyTwice` は `noticesOnce`、`KeepSuspended` は `settledMeansResumed` を、それぞれその不変条件だけを渡した `quint run` でも破った（レビューが写しで試した 2 つの変異は、`NotifyTwice` と `KeepSuspended` と同じ形で、どちらも乱数の探索で落ちるようになった）。`RetryUndecided` は、20 手・1 万本では種に左右され、9 回中 3 回だけ見つけた（2 通目の `Create` が並び、1 通目がもう一度失敗し、判断より先に 2 通目を処理する、の 3 つが揃う必要がある）。30 手・5 万本では 6 回とも、40 手・20 万本でも見つけた。CI が毎回確かめるのは、決まった筋書きの `retryUndecidedRetriesTwiceForOneDecisionTest` である |
| 探索が届く状態（正しい形、20 手・1 万本。その状態を否定した式を不変条件にして流す） | 同じ子の列に `Create` が 2 通並ぶ状態、親が suspend されている間に子の判断が取っておかれている状態、親が `Recreation` で待つ間に子の判断が取っておかれている状態、再起動が進行中の状態、閉じて `post_stop` を 1 回回した状態は見つけた。判断のまだの開始の失敗の後ろに `Create` が残る状態（高 1 の 2 通目が届く時点）は、2 回のうち 1 回見つけた |
| `npm run check:aislop`、`license_headers.py --check` | どちらも exit 0。Rust のコードは変えていない |

#### 11.12.15 選ばなかった形

- **停止を同期のまま残し、止める対象の `Create` を先に済ませる**（#251 の試作の形）：#251 の 13 件だけで済むが、T1 の「子が
  揃うまで待つ」が無く、§6 の 4a の行（Terminate、T1〜T5）を満たさない
- **開始のやり直しと親の再起動の後半で、`create_initial` を同期に回す**：#251 の試作で深さ 2 になった形（11.12.7）
- **開始の失敗のあとも段 3 と同じく `Resume` を送り、`Create` は suspend に触らない**：判断が親の停止になると、子はインスタンスの
  無いまま、T1 の `Terminate` が届くまで suspend が 0 になり、通常のメッセージが届く。やり直しの `Create` が解く形（原典の
  `finishCreate`）にした
- **待つ間の system message を取っておく仕組み（`StashWhenWaitingForChildren`）を 4a で入れる**：4a では、待つ間に `Resume` などが
  届く経路が無い（11.12.5）。入れても届かない枝になり、確かめられない。`Suspend` と `Recreate` を送る 4b で入れる
- **`SystemMessage` を 4b か 4c まで `pub` のままにする**：#253 の完了条件（段 3 のレビューの 1）に反する。外して足し直す形に
  した（11.12.9）
- **閉じたメールボックスの system message を、原典どおり `DeadLetter` として publish する**：11.12.3 の理由で、`Watch` を送る 4c で
  決める
- **ワイルドカードの選択を 4a で入れる**：11.12.9 の理由で 4c

#### 11.12.16 設計の独立レビューの指摘の処理（2026-11-27 のレビュー、`d436bea` に対して）

レビューの判定は「要修正（高 1、中 3、低 4）」で、全体の形（`Create` と `Terminate` を system message にすること、名前の予約と
解放の時点、T1〜T5 の順、`resume_if_open` の改め方、深さ 1 の保ち方、`SystemMessage` の 4 種類、ワイルドカードの 4c 行き）は
原典と合うとされた。指摘ごとの処理は次のとおりである。

| 指摘 | 判断 | 処理 |
|:---|:---|:---|
| 高 1：開始のやり直しの `Create` が 2 通並ぶと、判断を経ない作り直しと開始の失敗の記録の喪失が同じ実行の中で起きる | 直す（候補 b） | `Create` の処理に「開始に失敗していて、その失敗への判断がまだ」の行を足し、何もしない形にした（11.12.2 の表と「やり直しの `Create` が 2 通並ぶとき」）。判断を当てたら、セルの記録に「判断が済んだ」を付ける。候補 a と c を選ばなかった理由も同じ節に書いた。レビューの「原典ではこの形は起きない」は訂正して記した。原典でも `Recreate` が 2 通並べば 2 通目は作り直す（`Recreate` は `StashWhenFailed` でない）が、原典は失敗ごとに `Failed` が届き、親の `Suspend` で数が合うので壊れない。4a は記録が 1 枠で `Suspend` も送らないので、2 通目を捨てる。Quint に `oneRetryPerDecision`、負の対照 `RetryUndecided`、正しい形の `twoQueuedCreatesRetryOncePerDecisionTest` を足し、この筋書きに要る 3 回の失敗のため `FAILS` を 2 から 3 にした。11.12.12 の新しい単体テストに 1 件足した |
| 中 1：`recoveryNotNested` は状態から導かれない幽霊の印で、負の対照 `CreateInsideRestart` が同語反復 | 直す | 各アクターに進行中の再起動の数 `restarting` を持たせ（前半の始めで増え、後半の終わりで減る）、再起動が始まる時点の深さの最大を `deepestRestart` に残す。`recoveryNotNested` は深さが 1 を超えないこととした。`CreateInsideRestart` は、後半が子の作り直しをその場で行い、失敗すると判断まで再起動が進行中のまま残る遷移にした（11.12.7）。印の `nested` と、`decide` の引数で印を立てる分岐は消した。直す中で、直す前の仕様が decider の panic の判断だけを親が子を待つ間は起きないとして外していたことに気づき、判断のすべてを親の suspend が解けるまで取っておく形を設計に足した（下の最後の行） |
| 中 2：`notices` が集合で、終了の通知の二重を数えられない | 直す | `notices`（watcher への `Terminated`）と `parentNotices`（親への `ChildTerminated`、ガーディアンの下では名前を空けること）を世代ごとの数にし、`post_stop` も世代ごとの回数（`postStops`）にした。不変条件 `noticesOnce` と `postStopOnce` を足し、負の対照 `NotifyTwice`（レビューが写しで試した変異と同じ形）を足した |
| 中 3：suspend の残りを捕まえる不変条件が無い | 直す | `settledMeansResumed`（何も進行中でないとき、インスタンスのある、または作られる前のアクターの suspend は 0）を足した。正しい形で成り立つことを `quint run` で確かめてから足した（11.12.14）。負の対照 `KeepSuspended`（レビューが写しで試した「後半が suspend を解かない」変異と同じ形）を足した |
| 低 1：名前の再利用の記述が、メールボックスの無い親の場合と合わない | 直す | 11.12.10 の行をメールボックスのある親とガーディアンの下に分け、11.12.4 の「遅くはならない」の範囲を限った |
| 低 2：`ChildTerminated` で親の表から外すときの照合が書かれていない | 直す | 11.12.4 の 1 に、`ActorRef`（id と世代）が表の登録と一致するときだけ外し、一致しない通知は表を変えないことを書いた |
| 低 3：`sysDead` と `deadLettered` は数えるだけ。T5 は仕様の外 | 直す | 11.12.3 の文を「数えるだけ。保存の主張は `mailbox_scheduling.qnt` の `systemMessagesConserved`」に直し、11.12.11 の仕様の単純化に T5 と数えるだけの 2 つを書いた |
| 低 4：`Terminate` の処理の「停止中なら何もしない」が誤読できる | 直す | 11.12.3 の 1 を、理由が `Termination` のときだけ何もせず、ほかの理由なら `Termination` に替えて進む形に書いた |
| 中将の点 (1)：停止を非同期にした分の洗い出しを実装の最初の工程にする進め方 | 直す（設計に書く） | 11.12.12 の「一覧に無いテスト」の行に、写しは生成と停止の両方を system message にした形で作ること、テストスイートの全体（testkit を含む）を流すこと、落ちずに意味が弱まるテストを目で読む分を残すことを書いた |
| 中将の点 (2)：`Suspend` を外すと形が変わる単体テスト 3 件 | 直す（記録する） | 11.12.9 の表のとおり triage として扱う。レビューの答えと 2 つの条件（テストだけのコミット、実装の PR への記録）を 11.12.9 に書いた |
| （レビューの指摘ではない）親が suspend されている間の子の失敗 | 足す（要レビュー） | 中 1 を直す中で見つけた。4a では親が suspend されたまま複数の実行にまたがる（自分の失敗の判断待ち、開始の失敗、子を待つ間）。その間に子の判断を当てると、decider の panic が子を待つ親やインスタンスの無い親を失敗させ、深さ 1 が崩れる。原典の `Failed` の `StashWhenFailed` と `StashWhenWaitingForChildren` に合わせ、判断を親の `ParentEntry` に取っておき、親の suspend が 0 に戻ったら届いた順に判断し、親が閉じたら捨てる形を 11.12.6 に足した。Quint では `decide` の前提で表す。この行は、次の設計レビューで見てもらう |

#### 11.12.17 設計の再レビューの指摘の処理（2026-11-27 のレビュー、`6a541e8` に対して）

再レビューは、前回の指摘（高 1、中 3、低 4）がすべて直っていることを確かめ、11.12.6 に足した「親が suspend されている間の子の
失敗」の規則も原典と合うとした。残る指摘は次の 2 件で、どちらも直した。

| 指摘 | 判断 | 処理 |
|:---|:---|:---|
| 中 1：取っておいた判断を当てるとき、子が既に親の表から外れている場合の扱いが書かれていない（原典 `FaultHandling.handleFailure` は `getChildByRef` と UID の照合で、`preRestart` で止めた子からの `Failed` を捨てる） | 直す | 11.12.6 に、取っておいた判断を解放するのは子が `ActorRef` の id と世代で今も親の表にいるときだけで、表に無い子の失敗は判断せずに捨てる（decider を呼ばない）ことを書いた。11.12.12 の「判断の取り置き」の単体テストに、取っておかれた子の失敗が、親の再起動の `pre_restart` で子が止められて表から外れたあとに捨てられ、decider が呼ばれない筋書きを 1 件足した |
| 低 1：仕様は取り置きを `decide` の前提としてだけ表し、解放の順と捨てる条件は状態に無い | 直す | 11.12.11 の仕様の単純化に 1 行書いた。その範囲は 11.12.12 の単体テストが担う |

#### 11.12.18 段 4a の実装で決めたこと

承認済みの設計に書いていない形を、実装で次のとおり決めた（段 3 の 11.11.13 の前例）。

| 何を | 決めた形 | 理由 |
|:---|:---|:---|
| 待つ子の持ち方 | `ParentEntry` は、11.12.4 の `Waiting { to_die, reason }` でなく、停止を頼んだ子の一覧 `dying`（原典の `shallDie`）と、待つ理由 `waiting` を別に持つ。表の子に `Terminate` を送るたびに `dying` に入れ、`ChildTerminated` で外す。待ちが終わるのは `dying` が空になったとき | 原典の `Children.stop` は、どの経路で頼んだ停止でも表の子を `shallDie` に入れ、`faultRecreate` は `TerminatingChildrenContainer` の `toDie` が空になるまで待つ（`FaultHandling.scala:114`）。`pre_restart` で止めた子だけを数える形だと、失敗の前に止めた子の `ChildTerminated` を待たずに `post_restart` が走る |
| 停止中の印 | 子のいない `Terminate` も `waiting` を `Termination` にしてから finishTerminate へ進む。段 3 の `stopping: bool` はこれに替えた | `post_stop` の中の子の生成を、今と同じく断るため（`post_stop_cannot_create_an_explicitly_supervised_child`） |
| decider の panic のあとの子の再開 | 11.12.6 の表の「親が再起動されて残ったときだけ、実行器が `Resume` を送る」は、親への判断のすぐあとでなく、親の再起動の後半の終わりに送る形にした。親の `ParentEntry` に `resume_after_restart` として子を記録し、後半が、まだ表にいる子に `Resume` を送る。親が止まれば記録はエントリと一緒に消え、子は T1 で止まる | 判断のすぐあとに送ると、親の再起動が子を待つ（`Recreation`）間に `Resume` が届く。原典で子が再開するのは親の `finishRecreate` のあとである。親の判断がさらに取っておかれた場合も、同じ記録で済む |
| decider が panic した親の suspend | 親を失敗させる時点で、親のメールボックスを 1 つ suspend する（原典の `handleInvokeFailure` の `suspendNonRecursive`）。判断の効果の終わりで解くのは、ほかの失敗と同じ（11.12.6 の表） | 「失敗 1 回に suspend 1 回」を decider の panic にも当てる。11.12.5 の「前半の始めに実行器が suspend する」は、この親については失敗の時点に前へ出した。all-for-one の兄弟と `Kill` の生き残りの子は、前半の始めに suspend する |
| 古い振る舞いの枠 | 再起動の前半が終わっても、古いインスタンスの振る舞いの枠をセルに戻しておき、後半の終わりで捨てる | 段 3 と同じく、枠は `post_restart` のあとまで生きる。回復の重なりの深さを数えるテスト（`RecoveryScope`）は、この寿命を前提にしている |
| 失敗の記録 | `Failure` に、子、親、種類、原因、送り手、失敗した時点の再起動の回数に加えて、子の戦略と、それを親から受け継ぐかを入れる | 11.12.2 の「判断の時点で子が止まっていたとき」は、セルから何も読めない。受け継がない戦略も記録しておかないと判断できない |
| 取り置きの解放の見回り | 取り置いた失敗の数を数えておき、0 でないときだけ、実行のあとに親の表を見回る | 実行のたびに全アクターを見回ると、1 件ごとの経路が重くなる。数えた上での見回りなら、取り置きの無いときの費用は比較 1 回である（11.12.19 の S1〜S7） |
| `PoisonPill` と `stop_and_reply` | `PoisonPill` は、実行が自分のメールボックスの system message の列に直接 `Terminate` を積む（予約はしない。実行の終わりの予約が拾う）。`stop_and_reply_after_termination` は、返事を記録して `Executor::stop` で自分に `Terminate` を送る。同じメッセージで `receive` が失敗しても、返事が記録されていれば判断を当てない | 11.12.3 の表のとおりの効果を、`AfterRun` を使わずに出す |
| メールボックスの読み口 | `Mailbox::is_open_and_suspended` を足した | 取り置きの判定に要る。テストの補助に同じ名前の `is_suspended`（閉じていても数だけを見る）があるので、名前を分けた |
| 名前の無いトップレベルのアクター | `$n` の名前でも `user_children` に入れ、止まったら外す | 11.12.2 の 2 のとおり。名前の重なりの検査は、名前を渡した生成だけが読む |
| `Kill` の生き残りの子 | 後半が同期に再起動する子から、`dying` にいる子、子を待っている子（`is_waiting`）、まだ作られていない子（`is_created` が偽。最初の `Create` の前か、開始に失敗した子）を外す | 段 3 は `pending_stops` にいる子を外していた。`pending_stops` は消えたので、同じ意味の `dying` を読む。子を待っている子と作られていない子を外すのは、この表の「停止中か、子を待っているアクターの再起動」と「all-for-one のインスタンスの無い兄弟」と同じ理由である（#254 のコメントの 3 で、実装に合わせて書き足した。段 4b でこの行の振る舞いは消える。11.13.9） |
| 停止中か、子を待っているアクターの再起動 | 判断が Restart でも、停止中のアクター（`Terminate` を処理し、子を待つ）は再起動しない。all-for-one では、子を待っている兄弟（停止、開始のやり直し、進行中の再起動のどれでも）を飛ばす。`Kill` の生き残りの子も同じ | 原典の `faultRecreate` は、停止中でないアクターだけを再起動する（`isNormal`）。子を待つ間の `Recreate` は `StashWhenWaitingForChildren` で取っておかれ、待ちのあとに処理される。4a は取り置く仕組みを持たない（11.12.5）ので飛ばす。進行中の再起動の後半が新しいインスタンスを作るので、もう 1 回の再起動の分だけ原典と回数が違う（PR #291 の Devin のレビュー） |
| all-for-one のインスタンスの無い兄弟 | 最初の `Create` をまだ処理していない兄弟は、その `Create` に任せる。開始に失敗して判断が済んだ兄弟には `Create` を送る | 原典の `faultRecreate` は、インスタンスが無ければ `faultCreate` へ落とす。`post_restart` だけを回して `pre_start` を飛ばす形を避ける（PR #291 の Devin のレビュー） |
| セルの無い親の decider の panic | decider が panic した親にセルが無い（function ref、ガーディアン、止まった親）ときは、`resume_after_restart` に記録せず、子にすぐ `Resume` を送る | セルの無い親は再起動されないので、記録すると子が suspend のまま残る。段 3 と同じ振る舞いになる（PR #291 の Devin のレビュー） |
| 子を待つ function ref | 止めた function ref が子を待つ間は、watch の上では生きている（`is_live`）。待つ間の `watch` は同じ登録になり、最後の子が止まったときに 1 回だけ知らせる | 待つ間に生きていないとすると、`watch` がその場で確かめられていない `Terminated` を返し、最後の子の分と 2 回届く（PR #291 の Devin のレビュー） |
| 親の表の更新 | 子を外す、停止を頼んだ子に数える、すべての子を待つ、の 3 つは `ParentEntry` の振る舞いにした。メールボックスを持つ親と持たない親は、同じ `remove_child` を通る | 貧血モデルを避ける規則（rich-domain-model）。親の種類で判定が分かれていた（PR #291 の Devin のレビュー） |
| testkit の `TestFsmRef` | 機械の `pre_start` の終わりで、handle に「始まった」の印を付ける（`FsmHandle::mark_started`、`has_started`。印は再起動をまたいで残る）。`TestFsmRef` は、印が付くか機械が止まるまで `run_one` を繰り返す | 11.12.10 の「作られるか止まるまで回す」を、testkit の外から見える形で判定する。生成の要求の印（factory が走ったこと）だけでは、開始の失敗の判断と停止がまだ列にあることがある |

洗い出し（11.12.12 の決まり）で triage へ差し戻すと決まったテストと直し方は、実装の PR の本文の表にある（中将の判断、2026-11-27）。

#### 11.12.19 段 4a の実装の測り（2026-11-27）

§7.5 の手順（`scripts/stage1-bench.sh`、A＝段 4a の枝が取り込んだ main `526e4f2`、B＝実装のコミット `e64d581`、A0＝`7ecb3f5`）。A は、§11.12.12 の「段に入る直前の main（`74d8872`）」でなく、枝に取り込んだ main にした（#286 の dead letters の直しを含み、B との差が段 4a の分だけになる）。06:00 JST から、1 分の load average は 22〜30（機械はほかの bot と共有）。止める基準ではない。

計測のテスト（`actor_system_send_stage1_bench.rs`）は、生成のあとすぐに時計を始める。4a では生成が非同期なので、B だけ、時計の中で各アクターの `Create` も回る（S5〜S7 では 100〜1 万の `Create`）。そこで 2 通り測った。(1) テストのまま。(2) 写しのテストで、時計を始める前に `run_until_idle` を入れた形（スクリプトが B の写しを A と A0 にも写すので、3 つとも同じ形）。リポジトリのテストは変えていない。

| 場面 | (1) B/A | (1) B/A0 | (1) A/A の組の比 | (2) B/A | (2) B/A0 | (2) A/A の組の比 |
|:---|:---|:---|:---|:---|:---|:---|
| S1 | 0.989 | 0.825 | 0.998, 1.049, 0.993 | 0.994 | 0.892 | 0.965, 1.024, 0.989 |
| S2 | 0.971 | 0.937 | 1.037, 0.927, 1.101 | 0.908 | 0.984 | 0.830, 1.079, 1.024 |
| S3 | 0.870 | 1.499 | 1.064, 0.841, 1.098 | 0.874 | 1.529 | 0.895, 1.107, 1.060 |
| S4 | 0.916 | 1.456 | 1.037, 0.870, 1.051 | 0.882 | 1.696 | 0.807, 1.178, 0.913 |
| S5 | 0.950 | 0.306 | 1.025, 0.995, 1.051 | 0.925 | 0.312 | 0.929, 1.002, 0.993 |
| S6 | 0.965 | 0.042 | 1.127, 0.902, 1.070 | 0.881 | 0.044 | 1.025, 0.984, 0.997 |
| S7 | 0.927 | 0.004 | 0.956, 0.963, 1.113 | 0.865 | 0.004 | 0.926, 1.023, 0.957 |

B/A は 7 場面とも 1 を下回ったが、A/A の組の比の揺れ（0.81〜1.18）の中に収まり、速くなったとは言えない。1 件ごとの送受信の経路で段 4a が変えたのは、`invoke` が `Yield` を決めるために見る表が 3 つから 1 つになったこと、実行のあとの取り置きの見回りが数の比較 1 回であること、失敗したメッセージの送り手を消す代入 1 回である。条件 1 の数え上げ（`stage3` の S1、testkit の回し方、S4）は、生成を数える範囲の外に出すと、段 3 と同じ値（1 件ごとの Arc の複製と原子の増分）に一致した。B/A0 の S3 と S4 は段 3 の 11.11.14 と同じ理由（1 件ごとのロックの区間）で、段 4a の差ではない。

### 11.13 段 4b で決めること（[Issue #254](https://github.com/j5ik2o/fraktor-ng-rs/issues/254)）

段 4b の設計の差分である。実装の前に、実装と別の系統の設計の独立レビューを通す（§6 の冒頭）。扱うのは、§6 の段 4b の行
（Failed、Suspend、Resume、Recreate、all-for-one）、#254 の完了条件の回復の重なりの深さ 1 の 2 件、段 4a のレビューから回した
3 点（#254 のコメント：子を待つ間の `Recreate` の取り置き、all-for-one のインスタンスの無い兄弟、`Kill` の行の書き足し）、
11.12.18 で 4b に回した差である。原典は `dungeon/FaultHandling.scala`、`ActorCell.scala` の `systemInvoke`、
`dispatch/sysmsg/SystemMessage.scala`、`FaultHandling.scala`（`SupervisorStrategy`、`OneForOneStrategy`、`AllForOneStrategy`）、
`dungeon/Dispatch.scala`、`dungeon/Children.scala` である（Pekko v2.0.0）。

**元帥の決定（2026-11-27）：D4b-1 は A**。原典の `finishRecreate` は、再起動したアクターが残した子（`pre_restart` で止めなかった
子）のすべてに `Recreate` を送り、子も再起動する（`FaultHandling.scala:278-300` の `survivors`）。失敗したアクターの子は、判断が
出るまで suspend される（`handleInvokeFailure` の `suspendChildren`、`faultSuspend`）。fraktor は段 3 からどちらも行わない（残した子を
再起動するのは `Kill` のときだけで、子は suspend しない）。段 4b は、両方を原典どおりに入れる（11.13.6、11.13.7）。

これにより、#254 の完了条件で「書き換えずに通す」とした `repeated_child_recovery_failures_do_not_nest_parent_recovery`
（`executor_value_test.rs`）のうち、親が `pre_restart` で残した子 `healthy` が親の再起動のあとも作り直されないことを確かめる 2 つの
assert（`run_recovery` の `healthy_creations == 1` と、再起動のあとの値が 1 であること。`:519-520`）が、原典の振る舞いと合わなくなる。
元帥は、2026-11-26 の決定の「書き換えずに深さ 1 のまま通す」を、深さ 1 の assert に限ると読み直した。深さ 1 の assert は書き換えず、
`healthy` の 2 つの assert は、原典より強い前提（親が再起動しても残した子は作り直されない）に頼っているので、triage へ差し戻す。
テストの担当（Opus）が、原典の振る舞い（`healthy` は親の再起動のたびに作り直される）を確かめる形に直す（#254 と #251 に記録）。
深さ 1 の流れは 11.13.10 にある。

決める前に比べた形（記録として残す）：

| 選択肢 | 中身 | 結果 |
|:---|:---|:---|
| A（採った） | 原典どおり、子の suspend と残した子の再起動を入れる。`healthy` の 2 つの assert は triage へ差し戻す。深さ 1 の assert は書き換えない | 原典の意味論になる |
| B（採らなかった） | 段 4a と同じく、子を suspend せず、残した子は `Kill` のときだけ再起動する | `run_recovery` は書き換えずに通るが、原典との差（失敗したアクターの子が判断の前に通常のメッセージを処理する。残した子が再起動しない）が残り、`docs/decisions/pekko-semantics-first.md` の「作りのずれは直す」から外れる |

Quint では、A の形で不変条件 `noReceiveUnderAFailedParent` が成り立ち、子を suspend しない形（負の対照 `NoSuspendChildren`。
B の半分）では成り立たない。子を suspend して、残した子を再起動しない形（負の対照 `NoSurvivorRecreate`）では、子が suspend された
まま残る（`settledMeansResumed`。11.13.11）。2 つは対で、片方だけを入れることはできない。

**変えないこと**：DeathWatch と終了の通知の表、stash、`stop_and_reply` の返事は、実行器の表のまま（段 4c）。`MailboxCell::Env`
は `Executor` のままで、`ActorContext` の `&mut Executor` も残す（段 4 の終わりで外す）。システムの終了（`stop_all`、T6〜T8）は
今のまま（段 6）。dispatcher は手で回す 1 スレッドのものだけ（複数スレッドは段 5）。閉じたメールボックスの system message を
捨てる扱い（11.12.3）も今のまま（段 4c）。`post_restart` の失敗でアクターを止める扱い（11.12.5 の後半の失敗の行）も今のまま
（11.13.15）。

#### 11.13.1 形の全体

| 項目 | 今（段 4a、main `174052c`） | 段 4b |
|:---|:---|:---|
| 失敗の記録 | `after_run` の 1 枠に記録し、実行が返ってから実行器が判断する（`finish_run` の `decide`） | 失敗した実行の中で、原典の `handleInvokeFailure` を行う。自分を suspend し、子に `Suspend` を送り、親の列に `Failed` を積む（11.13.3）。`after_run` は消える |
| 判断 | 実行器が行う。親が suspend されていれば `ParentEntry.held` に取っておき、`release_held_failures` が見回って解く | 親の実行が `Failed` を処理するときに行う（原典 `handleFailure`。11.13.4）。取り置きは、親の system message の取り置き（11.13.8） |
| 再起動 | 実行器が `restart_cell` で前半を回し、子を待てば後半を親の実行で回す | 子の列の `Recreate` を子の実行が処理する（原典 `faultRecreate`、`finishRecreate`。11.13.6） |
| 開始のやり直し | `Create` を送る（判断の済んだ印と、まだの印で 2 通目を捨てる） | `Recreate` を送る。インスタンスが無ければ `faultCreate` に落ちる。`Create` は生成の 1 通だけになる |
| 失敗したアクターの子 | suspend しない | `Suspend` で suspend する（原典の再帰の suspend。11.13.7） |
| 残した子（生き残り） | `Kill` のときだけ、後半が同期に再起動する | どの失敗でも、後半が `Recreate` を送る（11.13.6） |
| decider の panic | `pending_decider_failures`、`fail_supervisor`、`apply_supervisor_failure`、`resume_after_restart` | 親の実行の中の、親自身の `handleInvokeFailure`（原典の Escalate と、`handleFailure` が投げた場合。11.13.4）。失敗した子は、親の再起動の後半から生き残りとして `Recreate` を受ける |
| all-for-one | `apply_all_for_one`。子を待つ兄弟を飛ばし、作られていない兄弟は最初の `Create` に任せる | 兄弟のすべてに `Suspend` と `Recreate` を送る（原典 `restartChild(suspendFirst = true)`。11.13.5） |
| 子を待つ間に届く system message | 取っておく仕組みは無い（届く経路を作らない） | 原典の `shouldStash` で取っておき、状態が下がったら列の残りより先に処理する（11.13.8） |
| suspend の数を変える場所 | 再起動の前半の始めに実行器が変える（I4 の例外。11.12.5） | 自分の実行の中だけ（I4 を満たす） |
| `Next::Yield` | 失敗、`Kill`、`pending_decider_failures` で返す | 返さない。suspend されたメールボックスは、もともと通常のメッセージを取らない（11.13.3） |
| 再起動の回数（`RestartHistory`） | 子のセルが持つ。`Failure` が失敗の時点の回数を写す | 親の表の子の記録（原典 `ChildRestartStats`）が持つ（11.13.4） |
| `SystemMessage` | 4 種類（`Create`、`Resume`、`Terminate`、`ChildTerminated`） | 7 種類（`Suspend`、`Recreate`、`Failed` を足す。`Resume` は原因の有無を持つ。11.13.2） |
| Quint | `actor_termination.qnt` の `decide`（実行器の判断） | `actor_termination.qnt` の `decide` を消し、`Failed`、`Suspend`、`Resume`、`Recreate` と取り置きを書く（11.13.11） |

消すもの：`after_run`、`finish_run` の `decide` と `release_held_failures`、`ParentEntry.held`、`held_failures`、
`take_releasable_failure`、`ParentEntry.resume_after_restart`、`apply_decision` と `restart_child` の実行器の経路、`restart_cell` の
`suspend_first`、`pending_decider_failures`、`finish_boundary`、`apply_pending_decider_failures`、`apply_supervisor_failure`、
`StartFailure` の `Undecided` と `Decided`（と `mark_start_failure_decided`）、`finish_recreate` の「`Kill` のときだけ」の分岐と
「判断の済んだ開始の失敗に `Create`」の分岐、`Failure` の `restart_count`、`own_strategy`、`strategy_inherited`、`sender`
（11.13.4）、`Mailbox::is_open_and_suspended`（取り置きの判定だけが使う）。`Next` は、`Yield` を返す口が無くなるので、
`MailboxCell::invoke` の戻りごと消すか、実装で決める（使わない枝を抑制では残さない）。

#### 11.13.2 `SystemMessage` の形

| 種類 | 中身 | 送り手 | 原典 |
|:---|:---|:---|:---|
| `Create` | なし | 生成（11.12.2） | `Create(None)` |
| `Terminate` | なし | 停止の口（11.12.3） | `Terminate()` |
| `ChildTerminated { child }` | 子の `ActorRef` | 子の finishTerminate（11.12.4） | `DeathWatchNotification` の親の分 |
| `Suspend` | なし | 失敗したアクター（子へ）、`faultSuspend`（子へ）、all-for-one（兄弟へ） | `Suspend()`（`StashWhenWaitingForChildren`） |
| `Resume { caused_by_failure: bool }` | 失敗への答えか | `faultResume`（子へ）、メールボックスの無い親の decider の panic（11.13.4） | `Resume(causedByFailure)`（`StashWhenWaitingForChildren`） |
| `Recreate { cause: RestartCause }` | 再起動の原因（今の `RestartCause`） | 判断の Restart、all-for-one、`finishRecreate`（生き残りへ） | `Recreate(cause)`（`StashWhenWaitingForChildren`） |
| `Failed(Failure)` | 失敗した子の `ActorRef`（id と世代）、種類（`FailureKind`）、原因の名前と文 | 失敗したアクター（親へ） | `Failed(child, cause, uid)`（`StashWhenFailed`、`StashWhenWaitingForChildren`） |

`Failure` は、今の構造体から、判断に要らなくなる 4 つ（`restart_count`、`own_strategy`、`strategy_inherited`、`sender`）を外した形に
する。再起動の回数と子の戦略は親の表から読み（11.13.4）、失敗したメッセージの送り手は子のセルが持つ（11.12.3 の「失敗した
メッセージの送り手」。原典の `currentMessage` と同じく、失敗した子の側にある）。`Failed` は、11.12.9 の決まりどおり、送り始める
この段で enum に足す。`Watch`、`Unwatch`、`DeathWatchNotification` は段 4c、`Supervise` は足さない（11.12.9）。

#### 11.13.3 失敗（`handleInvokeFailure`）

失敗は、失敗した実行の中で、原典の `handleInvokeFailure`（`FaultHandling.scala:215-245`）の順に処理する。

1. 自分のメールボックスを suspend する（1 回）
2. 返事を待つ失敗がまだ無ければ（原典 `isFailed` が偽）、「失敗の張本人」（原典 `perpetrator`）をセルに記録する。張本人は、
   自分の失敗なら自分、子の `Failed` の判断で decider が panic したなら（11.13.4）その子である
3. 親の表の子のすべてに `Suspend` を送る。ただし、2 で張本人に子を記録したときは、その子には送らない（その子は自分の失敗で
   suspend されている）
4. 親の列に `Failed` を積む。メールボックスの無い親（ガーディアン、function ref）は、その場で判断する（11.13.4）

失敗の種類と、その場所：

| 失敗 | 場所 | 今との違い |
|:---|:---|:---|
| `receive` の失敗（catcher の下） | `invoke` | `after_run` に記録して `Yield` を返す代わりに、上の 1〜4 を行い、`Continue` を返す。suspend されたメールボックスは、次の通常のメッセージを取らない（I4、`mailbox_scheduling.qnt` の `noInvokeWhileSuspended`） |
| `Kill` | `invoke` | 同じ。`FailureKind::Killed` |
| 開始の失敗（`Create` と、やり直しの `create`） | `create_now` | 同じ。`FailureKind::Initialization`。「開始に失敗した」の印は残す（インスタンスが無いことの印。判断が済んだかの区別は消す） |
| decider の panic | 親の実行の `Failed` の処理 | 親の 1〜4。張本人はその子（11.13.4） |
| 子の終了のフックの失敗 | 親の実行の `ChildTerminated` の処理 | 今は publish だけ（11.12.4 の 2）。原典どおり、publish のあとに 1〜4 を行う（`FaultHandling.scala:327-336` の `handleInvokeFailure(Nil, e)`。`FailureKind::MessageHandling`）。停止中の親での子の生成の拒否も同じ。親の親の判断も失敗を publish するので、publish は 2 回になる（原典も、`handleChildTerminated failed` のログと、親の親の `logFailure` の 2 つ） |

1 つの実行で 2 度失敗しても（失敗したあとの system message の処理がまた失敗するなど）、そのたびに 1〜4 を行う。suspend は
2 回、`Failed` も 2 通で、親はそれぞれを判断する。4a の「1 つの実行で記録される失敗は高々 1 件」（11.12.8）は、`after_run` の
1 枠のための決まりで、この段で要らなくなる。`stop_and_reply` の返事が記録されているときは、今と同じく判断を当てない
（`Terminate` が積まれている。11.12.3）。

#### 11.13.4 判断（親の実行の中の `Failed`）

親の実行が `Failed` を処理するとき（原典 `handleFailure`、`FaultHandling.scala:305-325`）：

1. 失敗した子が、親の表に、`Failed` の `ActorRef`（id と世代）と同じ登録で残っているときだけ判断する。ほかは捨てる（原典の
   「dropping Failed from unknown child」「from old child」。11.12.6 の照合を、取り置きの解放から `Failed` の処理へ移す）
2. 失敗を publish する（今の `publish_child_failure`）
3. 戦略を決める。子が親から受け継ぐなら親の戦略（`ParentEntry.strategy`）、生成のときに戦略を渡された子なら、その戦略
   （親の表の子の記録に置く。下）。decider を catcher で包んで回す（今の `decide_catching`）
4. 判断を当てる

| 判断 | one-for-one | all-for-one |
|:---|:---|:---|
| Restart（許された） | 子に `Recreate` を送る（`restartChild(suspendFirst = false)`） | 11.13.5 |
| Restart（回数を超えた）、Stop | 子に `Terminate` を送る | 子のすべてに `Terminate` を送る |
| decider の panic | 親の失敗にする（11.13.3 の 1〜4。張本人はその子）。子は suspend のまま待つ | 同じ |

decider の panic のあとの子は、親の判断の効果で決まる。親が再起動され、親の `pre_restart` がその子を止めなければ（既定の
`pre_restart` は子をすべて止める）、親の後半が生き残りの子に `Recreate` を送り（11.13.6）、子は再起動されるか（インスタンスが
あれば）、開始をやり直す（無ければ）。親が止まるか、`pre_restart` が子を止めれば、子は `Terminate` で止まる。これで、
4a の `resume_after_restart`（後半の終わりに `Resume` を送る）と、「判断の済んだ開始の失敗に `Create`」の 2 つの経路が、原典の
1 つの経路（生き残りの `Recreate`）になる。

**再起動の回数を親の表に置く**：原典は、子の再起動の回数と時間の窓を、親の子の表（`ChildRestartStats`）に置く。判断は親の
実行の中で行うので、親の実行から子のセルを読むと、段 5 で子の実行と競合する。4b では、`RestartHistory` を子のセルから親の
表の子の記録へ移す。親の表の子の記録は、名前、`ActorRef` に加えて、`RestartHistory` と、生成のときに渡された戦略（受け継ぐ
なら無し）を持つ。記録は生成で作り、親が `ChildTerminated` を処理したときに消える（11.12.4）。`Failed` は子が表にいるときだけ
判断するので、4a の「失敗した時点の回数を `Failure` に写す」（11.12.2 の「判断の時点で子が止まっていたとき」）は要らなくなる。
止まった子の `Failed` も、親が `ChildTerminated` より先に処理すれば判断される（原典と同じ）。親の列には、子の `Failed` が
`ChildTerminated` より先に積まれる。親が suspend されていて `Failed` が取っておかれたときは、`ChildTerminated` が先に処理され、
戻った `Failed` は 1 で捨てられる（これも原典と同じ）。

**メールボックスの無い親**：ガーディアンと function ref には `Failed` を積む列が無い。4a の `ChildTerminated` と同じく、失敗した
子の実行の中で、実行器がその場で 1〜4 を行う。Restart は子の列に `Recreate` を積み、子の実行がこのあとで処理する。decider の
panic は、4a と同じく publish を 1 回して、インスタンスのある子には `Resume { caused_by_failure: true }` を送り、開始に失敗した
子はそのまま残す（11.12.18 の「セルの無い親の decider の panic」）。原典のガーディアンはアクターで、user guardian の失敗は
root guardian の判断（既定は停止）になるが、fraktor のガーディアンはセルを持たないので、この差は残す。4b の再帰の suspend
により、開始に失敗したまま残された子の子は、suspend されたまま残る（4a では動き続けた）。Quint の `settledMeansResumed` は、この
子の下を除く（11.13.11）。

**親の戦略を読む時点**：判断は親の実行の中で、親のインスタンスがあるときにだけ行う。`Failed` は、親が suspend されている間は
取っておかれ（11.13.8）、親の列では親の `Create` が先にある。11.12.2 の「親の戦略を読む時点」の前提（手で回す dispatcher が
FIFO で、親が先に予約される）は要らなくなる。Quint の `decideWithInstance` で確かめる。

#### 11.13.5 all-for-one

原典 `AllForOneStrategy.processFailure`（`FaultHandling.scala:554-566`）：Restart が許されるのは、親の表の子のすべてで再起動の
許しが出たとき（`children.forall(requestRestartPermission)`。最初に断られた子で数えるのをやめる。今の `apply_all_for_one` と同じ）。
許されれば、子のすべてに、失敗した子でなければ `Suspend` を、そのあとで `Recreate` を送る（`restartChild(suspendFirst = crs.child
!= child)`）。許されなければ、子のすべてに `Terminate` を送る。

子のすべては、親の表の子のすべてである（`getAllChildStats`。停止を頼んだ子も、閉じて `ChildTerminated` を待つ子も含む）。兄弟の
状態で分けず、どの兄弟にも同じものを送る。兄弟の側で起きることは、その兄弟の列の順と状態で決まる。

| 兄弟の状態 | 兄弟の実行で起きること（原典） |
|:---|:---|
| 作られて、子を待っていない | `Suspend`（`faultSuspend`：自分と子を suspend）、`Recreate`（`faultRecreate`：`pre_restart`、子を待つか、`finishRecreate` が suspend を 1 つ解き、`post_restart`） |
| まだ最初の `Create` を処理していない | 列は `Create`、`Suspend`、`Recreate` の順。`Create` が `pre_start` を回し、そのあとの `Recreate` が再起動する。`pre_start` は 2 回（`Create` と、既定の `post_restart`）、`post_restart` は 1 回で、再起動の許しを 1 回数える。#254 のコメントの 2。4a はこの兄弟を最初の `Create` に任せ、`pre_start` が 1 回少なく、`post_restart` が無く、回数に数えなかった |
| 開始に失敗している | `Suspend` のあとの `Recreate` は、インスタンスが無いので `faultCreate` に落ちる（前の試みの子を止めて待ち、作り直す）。その兄弟の `Failed` がまだ親の列にあれば、その判断がもう 1 回 `Recreate` を送り、作られた兄弟を再起動する。suspend は、兄弟の失敗の 1 回と `Suspend` の 1 回を、2 通の `Recreate` がそれぞれ解く |
| 子を待っている（`Creation`、`Recreation`） | `Suspend` と `Recreate` は取っておかれ（`StashWhenWaitingForChildren`）、待ちが終わってから処理される。#254 のコメントの 1。4a はこの兄弟を飛ばし、再起動が 1 回少なかった |
| 停止中（`Termination` で待つ） | 取っておかれ、閉じるときに捨てられる（4a と同じく dead letters にはしない。段 4c） |
| 閉じている | 閉じたメールボックスに断られる（今と同じ） |

失敗した子が開始の失敗なら、その子への `Recreate` は `faultCreate` に落ちる（4a の「失敗した子に `Create`」と同じ効果）。

#### 11.13.6 `Recreate`（`faultRecreate`、`faultCreate`、`finishRecreate`）

子の実行が `Recreate` を処理するとき（原典 `faultRecreate`、`FaultHandling.scala:92-117`）：

| セルの状態 | すること |
|:---|:---|
| インスタンスが無い（開始に失敗した） | `faultCreate`（`FaultHandling.scala:158-176`）：前の試みの子に `Terminate` を送り、いれば「子を待つ（理由：Creation）」にする。いなければ（または最後の子の `ChildTerminated` で）`finishCreate`：suspend を 1 つ解き、張本人の記録を消して、作る（11.12.2 の「作る」）。作るのが失敗すれば 11.13.3 |
| 子を待っていない（原典 `isNormal`） | 前半：stash を戻し、失敗したメッセージの送り手を `sender()` にして `pre_restart` を回す（今の `restart_cell` の前半）。停止を頼んだ子（`dying`）がいれば「子を待つ（理由：Recreation）」にし、いなければ後半へ |
| 子を待っている | ここには来ない（取っておかれる。11.13.8）。原典の分岐（`faultResume(null)` で suspend の数を合わせる）は写すが、届かない |

後半（原典 `finishRecreate`、`FaultHandling.scala:278-303`）：

1. 生き残りの子を控える（親の表の子のすべて。待ちが終わった時点なので、停止を頼んだ子はもういない。新しいインスタンスが作る子は
   含まない）
2. suspend を 1 つ解き、張本人の記録を消す
3. 新しいインスタンスを作り、`post_restart` を回す（今の `finish_recreate`）。古い振る舞いの枠は、ここで捨てる（11.12.18）
4. 1 の子のすべてに `Recreate` を送る（D4b-1 の A。原典の「only after parent is up and running again do restart the children
   which were not stopped」）

`Kill` の生き残りの子の扱い（11.12.18）は、4 に含まれて消える。4 の `Recreate` を受けた子の側では、上の表のとおり、子を待つ子は
取っておき、作られていない子は先に `Create` を処理し、開始に失敗した子は `faultCreate` に落ちる。11.12.18 の「`Kill` の生き残りの
子」の行の 3 つの条件（`dying` にいる子、待っている子、作られていない子を外す）は、送り手の側の条件でなく、受け手の側の
振る舞いになる。

`Recreate` を受けたアクターは、必ず suspend されている（原典は `faultRecreate` と `faultCreate` で assert する）。どの `Recreate` も、
それより先に届いた suspend（自分の失敗、親の失敗の `Suspend`、親の `faultSuspend` の `Suspend`、all-for-one の `Suspend`）と
対になり、後半か `finishCreate` がそれを解く。Quint の `recreateWhileSuspended` で確かめる。

#### 11.13.7 `Suspend` と `Resume`（`faultSuspend`、`faultResume`）

| system message | すること（原典） |
|:---|:---|
| `Suspend` | 自分を suspend し、親の表の子のすべてに `Suspend` を送る（`faultSuspend`、`FaultHandling.scala:124-128`） |
| `Resume { caused_by_failure }` | インスタンスが無ければ `faultCreate`（原典「changing Resume into Create」）。あれば、suspend を 1 つ解き、失敗への答えなら張本人の記録を消し、子のすべてに `Resume` を送る。張本人の子には `caused_by_failure` をそのまま、ほかの子には偽で送る（`faultResume`、`FaultHandling.scala:136-156`、`Children.resumeChildren`） |

fraktor には Resume の判断（`Directive::Resume`）が無いので、`Resume` を送るのは、メールボックスの無い親の decider の panic
（11.13.4）と、そこから広がる `faultResume` だけである。`faultRecreate` の届かない分岐も `Resume` の形で写す。

**失敗 1 回に suspend 1 回**（4a の 11.12.6 の決まりの、4b の形）：suspend を 1 つ足すのは、失敗（11.13.3 の 1）、`Suspend`、
`Terminate`（子を待つとき）の 3 つで、1 つ解くのは、`finishRecreate`、`finishCreate`、`faultResume` の 3 つである。どれも、その
アクターの実行の中で起きる（I4）。どの足しも、ちょうど 1 つの解きと対になる。

| 足す | 解く |
|:---|:---|
| 自分の失敗 | その `Failed` への判断の効果：Restart の `Recreate`（後半か `finishCreate`）。Stop は解かずに閉じる。decider の panic は、親の判断の効果（親の後半からの `Recreate`、親の停止、ガーディアンの `Resume`） |
| 親の失敗の `Suspend`、親の `faultSuspend` の `Suspend` | 親の判断の効果：親の後半からの `Recreate`、親の `faultResume` からの `Resume`。親が止まれば閉じる |
| all-for-one の `Suspend` | 同じ判断の `Recreate` |
| `Terminate`（子を待つ） | 解かずに閉じる |

4a の 11.12.6 の表の行は、次のとおり置き換わる。「`receive` の失敗、`Kill` の Restart」は同じ（後半）。「decider の panic」は、
親の後半からの `Recreate` になる（実行器の `Resume` は消える）。「開始の失敗の Restart」は、`Recreate` からの `finishCreate` になる。
「失敗していないのに再起動されるアクター」は、前半の始めに実行器が suspend する形をやめ、`Suspend` の system message にする。

#### 11.13.8 取り置き（`shouldStash`）

原典 `ActorCell.systemInvoke`（`ActorCell.scala:490-555`）の形をそのまま写す。セルは、取っておいた system message の列
（`system_stash`。原典 `sysMsgStash`）を持つ。セルの状態は 3 つである。

| 状態 | 条件 | 取っておくもの |
|:---|:---|:---|
| 既定 | 子を待っておらず、suspend されていない | なし |
| suspend されている | 子を待っておらず、suspend の数が 1 以上 | `Failed`（`StashWhenFailed`） |
| 子を待っている | 待つ理由がある（`Creation`、`Recreation`、`Termination`） | `Recreate`、`Suspend`、`Resume`、`Failed`（`StashWhenWaitingForChildren`） |

`Create`、`Terminate`、`ChildTerminated` はどの状態でも取っておかない。1 通を処理するたびに状態を求め直し、状態が下がったら
（子を待つから suspend へ、suspend から既定へ、子を待つから既定へ）、取っておいた列を、届いた順のまま、メールボックスの列の
残りより先に処理する。取っておいたものを処理する間にも、状態に応じて取っておく（原典の `invokeAll` の `rest.reversePrepend
(unstashAll())`）。閉じたら、取っておいた列は捨てる（原典は dead letters へ流す。11.12.3 と同じく段 4c）。

実装の形：取り置きの判定と列は、セルの型（`ActorCell` か、セルが持つ小さな型）の振る舞いにする（rich-domain-model）。
`MailboxCell::system_invoke` は、1 通を受け取り、手元の列（その 1 通と、状態が下がったときに戻した列）を空になるまで処理して
返す。メールボックスの側（`Mailbox`、`MailboxRun`、`mailbox_scheduling.qnt`）は変えない。メールボックスから見ると、1 通の
system message の処理が長くなるだけである。

4a の `ParentEntry.held` は、この取り置きの `Failed` になる。4a の「親が suspend されている間の子の失敗は取っておく」「親の
suspend が 0 に戻ったら届いた順に判断する」「親が閉じたら捨てる」「子が表に無ければ捨てる」（11.12.6）は、それぞれ、
`StashWhenFailed`、状態が下がったときの戻し、閉じるときの捨て、11.13.4 の 1 になる。原典と同じく、取り置きは親の
`Failed` の処理の中で起きるので、見回り（11.12.18 の「取り置きの解放の見回り」）は要らない。

**子を待つ間の `Recreate`**（#254 のコメントの 1）：4a は、停止中か子を待つアクターへの再起動を飛ばした（11.12.18）。4b は、
`Recreate` を取っておき、待ちのあとで処理する。待ちが `Recreation` なら、後半のあとにもう 1 回再起動し、再起動の回数が原典と
合う。`Creation` なら、作れたあとに再起動する（作るのがまた失敗すれば、取っておいた `Recreate` は `faultCreate` に落ちる）。
`Termination` なら、閉じるときに捨てられる（原典の `faultRecreate` の `isNormal` の検査と同じく、止まるアクターは再起動しない）。
入れ子にはならない：取っておいた `Recreate` を処理する時点で、前の再起動は後半まで終わっている（Quint の
`recoveryNotNested`。負の対照 `NoStashWhileWaiting`）。

#### 11.13.9 11.12.18 で 4b に回した差を消す

| 11.12.18 の行 | 4a の形 | 4b の形 |
|:---|:---|:---|
| decider の panic のあとの子の再開 | 親の `resume_after_restart` に記録し、後半の終わりに `Resume` | 消える。親の後半が、生き残りの子として `Recreate` を送る（11.13.6）。子は再開でなく再起動される（原典） |
| decider が panic した親の suspend | 親を失敗させる時点で実行器が suspend する | 親の実行の中の `handleInvokeFailure`（11.13.3）。I4 の例外が消える |
| 取り置きの解放の見回り | 取り置いた失敗の数を数え、実行のあとに親の表を見回る | 消える（11.13.8） |
| `Kill` の生き残りの子 | 後半が同期に再起動する子から、`dying` にいる子、待っている子、作られていない子を外す（下の直しを参照） | 消える。どの失敗でも、後半が生き残りの子のすべてに `Recreate` を送る（11.13.6） |
| 停止中か、子を待っているアクターの再起動 | 停止中のアクターは再起動しない。all-for-one と `Kill` の生き残りでは、子を待つ兄弟を飛ばす | `Recreate` を取っておく（11.13.8）。停止中のアクターでは閉じるときに捨てられ、子を待つアクターでは待ちのあとに再起動する |
| all-for-one のインスタンスの無い兄弟 | 最初の `Create` を処理していない兄弟は、その `Create` に任せる。開始に失敗して判断の済んだ兄弟には `Create` | どの兄弟にも `Suspend` と `Recreate`（11.13.5） |
| セルの無い親の decider の panic | 子にすぐ `Resume` | 同じ（11.13.4）。`Resume` は原因の有無を持つ |
| 失敗の記録 | `Failure` に、子の戦略と、それを受け継ぐかを入れる | 子の戦略は親の表の子の記録に置き、`Failure` からは外す（11.13.4） |
| メールボックスの読み口 `is_open_and_suspended` | 取り置きの判定に使う | 消える（取り置きはセルの状態で決める） |

**11.12.18 の `Kill` の行の直し**（#254 のコメントの 3）：4a の実装（`finish_recreate` の `survivors`）は、`dying` にいる子に
加えて、子を待っている子（`is_waiting`）と、作られていない子（`is_created` が偽）も外している。11.12.18 の行をそのとおりに
直した（この PR の 11.12.18 の差分）。4b では、この行の振る舞いそのものが上の表のとおり消える。

#### 11.13.10 回復の重なりの深さ 1（#251 の 2 件）

`run_recovery(failures = 3, max_retries = 3)`（`do_not_nest`）の流れ（D4b-1 の A）：

1. 親が `"go"` を処理し、子 `bob` を作る（名前を予約し、`Create` を積む）
2. `bob` の実行が `Create` を処理し、`pre_start` が失敗する。`bob` を suspend し、親の列に `Failed(bob)` を積む
3. 親の実行が `Failed(bob)` を処理する。decider が panic し、親の失敗になる。親を suspend し、`healthy` に `Suspend` を送り
   （`bob` は張本人なので送らない）、親の親（テストの `Guardian` アクター）の列に `Failed(parent)` を積む
4. 親の親の実行が `Failed(parent)` を処理し、親の生成のときに渡された戦略（`one_for_one(max_retries)`）の Restart で、親に
   `Recreate` を送る
5. 親の実行が `Recreate` を処理する。`pre_restart` が範囲を 1 にし、子を止めないので、その場で後半に進む。`post_restart` が
   深さ 1 を記録し、生き残りの `bob` と `healthy` に `Recreate` を送る。後半の終わりで古い振る舞いの枠が捨てられ、範囲は 0 に戻る
6. `bob` の実行が `Recreate` を処理する。インスタンスが無いので `faultCreate` から作り直し、`pre_start` がまた失敗する（2 に戻る）。
   `healthy` の実行は、`Suspend` のあとの `Recreate` で再起動される（D4b-1）
7. 2〜6 を 3 回繰り返し、4 回目の開始が成功する。試行 4、親の再起動 3、深さ 1

どの段も別の system message で、別の実行で進む。親の再起動（5）の中で `bob` の `pre_start` が走ることも、`bob` の失敗の判断
（3）が走ることも無い。`bob` の 2 回目の失敗の判断は、親の列の `Failed` で、親の後半が返ったあとの親の実行で行われる。深さ 1 は、
4a の「生成は `Create` を処理する子の実行でだけ」に加えて、「判断は親の実行でだけ、再起動は自分の実行でだけ」で保たれる。

`max_retries = 2`（`obey_parent_retry_limit`）では、3 回目の親の `Failed` で再起動の許しが出ず、親の親が親に `Terminate` を送る。
親の実行が T1 で `healthy` と `bob` に `Terminate` を送って待ち、両方の `ChildTerminated` のあとで閉じる。`bob` は suspend のまま
`Terminate` を処理する。試行 3、再起動 2、深さ 1、親と子が止まり、`bob` についての終了のフックは 1 回で、4a と同じである。

D4b-1 の A では、`healthy` が親の再起動のたびに作り直される。`run_recovery` のうち、`healthy_creations == 1` と、再起動のあとの
値が 1 であることの 2 つの assert（`:519-520`）は落ちるので、triage へ差し戻して原典の振る舞いを確かめる形に直す（元帥の決定、
2026-11-27。11.13 の冒頭）。深さ（`restart_depths`）、試行の数、再起動の数、停止、通知の数は変わらず、その assert は書き換えない。

#### 11.13.11 不変条件と Quint

**`mailbox_scheduling.qnt`**：遷移は変えない。1 つのメールボックスから見ると、取り置き（11.13.8）は 1 通の system message の
処理の中で起き、列と印の扱いは変わらない。I12（system message は失われない）は、取り置きの列を「処理済み」と「列の中」の間の
置き場所として含む。閉じるときに捨てる扱いは 11.12.3 のまま。

**`actor_termination.qnt`**：段 4a の実行器の判断（`decide` の動作）を消し、4b の形を書いた。`runSys` が処理する system message に
`Suspend`、`Resume(caused)`、`Recreate`、`Failed(子, 開始の失敗か)` を足し、各アクターに取っておいた列（`stash`）と張本人
（`failed`）を足した。判断は、親の `runSys` が `Failed` を処理するときに行い、決め方（Restart、Stop、decider の panic）は
`runSys` の引数で選ぶ。ガーディアン（ROOT）の下の子の失敗は、その場で判断する（引数 `rd`）。p の戦略は、始めに one-for-one か
all-for-one を選ぶ（`afo`）。`runSys` は、処理の前の状態で取っておくかを決め、処理のあとで状態が下がったら、取っておいた列を
列の先頭へ戻す。1 手で 1 通を処理するので、列の先頭へ戻すことは、原典の `invokeAll` が残りの前に処理することと同じ順になる。

| 動作 | 実装で当たるところ |
|:---|:---|
| `spawn` | 生成の手順（11.12.2）。親が suspend されていないときだけ（親は自分の実行の中で子を作る） |
| `runSys` | 実行が system message を 1 件処理するか、取っておく。`Failed` は 11.13.4、`Recreate` は 11.13.6、`Suspend` と `Resume` は 11.13.7、取り置きは 11.13.8 |
| `receive` | 実行が通常のメッセージを 1 件処理する。失敗すれば 11.13.3 |
| ほか | 4a と同じ（`send`、`stop`、`tellRest`） |

足した不変条件：

| 不変条件 | 中身 | 何を表すか |
|:---|:---|:---|
| `noReceiveUnderAFailedParent` | 張本人の記録がある（親に失敗を知らせ、答えを待つ）アクターの子は、通常のメッセージを処理しない | 再帰の suspend（11.13.3、11.13.7。D4b-1 の A） |
| `decideWithInstance` | 親が `Failed` を判断するのは、親にインスタンスがあるときだけ | `StashWhenFailed`（11.13.4、11.13.8） |
| `recreateWhileSuspended` | `faultRecreate` と `faultCreate` が走るアクターは suspend されている | 原典の assert。suspend と `Recreate` の対（11.13.6） |
| `stashOnlyWhileSuspended` | 取っておいた列があるのは、suspend されているか子を待っているアクターだけ | 状態が下がったら戻す（11.13.8） |

4a の不変条件は、`oneRetryPerDecision` を除いてすべて残した（`recoveryNotNested`、`settledMeansResumed`、`noReceiveWhileWaiting`
など）。`settledMeansResumed` は、何も進行中でない（system message の列が空で、通知が残っていない）ときに、インスタンスのある
（または作られる前の）アクターの suspend が 0 で、取っておいた列も空であることに強めた。ただし、開始に失敗したまま残された
アクター（ガーディアンの decider の panic。11.13.4）の下のアクターは除く。`oneRetryPerDecision` は、4a の「判断のまだの開始の
失敗に届いた 2 通目の `Create` は何もしない」（11.12.2）を表していた。4b では、失敗ごとに `Failed` が届き、親の失敗の `Suspend`
で数が合うので、原典どおり 2 通目の `Recreate` も作り直す（11.12.2 の「原典との関係」の形）。4a の決まりを 4b に残すと、2 通目が
答えるはずの `Suspend` が解かれずに残る（負の対照 `IgnoreUndecidedRecreate`）。そのため、この不変条件と、負の対照
`RetryUndecided` は消した。

負の対照（4b で足したもの。`initWith(<誤り>, afo)` から始める `run` が、毎回の `quint test` で確かめる）：

| モード | 誤り | 破れる不変条件 | 確かめる `run` |
|:---|:---|:---|:---|
| `NoSuspendChildren` | 失敗が、失敗した本人だけを suspend する（段 3、4a の形） | `noReceiveUnderAFailedParent` | `noSuspendChildrenLetsAChildReceiveUnderAFailedParentTest` |
| `NoStashWhenFailed` | suspend された親が、子の `Failed` をその場で判断する | `decideWithInstance` | `noStashWhenFailedDecidesWithoutAnInstanceTest` |
| `NoStashWhileWaiting` | 子を待つアクターが、`Recreate` などをその場で処理する | `recoveryNotNested` | `noStashWhileWaitingNestsTheRestartTest` |
| `DropRecreateWhileWaiting` | 子を待つアクターに届いた `Recreate` を捨てる（4a の飛ばし）。前の `Suspend` は取っておく | `settledMeansResumed` | `dropRecreateWhileWaitingLeavesTheActorSuspendedTest` |
| `IgnoreUndecidedRecreate` | 判断のまだの開始の失敗に届いた `Recreate` は何もしない（4a の 2 通目の決まり） | `settledMeansResumed` | `ignoreUndecidedRecreateLeavesTheChildSuspendedTest` |
| `NoSurvivorRecreate` | 後半が、残した子に `Recreate` を送らない（4a の、`Kill` 以外の形） | `settledMeansResumed` | `noSurvivorRecreateLeavesTheChildSuspendedTest` |
| `NoChildCheck` | 親が、`Failed` の世代を表と照らさずに判断する | `recreateWhileSuspended` | `noChildCheckRecreatesAnUnsuspendedChildTest` |

4a の負の対照のうち、`ResumeAfterStartFailure`、`ResumeOnRestart`、`CreateInsideRestart`、`RetryUndecided` は消した。前の 3 つは
実行器の判断（判断のあとの `Resume`、判断の中の同期の作り直し）の誤りで、判断が親の `Failed` になると、その遷移そのものが
無くなる。`ResumeOnRestart` が表していた「待つ間に通常のメッセージを処理する」は `noReceiveWhileWaiting` に残り、4b では待つ間の
`Resume` は取っておかれる。`CreateInsideRestart` が表していた深さ 2 は、`NoStashWhileWaiting` が 4b の形で表す。残した 4a の
負の対照（11 個）は、`run` を 4b の動作で書き直した（`receive` の失敗と開始の失敗は、親の `Failed` か、ガーディアンのその場の
判断を経る）。

正しい形の `run` は 13 本である。4a の 9 本のうち 7 本（`aStopBehindTheCreateRunsAfterPreStartTest` ほか）は残し、開始の失敗と
再起動を経るものは 4b の動作で書き直した。残りの 2 本は、`anEscalatedStartFailureIsRetriedAfterTheParentRestartTest` を 4b の
動作で書き直し、`twoQueuedCreatesRetryOncePerDecisionTest` を `twoRecreatesBehindAFailedStartAreBalancedTest` に置き換えた。
それに 4 本を足した。下の表は、書き直した 1 本、置き換えた 1 本、足した 4 本である。

| `run` | 確かめること |
|:---|:---|
| `anEscalatedStartFailureIsRetriedAfterTheParentRestartTest` | #251 の形。decider の panic で親が再起動され、親の後半の `Recreate` で、子の開始のやり直しが子の実行で走る（深さ 1） |
| `twoRecreatesBehindAFailedStartAreBalancedTest` | 4a の高 1 の重なりを原典の形で。子の列に `Recreate`、`Suspend`、`Recreate` が並び、1 通目がまた失敗し、2 通目が作り、2 回目の失敗の判断が再起動する。suspend が 0 に戻る |
| `aRecreateWhileWaitingForChildrenRunsAfterTheWaitTest` | #254 の 1。子を待つ c1 に届いた `Suspend` と `Recreate` が取っておかれ、待ちのあとで処理され、c1 は 2 回再起動する |
| `anAllForOneRestartRestartsASiblingThatHasNotStartedTest` | #254 の 2。最初の `Create` の前の兄弟の列が `Create`、`Suspend`、`Recreate` になり、再起動が 1 回数えられる |
| `aParentsRestartRecreatesTheChildrenItKeepsTest` | D4b-1 の A。親の失敗が子を suspend し、親の後半の `Recreate` が子を再起動して解く |
| `aFailedFromAnOldChildIsDroppedTest` | インスタンスの無い親が子の `Failed` を取っておき、作り直したあとで戻す。その子が止まって同じ名前で作り直されていれば、古い世代の `Failed` は捨てられる |

`run` は全部で 31 本である（正しい形 13 本、負の対照 18 本）。

仕様の単純化（4a の 11.12.11 の一覧に足すもの、変わるもの）：

- 監督の判断は 3 つ（Restart、Stop、decider の panic）。再起動の許しは、子ごとの窓でなく、全体の回数（`RESTARTS`）で表す。
  all-for-one の許しも 1 回分として数える。`Kill` は入れない（`receive` の失敗と同じ口を通る）
- all-for-one にできるのは p だけ（兄弟は c1 と c2）。ほかは one-for-one
- ガーディアンの判断はその場で行い、decider の panic は、インスタンスのある子に `Resume(true)`、開始に失敗した子には何もしない
  （11.13.4）。4a の一覧の「監督の判断は 3 つで、all-for-one と `Kill` は入れない」は、この 3 行に置き換わる
- 失敗するのは `receive`、生成（`Create` と、やり直しの作成）、decider の panic だけ。`pre_restart`、`post_restart`、`post_stop`、
  子の終了のフックの失敗は入れない（11.13.15）
- `spawn` は、親が suspend されているときは行わない。`WeakActorSystem::spawn_child` は、suspend された親の下にも子を作れるが、
  その子は親の `Suspend` を受けていないので、親の後半の `Recreate` を、suspend されないまま受ける（`recreateWhileSuspended`
  の外。suspend の数は 0 より下がらないので、数は崩れない）。11.13.14 の壊しうる入力に入れ、単体テストで確かめる
- 4a の「判断の取り置きは `decide` の前提としてだけ表す」は消える。取り置きは状態（`stash`）に持ち、戻す順と捨てる条件を
  仕様が確かめる

#### 11.13.12 テスト（テストの担当に渡す面）

red のテストを書くのに使える面（どれも、今の公開 API か testkit から観測できる）：

| 面 | 何を読むか |
|:---|:---|
| フックの回数と順 | `pre_start`、`pre_restart`、`post_restart`、`post_stop` の各回数と順（テストのアクターが共有の記録に積む） |
| 再起動の回数 | decider に渡る `restart_count`（`SupervisorStrategy::with_decider` の引数）、再起動の許しの上限で止まること |
| suspend | 子が通常のメッセージを処理しないこと（親の失敗のあと、判断の前に子へ送ったメッセージが、判断の効果のあとまで `receive` に届かない）。単体テストでは `Mailbox` の `is_suspended`（テストの補助） |
| 失敗の publish | イベントの受け手（`EventSink`）が受ける失敗の数と、その子の path |
| 1 手ずつの順 | `run_one` で 1 回の実行ずつ進め、どの実行で何が起きたかを確かめる（判断が親の実行で起きること、`Recreate` が子の実行で起きること） |
| 停止と名前 | `is_actor_terminated`、同じ名前で作れるか |

新しい単体テスト（実装の PR で red から始める）：

- 失敗：`receive` の失敗のあと、判断の前に子へ送ったメッセージは、親の判断の効果まで処理されない（11.13.3 の 3）／decider の
  panic の張本人の子には `Suspend` が送られない（suspend が 2 にならず、親の後半の `Recreate` 1 通で動き出す）／1 つの実行で 2 度
  失敗すると、親は 2 回判断する／子の終了のフックの失敗が、親の失敗になって親の親が判断する
- 判断：`Failed` は親の実行で判断される（子の実行が返った時点では、まだ判断されていない。`run_one` で確かめる）／suspend された
  親（自分の失敗、開始の失敗）への子の `Failed` は、親の作り直しや再起動のあとで判断される／止まって同じ名前で作り直された
  子の、古い世代の `Failed` は判断されない（decider が呼ばれない）／再起動の回数は、親の表の記録で数えられ、子が作り直されても
  引き継がれる（今と同じ値になること）
- all-for-one：最初の `Create` の前の兄弟は、`pre_start` 2 回、`post_restart` 1 回で、再起動が 1 回数えられる（#254 の 2）／子を
  待つ兄弟は、待ちのあとで再起動される（#254 の 1）／止まる途中の兄弟は再起動されない
- 子を待つ間の `Recreate`：自分の再起動で子を待つアクターに届いた `Recreate` は、後半のあとにもう 1 回の再起動になる（`post_restart`
  が 2 回）。`Creation` の待ちの間に届いた `Recreate` は、作れたあとの再起動になる
- 生き残り（D4b-1 の A）：`pre_restart` で子を残した親が再起動すると、子も再起動される（`Kill` 以外の失敗でも）
- suspend の数：上のどの筋書きでも、終わったあとに通常のメッセージが届く（suspend が残らない）

#### 11.13.13 実装の最初の工程：洗い出し

#251 の一覧には、段 4b の行が無い（段 3 の 2 件と段 4a の 13 件だけ。回復の重なりの深さの 2 件は 4a と 4b の両方の完了条件）。
4a と同じく（11.12.12 の「一覧に無いテスト」）、実装の最初の工程として、4b で変わる順に頼る既存のテストを洗い出し、中将の判定を
受けてから red に入る。テストは書き換えずに上げる。

1. 写しは、11.13.1 の表の形をすべて入れた形で作る（`Failed`、`Suspend`、`Resume`、`Recreate` を system message にし、取り置きと、
   子の suspend と、生き残りの `Recreate` を入れる。D4b-1 は A に決まった）
2. その写しで、テストスイートの全体（testkit を含む `cargo test --workspace`）を流し、落ちたテストを、原典の振る舞いと判定つきで
   #254 に載せる
3. 落ちずに通るが意味が弱まるテストを目で読む。少なくとも、`pre_restart` を上書きして子を残すテスト（`ticket669_spec.rs`、
   `supervisor_misc_spec.rs`、`supervisor_tree_spec.rs`、`supervision_all_for_one_regression.rs`、`executor_value_test.rs` の
   `run_recovery` と `a_retained_child_handles_messages_with_a_fresh_behavior_after_kill_recovery`、`actor_system_value_test.rs`）と、
   段 3 の橋渡しの `Yield` を確かめたテスト（`executor_value_test.rs` の `a_failed_message_is_followed_by_the_restart_before_the_next_message`、
   `a_kill_ends_the_run_and_the_messages_behind_it_go_to_dead_letters` など、`with_catcher("YieldOn…")` の群）を読む。読んだ範囲と
   見つけたものも #254 に載せる

設計の時点で読んで見つけた例：`run_recovery` の `healthy` の 2 つの assert（D4b-1）。子の終了のフックの失敗で、
`IllegalStateException` の publish を 1 回と数えるテスト（11.13.3 の表の最後の行で 2 回になる）。段 3 の `Yield` を確かめた群のうち、`run_one`
の戻りと回数を数えるもの（`a_failed_message_is_followed_by_the_restart_before_the_next_message` は「`run_one` が system message
だけの実行で止まった」ことを読む）は、判断と再起動が親と子の別の実行になるので、回数が変わりうる。記録の順（`boom`、
`restarted`、`next`）は変わらない見込みである。

#### 11.13.14 壊しうる入力と影響

| 区分 | 入力 | 期待 |
|:---|:---|:---|
| 失敗 | 子のいるアクターが失敗し、判断の前に子へ送る | 子は判断の効果まで処理しない。Restart なら親の後半の `Recreate` のあと、Stop なら止まる |
| 失敗 | 1 つの実行で 2 度失敗する | `Failed` が 2 通、suspend 2 回。2 回の判断の効果で 0 に戻る |
| 判断 | 開始に失敗した親に、子の `Failed` が届く | 取っておかれ、親の作り直しのあとで判断される。子がその間に止まれば捨てられる |
| 判断 | 子が失敗して止まり、同じ名前で作り直されたあとに、古い `Failed` が判断される | 捨てられる（decider は呼ばれない） |
| 判断 | ガーディアンの下の子の decider が panic する | 4a と同じ（インスタンスのある子は `Resume`、開始に失敗した子はそのまま）。その子の子は suspend のまま残る（11.13.4） |
| all-for-one | 兄弟が作られる前、開始に失敗した、子を待つ、止まる途中、閉じた | 11.13.5 の表 |
| 再起動 | 自分の再起動で子を待つ間に、もう 1 回の `Recreate` が届く | 後半のあとにもう 1 回再起動する。入れ子にならない |
| 再起動 | 子を残す `pre_restart` で再起動する | 残した子も再起動される（D4b-1 の A） |
| 再起動 | 再起動の許しの上限を超える | 子に `Terminate`。all-for-one なら全員 |
| 生成 | `WeakActorSystem::spawn_child` で、suspend された親の下に子を作る | 子は親の `Suspend` を受けない。親の後半の `Recreate` で、suspend されないまま再起動される（suspend の数は 0 のまま） |
| 停止 | 子を待つ間に `Terminate` が届く | 理由が `Termination` になり、取っておいた `Recreate` などは閉じるときに捨てられる |
| 回復の深さ | #251 の 2 件 | 深さ 1（11.13.10） |
| panic | catcher の無い decider の panic、`pre_restart` の panic | 段 3 の巻き戻り。次の呼び出しでシステムが終わる（今と同じ） |
| 速さ | 1 件ごとの送受信 | 経路は変わらない。失敗の無い実行では、`invoke` が `after_run` と `pending_decider_failures` を見なくなり、実行のあとの取り置きの見回りの比較も無くなる。§7.5 の S1〜S4 を測って報告する。止める基準ではない |

#### 11.13.15 選ばなかった形

- **判断を実行器に残し、`Recreate` だけを system message にする**：`after_run` の 1 枠と取り置きの見回りが残り、親の戦略を
  読む時点の前提（11.12.2）も残る。§6 の 4b の行（`Failed` を system message に）を満たさない
- **4a の「判断のまだの開始の失敗に届いた 2 通目は何もしない」を残す**：4b では、2 通目が答える `Suspend` が解かれずに残る
  （`IgnoreUndecidedRecreate`）。原典は失敗ごとの `Failed` と `Suspend` で数を合わせており、4b はその形になる
- **all-for-one の兄弟を状態で分けて送る（4a の形）**：送り手が兄弟の状態を読むと、段 5 で兄弟の実行と競合する。原典は状態で
  分けず、受け手の側の `systemInvoke` が振る舞いを決める
- **再起動の回数を子のセルに残す**：親の実行が子のセルを読むことになり、段 5 で競合する。原典は親の表に置く
- **`post_restart` の失敗を原典どおり親の判断にする**（`PostRestartException` で `handleInvokeFailure`、`setFailedFatally`）：
  既定の decider の答え（止める）は今と同じで、違うのは decider を上書きした場合だけである。`FailureKind` に種類を足すと公開 API
  が変わり、`isFailedFatally` の扱い（次の `Resume` を `Recreate` に変える）も要る。#254 の範囲の外として残す
  - 中将が [Issue #301](https://github.com/j5ik2o/fraktor-ng-rs/issues/301)（post_restart の失敗の扱いを原典どおりにする）に分けた（2026-11-27）
- **ガーディアンをアクターにして、原典どおり user guardian の失敗を root guardian に判断させる**：#234 の範囲の外。11.13.4 の差を
  残す

#### 11.13.16 動かしたコマンドと結果（設計の時点、2026-11-27、quint 0.32.0）

| 検査 | 結果 |
|:---|:---|
| `scripts/quint-check-specs.sh`（CI と同じ。typecheck、各仕様の `run`、`quint run` 20 手・1 万本） | exit 0。`actor_termination.qnt` は 31 passing（正しい形 13 本、負の対照 18 本）。ほかの仕様は変えていない |
| `actor_termination.qnt` の `quint run --invariant=invariant`（正しい形、rust の backend、20 手・1 万本、5 回） | 5 回とも違反なし（trace の長さの最大 21、平均 20.7〜20.8） |
| 同（40 手・5 万本、3 回） | 3 回とも違反なし（最大 41、平均 31.9〜32.0） |
| 同（`--backend=typescript`、60 手・5,000 本） | 違反なし（最大 49、平均 31.0） |
| 18 の誤りのモードから始める `quint run`（20 手・1 万本、モードごとに 3 回。`init` の `initWith(Fixed, afo)` を `initWith(<モード>, afo)` に書き換えた写し） | 15 のモードは 3 回とも違反を見つけた。`IgnoreUndecidedRecreate` と `NoChildCheck` は 3 回中 1 回、`DropRecreateWhileWaiting` は 3 回とも見つけなかった（どれも、2 通の `Recreate` が並ぶか、子を待つ間に all-for-one の判断がもう 1 回届くか、止めた子を同じ名前で作り直すかが要り、筋書きが長い）。この 3 つは、40 手・5 万本では 2 回とも見つけた。CI が毎回確かめるのは、決まった筋書きの `run` である |
| 探索が届く状態（正しい形、20 手・1 万本。その状態を否定した式を不変条件にして流す） | 取り置きの列に `Recreate` がある、同じ列に `Recreate` が 2 通並ぶ、all-for-one で最初の `Create` の前の兄弟に `Recreate` が届いている、2 回再起動した、子の失敗を張本人に記録した（decider の panic）、開始に失敗した親が `Failed` を取っておいている、開始に失敗したまま残された親の下に子がいる、`Creation` の待ちの間に取っておいている、suspend の数が 2 以上の生きたアクター、の 9 つは見つけた。`Recreation` の待ちの間に `Recreate` を取っておいている状態（#254 の 1）は、20 手・1 万本では見つけず、40 手・5 万本で 2 回とも見つけた |
| 設計の途中で見つけたこと | 最初に書いた仕様は、`faultResume` が子のすべてに原因なしの `Resume` を送っていた。40 手・5 万本の探索が、張本人の子の失敗の記録が残り、その孫が親の失敗の間に通常のメッセージを処理する筋書き（`noReceiveUnderAFailedParent` の違反）を見つけた。原典の `resumeChildren` は、張本人の子にだけ原因を渡す（`child.resume(if (perp == child) causedByFailure else null)`）。仕様をそのとおりに直し、11.13.7 の表にも書いた。直したあと、違反を出した 2 つの seed はどちらも違反なしになった。また、ガーディアンの decider の panic で開始に失敗したまま残された子の下で、`settledMeansResumed` が破れることを見つけ、11.13.4 に書いて不変条件から外した |
| `uv run python3 tools/license-headers/license_headers.py --check --manifest docs/design/issue-206-file-origin.tsv` | errors 0 |
| `npm run check:aislop` | exit 0 |

Rust のコードは変えていないので、Rust のビルドとテストは流していない。
