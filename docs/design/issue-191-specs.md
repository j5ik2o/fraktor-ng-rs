# 設計：#191 actor の DeadLetterSuspension／DeadLetterSupression

[Issue #191](https://github.com/j5ik2o/fraktor-ng-rs/issues/191) の設計の記録である。Pekko v2.0.0（commit `9fbd2283b1cf701c90a1223bf621942c191db61f`）の actor-tests にある `DeadLetterSuspensionSpec` と `DeadLetterSupressionSpec` の 2 本を Rust へ移植し、台帳の対象ノード 2 件を green にする。`ConsistencySpec` は元帥の決定（2026-11-26）でこの issue から外れた。経緯は §1 に書く。

この文書は第 1 段（設計）の記録であり、第 2 段（実装）は設計レビューを通ってから red → green の順に行う。台帳は第 2 段で更新する。

進め方の規約は `.takt/facets/knowledge/fraktor-porting-rules.md`、設計文書の前例は [PR #205](https://github.com/j5ik2o/fraktor-ng-rs/pull/205) の [issue-177-frequency-sketch.md](issue-177-frequency-sketch.md) に従う。ノード 1 件＝テストファイル 1 件、ファイル冒頭に出典（Pekko v2.0.0 のファイルパス）をコメントで残す。

原典の綴りは `DeadLetterSupressionSpec`（`Suppression` ではない）であり、台帳の id もその綴りである。移植もファイル名とノード id は原典の綴りのままにする。

## 1. 何を移植するか

### 件数の数え直し

原典の `" in {"` の数を数え直した。`DeadLetterSuspensionSpec` は台帳の目安と一致する。`DeadLetterSupressionSpec` は台帳の目安が 0 件になっているが、数え直しは 2 件であり、issue の目安（2 件）と一致する。台帳の目安 0 は誤りであり、第 2 段で台帳を更新するときに `cases.total: 2` として記録する。`ConsistencySpec` は数え直し 1 件であるが、この issue の対象からは外れた（下の段落）。

| 原典 | 目安（issue／台帳） | 数え直し | 内訳 |
|:---|---:|---:|:---|
| `actor-tests/src/test/scala/org/apache/pekko/actor/DeadLetterSuspensionSpec.scala` | 1／1 | **1** | 1 件の中に、数え上げ 1〜4、一時停止、再開、リセットの段がすべて入る |
| `actor-tests/src/test/scala/org/apache/pekko/actor/DeadLetterSupressionSpec.scala` | 2／0 | **2** | 宛先が停止したアクターの場合と、`system.deadLetters` の場合の 2 件 |
| 合計 | 3 | **3** | |

台帳の対象ノードは次の 2 件で、どれも `status: unported`、`triage.kind: pending` である。

- `actor-tests/src/test/scala/org/apache/pekko/actor/DeadLetterSuspensionSpec.scala`
- `actor-tests/src/test/scala/org/apache/pekko/actor/DeadLetterSupressionSpec.scala`

### ConsistencySpec をこの issue から外した理由

`ConsistencySpec`（数え直し 1 件）は、元帥の決定（2026-11-26）で、この issue から外し、未移植のまま後回しにする。除外ではない。別の issue [#235](https://github.com/j5ik2o/fraktor-ng-rs/issues/235) に分け、複数スレッドの実行器の設計 [#234](https://github.com/j5ik2o/fraktor-ng-rs/issues/234) の後に置いた。理由は、ケースが守る性質（1 つのシステムの中で、アクターが実際に 2 つ以上のスレッドで同時に動き、その状態の見え方が一貫すること）を、fraktor-ng-rs の今の作りでは確かめられないためである。`ActorSystem` は 1 シャード＝1 executor で、executor は `Rc<RefCell<Executor>>` の後ろにあり、event stream も `Rc<RefCell<StreamSetup>>` を持つため、システムは `Send` でも `Sync` でもない（再レビューの試作で `assert_send::<ActorSystem>()` は E0277 が 3 件）。複数スレッドで動く実行器は無く、`Executor` は `pub(crate)` なのでテストの側からも組めない。「スレッドごとに 1 つずつシステムを作る」形は、1 つのシステムの中でアクターが複数スレッドで動くことにはならない。この PR では、ConsistencySpec の台帳ノードを `unported` のまま変えず、設計からも外す。アクター数を原典の式で求める形などの設計の材料は、#235 の側で使う。

### 対応表：DeadLetterSuspensionSpec（1 件）

移す先は `crates/fraktor-actor/tests/dead_letter_suspension_spec.rs` である。

| # | 原典のケース | Rust のテスト名 | 移す／作らない |
|:---:|:---|:---|:---|
| 1 | must suspend dead-letters logging when reaching 'pekko.log-dead-letters', and then re-enable | `suspends_dead_letter_logging_at_the_limit_and_re_enables_after_the_suspend_duration` | 移す（形を変える。§2.3、§6） |

ケースの中身は 1 件だが、段は次のとおりであり、移植は段を 1 件のテストの中に原典の順で書く。

1. 停止したアクターへの送信が、数え上げ 1 の Info ログ（`was not delivered`）になる。
2. `Dropped` を自ら publish するアクターへの送信が、数え上げ 2 の Info ログ（`was dropped. Don't like numbers`）になる。
3. `unhandled(n)` を呼ぶアクターへの送信が、数え上げ 3 の Info ログ（`was unhandled`）になる。
4. 停止したアクターへの送信が、数え上げ 4 の Info ログになり、文面に「no more dead letters will be logged in next [2.000 s]」が付く。期間の表示は原典の `PrettyDuration.pretty`（`%.4g` と単位の略号）であり、「2 seconds」ではない（§2.3）。ここで上限（設定は 4）に達し、ログは一時停止に入る。
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
- 抑制対象のメッセージは `SuppressedDeadLetter` として届き、宛先は元の宛先ではなく、その dead letter を処理した参照になる（§2.2）。停止したアクター宛てと `deadLetters` 宛てでは処理するのが `deadLetters` の参照なので、宛先は `system.deadLetters` になる。`SuppressedDeadLetter` の購読者と `AllDeadLetters` の購読者へ届き、`DeadLetter` の購読者へは届かない。
- `AllDeadLetters` の購読者へは、抑制対象が先、通常が後の順で届く（送った順である）。
- 既定の dead letters ログには、抑制対象は現れない。この性質は、リスナーが数え上げの対象から抑制対象を外すことで守る（§2.3。SuspensionSpec の数え上げにも効く）。

ケース 1 とケース 2 の違いは宛先だけである。ケース 1 は停止したアクターへ送り、ケース 2 は `system.deadLetters` へ直接送る。ケース 2 の `DeadLetter` の宛先は `system.deadLetters` 自身になる。

`ActorSelection` 経由の後半の比べ方は、ケースで違う。ケース 1 は、宛先が元のアクターと同一視できないため message だけを見る（原典 `:68-70`）。ケース 2 は、`system.deadLetters` への選択が deadLetters の参照そのものに解決されるため、`DeadLetter(NormalMsg, testActor, system.deadLetters)` と宛先まで含めて比べる（原典 `:103-104`）。移植もこの違いのままに比べる。そのため §2.5 の解決は、function ref（deadLetters を含む）も対象にしなければならない（H2）。

作らないケースは無い（4 件全件を移す）。`ActorSelection` は、設計レビューの推奨どおり、この issue に含める（§2.5）。含めなければ 2 ノードとも green にできないことが理由の 1 つである。

## 2. 本体の有無と、足すものの形

### 2.0 既存実装の有無

`crates/` の全体を `DeadLetter`、`SuppressedDeadLetter`、`AllDeadLetters`、`Dropped`、`DeadLetterSuppression`、`DeadLetterListener`、`log-dead-letters`、`ActorSelection` で検索した。結果は次のとおりである。

| 対象 | 有無 | 根拠 |
|:---|:---|:---|
| `ActorSystem::dead_letters()`（宛先のハンドル） | ある | `kernel/executor/executor_value.rs` の `create_dead_letters` が、ルートの下の `deadLetters` というパスの function ref を作る。ハンドラは受け取ったメッセージを何もせず捨てる |
| 停止したアクターへの配送 | ある（ただし dead letter にならない） | `Executor::deliver` は、宛先のセルが表に無いとき `return` で黙って捨てる。`DeadLetter` の publish はどこにも無い |
| 棚上げとキューの残りの dead letters への転送 | ある（ただし包んでいない） | `stash_to_dead_letters` と `queued_to_dead_letters` が、元の送信者を保って生のメッセージを `dead_letters` ハンドルへ送り直す。原典は `DeadLetter(msg, sender, 元の宛先)` に包んで送る（`Mailboxes.scala:71`）ので、宛先が保たれる。移植は包む形へ変える（M3、§2.2） |
| `UnhandledMessage` | ある | `kernel/actor/unhandled_message.rs`。`Executor::publish_unhandled` がイベントストリームへ publish する。ただし原典では `UnhandledMessage` も `AllDeadLetters` の 1 種であり dead letters ログの数え上げの対象だが、移植にはその結び付けが無い |
| `Dropped` | 無い | 型そのものが無い。原典の SuspensionSpec はアクターが自ら publish するので、この issue では型の追加だけで足りる |
| `DeadLetter`、`SuppressedDeadLetter`、`AllDeadLetters` | 無い | 型そのものが無い |
| `DeadLetterSuppression`（抑制の印） | 無い | 型そのものが無い |
| `DeadLetterListener`（数え上げ、一時停止、Info ログ） | 無い | 型も、設定（`log-dead-letters` 等）も、Info ログの文面の生成も無い |
| イベントストリームの購読と publish | ある | `kernel/event/event_stream.rs`。チャンネルは `EventClass::of::<T>()` で、`declare_subclass::<Child, Parent>()` で親子を宣言できる。`Logging::Info` のイベント型も既にある |
| 時刻の port | ある | `kernel/clock.rs` の `Clock`（単調時刻）。`ActorSystem::set_clock` で executor へ入れられ、testkit 側には手動で進める時計の前例（`ManualScheduler` と、各 spec の `TestClock`）がある |
| testkit | ある（ただし `EventFilter` に Info と前方一致が無い） | `TestProbe` 相当（`TestKit`）、`expect_msg`、`expect_no_message`、`watch` と `expect_terminated`、`TestActors::echo_actor_props` はある。`EventFilter` は Error と Warning だけで、`info` と `start` の前方一致が無く、`intercept` が読むのは event stream ではなく `event_sink` の登録表である（M5）。SuspensionSpec が使う形を testkit へ足す（§2.3） |
| `ActorSelection` | 無い | 型も、`ActorSystem::actor_selection` も、パス解決も無い（§2.5） |
| dispatcher、スレッドプール、設定文字列 | 無い | この issue では足さない |

したがって、この issue の本体作業は「dead letters の生成、振り分け、ログの数え上げと一時停止」を新規に足すことであり、既存の公開 API の変更は `ActorSelection` の追加と、dead letters ハンドルの振る舞いの変更（捨てるだけ→ publish する）だけである。

### 2.1 dead letters のイベント型

配置は前例（`unhandled_message.rs`）と同じく `kernel/actor/` の下に置く。

| ファイル | 中身 |
|:---|:---|
| `crates/fraktor-actor/src/kernel/actor/dead_letter.rs` | `DeadLetter` |
| `crates/fraktor-actor/src/kernel/actor/suppressed_dead_letter.rs` | `SuppressedDeadLetter` |
| `crates/fraktor-actor/src/kernel/actor/dropped.rs` | `Dropped` |
| `crates/fraktor-actor/src/kernel/actor/all_dead_letters.rs` | `AllDeadLetters`（チャンネルの印の型、ZST）と `AnyDeadLetter`（見る側の列挙型） |
| `crates/fraktor-actor/src/kernel/actor/dead_letters_shared.rs` | `DeadLettersShared`（ハンドラと executor が共有する欄。H3、§2.2） |
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
// Dropped の送信者が無い形（原典の Dropped.apply(message, reason, recipient)）は、
// fraktor-ng-rs に noSender が無いため、送信者に deadLetters の参照を入れる約束にする（L5）。
pub struct Dropped { /* message, reason: String, sender, recipient */ }

pub struct AllDeadLetters; // チャンネルの印だけの型（ZST）。値としては publish しない。

pub enum AnyDeadLetter<'a> {
  Dead(&'a DeadLetter),
  Suppressed(&'a SuppressedDeadLetter),
  Dropped(&'a Dropped),
  Unhandled(&'a UnhandledMessage),
}
impl<'a> AnyDeadLetter<'a> {
  pub fn from_message(message: &'a Message) -> Option<Self>;
  pub fn message(&self) -> &Message;
  pub fn sender(&self) -> &ActorRef;
  pub fn recipient(&self) -> &ActorRef;
}
```

`AllDeadLetters` の購読は、イベントストリームの既存の仕組みで表す。印の型 `AllDeadLetters` をチャンネルの親とし、システムの生成時に `declare_subclass::<DeadLetter, AllDeadLetters>()` 等の 4 件を宣言する（リスナーの有無とは関係なく宣言する。SupressionSpec はリスナーを使わないが `AllDeadLetters` を購読するためである）。購読者は `EventClass::of::<AllDeadLetters>()` で購読し、届いた `Message` を `AnyDeadLetter::from_message` で判別する。

テストでの `DeadLetter` の比べ方は、`Message` に `PartialEq<Message>` が無いため、payload を元の型へ downcast してから、message、sender、recipient を要素ごとに比べる形にする（L5）。

設計レビュー（M4 と論点 1）で、前の版の「同名の trait」は採らないと決めた。同名の struct と trait は E0428 になり、`dyn AllDeadLetters` は `EventClass::of` の `Sized` の要求で E0277 になり、型を消した `Message` からは `&dyn Trait` を取り出せない。したがって、チャンネルには印の型を残し、trait の代わりに見る側の列挙型 `AnyDeadLetter` を置く。publish する値そのものを列挙型にする対案は、型ごとのチャンネル（`DeadLetter` だけを購読する probe）を壊すので採らない。

### 2.2 dead letters の生成と振り分け

生成の場所は 3 つある。どれも、最終的にはイベントストリームへの publish に帰着する。

1. **停止した（存在しない）アクターへの配送。** `Executor::deliver` が宛先のセルを見つけられないとき、現在は黙って捨てている。ここで、宛先が `deadLetters` ハンドル自身でない限り、振り分け規則で publish する。ただし、**メッセージが `DeadLetter` そのものなら、publish せずに捨てる**（H1）。これは原典の規則（`dispatch/Mailboxes.scala:69-71` の `case _: DeadLetter => // actor subscribing to DeadLetter, drop it`）であり、これが無いと、`DeadLetter` を購読したまま止まったアクターへ dead letter が配送され続け、`run_until_idle` が返らない循環になる。
2. **`deadLetters` ハンドルへの配送。** ハンドラを「何もしない」から「振り分け規則で publish する」へ変える。受け取ったメッセージが `DeadLetter` なら、特別扱い（中身が抑制対象の場合）に当たらない限り、**宛先を保ったままそのまま publish する**（M3。原典の `DeadLetterActorRef.!` の `case d: DeadLetter => if (!specialHandle(...)) eventStream.publish(d)`）。棚上げとキューの残りの転送は、この規則に合わせて、生のメッセージではなく `DeadLetter(message, sender, 元の宛先)` に包んで `deadLetters` へ送る形へ変える。現在の生のままの転送では、宛先が deadLetters になってしまうためである。
3. **アクターが自ら publish する `Dropped` と、`publish_unhandled` の `UnhandledMessage`。** この 2 つは既にイベントストリームへ出ているので、生成側は変えない。リスナーが数え上げの対象として拾う（§2.3）。

ハンドラと executor の共有の置き方（H3）。deadLetters のハンドラは executor の借用の中で同期に走る（`FunctionBoundary::post` がその場で呼び、`run_until_idle` は `executor.borrow_mut()` を保ったまま配送する）。そのため、ハンドラが executor や `ActorSystem` を借りに行く形は、二重借用で panic する。また `Rc<RefCell<_>>` を捕まえるクロージャは `function_ref` の `Send + Sync + 'static` の要求で E0277 になる。したがって、ハンドラが要る状態はすべて共有の欄 `DeadLettersShared` に置く。

```rust
pub(crate) struct DeadLettersShared {
  bus: spin::Mutex<Option<EventStreamBusWeak>>,   // システムの生成後に埋める
  suppression: spin::Mutex<BTreeSet<TypeId>>,      // §2.2 の登録表（HashSet は alloc に無い。再レビューの低 3）
  self_ref: spin::Mutex<Option<ActorRef>>,         // deadLetters 自身。生成後に埋める
  clock: spin::Mutex<Option<Arc<dyn Clock>>>,      // リスナーの開始時に捕まえる
  terminating: AtomicBool,                         // 現在は常に false（§2.3）
}
```

`DeadLettersShared` は `Executor::new` が作り、`Arc` でハンドラと executor が共有する。bus と自分の参照は、deadLetters のハンドルが event stream より先に作られる（`executor_value.rs:163-191`）ため、システムの生成が済んだ後に埋める。埋まる前の配送は、現在と同じく捨てる。

振り分け規則（原典の `EmptyLocalActorRef`／`DeadLetterActorRef` の `specialHandle` と `Mailboxes.scala` の観測できる部分）:

- メッセージが `DeadLetter` で、宛先が停止したアクターなら、捨てる（H1）。
- メッセージが抑制対象なら、`SuppressedDeadLetter(message, sender, 処理した参照)` を publish する（M1）。宛先は、その dead letter を処理した参照である。停止したアクター宛てと `deadLetters` 宛てでは処理するのが deadLetters の参照なので宛先は `system.deadLetters` になり、見つからないパスへの選択では処理するのがその空の参照なので宛先は空の参照になる。
- それ以外なら、`DeadLetter(message, sender, recipient)` を publish する。`recipient` は元の宛先である。経路 2 で受けた `DeadLetter` は、宛先を保ったままそのまま publish する。
- `DeadLetter` の中身が抑制対象の場合は、その `DeadLetter` を publish せず、中身について `SuppressedDeadLetter` を publish する。送信者は中身の送信者である。
- 送信者が `deadLetters` ハンドル自身の場合は、送信者を `deadLetters` のままにする。fraktor-ng-rs の `tell` は送信者を常に対象のハンドルで渡すので、`noSender` の置き換えの分岐は作らない。

公開 API の形:

```rust
pub trait DeadLetterSuppression: Any + Send + Sync {}
// 印は型への impl で付ける。例：impl DeadLetterSuppression for SuppressedMsg {}
```

ただし、`Message` は型を消した `Arc<dyn Any + Send + Sync>` であり、ある型に印が付いているかを実行時に判定する仕組みが Rust には無い。そのため、印の判定は登録制にする。登録表は `DeadLettersShared`（H3 の共有の欄）に置き、テストと利用者は次で登録する。

```rust
impl ActorSystem {
  pub fn register_dead_letter_suppression<T: DeadLetterSuppression>(&self);
}
```

これは `EventStream::declare_subclass` が「Rust には実行時のクラス階層が無いので、テストが結び付きを宣言する」としている前例と同じ判断であり、設計レビューの論点 2 もこの形を推奨した。`register::<T: DeadLetterSuppression>` という境界があるので、登録し忘れは印の付け忘れと同じ扱いになり、許容する。`Message` に印を持たせる対案は、送信の経路（転送、選択、forward）ごとに印を運ぶ必要があるので採らない。登録が無い型は通常のメッセージとして扱われる。

kernel 自身の抑制対象は、システムの生成時に kernel が既定で登録する（M2）。対象は次のとおりである。

- `Terminated`（型で登録する）。これを登録しないと、`queued_to_dead_letters` が DeathWatch の通知を deadLetters へ送るたびに数え上げとログが起きる。
- 文字列の制御メッセージ `"PoisonPill"` と `"Kill"`。この 2 つは fraktor-ng-rs では型ではなく `&'static str` の値である（`executor_value.rs:61-62`）。`&str` の型を登録すると `"done"` などの文字列がすべて抑制されてしまうため、この 2 つは型ではなく**値で判定する**（payload が `&str` で、値が `"PoisonPill"` か `"Kill"` に等しいときだけ抑制対象とする）。

### 2.3 DeadLetterListener と設定

原典の `pekko.event.DeadLetterListener` は、イベントストリームから `DeadLetter`、`Dropped`、`UnhandledMessage` を購読するアクターであり、数え上げと上限と一時停止を持ち、`Logging::Info` をイベントストリームへ publish する。移植はアクターにはせず、`EventStream` の開始処理が作る function ref（`start_unsubscriber` と `start_unhandled_message_forwarder` の前例と同じ形）として実装する。状態は `Arc<spin::Mutex<DeadLetterListener>>` に置く（H3。`Rc<RefCell<_>>` は `Send + Sync` の要求で E0277 になる）。リスナーは executor を借りに行かず、要るもの（時計、自分の購読先の bus、event sink、終了の旗）は開始のときに捕まえる（再レビューの低 4。sink へも出すため、sink も捕まえる）。

設定は値で渡し、開始は 1 回の呼び出しにまとめる（L7。設定と開始を分けると、開始の後に変えた設定と時計が反映されないためである）。

```rust
pub enum DeadLetterLogLimit { Off, Count(NonZeroU32), Unlimited }
pub struct DeadLetterSettings {
  pub limit: DeadLetterLogLimit,          // pekko.log-dead-letters。既定は Count(10)
  pub suspend_duration: Option<Duration>, // pekko.log-dead-letters-suspend-duration。既定は Some(5 分)、None は無期限
  pub during_shutdown: bool,              // pekko.log-dead-letters-during-shutdown。既定は false
}
impl ActorSystem {
  pub fn start_dead_letter_listener(&self, settings: DeadLetterSettings);
}
```

上限の端の値の写像は原典のとおりにする（L4）。設定値が 0 なら `Off`（リスナーを作らない）、`Int.MaxValue` に当たる値なら `Unlimited`（常時ログする）、それ以外は `Count` である。`Count` の中身を `NonZeroU32` にするのは、0 の `Count` が「いつまでも一時停止に入らずログし続ける」という原典に無い意味になるためである。一時停止の期間の既定は 5 分である（L2。原典の `reference.conf` の `pekko.log-dead-letters-suspend-duration = 5 minutes`）。

原典は `ActorSystem` の開始時に `LogDeadLetters > 0` ならリスナーを作る。移植はシステム生成の経路を変えないため、明示の開始呼び出しにする（設計レビューの論点 3 もこの形を推奨した）。開始のときに、時計（`Clock`）、deadLetters の参照、終了の旗を捕まえる。`FraktorSpec` が設定文字列を持たないことと合わせ、テストはアクターを作る前に `start_dead_letter_listener(settings)` を 1 回呼ぶ。

Info の届け方（M5）。testkit の `EventFilter::intercept` は event stream ではなく `event_sink` を読む。executor は失敗と unhandled について stream と sink の両方へ同じイベントを出す前例がある（`publish_child_failure`、`publish_unhandled`）。リスナーも同じく、Info を event stream と event sink の両方へ出す。あわせて testkit へ `EventFilter::info()` と `start` の前方一致を足す（`crates/fraktor-testkit/src/event_filter/`）。SuspensionSpec はこの 2 つを使って原典の形のまま書く。

状態機械の中身は原典の `DeadLetterListener` の字面どおりに写す。

- 状態は `Logging` と `Suspended { deadline }` の 2 つである。数え上げ `count: u32` を持つ。
- 抑制の判定は 2 段ある。振り分けの段（§2.2）で抑制対象は `SuppressedDeadLetter` になるので、リスナーは `SuppressedDeadLetter` を購読しないことで自然に外れる。もう 1 段は原典の `isWrappedSuppressed` である（L3）。原典が見るのは「`d.message` が `WrappedMessage` で、その中身が抑制対象」であり、`DeadLetter` の中身が直接抑制対象である場合は数える。移植で包みに当たるのは、payload がさらに `DeadLetter` 等の包み型である場合だけであり、その内側が抑制対象なら数えない。中身が直接抑制対象の `DeadLetter` は数える（生成の経路では作られないが、手で publish された場合の規則として単体テストで固定する）。終了中の抑制は**未移植**とする（論点 8）。設定の値だけを持ち、判定は書かない。原典の判定は「終了させる coordinated shutdown が進行中か」であり、`Executor::is_terminated` とは意味が違い、その形は二重借用にもなるためである。
- `Logging` で受けたとき：`count` を 1 増やす（`u32::MAX` に達していたら 1 へ戻す。原典は `Int.MaxValue` で 1 へ戻して Info を出す）。`limit` が `Count(n)` で `count == n` なら、一時停止の文面を付けてログし、`Suspended` へ移る。`suspend_duration` が `None`（無期限）の原典は `receiveWithMaxCountLogging` であり、上限でリスナー自身が止まる。移植では無期限のときは `Suspended` の期限を無限とし、再開しないことで同じ観測にする。`Unlimited` なら文面を付けずに常時ログする。
- `Suspended` で受けたとき：`count` を 1 増やす。期限を過ぎていれば、その 1 件を「of which `count - limit - 1` were not logged. The counter will be reset now」の文面でログし、`count` を 0 に戻して `Logging` へ戻る。期限の判定は厳密に「過ぎている」であり、ちょうど期限の時刻では再開しない（原典の `Deadline.isOverdue`）。
- ログは `Info::new(recipient.path, log_class, text)` をイベントストリームへ publish する。文面は原典の `logDeadLetter` の形を保つ。

```text
Message [<型名>] from <sender> to <recipient> was not delivered. [<count>] dead letters encountered<doneMsg>. If this is not an expected behavior then <recipient> may have terminated unexpectedly. This logging can be turned off or adjusted with configuration settings 'pekko.log-dead-letters' and 'pekko.log-dead-letters-during-shutdown'.
Message [<型名>] to <recipient> was dropped. <reason>. [<count>] dead letters encountered<doneMsg>. …
Message [<型名>] from <sender> to <recipient> was unhandled. [<count>] dead letters encountered<doneMsg>. …
```

送信者が実在しない場合（`deadLetters` 自身である場合）に `from` を省く規則は原典の `isReal` のとおりに写す。`<sender>` と `<recipient>` の表示は、どちらも参照のパス文字列（`ref.path().to_string()`）である（L5。`ActorRef` の `Debug` は `ActorRef(id, generation n)` であり、文面には使わない）。型名は `i32` になってよい（論点 4）。テストの期待値は文字列の直書きではなく `core::any::type_name::<i32>()` で組む。型名の取り出しは、`Message` の中身に非公開の trait を挟む形にする（L8）。

```rust
// crates/fraktor-actor/src/kernel/message/message_value.rs の中だけの trait
trait Payload: Any + Send + Sync { fn type_name(&self) -> &'static str; }
impl<T: Any + Send + Sync> Payload for T { fn type_name(&self) -> &'static str { core::any::type_name::<T>() } }
// Message は Arc<dyn Payload> を持ち、pub(crate) fn payload_type_name(&self) -> &'static str を足す
```

`dyn Any` からは型名を取れないのでこの形が必要であり、メッセージごとに欄は増えない。包まれたメッセージの文面は、原典の `WrappedMessage.unwrap` のとおりに、型名は再帰的に開封した内側の型のものとし、包まれていた場合は文面に「wrapped in [<包みの型名>]」を付ける（L3）。

一時停止の文面の期間の表示は、原典の `PrettyDuration.pretty` の形（`%.4g` と単位の略号、例：2 秒は `[2.000 s]`）にする（L1）。crate の中に Duration を整える既存の関数は無いので、no_std で書ける `%.4g` 相当の整形をリスナーのモジュールに小さく足す（`dead_letter_listener/pretty_duration.rs`）。#186 の移植が `kernel::pretty_duration` を main に入れた後は、そちらへ寄せる。

期限は `Clock` port から読む。リスナーは開始のときに、システムへ入っている時計への参照を捕まえる。時計が入っていないときは、executor の `now()` と同じ順で、シャードの境界の `monotonic_time` へ fallback する（再レビューの低 4）。テストは `ActorSystem::set_clock` で手動の時計を入れてからリスナーを開始し、期限を進める。

### 2.4 ConsistencySpec の本体側

この issue から外れたため、本体側の設計は書かない（§1）。材料は [#235](https://github.com/j5ik2o/fraktor-ng-rs/issues/235) の側で使う。

### 2.5 ActorSelection の最小実装

SupressionSpec の 2 ケースは後半で `system.actorSelection(deadActor.path) ! msg` を使う。fraktor-ng-rs には選択の仕組みが無いため、次の最小実装を足す。

```rust
pub struct ActorSelection { /* 境界の Arc<dyn ShardBoundary> と運び手の参照、path: ActorPath */ }
impl ActorSystem {
  pub fn actor_selection(&self, path: &ActorPath) -> ActorSelection;
}
impl ActorSelection {
  pub fn tell<M: Any + Send + Sync>(&self, message: M, sender: ActorRef);
}
```

形は、設計レビューの論点 6 の推奨どおりにする。`tell` は executor を同期に引かず、選択の封筒（内部型 `ActorSelectionMessage { path, message, sender }`）をシステムの境界へ送り、**解決は配送のときに executor が行う**（Pekko の `ActorSelectionMessage` と `deliverSelection` と同じ形）。作るときに、境界の `Arc<dyn ShardBoundary>` と運び手の参照を捕まえておき、`tell` は境界へ post するだけにする（再レビューの低 2）。`WeakActorSystem` を持つ形では、`upgrade().borrow_mut()` が要るため `receive` の中から呼ぶと二重借用になり、`ActorSelection` も `!Send` になるため、その形は採らない。

解決の規則は、この issue のケースが要る範囲に限る。

- 解決の対象は、セルの表と function ref の表の両方である（H2）。`deadLetters` は function ref なので、`/deadLetters` への選択は deadLetters の参照そのものに解決され、直接配送される。そのためケース 2 の後半は宛先まで含めて比べられる。
- パスが生きているアクターを指せば、そのアクターへ通常どおり配送する。
- パスが停止したアクター、または存在しないパスを指せば、宛先はそのパスを持つ「空の参照」として扱い、§2.2 の振り分け規則で publish する。このときの `recipient` はその空の参照である。ケース 1 の後半が message だけを見るのはこのためであり、移植もケース 1 は message だけを比べ、ケース 2 は宛先まで比べる。
- ワイルドカードを含む選択、アンカーからの相対選択、`Identify` への応答は作らない。ケースは使わない。これらはケースの「作らない」区分には当たらない（ケースは全件移す）。

配置は `crates/fraktor-actor/src/kernel/actor_selection.rs` とし、解決は executor の `deliver` の中で選択の封筒を判別して行う。台帳には `ActorSelectionSpec`（目安 28 件、unported）があり、この最小実装はその前段になる。

### 2.6 no_std

新しく足す型とリスナーは `core` と `alloc` だけで書ける。使うのは `alloc::{string::String, vec::Vec, sync::Arc, collections::BTreeMap}`、`core::{any::TypeId, time::Duration, sync::atomic::AtomicBool}` と、既存の依存の `spin::Mutex`（no_std 対応）と既存の `Clock` port である。`std` を必要とするのはテスト（手動の時計）だけであり、新しいモジュールは `std` を import しない。前例どおり、クレート全体は `#![no_std]` ではない。

## 3. 守る性質（不変条件）

### dead letters の生成と振り分け

| 番号 | 不変条件 |
|:---|:---|
| D1 | 停止したアクターへの通常のメッセージ 1 件は、`DeadLetter(message, sender, そのアクター)` をちょうど 1 件 publish する。二重にも、ゼロにもならない。ここでいう停止したアクターはセルのアクターに限る。停止した function ref は境界が閉じられ、`post` が黙って捨てるため、dead letter にならない（再レビューの低 7） |
| D2 | 抑制対象のメッセージ 1 件は、`SuppressedDeadLetter(message, sender, 処理した参照)` をちょうど 1 件 publish し、`DeadLetter` は publish しない。宛先は処理した参照であり、deadLetters の参照が処理した場合は `system.deadLetters`、空の参照が処理した場合はその空の参照になる（M1） |
| D3 | `deadLetters` へ送られた `DeadLetter` は、特別扱いに当たらない限り宛先を保ったまま publish される。中身が抑制対象の場合は、その `DeadLetter` は publish されず、中身について D2 が起きる |
| D4 | `AllDeadLetters` の購読者は、D1 と D2 の両方を送った順に受ける。`DeadLetter` の購読者は D2 を受けず、`SuppressedDeadLetter` の購読者は D1 を受けない |
| D5 | 生きているアクターへの配送は dead letter を作らない。棚上げとキューの残りの転送は、`DeadLetter` に包んで送るため、転送された 1 件につき dead letter をちょうど 1 件作り、宛先は元のアクターのままである |
| D6 | 抑制の判定は型（登録した `TypeId`）で決まる。ただし文字列の制御メッセージ `"PoisonPill"` と `"Kill"` だけは値で決まる（M2）。判定はそれ以外のメッセージの内容には依存しない |
| D7 | 停止したアクター宛ての `DeadLetter` は publish しない（捨てる）。`DeadLetter` を購読したまま止まったアクターがいても、dead letter のイベントについては循環せず、配送は終わる（H1）。範囲は dead letter のイベントに限る。リスナーの Info を購読したまま止まった購読者がいる場合の循環は D7 の範囲の外であり、§6 に記録する（再レビューの低 1） |

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

この issue から外れたため、性質は書かない（§1）。

## 4. PBT を足すか

足す。依存は足さない形（固定 seed の SplitMix64 で操作列を作り、単純なモデルと毎手比べる。前例の `frequency_sketch_property_test.rs` と #187 の設計の方針）にする。`proptest` を使うかは [#212](https://github.com/j5ik2o/fraktor-ng-rs/issues/212) の決定に従い、この issue では足さない。PBT は原典外であり、台帳の件数に含めない。

| 対象 | 性質 | 確かめる不変条件 |
|:---|:---|:---|
| リスナーの状態機械 | ランダムな dead letter の列（種別 3 種と抑制対象を混ぜる）と、ランダムな時計の進行を、手動の時計を持つリスナーへ流し、モデル（状態、`count`、ログの列）と毎手比べる。上限は 1、2、4、無制限から選び、一時停止の期間は 0、短い値、長い値から選ぶ | L1〜L6 |
| 振り分け | ランダムな宛先（生きているアクター、停止したアクター、`DeadLetter` を購読したまま停止したアクター、`deadLetters`）とメッセージの種別（通常、抑制対象、`DeadLetter` で包んだもの）の組み合わせを送り、3 種の購読者が受ける列をモデルと比べる。購読したまま停止したアクターへの dead letter が循環しないこと（D7）もこのモデルで確かめる | D1〜D7 |

## 5. Quint で書くかどうか

**DeadLetterListener の一時停止については Quint を書く。ほかの 2 つについては書かない。**

### 書く：DeadLetterListener の一時停止

理由は、この部分が時刻に依存する状態遷移であり、壊れ方が遷移の列の全体でしか出ないためである。数え上げをリセットし忘れる誤り、抑制対象を数えてしまう誤り、一時停止の間にログしてしまう誤り、再開の文面の件数を 1 つずらす誤りは、固定のケース（SuspensionSpec の 1 件）では一部の順序しか踏まない。FrequencySketch の前例（#177）でも、リセットの後の大きさの取り違えは PBT と原典のケースを通り、Quint だけが落とした。

置き場所は、設計レビューの M6 と論点 7 の推奨どおりに分ける。本体は CI が毎回検査する場所に置き、負の対照は設計の側に置く。

- 本体：`crates/fraktor-actor/specs/dead_letter_listener.qnt`。1 つのモジュールにし、`invariant` を不変条件の積として定義し、名前が `*Test` で終わる run を持たせる（run を持たない本体だけのファイルは CI の検査が「defines no run」で落とすためである）。CI の検査は `scripts/quint-check-specs.sh` であり、`docs/design/specs/` はその対象の外である。
- 負の対照：`docs/design/specs/dead_letter_listener_test.qnt`。bug のモジュールごとに、`.expect(not(<不変条件>))` を持つ run を書き、`quint test --main=<bug のモジュール> --match=<run 名> docs/design/specs/dead_letter_listener_test.qnt` で流す。`quint verify` は Apalache が要り、CI にも入れない方針（`docs/design/quint-in-ci.md`）なので使わない。seed は run の定義では固定できず、要る場合は CLI の `--seed` で渡して結果に記録する（再レビューの低 5）。第 2 段の完了報告に、流した結果を添える。

本体の中身は次のとおりである。

- 状態：`mode`（`Logging` か `Suspended`）、`count`、`now`（時刻。整数の刻み）、`deadline`、`logged`（ログした dead letter の列の長さと、最後の文面の種別）。
- 遷移：`receiveDeadLetter`、`receiveDropped`、`receiveUnhandled`、`receiveSuppressed`、`tick`（時刻を 1 刻み進める）。上限と一時停止の期間は、状態の変数にし、run ごとの `initWith(l, d)` と、`nondet` で選ぶ `init` で与える（再レビューの低 5。Quint の `const` で書くと CI の script が `QNT500 Uninitialized const` で落ちる。状態の変数の形では試作で exit 0、3 passing であった）。
- 不変条件：`countIsUnsuppressedSeen`（L1）、`noLogWhileSuspended`（L2。期限を過ぎていない `Suspended` では `logged` が増えない）、`suspendAtLimit`（L3）、`resumeResetsCount`（L4。再開の直後は `count == 0` で `mode == Logging`）、`deadlineIsStrict`（L5。`now == deadline` では再開しない）。`invariant` はこの 5 つの積である。
- run（インスタンス）：`limit4Test`（上限 4、期間 2。原典の設定）、`limit1Test`（上限 1。最初の 1 件で一時停止に入る）、`limit2shortTest`（上限 2、期間 0。時計を進めない間は再開せず、1 刻み進めた後の最初の 1 件で再開する。M7）、`unlimitedTest`（一時停止に入らない）。
- 負の対照：`bugNoReset`（再開で `count` を戻さない）、`bugCountsSuppressed`（抑制対象を数える）、`bugLogsWhileSuspended`（一時停止でもログする）、`bugResumeAtDeadline`（`now == deadline` で再開する）、`bugOffByOneNotLogged`（再開の文面の件数を 1 つずらす）。それぞれ狙った不変条件が落ちることを確かめる。

モデルにしていないもの：ログの文面の文字列そのもの（件数と種別だけを持つ。文面は §6 の単体テストと原典のケースで確かめる）、`u32::MAX` での数え上げの巻き戻し（到達しない）、終了中の抑制（ケースが使わない）。

### 書かない：振り分け

- **振り分け（書かない）。** 状態を持たない。1 件の入力に対する出力が型と宛先だけで決まる純粋な規則であり、§4 のモデル PBT が入力の組み合わせ全体を比べる。遷移の列でしか出ない壊れ方は無い。

なお、指示文にある「メールボックスの一時停止」は、この 3 本の原典には現れない。DeadLetterSuspensionSpec の suspension は dead letters ログの一時停止であり、アクターのメールボックスの停止（Pekko の `Mailbox` の suspended 状態）ではない。fraktor-ng-rs の executor にはメールボックスの停止という状態も無い。したがって、メールボックスの停止についての Quint は、この issue では書かない。

## 6. 壊しうる入力

| 対象 | 入力 | 何が起きうるか | 確かめるもの |
|:---|:---|:---|:---|
| 振り分け | `deadLetters` へ `DeadLetter` を送る | 宛先を deadLetters へ付け替えて publish すると、購読者とログの宛先が原典と違うものになる | ケース 1 と 2 の選択の段、§4 の振り分け PBT |
| 振り分け | `DeadLetter` を購読したままアクターを止める | dead letter がそのアクター宛てに作られ続け、配送が終わらない（H1） | D7、§4 の振り分け PBT |
| 振り分け | `DeadLetter` が抑制対象を包む | 中身を見ずに publish すると抑制が漏れる。逆に、中身が直接抑制対象の `DeadLetter` まで数えないことにすると、原典の `isWrappedSuppressed` の読みと違う（L3） | リスナーの単体テスト（L1） |
| 振り分け | 抑制対象を生きているアクターへ送る | 通常どおり配送される。dead letter にならない限り抑制は効かない | 振り分け PBT の宛先の部品に含める |
| 振り分け | 送信者が `deadLetters` 自身である dead letter | 文面の `from` を出すと原典と違う文面になる | リスナーの単体テスト（`isReal` の規則） |
| 振り分け | 空文字のメッセージ、極端に長い文字列のメッセージ | 型名と宛先のパスの表示だけが変わり、振り分けは変わらない | 振り分け PBT の部品に含める |
| リスナー | 上限が 1 | 最初の 1 件で一時停止に入り、文面が付く | Quint の `limit1`、単体テスト |
| リスナー | 上限が 0（`Off`） | リスナーが購読せず、数え上げもログも無い | 単体テスト（L7） |
| リスナー | 一時停止の期間が 0 | 手動の時計を進めなければ `now == deadline` であり、L5 により再開しない。数え上げだけが進み、`Suspended` のままである。1 刻み進めた後の最初の 1 件が再開になり、件数は 0 になる（M7） | Quint の `limit2shortTest`、PBT の期間の部品 |
| リスナー | 一時停止の期間が無期限（`None`） | 再開しない。上限に達した後は二度とログしない | 単体テスト |
| リスナー | 期限ちょうどの時刻に届く dead letter | 再開しない（L5）。1 刻み前と 1 刻み後で結果が変わる境界である | Quint の `deadlineIsStrict`、PBT は時刻の進行に期限の前後を含める |
| リスナー | 一時停止の間に届く大量の dead letter | 数え上げだけが進み、再開の文面の件数が大きくなる。件数の計算 `count - limit - 1` が負にならないのは、再開が上限到達の後にだけ起きるためである | PBT、Quint |
| リスナー | 上限が `Unlimited` で、Info の購読者が購読したまま止まっている | リスナーが出した Info が止まった購読者宛ての `DeadLetter(Info)` になり、リスナーがそれを数えてまた Info を出す循環が残る（再レビューの低 1。試作のモデルでは 100000 回で打ち切っても終わらず、`Count(4)` なら 5 回で終わる）。Pekko でも同じ循環はあるが、既定で動く unsubscriber が抜け道になる。fraktor の unsubscriber は `start_unsubscriber` を呼んだときだけ動く。この issue では直さず、D7 の範囲の外としてここに記録する | 範囲の外（この文書に明記） |
| リスナー | 数え上げが `u32::MAX` に達する | 原典は `Int.MaxValue` で 1 へ戻す。移植は飽和させ、再開の文面の件数（`count - limit - 1`）が下溢れしないようにした（CodeRabbit の指摘で変更）。この境界を実行で踏む検査は無い（到達に 2^32 件が要る） | 範囲の外（この文書に明記） |
| リスナー | 時刻を返さない特注の境界で、時計も入れていない | 一時停止が明けない。しかもリスナーは止まらないので、watcher へ `Terminated` も届かない。Pekko のどちらの形（再開する形、止まる形）とも違う観測である。既定の `ChannelBoundary` は時刻を返し、`ActorSystem::set_clock` が明示の上書きである。再レビュー（低 4）と再々レビュー（低 3）で範囲の外と決め、`DeadLetterSettings` の rustdoc にも書いた | 範囲の外（この文書に明記） |
| ActorSelection | 存在しないパスへの選択 | 空の参照として振り分けられる。パスが長い、空の要素を含む場合も同じである | ケース 1 と 2 の選択の段 |
| ActorSelection | ワイルドカードを含むパス | この issue では作らない。ワイルドカードは通常の要素として扱われ、一致するアクターは無い | 範囲の外（この文書に明記） |

「ワイルドカードの連続」に当たる入力は、ActorSelection のワイルドカードを作らないため、この issue には無い。

## 7. 影響

- **公開 API**：追加が中心である。`fraktor_actor::kernel::actor::{DeadLetter, SuppressedDeadLetter, Dropped, AllDeadLetters, AnyDeadLetter, DeadLetterSuppression}`、`kernel::actor_system::{DeadLetterSettings, DeadLetterLogLimit}`、`ActorSystem::{actor_selection, register_dead_letter_suppression, start_dead_letter_listener}`、`kernel::actor_selection::ActorSelection`。`Message` へ `pub(crate)` の `payload_type_name` を足し、中身に非公開の `Payload` trait を挟む（L8）。testkit へ `EventFilter::info()` と `start` の前方一致を足す（M5）。
- **振る舞いの変更**：`deadLetters` ハンドルが、受け取ったメッセージを捨てるだけではなくイベントストリームへ publish するようになる。既存のテストで `dead_letters()` を送信者として使うものは影響を受けない（送信者として使う分には publish は起きない）。ただし、停止したアクターの棚上げとキューの残りを `deadLetters` へ転送する既存の経路は、これまで観測できなかった dead letter を publish するようになる。転送は `DeadLetter` に包む形へ変わる（M3）。購読者がいないシステムでは publish は何も起こさないが、イベントストリームの購読を持つ既存のテスト（`event_stream_spec.rs` 等）が dead letter を受ける可能性がある。第 2 段では `cargo test --workspace` でこの影響を確かめ、落ちたテストは実装でごまかさず報告する。
- **台帳**：第 2 段で DeadLetterSuspensionSpec と DeadLetterSupressionSpec の 2 ノードを green（1/1/1、2/2/2、`verified_against: v2.0.0`）にする。ConsistencySpec のノードは `unported` のまま変えない（#235 へ分けた）。`docs/baseline/v2.0.0-undocumented-nodes.tsv` を作り直す。ゴール文書 `docs/goals/issue-191-specs.md` は第 2 段で作る。
- **依存**：足さない（`spin` は既存の依存である）。
- **計算量**：dead letter の publish は購読者の数に比例する。リスナーの処理は 1 件あたり定数である。

## 8. レビューで見てほしい点（設計レビューで決まった形）

初版の 8 件は、PR #230 の独立レビュー（2026-11-26、判定は要修正）が推奨を出し、元帥が ConsistencySpec について決めた。現在の設計はその形になっている。再レビューでは、この形で実装に入ってよいかを見てほしい。

1. **`AllDeadLetters`（§2.1）。** チャンネルには印の型を残し、trait は置かず、見る側の列挙型 `AnyDeadLetter` にする（レビューの論点 1 の推奨）。
2. **抑制の印（§2.2）。** `TypeId` の登録制にする。kernel が `Terminated` を既定で登録し、文字列の制御メッセージは値で判定し、登録表は共有の欄に置く（レビューの論点 2 の推奨と条件）。
3. **リスナーの開始（§2.3）。** 明示の開始 `start_dead_letter_listener(settings)` の 1 回の呼び出しにまとめる。開始のときに時計と deadLetters の参照と終了の旗を捕まえる。`declare_subclass` の 4 件は、リスナーの有無と関係なくシステムの生成時に宣言する（レビューの論点 3 の推奨）。
4. **ログの型名（§2.3）。** `i32` でよい。期待値は `core::any::type_name::<i32>()` で組み、取り出しは `Message` の中の非公開の `Payload` trait で行う（レビューの論点 4 の推奨）。
5. **ConsistencySpec（§1）。** 元帥の決定（2026-11-26）でこの issue から外し、[#235](https://github.com/j5ik2o/fraktor-ng-rs/issues/235) へ分けた。未移植のままであり、除外ではない。
6. **ActorSelection（§2.5）。** この issue に含める。`tell` は選択の封筒を送り、解決は配送のときに行う。解決の対象はセルと function ref の両方である（レビューの論点 6 の推奨）。
7. **Quint（§5）。** リスナーの一時停止に限る。本体は `crates/fraktor-actor/specs/dead_letter_listener.qnt` に 1 モジュールで置き、負の対照は `docs/design/specs/` に置く（レビューの論点 7 の推奨）。
8. **作らないもの（§2.3、§2.5）。** ActorSelection のワイルドカードと `Identify` は作らない。リスナーの終了中の抑制は未移植とし、設定の値だけを持つ（レビューの論点 8 の推奨）。

## 9. 設計レビュー（PR #230、HEAD c5a102e）への回答

判定は「要修正」（高 3、中 8、低 8）であった。採らなかった指摘は無い。

| 指摘 | 対応 |
|:---|:---|
| H1（停止したアクター宛ての `DeadLetter` を捨てる規則が無い） | 直した。§2.2 の経路 1 に捨てる規則を足し、D7 を新設し、振り分け PBT の宛先に「購読したまま停止したアクター」を入れた |
| H2（ケース 2 の後半は宛先まで比べる） | 直した。§2.5 の解決の対象に function ref（deadLetters を含む）を入れ、§1 にケースごとの比べ方の違いを書いた |
| H3（`Rc<RefCell>` と二重借用） | 直した。状態は `DeadLettersShared`（`spin::Mutex` と `AtomicBool` の共有の欄）に置き、リスナーの状態は `Arc<spin::Mutex<_>>` にした。終了中の抑制は未移植にした |
| M1（`SuppressedDeadLetter` の宛先） | 直した。D2 と §2.2 を「処理した参照」に直した |
| M2（kernel 自身の抑制対象） | 直した。`Terminated` の既定の登録と、文字列の制御メッセージの値での判定を §2.2 に書き、D6 を直した |
| M3（棚上げとキューの残りの宛先） | 直した。転送を `DeadLetter` に包む形に変え、経路 2 の規則を「宛先を保ったまま publish する」に直した |
| M4（`AllDeadLetters` の同名の trait） | 直した。trait を置かず、印の型と見る側の列挙型 `AnyDeadLetter` にした（論点 1 の推奨） |
| M5（`EventFilter` に Info と前方一致が無い） | 直した。testkit への追加と、Info を event sink へも出す経路を §2.3 と §7 に書いた |
| M6（Quint の置き場所） | 直した。本体を `crates/fraktor-actor/specs/dead_letter_listener.qnt` の 1 モジュールにし、`invariant` と `*Test` の run を持たせ、負の対照と流すコマンドを §5 に書いた |
| M7（期間 0 の行と L5 の食い違い） | 直した。§6 を「1 刻み以上進んだ後の最初の 1 件が再開になる」に直し、Quint の run も合わせた |
| M8（ConsistencySpec の形の変更） | 元帥の決定（2026-11-26）で、この issue から外して #235 へ分けた（§1）。 |
| L1（期間の表示） | 直した。文面を `[2.000 s]` とし、`%.4g` 相当の整形を足すことを §2.3 に書いた |
| L2（一時停止の期間の既定値） | 直した。既定を 5 分にした |
| L3（`isWrappedSuppressed` の読み） | 直した。§2.3 の判定を原典の読みに直し、文面の開封と「wrapped in [..]」を書いた |
| L4（上限の端の値） | 直した。`Count` の中身を `NonZeroU32` にし、0 と最大値の写像を §2.3 に書いた |
| L5（文面と比較の形） | 直した。送信者と宛先の表示はパス文字列とし、`DeadLetter` の比べ方（downcast して要素ごとに比べる）と `Dropped` の送信者なしの約束を §2.1 と §2.3 に書いた |
| L6（`ActorSelection::tell` の二重借用） | 直した。選択の封筒を送り、配送のときに解決する形にした |
| L7（設定と開始の分離） | 直した。`start_dead_letter_listener(settings)` の 1 回の呼び出しにまとめた |
| L8（`payload_type_name` の作り方） | 直した。`Message` の中の非公開の `Payload` trait で取り出す形を §2.3 に書いた |
| 論点 1〜8 への推奨 | すべて推奨どおりにした。§8 に決まった形をまとめた |

## 10. 再レビュー（PR #230、HEAD 02ac72b）への回答

判定は「要修正（ConsistencySpec の 1 点だけ）」であった。DeadLetterSuspensionSpec と DeadLetterSupressionSpec の 2 ノードは実装に入ってよいとされ、この 2 ノードの第 2 段を進める。ConsistencySpec は、元帥の決定でこの issue から外れた。

| 指摘 | 対応 |
|:---|:---|
| 高 1（ConsistencySpec の形は条件を満たさない） | 元帥の決定（2026-11-26）で、この issue から外して [#235](https://github.com/j5ik2o/fraktor-ng-rs/issues/235) へ分け、未移植のまま後回しにした（除外ではない）。設計からも外した（§1） |
| 低 1（`Unlimited` かつ Info の購読者が停止していると循環が残る） | D7 の範囲を dead letter のイベントに限ると書き、§6 に循環の行を足して範囲の外として記録した |
| 低 2（`ActorSelection` が弱い参照を持つ） | 作るときに境界の `Arc<dyn ShardBoundary>` と運び手の参照を捕まえる形に直した |
| 低 3（`HashSet` が no_std と合わない） | 登録表を `BTreeSet<TypeId>` に直した |
| 低 4（リスナーが捕まえるもの） | event sink を捕まえるものに足し、時計の fallback（境界の `monotonic_time`）を書いた |
| 低 5（Quint の書き方） | 上限と期間を状態の変数と `initWith` で与える形に直し、負の対照の流し方を `quint test --main=…` に直した |
| 低 6（Consistency の追加の実行と割り当て） | ConsistencySpec がこの issue から外れたため、対象が無くなった |
| 低 7（停止した function ref） | D1 の対象をセルのアクターに限ると書いた |

## 11. 実装レビュー（PR #230 のボットレビュー）への対応

実装後に Cursor と Devin が付けた指摘 11 件について、一次情報で確かめてから対応した。直したものには、落ちることを確かめた回帰テストを足している。

| 指摘 | 判定 | 対応 |
|:---|:---|:---|
| Cursor：リスナーの状態が `!Send`（設計の旧案） | 直した | 設計レビュー H3 で直し済みであり、実装は `Arc<spin::Mutex<DeadLetterListener>>` である |
| Cursor：期間 0 が L5 と矛盾する | 直した | 設計レビューで直し済みであり、再開は期限より後（`>`）だけであり、期間 0 では同じ時刻の到着は再開しない。Quint の `limit2shortTest` が固定している |
| Devin BUG：配送できない `SuppressedDeadLetter` が巡り続ける | 直した | 振り分けの入口 `route_undeliverable` で、`SuppressedDeadLetter` 自体は `DeadLetter` に包んで 1 回だけ出し直すようにした。その配送が失敗すれば D7 で落ちるので、巡らない。購読したまま止まったアクターへの配送が失敗する回帰テストを足した（修正前は `run_until_idle` が終わらない） |
| Devin BUG：リスナーの開始が unsubscriber 有効時に panic する | 直した | 購読を `Executor` から `ActorSystem::start_dead_letter_listener` へ移し、executor の借用を解いた後に購読するようにした。unsubscriber を先に始める回帰テストを足した（修正前は panic） |
| Devin ANALYSIS：上限 `Off` でも購読している（L7 と違う） | 直した | `Off` のときは購読せず、ハンドルだけを作って返すようにした（上の移設と同時） |
| Devin BUG：選択で解決したメッセージが後から来た直接の送信に追い越される | 直した | セル宛ての解決は、その配送の段の中で `deliver` するようにした。function ref 宛ては従来どおり同期に実行される。選択の直後に `PoisonPill` を送る回帰テストを足した（修正前は選択のメッセージが dead letter になった） |
| Devin BUG：dead letters ハンドルが自分自身を強く捕まえて漏れる | 直した | ハンドラが持つスロットを境界への弱い参照と識別子とパスに変え、呼ぶたびにハンドルを作り直すようにした。executor を落とすと境界が解放される回帰テストを足した（修正前は解放されなかった）。なお、システムが止まった後の送信を捨てているのは弱い参照ではなく、ハンドルが `function_refs` に入っていて停止で閉じる仕組みである（再々レビューの低 5）。terminate の後の tell が何も発行しない回帰テストも足した |
| Devin BUG：時計の無い特注の境界ではリスナーが再開しない | 採らない | 既定の `ChannelBoundary` は `monotonic_time` を返し、`ActorSystem::set_clock` が明示の上書きである。境界が時刻を返さないのは、その境界を使う側が時刻の無い設計を選んだということであり、再レビュー低 4 で決めた fallback（`Duration::ZERO`）のとおりにする。開始時に落とす変更は、この issue の範囲を超える |
| Devin BUG：期間が無いとき、リスナーが止まらず watcher へ `Terminated` が届かない | 直した | Pekko は上限で `context.stop(self)` を呼ぶ。状態機械の停止印を executor が配送の境界で読み、保留中の停止として実行するようにした。あわせて、dead letter の振り分けで終わる配送でも境界の仕事を回すようにした。watcher へ `Terminated` が届く回帰テストを足した |
| Devin ANALYSIS：PBT の手順に選択宛てが無く、ゴール文書の表と違う | 直した | PBT の手順に、止まったアクターへの選択、生きているアクターへの選択、生きているアクターへの抑制対象の 3 つを足した（設計 §5 が部品として挙げていたもの） |
| Devin ANALYSIS：Quint の本体に再開の文面の件数の ghost が無い | 直した | 本体の仕様に `lastNotLogged` の ghost と不変条件 `resumeNotLoggedIsExact` を足し、再開する 2 つの run で値 1 を固定した。負の対照 `bugOffByOneNotLogged` と対になる |

### CodeRabbit の指摘（7 件）への対応

| 指摘 | 判定 | 対応 |
|:---|:---|:---|
| 停止中の件数が `u32` を回ると、再開の文面の件数が下溢れする | 直した | 件数の増加を `saturating_add` に変えた。飽和した件数は停止が始まった上限以上に留まるので、`count - limit - 1` は下溢れしない。Quint のモデル（無制限の整数）では式そのものが固定されている |
| 時計の無い特注の境界ではリスナーが再開しない | 採らない | Devin の同じ指摘への回答と同じである。既定の境界は `monotonic_time` を返し、`set_clock` が明示の上書きであり、再レビュー低 4 で決めた fallback のとおりにする |
| リスナーは開始時に捕まえた event sink へ出し続ける | 直した | executor の event sink を共有のセル（`Arc<spin::Mutex<Option<…>>>`）に変え、リスナーを含むすべての出版経路が、その時点で入っている sink を読むようにした。開始後に sink を入れる回帰テストを足した（修正前は届かなかった） |
| 選択の配送の後で境界の仕事が回らない | 直した | `deliver` の選択の分岐でも `finish_boundary` を呼ぶようにした。解決できない選択が dead letter を出し、リスナーが上限で止まる回帰テストを足した（修正前は `Terminated` が届かなかった） |
| `Unlimited` かつ Info の購読者が停止していると循環が残る | 採らない | §6 に範囲の外として記録済みの循環であり、元帥が確認した設計の範囲の外である。対象の購読者が購読したまま停止し、unsubscriber が動いていないときにだけ起きる |
| 線引きの印の引き継ぎが、途中の別の理由で切れる | 直した | 前の行が一括の決定で `Apache-2.0` になっていた「自分で書いた」行は、理由が変わっても新規ファイルとして扱い続けるようにした。単体テストを足した |
| 線引きの履歴が `--out-tsv` と結び付いている | 直した | 履歴は決まった場所の一覧（`docs/design/issue-206-file-origin.tsv`）から読むようにした。出力先を変えても既存の行の扱いは変わらない |

### 再々レビュー（実装中に変わった kernel の作り）への対応

実装後に変わった kernel の作りへの再レビュー（固定 `790a1e0`）の指摘へ対応した。sink のセル（知らせの落ちも二重も無い）、選択の後の順序、停止後の送信の安全性は、問題無しとされた。

| 指摘 | 判定 | 対応 |
|:---|:---|:---|
| 中 1：リスナーの停止の印を `finish_boundary` の入口でしか読まない | 直した | handler が停止へ移った瞬間に、シャードの境界へ内部の封筒（`StopDeadLetterListener`）を post し、その配送で executor が停止を実行する形に変えた。入口の検査は消えたので、配送 1 件ごとのロックも無くなった。deadLetters への直接の tell と、stop の中の stash の流し出しの 2 経路の回帰テストを足した（どちらも修正前は `Terminated` が届かなかった） |
| 中 2：dead letter の発行 1 件ごとにロックと割り当てが増えた | 直した | `slot` と bus の欄を `spin::Once` に変えた（どちらもシステムを作るときに 1 回しか書かない）。`slot` には `Arc<ActorPath>` を持たせ、1 件ごとの深い複製と `Arc::new` を消した。登録表は、実行時に型を登録する公開の経路（`register_dead_letter_suppression`）があるため凍らせず、ロックのままにした。残る 1 件ごとの費用は、境界への `Weak::upgrade`（循環を断つための費用）と登録表のロックと、もとからある bus のロックである。測り直した結果は PR 本文に書いた |
| 低 1：複数のスレッドから同時に発行すると Info の行の順序が入れ替わる | #234 で決める | 数え上げはロックの中で行い、発行はロックの外で行うためである。ロックの中で発行するか、順序を約束しないと文書に書くかは、複数スレッドの実行器（#234）の段 1 で決める |
| 低 2：新しく足したロックが #236 の在庫表に載っていない | PR 本文に申し送り | この PR では #236 のファイルを触らず、PR 本文に「#236 への申し送り」として書いた |
| 低 3：時計の無い境界で一時停止が明けないことが利用者に見えない | 書いた | §6 に 1 行足し、`DeadLetterSettings` の rustdoc にも書いた |
| 低 4：`Unlimited` の循環が `run_until_idle` を返らなくする | 書いた | `DeadLetterLogLimit::Unlimited` の rustdoc に、unsubscriber を始めていないと循環すると書いた。unsubscriber を既定で始めるかは、この issue の範囲の外の判断として残す |
| 低 5：止まった後の送信が安全な理由の書き方が実際とずれている | 直した | 上の Devin の行に、送信を捨てるのは `function_refs` を閉じる仕組みであることを足し、terminate の後の tell が何も発行しない回帰テストを足した |
