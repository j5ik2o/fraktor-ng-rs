# 設計：#200 ReceiveTimeoutSpec（受信タイムアウトの本体と Spec の移植）

[Issue #200](https://github.com/j5ik2o/fraktor-ng-rs/issues/200) の設計の記録である。Pekko v2.0.0 の actor-tests にある actor の `ReceiveTimeoutSpec` を Rust へ移植し、台帳の対象ノードを green にする。本書は第1段（設計だけ）であり、実装とテストの作成と台帳の更新は、設計レビューが通ってから第2段で行う。本書の段では台帳を触らない。進め方の変更（オーナーの指示、2026-11-26）により、第2段ではテストを Claude Opus の担当が先に書いて red で置き、本書の担当は、そのテストを通す実装だけを書く（テストは書き換えない・弱めない・消さない）。

原典（Pekko v2.0.0）:

- [ReceiveTimeoutSpec.scala](https://github.com/apache/pekko/blob/v2.0.0/actor-tests/src/test/scala/org/apache/pekko/actor/ReceiveTimeoutSpec.scala)（352 行）
- 本体は [ReceiveTimeout.scala](https://github.com/apache/pekko/blob/v2.0.0/actor/src/main/scala/org/apache/pekko/actor/dungeon/ReceiveTimeout.scala) の `dungeon.ReceiveTimeout`（`ActorCell` の dungeon trait）と、[Actor.scala](https://github.com/apache/pekko/blob/v2.0.0/actor/src/main/scala/org/apache/pekko/actor/Actor.scala) の `ReceiveTimeout(timeout: FiniteDuration)` と `NotInfluenceReceiveTimeout` と `Identify`、[ActorCell.scala](https://github.com/apache/pekko/blob/v2.0.0/actor/src/main/scala/org/apache/pekko/actor/ActorCell.scala) の `receiveTimeout` と `setReceiveTimeout`、および Scala 2.13 版の `ReceiveTimeoutCompat.isNotInfluenceReceiveTimeout`（`Identify` か `NotInfluenceReceiveTimeout` のどちらかであれば真）である
- 依存の [Issue #180](https://github.com/j5ik2o/fraktor-ng-rs/issues/180)（TestLatch）は main に入っている。依存する issue はほかに無い

移植の規約は `.takt/facets/knowledge/fraktor-porting-rules.md` に従う。形の前例は [PR #205](https://github.com/j5ik2o/fraktor-ng-rs/pull/205) の `docs/design/issue-177-frequency-sketch.md` と `docs/goals/issue-177-frequency-sketch.md`、および `docs/design/issue-195-deployer.md`（HOCON を型付きの入力に写す忠実さの判断）である。方針は `docs/decisions/pekko-semantics-first.md`（速さより Pekko の意味論を優先する）に従う。Scala・Java の書き方の対応は `docs/decisions/scala-java-to-rust-correspondence.md`（[PR #282](https://github.com/j5ik2o/fraktor-ng-rs/pull/282) で main に入った）を引いた。同文書の Duration の無限の扱い（汎用の無限の代数を持つ型は作らず、使う場所ごとの型で表す）と、`docs/design/issue-185-util-specs.md` の注意（FSM と ReceiveTimeout では `Option<Duration>` を比較に使わず専用の enum にする）に従う。

結論を先に書く。**受信タイムアウトの本体は fraktor-ng-rs にまだ無い。** `crates/fraktor-actor` の中を `receive_timeout`、`ReceiveTimeout`、`NotInfluence`、`setReceiveTimeout` で探した結果は 0 件であり、`ActorContext` にも受信タイムアウトの口は無い。原典の15件のうち14件は、本書が設計する本体（セルの状態と世代、期限の検査を実行器の運転に組み込む形）を足せば今の1スレッドの実行器のまま移植できる。残る1件（Timers で予定したメッセージのケース）は、Pekko の `Timers` trait と `Scheduler` の port が前提であり、[Issue #234](https://github.com/j5ik2o/fraktor-ng-rs/issues/234) の段6（[Issue #257](https://github.com/j5ik2o/fraktor-ng-rs/issues/257)）の後として分ける（§9）。除外と作らない区分の提案は、ケース単位では無い。ケース3の中の Java API（`getTimeout` が `java.time.Duration` を返す部分）だけは、Rust に対応物が無い部分として §1 に注記する。決めるのはオーナーであるため、本書は提案として書くにとどめ、台帳には書かない。

## 1. 何を移植するか

### 件数の数え直し

原典の `" in {"` の数を数えた。依頼の目安は15件であり、数え直した結果も15件で、目安と一致する。台帳の `test_cases_approx: 2` は変換時の概数であり、実数とは違う。第2段で台帳の `cases.total` に15を記録する。`ignore` と `pending` は無い。14件に `TimingTest` のタグが付いている（ケース1とケース14には付いていない）。タグは ScalaTest の実行時の振り分けであり、Rust には対応物が無いため、移植ではタグを写さず、時間の値は原典のまま使う（§5）。

### 対応表

移す先はすべて `crates/fraktor-actor/tests/receive_timeout_spec.rs`（ノード1件＝ファイル1件の規約どおり）である。原典でアクターのコンストラクタの中に書かれている `context.setReceiveTimeout(...)` は、Rust では `Actor::pre_start` の中で `ActorContext` 経由で呼ぶ形に写す（コンストラクタに当たる場所が無く、`pre_start` が原典の生成直後の実行に当たるためであり、ほかの移植の前例と同じである）。

| 番号 | 原典のケース | Rust のテスト名 | 区分 |
|:---:|:---|:---|:---|
| 1 | classify messages that do not influence the timeout | `classifies_messages_that_do_not_influence_the_timeout` | 移す |
| 2 | get timeout | `gets_timeout` | 移す |
| 3 | carry the configured timeout duration | `receive_timeout_carries_the_configured_duration` | 移す（注1） |
| 4 | reschedule timeout after regular receive | `reschedules_timeout_after_regular_receive` | 移す |
| 5 | be able to turn off timeout if desired | `can_turn_off_timeout` | 移す |
| 6 | not receive timeout message when not specified | `does_not_receive_timeout_when_not_specified` | 移す |
| 7 | get timeout while receiving NotInfluenceReceiveTimeout messages | `gets_timeout_while_receiving_not_influence_messages` | 移す（注2） |
| 8 | get timeout while receiving only NotInfluenceReceiveTimeout messages | `gets_timeout_while_receiving_only_not_influence_messages` | 移す |
| 9 | get timeout while receiving NotInfluenceReceiveTimeout messages scheduled with Timers | （第2段の本書の範囲では名前を付けない） | #234 の段6の後（§9） |
| 10 | be able to turn on timeout in NotInfluenceReceiveTimeout message handler | `can_turn_on_timeout_in_not_influence_handler` | 移す |
| 11 | be able to turn off timeout in NotInfluenceReceiveTimeout message handler | `can_turn_off_timeout_in_not_influence_handler` | 移す |
| 12 | remove / change existing timeout while handling a NotInfluenceReceiveTimeout message | `removes_or_changes_timeout_while_handling_not_influence_message` | 移す |
| 13 | work correctly if the timeout is changed multiple times while handling a NotInfluenceReceiveTimeout message | `changing_timeout_multiple_times_in_not_influence_handler_uses_the_last_setting` | 移す |
| 14 | get the timeout when scheduled immediately on restart | `gets_timeout_when_scheduled_immediately_on_restart` | 移す |
| 15 | Will cancel receive timeout task if stopped | `cancels_receive_timeout_task_when_actor_stops` | 移す |

注1（ケース3の Java API の部分）: 原典は `msg.timeout should ===(500.milliseconds)` と `msg.getTimeout should ===(java.time.Duration.ofMillis(500))` の2つを確かめる。後者は Java の利用者向けの取り出し口であり、Rust には `java.time.Duration` が無い。移植では `ReceiveTimeout::timeout() -> core::time::Duration` の1つで両方を代表させる。これはケースの除外ではなく、ケースの中の1行についての「作らない」提案である。理由は、`docs/decisions/scala-java-to-rust-correspondence.md` の決まり（Pekko どおりに写せるものは写し、Rust で同じ振る舞いを作れず数から外す・弱めるときだけ伺う）により、観測できる振る舞い（設定した期間がメッセージに載ること）は Rust の取り出し口で同じく確かめられるためである。レビューで認められなければ、この行に対応する第2の取り出し口の要否を別途決める。

注2（ケース7の予定送信の写し方）: 原典は `system.scheduler.scheduleWithFixedDelay(100.millis, 100.millis)` で `TransparentTick` と `Identify(None)` を送り続ける。移植では、システムの scheduler を新しく作らず、testkit に既にある `ManualScheduler`（`crates/fraktor-testkit/src/manual_scheduler.rs`。Pekko の `ExplicitlyTriggeredScheduler` の移植）でテスト側が時刻を進めて送る形に写す。確かめる振る舞い（影響しないメッセージが届き続けても期限が来ること）は変わらない。システムの scheduler そのものを確かめるケースではないため、#234 の段6の後には回さない。ただし、この写し方を「原典の経路と違う」として認めない場合は、ケース7も段6の後へ移る。その判断は §10 の判断材料に挙げる。

### 原典の補助の型

- `Tick` は通常のメッセージ、`TransparentTick` は `NotInfluenceReceiveTimeout` を付けたメッセージである。移植ではテストファイルの中に2つの型を置き、`TransparentTick` には §3 の印の付け方を適用する。
- `RestartingParent` と `RestartingChild`（ケース14）と `StoppingSelfActor`（ケース15）は、テストファイルの中に置く。ケース14は親が子へ転送する形であり、`ActorContext` の転送は `pub(crate)` の `forward` しか無い点に注意が要る。第2段でテストが転送を必要とする場合は、親が受けたメッセージを `tell` で送り直す形（送信者は親に変わるが、原典のケース14は送信者を見ていない）か、転送の口を公開するかを、テストを書く担当と設計レビューで決める。本書は「送り直しで写す」を既定とする。
- ケース14の `EventFilter.error("boom bang", occurrences = 1)` は、testkit の `EventFilter` が既にあるため、そのまま写せる。

## 2. 本体の現状（fraktor-ng-rs にあるものと無いもの）

`crates/fraktor-actor` と `crates/fraktor-testkit` を探した結果は次のとおりである。

| 対象 | 現状 |
|:---|:---|
| `ReceiveTimeout` メッセージ、`NotInfluenceReceiveTimeout`、`setReceiveTimeout`、`receiveTimeout`、`cancelReceiveTimeout` | 無い。検索は0件である |
| `Identify` と `ActorIdentity`、自動受信（`AutoReceivedMessage`）の処理 | 無い。検索は0件である。ケース1は分類の判定だけを確かめるため、自動受信の返信までを本書の範囲には入れない（§3） |
| `Timers` trait（`startTimerWithFixedDelay` など） | 無い。`crates/fraktor-actor/src/kernel/actor/fsm.rs` のモジュール文書が「Timers, state timeouts, and onTermination are not part of it」と明記している |
| kernel の `Scheduler` の port | 無い。kernel が時刻に触れる port は `Clock`（`crates/fraktor-actor/src/kernel/clock.rs`。単調時計の `monotonic() -> Duration`）だけである。`Scheduler` の port の持ち主は #234 の段6（#257）である（#181 の設計の判断） |
| セル（`crates/fraktor-actor/src/kernel/executor/actor_cell.rs` の `ActorCell`） | ある。状態は生成・再起動・停止と振る舞いのスタックを持ち、受信タイムアウトの状態は持っていない。足す場所はここである |
| 実行器（`Executor`）の運転 | ある。1スレッドでシャードのキューを運転する。待ちの間にシャードを運転する仕組み（`crates/fraktor-actor/src/adaptor/ask_driver.rs` の wait driver）もあり、TestProbe と TestLatch の待ちが実行器を進める |
| testkit | `TestLatch`（#180 で main に入った）、`TestProbe`、`FraktorSpec`、`EventFilter`、`ManualScheduler` が既にある。台帳の対象ノードの `needs.testkit` に挙がっている `default-timeout`、`expect-msg`、`expect-no-message`、`test-probe`、`event-filter`、`test-latch` はすべて揃っている |

### 原典の本体の意味論（写す対象）

Pekko の `dungeon.ReceiveTimeout` は、セルごとに `State(timeout, task, version)` を1つ持つ。振る舞いは次のとおりであり、本書の設計はこの順序をそのまま写す。

1. `setReceiveTimeout(timeout)` は、状態が無ければ版（version）1で作り、あれば `timeout` を差し替えて版を1つ上げる。予定の張り直しはその場では行わない。
2. メッセージの処理の前に、状態があり、かつメッセージが `NotInfluenceReceiveTimeout` でなければ、張ってある予定を取り消す（`cancelReceiveTimeoutIfNeeded`）。この関数は処理の前の版を返す。
3. メッセージの処理の後に（`checkReceiveTimeoutIfNeeded`）、状態があるか版が変わっていれば、再予定の判定をする。再予定するのは、メッセージが影響するメッセージである場合か、処理の間に版が変わった（＝ハンドラが `setReceiveTimeout` を呼んだ）場合だけである。
4. 再予定の判定（`checkReceiveTimeout`）では、設定が有限の期間であれば、古い予定を取り消して `scheduler.scheduleOnce(timeout, self, ReceiveTimeout(timeout))` で張り直し、版を1つ上げる。設定が有限でなければ（`Duration.Undefined` と `Duration.Inf` を含む）、予定を取り消すだけである。有限の設定があり、予定がまだ張られていない場合は、再予定の条件が偽でも張る（生成直後と再起動直後のケースである。`ActorCell.create` の経路は `checkReceiveTimeout(reschedule = true)` を呼ぶ）。
5. 停止のときは予定を取り消す。取り消し済みの予定が後から発火しても、メッセージは届かない（原典は `Cancellable.cancel()` に頼る。移植では世代で同じことをする。§3）。

ケース12とケース13が確かめるのは、手順2と手順3の組み合わせである。影響しないメッセージの処理では予定は取り消されず、ハンドラが設定を変えたときだけ、処理の後の判定で新しい設定が効く。ケース13では、1回の処理の中で設定を2回変え、最後の値だけが効く。

## 3. 設計する本体

### 置き場所

| ファイル | 中身 |
|:---|:---|
| `crates/fraktor-actor/src/kernel/actor/receive_timeout.rs`（新規のモジュールの入口） | 再エクスポート |
| `crates/fraktor-actor/src/kernel/actor/receive_timeout/receive_timeout_message.rs` | `ReceiveTimeout` メッセージ |
| `crates/fraktor-actor/src/kernel/actor/receive_timeout/receive_timeout_setting.rs` | `ReceiveTimeoutSetting`（設定の enum） |
| `crates/fraktor-actor/src/kernel/actor/receive_timeout/not_influence_receive_timeout.rs` | 印の trait と分類の関数 |
| `crates/fraktor-actor/src/kernel/actor/receive_timeout/receive_timeout_state.rs` | セルが持つ状態（設定、世代、期限）。`pub(crate)` |
| `crates/fraktor-actor/src/kernel/actor/actor_context.rs` | `set_receive_timeout` と `receive_timeout` の2つの口を足す |
| `crates/fraktor-actor/src/kernel/executor/actor_cell.rs` | 状態を持ち、処理の前後の手順と停止時の取り消しを担う |
| `crates/fraktor-actor/src/kernel/executor/executor_value.rs` | 運転のたびに期限を検査し、期限が来たセルへ `ReceiveTimeout` を配送する |

モジュールの置き方は、既存の `kernel::actor` 配下の1ファイル1型の並び（`timeout` モジュールの `Timeout` の前例を含む）に従う。新しい関数や公開 API を足す前に同じ働きの既存の関数を探す決まりについては、§2 のとおり受信タイムアウトの口は存在せず、重なる口は無いことを確認した。ask の `Timeout`（`kernel::timeout::Timeout`）は ask の期限の型であり、受信タイムアウトの設定には流用しない。

### 公開 API の形

```rust
// crates/fraktor-actor/src/kernel/actor/receive_timeout/receive_timeout_setting.rs

/// Mirrors the `scala.concurrent.duration.Duration` values that
/// `ActorContext.setReceiveTimeout` accepts, without a general
/// infinite-duration algebra (design of #185, correspondence table of #282).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReceiveTimeoutSetting {
  /// `Duration.Undefined`: no receive timeout is in effect.
  Undefined,
  /// `Duration.Inf`: no receive timeout is in effect.
  Infinite,
  /// A finite inactivity duration after which `ReceiveTimeout` is sent.
  Finite(Duration), // core::time::Duration
}

// crates/fraktor-actor/src/kernel/actor/receive_timeout/receive_timeout_message.rs

/// Mirrors `org.apache.pekko.actor.ReceiveTimeout`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReceiveTimeout {
  timeout: Duration,
}

impl ReceiveTimeout {
  pub fn new(timeout: Duration) -> Self;
  /// The configured duration this message carries.
  pub fn timeout(&self) -> Duration;
}

// crates/fraktor-actor/src/kernel/actor/actor_context.rs (追加分)

/// Mirrors `ActorContext.setReceiveTimeout`. Callable from `pre_start`,
/// `post_restart`, and `receive`, as in Pekko.
pub fn set_receive_timeout(&mut self, timeout: ReceiveTimeoutSetting);

/// Mirrors `ActorContext.receiveTimeout`.
pub fn receive_timeout(&self) -> ReceiveTimeoutSetting;
```

- 原典の `setReceiveTimeout(Duration)` は引数を1つ取る。移植の enum は、`Duration.Undefined`（ケース5とケース12が使う解除）、`Duration.Inf`（ケース11が使う解除）、有限の期間の3つを区別する。`Option<Duration>` にしない理由は #185 の前例のとおりであり、`None` と有限値の比較で「無限は有限より大きい」という原典の大小が逆になる事故を、型の上で起こせなくするためである。比較そのものは本書の本体では行わないが、設定の読み出し口（`receive_timeout()`）が利用者に比較の機会を与えるため、enum で表す。
- 原典の `cancelReceiveTimeout` に当たる公開の口は作らない。原典でも `ActorContext` には無く、解除は `setReceiveTimeout(Duration.Undefined)` で行う。内部の予定の取り消しは `pub(crate)` とする。
- `ReceiveTimeout` の Java API `getTimeout` は作らない（§1 の注1）。

### 影響しないメッセージの印と分類

原典の `NotInfluenceReceiveTimeout` はマーカー trait であり、`ReceiveTimeoutCompat.isNotInfluenceReceiveTimeout` は `Identify` の具体クラスを先に見てから trait を見る。Rust の `Message` は型消去されており（`crates/fraktor-actor/src/kernel/message/message_value.rs`）、trait の実装の有無を消去の後から問う口が無い。移植は次の形にする。

```rust
/// Marker counterpart of `org.apache.pekko.actor.NotInfluenceReceiveTimeout`.
pub trait NotInfluenceReceiveTimeout: Any + Send + Sync {}

/// Mirrors `ReceiveTimeoutCompat.isNotInfluenceReceiveTimeout` for the
/// built-in kinds. User message kinds are classified through the
/// registration below, following the `DeadLetterSuppression`
/// registration precedent (`Executor::register_dead_letter_suppression`).
pub fn is_not_influence_receive_timeout(message: &Message) -> bool;
```

- 組み込みの判定は2つである。`Identify` の型であることと、`ReceiveTimeout` 自身ではないことの確認ではなく、原典どおり `Identify` を先に判定する。ただし `Identify` と `ActorIdentity` の型そのものは本書では新しく足さない。理由は、ケース1が確かめるのは分類だけであり、自動受信の返信（`ActorIdentity` を返す処理）は別の Spec（ActorRefSpec 系）の範囲だからである。そのためケース1の `Identify(None)` の行は、テストファイルの中に `Identify` に当たる最小の型を置いて分類だけを確かめる形を提案する。この置き方は「本体の型をテストのために作る」形になるため、§10 の判断材料に挙げる。代替は、本体の公開型として `Identify` だけを足し、自動受信の処理は足さない形である。
- 利用者のメッセージ型の印は、`Message` の包むときに trait を検出できないため、型の登録で表す。先行例は dead letter の抑制の登録（`register_dead_letter_suppression(TypeId)`)であり、分類の関数は、登録された `TypeId` の集合と照らす。テストの `TransparentTick` は、この登録を通して印を付ける。登録の置き場所（`ActorSystem` か `Executor` か）と、登録なしの型を既定で「影響する」とする点は原典と一致する（印が無いメッセージは期限を張り直す）。
- 分類の関数は公開する。ケース1が crate の外の統合テストから直接呼ぶためであり、内部 API を呼ぶケースを単体テストに置く例外を使わないで済む。

### セルの状態と発火の仕組み（新しい scheduler の口を作らない）

#181 の設計は、kernel のタイマの表と世代を FSM の中に置き、発火だけを外に分け、`Scheduler` の port は #234 の段6（#257）の持ち物として先に決めない、と決めた。本書も同じ線を守る。受信タイムアウトのために汎用のタイマ表や第2の scheduler の口は作らない。

- セルは `ReceiveTimeoutState` を1つ持つ。中身は `setting: ReceiveTimeoutSetting`、`generation: u64`、`deadline: Option<Duration>`（`Clock::monotonic()` の値。期限なしは `None`）である。原典の `version` が世代に当たる。世代はセルの中で単調に増え、再起動をまたいで戻さない（再起動前の期限が再起動後のセルに届くことを防ぐためであり、#181 の「世代の数えは再起動で戻さない」と同じ理由である）。
- 発火は実時間のスレッドでは行わない。実行器がシャードを1手運転するたびに、セルの期限と `Clock` の現在値を比べ、期限に達したセルへ `ReceiveTimeout(setting の期間)` を通常のメッセージとして配送する。期限の計算は `checked_add` で行い、あふれた場合は期限なしとして扱う（adaptor の ask の `AskDeadline::BeyondClockRange` の前例と同じ扱いである。§5）。
- 処理の前後の手順は §2 の原典の順序どおりに `ActorCell::deliver` と生成・再起動・停止の経路へ組み込む。生成と再起動の直後は、有限の設定があれば予定を張る（原典の `checkReceiveTimeout(reschedule = true)` の経路）。停止では期限を消し、世代を1つ上げる。停止したセルの期限が残っていても、世代が合わなければ配送しない。
- この形では、時間が進むのは実行器が運転されるときだけである。原典の TimingTest が実時間で待つケースは、移植では TestLatch と TestProbe の待ちが wait driver 経由で実行器を運転する間に期限が検査されるため、同じ観測（期限の後にメッセージが届く）が得られる。実時間の長さは原典の値（500ミリ秒、1秒）を使い、testkit の時間倍率（`TestKitSettings` の dilation）が既存の Spec と同じく効く形にする。

### no_std で書けるか

足す本体はすべて kernel であり、`core` と `alloc` だけで書ける。使うのは `core::time::Duration`、`Clock` の port、`TypeId` と `alloc` の集合だけであり、`std` の import は足さない。実時間の時計と待ちは adaptor と testkit の側に既にある。テストファイルは統合テストであり、従来どおり `std` を使ってよい。

## 4. 守る性質（不変条件）

`S` をセルの設定、`G` を世代、`D` を期限とする。

| 番号 | 不変条件 |
|:---:|:---|
| I1 | 配送される `ReceiveTimeout` の `timeout` は、配送のきっかけになった設定 `Finite(d)` の `d` と常に等しい。設定が `Undefined` か `Infinite` の間に `ReceiveTimeout` は配送されない |
| I2 | 影響するメッセージの処理が終わるたびに、設定が有限であれば期限は「処理の終了時刻＋期間」に張り直され、有限でなければ期限は無い。影響しないメッセージの処理では、ハンドラが設定を変えた場合にだけ期限が変わり、変えなければ期限は動かない |
| I3 | 1回の処理の中で設定を何回変えても、効くのは最後の値だけである（ケース13）。処理の後の判定は、処理の前の世代と処理の後の世代の差分だけで決まる |
| I4 | 世代は単調に増え、再起動と停止をまたいで戻らない。期限切れの配送は、配送の時点で世代が期限を張ったときの世代と一致する場合にだけ行う。取り消した期限、置き換えた期限、停止したセル、再起動前の個体の期限は、どれも配送されない（ケース15） |
| I5 | `ReceiveTimeout` 自身の処理は通常のメッセージと同じであり、その処理の後に次の期限が張られる（繰り返し届く。ケース8） |
| I6 | 設定の解除（`Undefined` と `Infinite`）は等しく効く。解除の後に期間が過ぎても配送されず、解除の前後でセルのほかの状態（振る舞い、子、購読）は変わらない（ケース5、ケース11、ケース12） |
| I7 | 分類は決定的である。同じ型のメッセージは常に同じ分類になり、分類はメッセージの中身（値）には依存しない。`Identify` に当たる型と登録された型だけが「影響しない」になる |
| I8 | 期間が0の設定は、有限として扱い、次の運転で直ちに期限切れになる。原典の `scheduleOnce(0)` が即時に発火することと一致する |

## 5. 壊しうる入力

| 区分 | 入力 | 期待 |
|:---|:---|:---|
| 境界 | 期間 `Duration::ZERO` | 有限として張り、次の運転で配送される（I8） |
| 境界 | 期間 `Duration::MAX` に近い大きな値 | 期限の加算があふれたら期限なしとして扱い、panic しない（`checked_add`。ask の `AskDeadline::BeyondClockRange` の前例） |
| 境界 | 処理の終了時刻ちょうどに期限が来る場合 | 期限は「現在値が期限以上」で判定し、その運転で配送される。原典の scheduler の即時発火と観測が一致する |
| 繰り返し | 1回の処理の中で `set_receive_timeout` を多数回呼ぶ（ケース13の一般化） | 最後の値だけが効く（I3）。世代は呼ぶたびに増えるが、期限は1つだけである |
| 繰り返し | 影響しないメッセージが期限より短い間隔で届き続ける（ケース7） | 期限は動かず、当初の期限で配送される（I2） |
| 並行に当たる順序 | 期限切れの配送がキューに入った直後に通常のメッセージが届く順序 | 原典でも `ReceiveTimeout` はメールボックスに入った通常のメッセージであり、順序は配送の順である。移植でもキューに入った順を保つ |
| 状態 | 停止の処理の中で `set_receive_timeout` を呼ぶ（ケース15の `StoppingSelfActor` は停止の前に呼ぶ） | 停止が優先し、配送されない（I4） |
| 状態 | 再起動の直後に設定する（ケース14） | 新しい個体の `pre_start`（原典の `postRestart` の経路）で張った期限が効く |
| 分類 | 印を登録していない型、値だけが違う同じ型 | すべて「影響する」として扱う（I7） |
| 文字列に当たる入力 | 本件には文字列の設定入力が無い | 設定は型付きの enum であり、名前や文字列で選ぶ経路を持たない（§8） |

時間の値は原典のまま（500ミリ秒、1秒、200ミリ秒）を使い、テストのために短く丸めない。待ちの上限は `TestLatch::DefaultTimeout` と TestProbe の既定に従う。

## 6. PBT を足すか

判断：足す。対象は実時間ではなく、セルの状態の機械（`ReceiveTimeoutState`）である。理由は、不変条件 I2、I3、I4 が「設定の変更、影響する／しないメッセージの処理、取り消し、期限切れ」の操作列に対する性質であり、固定の15件では操作列の組み合わせの一部しか踏まないためである。#177 の設計と同じく、固定 seed の乱数で操作列を作り、外部の依存は足さない。PBT は原典外であり、台帳の件数には含めない。置き場所は `crates/fraktor-actor/tests/receive_timeout_property_test.rs` とし、手動の `Clock` の実装で時刻を進める。

| 番号 | 性質 | 確かめる不変条件 |
|:---:|:---|:---:|
| P1 | 任意の操作列の後で、期限があるのは設定が有限のときだけであり、期限は最後に張った時刻＋期間と等しい | I1、I2 |
| P2 | 影響しないメッセージだけの列では、設定を変える操作が無い限り期限は一切動かない | I2、I7 |
| P3 | 世代がずれた期限は、時刻をいくら進めても配送されない。取り消しと停止と置き換えの後の古い期限を含む | I4 |
| P4 | 設定を変える操作が連続する列では、最後の設定だけが残り、期限は最大で1つである | I3 |
| P5 | 期間0と `Duration::MAX` を含む任意の期間で、状態の操作は panic しない | I8、§5 の境界 |

## 7. Quint を書くか

判断：書く。理由は、本件には状態遷移（設定の有無、期限の有無、世代、停止と再起動）があり、正しさの中心が「古い期限の発火を世代で抑える」ことにあるためである。これは #181 の設計が FSM のタイマで Quint を書いた理由（取り消しと置き換えと世代の誤りを固定テストが狭くしか見ない）と同じであり、#234 の設計も mailbox の予約で同じ判断をしている。逆に、状態遷移の無い移植（#185 の DurationSpec）では Quint を書かない判断が前例であり、本件はその場合に当たらない。

- 置き場所は `docs/design/specs/receive_timeout.qnt` と `docs/design/specs/receive_timeout_test.qnt`（`frequency_sketch.qnt` の前例の並び）とする。
- 状態は、設定（`Undefined`、`Infinite`、`Finite` の区別だけを持ち、期間の値は持たない）、期限の有無、世代、セルの生死（運転中、停止済み）、再起動の有無である。
- 遷移は、設定する、影響するメッセージを処理する、影響しないメッセージを処理する、期限が来る、停止する、再起動する、の6つである。期限が来る手と処理の手は分ける（割り込みを置くためであり、#181 の「発行と受信を分ける」と同じ作りである）。
- 不変条件は I1（有限のときだけ配送される）、I2（影響しない処理では期限が動かない）、I4（世代がずれた期限は配送されない）に対応するものを置く。
- 負の対照として、世代の判定を落としたものと、影響しないメッセージで期限を取り消すものを置き、それぞれ狙った不変条件が落ちることを確かめる。
- モデルにしないものは、実時間そのもの（期限は「来たかどうか」の真偽で表す）と、期間の算術（PBT と単体テストの範囲）である。

## 8. HOCON の設定について

原典の `ReceiveTimeoutSpec` は設定ファイルを使わない（`PekkoSpec()` に設定の上書きは無い）。したがって写す HOCON は無い。受信タイムアウトの値の渡し方は、原典でもコードの中の `setReceiveTimeout(Duration)` であり、文字列でも設定キーでもない。本書の設定の enum は、この型付きの渡し方をそのまま写したものである。前例（#154 の D1、G0b の決定 E、#195 の E1／E2）どおり、HOCON のパーサーは作らず、設定が関係する入力は型付きの値にする、という忠実さの判断をここに記録する。型の上で表せない入力（負の期間、文字列の期間）は、移植では作れない入力として扱う。原典の `FiniteDuration` は負の値を持てるが、負の期間の `setReceiveTimeout` は原典の Spec のどのケースも使っておらず、本書では `core::time::Duration` に負が無いことをもって同じ範囲外とする。

## 9. #234 の段が前提になるケースの切り分け

| ケース | 前提 | 扱い |
|:---:|:---|:---|
| 9（Timers で予定した影響しないメッセージ） | Pekko の `Timers` trait の移植と、kernel の `Scheduler` の port（#234 の段6、#257）。FSM のタイマの発火も同じ段の後として #181 の設計が後追いを切っている | 本書の第2段の範囲から外し、「#234 の段6の後」と分ける。台帳では未移植のまま残し、green の勘定には入れない（§10） |
| 7（scheduler で送り続ける影響しないメッセージ） | 原典は `system.scheduler` と `system.dispatcher` を使う | §1 の注2 の写し方（testkit の `ManualScheduler`）を認めれば前提にならない。認めなければ段6の後へ移る |
| そのほかの13件 | 複数スレッドの dispatcher は前提にならない。原典の TimingTest は実時間の経過を見ているだけであり、スレッドの並行性そのものは見ていない | 今の実行器のまま先に進める。段3（#252、アクターごとのメールボックス）が入っても、本書のセルの状態と手順はセルの側に閉じており、作り直しにはならない見込みである。その確認は段3の側の洗い出しに委ねる |

複数スレッドで時間切れが同時に起きること自体を確かめるケースは、原典の15件の中に無い。したがって「複数スレッドでの時間切れ」を理由に後へ回すケースは、ケース9を除いて無い。

## 10. 台帳で green にする対象ノード

対象ノードは1件だけである。

- id：`actor-tests/src/test/scala/org/apache/pekko/actor/ReceiveTimeoutSpec.scala`
- name：`actor-tests:ReceiveTimeoutSpec`
- 今の状態：`unported`、`cases.total` は `null`、`test_cases_approx: 2`（概数であり実数15とは違う。第2段で `cases.total: 15` を記録する）

第2段の終わりでの見込みは、ケース9の扱いで変わるため、レビューで決めてほしい点に挙げる。

- 案A（ケース9を段6の後として分ける場合）：移植14件、未移植1件となり、ノードは green にはならない。rollup の完了判定に照らすと、この issue の第2段は「14件を green 相当まで進め、1件を前提の段の後に残す」形になる。台帳の状態の書き方は、台帳の規約（状態遷移は unported → red → green の順だけ）と rollup の実装に合わせて第2段で決める。
- 案B（ケース9を除外として承認する場合）：移植14件、除外1件で green になる。ただし除外は人間だけが決める区分であり、本書は理由（前提の `Timers` と `Scheduler` の port が未移植であること）を提案として書くにとどめる。恒久的な除外ではなく前提が揃えば移せるため、本書は案Bを推奨しない。
- 本書の推奨は案Aである。issue の文面の「台帳の対象ノードを green にする」とは名前が違う形になるため、その点をレビューで確認してほしい。

あわせて第2段では、`rust.crate` に `fraktor-actor`、`rust.test_file` に `crates/fraktor-actor/tests/receive_timeout_spec.rs` を記録し、`needs.testkit` は現状のままで足りる（不足は無い）。`docs/baseline/v2.0.0-undocumented-nodes.tsv` の行の追加は、ほかの移植の前例に合わせて第2段で行う。

## 11. 影響

- **公開 API**：追加は `kernel::actor` の `ReceiveTimeout`、`ReceiveTimeoutSetting`、`NotInfluenceReceiveTimeout`、`is_not_influence_receive_timeout` と、`ActorContext` の `set_receive_timeout` と `receive_timeout` である。既存の公開 API の変更と削除は無い。
- **新しいコードファイルのヘッダー**：第2段で足す `crates/**/*.rs` の新しいファイルには、自作のものも含めて Apache-2.0 のヘッダーを付け、Pekko の移植であるファイルには原典の指定を入れる（加藤の決まり、2026-11-26。決まりの見直しは [Issue #241](https://github.com/j5ik2o/fraktor-ng-rs/issues/241) で進行中）。本書のような設計の Markdown には、既存の設計文書にヘッダーが無いため付けない。
- **台帳**：対象は §10 の1ノードだけである。本書の段では触らない。
- **#181 との重なり**：#181（[PR #275](https://github.com/j5ik2o/fraktor-ng-rs/pull/275)、設計レビュー中）は kernel の FSM にタイマの表と世代を足す。本書は FSM の表を使わず、別の汎用タイマ表も作らない。重なるのは「世代で古い発火を捨てる」という考え方だけであり、世代の数え自体はセルと FSM で別々に持つ。将来 `Scheduler` の port が段6で入るときは、本書の期限の検査と #181 の発火の出口の両方がその port の利用者になる。その差し替えは段6の側の仕事として、ここに記録する。
- **保存済みのデータと上流**：影響は無い。Pekko の上流へ送るものも無い。
- **守りの仕組み**：CI の必須チェックは変えない。第2段で、移植したテストと PBT と Quint の検査が必須チェックの中で動くようになる。
- **実行時間**：原典の時間の値を使うため、統合テストに最大で数秒の実時間の待ちが加わる。testkit の時間倍率が効く形にするため、倍率を上げる実行では待ちは長くなる。既存の TimingTest 相当の Spec（`actor_timeout_spec.rs` など）と同じ性質である。

## 12. 第2段の進め方（設計レビューが通ってから）

1. Claude Opus の担当が、`crates/fraktor-actor/tests/receive_timeout_spec.rs` に §1 の14件を書き、台帳の対象ノードを red にして、失敗を確認する（red）。ケース1の置き方（§3 の `Identify` の扱い）は、§10 の判断が決まってから書く。
2. 本書の担当が、§3 の本体だけを実装してテストを通す（green）。テストが誤りだと思った場合は、テストを書き換えず、triage への差し戻しとして報告する。
3. `crates/fraktor-actor/tests/receive_timeout_property_test.rs` に §6 の PBT を足し、`docs/design/specs/receive_timeout.qnt` と `receive_timeout_test.qnt` に §7 の Quint を足す。
4. 台帳の対象ノードを §10 の見込みどおりに更新し、`docs/baseline/v2.0.0-undocumented-nodes.tsv` を作り直す。
5. CI と同じ検査を流す。`cargo fmt --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`、台帳の Python テスト（`ledger/tools`）、Quint の検査である。必須のチェックが1つでも落ちたら、ほかの完了を待たずに直す。
6. PR をドラフトから外し、マージできる状態にする。マージはしない。ボットの指摘とトップレベルのコメントには、それぞれ直した／別件／採らないと理由を書いて返信し、スレッドを解決する。

## 13. レビューで決めてほしい点

1. **ケース9を「#234 の段6の後」として分け、対象ノードを green にしない案Aでよいか。** 認めない場合は、ケース9の除外の承認（案B）か、`Timers` と `Scheduler` の port を本書の範囲に引き込むか、どちらかになる。引き込む場合は公開 API の追加が大きく増え、#181 の後追いと #257 の範囲と重なるため、第2段へ進まず範囲を組み直す必要がある。
2. **ケース7の予定送信を testkit の `ManualScheduler` で写す形でよいか。** 認めない場合はケース7も段6の後へ移り、先に進めるのは13件になる。
3. **ケース1の `Identify` の扱いをどうするか。** 本書の提案は、テストファイルの中に `Identify` に当たる最小の型を置いて分類だけを確かめる形である。代替は、本体の公開型として `Identify` だけを足す形である。自動受信の返信そのものは、どちらの形でも本書の範囲に入れない。
4. **発火を実行器の運転の中の期限検査で行う形でよいか。** 代替は、実時間のスレッドで発火させる形であり、それは `Scheduler` の port の形を本書が先に決めることになり、#257 と二重になる。本書は前者を推奨する。
5. **ケース3の Java API の行（`getTimeout`）を、Rust の `timeout()` で代表させる形でよいか。** §1 の注1 の提案である。
