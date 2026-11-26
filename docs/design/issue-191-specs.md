# 設計：#191 actor の Consistency／DeadLetterSuspension／DeadLetterSupression

[Issue #191](https://github.com/j5ik2o/fraktor-ng-rs/issues/191) の設計の記録である。Pekko v2.0.0（commit `9fbd2283b1cf701c90a1223bf621942c191db61f`）の actor-tests にある `ConsistencySpec`、`DeadLetterSuspensionSpec`、`DeadLetterSupressionSpec` の 3 本を Rust へ移植し、台帳の対象ノード 3 件を green にする。

この文書は第 1 段（設計だけ）である。実装は、別の担当がこの設計をレビューして通ってから、第 2 段で red → green の順に行う。この文書自体がドラフト PR の唯一の変更である。台帳は第 2 段で更新する（この段では `status: unported` のまま触らない）。

進め方の規約は `.takt/facets/knowledge/fraktor-porting-rules.md`、設計文書の前例は [PR #205](https://github.com/j5ik2o/fraktor-ng-rs/pull/205) の [issue-177-frequency-sketch.md](issue-177-frequency-sketch.md) に従う。ノード 1 件＝テストファイル 1 件、ファイル冒頭に出典（Pekko v2.0.0 のファイルパス）をコメントで残す。

原典の綴りは `DeadLetterSupressionSpec`（`Suppression` ではない）であり、台帳の id もその綴りである。移植もファイル名とノード id は原典の綴りのままにする。

## 1. 何を移植するか

### 件数の数え直し

原典の `" in {"` の数を数え直した。`ConsistencySpec` と `DeadLetterSuspensionSpec` は台帳の目安と一致する。`DeadLetterSupressionSpec` は台帳の目安が 0 件になっているが、数え直しは 2 件であり、issue の目安（2 件）と一致する。台帳の目安 0 は誤りであり、第 2 段で台帳を更新するときに `cases.total: 2` として記録する。

| 原典 | 目安（issue／台帳） | 数え直し | 内訳 |
|:---|---:|---:|:---|
| `actor-tests/src/test/scala/org/apache/pekko/actor/ConsistencySpec.scala` | 1／1 | **1** | 「The Pekko actor model implementation must」の下に 1 件 |
| `actor-tests/src/test/scala/org/apache/pekko/actor/DeadLetterSuspensionSpec.scala` | 1／1 | **1** | 1 件の中に、数え上げ 1〜4、一時停止、再開、リセットの段がすべて入る |
| `actor-tests/src/test/scala/org/apache/pekko/actor/DeadLetterSupressionSpec.scala` | 2／0 | **2** | 宛先が停止したアクターの場合と、`system.deadLetters` の場合の 2 件 |
| 合計 | 4 | **4** | |

台帳の対象ノードは次の 3 件で、どれも `status: unported`、`triage.kind: pending` である。

- `actor-tests/src/test/scala/org/apache/pekko/actor/ConsistencySpec.scala`
- `actor-tests/src/test/scala/org/apache/pekko/actor/DeadLetterSuspensionSpec.scala`
- `actor-tests/src/test/scala/org/apache/pekko/actor/DeadLetterSupressionSpec.scala`

### 対応表：ConsistencySpec（1 件）

移す先は `crates/fraktor-actor/tests/consistency_spec.rs` である。

| # | 原典のケース | Rust のテスト名 | 移す／作らない |
|:---:|:---|:---|:---|
| 1 | provide memory consistency | `provide_memory_consistency` | 移す（形を変える。§2.4） |

原典は、fork-join の dispatcher（スレッド数はコア数の 1.5 倍、上限 2000）で `threads + 1` 個の `ConsistencyCheckingActor` を立て、各アクターへ手順番号 0〜9999 を同じ送信者から順に送り、アクターが「直前の手順番号の続きであること」と「`left.value + right.value == 42`」を毎回確かめ、最後に `"done"` へ `"done"` を返すことを検査する。失敗するとアクターが `"Test failed: ..."` を送信者へ送り、テストは `expectMsg("done")` で失敗する。

形を変える点は 3 つある。fraktor-ng-rs には dispatcher もスレッドプールも無く、executor は 1 つのシャードを逐次に配送する。そのため、スレッド数から決まるアクター数と 5 分の制限時間は写さない。アクター数は固定の 17 件、手順は原典どおり 0〜9999、制限は testkit の既定の待ち時間にする。`CacheMisaligned` のパディング（JVM の false sharing を避けるための埋め草）は写さない。各アクターの状態はそのアクターだけが持ち、共有メモリは存在しないためである。この形でも、ケースが守る性質（§3 の C1〜C3）は原典と同じであり、executor が将来並行化されたときに壊れれば落ちる回帰検査として残る。作らないケースは無い。

### 対応表：DeadLetterSuspensionSpec（1 件）

移す先は `crates/fraktor-actor/tests/dead_letter_suspension_spec.rs` である。

| # | 原典のケース | Rust のテスト名 | 移す／作らない |
|:---:|:---|:---|:---|
| 1 | must suspend dead-letters logging when reaching 'pekko.log-dead-letters', and then re-enable | `suspends_dead_letter_logging_at_the_limit_and_re_enables_after_the_suspend_duration` | 移す（形を変える。§2.3、§6） |

ケースの中身は 1 件だが、段は次のとおりであり、移植は段を 1 件のテストの中に原典の順で書く。

1. 停止したアクターへの送信が、数え上げ 1 の Info ログ（`was not delivered`）になる。
2. `Dropped` を自ら publish するアクターへの送信が、数え上げ 2 の Info ログ（`was dropped. Don't like numbers`）になる。
3. `unhandled(n)` を呼ぶアクターへの送信が、数え上げ 3 の Info ログ（`was unhandled`）になる。
4. 停止したアクターへの送信が、数え上げ 4 の Info ログになり、文面に「no more dead letters will be logged in next [2 seconds]」が付く。ここで上限（設定は 4）に達し、ログは一時停止に入る。
5. 一時停止の間に送った 2 件（停止したアクターへの 5、`Dropping` への 6）は、どちらもログにならない。ただし数え上げは進む。
6. 一時停止の期間（2 秒）が過ぎた後、停止したアクターへの 7 が Info ログになり、文面に「of which 2 were not logged. The counter will be reset now」が付く。数え上げはリセットされる。
7. 停止したアクターへの 8 が、数え上げ 1 の Info ログになる。

形を変える点は 2 つある。第 1 に、原典は設定文字列（`pekko.log-dead-letters = 4`、`pekko.log-dead-letters-suspend-duration = 2s`）でシステムを作るが、fraktor-ng-rs の `FraktorSpec` は設定文字列の構築子を持たない（`fraktor_spec.rs` の冒頭の記録どおり、設定の構築子は未移植である）。移植は設定をプログラムから渡す形（§2.3 の `DeadLetterSettings`）にする。第 2 に、原典は `Thread.sleep(2050)` で実時間を待つが、移植は `ActorSystem::set_clock` で入れる手動の `Clock`（§2.3）を使い、一時停止の期限をテストが進める。実時間の待ちは入れない。

原典のログ文面はメッセージの型名が `java.lang.Integer` である。移植の文面は Rust の型名（`i32`）になる（§2.3）。テストは型名まで含めた文面の前方一致で検査するので、この違いはテストの期待値の側に現れる。

### 対応表：DeadLetterSupressionSpec（2 件）

移す先は `crates/fraktor-actor/tests/dead_letter_supression_spec.rs` である。

| # | 原典のケース | Rust のテスト名 | 移す／作らない |
|:---:|:---|:---|:---|
| 1 | must suppress message from default dead-letters logging (sent to dead: 〈停止したアクター〉) | `suppresses_messages_sent_to_a_dead_actor` | 移す。`ActorSelection` 経由の後半を含む（§2.5） |
| 2 | must suppress message from default dead-letters logging (sent to dead: 〈deadLetters〉) | `suppresses_messages_sent_to_dead_letters` | 移す。`ActorSelection` 経由の後半を含む（§2.5） |

どちらのケースも、3 つの購読者（`DeadLetter` の購読者、`SuppressedDeadLetter` の購読者、`AllDeadLetters` の購読者）をイベントストリームへ登録し、抑制対象のメッセージ（`DeadLetterSuppression` を付けた `SuppressedMsg`）と通常のメッセージ（`NormalMsg`）を送って、次の振り分けを検査する。

- 通常のメッセージは `DeadLetter` として届き、`DeadLetter` の購読者と `AllDeadLetters` の購読者へ届く。`SuppressedDeadLetter` の購読者へは届かない。
- 抑制対象のメッセージは `SuppressedDeadLetter` として届き、宛先は元の宛先ではなく `system.deadLetters` になる（§2.2 の開封の規則）。`SuppressedDeadLetter` の購読者と `AllDeadLetters` の購読者へ届き、`DeadLetter` の購読者へは届かない。
- `AllDeadLetters` の購読者へは、抑制対象が先、通常が後の順で届く（送った順である）。
- 既定の dead letters ログには、抑制対象は現れない。この性質は、リスナーが数え上げの対象から抑制対象を外すことで守る（§2.3。SuspensionSpec の数え上げにも効く）。

ケース 1 とケース 2 の違いは宛先だけである。ケース 1 は停止したアクターへ送り、ケース 2 は `system.deadLetters` へ直接送る。ケース 2 の `DeadLetter` の宛先は `system.deadLetters` 自身になる。

作らないケースは無い（4 件全件を移す）。ただし `ActorSelection` 経由の後半は、fraktor-ng-rs に `ActorSelection` が無いため、§2.5 の最小実装を足すことが前提である。レビューで最小実装をこの issue の範囲外とする判断が出た場合は、後半はケースの除外の提案として人間の承認が要る（移植の規約により、エージェントは除外を台帳へ書けない）。その場合はこの 2 ノードを green にできず、needs_human として報告する。

## 2. 本体の有無と、足すものの形

### 2.0 既存実装の有無

`crates/` の全体を `DeadLetter`、`SuppressedDeadLetter`、`AllDeadLetters`、`Dropped`、`DeadLetterSuppression`、`DeadLetterListener`、`log-dead-letters`、`ActorSelection` で検索した。結果は次のとおりである。

| 対象 | 有無 | 根拠 |
|:---|:---|:---|
| `ActorSystem::dead_letters()`（宛先のハンドル） | ある | `kernel/executor/executor_value.rs` の `create_dead_letters` が、ルートの下の `deadLetters` というパスの function ref を作る。ハンドラは受け取ったメッセージを何もせず捨てる |
| 停止したアクターへの配送 | ある（ただし dead letter にならない） | `Executor::deliver` は、宛先のセルが表に無いとき `return` で黙って捨てる。`DeadLetter` の publish はどこにも無い |
| 棚上げとキューの残りの dead letters への転送 | ある | `stash_to_dead_letters` と `queued_to_dead_letters` が、元の送信者を保って `dead_letters` ハンドルへ送り直す。送り先のハンドラが捨てるため、現在は観測できない |
| `UnhandledMessage` | ある | `kernel/actor/unhandled_message.rs`。`Executor::publish_unhandled` がイベントストリームへ publish する。ただし原典では `UnhandledMessage` も `AllDeadLetters` の 1 種であり dead letters ログの数え上げの対象だが、移植にはその結び付けが無い |
| `Dropped` | 無い | 型そのものが無い。原典の SuspensionSpec はアクターが自ら publish するので、この issue では型の追加だけで足りる |
| `DeadLetter`、`SuppressedDeadLetter`、`AllDeadLetters` | 無い | 型そのものが無い |
| `DeadLetterSuppression`（抑制の印） | 無い | 型そのものが無い |
| `DeadLetterListener`（数え上げ、一時停止、Info ログ） | 無い | 型も、設定（`log-dead-letters` 等）も、Info ログの文面の生成も無い |
| イベントストリームの購読と publish | ある | `kernel/event/event_stream.rs`。チャンネルは `EventClass::of::<T>()` で、`declare_subclass::<Child, Parent>()` で親子を宣言できる。`Logging::Info` のイベント型も既にある |
| 時刻の port | ある | `kernel/clock.rs` の `Clock`（単調時刻）。`ActorSystem::set_clock` で executor へ入れられ、testkit 側には手動で進める時計の前例（`ManualScheduler` と、各 spec の `TestClock`）がある |
| testkit | ある | `TestProbe` 相当（`TestKit`）、`expect_msg`、`expect_no_message`、`watch` と `expect_terminated`、`TestActors::echo_actor_props`、イベントの購読で Info を受ける経路は既存の spec（`event_stream_spec.rs` 等）が使っている |
| `ActorSelection` | 無い | 型も、`ActorSystem::actor_selection` も、パス解決も無い（§2.5） |
| dispatcher、スレッドプール、設定文字列 | 無い | ConsistencySpec の形を変える理由である（§1、§2.4）。この issue では足さない |

したがって、この issue の本体作業は「dead letters の生成、振り分け、ログの数え上げと一時停止」を新規に足すことであり、既存の公開 API の変更は `ActorSelection` の追加と、dead letters ハンドルの振る舞いの変更（捨てるだけ→ publish する）だけである。

### 2.1 dead letters のイベント型

配置は前例（`unhandled_message.rs`）と同じく `kernel/actor/` の下に置く。

| ファイル | 中身 |
|:---|:---|
| `crates/fraktor-actor/src/kernel/actor/dead_letter.rs` | `DeadLetter` |
| `crates/fraktor-actor/src/kernel/actor/suppressed_dead_letter.rs` | `SuppressedDeadLetter` |
| `crates/fraktor-actor/src/kernel/actor/dropped.rs` | `Dropped` |
| `crates/fraktor-actor/src/kernel/actor/all_dead_letters.rs` | `AllDeadLetters`（チャンネルの印の型）と、3 型と `UnhandledMessage` が共有する取り出しの trait |
| `crates/fraktor-actor/src/kernel/actor/dead_letter_suppression.rs` | `DeadLetterSuppression`（印の trait）と、印の登録 |
| `crates/fraktor-actor/src/kernel/event/dead_letter_listener.rs` | `DeadLetterListener`（§2.3） |
| `crates/fraktor-actor/src/kernel/actor_system/dead_letter_settings.rs` | `DeadLetterSettings` と `DeadLetterLogLimit`（§2.3） |

公開 API の形（シグネチャは要点だけを示す）:

```rust
pub struct DeadLetter { /* message: Message, sender: ActorRef, recipient: ActorRef */ }
impl DeadLetter {
  pub fn new<M: Any + Send + Sync>(message: M, sender: ActorRef, recipient: ActorRef) -> Self;
  pub fn message(&self) -> &Message;
  pub fn sender(&self) -> &ActorRef;
  pub fn recipient(&self) -> &ActorRef;
}
// SuppressedDeadLetter も同じ形。Dropped は reason: String が増える。
pub struct Dropped { /* message, reason: String, sender, recipient */ }

pub trait AllDeadLetters: Any + Send + Sync {
  fn message(&self) -> &Message;
  fn sender(&self) -> &ActorRef;
  fn recipient(&self) -> &ActorRef;
}
// DeadLetter、SuppressedDeadLetter、Dropped、UnhandledMessage が実装する。
// UnhandledMessage は既存の型に impl を足すだけで、型そのものは変えない。
```

`AllDeadLetters` の購読は、イベントストリームの既存の仕組みで表す。`AllDeadLetters` という印の型をチャンネルの親とし、第 2 段の開始処理（システムの生成時）で `declare_subclass::<DeadLetter, AllDeadLetters>()` 等の 4 件を宣言する。購読者は `EventClass::of::<AllDeadLetters>()` で購読し、届いた `Message` から元の型（`DeadLetter` 等）を取り出す。これは `event_stream_spec.rs` が `declare_subclass` で階層を宣言する前例と同じ形である。原典の `AllDeadLetters` は trait であり、Rust では「trait をチャンネルにする」ことができないため、印の型と取り出しの trait に分ける。この分離が §8 のレビュー点 1 である。

### 2.2 dead letters の生成と振り分け

生成の場所は 3 つある。どれも、最終的にはイベントストリームへの publish に帰着する。

1. **停止した（存在しない）アクターへの配送。** `Executor::deliver` が宛先のセルを見つけられないとき、現在は黙って捨てている。ここで、宛先が `deadLetters` ハンドル自身でない限り、§2.2 の振り分け規則で publish する。棚上げとキューの残りは既に `dead_letters` ハンドルへ送り直されているので、この経路には入らない（経路 2 で処理される）。
2. **`deadLetters` ハンドルへの配送。** ハンドラを「何もしない」から「振り分け規則で publish する」へ変える。原典の `DeadLetterActorRef` と同じく、受け取ったメッセージが `DeadLetter` そのものなら中身を開封してから振り分ける（無限の循環を作らないため）。開封は再帰せず 1 段だけである。
3. **アクターが自ら publish する `Dropped` と、`publish_unhandled` の `UnhandledMessage`。** この 2 つは既にイベントストリームへ出ているので、生成側は変えない。リスナーが数え上げの対象として拾う（§2.3）。

振り分け規則（原典の `EmptyLocalActorRef`／`DeadLetterActorRef` の `specialHandle` の観測できる部分）:

- メッセージが抑制対象（§2.1 の印が付いた型）なら、`SuppressedDeadLetter(message, sender, deadLetters)` を publish する。宛先は元の宛先ではなく `deadLetters` ハンドルになる。これは SupressionSpec のケース 1 の期待値（宛先が `system.deadLetters`）が示す規則であり、ケース 2（宛先が最初から `deadLetters`）とも矛盾しない。
- それ以外なら、`DeadLetter(message, sender, recipient)` を publish する。`recipient` は元の宛先である。経路 2（宛先が `deadLetters` 自身）では `recipient` は `deadLetters` ハンドルになる。
- `DeadLetter` を開封した場合の送信者は、開封した中身の送信者である（包んだ側の送信者ではない）。SupressionSpec の `ActorSelection` の段が「recipient は同じでないので message だけを見る」としているのはこの規則のためである。
- 送信者が `deadLetters` ハンドル自身の場合は、原典は `noSender` を `deadLetters` へ置き換える規則と合わせて、送信者を `deadLetters` のままにする。fraktor-ng-rs の `tell` は送信者を常に対象のハンドルで渡すので、置き換えの分岐は作らない。

公開 API の形:

```rust
pub trait DeadLetterSuppression: Any + Send + Sync {}
// 印は型への impl で付ける。例：impl DeadLetterSuppression for SuppressedMsg {}
```

ただし、`Message` は型を消した `Arc<dyn Any + Send + Sync>` であり、ある型に印が付いているかを実行時に判定する仕組みが Rust には無い。そのため、印の判定は登録制にする。`ActorSystem`（正確には executor が持つ dead letters の振り分け器）が `TypeId` の集合を持ち、テストと利用者は次で登録する。

```rust
impl ActorSystem {
  pub fn register_dead_letter_suppression<T: DeadLetterSuppression>(&self);
}
```

これは `EventStream::declare_subclass` が「Rust には実行時のクラス階層が無いので、テストが結び付きを宣言する」としている前例と同じ判断である。登録が無い型は通常のメッセージとして扱われる。原典との違い（原典は型の trait だけで決まる）として §8 のレビュー点 2 に挙げる。対案は、`Message` の生成時に印を値として持たせる案（`Message::suppressed(...)`）であり、その場合は送信側のすべての経路が印を落とさないよう写す必要がある。

### 2.3 DeadLetterListener と設定

原典の `pekko.event.DeadLetterListener` は、イベントストリームから `DeadLetter`、`Dropped`、`UnhandledMessage` を購読するアクターであり、数え上げと上限と一時停止を持ち、`Logging::Info` をイベントストリームへ publish する。移植はアクターにはせず、`EventStream` の開始処理が作る function ref（`start_unsubscriber` と `start_unhandled_message_forwarder` の前例と同じ形）として実装する。状態はそのクロージャが持つ `Rc<RefCell<DeadLetterListener>>` に置く。

設定は値で渡す。

```rust
pub enum DeadLetterLogLimit { Off, Count(u32), Unlimited }
pub struct DeadLetterSettings {
  pub limit: DeadLetterLogLimit,          // pekko.log-dead-letters。既定は Count(10)
  pub suspend_duration: Option<Duration>, // pekko.log-dead-letters-suspend-duration。既定は Some(5s)、None は無期限
  pub during_shutdown: bool,              // pekko.log-dead-letters-during-shutdown。既定は false
}
impl ActorSystem {
  pub fn set_dead_letter_settings(&self, settings: DeadLetterSettings);
  pub fn start_dead_letter_listener(&self); // 設定が Off でないときに購読を始める
}
```

原典は `ActorSystem` の開始時に `LogDeadLetters > 0` ならリスナーを作る。移植はシステム生成の経路を変えないため、明示の開始呼び出しにする。`FraktorSpec` が設定文字列を持たないことと合わせ、テストは `set_dead_letter_settings` と `start_dead_letter_listener` を呼んでからアクターを作る。この開始の形が §8 のレビュー点 3 である。

状態機械の中身は原典の `DeadLetterListener` の字面どおりに写す。

- 状態は `Logging` と `Suspended { deadline }` の 2 つである。数え上げ `count: u32` を持つ。
- 抑制の判定は 2 段ある。振り分けの段（§2.2）で抑制対象は `SuppressedDeadLetter` になるので、リスナーは `SuppressedDeadLetter` を購読しないことで自然に外れる。もう 1 段は原典の `isWrappedSuppressed`（包まれたメッセージの中身が抑制対象の場合）と、終了中の抑制（`during_shutdown == false` かつシステムが終了中）である。前者は、リスナーが受けた `DeadLetter` の中身の型が登録済みの抑制対象なら数えないことで写す。後者は、この issue のケースが使わないため、設定の値だけを持ち、判定は `Executor::is_terminated` が真のときに数えない形で写す。
- `Logging` で受けたとき：`count` を 1 増やす（`u32::MAX` に達していたら 1 へ戻す。原典は `Int.MaxValue` で 1 へ戻して Info を出す）。`limit` が `Count(n)` で `count == n` なら、一時停止の文面を付けてログし、`Suspended` へ移る。`suspend_duration` が `None`（無期限）の原典は `receiveWithMaxCountLogging` であり、上限でリスナー自身が止まる。移植では無期限のときは `Suspended` の期限を無限とし、再開しないことで同じ観測にする。`Unlimited` なら文面を付けずに常時ログする。
- `Suspended` で受けたとき：`count` を 1 増やす。期限を過ぎていれば、その 1 件を「of which `count - limit - 1` were not logged. The counter will be reset now」の文面でログし、`count` を 0 に戻して `Logging` へ戻る。期限の判定は厳密に「過ぎている」であり、ちょうど期限の時刻では再開しない（原典の `Deadline.isOverdue`）。
- ログは `Info::new(recipient.path, log_class, text)` をイベントストリームへ publish する。文面は原典の `logDeadLetter` の形を保つ。

```text
Message [<型名>] from <sender> to <recipient> was not delivered. [<count>] dead letters encountered<doneMsg>. If this is not an expected behavior then <recipient> may have terminated unexpectedly. This logging can be turned off or adjusted with configuration settings 'pekko.log-dead-letters' and 'pekko.log-dead-letters-during-shutdown'.
Message [<型名>] to <recipient> was dropped. <reason>. [<count>] dead letters encountered<doneMsg>. …
Message [<型名>] from <sender> to <recipient> was unhandled. [<count>] dead letters encountered<doneMsg>. …
```

送信者が実在しない場合（`deadLetters` 自身である場合）に `from` を省く規則は原典の `isReal` のとおりに写す。型名は `core::any::type_name::<T>()` ではなく、メッセージが保持する型の表示名を使う。`Message` は型名を持たないため、リスナーは既知の型（`i32` は `i32`）を `type_name` で得る経路を `Message` へ 1 つ足す（`pub(crate)` の `payload_type_name`）。これは §8 のレビュー点 4 である。

期限は `Clock` port から読む。executor が既に `Clock` を持つ（`set_clock`）ので、リスナーは同じ時計への参照を受け取る。テストは手動の時計を入れて期限を進める。

### 2.4 ConsistencySpec の本体側

足す本体は無い。使うものはすべて既にある（アクターの生成、`tell`、testkit の `expect_msg`）。テスト側のアクターは原典の `ConsistencyCheckingActor` を写す。

```rust
struct ConsistencyCheckingActor { left: i64, right: i64, last_step: i64 }
// left は 42 から始め、受けるたびに left += 1、right -= 1。
// receive(step: i64): last_step != step - 1 なら sender へ "Test failed: …" を送る。
//   left + right != 42 なら sender へ "Test failed: 42 failed" を送る。
// receive("done"): sender へ "done" を送り、自分を止める。
```

状態は `i64` の 2 値だけにし、原典の `CacheMisaligned` のパディングと `Long` の各フィールドは写さない。テストは 17 個のアクターへ 0〜9999 を順に送り、全アクターから `"done"` を受ける。失敗の文面が test actor へ届いた場合は `expect_msg("done")` が失敗するので、原典と同じく失敗として観測できる。

### 2.5 ActorSelection の最小実装

SupressionSpec の 2 ケースは後半で `system.actorSelection(deadActor.path) ! msg` を使う。fraktor-ng-rs には選択の仕組みが無いため、次の最小実装を足す。

```rust
pub struct ActorSelection { /* system への弱い参照、path: ActorPath */ }
impl ActorSystem {
  pub fn actor_selection(&self, path: &ActorPath) -> ActorSelection;
}
impl ActorSelection {
  pub fn tell<M: Any + Send + Sync>(&self, message: M, sender: ActorRef);
}
```

解決の規則は、この issue のケースが要る範囲に限る。

- パスが生きているアクターを指せば、そのアクターへ通常どおり配送する。
- パスが停止したアクター、または存在しないパスを指せば、宛先はそのパスを持つ「空の参照」として扱い、§2.2 の振り分け規則で publish する。このときの `recipient` はそのパスの参照であり、`deadLetters` ハンドルではない。原典のケースが選択の段で recipient を検査しない（message だけを見る）のは、宛先が元のアクターと同一視できないためであり、移植も message だけを検査する。
- ワイルドカードを含む選択、アンカーからの相対選択、`Identify` への応答は作らない。ケースは使わない。これらはケースの「作らない」区分には当たらない（ケースは全件移す）。

配置は `crates/fraktor-actor/src/kernel/actor_selection.rs` とし、解決は executor がセルの表を持つため executor 側に `pub(crate)` の解決関数を 1 つ足す。

### 2.6 no_std

新しく足す型とリスナーは `core` と `alloc` だけで書ける。使うのは `alloc::{string::String, vec::Vec, rc::Rc}`、`core::{any::TypeId, cell::RefCell, time::Duration}` と、既存の `Clock` port である。`std` を必要とするのはテスト（スレッドは使わない。手動の時計も `alloc` と `core` で書ける）だけであり、新しいモジュールは `std` を import しない。前例どおり、クレート全体は `#![no_std]` ではない。

## 3. 守る性質（不変条件）

### dead letters の生成と振り分け

| 番号 | 不変条件 |
|:---|:---|
| D1 | 停止したアクターへの通常のメッセージ 1 件は、`DeadLetter(message, sender, そのアクター)` をちょうど 1 件 publish する。二重にも、ゼロにもならない |
| D2 | 抑制対象のメッセージ 1 件は、宛先が何であれ `SuppressedDeadLetter(message, sender, deadLetters)` をちょうど 1 件 publish し、`DeadLetter` は publish しない |
| D3 | `deadLetters` へ送られた `DeadLetter` は開封され、中身が振り分けられる。`DeadLetter` が `DeadLetter` を包む循環はできない |
| D4 | `AllDeadLetters` の購読者は、D1 と D2 の両方を送った順に受ける。`DeadLetter` の購読者は D2 を受けず、`SuppressedDeadLetter` の購読者は D1 を受けない |
| D5 | 生きているアクターへの配送は dead letter を作らない。棚上げとキューの残りの転送（既存）は、転送された 1 件につき dead letter をちょうど 1 件作る |
| D6 | 抑制対象と `DeadLetter` の開封の判定は、メッセージの内容ではなく型だけで決まる |

### DeadLetterListener

| 番号 | 不変条件 |
|:---|:---|
| L1 | `count` は、抑制されていない dead letter（`DeadLetter`、`Dropped`、`UnhandledMessage`）を受けた回数である。抑制対象と `SuppressedDeadLetter` は数えない |
| L2 | `Logging` の間は、受けた dead letter はすべて Info ログになる。`Suspended` の間は、期限を過ぎた最初の 1 件だけが Info ログになり、それ以外はログにならない |
| L3 | `count` が上限 `n` に達した 1 件は、一時停止の文面（`no more dead letters will be logged in next […]`）を付けてログされ、その直後に `Suspended` へ移る |
| L4 | `Suspended` から再開した 1 件の文面には、ログされなかった件数 `count - n - 1` が入り、その直後に `count` は 0 になり `Logging` へ戻る。次に受ける dead letter は `count == 1` でログされる |
| L5 | 期限の判定は厳密である。期限ちょうどの時刻に受けた dead letter は再開のきっかけにならず、数え上げだけが進む |
| L6 | ログの文面の数え上げは、その dead letter を数えた後の `count` と一致する。文面の形（`was not delivered`／`was dropped. <理由>`／`was unhandled`）は dead letter の種別だけで決まる |
| L7 | 上限が `Unlimited` のときは一時停止に入らない。上限が `Off` のときはリスナーは購読せず、ログは 1 件も出ない |

### Consistency

| 番号 | 不変条件 |
|:---|:---|
| C1 | 同じ送信者が同じアクターへ送ったメッセージは、送った順に処理される（手順番号が飛ばない、入れ替わらない） |
| C2 | 1 件の処理の中での状態の読み書きは、そのアクターの前後の処理と混ざらない。`left + right` は処理の前後で常に 42 である |
| C3 | `"done"` は、そのアクターが受けたすべての手順を処理した後にだけ返る |

## 4. PBT を足すか

足す。依存は足さない形（固定 seed の SplitMix64 で操作列を作り、単純なモデルと毎手比べる。前例の `frequency_sketch_property_test.rs` と #187 の設計の方針）にする。`proptest` を使うかは [#212](https://github.com/j5ik2o/fraktor-ng-rs/issues/212) の決定に従い、この issue では足さない。PBT は原典外であり、台帳の件数に含めない。

| 対象 | 性質 | 確かめる不変条件 |
|:---|:---|:---|
| リスナーの状態機械 | ランダムな dead letter の列（種別 3 種と抑制対象を混ぜる）と、ランダムな時計の進行を、手動の時計を持つリスナーへ流し、モデル（状態、`count`、ログの列）と毎手比べる。上限は 1、2、4、無制限から選び、一時停止の期間は 0、短い値、長い値から選ぶ | L1〜L6 |
| 振り分け | ランダムな宛先（生きているアクター、停止したアクター、`deadLetters`）とメッセージの種別（通常、抑制対象、`DeadLetter` で包んだもの）の組み合わせを送り、3 種の購読者が受ける列をモデルと比べる | D1〜D6 |
| Consistency | PBT は足さない。性質は順序そのものであり、ランダム化する入力が無い（手順列は固定である）。代わりにアクター数と手順数を変えた 2 通りの実行を通常のテストとして書く | C1〜C3 |

## 5. Quint で書くかどうか

**DeadLetterListener の一時停止については Quint を書く。ほかの 2 つについては書かない。**

### 書く：DeadLetterListener の一時停止

理由は、この部分が時刻に依存する状態遷移であり、壊れ方が遷移の列の全体でしか出ないためである。数え上げをリセットし忘れる誤り、抑制対象を数えてしまう誤り、一時停止の間にログしてしまう誤り、再開の文面の件数を 1 つずらす誤りは、固定のケース（SuspensionSpec の 1 件）では一部の順序しか踏まない。FrequencySketch の前例（#177）でも、リセットの後の大きさの取り違えは PBT と原典のケースを通り、Quint だけが落とした。

仕様は `docs/design/specs/dead_letter_listener.qnt`（本体）と `docs/design/specs/dead_letter_listener_test.qnt`（インスタンスと負の対照）に置く。

- 状態：`mode`（`Logging` か `Suspended`）、`count`、`now`（時刻。整数の刻み）、`deadline`、`logged`（ログした dead letter の列の長さと、最後の文面の種別）。
- 遷移：`receiveDeadLetter`、`receiveDropped`、`receiveUnhandled`、`receiveSuppressed`、`tick`（時刻を 1 刻み進める）。上限と一時停止の期間はインスタンスの定数にする。
- 不変条件：`countIsUnsuppressedSeen`（L1）、`noLogWhileSuspended`（L2。期限を過ぎていない `Suspended` では `logged` が増えない）、`suspendAtLimit`（L3）、`resumeResetsCount`（L4。再開の直後は `count == 0` で `mode == Logging`）、`deadlineIsStrict`（L5。`now == deadline` では再開しない）。
- インスタンス：`limit4`（上限 4、期間 2。原典の設定）、`limit1`（上限 1。最初の 1 件で一時停止に入る）、`limit2short`（上限 2、期間 0。期限が即座に過ぎる）、`unlimited`（一時停止に入らない）。
- 負の対照：`bugNoReset`（再開で `count` を戻さない）、`bugCountsSuppressed`（抑制対象を数える）、`bugLogsWhileSuspended`（一時停止でもログする）、`bugResumeAtDeadline`（`now == deadline` で再開する）、`bugOffByOneNotLogged`（再開の文面の件数を 1 つずらす）。それぞれ狙った不変条件が落ちることを確かめる。

モデルにしていないもの：ログの文面の文字列そのもの（件数と種別だけを持つ。文面は §6 の単体テストと原典のケースで確かめる）、`u32::MAX` での数え上げの巻き戻し（到達しない）、終了中の抑制（ケースが使わない）。

### 書かない：振り分けと Consistency

- **振り分け（書かない）。** 状態を持たない。1 件の入力に対する出力が型と宛先だけで決まる純粋な規則であり、§4 のモデル PBT が入力の組み合わせ全体を比べる。遷移の列でしか出ない壊れ方は無い。
- **Consistency（書かない）。** 守る性質は配送の順序と、1 アクターの状態の不可分性であり、Quint はスレッドとメモリの可視性をモデルにしないため、確かめる対象にならない。原典自身も実行でしか確かめていない。壊れ方は executor の配送順序の変更であり、これはケースの実行が落とす。

なお、指示文にある「メールボックスの一時停止」は、この 3 本の原典には現れない。DeadLetterSuspensionSpec の suspension は dead letters ログの一時停止であり、アクターのメールボックスの停止（Pekko の `Mailbox` の suspended 状態）ではない。fraktor-ng-rs の executor にはメールボックスの停止という状態も無い。したがって、メールボックスの停止についての Quint は、この issue では書かない。

## 6. 壊しうる入力

| 対象 | 入力 | 何が起きうるか | 確かめるもの |
|:---|:---|:---|:---|
| 振り分け | `deadLetters` へ `DeadLetter` を送る | 開封せずに包んだまま publish すると、購読者が二重の包みを受け、リスナーが中身の型で文面を作れなくなる | ケース 1 と 2 の選択の段、§4 の振り分け PBT |
| 振り分け | `DeadLetter` が抑制対象を包む | 包みの型だけを見ると通常の dead letter になり、抑制が漏れる。原典のリスナーは包みの中身も見る（`isWrappedSuppressed`） | リスナーの単体テスト（L1） |
| 振り分け | 抑制対象を生きているアクターへ送る | 通常どおり配送される。dead letter にならない限り抑制は効かない | 振り分け PBT の宛先の部品に含める |
| 振り分け | 送信者が `deadLetters` 自身である dead letter | 文面の `from` を出すと原典と違う文面になる | リスナーの単体テスト（`isReal` の規則） |
| 振り分け | 空文字のメッセージ、極端に長い文字列のメッセージ | 型名と宛先のパスの表示だけが変わり、振り分けは変わらない | 振り分け PBT の部品に含める |
| リスナー | 上限が 1 | 最初の 1 件で一時停止に入り、文面が付く | Quint の `limit1`、単体テスト |
| リスナー | 上限が 0（`Off`） | リスナーが購読せず、数え上げもログも無い | 単体テスト（L7） |
| リスナー | 一時停止の期間が 0 | 期限は到達した瞬間に過ぎているので、次の dead letter が即座に再開の 1 件になる。再開の文面の件数は 0 になりうる | Quint の `limit2short`、PBT の期間の部品 |
| リスナー | 一時停止の期間が無期限（`None`） | 再開しない。上限に達した後は二度とログしない | 単体テスト |
| リスナー | 期限ちょうどの時刻に届く dead letter | 再開しない（L5）。1 刻み前と 1 刻み後で結果が変わる境界である | Quint の `deadlineIsStrict`、PBT は時刻の進行に期限の前後を含める |
| リスナー | 一時停止の間に届く大量の dead letter | 数え上げだけが進み、再開の文面の件数が大きくなる。件数の計算 `count - limit - 1` が負にならないのは、再開が上限到達の後にだけ起きるためである | PBT、Quint |
| リスナー | 数え上げが `u32::MAX` に達する | 原典は `Int.MaxValue` で 1 へ戻す。移植も同じく戻す。この境界を実行で踏む検査は無い（到達に 2^32 件が要る） | 範囲の外（この文書に明記） |
| Consistency | アクター数が 1、または手順数が 0 | アクター数 1 では順序の検査だけが残る。手順数 0 では `"done"` だけが往復する | §4 の 2 通りの実行 |
| Consistency | 失敗の文面が test actor へ届く順序 | `"done"` の期待が失敗の文面で失敗し、ケースが落ちる。原典と同じ観測である | ケース 1 |
| ActorSelection | 存在しないパスへの選択 | 空の参照として振り分けられる。パスが長い、空の要素を含む場合も同じである | ケース 1 と 2 の選択の段 |
| ActorSelection | ワイルドカードを含むパス | この issue では作らない。ワイルドカードは通常の要素として扱われ、一致するアクターは無い | 範囲の外（この文書に明記） |

「ワイルドカードの連続」に当たる入力は、ActorSelection のワイルドカードを作らないため、この issue には無い。

## 7. 影響

- **公開 API**：追加が中心である。`fraktor_actor::kernel::actor::{DeadLetter, SuppressedDeadLetter, Dropped, AllDeadLetters, DeadLetterSuppression}`、`kernel::actor_system::{DeadLetterSettings, DeadLetterLogLimit}`、`ActorSystem::{actor_selection, register_dead_letter_suppression, set_dead_letter_settings, start_dead_letter_listener}`、`kernel::actor_selection::ActorSelection`。`UnhandledMessage` へ `AllDeadLetters` の impl を足す。
- **振る舞いの変更**：`deadLetters` ハンドルが、受け取ったメッセージを捨てるだけではなくイベントストリームへ publish するようになる。既存のテストで `dead_letters()` を送信者として使うものは影響を受けない（送信者として使う分には publish は起きない）。ただし、停止したアクターの棚上げとキューの残りを `deadLetters` へ転送する既存の経路は、これまで観測できなかった dead letter を publish するようになる。購読者がいないシステムでは publish は何も起こさないが、イベントストリームの購読を持つ既存のテスト（`event_stream_spec.rs` 等）が dead letter を受ける可能性がある。第 2 段では `cargo test --workspace` でこの影響を確かめ、落ちたテストは実装でごまかさず報告する。
- **台帳**：第 2 段で対象ノード 3 件を green（1/1/1、1/1/1、2/2/2、`verified_against: v2.0.0`）にし、`docs/baseline/v2.0.0-undocumented-nodes.tsv` を作り直す。ゴール文書 `docs/goals/issue-191-specs.md` は第 2 段で作る。
- **依存**：足さない。
- **計算量**：dead letter の publish は購読者の数に比例する。リスナーの処理は 1 件あたり定数である。Consistency のテストは 17 アクター× 10,000 手順で、逐次 executor では 170,000 配送になる。実行時間が問題になる場合は、手順数を変えずアクター数を先に調整し、その変更を報告する。

## 8. レビューで見てほしい点

1. **`AllDeadLetters` を印の型と trait に分ける形（§2.1）。** 原典の trait をチャンネルにできないための分離である。対案は、`AllDeadLetters` を列挙型にして 3 型と `UnhandledMessage` を包む案であり、その場合は個別のチャンネルの購読者が列挙型から取り出す形に変わる。どちらを取るかを確認してほしい。
2. **抑制の印を登録制にする形（§2.2）。** 原典は型の trait だけで決まるが、Rust では実行時に判定できないため `TypeId` の登録にした。対案は `Message` に印を持たせる案である。登録し忘れが「抑制されない」という静かな失敗になる点を含めて確認してほしい。
3. **リスナーの開始を明示の呼び出しにする形（§2.3）。** 原典はシステム開始時に自動で作る。移植は `FraktorSpec` に設定の構築子が無いため明示にした。システム生成の経路を変えて自動にする案との比較を確認してほしい。
4. **ログ文面の型名（§2.3）。** `java.lang.Integer` にはならず `i32` になる。`Message` へ型名の取り出しを足す案であり、文面の前方一致で検査する前提である。
5. **ConsistencySpec の形の変更（§1、§2.4）。** アクター数を固定の 17 にし、dispatcher と 5 分の制限とパディングを写さない案である。逐次 executor では性質が自明に成り立つため、このケースを移植する意味は回帰検査としての位置付けになる。その位置付けで green としてよいかを確認してほしい。
6. **ActorSelection の最小実装をこの issue に含める判断（§2.5）。** 含めない場合は SupressionSpec の 2 ケースの後半が除外の提案になり、人間の承認なしには green にできない。範囲を広げてよいかを確認してほしい。
7. **Quint をリスナーの一時停止に限る判断（§5）。** 特に、振り分けを「状態を持たない」として Quint の対象外とした理由と、メールボックスの一時停止は原典に無いとした読みを確認してほしい。
8. **作らないもの（§2.5、§2.3）。** ActorSelection のワイルドカードと `Identify`、リスナーの終了中の抑制の完全な写し（設定の値と `is_terminated` での判定まで）を、この issue の範囲外とした判断を確認してほしい。
