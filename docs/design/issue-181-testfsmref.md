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

注1（ケース1の同期性）：原典は `fsm ! "go"` の直後の行で `stateName` を読む。原典の `TestActorRef` は CallingThreadDispatcher で動くので、送信がその場で処理される。今の fraktor-ng-rs の実行器では送信はキューに入り、待ちの運転手が処理するので、直後に読むと古い状態が見えうる。移植のテストは、送信の後で `awaitCond` に当たる `FraktorSpec::await_cond` で状態が変わるまで待ってから確かめる形にする。送信したその場で処理されるという強い同期性そのものを確かめる形への書き戻しは、testkit の CallingThreadDispatcher（[Issue #234](https://github.com/j5ik2o/fraktor-ng-rs/issues/234) の段7b、[Issue #259](https://github.com/j5ik2o/fraktor-ng-rs/issues/259)）の後とする。これはケースの除外ではなく、待ち方の違いである。アサーションの内容（状態名と状態データの遷移）は弱めない。

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

- ハンドルは `Fsm::with_handle()` で有効にする形を提案する。`Fsm::new()` で作った通常の機械はハンドルを持たず、既存の利用者にコストと制約を足さない。`Props` の factory の中で `let handle = fsm.handle()` のように取り出し、factory の外側へ複製を渡す。これは既存の移植テストが `Arc<AtomicI32>` などを factory へ持ち込んで観測している形と同じである。
- ハンドルが状態データの複製を返すため、`D: Clone` が要る。原典の `stateData` は参照を返すので、これは Rust 側の制約として API 文書に書く。`Clone` でないデータの機械は、ハンドル無しの通常の形で使う。
- スナップショットの更新は遷移の完了時点（`make_transition` が `current` を置いた直後）で行い、遷移しなかった場合（移る先の状態に状態関数が無い場合）は変えない。

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
  pub(crate) fn apply_state(&mut self, context: &mut ActorContext<'_>, next: FsmState<S, D>,
                            timeout: Option<Duration>, /* stop reason は §3.5 */);
}
```

制御メッセージの型は公開しない。`crates/fraktor-actor/src/kernel/actor/fsm_control.rs` に、状態の設定（状態名とデータとタイムアウトの `Option` の組）と、タイマの開始（3種）と取り消しを表す列挙型を1つ置き、`Fsm::receive` が通常のイベントより前に処理する。制御メッセージはメールボックスの中で利用者のメッセージと同列に並ぶので、順序は「送った順」に保たれる。テスト側が状態を読むのはハンドル経由なので、制御メッセージの処理完了そのものは、`await_cond` でスナップショットの変化を見て確かめる。

### 3.2 fraktor-actor（kernel）に足すタイマの表

`Fsm` の中に、名前からタイマへの表を持つ。

```rust
// crates/fraktor-actor/src/kernel/actor/fsm_timer.rs（形）
pub(crate) struct FsmTimer {
  name: String,            // 空文字も許す（§5）
  message: Message,        // 発火時に自分へ送るメッセージ
  kind: FsmTimerKind,      // Single / FixedDelay(Duration) / FixedRate(Duration)
  generation: u64,         // 状態タイマと単一タイマの取り消し判定に使う
}
```

- 同じ名前で開始すると、前のタイマを取り消して置き換える（原典の `setTimer` の意味論）。
- 存在しない名前の取り消しは、何もしない（原典の `cancelTimer` は存在しない名前でも例外を投げない）。
- `is_timer_active(name)` は表に名前があることと等しい。状態タイマは別の1枠で持ち、`is_state_timer_active()` が見る。
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

impl<S, D> TestFsmRef<S, D>
where S: Clone + PartialEq + Send + Sync + 'static, D: Clone + Send + 'static {
  /// 既定の supervisor（guardian）とランダムな名前で作る。原典の `TestFSMRef(factory)`。
  pub fn new(system: &mut ActorSystem, fsm: Fsm<S, D>) -> Self;
  /// 名前を指定して作る。原典の `TestFSMRef(factory, name)`。
  /// 名前が不正か使用中なら `Err`（原典は例外）。
  pub fn with_name(system: &mut ActorSystem, fsm: Fsm<S, D>, name: &str)
    -> Result<Self, ActorCreationError>;
  /// supervisor を指定して作る。原典の `TestFSMRef(factory, supervisor)`。
  /// `WeakActorSystem::spawn_child` で、その親の子として作る。
  pub fn with_supervisor(system: &mut ActorSystem, fsm: Fsm<S, D>, supervisor: &ActorRef) -> Self;
  /// supervisor と名前を指定して作る。原典の `TestFSMRef(factory, supervisor, name)`。
  pub fn with_supervisor_and_name(system: &mut ActorSystem, fsm: Fsm<S, D>,
    supervisor: &ActorRef, name: &str) -> Result<Self, ActorCreationError>;

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

ケース3〜6が原典で確かめる `underlyingActor.supervisor` と `underlyingActor.name` は、原典ではアクターが構築時に `context.parent` と `context.self.path.name` を自分のフィールドへ記録したものである。移植では、本体を返さないので、`TestFsmRef` が生成時に決まった supervisor と名前を保持して返す。値は生成の経路（guardian の子として作ったか、`spawn_child` で指定の親の子として作ったか）から決まるので、パスの親と名前とも一致する。テストは加えて `actor_ref` のパスからも同じ名前と親を確かめられる。この違い（アクター自身が記録した値ではなく、生成した側が保持する値を読むこと）は、ケースの主張（どの supervisor と名前で作られたか）を変えない。

ランダムな名前は、原典の `TestActorRef.randomName` と同じく、既存の名前と衝突しない生成名を使う。既存の生成名の仕組み（`actor_of` が採番する名前）に乗り、独自の採番は作らない。

### 3.4 タイマの発火をどうするか（レビューで確定してほしい点）

ケース2は発火を見ていない（開始して有効を確かめ、取り消して無効を確かめるだけである）。発火が要るのは、ケース1の状態タイムアウトだけである。発火の置き場所について、2案を出す。

- 案A（推奨）：発火は testkit 側に閉じる。`TestFsmRef`（std のクレート）が期限を持つスレッドを持ち、期限が来たら対象のアクターへ発火メッセージ（名前付きタイマはそのメッセージ、状態タイマは `StateTimeout`）を送る。状態タイマと単一タイマは、ハンドルの世代番号が張った時点から変わっていないときだけ送る（遷移していたら送らない。原典で遷移が状態タイマを取り消すことに対応する）。利点は、本体の `Fsm` と `ActorContext` に scheduler の口を足さずに済み、本件の範囲で閉じることである。欠点は、本体の `Fsm` を通常の `Props` で使ったときのタイマ発火が、依然として未移植のまま残ることである。この残りは隠さず、§6 の範囲の外に書く。
- 案B：`ActorContext` に scheduler の port を足し、`Fsm` 本体が発火までを持つ。利点は Pekko の `FSM` の意味論に本体ごと近づくことである。欠点は、scheduler の移植そのものが本件の範囲を超え、[Issue #234](https://github.com/j5ik2o/fraktor-ng-rs/issues/234) の実行器の段と調整が要ることである。

推奨は案Aである。理由は、本 Spec の6件が見るのは testkit 経由の振る舞いだけであり、案Bの基盤は本 Spec の green に必要な大きさを超えるためである。ただし、案Aは「testkit の外では発火しない」という意味論の狭めになりうるので、`docs/decisions/pekko-semantics-first.md` の判断（狭めは利用者への約束として文書に書かない、作りのずれは直す）に照らして、レビューで上司の判断を仰ぐ。案Bを選ぶ場合は、本件を「タイマの表とケース2、ケース3〜6」と「状態タイムアウト（基盤の後）」に分ける。

### 3.5 `setState` の `stopReason` と `onTermination`

原典の `setState` は `stopReason: Option[FSM.Reason]` も取るが、本 Spec のケースは使っていない。今の `Fsm` には `onTermination` も停止理由の処理も無い。本件では `stopReason` の引数は作らず、ケースが使う範囲（状態名、データ、タイムアウト）に限る。停止理由で止める振る舞いは、本体の `Fsm` に停止の意味論が入るときに一緒に扱う。これはケースの除外ではなく、本体の未移植部分の切り分けである。

### 3.6 no_std で書けるか

- fraktor-actor 側の追加（ハンドル、タイマの表、制御メッセージ、`StateTimeout`）は `core` と `alloc` と、kernel がすでに使う `spin::Mutex`（`kernel::sync::lock`）だけで書ける。`std` の import は足さない。時刻は `core::time::Duration` で表し、実時間の計測は持たない（期限の管理は案Aでは testkit 側、案Bでは scheduler の port 側にある）。
- fraktor-testkit 側は、もともと `std` のクレートである（`FraktorSpec` が `std::time::Instant` などを使う）。案Aの期限スレッドも `std` の範囲で書ける。
- ハンドルのスナップショットは `alloc::sync::Arc` と `spin::Mutex` で持ち、スレッド境界を越えられる（`S` と `D` に既存の `Send` の境界がある）。

## 4. 守る性質（不変条件）

| 番号 | 不変条件 |
|:---:|:---|
| I1 | スナップショットの一致：ハンドルの状態名とデータは、機械が最後に完了した遷移の後の `current` と等しい。遷移しなかったイベントと、移る先に状態関数が無い移動では、スナップショットは変わらない（既存の `make_transition` の意味論のまま） |
| I2 | `set_state` の既定：`None` の要素は今の値を保つ。状態名だけを変えてもデータは変わらず、データだけを変えても状態名は変わらない（ケース1が両方向を見る） |
| I3 | `set_state` の遷移の扱い：状態の設定は、機械の内側から起きた遷移と等価である。遷移ハンドラと購読者への通知は、通常の遷移と同じ経路（`make_transition`）で起きる。移る先に状態関数が無い場合は、通常の移動と同じく、状態は変わらない |
| I4 | タイマの表：名前ごとにタイマは最大で1つである。同じ名前の開始は置き換えである。`is_timer_active(name)` は、開始の後で真、取り消しと（単一タイマの）発火の後で偽である。存在しない名前の取り消しは表を変えない |
| I5 | 状態タイマは最大で1つである。新しい状態へ移るたびに、前の状態タイマは無効になる（世代番号で判定し、古い期限の発火は届けない）。`set_state` で張った状態タイマは、その状態にいる間だけ有効である |
| I6 | 制御と利用者のメッセージの順序：制御メッセージはメールボックスの中で送った順に処理される。利用者のメッセージの合間に送った `set_state` は、その前後のイベントに対して送った順の位置で効く |
| I7 | ハンドル無しの機械の不変：ハンドルを有効にしない `Fsm` の振る舞いは、既存のテスト（`fsm_transition_spec` など）が見る範囲で変わらない |

## 5. 壊しうる入力

| 入力 | 何が起きうるか | 扱い |
|:---|:---|:---|
| 空文字のタイマ名 | 原典は名前を検証しないので、空文字も1つの名前として動く | 拒否しない。空文字をキーとして表に入れる。テストに1件足す（原典外） |
| 空文字のアクター名（ケース4と6） | 既存の名前の検証（`actor_of_with_name` の経路）が拒否する | `Err` を返す。原典は例外を投げるので、対応する失敗として扱う |
| ワイルドカードを含む名前 | アクター名の検証は既存の経路の仕事である。タイマ名にはワイルドカードの意味論は無い（完全一致のキーである） | タイマ名は展開や照合をしない。連続したワイルドカード文字もただの文字列として扱う |
| 非常に長いタイマ名・状態名 | 確保は文字列の長さに比例する | 上限は設けない（原典も設けない）。PBT の抽選に長い文字列を含める |
| タイムアウト `Duration::ZERO` | 期限が即時になる | 状態タイマは次の機会に発火する。テストでは `await_cond` で届くことを確かめる |
| タイムアウト `Duration::MAX` に近い値 | 期限の加算があふれうる | 期限は飽和加算で持ち、あふれた場合は「事実上発火しない」として扱う。panic しない |
| 開始前の `state_name` の読み取り（`start_with` の前） | 原典では構築の失敗に当たる | ハンドルは `None` を返し、`TestFsmRef::state_name` は理由を書いて panic する（原典の例外に対応する失敗として、API 文書に書く） |
| 存在しない状態への `set_state` | I3 のとおり、状態は変わらない | 既存の `make_transition` の意味論に合わせ、エラーにはしない |
| 存在しないタイマの取り消し、同じ名前の再開始 | I4 のとおり | 何もしない、置き換える。どちらも panic しない |
| 停止したアクターへの `set_state` とタイマ操作 | 制御メッセージは dead letters へ行き、スナップショットは変わらない | 例外にしない。テスト側は `await_cond` の時間切れで気づく |

## 6. PBT と Quint

### PBT は足す

原典外の性質テストを `crates/fraktor-testkit/tests/test_fsm_ref_property_test.rs` に足す。依存は足さず、既存の移植（#177 など）と同じく固定 seed の SplitMix64 で抽選する（`proptest` の採否は [Issue #212](https://github.com/j5ik2o/fraktor-ng-rs/issues/212) の判断に従う）。

| 番号 | 性質 | 確かめる不変条件 |
|:---:|:---|:---|
| P1 | ランダムな操作列（イベント送信、`set_state`、タイマの開始と取り消し）の後で、待ち合わせた後のスナップショットが、同じ操作列を単純なモデルで追った状態名とデータに等しい | I1、I2、I6 |
| P2 | 任意の名前の列（空文字、長い文字列、重複を含む）について、開始と取り消しの後の `is_timer_active` が、名前の集合のモデルと等しい | I4 |
| P3 | 任意のタイムアウト値（`ZERO`、境界、非常に大きい値を含む）で `set_state` しても panic せず、状態名とデータは指定どおりになる | I2、§5 |

### Quint は書く

判断：書く。理由は、`set_state` がイベント処理を通らない外部からの状態の書き換えであり、既存の `crates/fraktor-actor/specs/fsm_transition.qnt`（#194）はイベントによる遷移だけをモデルにしていて、外部からの書き換えとタイマの表と状態タイマの世代を取りこぼすためである。固定のテストは I4 と I5 の列の性質（置き換え、取り消し、古い世代の発火の抑止）を狭い範囲でしか見ない。

範囲は小さく絞る。置き場所は testkit の既存の仕様と同じ `crates/fraktor-testkit/specs/test_fsm_ref.qnt` とする。

- 状態：今の状態名とデータ、タイマの表（名前の集合）、状態タイマの有無と世代、機械の世代
- 遷移：`sendEvent`（既存の仕様の遷移の要約）、`setState`（要素の `Option` を非決定的に選ぶ）、`startTimer`、`cancelTimer`、`fireStateTimer`（世代が合うときだけ効く）
- 不変条件：`snapshotMatches`（I1）、`oneTimerPerName`（I4）、`stateTimerGeneration`（I5。古い世代の発火では状態が変わらない）
- 負の対照：世代の判定を落とした `bugStaleStateTimer`（古い状態タイマが遷移の後に効いてしまう）と、取り消しを落とした `bugCancelKeepsTimer` を置き、それぞれ狙った不変条件が落ちることを確かめる

モデルにしないもの：実時間そのもの（期限は「発火してよいか」の真偽で表す）、ハンドラと購読者への通知の順序（既存の `fsm_transition.qnt` の範囲）、名前の文字列の中身（集合の要素としてだけ扱う）。

## 7. 第2段の進め方（この設計が通ってから）

1. red：`crates/fraktor-testkit/tests/test_fsm_ref_spec.rs` に6件を書き、台帳の対象ノードを red にする。失敗が「型が無い」ことによることを確かめる。
2. 本体：§3 の順に、fraktor-actor のハンドルと制御メッセージとタイマの表、fraktor-testkit の `TestFsmRef` を足す。新しいファイルには、自作のものも含めて Apache-2.0 のヘッダーを付ける。タイマの発火は §3.4 で確定した案で実装する。
3. green：6件を通し、台帳の対象ノードを green（6/6/6、`verified_against: v2.0.0`）にする。ケース1の状態タイムアウトが案の確定待ちで保留になった場合は、その時点で止めて報告する（台帳を green にしない）。
4. 検査：CI と同じ検査を流す（`cargo fmt --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`、ledger/tools の Python テスト、Quint の typecheck と run と負の対照、syn チェッカー）。
5. 多重スレッドの実行器（#234）との関係：本件の6件は、段7b（#259）の CallingThreadDispatcher を前提にしない。注1の同期性だけが段7b の後で直線の形に戻せるもので、本件の green を妨げない。

## 8. レビューで見てほしい点

- §3.4 のタイマ発火の案Aと案Bのどちらにするか（本件の範囲と、意味論の狭めに当たるかの判断）
- 本体を返さず、共有ハンドル（複製）と制御メッセージで表す形（§3）が、所有権モデルの決定と #182 の土台の共有化の進め方として妥当か
- `D: Clone` の制約を、`state_data` が複製を返すことの代償として妥当と見るか
- ケース3〜6について、アクター自身が記録した値ではなく、生成した側が保持する supervisor と名前を読む形（§3.3）で、原典の主張を保てると見るか
- `stopReason` と `onTermination` を本件の範囲の外に置くこと（§3.5）
- Quint の範囲（§6）が、既存の `fsm_transition.qnt` と重ならず、足りているか
