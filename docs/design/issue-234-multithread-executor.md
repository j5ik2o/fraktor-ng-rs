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
| C1 | 原子操作は `portable-atomic` 越しにする | API は core と同じで、CAS のある target では core の命令になる。CAS の無い target では critical-section で代わりに行い、その実装は adaptor（embassy や HAL）が渡す。`MailboxStatus`、id の数え上げ（今の `NEXT_ID` などの `AtomicU64`）、印の `AtomicBool` は、どれもこれを使う |
| C2 | ロックは kernel が持つ port 越しにする | 閉じた区間を渡す形にする（下の「ロックの port」）。embassy-sync のロック（`CriticalSectionRawMutex`、`ThreadModeRawMutex`）と同じ形で、std の `Mutex`、`parking_lot`、`spin` でも作れる。システム全体の表も、この port を使う（再レビュー L6 の「spin のまま」は取りやめた） |
| C3 | Arc は kernel の中の別名 `kernel::sync::Arc` を使う | `cfg(target_has_atomic = "ptr")` なら `alloc::sync::Arc`、無ければ `portable_atomic_util::Arc`。target の cfg で選ぶので、kernel に `feature = "std"` を置かない決まり（syn-check の `cfg-std-forbid`）には触れない。所有権モデルの決定文書の「Arc」は、この別名を含む。CAS の無い target（thumbv6m、RP2040）も対象に入れる。ただし、その target で build できるようにするのは段 0b（#243）で、`Arc<dyn …>` の作り方と、`self: Arc<Self>` を使う公開 API の変更を含む |

| 置き場所 | 型（名前は案） | 中身 |
|:---|:---|:---|
| kernel `sync.rs` | `Arc`、`Weak`（別名）、原子操作の型の再公開 | C1 と C3 |
| kernel `sync/raw_lock.rs` | trait `RawLock: Send + Sync`（port） | `fn lock(&self, f: &mut dyn FnMut())`。閉じた区間を 1 つ走らせる。dyn で呼べる形なので、型引数が公開 API に伝わらない |
| kernel `sync/lock_cell.rs` | `LockCell<T>` | `Box<dyn RawLock>` と中身を持ち、`with(&self, f: impl FnOnce(&mut T) -> R) -> R` だけを出す。中は `UnsafeCell` で、unsafe はこの型の中に閉じる |
| kernel `sync/lock_factory.rs` | trait `LockFactory: Send + Sync`（port） | `RawLock` を作る。システムを作るときに adaptor が渡す。kernel の既定の工場は `spin`（`portable_atomic` の機能つき）で作る |
| kernel `dispatch/mailbox_status.rs` | `MailboxStatus` | `portable_atomic::AtomicU32` 1 つ。bit 0 が closed、bit 1 が scheduled、bit 2 から上が suspend の数（Pekko `Mailbox.scala:47-55` と同じ配置）。`set_as_idle`、`suspend`、`resume`、`become_closed` は compare-and-set の繰り返し。§3.8 の工夫 2 と 5 を段 2 で入れると、予約は `fetch_or` の 1 手になり、bit 2 が「system message がある」の印になって、suspend の数は bit 3 から上へ移る |
| kernel `dispatch/message_queue.rs` | trait `MessageQueue: Send + Sync` | `enqueue(Envelope)`、`dequeue`、`has_messages`、`number_of_messages`、`drain`。**閉じたかどうかは持たない**（下の「閉じる門」）。§3.8 の工夫 3 により、列そのものは adaptor が差し替えられる |
| kernel `dispatch/…` | `UnboundedQueue`、`BoundedQueue`（待たない）、`PriorityQueue`（stable の有無）、`ControlAwareQueue` | kernel の既定の列。`VecDeque<Envelope>` を `LockCell` で守る |
| kernel `dispatch/message_queue_factory.rs` | trait `MessageQueueFactory: Send + Sync`（port） | dispatcher の設定が持ち、メールボックスごとに列を作る。std と tokio はロックを使わない列、embassy は固定長の列を渡せる（§3.7、§3.8 の工夫 3） |
| kernel `dispatch/system_message.rs` | enum `SystemMessage` | Create、Suspend、Resume、Recreate、Terminate、Supervise、Watch、Unwatch、Failed、DeathWatchNotification、ChildTerminated |
| kernel `dispatch/mailbox.rs` | `Mailbox` | 状態、閉じる門、通常のキュー（`Arc<dyn MessageQueue>`。Balancing では team で共有）、system message のキュー、`ActorCell` を入れる場所（`LockCell<Option<ActorCell>>`）、dispatcher への戻りの辺（§3.4 の条件 2 により `Weak` か id）。1 アクターに 1 つ |
| kernel `dispatch/mailbox_run.rs` | `MailboxRun`（kernel が adaptor に渡す仕事） | `run(self) -> RunOutcome`。1 回の実行で、system message を処理し、通常のメッセージを throughput 件まで処理し、まだ残っているかを返す（§3.8 の工夫 1）。`RunOutcome::Again` なら adaptor はもう一度予約し、`Idle` なら何もしない。`Idle` を返してよいのは、`set_as_idle` のあとに予約を読み直し、その予約に負けたとき（ほかの送り手が先に予約した、または仕事が無い）だけである。読み直して勝ったら `Again` を返す。この順を守らないと、`LostWakeup` と同じ取りこぼしになる。段 2 で、仕様の `finishIdle` と `finishRegister` に対応づける（再レビュー 3 の L2） |
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

自分専用のキューでは、門とキューを同じロックで守ってよい（送るたびのロックは 1 回で済む）。§3.8 の工夫 3 で、ロックを
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
| embassy | 割り込みからも触るものは `CriticalSectionRawMutex`、スレッドのモードだけで触るものは `ThreadModeRawMutex` |
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
| `LockFactory` | std の `Mutex` か `parking_lot` | std と同じ | `CriticalSectionRawMutex`、`ThreadModeRawMutex` |
| 原子操作、Arc | core（`portable-atomic` はそのまま core の命令になる） | 同じ | CAS のある target は core と同じ（段 0 のあと）。CAS の無い target（thumbv6m、RP2040）は段 0b（#243）のあとで、`portable-atomic` の critical-section と `portable_atomic_util::Arc` |
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
```

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
| I11 | system message は、実行の始めと、通常のメッセージ 1 件ごとのあとに、届いている分をすべて処理する。system message を追い越す通常のメッセージは、多くても 1 件（直前の処理し切りのあとで system message が届き、その直後に取り出したもの） | §3.5 の 1 と 2 | `systemBeforeNormal`（`drainDone` のあとでしか `startInvoke` できない。負の対照は `MessagesBeforeSystem`） |
| I12 | system message も失われない。処理済み、キューの中、dead letters のどれかにある。閉じるときの残りと、閉じたあとに来たものは dead letters へ行く（DeathWatch のため。dead letters は Watch に DeathWatchNotification を返す） | 原典 `Mailbox.scala:318-338`、`:503-505` と同じ | `systemMessagesConserved` |
| I13 | プールに入ったメールボックスには、その実行でできる仕事が必ずある（suspend 中を通常のメッセージで予約しない） | `can_be_scheduled` | `submittedOnlyWithWork` |
| I14 | 何も進行中でない（送り手が途中にいない、実行も予約も無い、親の判断も待っていない）とき、開いたメールボックスは、できる仕事をやり終えている。suspend されているのは、外からの Suspend がまだ Resume で打ち消されていないときだけ | I3 と、失敗のたびに親が Resume を返すこと | `settledMeansIdle`（幽霊の変数 `externalSuspendsPending` は、外からの Suspend で 1 増え、外からの Resume で 1 減る。再レビュー L5） |
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

### 5.2 CI の決まりへの合わせ方

- 不変条件は `invariant`。§4.1 の I1〜I8、I11〜I14 の論理積である
- `run` は 20 本で、名前はどれも `Test` で終わる。`Test` で終わる名前は `run` にしか使っていない
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
| `DropSystemOnClose` | 閉じるときに残りの system message を捨て、閉じたあとの system message も捨てる。この仕様の最初の版（`e702bbe`）の誤りである（設計レビュー H2） | `systemMessagesConserved` | `dropSystemOnCloseLosesAWatchTest`（正しい形は `remainingSystemMessagesAreDeadLettersTest`、`systemMessageAfterCloseIsADeadLetterTest`） |

`LateEnqueue` は、Pekko でも起きうる取りこぼし（閉じた直後に積まれたメッセージが、dead letters にならずに消える）
である。この設計では、閉じる門の中で見てから積むので、起きない（I6）。これは、Pekko のテストと文書が許している振る舞いの
範囲の中で、選べる振る舞いを狭めることで、意味論のずれではない（`docs/decisions/pekko-semantics-first.md` の例）。
I6 は、利用者への約束として、公開 API の文書には書かない（元帥の M1、2026-11-26）。

`DropSystemOnClose` を避ける理由は DeathWatch である。原典の `Mailbox.scala:318-338`（`processAllSystemMessages`）は、
「if we closed the mailbox, we must dump the remaining system messages to deadLetters (this is essential for
DeathWatch)」としている。閉じたあとの `systemEnqueue`（`:503-505`）も dead letters のメールボックスへ回す。
dead letters が Watch を受けると、DeathWatchNotification を返す。捨ててしまうと、Terminate と同じ実行の中で後ろに
入った Watch が消え、watcher には Terminated が永遠に届かない（`expect_terminated` が時間切れになる）。

手で並べた `run` だけでなく、ランダムな探索でも違反が見つかることを確かめた（§10 の 6、10、14）。誤りの 6 つのモードから
始めた `quint run`（20 手、1 万本）は、6 つとも違反を見つけた。再レビューで見逃していた変異 R12（外からの Suspend の
あとの Resume が効かない）も、幽霊の変数を数に替えた版では探索が見つけた。設計レビューで探索が見逃した変異 3 つ（suspend 中の
予約、Resume を失う、親に Failed を伝えない）も、I13 と I14 を足した版では探索が見つけた。正しい形は、CI と同じ
条件（20 手、1 万本）でも、60 手・10 万本でも違反が無かった。最初の版は、Apalache による 12 手までの有界の検証でも
違反が無かった（§10 の 8。直した版では流していない）。

### 5.4 この仕様で扱わないもの（後の段で書く仕様）

| 仕様（案） | 段 | 状態と不変条件 | 負の対照 |
|:---|:---|:---|:---|
| `actor_termination.qnt` | 段 4a | T1〜T5。子の集合、「停止中で、開いていて、suspend されている」状態、post_stop、閉じたか、通知の順 | `post_stop` の前に Terminated を送る。子が残っているのに閉じる（子の ChildTerminated が閉じたメールボックスに届く） |
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
| 0 | kernel を no_std で build できるようにする（元帥の C4、2026-11-26） | crate は割らない。lib.rs を `#![cfg_attr(not(feature = "std"), no_std)]` にし、adaptor を `std` の機能（既定で有効）の後ろに置く（lib.rs は kernel ではないので、`cfg-std-forbid` には触れない）。kernel の各ファイルで `alloc` を明示して import する。`AtomicU64` の 3 か所（`NEXT_ID`、`NEXT_SPAWN`、`NEXT_LIST`）と、kernel のほかの原子操作を `portable-atomic` にする（C1）。`AtomicU64::try_update`（`double_linked_list_value.rs`）は portable-atomic に無いので `fetch_update` にする（再レビュー 3 の L5）。kernel の `Arc` を `kernel::sync::Arc` にする（C3。CAS のある target では `alloc::sync::Arc` のまま）。`spin` に `portable_atomic` の機能を足す。`getrandom` を adaptor の側の依存にする。前提：`portable-atomic` と `portable-atomic-util` のライセンスの記録を、段 0 の PR より前に #206 の側へ入れる（§7.4 の追記の案。元帥、2026-11-26） | 既存のテストのすべてが通る。CI に `cargo check -p fraktor-actor --no-default-features --target thumbv7em-none-eabihf` を足し、通る（CI の実装は段 0 の PR で行う。再レビュー 3 の試作では、この段の直しでエラーが 0 になった）。根拠は `docs/decisions/crate-structure-comparison.md` の 2026-11-26 の節 | 3 |
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
- 依存：段 0 で `portable-atomic`（`fallback`、`critical-section` の機能）と `portable-atomic-util` を足し、`spin` に
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
