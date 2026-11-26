# 設計：#200 ReceiveTimeoutSpec（受信タイムアウトの本体と Spec の移植）

[Issue #200](https://github.com/j5ik2o/fraktor-ng-rs/issues/200) の設計の記録である。Pekko v2.0.0 の actor-tests にある actor の `ReceiveTimeoutSpec` を Rust へ移植する。台帳の対象ノードは、ケース9を案Aで #257 の後へ回すため green にはせず、red（15/14/14）のまま本件を完了とする（中将の決定、2026-11-27。§10、§13-1）。本書は第1段（設計だけ）であり、実装とテストの作成と台帳の更新は、設計レビューが通ってから第2段で行う。本書の段では台帳を触らない。進め方の変更（オーナーの指示、2026-11-26）により、第2段ではテストを Claude Opus の担当が先に書いて red で置き、本書の担当は、そのテストを通す実装だけを書く（テストは書き換えない・弱めない・消さない）。

原典（Pekko v2.0.0）:

- [ReceiveTimeoutSpec.scala](https://github.com/apache/pekko/blob/v2.0.0/actor-tests/src/test/scala/org/apache/pekko/actor/ReceiveTimeoutSpec.scala)（352 行）
- 本体は [ReceiveTimeout.scala](https://github.com/apache/pekko/blob/v2.0.0/actor/src/main/scala/org/apache/pekko/actor/dungeon/ReceiveTimeout.scala) の `dungeon.ReceiveTimeout`（`ActorCell` の dungeon trait）と、[Actor.scala](https://github.com/apache/pekko/blob/v2.0.0/actor/src/main/scala/org/apache/pekko/actor/Actor.scala) の `ReceiveTimeout(timeout: FiniteDuration)` と `NotInfluenceReceiveTimeout` と `Identify`、[ActorCell.scala](https://github.com/apache/pekko/blob/v2.0.0/actor/src/main/scala/org/apache/pekko/actor/ActorCell.scala) の `receiveTimeout` と `setReceiveTimeout`、および Scala 2.13 版の `ReceiveTimeoutCompat.isNotInfluenceReceiveTimeout`（`Identify` か `NotInfluenceReceiveTimeout` のどちらかであれば真）である
- 依存の [Issue #180](https://github.com/j5ik2o/fraktor-ng-rs/issues/180)（TestLatch）は main に入っている。依存する issue はほかに無い

移植の規約は `.takt/facets/knowledge/fraktor-porting-rules.md` に従う。形の前例は [PR #205](https://github.com/j5ik2o/fraktor-ng-rs/pull/205) の `docs/design/issue-177-frequency-sketch.md` と `docs/goals/issue-177-frequency-sketch.md`、および `docs/design/issue-195-deployer.md`（HOCON を型付きの入力に写す忠実さの判断）である。方針は `docs/decisions/pekko-semantics-first.md`（速さより Pekko の意味論を優先する）に従う。Scala・Java の書き方の対応は `docs/decisions/scala-java-to-rust-correspondence.md`（[PR #282](https://github.com/j5ik2o/fraktor-ng-rs/pull/282) で main に入った）を引いた。同文書の Duration の無限の扱い（汎用の無限の代数を持つ型は作らず、使う場所ごとの型で表す）と、`docs/design/issue-185-util-specs.md` の注意（FSM と ReceiveTimeout では `Option<Duration>` を比較に使わず専用の enum にする）に従う。

結論を先に書く。**受信タイムアウトの本体は fraktor-ng-rs にまだ無い。** `crates/fraktor-actor` の中を `receive_timeout`、`ReceiveTimeout`、`NotInfluence`、`setReceiveTimeout` で探した結果は 0 件であり、`ActorContext` にも受信タイムアウトの口は無い。原典の15件のうち14件は、本書が設計する本体（セルの3状態の機械と、実行器が持つ期限の索引）を足せば移植できる。ただし期限の置き場所は、段3（#252、承認済み）の §11.11 の形（セルは `Arc<Mailbox<ActorCell>>` の中、`actors: BTreeMap<(u64, u32), ActorEntry>`、`ManualExecutor`、`AfterRun`）を前提に書き、実装の順は #252 のマージの後とする。初版が書いた「期限を実行器の運転の中で検査する形は作り直しにはならない」という見込みは、独立レビュー H1 のとおり誤りであり撤回する（§3、§14）。残る1件（ケース9、Timers で予定したメッセージ）は案Aで確定し、#234 の段6（#257）の後へ回す。台帳は red のまま 15/14/14 とし、数からは外さない（§9、§10）。§13 の3点は中将が決めた（2026-11-27）。ケース9は red のまま完了とし後続の issue に分けること、ケース3の中の Java API（`getTimeout`）は Rust の `timeout()` で同じ観察を確かめる形で写すこと、kernel の実行器に期限の索引を持たせることは条件つきで採ることである。

## 1. 何を移植するか

### 件数の数え直し

原典の `" in {"` の数を数えた。依頼の目安は15件であり、数え直した結果も15件で、目安と一致する。台帳の `test_cases_approx: 2` は変換時の概数であり、実数とは違う。第2段で台帳の `cases.total` に15を記録する。`ignore` と `pending` は無い。`TimingTest` のタグが付いているのは12件であり、ケース1、14、15には付いていない（独立レビュー L1 で数え直した）。タグは ScalaTest の実行時の振り分けであり、Rust には対応物が無いため、移植ではタグを写さず、時間の値は原典のまま使う（§5）。

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

注1（ケース3の Java API の部分）: 原典は `msg.timeout should ===(500.milliseconds)` と `msg.getTimeout should ===(java.time.Duration.ofMillis(500))` の2つを確かめる。後者は Java の利用者向けの取り出し口であり、Rust には `java.time.Duration` が無い。移植では `ReceiveTimeout::timeout() -> core::time::Duration` で、設定した期間が `ReceiveTimeout` に載るという同じ観察を確かめる形で写す（中将の決定、2026-11-27。§13-2）。ケースの数からは外さない。理由は、`docs/decisions/scala-java-to-rust-correspondence.md` の決まり（Pekko どおりに写せるものは写し、Rust で同じ振る舞いを作れず数から外す・弱めるときだけ伺う）により、観測できる振る舞い（設定した期間がメッセージに載ること）は Rust の取り出し口で同じく確かめられるためである。

注2（ケース7の予定送信の写し方）: 原典は `system.scheduler.scheduleWithFixedDelay(100.millis, 100.millis)` で `TransparentTick` と `Identify(None)` を送り続ける。移植では、std のスレッドが100ミリ秒ごとに両方を送り続け、latch が開いたら止める形に写す（独立レビュー M4 の推奨。`ActorRef` は `Send + Sync` であり、原典の `system.scheduler` も別のスレッドで送るため、写し方として最も近い）。testkit の `ManualScheduler` は仮想の時刻で動き実行器の `Clock` とつながっていないため使わない。条件は「待ちの間ずっと送り続ける」ことであり、tick を最初にまとめて送る写し方や短い間だけ送る写し方では、影響しないメッセージで時計を戻す誤った実装でもケースが通ってしまう。時計を戻す変異でケースが落ちることを第2段で確かめる。この写し方では #234 の段6は前提にならない。

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
3. メッセージの処理の後に（`checkReceiveTimeoutIfNeeded`、`finally` の中にあるためハンドラが失敗しても走る）、状態があるか、処理の前後で版が変わっていれば再予定の判定をする。再予定の引数は「メッセージが影響するメッセージであるか、版が変わった（＝ハンドラが `setReceiveTimeout` を呼んだ）か」のどちらかである。影響するメッセージは、版の差分が無くても張り直しの対象になる（独立レビュー L6 のとおり条件を書き直した）。
4. 再予定の判定（`checkReceiveTimeout`）では、設定が有限の期間であれば、古い予定を取り消して `scheduler.scheduleOnce(timeout, self, ReceiveTimeout(timeout))` で張り直し、版を1つ上げる。設定が有限でなければ、予定を取り消すだけである。有限の設定があり、予定（task）がまだ張られていない（`emptyCancellable` の）場合は、再予定の引数が偽でも張る（生成直後と再起動直後の経路。`ActorCell.create` と `finishRecreate` は `checkReceiveTimeout(reschedule = true)` を呼ぶ）。
5. 停止のとき（`terminate` と `faultCreate`）は予定を取り消す。**再起動（`faultRecreate`）では予定を取り消さず、設定は再起動をまたいで残る**（独立レビュー L5）。取り消しの効き目は `Cancellable.cancel()` だけであり、原典の `version` は「1回の処理の前後だけで比べるので一巡しても害が無い」値で、古い発火を抑える仕組みではない。**メールボックスに入った後の `ReceiveTimeout` は、設定がその後に変わっても、普通のメッセージとして必ず配られる**（`ActorCell.scala` の文書。独立レビュー H2）。移植もこの振る舞いをそのまま写し、世代で積んだメッセージを捨てることはしない（§3、§4）。

ケース12とケース13が確かめるのは、手順2と手順3の組み合わせである。影響しないメッセージの処理では予定は取り消されず、ハンドラが設定を変えたときだけ、処理の後の判定で新しい設定が効く。ケース13では、1回の処理の中で設定を2回変え、最後の値だけが効く。

## 3. 設計する本体

### 置き場所

| ファイル | 中身 |
|:---|:---|
| `crates/fraktor-actor/src/kernel/actor/receive_timeout.rs`（新規のモジュールの入口） | 再エクスポート |
| `crates/fraktor-actor/src/kernel/actor/receive_timeout/receive_timeout_message.rs` | `ReceiveTimeout` メッセージ |
| `crates/fraktor-actor/src/kernel/actor/receive_timeout/receive_timeout_setting.rs` | `ReceiveTimeoutSetting`（設定の enum） |
| `crates/fraktor-actor/src/kernel/actor/receive_timeout/not_influence_receive_timeout.rs` | 印の trait と分類の関数 |
| `crates/fraktor-actor/src/kernel/actor/receive_timeout/receive_timeout_state.rs` | セルが持つ状態（設定、3状態、期限、版）。`pub(crate)` |
| `crates/fraktor-actor/src/kernel/actor/actor_context.rs` | `set_receive_timeout` と `receive_timeout` の2つの口を足す |
| `crates/fraktor-actor/src/kernel/executor/actor_cell.rs` | 状態を持ち、処理の前後の手順と停止時の取り消しを担う |
| 実行器（段3 §11.11 の形。ファイルは #252 のマージ後に確定） | 期限の索引を持ち、最も早い期限だけを検査して、期限が来たセルへ `ReceiveTimeout` を積む |

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
- `cancelReceiveTimeout` に当たる公開の口は作らない。Scala の `ActorContext` には無い（解除は `setReceiveTimeout(Duration.Undefined)` で行う）。Java の `AbstractActor.ActorContext` には `cancelReceiveTimeout()` があるが、Java API そのものを Rust に作らない方針（対応表）により作らない（独立レビュー L7）。内部の予定の取り消しは `pub(crate)` とする。
- `ReceiveTimeout` の Java API `getTimeout` は作らない（§1 の注1）。

### 影響しないメッセージの印と分類

原典の `NotInfluenceReceiveTimeout` はマーカー trait であり、`ReceiveTimeoutCompat.isNotInfluenceReceiveTimeout` は `Identify` の具体クラスを先に見てから trait を見る。原典では分類は型だけで決まる静的な性質である。Rust の `Message` は型消去されており（`crates/fraktor-actor/src/kernel/message/message_value.rs`）、trait の実装の有無を消去の後から問う口が無い。移植は次の形に決める（独立レビュー M1、M2）。

- **組み込みの型を kernel に足す。** `Identify` を kernel の公開型として足し（署名は下のコードブロックで確定する）、分類は登録なしで `Identify` を「影響しない」と判定する。テストファイルの中の最小の型で写す初版の提案は採らない。その形では組み込みの判定が働かず、ケース1の1行目が登録で印を付けた2行目と同じことしか確かめなくなり、ケースが弱まるためである。自動受信（`ActorIdentity` の返信）は足さず、後追いの issue を切る（ActorRefSpec と ActorSelectionSpec 系の範囲。中将が切る）。原典の `Identify(None)` は、識別子が無いことを表す値として `Identify::new(Option::<()>::None)` と書く。ケース1とケース7でこの形を使う。なお自動受信は未実装であるため、ケース7のテストアクターは未知のメッセージを無視する（返信は確かめない）。
- **利用者の型の印は登録で表し、trait は登録の境界で使う。** `ActorSystem` に `register_not_influence_receive_timeout::<T: Any + NotInfluenceReceiveTimeout>(&self)` の登録の口を置く（署名は下のコードブロック。dead letter 抑制の `DeadLetterSuppression` と `ActorSystem::register_dead_letter_suppression` の前例に揃える）。登録表はシステムごと（dead letter 抑制の `DeadLettersShared` と同じ範囲）とし、**登録はそのシステムで最初のメッセージが届くより前に済ませる**ことを制約とする。登録より前に届いたメッセージは「影響する」と分類される（この制約を破った場合の観測であり、I7 の「同じ型は常に同じ分類」は、登録を済ませたシステムの中での性質として書く。§4）。
- **分類の関数は通常の公開 API にはしない。** 原典の `ReceiveTimeoutCompat` は `private[pekko]` であり、無条件の `pub` にすると利用者が頼れる約束を足すことになる。分類の関数本体は `pub(crate)` とし、red のテストから使う口は、段3で既に入っている cargo の feature `test-support`（`crates/fraktor-actor/Cargo.toml` の `test-support = []`。main でも確認した）の下に置く `ActorSystem::is_not_influence_receive_timeout(&self, message: &Message) -> bool` と確定する（独立再レビュー R3）。これによりケース1も含め14件すべてを `tests/receive_timeout_spec.rs` に収める。あわせて、Timers の包みの型（`NotInfluenceReceiveTimeoutTimerMsg` に当たるもの）を後から組み込みとして足せる形にしておく（ケース9、§9）。

```rust
// crates/fraktor-actor/src/kernel/actor/identify.rs（新規）

/// Mirrors `org.apache.pekko.actor.Identify`.
/// `Identify(None)` in the original is written
/// `Identify::new(Option::<()>::None)`.
#[derive(Clone, Debug)]
pub struct Identify {
  message_id: Message, // crate::kernel::message::Message
}

impl Identify {
  pub fn new<T: core::any::Any + Send + Sync>(message_id: T) -> Self;
  pub fn message_id(&self) -> &Message;
}

// crates/fraktor-actor/src/kernel/actor/receive_timeout/not_influence_receive_timeout.rs

/// Mirrors `org.apache.pekko.actor.NotInfluenceReceiveTimeout`.
pub trait NotInfluenceReceiveTimeout {}

// crates/fraktor-actor/src/kernel/actor_system/actor_system_value.rs (追加分)

/// Registers `T` as a message type that does not influence the
/// receive timeout, mirroring the marker trait in Pekko. Call before
/// the first message is delivered in this system.
pub fn register_not_influence_receive_timeout<T: core::any::Any + NotInfluenceReceiveTimeout>(&self);

/// Test-only view of the classification, mirroring the
/// `private[pekko]` `ReceiveTimeoutCompat`. Not a public API promise.
#[cfg(feature = "test-support")]
pub fn is_not_influence_receive_timeout(&self, message: &Message) -> bool;
```

### セルの状態と発火の仕組み（新しい scheduler の口を作らない）

#181 の設計との関係を正しく書く（独立レビュー M7）。#181 の設計は「kernel は時間を数えない。testkit の期限のスレッドが記録を読んで印を送り返す」と決めた。本書は違う。受信タイムアウトは本番の機能であり、testkit のスレッドに頼れないため、**kernel の実行器が `Clock` を読んで期限を比べ、発火させる**。つまり本書は、kernel に時間を数えさせる分担を #234 の段6からこの件のために前倒しする（§13-3 で、中将が条件つきで採ると決めた。2026-11-27）。重ねて作っていない点は正しい。FSM のタイマの表と世代は使わず、汎用のタイマの表も作らない。

#### 状態は3つ（H2）

セルは `ReceiveTimeoutState` を1つ持つ。中身は `setting: ReceiveTimeoutSetting` と、次の3状態である。

| 状態 | 意味 | 原典の対応 |
|:---|:---|:---|
| なし | 予定は無い | `task` が `emptyCancellable` で、発火済みでもない |
| 張ってある（期限） | 期限 `deadline` が決まっている | `task` が張ってある |
| 発火済み（未処理） | 期限が来て `ReceiveTimeout` を積んだが、まだ処理されていない | `task` が発火済みで空でない |

- 期限が来たら、**期限を消して「発火済み」にし、`ReceiveTimeout(setting の期間)` を1件だけ積む**。発火で期限を消すため、`run_one` のたびに積まれることはなく、「発火1回につき積むのは1件」が成り立つ。
- 積んだ後の `ReceiveTimeout` は普通のメッセージであり、**設定がその後に `Undefined` や `Infinite` に変わっても、世代がずれても、必ず配る**。判定（有限か、期限が来たか）は積む時点だけであり、受信の時点では判定しない（H2 で I1 と I4 を書き直した。§4）。二重に届く場合があるのは原典どおりであり、欠陥ではない。たとえば発火済みの `ReceiveTimeout` がキューに残っている間に新しい期限が張られて再び発火すれば、2件届く。
- 影響しないメッセージは「発火済み」を張り直さない（原典では `task` が空でないので張り直しが起きない）。影響するメッセージの前処理で、張ってある予定を取り消し、状態を「なし」に戻す。
- 原典の `version` に当たる版は、1回の処理の前後の変更の検出だけに使う（§2）。古い期限の抑止には使わない。期限の索引（次項）の行が古くなる場合の照合にだけ、世代を使う。
- 時計が無い場合（`self.clock` も境界の `monotonic_time` も `None` のとき。段3の `Executor::now()` は `self.clock` を先に見て、無ければ境界の `monotonic_time()` を見る）は、`set_receive_timeout(Finite(_))` を呼んだ時点で理由を付けて panic する（独立レビュー L2、独立再レビュー R5）。panic の文言は restart window 用の文言を流用せず、受信タイムアウト専用の理由（有限の受信タイムアウトには単調時計が要る旨）にする。黙って発火しない形にはしない。

#### 期限の置き場所は段3 §11.11 の形を前提にする（H1）

初版の「実行器がシャードを1手運転するたびに全セルを走査する」形と「段3が入っても作り直しにはならない見込み」は撤回する。段3（#252、PR #284 で設計は承認済み）の §11.11 では、`cells: Vec<ActorCell>` が `actors: BTreeMap<(u64, u32), ActorEntry>` に替わり、`ActorCell` は `Arc<Mailbox<ActorCell>>` の中へ移り、`ShardQueue` は消え、`run_one` の意味は「列が空になったら false」に変わる。本書はその形を前提にする。

- 実行器は**張ってある期限の索引**を持つ（独立レビュー L4）。期限と宛先の id と世代で並ぶ `BTreeMap` であり、張ってある期限が無いときは時計も読まない。
- **索引の古い行は即時に消す形を採る**（独立再レビュー R1）。予定の取り消し・張り直し・停止のときに、旧い行 `(old_deadline, key, old_gen)` を索引から即時に削除する。したがって索引に古い行は残らず、索引の大きさは生存しているアクターの数以下に収まる。念のため発火の前にはセルの状態と世代で照合し、世代が要るのはこの照合のためである。検査は、先頭から期限が来た行を順に取り除いて発火するループにする（先頭だけを見て止めない）。
- **期限の検査は4つの入口で行う**（独立再レビュー R1。段3の実装 `executor_value.rs` に照らして確定する）。段3の `run_one` と `run_until_idle` は `manual.pop()` が `None` なら即座に戻るため、キューが空のときに検査する入口が要る。(1) `run_one` と `run_until_idle` の先頭（`manual.pop()` の前）。(2) `Run` の実行の後（`finish_run` の後、`AfterRun` の処理を含む）。(3) `await_envelope` の待ちの前。(4) `await_envelope` の待ちの後。これにより、`ActorSystem::set_clock` で時刻を進めた直後や `await_envelope` の直後でも、キューが空なら期限が発火する。
- **`await_envelope` の契約**（独立再レビュー R1）。待ちの長さは最も早い期限で切る。`await_envelope(max)` は `min(max, 最も早い期限 − 今)` までしか待たず、**期限で切った待ちが満了したら、期限切れを発火してメールボックスへ `ReceiveTimeout` を積み（`ManualExecutor` に `Run` が入る）、`has_work(self) == true` で `true` を返す**。段3の `await_envelope` は現状、待ち明けに `has_work(self)` を返すだけであり、発火しないと `false` を返して `TestKitBase::await_one` が `run_one()` を呼ばなくなるため、この契約を本書の要件として明記する。
- **`await_envelope` の時刻は `self.clock` 優先の現在時刻にそろえる**（独立再レビュー R1）。段3の `await_envelope` は現状 `self.boundary.monotonic_time()` を直接読んでおり `self.clock` を見ないが、期限は `Executor::now()`（`self.clock` 優先）で計算するため、`set_clock` の使用時に時間軸が食い違う。`await_envelope` の `最も早い期限 − 今` の計算は、`Executor::now()` と同じ解決順（`self.clock` を先に見る）の現在時刻で行うことを要件とする。
- この形は **Manual の dispatcher に限る**。段5以降の複数スレッドの dispatcher では、誰も実行器を「運転」しないため、アイドルのアクターの期限はこの形では起きない。段5か段6で、**原典どおり scheduler に `ReceiveTimeout` を予約する形（時計のスレッドか `Scheduler` の port による発火）へ作り直す**ことを、#234 の設計（§8 の表か段3・段5 の完了条件）と #252 側へ書いてもらう前提として、ここに明記する。索引は、そのときに `Scheduler` の port へ差し替える単位になる。
- 実装の順は **#252 のマージの後**とする。今の `executor_value.rs` に組み込むと、#284 と同じ所を同時に書き換えることになるためである。

#### `ActorContext` からセルの状態へ届く経路（M3）

`ActorContext` は処理の間セルに触れない（実行器はセルを取り出して手元に持つ）。そのため、受信タイムアウトの状態は `stack` と同じく、セルから取り出して文脈に貸し、処理の後に戻す。段3の実装では `ActorContext::new` を作る場所は4か所ではなく**6か所**である（独立再レビュー R4。main の `executor_value.rs` と `actor_cell.rs` で確認した）。6か所を次のとおりにする。

| 文脈を作る場所 | 貸す状態 | 備考 |
|:---|:---|:---|
| `Executor::invoke` → `ActorCell::receive`（通常の処理） | セルの状態 | 段3では通常配送の文脈作成は `deliver` ではなく `invoke` から `ActorCell::receive`（`actor_cell.rs`、struct のフィールド分割借用）で作る。処理の前後の手順（§2）はこの貸し借りの前後で担う |
| `create_initial`（`pre_start`） | 新しいセルの状態 | 原典の `create` の経路。有限なら生成直後に張る |
| `restart_cell` の `pre_restart`（古い文脈） | 古い個体の状態 | 原典では `preRestart` の中の `setReceiveTimeout` も状態に残る |
| `restart_cell` の `restart`（新しい文脈、`post_restart`） | 再起動をまたいで残った状態 | `finishRecreate` に当たり、新しい個体の `post_restart` の後に有限なら張り直す |
| `Executor::run_post_stop` | 貸さない | 停止済みであり、状態は `terminate` 相当で取り消し済みであることとし、新たな予定は張らない |
| `Executor::execute_child_terminated_hook` | 親のセルの状態を貸し戻す | フックの中で親が `set_receive_timeout` を呼ぶ場合に備え、親の状態を貸す |

あわせて、`create_initial` の失敗時のリトライ経路（原典の `faultCreate`。`FaultHandling.scala` では `cancelReceiveTimeout()` と `cancelReceiveTimeoutTask()` が走る）では、`ReceiveTimeoutState` を初期化し、前回の試行で `pre_start` が呼んだ `set_receive_timeout` がリトライに残らないようにする（独立再レビュー R4）。

段4で文脈が「自分のセルを借りる」形になる（#234 §6）ため、この貸し借りは段4でその形へ移る。

#### 再起動と失敗の経路（L5）

- 再起動では設定を消さない（`faultRecreate` は予定を取り消さない）。新しい個体の `post_restart` の後に、有限なら張り直す。#181 の FSM が再起動で表を空にする形とは違うので、読み手がそちらを写さないよう明記する。
- 処理の後の判定は、ハンドラが失敗しても走らせる（原典では `finally` の中）。今は `Resume` の指示が無いため失敗の経路の違いは観測されないが、`Resume` を足すときに効く。

### no_std で書けるか

足す本体はすべて kernel であり、`core` と `alloc` だけで書ける。使うのは `core::time::Duration`、`Clock` の port、`TypeId` と `alloc` の集合だけであり、`std` の import は足さない。実時間の時計と待ちは adaptor と testkit の側に既にある。テストファイルは統合テストであり、従来どおり `std` を使ってよい。

## 4. 守る性質（不変条件）

`S` をセルの設定、`D` を期限とする。版と世代で積んだメッセージを捨てる約束は置かない（H2）。

| 番号 | 不変条件 |
|:---:|:---|
| I1 | 期限が来て積む時点では、設定が `Finite(d)` のときだけ `ReceiveTimeout(d)` を積む。**積んだ `ReceiveTimeout` は、設定がその後に変わっても、必ず配られる** |
| I2 | 処理の後の判定は原典の `checkReceiveTimeoutIfNeeded` の条件どおりである。影響するメッセージの処理の後は有限なら張り直す。影響しないメッセージの処理では、ハンドラが設定を変えた場合か、予定がまだ張られていない場合にだけ張る（L6） |
| I3 | 1回の処理の中で設定を何回変えても、効くのは最後の値だけである（ケース13）。処理の後の判定は、版の差分（影響しないメッセージの場合）と、メッセージが影響するかどうかと、予定が張られているかどうかで決まる。版の差分だけで決まるわけではない（L6） |
| I4 | 発火1回につき、積む `ReceiveTimeout` は1件だけである。発火すると期限は消え、状態は「発火済み」になる。世代は期限の索引の行の照合だけに使い、積んだメッセージの配送を止めるためには使わない |
| I5 | `ReceiveTimeout` 自身の処理は通常のメッセージと同じであり、その処理の後に次の期限が張られる（繰り返し届く。ケース8） |
| I6 | 設定の解除（`Undefined` と `Infinite`）は等しく効く。解除は、まだ積まれていない予定にだけ効く。解除の前後でセルのほかの状態（振る舞い、子、購読）は変わらない（ケース5、ケース11、ケース12） |
| I7 | 分類は決定的である。登録を済ませたシステムの中では、同じ型のメッセージは常に同じ分類になり、分類はメッセージの中身（値）には依存しない。組み込みの `Identify` と登録された型だけが「影響しない」になる |
| I8 | 期間が0の設定は、有限として扱い、次の運転で直ちに期限切れになる。原典の `scheduleOnce(0)` が即時に発火することと一致する。ただし期間0では処理のたびに張り直されるため **`run_until_idle` は戻らない**。0の確認は `run_one` か時間を区切った待ちで行う（L3）。原典の文書の「1ミリ秒が最小」は、最小値を強制する規則としては写さず、上の注意として記録する |

## 5. 壊しうる入力

| 区分 | 入力 | 期待 |
|:---|:---|:---|
| 境界 | 期間 `Duration::ZERO` | 有限として張り、次の運転で配送される（I8） |
| 境界 | 期間 `Duration::MAX` に近い大きな値 | 期限の加算があふれたら期限なし（状態は「なし」）として扱い、panic しない（`checked_add`。ask の `AskDeadline::BeyondClockRange` の前例） |
| 境界 | 処理の終了時刻ちょうどに期限が来る場合 | 期限は「現在値が期限以上」で判定し、その運転で配送される。原典の scheduler の即時発火と観測が一致する |
| 繰り返し | 1回の処理の中で `set_receive_timeout` を多数回呼ぶ（ケース13の一般化） | 最後の値だけが効く（I3）。世代は呼ぶたびに増えるが、期限は1つだけである |
| 繰り返し | 影響しないメッセージが期限より短い間隔で届き続ける（ケース7） | 期限は動かず、当初の期限で配送される（I2） |
| 並行に当たる順序 | 期限切れの配送がキューに入った直後に通常のメッセージが届く順序 | 原典でも `ReceiveTimeout` はメールボックスに入った通常のメッセージであり、順序は配送の順である。移植でもキューに入った順を保つ |
| 状態 | 停止の処理の中で `set_receive_timeout` を呼ぶ（ケース15の `StoppingSelfActor` は停止の前に呼ぶ） | 停止が優先し、配送されない（I4） |
| 状態 | 再起動の直後に設定する（ケース14） | 新しい個体の `pre_start`（原典の `postRestart` の経路）で張った期限が効く |
| 順序 | 発火済みの `ReceiveTimeout` がキューにある間に設定を解除・変更する | 積んだ `ReceiveTimeout` は必ず配られる（I1）。原典どおりであり、二重に届く場合も欠陥ではない |
| 状態 | 時計が無い境界で `set_receive_timeout(Finite(_))` を呼ぶ | 理由を付けて panic する（L2） |
| 分類 | 印を登録していない型、値だけが違う同じ型 | すべて「影響する」として扱う（I7） |
| 文字列に当たる入力 | 本件には文字列の設定入力が無い | 設定は型付きの enum であり、名前や文字列で選ぶ経路を持たない（§8） |

時間の値は原典のまま（500ミリ秒、1秒、200ミリ秒）を使い、テストのために短く丸めない。待ちの上限は `TestLatch::DefaultTimeout` と TestProbe の既定に従う。

## 6. PBT を足すか

判断：足す。ただし置き場所と形を独立レビュー M6 のとおりに直す。`ReceiveTimeoutState` は `pub(crate)` なので、`tests/` の統合テストからは見えない。

- 状態の機械の PBT は単体テストに置く。`crates/fraktor-actor/src/kernel/actor/receive_timeout/receive_timeout_state_test.rs`（`executor_value_test.rs` の並び）。
- アクターを通す PBT は `tests/` に置き、公開の `ActorSystem::set_clock` で手で進める時計を入れる。
- 形は、性質の列挙よりも参照モデルとの完全一致を主にする。原典の `State(timeout, task, version)` と `invoke` の前後の手順をそのまま写した参照モデルを用意し、操作列（影響する／しない受信、受信中の複数回の設定、期限の到来、積んだ `ReceiveTimeout` の処理、再起動、停止）ごとに、届く `ReceiveTimeout` の列を完全一致で比べる。固定 seed の乱数で操作列を作り、外部の依存は足さない。PBT は原典外であり、台帳の件数には含めない。

| 番号 | 性質 | 確かめる不変条件 |
|:---:|:---|:---|
| P1 | 任意の操作列の後で、期限があるのは設定が有限で張ってある状態のときだけであり、期限は最後に張った時刻＋期間と等しい | I1、I2 |
| P2 | 影響しないメッセージだけの列では、設定を変える操作が無い限り期限は一切動かない | I2、I7 |
| P3 | 発火1回につき積まれるのは1件だけであり、積まれた `ReceiveTimeout` は設定の変更・解除・再起動の後でも必ず届く（参照モデルと完全一致） | I1、I4 |
| P4 | 設定を変える操作が連続する列では、最後の設定だけが残り、期限は最大で1つである | I3 |
| P5 | 期間0と `Duration::MAX` を含む任意の期間で、状態の操作は panic しない（時計が無い場合の panic は §3 の決められた経路だけ） | I8、§5 の境界 |

## 7. Quint を書くか

判断：書く。理由は、本件には状態遷移があり、3状態（なし／張ってある／発火済み）と、影響する／しないメッセージの処理・再設定・停止・再起動の組み合わせの中で、発火1回につき1件であることと、積んだ `ReceiveTimeout` の確実な配送（I1・I2・I4）を守ることが正しさの中心にあるためである（独立再レビュー R2。初版の「古い期限の発火を世代で抑えることが中心」という理由は H2 で原典の振る舞いと違うことが分かり、撤回した）。これは #181 の設計が FSM のタイマで Quint を書いた理由（取り消しと置き換えの誤りを固定テストが狭くしか見ない）と同じであり、#234 の設計も mailbox の予約で同じ判断をしている。逆に、状態遷移の無い移植（#185 の DurationSpec）では Quint を書かない判断が前例であり、本件はその場合に当たらない。

- 置き場所は `crates/fraktor-actor/specs/receive_timeout.qnt` と `crates/fraktor-actor/specs/receive_timeout_test.qnt` とする（独立レビュー M5）。`scripts/quint-check-specs.sh` は `crates/*/specs/*.qnt` だけを読むため、`docs/design/specs/` では CI の検査の対象にならない。前例は状態遷移の `fsm_transition.qnt` と `mailbox_scheduling.qnt`（どちらも `crates/fraktor-actor/specs/`）である。
- 状態は、設定（`Undefined`、`Infinite`、`Finite` の区別だけを持ち、期間の値は持たない）、期限の有無、**発火済み（未処理）の印**、**キューに入った `ReceiveTimeout` の件数（整数）と配送済みの件数**、セルの生死（運転中、停止済み）、再起動の有無である（H2、独立再レビュー R2）。件数を真偽ではなく整数にするのは、二重の配送と「発火で期限を消さない場合に2件以上積まれる」負の対照を表すためである。
- 遷移は、設定する、影響するメッセージを処理する、影響しないメッセージを処理する、期限が来る、**キューに入った `ReceiveTimeout` を処理する（影響するメッセージとして消費し、配送済みの件数に数える）**、停止する、再起動する、の7つである。期限が来る手と処理の手は分ける（割り込みを置くためであり、#181 の「発行と受信を分ける」と同じ作りである）。`ReceiveTimeout` を処理する手を独立させるのは、I1 の「積まれたものは必ず届く」と「受信時に捨てる負の対照」を表すためである。
- 不変条件は I1（有限のときだけ積まれ、積まれたものは必ず届く）、I2（影響しない処理では期限が動かない）、I4（発火1回につき1件）に対応するものを置く。
- 負の対照として、受信の時点で積んだ `ReceiveTimeout` を捨てるものと、発火で期限を消さないものと、影響しないメッセージで期限を取り消すものを置き、それぞれ狙った不変条件が落ちることを確かめる（H2）。
- モデルにしないものは、実時間そのもの（期限は「来たかどうか」の真偽で表す）と、期間の算術（PBT と単体テストの範囲）である。

## 8. HOCON の設定について

原典の `ReceiveTimeoutSpec` は設定ファイルを使わない（`PekkoSpec()` に設定の上書きは無い）。したがって写す HOCON は無い。受信タイムアウトの値の渡し方は、原典でもコードの中の `setReceiveTimeout(Duration)` であり、文字列でも設定キーでもない。本書の設定の enum は、この型付きの渡し方をそのまま写したものである。前例（#154 の D1、G0b の決定 E、#195 の E1／E2）どおり、HOCON のパーサーは作らず、設定が関係する入力は型付きの値にする、という忠実さの判断をここに記録する。型の上で表せない入力（負の期間、文字列の期間）は、移植では作れない入力として扱う。原典の `FiniteDuration` は負の値を持てるが、負の期間の `setReceiveTimeout` は原典の Spec のどのケースも使っておらず、本書では `core::time::Duration` に負が無いことをもって同じ範囲外とする。

## 9. #234 の段が前提になるケースの切り分け

| ケース | 前提 | 扱い |
|:---:|:---|:---|
| 9（Timers で予定した影響しないメッセージ） | Pekko の `Timers` trait の移植と、kernel の `Scheduler` の port（#234 の段6、#257）。ケース9が確かめるのは、Timers が予定したメッセージに印が残ること（原典の `TimerSchedulerImpl` が、影響しないメッセージを包んで予定する回帰）であり、テストの中でタイマのまねをすると何も確かめないケースになる | **案Aで確定**（中将の判断）。#257 の後へ回す。ケースは unported のまま残し、数からは外さない。台帳は red のまま 15/14/14（§10）。後続の issue は中将が作る。その issue には、ケース9と「Timers の包みは、包む時点で中のメッセージを分類し、影響しないと判定されること」を書く |
| 7（scheduler で送り続ける影響しないメッセージ） | 原典は `system.scheduler` と `system.dispatcher` を使う | §1 の注2 の写し方（std のスレッドで送り続ける）で前提にならない。決着済みであり、§13 の問いには残さない |
| そのほかの13件 | 複数スレッドの dispatcher そのものは前提にならない。ただし本体の置き場所は段3 §11.11 の形が前提である（§3） | 先に進める。ただし実装は #252 のマージの後とする。段5以降では、期限の発火を scheduler への予約へ作り直す必要がある（§3）。この作り直しが要ることは #234 の設計側にも書いてもらう |

複数スレッドで時間切れが同時に起きること自体を確かめるケースは、原典の15件の中に無い。

## 10. 台帳で green にする対象ノード

対象ノードは1件だけである。

- id：`actor-tests/src/test/scala/org/apache/pekko/actor/ReceiveTimeoutSpec.scala`
- name：`actor-tests:ReceiveTimeoutSpec`
- 今の状態：`unported`、`cases.total` は `null`、`test_cases_approx: 2`（概数であり実数15とは違う。第2段で `cases.total: 15` を記録する）

第2段の終わりでの扱いは、**案Aで確定**である（中将の判断。独立レビューも案Aを推奨し、案B（除外）は採らない）。

- ケース9を #257 の後へ回し、ケースは unported のまま残す。ノードは green にしない。
- 台帳の書き方は `actor-tests:ExtensionSpec` の前例（ケース5を前提の後に残し、`status: red`、`cases 8/5/5`、`triage.kind: pending`、`reason` に残りの理由）と同じ形にする。本件では `status: red`、`cases.total: 15`、`ported: 14`、`passing: 14`、`triage.reason` に「ケース9は #257 の Timers の port の後」と書く。状態遷移の規約（unported → red → green）にも合う。
- したがって issue の文面の「台帳の対象ノードを green にする」は案Aでは満たせない。この完了の扱いは、中将が「対象ノードを red（15/14/14）のまま #200 を完了とする」と決めた（2026-11-27。§13-1）。

あわせて第2段では、`rust.crate` に `fraktor-actor`、`rust.test_file` に `crates/fraktor-actor/tests/receive_timeout_spec.rs` を記録し、`needs.testkit` は現状のままで足りる（不足は無い）。`docs/baseline/v2.0.0-undocumented-nodes.tsv` の行の追加は、ほかの移植の前例に合わせて第2段で行う。

## 11. 影響

- **公開 API**：追加は `kernel::actor` の `ReceiveTimeout`、`ReceiveTimeoutSetting`、`NotInfluenceReceiveTimeout`、`Identify` と、`ActorContext` の `set_receive_timeout` と `receive_timeout` と、`ActorSystem` の登録の口 `register_not_influence_receive_timeout` である。分類の関数 `is_not_influence_receive_timeout` は通常の公開 API にはせず、feature `test-support` の下にだけ置く（M1、独立再レビュー R3）。既存の公開 API の変更と削除は無い。
- **新しいコードファイルのヘッダー**：第2段で足す `crates/**/*.rs` の新しいファイルには、自作のものも含めて Apache-2.0 のヘッダーを付け、Pekko の移植であるファイルには原典の指定を入れる（加藤の決まり、2026-11-26。決まりの見直しは [Issue #241](https://github.com/j5ik2o/fraktor-ng-rs/issues/241) で進行中）。本書のような設計の Markdown には、既存の設計文書にヘッダーが無いため付けない。
- **台帳**：対象は §10 の1ノードだけである。本書の段では触らない。
- **#181 との重なり**：#181（[PR #275](https://github.com/j5ik2o/fraktor-ng-rs/pull/275)、設計レビュー中）は kernel の FSM にタイマの表と世代を足し、「kernel は時間を数えない」と決めた。本書は FSM の表を使わず、別の汎用タイマ表も作らないが、**kernel の実行器が時間を数える点では #181 と違う線である**（M7。理由は §3）。重なるのは「期限と世代で索引を照合する」という考え方だけであり、数えはセルと FSM で別々に持つ。将来 `Scheduler` の port が段6で入るときは、本書の期限の索引と #181 の発火の出口の両方がその port の利用者になる。その差し替えは段6の側の仕事として、ここに記録する。
- **保存済みのデータと上流**：影響は無い。Pekko の上流へ送るものも無い。
- **守りの仕組み**：CI の必須チェックは変えない。第2段で、移植したテストと PBT と Quint の検査が必須チェックの中で動くようになる。
- **実行時間**：原典の時間の値を使うため、統合テストに最大で数秒の実時間の待ちが加わる。時間の倍率（dilation）は、待ちの長さ（TestLatch、expectNoMessage）だけに掛かり、`set_receive_timeout` の値には掛からない。原典も同じである（L7）。ケース13で値に倍率を掛ける写し方をしないこと。

## 12. 第2段の進め方（設計レビューが通ってから）

1. 実装は #252 のマージの後とする。Claude Opus の担当が、`crates/fraktor-actor/tests/receive_timeout_spec.rs` に §1 の14件を書き、台帳の対象ノードを red（15/14/14 の見込み）にして、失敗を確認する（red）。ケース1は kernel の公開型 `Identify` と、`test-support` の下の分類の口 `ActorSystem::is_not_influence_receive_timeout` を使う（§3）。ケース7は std のスレッドで送り続ける形で書く（§1 の注2）。
2. 本書の担当が、§3 の本体だけを実装してテストを通す（green）。テストが誤りだと思った場合は、テストを書き換えず、triage への差し戻しとして報告する。
3. §6 の PBT（単体テストと、公開の時計を使う `tests/` の2か所）と、§7 の Quint（`crates/fraktor-actor/specs/receive_timeout.qnt` と `receive_timeout_test.qnt`）を足す。
4. 台帳の対象ノードを §10 の確定どおり（red、15/14/14）に更新し、`docs/baseline/v2.0.0-undocumented-nodes.tsv` を作り直す。
5. CI と同じ検査を流す。`cargo fmt --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`、台帳の Python テスト（`ledger/tools`）、Quint の検査である。あわせて、`test-support` を無効にした状態の検査 `cargo clippy -p fraktor-actor --lib -- -D warnings` も流し、分類の口が通常のビルドに漏れていないことを確かめる（独立再レビュー R3）。必須のチェックが1つでも落ちたら、ほかの完了を待たずに直す。
6. PR をドラフトから外し、マージできる状態にする。マージはしない。ボットの指摘とトップレベルのコメントには、それぞれ直した／別件／採らないと理由を書いて返信し、スレッドを解決する。

## 13. 中将の決定（2026-11-27）

初版と1回目のレビュー対応で「レビューで決めてほしい点」として残した3点は、中将が次のとおり決めた。決めた人は3点とも**中将（2026-11-27）**である。

1. **ケース9は案Aのとおりとし、対象ノードが green にならないまま #200 を完了とする。** ケース9の扱い自体は案Aに確定した（§9、§10）。issue の文面の「対象ノードを green にする」は満たせないため、**#200 は台帳のノードを red（15/14/14）のまま完了とし、ケース9と Timers の包みの分類は #257 に依存する後続の issue に分ける**（`actor-tests:ExtensionSpec` の前例、元帥の決定と同じ形）。後続の issue は中将が作る。
2. **ケース3の Java API の行（`getTimeout`）は、Rust の `ReceiveTimeout::timeout() -> core::time::Duration` で同じ観察を確かめる形で写す。** §1 の注1 の形である。ケースの数からは外さない。
3. **kernel の実行器に期限の索引を持たせることは、条件つきで採る。** #181 は「kernel は時間を数えない」と決め、#234 の段6が `Scheduler` の port の持ち主である。本書は受信タイムアウトのためにその分担を前倒しする（§3、M7）。条件は次の3つである。(a) 中1（独立再レビュー R1）の発火の入口・`await_envelope` の契約・索引の古い行の扱いを明記すること（§3 に明記した）。(b) 汎用のタイマの表は作らないこと。(c) 段5〜段6で `Scheduler` の port へ作り直すこと、および段5の複数スレッドの dispatcher では受信タイムアウトが未対応である制約を、設計に書くこと（§3 に書いた）。#252・#256・#257 への転記は中将が行う。

決着済みの点（初版の問いから外したもの）：ケース7の写し方は std のスレッドで送り続ける形（M4）、ケース1の `Identify` は kernel の公開型にする（M2）、発火の置き場所は段3 §11.11 を前提にし段5以降で作り直す（H1）。

## 14. 独立レビュー（PR #283）への回答表

独立レビュー（Opus、判定は要修正。高2件、中7件、低7件）の全16件への回答である。全件を「直す」とし、本文へ反映した。採らない／別件は無い。

| # | 重さ | 回答 | 理由と反映先 |
|:--|:--|:--|:---|
| H1 | 高 | 直す | 段3 §11.11 でセルの置き場所がメールボックスへ移り、段5ではアイドルのアクターの期限を誰も検査しない。「作り直しにはならない」を撤回し、段3の形を前提、待ちを最も早い期限で切ること、段5か段6で scheduler への予約へ作り直すこと、実装は #252 のマージの後とすることを §3 と §9 に書いた |
| H2 | 高 | 直す | 原典の `version` は1回の処理の前後だけで比べる値であり、積んだ `ReceiveTimeout` は設定変更後も配られる。I1 と I4 を積む時点だけの判定に書き直し、状態を「なし／張ってある／発火済み（未処理）」の3つにし、発火で期限を消すことを §2、§3、§4 に書いた。二重に届きうることは原典の振る舞いとして明記した |
| M1 | 中 | 直す | 引数なしの公開関数では登録表を読めない。登録表はシステムごと、登録は最初のメッセージより前を制約とし、登録の境界で trait を使う形に決めた。分類の関数は公開せず、`pub(crate)` か feature `test-support` の下に置くことを §3 に書いた |
| M2 | 中 | 直す | テストの中の最小の型では組み込みの判定が働かずケース1が弱まる。kernel の公開型 `Identify` を足し、自動受信は後追いの issue に分けることを §3 に書いた |
| M3 | 中 | 直す | `ActorContext` はセルに触れない。状態を `stack` と同じく文脈に貸す形にし、文脈を作る4か所（deliver、create、pre_restart、restart）の表を §3 に書いた（その後、独立再レビュー R4 で段3の実装に照らして6か所へ更新した） |
| M4 | 中 | 直す | `ManualScheduler` は実行器の `Clock` とつながっておらず、誤った実装でもケース7が通る写し方がありうる。std のスレッドで100ミリ秒ごとに送り続け、latch が開いたら止める形に §1 の注2 を書き直した |
| M5 | 中 | 直す | `docs/design/specs/` は CI の Quint 検査の対象外である。置き場所を `crates/fraktor-actor/specs/receive_timeout.qnt` に §7 で直した |
| M6 | 中 | 直す | 状態は `pub(crate)` で `tests/` から見えない。PBT を単体テストと、公開の `set_clock` を使う `tests/` に分け、参照モデルとの完全一致の形に §6 を書き直した。P3 は H2 に合わせて置き換えた |
| M7 | 中 | 直す | 「#181 と同じ線」は事実と違う。#181 は kernel が時間を数えず、本書は実行器が `Clock` を読む。違いと理由（本番の機能なので testkit に頼れない）を §3 と §11 に書き、問いの言い直しを §13-3 に置いた。表は重ねていない点は維持した |
| L1 | 低 | 直す | `TimingTest` のタグは12件であり、ケース1、14、15には無い。§1 を数え直して直した |
| L2 | 低 | 直す | 時計が無い場合を決めていなかった。前例 `require_restart_clock` に合わせ、`set_receive_timeout(Finite)` の時点で理由つきで panic することを §3 に書いた |
| L3 | 低 | 直す | 期間0では `run_until_idle` が戻らない。I8 にその注意と、確認は `run_one` か時間を区切った待ちで行うことを §4 に書いた |
| L4 | 低 | 直す | 全セルの走査は負担が増える。実行器に期限の索引を持たせ、期限が無いときは時計も読まない形を §3 に書いた。世代は索引の照合に使う |
| L5 | 低 | 直す | 再起動の扱いが抜けていた。再起動では設定を消さず `post_restart` の後に張り直すことと、処理の後の判定は失敗でも走らせることを §2 と §3 に書いた |
| L6 | 低 | 直す | I2 と I3 が原典の `checkReceiveTimeoutIfNeeded` の条件を落としていた。影響するか、版が変わったか、予定が張られているか、の条件どおりに §2 と §4 を書き直した |
| L7 | 低 | 直す | (a) Java の `cancelReceiveTimeout()` はあるため、Scala の側に限る書き方と、Java API を作らない理由を §3 に書いた。(b) 倍率は待ちだけに掛かり設定の値には掛からないことを §11 に書いた |

### 独立再レビュー（PR #283、antigravity）への回答表

独立再レビュー（判定は要修正。高0件、中3件、低2件）の全5件への回答である。全件を「直す」とし、本文へ反映した。採らない／別件は無い。あわせて §13 の3点は、中将が決定した内容（決めた人はいずれも中将（2026-11-27））へ書き直した。

| # | 重さ | 回答 | 理由と反映先 |
|:--|:--|:--|:---|
| R1 | 中 | 直す | 段3の実装（main の `executor_value.rs`）に照らすと、キューが空のときの検査入口、待ち明けの戻り値、古い索引行の扱い、時計の整合が抜けていた。期限の検査の4入口（`run_one`・`run_until_idle` の先頭、`Run` 実行の後、`await_envelope` の待ちの前後）、`await_envelope` の契約（期限で切った待ちが満了したら発火してメールボックスへ積み `true` を返す）、索引の古い行は取り消し・張り直し・停止で即時に消す形を採ること、`await_envelope` の時刻は `self.clock` 優先の現在時刻にそろえることを §3 に明記した |
| R2 | 中 | 直す | §7 の冒頭に初版の「古い期限の発火を世代で抑えることが中心」という理由が残り、H2 の改訂と矛盾していた。理由を3状態と I1・I2・I4 の保護へ書き直し、キューに入った `ReceiveTimeout` は有無ではなく件数（整数）と配送済みの件数で表すことと、`ReceiveTimeout` を処理する遷移を独立させて7遷移にすることを §7 に書いた |
| R3 | 中 | 直す | red のテストを書く前に口を確定する必要がある。`Identify` の型と `Identify::new` と `message_id()`、`NotInfluenceReceiveTimeout` の trait、`ActorSystem::register_not_influence_receive_timeout` の署名を §3 のコードブロックに明記し、`Identify(None)` は `Identify::new(Option::<()>::None)` と書くこと、ケース7のテストアクターは未知のメッセージを無視することを書いた。分類の口は、段3で既にある feature `test-support` の下の `ActorSystem::is_not_influence_receive_timeout(&self, message: &Message) -> bool` と確定し、ケース1を含め14件すべてを `tests/receive_timeout_spec.rs` に収めることと、`test-support` 無効時の clippy 検査を §3 と §12 に書いた |
| R4 | 低 | 直す | 段3で `ActorContext::new` を作る場所は6か所だった。文脈作成の表を6か所（`invoke` → `ActorCell::receive`、`create_initial`、`restart_cell` の `pre_restart` と `restart`、`run_post_stop`、`execute_child_terminated_hook`）へ更新し、`run_post_stop` は貸さず取り消し済みとすること、`execute_child_terminated_hook` は親の状態を貸し戻すことを §3 に書いた。あわせて `create_initial` の失敗リトライ（原典の `faultCreate`）で `ReceiveTimeoutState` を初期化する行を §3 に足した |
| R5 | 低 | 直す | 文言の3か所を揃えた。(1) 時計が無い条件を「`self.clock` も境界の `monotonic_time` も `None` のとき」とし、panic の文言は受信タイムアウト専用の理由にすることを §3 に書いた。(2) 冒頭文の「台帳の対象ノードを green にする」を、red（15/14/14）のまま完了とする書き方へ直した。(3) `checked_add` があふれた場合は期限なし（状態は「なし」）になることを §5 に補った |
