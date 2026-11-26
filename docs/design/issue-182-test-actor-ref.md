# 設計：#182 TestActorRefSpec（testkit に TestActorRef の能力を用意する）

[Issue #182](https://github.com/j5ik2o/fraktor-ng-rs/issues/182) の第1段（設計だけ）の記録である。実装は、別の担当がこの設計をレビューして通ってから行う。この文書は実装を含まない。第2段では、テストを Claude Opus の担当が先に red で書き、実装の担当はそのテストを書き換えずに通す実装だけを書く。

対象は Pekko v2.0.0 の `testkit/src/test/scala/org/apache/pekko/testkit/TestActorRefSpec.scala` である。原典の本体は `testkit/src/main/scala/org/apache/pekko/testkit/TestActorRef.scala`（`LocalActorRef` を継承し、dispatcher を `CallingThreadDispatcher` へ差し替え、receiveTimeout を None にし、`underlyingActor` と `receive` の直接呼び出しと `watch`／`unwatch` を足したもの）と、`testkit/src/main/scala/org/apache/pekko/testkit/CallingThreadDispatcher.scala` である。

台帳の対象ノードは `testkit/src/test/scala/org/apache/pekko/testkit/TestActorRefSpec.scala`（`testkit:TestActorRefSpec`、`test_cases_approx: 23`、`status: unported`、`needs.testkit: ['extends-pekko-spec', 'pekko-spec', 'test-actor', 'default-timeout', 'expect-msg', 'test-probe', 'event-filter', 'test-actor-ref', 'calling-thread-dispatcher']`）の1件だけである。第2段でこのノードを green にする。`needs` のうち `test-actor-ref` と `calling-thread-dispatcher` が未充足であり、本件がその2つを用意する。

前提として読み直したものは次のとおりである。移植の規約（`.takt/facets/knowledge/fraktor-porting-rules.md`）、速さより Pekko の意味論を優先する決定（`docs/decisions/pekko-semantics-first.md`）、Scala・Java の書き方と Rust の対応表（`docs/decisions/scala-java-to-rust-correspondence.md`）、[Issue #181](https://github.com/j5ik2o/fraktor-ng-rs/issues/181) の設計と実装（`docs/design/issue-181-testfsmref.md`、[PR #275](https://github.com/j5ik2o/fraktor-ng-rs/pull/275) で main に入った `TestFsmRef` と kernel の `test_support`）、[Issue #234](https://github.com/j5ik2o/fraktor-ng-rs/issues/234) の設計（`docs/design/issue-234-multithread-executor.md`。CallingThreadDispatcher は段7b、[Issue #259](https://github.com/j5ik2o/fraktor-ng-rs/issues/259)）、前例の設計の形（[PR #205](https://github.com/j5ik2o/fraktor-ng-rs/pull/205) の `docs/design/issue-177-frequency-sketch.md`）。

## 1. 原典のケースを数え直した結果と、移す先の対応表

原典を `" in {"` の数で数え直した。`ignore` と `pending` は無い。

- 「A TestActorRef should be an ActorRef, hence it」の下に7件
- 「A TestActorRef」の下に8件
- 「A TestActorRef Companion Object」の下に8件
- 合計23件。台帳の目安23件と一致する

移す先は `crates/fraktor-testkit/tests/test_actor_ref_spec.rs`（ノード1件＝ファイル1件、冒頭に出典のパスを残す）である。内部 API（番兵の処理済み番号、underlying のスナップショット）を直接呼ぶ確認は、#181 の前例と同じく単体テスト側に置くものが出れば、台帳の `rust.cases_outside_test_file` に書く。

| 番号 | 原典のケース | Rust のテスト名 | 区分 |
|:---:|:---|:---|:---|
| 1 | support nested Actor creation（used with TestActorRef） | `supports_nested_actor_creation_with_test_actor_ref` | 移す（注1） |
| 2 | support nested Actor creation（used with ActorRef） | `supports_nested_actor_creation_with_actor_ref` | 移す（注1） |
| 3 | support reply via sender() | `supports_reply_via_sender` | 移す。本体の返信の数えは今の実行器で移す。スレッドの同一性の確認（`assertThread`）のうち「送ったその場で処理される」部分は #234 の段7b の後（注2） |
| 4 | stop when sent a poison pill | `stops_when_sent_a_poison_pill` | 移す（注3） |
| 5 | restart when Kill:ed | `restarts_when_killed` | 移す。スレッドの同一性の確認はケース3と同じく段7b の後（注2） |
| 6 | support futures | `supports_futures` | 移す。返信の到着は今の ask で移す。`f.isCompleted` が送信の直後に真であること（CallingThreadDispatcher の同期性そのもの）は #234 の段7b の後（注2） |
| 7 | support receive timeout | `supports_receive_timeout` | ReceiveTimeout 本体の後（注4）。数から外さず、red のまま残す |
| 8 | allow access to internals | `allows_access_to_internals` | 移す。本件の中心（§3.2） |
| 9 | set receiveTimeout to None | `sets_receive_timeout_to_none` | 移す（注5） |
| 10 | set CallingThreadDispatcher | `sets_calling_thread_dispatcher` | 移す。dispatcher の種類の記録として移し、送った側のスレッドで処理する実行そのものは #234 の段7b の後（注2、注6） |
| 11 | allow override of dispatcher | `allows_override_of_dispatcher` | 移す（注6） |
| 12 | proxy receive for the underlying actor without sender() | `proxies_receive_without_sender` | 移す。待ちを入れた形で移し、送ったその場で処理される形への書き戻しは #234 の段7b の後（注1、注7） |
| 13 | proxy receive for the underlying actor with sender() | `proxies_receive_with_sender` | 移す（注1、注7） |
| 14 | not throw an exception when parent is passed in the apply | `does_not_throw_when_parent_is_passed_in_the_apply` | 移す（注8） |
| 15 | not throw an exception when child is created through childActorOf | `does_not_throw_when_child_is_created_through_child_actor_of` | 移す（注8） |
| 16 | allow creation of a TestActorRef with a default supervisor | `allows_creation_with_a_default_supervisor` | 移す（注9） |
| 17 | allow creation of a TestActorRef with a default supervisor and specified name | `allows_creation_with_a_default_supervisor_and_a_name` | 移す（注9） |
| 18 | allow creation of a TestActorRef with a specified supervisor | `allows_creation_with_a_specified_supervisor` | 移す（注9） |
| 19 | allow creation of a TestActorRef with a specified supervisor and specified name | `allows_creation_with_a_specified_supervisor_and_a_name` | 移す（注9） |
| 20 | allow creation of a TestActorRef with a default supervisor with Props | `allows_creation_with_a_default_supervisor_from_props` | 移す（注10） |
| 21 | allow creation of a TestActorRef with a default supervisor and specified name with Props | `allows_creation_with_a_default_supervisor_and_a_name_from_props` | 移す（注10） |
| 22 | allow creation of a TestActorRef with a specified supervisor with Props | `allows_creation_with_a_specified_supervisor_from_props` | 移す（注10） |
| 23 | allow creation of a TestActorRef with a specified supervisor and specified name with Props | `allows_creation_with_a_specified_supervisor_and_a_name_from_props` | 移す（注10） |

注1（同期性と待ち方）：原典の `TestActorRef` は CallingThreadDispatcher で動くので、送信も入れ子の生成もその場で処理され、次の行で結果を読める。今の fraktor-ng-rs の実行器では送信はキューに入り、待ちの運転手が処理する。移植のテストは、#181 の前例と同じく、送信の後で番号付きの番兵（何もしない制御メッセージ）を送り、ハンドルが持つ「処理を終えた制御メッセージの通し番号」がその番号に達するまで `FraktorSpec::await_cond` で待ってから確かめる形にする（§3.1）。アサーションの内容（返信の中身、入れ子の参照が親と別物であること、停止したこと）は弱めない。送信したその場で処理されるという強い同期性そのものを確かめる形への書き戻しは、段7b（#259）の後とする。これはケースの除外ではなく、待ち方の違いである。

注2（段7b が前提になる部分）：ケース3と5の `assertThread`（処理が別のスレッドで起きていないこと）、ケース6の `isCompleted` の即時性、ケース10の dispatcher の実行そのものは、testkit の CallingThreadDispatcher（#234 の段7b、#259）が前提である。設計の指示どおり、前提になる部分は「#234 の段7b の後」として分け、数から外さずに red のまま残す形にする（ExtensionSpec のケース5 の前例）。具体的には、ケース本体のテストは今の段で green にし、同期性だけを見るテストを別名で red のまま置く（例：`supports_reply_via_sender_on_the_calling_thread`、`supports_futures_completed_immediately`、`sets_calling_thread_dispatcher_and_runs_on_the_calling_thread`）。台帳を green にする判定でこの3件の扱いをどうするか（段7b の完了までノードを green にしないのか、red のまま残す形を rollup が許すのか）は、設計レビューで決めてほしい点に挙げる（§8）。スレッドの同一性そのものは Rust でも `std::thread::ThreadId` をアクターが記録する形で写せるので、JVM 固有として外す提案はしない。

注3（ケース4の EventFilter）：原典は `EventFilter[ActorKilledException]().intercept` で囲む。移植の `PoisonPill` は既存の移植の前例どおり文字列のペイロード `"PoisonPill"` で送る（`crates/fraktor-actor/src/kernel/executor/executor_value.rs` の `POISON_PILL`）。本体の確認は、watch しているアクターが `Terminated` を包んで testActor へ転送することと、`isTerminated` が真になることであり、これは移せる。EventFilter が例外の型の階層で絞る部分は、Rust には例外の型階層が無いので、そのままでは写せない。既存の `EventFilter`（`crates/fraktor-testkit/src/event_filter.rs`）がイベントの型で絞る形で、失敗のイベントを観測できるかを第2段の red の前に確かめ、できない場合は §7 の提案2として扱う（この文書では決めない）。

注4（ケース7の前提）：原典の `ReceiveTimeoutActor` は構築時に `context.setReceiveTimeout(1.second)` を呼び、`ReceiveTimeout` を受けて相手へ知らせて止まる。本体側の前提が fraktor-ng-rs に無い。2026-11-27 の main（`c0f94f0`）で `crates/` の全体を検索し、`ReceiveTimeout` と `set_receive_timeout` と `receive_timeout` のいずれも本体には無いことを確かめた（台帳の `actor-tests:ReceiveTimeoutSpec` も `status: unported`）。したがってケース7は、アクター側の receive timeout の本体（設定の口、期限の管理、発火）が入った後とする。本件の `TestActorRef` は receiveTimeout を None にする側（ケース9）だけを担い、timeout を張る側は作らない。ケース7は数から外さず、red のまま残す。

注5（ケース9の写し方）：原典は `underlyingActor.context.receiveTimeout` が `Duration.Undefined`（None と同じもの）であることを見る。本体に receive timeout の口自体が無い（注4）ので、移植では `TestActorRef` が生成時に記録する「このアクターの receive timeout は None に設定された」という値を、underlying の面（§3.2）から読んで確かめる形にする。これは原典の確認先（アクターの context）がまだ無いための写し方の違いであり、意味（TestActorRef で作ったアクターには receive timeout が付かない）は変えない。本体に口が入った後は、context 側の値を見る形へ戻す。

注6（ケース10と11の写し方と HOCON）：原典は dispatcher のクラス（`getClass`）を見る。Rust にはクラスが無いので、対応表の「Scala の実行時クラス階層で分類すること」の前例と同じく、分類は型で表す。移植では、解決した dispatcher の種類が `DispatcherKind::CallingThread`（ケース10）か `DispatcherKind::Default`（ケース11）であることを見る。`DispatcherKind` は `crates/fraktor-actor/src/kernel/dispatch/dispatcher_kind.rs` にすでにある（ただしその文書が書くとおり、dispatcher の実行そのものは #234 の仕事であり、今は設定の検証で種類を見るだけである）。ケース11の `Props[WorkerActor]().withDispatcher("disp1")` と、Spec の冒頭の `PekkoSpec("disp1.type=Dispatcher")` という HOCON の設定は、前例（#154 の D1、G0b の決定 E、#192 の `DispatchSettings`）どおり、文字列の設定体系は作らず、型付きの入力にする。具体的には、`disp1` という名前で種類が `Default` の dispatcher 定義を `DispatchSettings` に登録した `ActorSystemSetup` でシステムを作る。原典の規則（Props に dispatcher の指定があれば TestActorRef は差し替えない。指定が無ければ CallingThreadDispatcher にする）は、そのまま写す（§3.3 の I2）。

注7（ケース12と13の `receive` の例外）：原典の `receive` は、アクターの `receive` を直接呼ぶので、そこで投げられた例外が呼び出し側へ届く。本件のケース12と13のアクター（`WorkerActor`）は例外を投げないので、この2件では問題にならない。例外の伝播そのものはケース14と15の側の話であり、そちらは「呼び出し側へ投げない」ことが原典の確認内容である（注8）。直接呼び出しで例外を利用者が受け取る形は、移植では失敗が監督で処理され、呼び出し側は番兵の待ちの時間切れか失敗のイベントで気づく形になる。この違いは §5 に記録する。

注8（ケース14と15）：原典の確認は2つある。(1) 親を指定した生成（ケース14は `TestActorRef(props, parent.ref, "Child")`、ケース15は `TestProbe.childActorOf`）が、親のセルの状態を理由に例外を投げないこと。(2) 子が `1` を受けて `RuntimeException("expected")` を投げても、送信した側へは伝播せず、失敗は監督とログ（`EventFilter` が1件観測する）で処理されること。移植では (1) は `try_spawn_child`（§2）と既存の `TestProbe::child_actor_of_named` が担う。(2) は、子の失敗を親（TestProbe の testActor。既存の `child_actor_of` は Pekko と同じく stopping strategy を使う）が止める形で写す。ログの観測の部分は注3と同じ扱いにする。

注9（ケース16〜19の生成の形）：原典の companion のうち、型引数だけで作る形（`TestActorRef[WorkerActor]`）は Scala の `ClassTag` とリフレクションでコンストラクタを探す形である。対応表の前例（`docs/design/issue-193-specs.md` の `classOf[…]` の行）どおり、リフレクションでのコンストラクタの検索そのものは移さず、引数を捕まえたクロージャを `Props::new` へ渡す形で写す。したがってケース16〜19は factory を受ける生成（`TestFsmRef` の `new`／`with_name`／`with_supervisor`／`with_supervisor_and_name` と同じ4つの口）で移し、ケース20〜23と API の形は同じになるが、ケースとしては別々に残す（注10）。supervisor と名前の確認は、#181 の §3.3 と同じく、同語反復を避けるためにパスでの確認を必須にする。`actor_ref().path().parent()` が supervisor のパス（ケース16と17と20と21では `resolve_actor_ref` で解決した `/user` の guardian のパス）と等しいこと、`actor_ref().path()` の名前が指定の名前であること、そしてテストのアクターがメッセージを受けたときに `context.self_ref()` を記録し、そのパスの親と名前も同じであることを見る。

注10（ケース20〜23の Props の形）：Rust の `Props` は factory を包んだものが唯一の形なので、ケース16〜19との違いは「`Props` の値を先に作って渡す」ことだけになる。原典で2系統あるのは Scala の factory と Props の両方の入口を Pekko が持つためであり、移植で入口を1本化しても、ケースは8件とも残し、Props を渡す口（`from_props` 系、§3.3）で20〜23を移す。これは件数を保つための形であり、作らない・除外の提案ではない。

作らない・除外の確定は、この文書では1件もしない。提案は §7 に理由と一緒に挙げるだけであり、決めるのは上司である。

## 2. 対応する本体が fraktor-ng-rs にすでにあるか

2026-11-27 の main（`c0f94f0`、[PR #275](https://github.com/j5ik2o/fraktor-ng-rs/pull/275) のマージ直後）で確かめた。

| 原典の本体 | fraktor-ng-rs の現状 |
|:---|:---|
| `TestActorRef`（本体） | 無い。`crates/fraktor-testkit` に同名の型は無い。台帳の `needs` の `test-actor-ref` が未充足である。本件で用意する |
| `CallingThreadDispatcher` | 無い。台帳の `needs` の `calling-thread-dispatcher` が未充足である。dispatcher の種類の列挙に `DispatcherKind::CallingThread` はあるが、実行する dispatcher は無い。#234 の段7b（#259）の担当であり、本件では作らない（§1 の注2） |
| `TestFSMRef`（`TestActorRef` を継承する） | ある。ただし移植では継承ではなく独立した型として入っている（`crates/fraktor-testkit/src/test_fsm_ref/test_fsm_ref_value.rs`）。生成（supervisor と名前の指定）、番兵と処理済み番号、guardian の解決は本件と重なる（§3.4） |
| kernel の `test_support`（feature `test-support` の下の testkit 向けの面） | ある。#181 で `FsmHandle` と `FsmControl` が入った（`crates/fraktor-actor/src/kernel/actor/test_support.rs`）。本件の underlying のハンドルと番兵の制御も、同じ feature の下に足す（§3.1） |
| 任意の親の下に子を作る口（Result を返す形） | ある。#181 で入った `WeakActorSystem::try_spawn_child(parent, props, name, strategy) -> Result<ActorRef, ActorCreationError>`（`crates/fraktor-actor/src/kernel/actor_system/weak_actor_system.rs`）。名前と設定の検証を通す。本件はこれを使うので、新しい生成の口は足さない |
| トップレベルの生成（名前あり） | ある。`ActorSystem::actor_of`、`actor_of_named`、`try_actor_of`（#278 で `actor_of_named` に一本化された経路） |
| guardian の解決 | ある。`ActorSystem::resolve_actor_ref` で `/user` のパスを解決できる（#181 の `TestFsmRef` が使う形と同じ） |
| `Props`（factory、dispatcher 名の記録） | ある。`crates/fraktor-actor/src/kernel/actor/props.rs`。`with_dispatcher(name)` と `dispatcher()` を持つ。dispatcher の解決は生成時に `DispatchSettings` に対して行われる |
| `Actor` trait（`receive`、`pre_start`、`post_stop`、`pre_restart`、`post_restart`、`supervisor_strategy`） | ある。`crates/fraktor-actor/src/kernel/actor/actor_trait.rs`。ケース5が使う `pre_restart`／`post_restart` の数えと、`supervisor_strategy` の差し替え（`OneForOneStrategy`）はこの面で書ける |
| 監督（`SupervisorStrategy`、`OneForOneStrategy` 相当、再起動） | ある。`crates/fraktor-actor/src/kernel/supervision/supervisor_strategy.rs`。ケース5の再起動と、ケース14と15の stopping strategy はこれを使う |
| `PoisonPill` と `Kill` | ある。型ではなく文字列のペイロードとして executor が処理する（`POISON_PILL` と `KILL`）。既存の移植のテストが `"PoisonPill"` を送る前例を持つ |
| watch と `Terminated` | ある。`ActorContext::watch`、`WeakActorSystem::watch`、`ActorSystem::is_actor_terminated`、testkit の `expect_msg_terminated_within`。ケース4の転送は function ref か watch するアクターで書ける |
| ask（`?`、future の完了の確認） | ある。adaptor の `ActorRef::ask`／`ask_with_timeout` が返す `AskFuture`（`crates/fraktor-actor/src/adaptor/ask_future.rs`）に `is_completed` と `await_result(max)` と `try_await_result(max)` がある。待ちの間に待ち手のスレッドが shard を運転する `ask_driver` もある。ケース6と、ケース1と2の入れ子の参照を ask で取り出す形はこれで書ける |
| receive timeout（`setReceiveTimeout`、`ReceiveTimeout`） | 無い。§1 の注4 のとおり、本体にも testkit にも無い。ケース7の前提であり、本件では作らない |
| テストの土台（`within`、`awaitCond`、`expectMsg`、`EventFilter`、`TestProbe`、`childActorOf`） | ある。`FraktorSpec::within`／`await_cond`、`TestProbe`（`child_actor_of`、`child_actor_of_named`、`child_actor_of_with_supervisor`、`watch`、`expect_msg` 系）、`EventFilter`。台帳の `needs` のうち本件以外は充足済みである |
| アクター本体を外から読む口（`underlyingActor`） | 無い。所有権モデルの決定（executor がアクター本体を単独所有する）の下で、本体そのものを返す形は #181 の設計の §2 が記録するとおり未決であり、本件でも本体への参照は返さない。代わりに共有ハンドルとスナップショットで表す（§3.2） |
| アクターの `receive` を直接呼ぶ口（`TestActorRef.receive`） | 無い。executor の外から特定のアクターの処理を1件だけ進める口は無い。本件では番兵で順序を保証する形に寄せる（§3.3） |
| dispatcher の差し替えを TestActorRef 側で行う仕組み | 無い。`Props::with_dispatcher` で名前を記録するだけであり、「指定が無ければ CallingThread に差し替える」という TestActorRef 側の規則は testkit に無い。本件で testkit 側に足す（§3.3） |

## 3. 新しく足すものと、公開 API の形

置き場所の決め方を先に書く。`TestActorRef` は testkit の型なので `crates/fraktor-testkit` に置く。本体側（`crates/fraktor-actor` の kernel）に足すのは、feature `test-support` の下でだけ見える testkit 向けの面だけであり、既定の公開 API には出さない（#181 で決めた形と同じ。約束を足さないため）。新しい型は、syn チェッカーの型ごとに1ファイルの決まりに従い、1型1ファイルで置く。シグネチャは設計の形であり、名前の細部は実装のときに既存の命名へ合わせる。新しいファイルには、自作のものも含めて Apache-2.0 のヘッダーを付ける。

### 3.1 fraktor-actor（kernel、`test-support` の下）に足すもの

汎用のアクター向けの共有ハンドルを1つ足す。#181 の `FsmHandle` が FSM 専用であるのに対し、こちらは任意の `Actor` の個体に付く。

```rust
// crates/fraktor-actor/src/kernel/actor/test_support/actor_handle.rs
/// テストと、テストが起動したアクターが共有するハンドル。
/// 個体の世代、処理を終えた番兵の通し番号、underlying のスナップショット、
/// 生成時に記録した receive timeout と dispatcher の種類を持つ。
#[derive(Clone)]
pub struct ActorHandle { /* Arc<spin::Mutex<ActorShared>> */ }

impl ActorHandle {
  pub fn new() -> Self;
  /// 何もしない番号付きの制御メッセージ（番兵）を送る先の参照と番号の組を作る側が使う。
  pub fn allocate_sequence(&self) -> u64;
  /// ハンドルが持つ、処理を終えた制御メッセージの通し番号の最大値を返す。
  pub fn processed_sequence(&self) -> u64;
  /// 今の個体の世代を返す。再起動のたびに増える。
  pub fn generation(&self) -> u64;
  /// 停止したかどうかを返す。
  pub fn is_terminated(&self) -> bool;
  /// 生成時に TestActorRef が記録した receive timeout を返す（§1 の注5）。
  /// `None` は「設定なし」、`Some(None)` のような二重の包みは作らない。
  pub fn receive_timeout_is_none(&self) -> bool;
  /// 生成時に解決した dispatcher の種類を返す。
  pub fn dispatcher_kind(&self) -> DispatcherKind;
}
```

- 番兵は、#181 の `FsmControl` と同じ考え方の制御メッセージにする。ただし宛先は FSM ではなく汎用のアクターなので、アクターの `receive` には渡さない。原典の `TestActorRef` が `ActorCell` の `autoReceiveMessage` で `InternalGetActor` を横取りする形と同じく、番兵は処理の層で横取りして、ハンドルの処理済み番号だけを進める。横取りの置き場所（executor の配送の前か、セルの受信の前か）は実装の段で既存の制御メッセージ（`PoisonPill` と `Kill` を executor が見る場所）と同じ層にそろえる。利用者のメッセージと同列にメールボックスへ並ぶので、順序は「送った順」に保たれる（I4）。
- ハンドルは `TestActorRef` が1つ作り、包んだ factory がアクターを作るたびに同じハンドルを付け直す（§3.3）。再起動で新しい個体が付いても、通し番号と世代の数えは戻さない。スナップショットは個体ごとに差し替える（I6）。
- 停止のときは、ハンドルの「停止した」印を立てる。`WeakActorSystem::on_termination` の登録（#181 の `TestFsmRef` がドライバの停止に使う仕組み）で testkit 側から立てるか、kernel 側の停止の経路で立てるかは、実装の段で既存の経路にそろえる。

underlying のスナップショットは、型ごとに違うので、ハンドルは型を持たず、testkit 側の型付きの包みが持つ形も検討したが、二重の共有状態は食い違いの元になる。設計としては、ハンドルを型引数付きにせず、スナップショットの複製を作る関数（cloner）を登録する形にする（#181 の `FsmDataCloner` と同じ発想）。

```rust
// 同じファイルの続き（形）
impl ActorHandle {
  /// underlying のスナップショットを書く。アクター側の公開の口（§3.2）が呼ぶ。
  pub fn write_snapshot(&self, snapshot: alloc::boxed::Box<dyn Any + Send + Sync>);
  /// underlying のスナップショットを読む。型が合わなければ `None`。
  pub fn read_snapshot(&self) -> Option<alloc::boxed::Box<dyn Any + Send + Sync>>; // 実体は Arc で共有する
}
```

スナップショットの中身を `Box<dyn Any>` のまま公開面に出す形は粗いので、実装の段では、testkit 側の `TestActorRef<A>` が型付きに読み出す内部の置き場として閉じ、公開 API には型付きの読み取りだけを出す（§3.3）。この置き場の形（`dyn Any` で持つか、`ActorHandle<A>` のように型引数を持たせるか）は、設計レビューで見てほしい点に挙げる（§8）。型引数を持たせると `Props` の factory の包みが型を運ぶ必要があり、`FsmHandle<S, D>` の前例は型引数を持つ側である。

### 3.2 underlying の面：本体を返さず、スナップショットで表す

原典の `underlyingActor` は本体そのものを返し、ケース8はそのフィールド `s` を読む。移植では本体を返さない（§2 の所有権モデルの行）。代わりに、アクターが自分の観測可能な状態をハンドルへ書く口を、feature `test-support` の下に1つ足す。

```rust
// crates/fraktor-actor/src/kernel/actor/test_support.rs の配下（形）
/// アクターが、テストから読める自分の状態の複製をハンドルへ書くための口。
/// `Fsm::attach_handle` の汎用版であり、通常の生成では付かない。
pub trait UnderlyingSnapshot: Send {
  /// 今の自分の状態のうち、テストが読むものの複製を返す。
  fn snapshot(&self) -> alloc::boxed::Box<dyn Any + Send + Sync>;
}
```

- 付ける経路は、`Props` の factory を `TestActorRef` が包むときに、作られた個体へハンドルを登録する形にする。個体が `UnderlyingSnapshot` を実装していなければ、スナップショットは付かず、`underlying` の読み取りは「この型は underlying の観測を付けていない」という理由で panic する（原典で構築に失敗したときに例外になるのに対応する失敗として、API 文書に書く）。
- 書き直しの時点は、`pre_start` の後と、各メッセージの処理の後である。処理の後の書き直しは、executor が個体を貸し出している層で、ハンドルが付いた個体にだけ行う。feature が切れているときは、この分岐自体を `#[cfg(feature = "test-support")]` で囲い、既存の利用者にコストを足さない（#181 の `Fsm` のハンドル保持と同じ扱い）。
- ケース8のテストのアクター `TA` は、フィールド `s: String` を持ち、`snapshot` で `s` の複製を返すよう、テストファイルの中で実装する。テストは `ref_handle.underlying::<String>()` の形ではなく、型を固定した読み取り（§3.3 の `underlying_snapshot`）で `s` を読む。原典の `actor.s` と読み方は違うが、確かめる内容（`"hallo"` を送った後に内部の状態が `"hallo"` になっていること）は同じである。
- 再起動で個体が差し替わったら、古いスナップショットは捨てる。原典の API 文書が「再起動で underlying の参照は捨てられる。古い個体は死後の確認に使える」と書くうち、移植で写せるのは「捨てられる」ほうだけであり、古い個体の保持はしない。この違いは §5 に記録する。

### 3.3 fraktor-testkit に足す `TestActorRef`

```rust
// crates/fraktor-testkit/src/test_actor_ref.rs と test_actor_ref/test_actor_ref_value.rs
pub struct TestActorRef {
  actor_ref: ActorRef,
  handle: ActorHandle,
  name: String,
  supervisor: ActorRef,
}

impl TestActorRef {
  /// 既定の supervisor（guardian）と生成名で作る。原典の `TestActorRef(factory)` と
  /// `TestActorRef(props)` の両方に対応する（Rust では factory は Props の中にある）。
  pub fn new(system: &mut ActorSystem, props: Props) -> Self;
  /// 名前を指定して作る。名前が不正か使用中なら `Err`（原典は例外）。
  pub fn with_name(system: &mut ActorSystem, props: Props, name: &str)
    -> Result<Self, ActorCreationError>;
  /// supervisor を指定して作る。その親の子として作る。
  pub fn with_supervisor(system: &mut ActorSystem, props: Props, supervisor: &ActorRef) -> Self;
  /// supervisor と名前を指定して作る。
  pub fn with_supervisor_and_name(system: &mut ActorSystem, props: Props,
    supervisor: &ActorRef, name: &str) -> Result<Self, ActorCreationError>;
  /// Props を先に作って渡す形。ケース20〜23が使う入口で、中身は上の4つと同じである。
  pub fn from_props(system: &mut ActorSystem, props: Props) -> Self; // ほか3つも同じ接頭辞で揃える

  pub fn actor_ref(&self) -> ActorRef;
  pub fn tell<M: Any + Send + Sync>(&self, message: M, sender: ActorRef); // 原典の `!`
  pub fn name(&self) -> &str;
  pub fn supervisor(&self) -> ActorRef;
  pub fn is_terminated(&self, system: &ActorSystem) -> bool;

  /// 原典の `receive(message)`。sender は dead letters。
  pub fn receive<M: Any + Send + Sync>(&self, message: M);
  /// 原典の `receive(message, sender)`。
  pub fn receive_with_sender<M: Any + Send + Sync>(&self, message: M, sender: ActorRef);
  /// 番兵を送り、その番号を返す。テストが処理の完了を待つために使う。
  pub fn send_sentinel(&self) -> u64;
  pub fn processed_sequence(&self) -> u64;

  /// underlying のスナップショットを型付きで読む。付いていなければ panic（§3.2）。
  pub fn underlying_snapshot<T: Any + Clone + Send + Sync>(&self) -> T;
  /// 生成時に設定した receive timeout が None であることを返す（ケース9）。
  pub fn receive_timeout_is_none(&self) -> bool;
  /// 生成時に解決した dispatcher の種類を返す（ケース10と11）。
  pub fn dispatcher_kind(&self) -> DispatcherKind;
  /// 原典の `watch` と `unwatch`。この参照が監視する形ではなく、
  /// testActor 側の監視と等価な登録を system 経由で行う（§5 に違いを記録する）。
  pub fn watch(&self, system: &ActorSystem, subject: &ActorRef);
  pub fn unwatch(&self, system: &ActorSystem, subject: &ActorRef);
}
```

生成の規則は原典のコンストラクタそのままである。

- Props に dispatcher の指定が無ければ、CallingThreadDispatcher の名前を付けた Props に差し替えてから生成する。指定があれば差し替えない（ケース10と11、I2）。差し替えた名前が `DispatchSettings` で種類 `CallingThread` として解決されるよう、testkit が使う既定の設定にその定義を含める。定義が無いシステムで作る場合は、名前だけが付き、種類は未定義として扱う経路を実装の段で確かめる。
- receive timeout は None に設定したものとして記録する（ケース9、I3）。本体に timeout の口が無い間は、記録はハンドルに持つ（注5）。
- supervisor を指定する生成は、#181 で入った `WeakActorSystem::try_spawn_child` を使い、渡す監督戦略は既定の戦略である。名前なしの生成は既存の生成名の仕組み（`actor_of` が採番する名前）に乗り、独自の採番は作らない。原典の `randomName`（`$` と base64）そのものは写さない。生成名が `$` で始まることは原典の利用者が頼れる約束ではないためである。
- 生成の直後に、対象が開始の失敗で止まっていないことを確かめる。止まっていたら panic する（#181 の `TestFsmRef::start` と同じ契約。原典では CallingThreadDispatcher のため生成時の失敗がその場で例外になる）。
- `receive` は、原典では直接呼び出しである。移植では「メッセージを送り、番兵でその処理の完了まで待つ」形にする（注1、注7）。待ちの上限は testkit の既定の timeout（`DefaultTimeout`）を使う。sender なしの形の sender は dead letters であり、原典の `receive(o)` が `underlying.system.deadLetters` を使うことと一致する。
- `TestFsmRef` との関係は §3.4 に書く。

### 3.4 `TestFsmRef` との関係（土台の共有）

原典では `TestFSMRef` が `TestActorRef` を継承する。移植の `TestFsmRef` は独立した型として先に入った。本件では `TestFsmRef` の公開 API は変えない。共有するのは中身の3つであり、testkit の非公開の生成の補助として切り出す。

- 生成の補助：supervisor と名前の4つの組み合わせから `try_spawn_child`／`actor_of_named` を選び、guardian を解決し、開始の失敗を確かめる部分。`TestFsmRef::start` の中にある同じ処理と二重になるので、非公開の関数1つに寄せ、両方から使う。寄せることで `TestFsmRef` の振る舞いが変わらないことを、既存の `test_fsm_ref_spec` が green のままであることで確かめる。
- 番兵と処理済み番号の考え方：型は別（`FsmControl` と本件の制御）だが、番号の振り方と待ち方は同じにする。テスト側の待ちの補助（番兵を送って番号まで待つ）も、testkit の非公開の補助に寄せる。
- ハンドルの世代と再起動の扱い：考え方は `FsmHandle` とそろえる（番号と世代は再起動をまたいで戻さない。スナップショットは個体ごとに差し替える）。

`TestActorRef` が `TestFsmRef` の土台になるかという問いへの答えは、原典の継承の向きとは逆になる。移植では `TestFsmRef` が先にあり、本件はその生成と待ちの仕組みを共有の補助へ引き上げて使う。将来 `TestFsmRef` を `TestActorRef` の上に組み直すかどうかは、本件の範囲の外であり、公開 API を変えるので別件とする。

### 3.5 no_std で書けるか

- fraktor-actor 側の追加（`ActorHandle`、番兵の制御、スナップショットの置き場）は `core` と `alloc` と、kernel がすでに使う `spin::Mutex` と kernel の `Arc` だけで書ける。`std` の import は足さない。時刻は `core::time::Duration` で表す。スレッドの同一性（注2）は testkit 側とテスト側だけが使うので、kernel には入れない。
- fraktor-testkit 側は、もともと `std` のクレートである（`FraktorSpec` が `std::time::Instant` などを使う）。`TestActorRef` はその範囲で書ける。
- ハンドルの共有はスレッド境界を越えられる（アクターとメッセージに既存の `Send`／`Sync` の境界がある）。

## 4. 守る性質（不変条件）

| 番号 | 不変条件 |
|:---:|:---|
| I1 | 生成の所属：`TestActorRef` で作ったアクターは、指定した supervisor の子として、指定した名前（指定が無ければ生成名）で作られる。`actor_ref().path().parent()` は supervisor のパスと等しく、`actor_ref().path()` の名前はその名前と等しい。既定の supervisor は guardian（`/user`）である |
| I2 | dispatcher の選択：Props に dispatcher の指定が無ければ、生成したアクターの dispatcher は CallingThread の種類になる。指定があれば、その指定が勝ち、TestActorRef は差し替えない |
| I3 | receive timeout の設定：`TestActorRef` で作ったアクターには receive timeout が付かない（None）。アクターが自分の構築で timeout を張る場合（ケース7の型）は、本件の範囲の外である |
| I4 | 制御と利用者のメッセージの順序：番兵を含む制御メッセージはメールボックスの中で送った順に処理される。番兵の処理済み番号は単調に増え、送った最後の番兵の番号まで進んだ時点で、それより前に送った利用者のメッセージはすべて処理済みである |
| I5 | underlying のスナップショットの一致：番兵で待った後のスナップショットは、アクターが最後に完了した処理の後の内部状態の複製と等しい。スナップショットは `pre_start` の後と各処理の後に書き直される |
| I6 | 再起動での差し替え：再起動で個体が差し替わったら、スナップショットは新しい個体のものになり、古い個体のスナップショットは読めない。通し番号と世代の数えは再起動をまたいで戻らない。世代は再起動のたびに増える |
| I7 | 停止後の読み取り：アクターが止まった後で underlying のスナップショットを読むと、理由を書いて panic する（原典は `IllegalActorStateException` を投げる）。`is_terminated` は停止の後で真になる |
| I8 | 名前の拒否：使用中の名前、空の名前、不正な名前での生成は `Err` を返し、既存のアクターには影響しない。止まったアクターが持っていた名前は再利用できる（既存の生成の経路の意味論） |
| I9 | 入れ子の生成：アクターの処理の中から `TestActorRef` で作った子も、`ActorContext::actor_of` で作った子も、作った側とは別の参照を持ち、null にはならない。子の親は作ったアクターである |
| I10 | ハンドル無しのアクターの不変：ハンドルを付けない通常の生成の振る舞いは、既存のテストが見る範囲で変わらない。feature `test-support` が切れているときは、testkit 向けの面は存在しない |

## 5. 壊しうる入力と、原典と違える点

| 入力 | 何が起きうるか | 扱い |
|:---|:---|:---|
| 空文字のアクター名 | 既存の名前の検証が拒否する | `Err` を返す。原典は例外を投げるので、対応する失敗として扱う |
| 使用中の名前、ワイルドカードを含む名前、非常に長い名前 | 名前の検証と一意性は既存の生成の経路の仕事である | その経路に委ね、TestActorRef 側で独自の検証は足さない。ワイルドカードは名前に意味を持たない（パスの照合ではなく完全な名前である） |
| supervisor がすでに止まっている、または開始していない参照 | 原典は、開始していないトップレベルのアクターへの接続で `IllegalStateException` を投げる | `try_spawn_child` が親の状態を理由に `Err` を返す形に写す。名前なしの口（panic する形）では、その理由を書いて panic する |
| Props に未定義の dispatcher 名が付いている | 設定の検証で拒否されるか、名前だけが付くかは経路で違う（既知のずれは [Issue #247](https://github.com/j5ik2o/fraktor-ng-rs/issues/247)） | 既存の経路の意味論に従う。本件で経路の検査の差は直さない。ケース11の `disp1` は型付きの定義を登録するので、この問題には当たらない |
| factory が panic する、または `pre_start` で失敗するアクター | 生成の失敗が呼び出し側へ出るか、監督で処理されるかが経路で違う | 生成の直後に停止を確かめ、止まっていたら panic する（§3.3）。原典では生成時の失敗は例外である |
| 処理の中で例外（panic）に当たるメッセージを `receive` で入れる | 原典では例外が呼び出し側へ届く | 移植では届かない。失敗は監督で処理され、呼び出し側は番兵の待ちと失敗のイベントで気づく（注7）。これは所有権モデル（本体を直接呼ばない）の帰結であり、意図した違いとして記録する |
| 止まったアクターへの送信と `receive` | メッセージは dead letters へ行く | 例外にしない。`receive` の待ちは timeout で失敗する |
| 止まった後の underlying の読み取り | 原典は `IllegalActorStateException` | I7 のとおり panic する |
| 再起動の前後で underlying を読む | 原典は古い個体の参照を保持できる | 移植は保持しない（I6）。死後の確認に古いスナップショットを使うテストは書けない。この違いを記録する |
| receive timeout の境界の値（`Duration::ZERO`、非常に大きい値） | 本体に timeout の口が無い間は、TestActorRef 側の記録は None だけである | 境界の値はケース7の前提が入ったときに扱う。本件では `Duration` の値そのものを受け取る口を作らない |
| 番兵を大量に送る、処理が止まらないアクターへ `receive` する | 待ちが timeout まで続く | 待ちの上限は testkit の既定の timeout。通し番号は `u64` で、現実の件数では回らない |
| 同じアクターへ複数の `TestActorRef` の包みを作る | ハンドルが二重になり、スナップショットが食い違う | 1つの生成に1つのハンドルだけを付ける。包んだ factory の複製が再起動で呼ばれることは想定内であり、同じハンドルを付け直す |
| アクターが `change_behavior` で振る舞いを替える | #181 と同じく、制御の横取りが処理の層にあるかで効き方が変わる | 番兵は処理の層で横取りするので、振る舞いを替えても番兵は処理される設計にする。実装の段で、横取りの置き場所がこの条件を満たすことを確かめる |

## 6. PBT と Quint

### PBT は足す

原典外の性質テストを `crates/fraktor-testkit/tests/test_actor_ref_property_test.rs` に足す。依存は足さず、既存の移植（#177、#181）と同じく固定 seed の SplitMix64 で抽選する（`proptest` の採否は [Issue #212](https://github.com/j5ik2o/fraktor-ng-rs/issues/212) の判断に従う）。

| 番号 | 性質 | 確かめる不変条件 |
|:---:|:---|:---|
| P1 | ランダムな生成の組み合わせ（supervisor の有無、名前の有無、名前の文字列は空文字・長い文字列・重複・ワイルドカード文字を含む）について、成功した生成は I1 のパス条件を満たし、拒否された生成は既存のアクターのパスと状態を変えない | I1、I8 |
| P2 | ランダムなメッセージ列（内部状態を数えるアクターへ、文字列と数値の列）を送り、最後に番兵を送って待った後のスナップショットが、同じ列を単純なモデルで追った内部状態と等しい。途中の状態が最後と同じ値になる列（行って戻る列）を含める（値の一致で待つ形の空振りを避けるため、待ちは番兵の番号で行う） | I4、I5 |
| P3 | ランダムな再起動の列（`Kill` を挟む）について、世代が単調に増え、再起動の後のスナップショットは新しい個体の開始状態から追った値になり、通し番号は戻らない | I6、I4 |
| P4 | dispatcher の指定の有無と名前の抽選について、解決した種類が I2 の規則どおりになる（指定なしは CallingThread、指定ありはその定義の種類） | I2 |

### Quint は書く

判断：書く。理由は、`TestActorRef` の中核が状態遷移だからである。個体は「開始前→稼働→（再起動で世代が変わる）→停止」と移り、underlying のスナップショットの有効さと番兵の通し番号と世代は、その遷移の列全体で成り立つ必要がある（I4、I5、I6、I7）。固定のテストは再起動と停止と送信が交差する列を狭い範囲でしか見ない。既存の仕様は、この遷移を捉えていない。`crates/fraktor-testkit/specs/watch_handle.qnt` は watch のハンドル、`crates/fraktor-actor/specs/` の FSM の仕様は機械の遷移であり、汎用のアクターの個体の世代と underlying の差し替えは範囲の外である。

範囲は小さく絞る。置き場所は testkit の既存の仕様と並ぶ `crates/fraktor-testkit/specs/test_actor_ref.qnt` の1ファイルとする。書き方は CI の決まり（`docs/design/quint-in-ci.md`）に合わせ、不変条件は `val invariant` を1つにまとめ、`run` の名前は `Test` で終え、負の対照は `run bug…Test` で `.expect(not(...))` を期待する形にする。

- 状態：個体の世代、停止したかどうか、今の個体のスナップショットの版（個体ごとに増える識別子）、処理済みの通し番号、送った最後の番兵の番号
- 遷移：`send`（利用者のメッセージ。スナップショットの版を今の個体のものとして進める）、`sentinel`（番兵の発行と処理を分ける）、`restart`（世代を増やし、スナップショットを新しい個体の版に差し替え、通し番号は保つ）、`stop`（停止の印を立て、スナップショットを無効にする）
- 不変条件：処理済み番号は単調であり、送った番兵以下であること（I4）。スナップショットの版は常に今の世代の個体のものであること（I5、I6）。停止の後はスナップショットが有効でないこと（I7）
- 負の対照：再起動でスナップショットを差し替えない `bugStaleSnapshot`、停止の後もスナップショットを有効のままにする `bugSnapshotAfterStop`、番兵の番号を戻す `bugSequenceRewind` を置き、それぞれ狙った不変条件が落ちることを確かめる

モデルにしないもの：実時間、dispatcher の実行そのもの（段7b の範囲）、メッセージの中身（スナップショットの版としてだけ扱う）、監督の方針の選択（再起動するかは遷移として与える）。

## 7. 作らない・除外の提案（決めるのは上司であり、この文書では決めない）

| 番号 | 提案 | 理由 |
|:---:|:---|:---|
| 提案1 | ケース7（support receive timeout）を、本件の green の範囲から外し、アクター側の receive timeout の本体が入った後の別件で移す形にするかどうか | 前提の本体（`setReceiveTimeout` と `ReceiveTimeout` と期限の発火）が fraktor-ng-rs のどこにも無い。本件で本体ごと作ると、ReceiveTimeoutSpec の対象と重なり、範囲が大きく膨らむ。§1 では「数から外さず red のまま残す」形で書いた。ノードを green にできるかどうかは、この提案の判断にかかる |
| 提案2 | ケース4と14と15の `EventFilter` が例外の型で絞る部分を、移植の確認から外すかどうか | Rust には例外の型階層が無く、`ActorKilledException` に当たる型も無い。失敗のイベントそのものを観測する形で代替できる場合は提案を取り下げる。第2段の red の前に、既存の `EventFilter` で代替できるかを確かめてから判断してほしい |
| 提案3 | ケース10と11の確認を、dispatcher の「クラス」ではなく種類（`DispatcherKind`）で見る形を、移植の忠実さの判断として確定するかどうか | 対応表の前例（実行時クラス階層は型で表す）に従う形であり、ケースの除外ではない。ただし原典の確認の形を変えるので、判断として記録してほしい。実行そのものは段7b の後である |

いずれの提案も、承認が台帳に記録されるまでは未承認として扱う（移植の規約どおり、`cases.excluded` と `cases.not_built` への記入は人間だけが行う）。

## 8. 第2段の進め方（この設計が通ってから）と、レビューで見てほしい点

### 第2段の進め方

1. red：テストの担当が `crates/fraktor-testkit/tests/test_actor_ref_spec.rs` に23件を書き、台帳の対象ノードを red にする。段7b が前提の同期性の3件とケース7は、red のまま残す形で置く（§1 の注2、注4）。失敗が「型が無い」ことによることを確かめる。
2. 本体：§3 の順に、fraktor-actor の `ActorHandle` と番兵の横取りと underlying のスナップショットの口、fraktor-testkit の `TestActorRef` と生成の共有の補助を足す。新しい関数や公開 API を足す前に、同じ働きの既存の関数を探し、並行の別の PR と同じ口を足していないかも確かめる（本件では生成の口は `try_spawn_child` を使い、新しい生成の口は足さないことを §2 で確かめた）。レビューの経過をコードのコメントに書かない。理由は設計の文書と PR に書く。
3. green：段7b と receive timeout の前提が要る部分を除いて通し、台帳の対象ノードの件数を実数（23件）で記録する。ノードを green にするかどうかは §7 の提案1と §1 の注2の扱いの判断に従う。
4. 検査：CI と同じ検査を流す（`cargo fmt --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`、ledger/tools の Python テスト、Quint の typecheck と run と負の対照、syn チェッカー）。加えて、dev-dependency を引かない `cargo clippy -p fraktor-actor --lib -- -D warnings` を流し、feature を切った形で testkit 向けの面が既定の公開 API に出ていないことを確かめる（#181 の前例）。

### レビューで見てほしい点

- 本体を返さず、共有ハンドルとスナップショットで underlying を表す形（§3.2）が、所有権モデルの決定と #181 の `FsmHandle` の形の延長として妥当か。特に、スナップショットの置き場に型を持たせるか `dyn Any` で持つか（§3.1 の末尾）
- dispatcher の差し替えを testkit 側の規則として足すこと（§3.3）と、種類で確認する形（§7 の提案3）
- 段7b が前提の同期性の3件とケース7を red のまま残す形で、台帳のノードの green の判定をどうするか（§1 の注2、§7 の提案1）
- `TestFsmRef` との共有を非公開の生成の補助に閉じ、公開 API を変えないこと（§3.4）
- `watch`／`unwatch` の写し方。原典は `TestActorRef` 自身が監視者になる形だが、移植の `TestActorRef` はアクターではないので、system 経由の登録で代替する形を §3.3 に書いた。この形でケースの確認に影響は無い（本 Spec のケースは `watch` を使わない）が、公開 API として妥当か
- Quint の範囲（§6）が、既存の仕様と重ならず、足りているか
