# 設計：#181 TestFSMRefSpec（testkit の TestFSMRef を用意する）

[Issue #181](https://github.com/j5ik2o/fraktor-ng-rs/issues/181) の第1段（設計だけ）の記録である。実装は、別の担当がこの設計をレビューして通ってから、別途指示を受けて行う。この文書は実装を含まない。

対象は Pekko v2.0.0 の `testkit/src/test/scala/org/apache/pekko/testkit/TestFSMRefSpec.scala` である。原典の本体は `testkit/src/main/scala/org/apache/pekko/testkit/TestFSMRef.scala`（`TestActorRef` を継承し、`stateName`、`stateData`、`setState`、タイマの proxy を足したもの）と、`actor/src/main/scala/org/apache/pekko/actor/FSM.scala` である。

台帳の対象ノードは `testkit/src/test/scala/org/apache/pekko/testkit/TestFSMRefSpec.scala`（`testkit:TestFSMRefSpec`、`test_cases_approx: 6`、`status: unported`、`needs.testkit: ['extends-pekko-spec', 'pekko-spec', 'within', 'await-cond', 'test-fsm-ref']`）の1件だけである。第2段でこのノードを green にする。

## 1. 原典のケースを数え直した結果と、移す先の対応表

原典を `" in {"` の数で数え直した。`ignore` と `pending` は無い。

- 「A TestFSMRef must」の下に2件
- 「A TestFSMRef Companion Object must」の下に4件
- 合計6件。台帳の目安6件と一致する

移す先は `crates/fraktor-testkit/tests/test_fsm_ref_spec.rs`（ノード1件＝ファイル1件、冒頭に出典のコメントを残す）である。

| 番号 | 原典のケース | Rust のテスト名 | 区分 |
|:---:|:---|:---|:---|
| 1 | allow access to state data | `allows_access_to_state_data` | 移す。ただし下の注1と注2の条件が付く |
| 2 | allow access to timers | `allows_access_to_timers` | 移す |
| 3 | allow creation of a TestFSMRef with a default supervisor | `allows_creation_with_a_default_supervisor` | 移す |
| 4 | allow creation of a TestFSMRef with a specified name | `allows_creation_with_a_specified_name` | 移す |
| 5 | allow creation of a TestFSMRef with a specified supervisor | `allows_creation_with_a_specified_supervisor` | 移す |
| 6 | allow creation of a TestFSMRef with a specified supervisor and name | `allows_creation_with_a_specified_supervisor_and_a_name` | 移す |

作らない・除外の提案は0件である。本 Spec には、provider をクラス名で選ぶケースのような、JVM や設定ファイルに固有のケースは無い。HOCON の設定を写す部分も本 Spec には無い（`PekkoSpec` の設定は土台の話で、本 Spec のケースは設定値を見ていない）。そのため、前例（#154 の D1、G0b の決定 E）の「HOCON は型付きの入力にする」という判断は、本件では生成の選択肢（名前と supervisor）を型付きの引数で受ける形にだけ効く（§3）。

注1（ケース1とケース2の同期性）：原典は `fsm ! "go"` の直後の行で `stateName` を読む。ケース2の「タイマを開始した後に `isTimerActive` が真」「取り消した後に偽」も、移植では制御メッセージを通るので同じく非同期である。原典の `TestActorRef` は CallingThreadDispatcher で動くので、送信がその場で処理される。今の fraktor-ng-rs の実行器では送信はキューに入り、待ちの運転手が処理するので、直後に読むと古い状態が見えうる。移植のテストは、送信の後で `awaitCond` に当たる `FraktorSpec::await_cond` で状態が変わるまで待ってから確かめる形にする。待ちは、ハンドルが持つ「処理を終えた制御メッセージの通し番号」が、送った最後の番号に達することでも待てるようにし、値の一致だけでは空振りで通らないようにする（§3.1、§4 の I6）。送信したその場で処理されるという強い同期性そのものを確かめる形への書き戻しは、testkit の CallingThreadDispatcher（[Issue #234](https://github.com/j5ik2o/fraktor-ng-rs/issues/234) の段7b、[Issue #259](https://github.com/j5ik2o/fraktor-ng-rs/issues/259)）の後とする。これはケースの除外ではなく、待ち方の違いである。アサーションの内容（状態名と状態データの遷移）は弱めない。

注2（ケース1の状態タイムアウト）：ケース1の後半は `setState(timeout = 100.millis)` の後、80〜500ミリ秒の範囲で状態タイムアウトが発火し、`StateTimeout` のイベントで状態2へ移ることを見る。今の `Fsm` にはタイマも状態タイムアウトも無い（§2）。この部分は §3.4 のタイマ発火の案（案Aか案Bか）をレビューで確定してから実装する。確定できない場合は、ケース1を「状態データの読み書き（タイムアウト以外）」と「状態タイムアウト」に分け、後者だけを保留として報告し、台帳を green にしない。

## 2. 対応する本体が fraktor-ng-rs にすでにあるか

2026-11-26 の main（`f08cc4a`）で確かめた。

| 原典の本体 | fraktor-ng-rs の現状 |
|:---|:---|
| `FSM`（状態関数、`goto`、`stay`、`using`、遷移の通知） | ある。`crates/fraktor-actor/src/kernel/actor/fsm.rs` の `Fsm<S, D>` と `fsm_state.rs` の `FsmState<S, D>`（[Issue #194](https://github.com/j5ik2o/fraktor-ng-rs/issues/194)、PR #244 で `FsmView` が入った）。状態関数は `Props` の factory の中で `Fsm` の値として組み立てる形である |
| `FSM.stateName`、`FSM.stateData`（本体の外から読む口） | 無い。`Fsm::current` は非公開で、外から読めるのは状態関数へ渡す `FsmView<S>`（`listener_count` と `state_name` だけ、データは無い）だけである |
| `FSM.applyState`（`TestFSMRef.setState` の実体） | 無い。遷移は `Fsm::make_transition`（非公開）がイベント処理の中でだけ行う |
| FSM のタイマ（`startTimerWithFixedDelay`、`startTimerAtFixedRate`、`startSingleTimer`、`cancelTimer`、`isTimerActive`、`isStateTimerActive`） | 無い。`fsm.rs` のモジュール文書が「Timers, state timeouts, and `onTermination` are not part of it」と明記している |
| `StateTimeout` | 無い。`crates/` の全体に同名の型は無い |
| `ActorContext` から scheduler を取る口 | 無い。`ActorContext` の公開メソッドにスケジューリングは無く、`ActorSystem` にも scheduler の公開口は無い（testkit には `ManualScheduler` があるが、アクターへはつながっていない） |
| `TestActorRef`（`TestFSMRef` の親クラス） | 無い。`crates/fraktor-testkit` に同名の型は無い。[Issue #182](https://github.com/j5ik2o/fraktor-ng-rs/issues/182) で別に移す対象である |
| `TestFSMRef` | 無い。台帳の `needs` の `test-fsm-ref` が未充足である |
| 任意の親の下に子を作る口（ケース5と6の supervisor 指定） | ある。`ActorSystem::downgrade()` が返す `WeakActorSystem::spawn_child(parent, props, name, strategy)` が、親と名前を指定した生成を持つ |
| guardian（ケース3の既定の supervisor） | ある。`ActorSystem::resolve_actor_ref` で `/user` のパスを解決できる |
| テストの土台（`within`、`awaitCond`） | ある。`FraktorSpec::within`、`within_range`、`await_cond`（台帳の `needs` の `within` と `await-cond` は充足済み） |

所有権モデルとの関係が、本件の設計の中心である。決定文書 `docs/decisions/fable-graph-model-proposal.md` の項目10が記録するとおり、executor がアクター本体を単独所有する決定の下で、`underlyingActor` が本体そのものを返す Pekko の API をどう表すかは未決である。したがって本件は、本体への `&mut` を返す形は採らない。代わりに、状態の複製を持つ共有ハンドルと、本体への制御メッセージで表す（§3）。この形は #194 で入った `FsmView` の形（状態関数へは読み取り専用の面だけを渡す）とも重ならない。`FsmView` はイベント処理の最中に状態関数へ渡す面であり、本件のハンドルはテスト側がイベント処理の外から読む面で、役割が違う。

## 3. 新しく足すものと、公開 API の形

置き場所の決め方を先に書く。`TestFSMRef` は testkit の型なので `crates/fraktor-testkit` に置く。`TestActorRef`（#182）と重なる部分は、生成（supervisor と名前の指定、ランダムな名前）と、本体への同期アクセスの土台である。本件では汎用の `TestActorRef` を作らず、FSM 専用の最小の土台（共有ハンドルと制御メッセージ）に閉じる。理由は2つある。第一に、#182 の対象は `TestActorRefSpec` の23件で、要求する面が本件より広く、本件に引き込むと範囲が膨らむ。第二に、土台の形（本体を返さず、複製と制御メッセージで表すこと）自体は #182 と共有できるので、#182 の設計のときに、本件の `FsmHandle` と制御メッセージの置き場所を共有化の候補として引き継げるよう、この文書に形を残す。

### 3.1 fraktor-actor（kernel）に足すもの

新しい型は、syn チェッカーの型ごとに1ファイルの決まりに従い、1型1ファイルで置く。シグネチャは設計の形であり、名前の細部は実装のときに既存の命名へ合わせる。

```rust
// crates/fraktor-actor/src/kernel/actor/fsm_handle.rs
/// 状態とデータの複製と、タイマの有効な名前の集合を持つ共有ハンドル。
/// `Fsm` が遷移のたびに書き、テスト側がいつでも読む。
#[derive(Clone)]
pub struct FsmHandle<S, D> { /* Arc<spin::Mutex<FsmSnapshot<S, D>>> */ }

impl<S: Clone, D: Clone> FsmHandle<S, D> {
  pub fn state_name(&self) -> Option<S>;
  pub fn state_data(&self) -> Option<D>;
  pub fn is_timer_active(&self, name: &str) -> bool;
  pub fn is_state_timer_active(&self) -> bool;
}
```

- ハンドルは、外から受け取って付ける形にする。`Fsm::attach_handle(&mut self, handle: FsmHandle<S, D>)` を足し、`D: Clone` の境界はこのメソッドにだけ付ける。`Fsm::new()` で作った通常の機械はハンドルを持たず、既存の利用者にコストと制約を足さない。
- ハンドルは `TestFsmRef` が1つ作り、包んだ factory が機械を作るたびに複製を渡す（§3.3）。再起動で factory が呼ばれ直しても、同じハンドルが新しい個体に付き直す。
- スナップショットは、次の時点で書く。(1) `attach_handle` の時点と `start_with` の時点で、その時点の開始状態を書く。`initialize()` を呼ばない機械（原典のケース1とケース2は呼ばない）でも、生成の直後から `state_name` が読める。(2) `make_transition` が `current` を置いたら、通知の有無によらず書く（`stay().using(d)` でデータだけが変わる場合を含む）。移る先に状態関数が無いときだけ書かない。(3) `pre_start` の中で、アクター自身の `self_ref` をハンドルへ書く。アクター自身から見た名前と親を、テスト側が後から確かめられるようにするためである。
- ハンドルは「処理を終えた制御メッセージの通し番号の最大値」を持つ。番号は `TestFsmRef` が送るときに振り、制御メッセージに載せて運ぶ。ハンドルは、処理した制御メッセージの番号で最大値を更新する。発火の印は番号を持たず、番号を進めない（印で進むと、繰り返しタイマのケースで、取り消しの処理より先に待ちが抜けてしまう）。番号は、再起動と、停止でスナップショットを空に戻す操作をまたいで保つ（ハンドルのうち、空に戻す対象はスナップショットとタイマの記録だけであり、番号とタイマの世代の数えは戻さない）。テストと性質ベーステストは、送った最後の番号まで進んだことを待ってから読む（値の一致で待つと、途中の状態が最後と同じ値の操作列で空振りするため）。
- ハンドルが状態データの複製を返すため、`D: Clone` が要る。原典の `stateData` は参照を返すので、これは Rust 側の制約として API 文書に書く。`Clone` でないデータの機械は、ハンドル無しの通常の形で使う。
- ハンドルの共有には、kernel の `Arc`（`kernel::sync` のもの。ポインタ幅の原子操作が無いターゲットでは `portable-atomic-util` から来る）と `spin::Mutex` を使う。

```rust
// crates/fraktor-actor/src/kernel/actor/state_timeout.rs
/// 状態タイムアウトが発火したときに、機械が自分へ届けるイベント。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StateTimeout;
```

```rust
// crates/fraktor-actor/src/kernel/actor/fsm.rs への追加（形）
impl<S, D> Fsm<S, D> {
  /// 状態関数の無い状態への移動を含む、原典の `applyState` に当たる遷移を、
  /// 制御メッセージ経由で受け付けるための内部の入り口（公開しない）。
  pub(crate) fn apply_state(&mut self, context: &mut ActorContext<'_>,
                            name: Option<S>, data: Option<D>,
                            timeout: Option<Duration> /* stop reason は §3.5 */);
  // 名前とデータは Option で受け、kernel の中で None を今の値に置き換えてから、
  // goto として（常に通知する形で）適用する。テスト側は今の状態を知らないので、
  // FsmState を組み立てて渡す形にはしない
}
```

制御メッセージの組み立てと送信は、別のクレートである `fraktor-testkit` の `TestFsmRef` が行うので、`pub(crate)` では届かない。公開の経路は1つに決める。`fraktor-actor` に cargo の feature `test-support` を足し、testkit 向けの面をすべて、その feature の下でだけ公開する。対象は、制御メッセージの型と組み立て（状態の設定、タイマの開始と取り消し、番兵、発火の印）だけではなく、`FsmHandle` とその読み取りの口、`Fsm::attach_handle`、期限のスレッドが読む「張った、取り消した」の記録まで含む。置き方は次の形にする。`pub(crate)` の型を `pub use` で再公開する形は、rustc の E0364 でコンパイルできず、syn-check の module-wiring の決まり（末端モジュールへの `pub mod` の禁止、`pub use` は直属の子モジュールからだけ）にも反するので採らない。

- `kernel/actor/test_support.rs` を、子モジュールを持つ非末端モジュールとして置き、`#[cfg(feature = "test-support")] pub mod test_support;` で出す。
- その配下に非公開の子モジュール（例：`test_support/fsm_handle.rs`）を置き、中身の型とメソッドは `pub` で書く。`test_support.rs` は直属の子モジュールだけを `pub use` する。feature が切れているときは、このモジュール自体が存在せず、外部のクレートからは到達できない。
- `Fsm::attach_handle` は、`Fsm` が常時公開の型なのでモジュールでは隠せない。メソッド自体を `#[cfg(feature = "test-support")]` で囲う。
- feature が切れているときに未使用の警告が出ないよう、`Fsm` のハンドルの保持、制御メッセージの分岐、testkit 専用のコードも、同じく `#[cfg(feature = "test-support")]` で囲う。

この形は、一時ディレクトリで rustc と cargo を流して確かめた。feature を切った形では自クレートが警告なしで通り、外のクレートからは到達できず（E0433）、feature を入れた形では外のクレートから `FsmHandle` を使えた。`fraktor-testkit` がこの feature を有効にする。原典で `applyState` が `private[pekko]` であることに合わせ、既定の公開 API には出さない（約束を足さないため）。`Fsm::receive` が通常のイベントより前に処理する。制御メッセージはメールボックスの中で利用者のメッセージと同列に並ぶので、順序は「送った順」に保たれる。

制御メッセージの種類によって、状態タイマと世代への扱いが違う。通常のイベントと同じ扱い（前の状態タイマの取り消しと世代を上げること）をするのは、状態の設定だけである。タイマの開始と取り消しは、原典の proxy が `receive` を通らず状態タイマに触れないことに合わせ、世代を上げず、状態タイマも取り消さない。

### 3.2 fraktor-actor（kernel）に足すタイマの表

`Fsm` の中に、名前からタイマへの表を持つ。

```rust
// crates/fraktor-actor/src/kernel/actor/fsm_timer.rs（形）
pub(crate) struct FsmTimer {
  name: String,            // 空文字も許す（§5）
  message: Message,        // 発火時に自分へ送るメッセージ
  kind: FsmTimerKind,      // Single / FixedDelay(Duration) / FixedRate(Duration)
  generation: u64,         // 名前付きタイマの取り消しと置き換えの判定に使う（振り方は §3.2）
}
```

- 同じ名前で開始すると、前のタイマを取り消して置き換える（原典の `setTimer` の意味論）。
- 名前付きタイマの世代は、表の行ごとに振らない。機械全体で単調に増える1本の数えから振る（原典の `timerGen` と同じ）。数えは共有のハンドルに置き、再起動をまたいで続ける。状態タイマの世代（機械の世代）も同じく、共有のハンドル側で進め、再起動で戻さない。再起動前の状態タイマの印が、再起動後の個体の世代に一致してしまうためである。行ごとに振ると、取り消して同じ名前で張り直したときに世代が振り直され、古い印が新しい行に通ってしまう。包んだメッセージは表の行だけが持ち、印は (名前, 世代) だけを持つ（古い印が古いメッセージを運ぶ余地を無くすため）。
- 存在しない名前の取り消しは、何もしない（原典の `cancelTimer` は存在しない名前でも例外を投げない）。
- `is_timer_active(name)` は表に名前があることと等しい。状態タイマは別の1枠で持ち、`is_state_timer_active()` が見る。
- 世代は kernel の `Fsm` が振る。発火は、生のメッセージではなく、世代を載せた内部の印で届く（名前付きタイマは (名前, 世代)、状態タイマは世代）。`Fsm::receive` が受信の中で判定する。表にあって世代が合うときだけ受け付け、単一タイマなら表から消してから、表の行が持つ包んだメッセージを取り出してイベントとして処理する。送る側で判定する形（判定と到着の間に取り消しや遷移が割り込む競合がある）は採らない。
- 停止のとき（`post_stop`）は、原典の `terminate` と同じく、すべてのタイマを取り消して表を空にし、状態タイマも消し、ハンドルのスナップショットも空に戻す。再起動では、新しい個体が空の表から始まる（§3.3 の factory の形で、ハンドルは開始状態で書き直される）。
- タイマの表の操作（開始、取り消し、有効か）は、この表だけで完結する。発火の仕組みは §3.4 で分ける。

### 3.3 fraktor-testkit に足す `TestFsmRef`

```rust
// crates/fraktor-testkit/src/test_fsm_ref.rs と test_fsm_ref/test_fsm_ref_value.rs
pub struct TestFsmRef<S, D> {
  actor: ActorRef,
  handle: FsmHandle<S, D>,
  supervisor: ActorRef,   // 生成時に決まった親
  name: String,
}

`TestFsmRef` は `Fsm` の値を直接は受け取らない。`Props::new` が `Fn() -> A` を取り、`Fsm` は `Clone` ではないので、値では再起動のたびに作り直せない。factory を受け取り、ハンドルは `TestFsmRef` が1つ作って、包んだ factory が機械を作るたびに `attach_handle` で付け直す。

impl<S, D> TestFsmRef<S, D>
where S: Clone + PartialEq + Send + Sync + 'static, D: Clone + Send + 'static {
  /// 既定の supervisor（guardian）とランダムな名前で作る。原典の `TestFSMRef(factory)`。
  pub fn new<F>(system: &mut ActorSystem, factory: F) -> Self
  where F: Fn() -> Fsm<S, D> + Send + Sync + 'static;
  /// 名前を指定して作る。原典の `TestFSMRef(factory, name)`。
  /// 名前が不正か使用中なら `Err`（原典は例外）。
  pub fn with_name<F>(system: &mut ActorSystem, factory: F, name: &str)
    -> Result<Self, ActorCreationError>
  where F: Fn() -> Fsm<S, D> + Send + Sync + 'static;
  /// supervisor を指定して作る。原典の `TestFSMRef(factory, supervisor)`。
  /// その親の子として作る。
  pub fn with_supervisor<F>(system: &mut ActorSystem, factory: F, supervisor: &ActorRef) -> Self
  where F: Fn() -> Fsm<S, D> + Send + Sync + 'static;
  /// supervisor と名前を指定して作る。原典の `TestFSMRef(factory, supervisor, name)`。
  pub fn with_supervisor_and_name<F>(system: &mut ActorSystem, factory: F,
    supervisor: &ActorRef, name: &str) -> Result<Self, ActorCreationError>
  where F: Fn() -> Fsm<S, D> + Send + Sync + 'static;

  pub fn actor_ref(&self) -> ActorRef;
  pub fn tell<M: Any + Send + Sync>(&self, message: M, sender: ActorRef); // 原典の `!`
  pub fn name(&self) -> &str;
  pub fn supervisor(&self) -> ActorRef;

  pub fn state_name(&self) -> S;   // 開始前は panic（§5）
  pub fn state_data(&self) -> D;
  /// 原典の `setState`。`None` の要素は今の値を保つ（原典の既定引数と同じ）。
  /// `timeout` の `None` は状態タイマを張らないことを表す。
  pub fn set_state(&self, state_name: Option<S>, state_data: Option<D>, timeout: Option<Duration>);
  pub fn start_timer_with_fixed_delay(&self, name: &str, message: Message, delay: Duration);
  pub fn start_timer_at_fixed_rate(&self, name: &str, message: Message, interval: Duration);
  pub fn start_single_timer(&self, name: &str, message: Message, delay: Duration);
  pub fn cancel_timer(&self, name: &str);
  pub fn is_timer_active(&self, name: &str) -> bool;
  pub fn is_state_timer_active(&self) -> bool;
}
```

supervisor を指定する生成には、名前の拒否を `Result` で返す口が要る。既存の口を探した結果、`WeakActorSystem::spawn_child` は名前を拒否されると panic する形しかなく、`ActorSystem` 側には `try_actor_of`（トップレベル用）があるが、任意の親の下で `Result` を返す口は無い。並行の PR にも同じ口は無いことを確かめたうえで、kernel に `WeakActorSystem::try_spawn_child(parent, props, name, strategy) -> Result<ActorRef, ActorCreationError>` を足し、`spawn_child` はその上に panic する形で残す。渡す監督戦略は既定の戦略である。新しい公開 API なので、§8 の確認点に挙げる。

子の生成の経路は、今は名前の検査だけであり、トップレベルの生成が通る設定の検証（dispatcher とルーターの検証）を通らない。戻り値を `ActorCreationError` にすると、設定の検証もされると読める形になるので、子の経路にも同じ検証を通す。入口によって検査が違う形は、[Issue #278](https://github.com/j5ik2o/fraktor-ng-rs/issues/278) がトップレベルで直している欠陥の型であり、子の側で同じ形を作らない。実装の段では、既存の検証の関数を探して子の経路からも使う。`try_spawn_child` の名前の引数は、既存の `spawn_child` と `try_actor_of` に揃えて `Option<&str>` とする。名前なしの生成（ケース5の、supervisor を指定して名前は自動採番の形）が要るためである。`spawn_child` は、名前ありと名前なしの両方を `try_spawn_child` へ委譲し、誤りでは panic する形に載せ替える。親がもう生きていない場合と、システムが終了している場合は、`try_spawn_child` では `Err` を返し（トップレベルの `try_spawn_user` が終了を `Err` で返す形に揃える）、`spawn_child` の panic はその上に残る。

既存の公開 API である `ActorContext::actor_of`（名前ありの子の生成で、`InvalidActorNameException` を返す）は、戻り値の型を変えない。内部の生成の経路は共有するが、設定の検証が誤りを返した場合に、その誤りを運ぶ型が `actor_of` には無いので、`actor_of` では panic する形にし、そのことを API 文書に書く。

ケース3〜6が原典で確かめる `underlyingActor.supervisor` と `underlyingActor.name` は、原典ではアクターが構築時に `context.parent` と `context.self.path.name` を自分のフィールドへ記録したものである。移植では、本体を返さないので、`TestFsmRef` が生成時に決まった supervisor と名前を保持して返す。ただし、それだけをテストで引数と比べるのは、自分の入力を返すだけの同語反復になり、実際にその親の子としてその名前で作られたことを確かめていない。そこで、パスでの確認を必須にする。`actor_ref().path().parent()` が supervisor のパス（ケース3では `resolve_actor_ref` で解決した `/user` の guardian のパス）と等しいことと、`actor_ref().path()` の名前が指定の名前であることを、ケース3〜6のすべてで見る。加えて、`pre_start` でハンドルへ書かれた `self_ref`（アクター自身から見た参照）からも、同じ名前を確かめる。

ランダムな名前は、原典の `TestActorRef.randomName` と同じく、既存の名前と衝突しない生成名を使う。既存の生成名の仕組み（`actor_of` が採番する名前）に乗り、独自の採番は作らない。

### 3.4 タイマの発火をどうするか（レビューで確定してほしい点）

ケース2は発火を見ていない（開始して有効を確かめ、取り消して無効を確かめるだけである）。発火が要るのは、ケース1の状態タイムアウトだけである。

決めた形は案Aである（中将の判断、2026-11-26。設計レビューの推奨と同じ）。ただし、最初の版の案A（送る側のスレッドが世代を判定し、生のメッセージを送る形）は採らない。競合（判定と到着の間に取り消しや受信が割り込む）と、単一タイマを表から消せない問題があるためである。決めた形は次のとおりである。

- 名前からタイマへの表、状態タイマ、世代、`StateTimeout`、受信の中での判定は、kernel の `Fsm` に置く（§3.2）。
- kernel は時間を数えない。発火の要求の出口を1つだけ作る。ハンドルが「張った、取り消した」の記録を持ち、testkit の期限のスレッドがそれを読んで、期限が来たら世代を載せた印を送り返すだけにする。判定は受信の中で行う。
- 期限のスレッドは、`TestFsmRef` の drop と、対象のアクターの停止で止める。

案B（`ActorContext` に scheduler の口を足す形）は採らない。kernel の `Scheduler` の port は [Issue #234](https://github.com/j5ik2o/fraktor-ng-rs/issues/234) の段6（[Issue #257](https://github.com/j5ik2o/fraktor-ng-rs/issues/257)）が持ち主であり、いま本件がその port の形と公開 API を先に決めると二重になる。案Aでも kernel は時間を数えず、Scheduler の port 越しに時刻へ触れるという移植の規約に反しない。kernel の `Fsm` に公開のタイマ API を足さない限り、案Aは「testkit の外では発火しない」という約束を足すことにはならず、未移植の機能が無いだけである。その代わり、API 文書で「タイマは `TestFsmRef` の下でだけ動く」と約束しないことと、`fsm.rs` のモジュール文書の「Timers … are not part of it」を、表と判定はあるが発火は段6の port の後である、という正確な文へ直すことが条件である。

後追いを別件として切る。段6（#257）が入ったら、発火の要求の出口を Scheduler の port へ差し替え、そのときに FSM の内側からのタイマの公開 API を足して、FSMTimingSpec と FSMActorSpec へ進む。kernel の表と判定は作り直さずに済む形にしてある。

範囲の外を1つ記録する。FSMActorSpec も台帳で `test-fsm-ref` を要求している。そこでは FSM の内側（遷移ハンドラの中）から単一タイマを開始し、状態ごとのタイムアウトも使う。ハンドルが見せるタイマは、誰が張ったかによらず kernel の表を写す形にする。ただし、FSM の内側からのタイマの API そのものは本件では作らず、上の後追いの件で扱う。

### 3.5 `setState` の `stopReason` と `onTermination`

原典の `setState` は `stopReason: Option[FSM.Reason]` も取るが、本 Spec のケースは使っていない。今の `Fsm` には `onTermination` も停止理由の処理も無い。本件では `stopReason` の引数は作らず、ケースが使う範囲（状態名、データ、タイムアウト）に限る。停止理由で止める振る舞いは、本体の `Fsm` に停止の意味論が入るときに一緒に扱う。これはケースの除外ではなく、本体の未移植部分の切り分けである。

`set_state` と前の状態タイマの関係は、原典の実装と文書で食い違う。原典の `TestFSMRef.setState` は `receive` を通らずに `applyState` を直接呼ぶので、前の状態タイマを取り消さず、世代も上げない。そのため、タイムアウト付きの `setState` を2回続けると、同じ世代の印が2つ発火し、`StateTimeout` が2回届く。一方で原典の API 文書は「機械の内側から起きた遷移と等価」と約束している。本件は文書の約束のほうを採る。制御メッセージの `set_state` は、通常のイベントと同じく、前の状態タイマを取り消して世代を上げてから適用する。この選択は、原典の実装の癖を写さない意図した違いとして、ここに記録する。

### 3.6 no_std で書けるか

- fraktor-actor 側の追加（ハンドル、タイマの表、制御メッセージ、`StateTimeout`）は `core` と `alloc` と、kernel がすでに使う `spin::Mutex`（`kernel::sync::lock`）だけで書ける。`std` の import は足さない。時刻は `core::time::Duration` で表し、実時間の計測は持たない（期限の管理は案Aでは testkit 側、案Bでは scheduler の port 側にある）。
- fraktor-testkit 側は、もともと `std` のクレートである（`FraktorSpec` が `std::time::Instant` などを使う）。案Aの期限スレッドも `std` の範囲で書ける。
- ハンドルのスナップショットは kernel の `Arc` と `spin::Mutex` で持ち、スレッド境界を越えられる（`S` と `D` に既存の `Send` の境界がある）。

## 4. 守る性質（不変条件）

| 番号 | 不変条件 |
|:---:|:---|
| I1 | スナップショットの一致：ハンドルの状態名とデータは、機械が最後に完了した遷移の後の `current` と等しい。スナップショットは、`attach_handle` と `start_with` の時点で開始状態を書き、`make_transition` が `current` を置いたら通知の有無によらず書く（`stay().using(d)` でデータだけが変わる場合を含む）。書かないのは、移る先に状態関数が無いときだけである |
| I2 | `set_state` の既定：`None` の要素は今の値を保つ。状態名だけを変えてもデータは変わらず、データだけを変えても状態名は変わらない（ケース1が両方向を見る） |
| I3 | `set_state` の遷移の扱い：状態の設定は `goto(状態名.unwrap_or(今の状態)).using(データ.unwrap_or(今のデータ))` と等価で、常に通知する。同じ状態への設定でも、遷移ハンドラと購読者に遷移が届く（原典の `State` の `notifies` の既定が真であるため）。遷移は通常と同じ経路（`make_transition`）で起きる。移る先に状態関数が無い場合は、通常の移動と同じく、状態は変わらない |
| I4 | タイマの表：名前ごとにタイマは最大で1つである。同じ名前の開始は置き換えである。`is_timer_active(name)` は、開始の後で真、取り消しと（単一タイマの）発火の後で偽である。存在しない名前の取り消しは表を変えない |
| I5 | 状態タイマは受信を単位に扱う。状態タイマは最大で1つである。通常のメッセージと、受け付けた名前付きタイマの印を受信するたびに、張ってある状態タイマを取り消し、世代を1つ上げる（原典の `FSM.receive` の意味論。購読の系統と ListenerTerminated と TimeoutMarker の受信では、どちらもしない）。そのうえで、遷移が次の状態のタイムアウト（明示の値、無ければその状態の既定）を持つときだけ張り直す。したがって、`stay()` で受けたメッセージの後では、状態が同じでも状態タイマは残らない。TimeoutMarker の受信は世代の一致だけを見て、世代を上げず、状態タイマの枠も空にしない。そのため、発火の直後は、次の通常のメッセージが来るまで `is_state_timer_active()` は真のままである（原典の癖を写す。FSMActorSpec がこの値を読むため） |
| I8 | 古い印は処理されない：取り消したタイマ、置き換えたタイマ、世代がずれた状態タイマの印は、届いてもイベントとして処理されない |
| I9 | 単一タイマは、受け付けた後で表から消える。繰り返しのタイマは表に残る |
| I10 | 停止したらタイマの表は空になる。状態タイマも消え、ハンドルのスナップショットも空に戻る。再起動した個体は、空の表と開始状態から始まる |
| I6 | 制御と利用者のメッセージの順序：制御メッセージはメールボックスの中で送った順に処理される。利用者のメッセージの合間に送った `set_state` は、その前後のイベントに対して送った順の位置で効く |
| I7 | ハンドル無しの機械の不変：ハンドルを有効にしない `Fsm` の振る舞いは、既存のテスト（`fsm_transition_spec` など）が見る範囲で変わらない |

## 5. 壊しうる入力

| 入力 | 何が起きうるか | 扱い |
|:---|:---|:---|
| 空文字のタイマ名 | 原典は名前を検証しないので、空文字も1つの名前として動く | 拒否しない。空文字をキーとして表に入れる。テストに1件足す（原典外） |
| 空文字のアクター名（ケース4と6） | 既存の名前の検証（`actor_of_named`、つまり `try_actor_of` の経路）が拒否する | `Err` を返す。原典は例外を投げるので、対応する失敗として扱う。`with_name` は、名前付き生成を1本化する [PR #281](https://github.com/j5ik2o/fraktor-ng-rs/pull/281)（#278）のマージの後に、その経路へ乗せる |
| ワイルドカードを含む名前 | アクター名の検証は既存の経路の仕事である。タイマ名にはワイルドカードの意味論は無い（完全一致のキーである） | タイマ名は展開や照合をしない。連続したワイルドカード文字もただの文字列として扱う |
| 非常に長いタイマ名・状態名 | 確保は文字列の長さに比例する | 上限は設けない（原典も設けない）。PBT の抽選に長い文字列を含める |
| タイムアウト `Duration::ZERO` | 期限が即時になる | 状態タイマは次の機会に発火する。テストでは `await_cond` で届くことを確かめる |
| タイムアウト `Duration::MAX` に近い値 | 期限の加算があふれうる | 期限は飽和加算で持ち、あふれた場合は「事実上発火しない」として扱う。panic しない |
| 開始前の `state_name` の読み取り（`start_with` の前） | 原典では構築の失敗に当たる | ハンドルは `None` を返し、`TestFsmRef::state_name` は理由を書いて panic する（原典の例外に対応する失敗として、API 文書に書く） |
| 存在しない状態への `set_state` | I3 のとおり、状態は変わらない | 既存の `make_transition` の意味論に合わせ、エラーにはしない |
| 存在しないタイマの取り消し、同じ名前の再開始 | I4 のとおり | 何もしない、置き換える。どちらも panic しない |
| 停止したアクターへの `set_state` とタイマ操作 | 制御メッセージは dead letters へ行き、スナップショットは変わらない | 例外にしない。テスト側は `await_cond` の時間切れで気づく |
| 印が送られてから届くまでの間に、取り消し・置き換え・通常の受信が割り込む | 古い印が届く | 受信の中で世代を見て捨てる（I8）。Quint のモデルでも、発行と受信を分けてこの割り込みを置く（§6） |
| 状態タイマを張った後に、`stay()` で受けるイベントが来る | 原典では状態タイマが取り消され、張り直されない | I5 のとおり受信で取り消す。ケース1の形（張った後に何も来なければ発火する）と合わせて、PBT のモデルにも入れる |
| `initialize()` を呼ばない機械 | 開始状態がスナップショットに入らないと、最初の読み取りで panic する | `attach_handle` と `start_with` の時点で書く（I1） |
| 状態関数が panic して再起動する | ハンドルが止まった個体の状態を返し続ける | factory が作るたびに同じハンドルを付け直し、開始状態で書き直す（§3.3） |
| 状態関数が `stay().using(d)` を返す | 通知が無いので、スナップショットの書き忘れが起きうる | I1 のとおり、`current` を置いたら通知の有無によらず書く |
| 同じ状態への `set_state` に購読者が付いている | 通知しない形に写すと `Transition(1, 1)` が届かない | I3 のとおり常に通知する |
| 途中の状態が最後の状態と同じ値になる操作列（例：1、2、1） | 値の一致で待つと、途中で条件が成り立ち、残りの操作を落とす実装でも通る | 通し番号が最後まで進むことを待つ（§3.1） |
| 状態関数が `context.change_behavior` で振る舞いを替える | それ以降の制御メッセージは `Fsm::receive` に届かず、失われる | 本件のケースには関係しない。制御メッセージを使う機械では振る舞いを替えないことを、API 文書に書く |

## 6. PBT と Quint

### PBT は足す

原典外の性質テストを `crates/fraktor-testkit/tests/test_fsm_ref_property_test.rs` に足す。依存は足さず、既存の移植（#177 など）と同じく固定 seed の SplitMix64 で抽選する（`proptest` の採否は [Issue #212](https://github.com/j5ik2o/fraktor-ng-rs/issues/212) の判断に従う）。

| 番号 | 性質 | 確かめる不変条件 |
|:---:|:---|:---|
| P1 | ランダムな操作列（イベント送信、`set_state`、タイマの開始と取り消し、`stay().using` を含む）の後で、通し番号が最後まで進むのを待った後のスナップショットが、同じ操作列を単純なモデルで追った状態名とデータに等しい。モデルは原典の意味論で追う（すべての通常の受信で状態タイマを取り消すこと、`set_state` は常に通知すること、`stay().using` でデータが変わること）。操作列の最後には、番号付きの番兵（何もしない制御メッセージ）を送ってから待つ。メールボックスが FIFO なので、番兵より前の利用者のメッセージ（番号が付かないもの）も処理済みになる | I1、I2、I3、I5、I6 |
| P1 の範囲の注意 | P1 の操作列で `set_state` に渡すタイムアウトは、`None` か十分に長い値に限る。状態タイマの発火は実時間に依存し、ランダムな操作列のモデルでは発火したかを当てられないためである。I5（受信で状態タイマを取り消すこと）は、決まった手順の固定テストで見る。長いタイムアウトで状態を設定し、`stay()` で受けるメッセージを送り、番兵で待ってから、状態タイマが有効でないことを確かめる | I5 |
| P2 | 任意の名前の列（空文字、長い文字列、重複を含む）について、開始と取り消しの後の `is_timer_active` が、名前の集合のモデルと等しい | I4 |
| P3 | 任意のタイムアウト値（`ZERO`、境界、非常に大きい値を含む）で `set_state` しても panic せず、状態名とデータは指定どおりになる | I2、§5 |

### Quint は書く

判断：書く。理由は、`set_state` がイベント処理を通らない外部からの状態の書き換えであり、既存の `crates/fraktor-actor/specs/fsm_transition.qnt`（#194）はイベントによる遷移だけをモデルにしていて、外部からの書き換えとタイマの表と状態タイマの世代を取りこぼすためである。固定のテストは I4 と I5 の列の性質（置き換え、取り消し、古い世代の発火の抑止）を狭い範囲でしか見ない。

範囲は小さく絞る。仕様が扱う状態は kernel の `Fsm` のものなので、置き場所は既存の `fsm_transition.qnt` と並ぶ `crates/fraktor-actor/specs/test_fsm_ref.qnt` の1ファイルだけとする。CI の検査スクリプト（`scripts/quint-check-specs.sh`）はファイルを1本ずつ確かめ（typecheck、`Test` で終わる run の存在と全件の通過、`invariant` での模擬実行で歩数が1を超えること）、2ファイルに分けると、run が無い側と init や step が無い側が落ちるためである。書き方は CI の決まり（`docs/design/quint-in-ci.md`）に合わせ、不変条件は `val invariant` を1つにまとめ、`run` の名前は `Test` で終え、負の対照は `run bug…Test` で `.expect(not(...))` を期待する形にする（`fsm_transition.qnt` の前例どおり。バグの遷移自体は実行できて不変条件を破るため）。

- 状態：今の状態名とデータ、タイマの表（名前から、世代と繰り返しの区分と個体 ID への対応）、状態タイマの有無と世代、機械の世代、届く前の印の集合（名前付きタイマの印は (名前, 世代, 個体 ID)、状態タイマの印は世代）
- 表の個体 ID は、モデルの判定用の単調な識別子である（実装の世代とは別に持つ）。世代が衝突する誤り（行ごとに振り直す形）では、古い印と新しい行の世代が同じ値になり、世代だけでは古い印の誤受理を不変条件で区別できないためである。受信の判定は世代だけで行い、不変条件は、受け付けた印の個体 ID が今の行の個体 ID と一致すること（古い印を受け付けていないこと）を確かめる。表の行には繰り返しの区分も持たせ、単一タイマが受け付けの後で消えること（I9）を表せるようにする。再起動の遷移（`restart`。表を空にし、開始状態へ戻し、世代の数えは戻さない）も遷移に含める
- 遷移：`sendEvent`（受信で状態タイマを取り消し、世代を上げる）、`setState`（要素の `Option` を非決定的に選ぶ。通常の受信と同じく取り消しと世代の更新をしてから適用する）、`startTimer`、`cancelTimer`、印の発行（`timerDue`）と受信（`receiveMarker`）を分けた2つの手。発行と受信の間に `sendEvent`、`setState`、`cancelTimer` が割り込める
- 不変条件：`snapshotMatches`（I1）、`oneTimerPerName`（I4）、状態タイムアウトが処理されるのは、張ってから通常のイベントを受けていないときだけであること（I5）、取り消し・置き換えたタイマの印はイベントとして処理されないこと（I8）、単一タイマは受け付けた後で表から消えること（I9）
- 遷移のうち `receiveMarker` は、名前付きタイマの印を受け付けたときにも、通常の受信と同じく、状態タイマを取り消して機械の世代を上げる（I5）
- 負の対照：世代の判定を落とした `bugStaleStateTimer`、取り消しを落とした `bugCancelKeepsTimer`、送る側で判定し受信では判定しない `bugSenderSideCheck`、遷移のときだけ世代を上げる `bugGenerationOnTransitionOnly`、名前付きタイマの世代を行ごとに振り直す `bugPerEntryGeneration` を置き、それぞれ狙った不変条件が落ちることを確かめる

モデルにしないもの：実時間そのもの（期限は「発火してよいか」の真偽で表す）、ハンドラと購読者への通知の順序（既存の `fsm_transition.qnt` の範囲）、名前の文字列の中身（集合の要素としてだけ扱う）。

## 7. 第2段の進め方（この設計が通ってから）

1. red：`crates/fraktor-testkit/tests/test_fsm_ref_spec.rs` に6件を書き、台帳の対象ノードを red にする。失敗が「型が無い」ことによることを確かめる。
2. 本体：§3 の順に、fraktor-actor のハンドルと制御メッセージとタイマの表、fraktor-testkit の `TestFsmRef` を足す。新しいファイルには、自作のものも含めて Apache-2.0 のヘッダーを付ける。タイマの発火は §3.4 で確定した形で実装する。新しい関数や公開 API を足す前に、同じ働きの既存の関数を探し、並行の別の PR と同じ口を足していないかも確かめる（本件では `try_spawn_child` について確かめた記録が §3.3 にある）。レビューの経過をコードのコメントに書かない。理由は設計の文書と PR に書く。
3. green：6件を通し、台帳の対象ノードを green（6/6/6、`verified_against: v2.0.0`）にする。ケース1の状態タイムアウトが案の確定待ちで保留になった場合は、その時点で止めて報告する（台帳を green にしない）。
4. 検査：CI と同じ検査を流す（`cargo fmt --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`、ledger/tools の Python テスト、Quint の typecheck と run と負の対照、syn チェッカー）。加えて、dev-dependency を引かない `cargo clippy -p fraktor-actor --lib -- -D warnings` を流す。ワークスペースの検査は dev-dependency 経由で `test-support` が有効になってしまうため、feature を切った形（既定の公開 API に testkit 向けの面が出ていないこと）は、この1本で確かめる。
5. 多重スレッドの実行器（#234）との関係：本件の6件は、段7b（#259）の CallingThreadDispatcher を前提にしない。注1の同期性だけが段7b の後で直線の形に戻せるもので、本件の green を妨げない。

## 8. レビューで見てほしい点

- 本体を返さず、共有ハンドル（複製）と制御メッセージで表す形（§3）が、所有権モデルの決定と #182 の土台の共有化の進め方として妥当か
- `D: Clone` の制約（`attach_handle` にだけ付ける）を、`state_data` が複製を返すことの代償として妥当と見るか
- kernel に足す2つの口（feature `test-support` の下に置く testkit 向けの面一式。制御メッセージと `FsmHandle` と `attach_handle` と発火の記録を含む。と、`WeakActorSystem::try_spawn_child`）を妥当と見るか
- `set_state` と前の状態タイマの関係で、原典の実装の癖ではなく文書の約束を採ったこと（§3.5）
- 発火の直後に `is_state_timer_active()` が真のままになる原典の癖を写すこと（§4 の I5）
- `stopReason` と `onTermination` を本件の範囲の外に置くこと（§3.5）
- Quint の範囲（§6）が、既存の `fsm_transition.qnt` と重ならず、足りているか

タイマ発火の案Aと案Bは、§3.4 のとおり案A（kernel に表・世代・受信の中での判定を置き、発火だけを testkit が担う形）で決まった。

## 9. 設計レビュー（PR #275）への回答表

レビューレポート `pr275-design-review.md`（判定：要修正、高5件・中7件・低6件）の指摘ごとに、扱いと直した場所を書く。高の4件目（状態タイマの意味論）は、原典の `FSM.scala`（v2.0.0）の `receive` と `makeTransition` と `terminate` を読み直して確かめたうえで直した。

| 指摘 | 扱い | 内容と直した場所 |
|:---:|:---:|:---|
| 高1 | 直す | 開始状態がスナップショットに入らない問題。`attach_handle` と `start_with` の時点で開始状態を書く形に変えた（§3.1、§4 の I1、§5） |
| 高2 | 直す | `TestFsmRef` は `Fsm` の値ではなく factory を受け取り、ハンドルは `TestFsmRef` が1つ作って作るたびに付け直す形に変えた。再起動でも同じハンドルが新しい個体に付く（§3.1、§3.3） |
| 高3 | 直す | 制御メッセージの公開経路を、cargo の feature `test-support` の下でのみ `pub` にする形に決めた（§3.1） |
| 高4 | 直す | 状態タイマは遷移ではなく受信を単位に扱う形へ、I5 と P1 と Quint を書き直した（§4 の I5、§5、§6）。`set_state` と前の状態タイマの関係は、文書の約束のほうを採る選択を §3.5 に記録した |
| 高5 | 直す | 発火は世代を載せた印で届け、判定は受信の中で行う形に変えた。単一タイマは受け付けた後で表から消える（§3.2、§3.4、§4 の I8 と I9） |
| 中1 | 直す | ケース3〜6で、パス（親と名前）の確認を必須にした。加えて `pre_start` でハンドルへ書かれた `self_ref` からも名前を確かめる（§3.3） |
| 中2 | 直す | kernel に `try_spawn_child`（`Result` を返す）を足す形に決め、既存の口と並行の PR を確かめた記録と、渡す監督戦略（既定）を書いた（§3.3） |
| 中3 | 直す | `set_state` は常に通知する、と I3 を書き直した（§4 の I3） |
| 中4 | 直す | ハンドルに処理を終えた制御メッセージの通し番号を持たせ、最後の番号まで待つ形にした（§3.1、§1 の注1、§5） |
| 中5 | 直す | `stay().using(d)` でもスナップショットを書く、と I1 を書き直し、P1 の操作列にも入れた（§4 の I1、§6） |
| 中6 | 直す | 制御メッセージのうち状態タイマと世代に触れるのは状態の設定だけ、と §3.1 に書いた。停止と再起動での表の後始末も §3.2 と I10 に書いた |
| 中7 | 直す | Quint のモデルで印の発行と受信を分け、届く前の印の集合と、割り込みの手と、不変条件3件と負の対照2件を足した（§6） |
| 低1 | 直す | ハンドルの共有は kernel の `Arc` と `spin::Mutex` を使う、と §3.1 に書いた |
| 低2 | 直す | Quint の置き場所を `crates/fraktor-actor/specs/` へ移し、CI の決まりに合わせた書き方を §6 に書いた |
| 低3 | 直す | 発火の直後は次の通常のメッセージまで `is_state_timer_active()` が真のままになる原典の癖を写す、と I5 に決めた |
| 低4 | 直す | FSMActorSpec との関係を §3.4 の範囲の外に書いた。ハンドルは kernel の表を写し、FSM の内側からのタイマの API は後追いの件で扱う |
| 低5 | 直す | 同期性の注をケース2にも広げ、待ちを通し番号でも待てる形にした（§1 の注1） |
| 低6 | 直す | `change_behavior` で振る舞いを替えると制御メッセージが失われることを、§5 の表に足した |
| 案A／案B | 決まった | 案A（kernel に表・世代・受信の中での判定を置き、発火だけを testkit が担う形）で決まった（中将の判断）。案Bは段6（#257）の Scheduler の port と二重になるため採らない（§3.4） |
| 後追い | 別件 | 段6（#257）の後に、発火の出口を Scheduler の port へ差し替え、FSM の内側からのタイマの公開 API を足す件を、別件として切る（§3.4） |

採らないとした指摘は無い。

### 再レビュー（PR #275、HEAD 247de68）への回答

再レビューレポート `pr275-design-rereview.md`（判定：要修正（軽い）、高1件・中3件・低5件）への回答である。前回の指摘はすべて直っていることが確認された。

| 指摘 | 扱い | 内容と直した場所 |
|:---:|:---:|:---|
| 高1 | 直す | 通し番号は `TestFsmRef` が送るときに振って制御メッセージに載せて運び、ハンドルは処理した最大値を保つ形に変えた。発火の印は番号を持たず進めない。番号は再起動と空に戻す操作をまたいで保つ。P1 は最後に番号付きの番兵を送って待つ形にした（§3.1、§1 の注1、§6 の P1） |
| 中1 | 直す | 名前付きタイマの世代は、機械全体で単調に増える1本の数え（共有のハンドルに置き、再起動をまたぐ）から振る形に決めた。Quint の表を名前から世代への対応にし、印は (名前, 世代) を持ち、負の対照に `bugPerEntryGeneration` を足した（§3.2、§6） |
| 中2 | 直す | `FsmHandle` とその読み取りの口、`attach_handle`、発火の記録まで、testkit 向けの面をすべて `test-support` の下に置く形に決め、置き方（`pub(crate)` の型を `test_support` モジュールで `pub use` する）を書いた。第2段の検査に、feature を切った形の clippy を1本足した（§3.1、§7） |
| 中3 | 直す | 子の生成の経路にも、トップレベルと同じ設定の検証を通して `ActorCreationError` を返す形に決めた。名前の無い生成は本件の口に無く、親が生きていない場合は `Err` を返すことも書いた（§3.3） |
| 低1 | 直す | §5 の名前の検証の経路を `actor_of_named`（`try_actor_of`）に直し、`with_name` は PR #281 のマージの後にその経路へ乗せることを書いた（§5） |
| 低2 | 直す | 状態の設定の入り口の引数を `Option<S>` と `Option<D>` にし、kernel の中で今の値に置き換えてから通知する形に直した（§3.1） |
| 低3 | 直す | P1 のタイムアウトを `None` か十分に長い値に限り、I5 は決まった手順の固定テストで見る形にした（§6） |
| 低4 | 直す | 世代のコメントを名前付きタイマ用に直し、包んだメッセージは表の行だけが持ち、印は (名前, 世代) だけを持つ形にした（§3.1、§3.2） |
| 低5 | 直す | Quint の `receiveMarker` が、名前付きタイマの印を受け付けたときにも状態タイマを取り消して世代を上げることを、遷移の説明に明記した（§6） |

採らないとした指摘は無い。

### 3回目の確認（PR #275、HEAD 0150c98）への回答

3回目の確認レポート `pr275-design-rereview2.md`（判定：要修正（軽い））への回答である。高の1件目（公開の置き方）は、一時ディレクトリで rustc と cargo を流して、通る形を確かめてから直した。中2件目（Quint の構成）は、CI の検査スクリプト `scripts/quint-check-specs.sh` と既存の `fsm_transition.qnt` を手元で読み直して確かめた。

| 指摘 | 扱い | 内容と直した場所 |
|:---:|:---:|:---|
| 高1 | 直す | `pub(crate)` の型を `pub use` で再公開する形をやめた。`test_support` を子モジュールを持つ非末端モジュールとして feature の下で出し、中身は非公開の子モジュールに `pub` で置いて直属だけを再公開する形にし、`attach_handle` はメソッド自体を feature で囲い、ハンドルの保持と制御メッセージの分岐も feature で囲って未使用の警告を避ける形に決めた。一時ディレクトリで確かめた結果（feature を切ると外からは到達できず、自クレートは警告なし。入れると外から使える）を §3.1 に記録した |
| 中1 | 直す | `try_spawn_child` の名前の引数を `Option<&str>` に揃え、`spawn_child` は名前ありと名前なしの両方を委譲する形に直した。親の停止とシステムの終了は `Err` で返すことも書いた。`ActorContext::actor_of` は戻り値の型を変えず、設定の検証の誤りは panic する形にして文書に書くことを §3.3 に明記した |
| 中2 | 直す | Quint は1ファイル構成に直し、負の対照の期待を `.expect(not(...))` に揃えた。表に繰り返しの区分と判定用の個体 ID を持たせ、印にも個体 ID を載せ、不変条件で古い印の誤受理を区別できる形にした。再起動の遷移も足した（§6） |
| 低1 | 直す | §3.2 の矛盾していた文から「と包んだメッセージ」を削り、受信時に表の行からメッセージを取り出す形に揃えた（§3.2） |
| 低2 | 直す | 状態タイマの世代（機械の世代）も再起動で戻さず、共有のハンドル側で進めることを §3.2 に明記した |
| 低3 | 直す | §3.1 の制御メッセージの一覧に番兵を足し、§8 の確認点を testkit 向けの面一式に揃えた（§3.1、§8） |

採らないとした指摘は無い。
