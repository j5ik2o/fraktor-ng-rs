# 設計：#182 TestActorRefSpec（testkit に TestActorRef の能力を用意する）

状態：**未承認（設計の独立レビューの指摘を反映した改訂版。伺うことは §8 に3件ある）**。

[Issue #182](https://github.com/j5ik2o/fraktor-ng-rs/issues/182) の第1段（設計だけ）の記録である。実装は、この設計が通ってから行う。この文書は実装を含まない。第2段では、テストを Claude Opus の担当が先に red で書き、実装の担当はそのテストを書き換えずに通す実装だけを書く。設計に付く Quint の仕様（`crates/fraktor-actor/specs/test_actor_ref.qnt`）は、この段で置く（§6）。

対象は Pekko v2.0.0 の `testkit/src/test/scala/org/apache/pekko/testkit/TestActorRefSpec.scala` である。原典の本体は `testkit/src/main/scala/org/apache/pekko/testkit/TestActorRef.scala`（`LocalActorRef` を継承し、Props に dispatcher の指定が無ければ `CallingThreadDispatcher` へ差し替え、`underlyingActor` と `receive` の直接呼び出しと `watch`／`unwatch` を足したもの）と、`testkit/src/main/scala/org/apache/pekko/testkit/CallingThreadDispatcher.scala`、testkit の `reference.conf`（`pekko.test.calling-thread-dispatcher` の定義）である。

台帳の対象ノードは `testkit/src/test/scala/org/apache/pekko/testkit/TestActorRefSpec.scala`（`testkit:TestActorRefSpec`、`test_cases_approx: 23`、`status: unported`）の1件だけである。第2段の終わりの見込みは **`status: red`、`cases.total: 23`、`ported: 20`、`passing: 20`** である（§7）。段7b（[Issue #259](https://github.com/j5ik2o/fraktor-ng-rs/issues/259)）が前提の3件（ケース3、5、6）は、弱めた版を書かずに unported のまま残す。この3件の扱いと、ノードが green にならないまま #182 を完了とするかどうかは、§8 で伺う。

前提として読み直したものは次のとおりである。移植の規約（`.takt/facets/knowledge/fraktor-porting-rules.md`）、速さより Pekko の意味論を優先する決定（`docs/decisions/pekko-semantics-first.md`）、Scala・Java の書き方と Rust の対応表（`docs/decisions/scala-java-to-rust-correspondence.md`）、[Issue #181](https://github.com/j5ik2o/fraktor-ng-rs/issues/181) の設計と実装（`docs/design/issue-181-testfsmref.md`、`TestFsmRef` と kernel の `test_support`）、[Issue #200](https://github.com/j5ik2o/fraktor-ng-rs/issues/200) の設計（`docs/design/issue-200-receive-timeout.md` の §9、§10、§13。台帳を red のまま完了とした前例）、[Issue #234](https://github.com/j5ik2o/fraktor-ng-rs/issues/234) の設計（`docs/design/issue-234-multithread-executor.md`。CallingThreadDispatcher は段7b、#259）。

## 0. 変更の記録

### 0.1 この改訂で変えたこと（2026-11-27）

設計の独立レビュー（高7件・中5件・低4件）を受けて、次のとおり直した。指摘ごとの対応は §11 の表にある。

1. **段7b が前提のケースの扱い**：「本体を green にして、同期性だけを別名の red のテストに分ける」形をやめた。弱めた版を green に数えない。ケース3、5、6 は unported のまま残し、台帳のノードは #200 の前例どおり `status: red`（23/20/20）で管理する（§7）。決めるのは上司なので、§8 で伺う。
2. **#200 が main に入ったことの反映**：[PR #283](https://github.com/j5ik2o/fraktor-ng-rs/pull/283) で `ActorContext::set_receive_timeout` と `ActorContext::receive_timeout() -> ReceiveTimeoutSetting` が入った。ケース7は今の本体で移せるので、移す側へ移した（初版の提案1は取り下げ）。ケース9は、実行器が持つその actor の設定を読む形にし、初版の `receive_timeout_is_none() -> bool`（生成時に書いた値を読み戻すだけの同語反復）はやめた。
3. **アクターの中からの生成（ケース1、3、5）**：`ActorContext` から生成する口を足した。テストのスレッドからの生成と、同じ実行器の関数1つを通る（入口で検査が違う形を作らない）。supervisor を省いた生成の親は `/user` であり、作ったアクターではない（I9 を訂正した）。
4. **supervisor を指定した生成の監督戦略（ケース5、14）**：既定の戦略を固定で渡す形をやめた。子は supervisor の戦略に従う。supervisor がアクターなら、その `supervisor_strategy`（再起動の後の差し替えも含めて、今の値）を引き継ぐ。testkit の test actor は、原典の `TestActor` の `DelegatingSupervisorStrategy` の既定どおり stopping strategy を持つ（§3.4）。
5. **underlying の読み取り**：`Props` の型消去で実装できない形（別トレイトの判定、factory の包み直し）をやめた。`Actor` trait に feature `test-support` の下の既定メソッド `underlying_snapshot` を足し、読み取りは、原典の `InternalGetActor` と同じくメールボックスを通る制御メッセージ（プローブ）で行う。プローブを処理するときに、実行器がその時点の個体のスナップショットを取る（§3.2）。処理のたびの書き直しは無くなり、`post_restart` の後の未更新の穴も無くなった。
6. **`receive` の写し方（ケース12、13）**：番兵を送って待つ形をやめた。原典どおり、メールボックスを通さずにアクターの振る舞いを1回直接呼ぶ口を足した。呼び出しの中で積まれたシステムメッセージ（自分の停止）は、dispatcher が CallingThread の種類のときだけ、返る前に処理する（§3.5）。
7. **dispatcher の取り違え**：`DispatchSettings` ではなく `Dispatchers`／`DispatcherConfig` を使う。`FraktorSpec` が作るシステムは、原典の testkit の `reference.conf` と同じく `pekko.test.calling-thread-dispatcher`（種類 `CallingThread`）を既定で知る。ケース11のために `FraktorSpec::with_dispatchers` を足す（§3.3）。
8. **EventFilter**：初版の提案2（例外の型での絞り込みを外す）は取り下げた。既存の `EventFilter::typed` で写せる。原典の `occurrences` の既定（`Int.MaxValue`＝数を見ずに黙らせるだけ）を写す `EventFilter::intercept_any` を足す（ケース4）。
9. **公開 API の整理**：生成の口は、factory を受ける4つと `Props` を受ける4つに分けた（同じ形の口の重複をやめた）。`watch`／`unwatch` は、包んだアクター自身を監視者にし、`subject` を返す。`Display` は原典の `toString` と同じ `TestActor[<path>]`。番兵の番号を公開 API に出すのはやめた。
10. **Quint の仕様を置いた**：`crates/fraktor-actor/specs/test_actor_ref.qnt`。プローブが配送中の停止、再起動の失敗による停止、直接の受信での自分の停止、を遷移に入れた（§6）。
11. **`TestFsmRef` は変えない**：初版の「生成の補助を `TestFsmRef` と共有する」はやめた。`TestFsmRef` にも同じ監督戦略と dispatcher のずれがあることを見つけたので、別件として報告する（§9）。

### 0.2 初版（2026-11-27、`e74d123`）

初版の判断のうち、この改訂で残したもの：本体を返さず共有ハンドルで underlying を表すこと、型引数を持たない `TestActorRef`、名前と設定の検証を生成の経路に委ねること、ケース10と11を dispatcher の種類で見ること、PBT を足すこと。

## 1. 原典のケースを数え直した結果と、移す先の対応表

原典を `" in {"` の数で数え直した。`ignore` と `pending` は無い。

- 「A TestActorRef should be an ActorRef, hence it」の下に7件
- 「A TestActorRef」の下に8件
- 「A TestActorRef Companion Object」の下に8件
- 合計23件。台帳の目安23件と一致する

移す先は `crates/fraktor-testkit/tests/test_actor_ref_spec.rs`（ノード1件＝ファイル1件、冒頭に出典のパスを残す）である。unported のまま残す3件は、テストファイルに書かず、冒頭の注釈に「段7b（#259）の後に移す」とケース名つきで書く（#200 の `receive_timeout_spec.rs` の冒頭と同じ形）。`#[ignore]` の付いた red のテストは置かない。

| 番号 | 原典のケース | Rust のテスト名 | 区分 |
|:---:|:---|:---|:---|
| 1 | support nested Actor creation（used with TestActorRef） | `supports_nested_actor_creation_with_test_actor_ref` | 移す（注1） |
| 2 | support nested Actor creation（used with ActorRef） | `supports_nested_actor_creation_with_actor_ref` | 移す（注1） |
| 3 | support reply via sender() | — | **unported**。段7b の後（注2、§8 の伺い1） |
| 4 | stop when sent a poison pill | `stops_when_sent_a_poison_pill` | 移す（注3） |
| 5 | restart when Kill:ed | — | **unported**。段7b の後（注2、§8 の伺い1） |
| 6 | support futures | — | **unported**。段7b の後（注2、§8 の伺い1） |
| 7 | support receive timeout | `supports_receive_timeout` | 移す（注4） |
| 8 | allow access to internals | `allows_access_to_internals` | 移す（注5） |
| 9 | set receiveTimeout to None | `sets_receive_timeout_to_none` | 移す（注4、注5） |
| 10 | set CallingThreadDispatcher | `sets_calling_thread_dispatcher` | 移す（注6） |
| 11 | allow override of dispatcher | `allows_override_of_dispatcher` | 移す（注6） |
| 12 | proxy receive for the underlying actor without sender() | `proxies_receive_without_sender` | 移す（注7） |
| 13 | proxy receive for the underlying actor with sender() | `proxies_receive_with_sender` | 移す（注7） |
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

共有の状態：原典は companion object の可変変数（`counter`、`thread`、`otherthread`）をケースの間で共有し、ScalaTest の逐次実行に頼る。`cargo test` は `#[test]` を並列に流すので、`static` やグローバルの状態にしない。各テスト関数の中で `Arc<AtomicI32>` と `Arc<spin::Mutex<Option<std::thread::ThreadId>>>` を作り、テストのアクターに渡す。テストのスレッドの `ThreadId` は、そのテスト関数の冒頭で `std::thread::current().id()` を取る（原典の `val thread = Thread.currentThread` に当たる）。

注1（ケース1と2、アクターの中からの生成）：原典は、`TestActorRef(Props(...))` で作るアクターの構築の中で、入れ子のアクターを作る。ケース1は入れ子も `TestActorRef(Props(...))`（supervisor を省くので親は `/user`）、ケース2は `context.actorOf(Props(...))`（親は外側のアクター）である。Rust のアクターの構築（factory）は `ActorContext` を持たないので、入れ子は `pre_start` で作る。ケース1は §3.3 の `TestActorRef::in_context`（supervisor なし、名前なし）、ケース2は `ActorContext::actor_of` で作る。`ActorContext::actor_of` は名前が要るので、テストが名前を作って渡す（`actor_of` の API 文書のとおり、原典の `context.actorOf(props)` の生成名は呼び出し側が作る）。外側は `?`（`ActorRef::ask`）で入れ子の参照を受け取り、`AskFuture::await_result` で待つ。原典の `Await.result` も待つので、待ち方は弱めていない。`a should not be null` と `nested should not be null` は、Rust の参照が null を取れないので型で保証される（対応表の `null` の行と同じく、null を表す値を型の上で作れない）。テストは、ask の返事が `ActorRef` として取り出せることと、`a` と別の参照であること（`a should not be theSameInstanceAs(nested)`）を確かめる。

注2（段7b が前提の3件）：ケース3、5、6 は、送ったその場で処理が終わることそのものを確かめる。ケース3は4回の `!` の直後に待ちなしで `counter should ===(0)` を2回見て、`assertThread()` で処理が別のスレッドで起きていないことを見る。ケース5は `boss ! "sendKill"` の直後に待ちなしで `counter should ===(0)`（子の `preRestart` と `postRestart` が呼ばれたこと）を見て、`assertThread()` を見る。ケース6は `a ? "work"` の直後に `f.isCompleted should be(true)`（"CallingThreadDispatcher means that there is no delay"）を見る。どれも複数のアクター（クライアント、サーバー、ワーカー、boss と子、ask の一時の宛先）の処理が送り手のスレッドで同期して進むことに頼る。今の実行器では送信はキューに入り、誰かが運転するまで処理されない。待ちを入れて書くと、原典の確かめる内容（即時性と、送り手のスレッドで処理されること）を外した弱いテストになる。今の1スレッドの実行器を送信のたびに空になるまで回す形（`run_until_idle`）で見かけを合わせることもできるが、それは今の実行器の作りから生まれる強さ（全体が1スレッドで同期して進むこと）に頼る形であり、`docs/decisions/pekko-semantics-first.md` が「直す」と決めた作りのずれに当たる。したがって、この3件は弱めた版を書かずに unported のまま残し、数からは外さない。段7b の CallingThreadDispatcher が入った後に、原典どおりの形で移す。扱いは §8 の伺い1にまとめる。

注3（ケース4）：原典は `EventFilter[ActorKilledException]().intercept { ... }` で囲む。`EventFilter[A]()` の `occurrences` の既定は `Int.MaxValue` であり、`TestEventListener.scala` の `awaitDone` が `todo == Int.MaxValue || todo == 0` で判定するとおり、件数を見ずに黙らせるだけである。`PoisonPill` による停止は `ActorKilledException` を出さないので、原典でも0件で通る。移植は `EventFilter::typed("ActorKilledException").intercept_any(...)`（§3.3）で写す。既存の `intercept` は件数1を確かめるので、使うと0件で必ず落ちる。`PoisonPill` は既存の移植の前例どおり文字列のペイロード `"PoisonPill"` で送り、sender は test actor にする。watch するアクターは `pre_start` で `context.watch(&a)` を呼び、`Terminated` を包んで test actor へ転送する。`expect_msg` 系で `WrappedTerminated(Terminated(a))` を待ち、その後で `a.is_terminated()` が真であることと、`assertThread()`（記録された別のスレッドが無いか、テストのスレッドと等しいこと）を確かめる。ケース4の `WorkerActor` は利用者のメッセージを1件も受けない（`PoisonPill` は振る舞いに届かない）ので、原典でも `otherthread` は書かれない。移植でも同じ形で写すので、弱めていない。

注4（ケース7とケース9、receive timeout）：#200 が main に入ったので、ケース7は今の本体で移す。原典の `ReceiveTimeoutActor` は構築で `context.setReceiveTimeout(1.second)` を呼ぶ。移植は `pre_start` で `context.set_receive_timeout(ReceiveTimeoutSetting::Finite(Duration::from_secs(1)))` を呼び、`ReceiveTimeout` を受けたら test actor へ `"timeout"` を送って自分を止める。テストは `expect_msg("timeout")` で待つ（既定の3秒の中に1秒の期限が入る）。`FraktorSpec` のシステムは単調時計を持つので、有限の期限を張れる（#200 の `receive_timeout_spec.rs` と同じ）。ケース9は `underlyingActor.context.receiveTimeout` が `Duration.Undefined` であることを見る。移植は `TestActorRef::receive_timeout()` が `ReceiveTimeoutSetting::Undefined` を返すことを見る。この値は、プローブを処理した時点で実行器がそのアクターについて持つ設定（`ActorContext::receive_timeout` が返すものと同じ）であり、生成時に testkit が書いた値ではない（§3.2）。なお、原典の `TestActorRef` の API 文書は「receiveTimeout を None にする」と書くが、v2.0.0 の本体にその処理は無く、ケース9が見るのはセルの既定値の `Undefined` である。移植も設定を触らない（I3）。

注5（ケース8と、underlying の読み取りの同期）：原典は `ref ! "hallo"` の直後に `ref.underlyingActor.s` を読む。原典の `underlyingActor` は、個体がまだ見えないとき `this ? InternalGetActor` でメールボックスを通って個体を受け取る。移植の `underlying_snapshot` は、この `InternalGetActor` の経路を常に通す形にする（§3.2）。読み取りはメールボックスへプローブを送り、プローブが処理されるまで実行器を回してから、プローブを処理した個体のスナップショットを返す。テストの形は原典と同じ（送った直後に、待ちを書かずに読む）であり、確かめる内容（`"hallo"` の後に内部の状態が `"hallo"` であること）も同じである。同期は読み取りの API の中にあり、対象のアクター1つのメールボックスの順序だけに頼る（注2の、複数のアクターが同期して進むことには頼らない）。dispatcher を差し替えたアクター（ケース11の形）では、原典の読み取りは同期しないことがあり、移植は常に同期する。これは原典が許す振る舞いの範囲の中で選べる振る舞いを狭めることであり、`docs/decisions/pekko-semantics-first.md` の「狭めはずれにしない」に当たる。同期を利用者への約束として API 文書に書くのは、CallingThread の種類のアクターについてだけにする（原典の約束の範囲）。ケース8のテストのアクター `TA` は、フィールド `s: String` を持ち、`underlying_snapshot` で `s` の複製を返す。

注6（ケース10と11、dispatcher の種類）：原典は dispatcher のクラス（`getClass`）を見る。対応表の「Scala の実行時クラス階層で分類すること」の行（分類子は型で表す）に従い、移植は生成のときに実行器が解決した dispatcher の種類（`DispatcherKind`）を見る。ケース10は `Some(DispatcherKind::CallingThread)`、ケース11は `Some(DispatcherKind::Default)` である。対応表に同じ種類の決定があるので、伺いはしない（初版の提案3は取り下げた）。ケース10が見るのは種類だけであり、原典も実行そのもの（送り手のスレッドで処理されること）はこのケースでは見ない。CallingThread の種類の実行そのものは段7b の仕事である（§9 に記録する）。ケース11の `PekkoSpec("disp1.type=Dispatcher")` は、前例（#154 の D1、G0b の決定 E、#192）どおり文字列の設定体系を作らず、`FraktorSpec::with_dispatchers(name, vec![DispatcherConfig { id: "disp1".into(), kind: DispatcherKind::Default }])` で写す（原典の `type=Dispatcher` は既定の種類の dispatcher である）。

注7（ケース12と13、`receive`）：原典の `receive(o)` と `receive(o, sender)` は、メールボックスと `autoReceiveMessage` を通さずに `underlying.receiveMessage(o)` を直接呼ぶ。sender を省くか `null` を渡すと `deadLetters` になる。ケース12と13の `WorkerActor` は `"work"` を受けて `sender() ! "workDone"` を送り、`context.stop(self)` を呼ぶ。原典は直後に `ref.isTerminated should ===(true)` を見る。これが真になるのは、`TestActorRef` の dispatcher が CallingThread で、`context.stop(self)` が積んだ `Terminate` が送り手のスレッドでその場で処理されるからである。移植の `receive` は、振る舞いを1回直接呼ぶ口（§3.5）で写し、dispatcher の種類が CallingThread のときは、その呼び出しが積んだ対象のアクター自身のシステムメッセージを返る前に処理する。ほかのアクターのメールボックスは回さないので、注2の「複数のアクターが同期して進むこと」には頼らない。ケース13は、その後で test actor が `"workDone"` を受けることを `expect_msg` で見る。

注8（ケース14と15）：原典の確認は2つある。(1) 親を指定した生成（ケース14は `TestActorRef(props, parent.ref, "Child")`、ケース15は `TestProbe.childActorOf(props, "Child")`）が例外を投げないこと。(2) 子が `1` を受けて `RuntimeException("expected")` を投げても送った側へは伝わらず、失敗は監督とログで処理され、`EventFilter[RuntimeException](occurrences = 1, message = "expected")` がちょうど1件を見ること。移植は (1) を `TestActorRef::with_supervisor_and_name`（`TestProbe::test_actor` を supervisor に渡す）と既存の `TestProbe::child_actor_of_named` で写し、(2) を既存の `EventFilter::typed("RuntimeException").with_message("expected").intercept(...)`（件数1）で写す。実行器は振る舞いの panic を原因名 `RuntimeException` とその文言で発行する（`executor_value.rs` の `RUNTIME_CAUSE`）。ケース14の子は、原典どおり test actor の stopping strategy で止まる（§3.4。原典の `TestActor` の `DelegatingSupervisorStrategy` は、登録の無い子に `stoppingStrategy` を使う）。

注9（ケース16〜19）：原典の型引数だけで作る形（`TestActorRef[WorkerActor]`）は、`ClassTag` とリフレクションでコンストラクタを探す形である。対応表の `classOf[…]` の行どおり、リフレクションでのコンストラクタの検索は移さず、factory を受ける4つの口（§3.3）で写す。原典の `WorkerActor` は構築で `val supervisor = context.parent` と `val name = context.self.path.name` を持つ。移植の `WorkerActor` は `pre_start` で `context.self_ref()` を記録し、`underlying_snapshot` でそれを返す。テストは、記録した参照の名前が指定の名前であることと、記録した参照のパスの親を `resolve_actor_ref` で解決した参照が supervisor と等しいこと（`ActorRef` の等しさ。原典の `should be(parent)` と同じく参照の同一性を見る）を確かめる。既定の supervisor のケース（16、17、20、21）では、supervisor は `resolve_actor_ref` で解決した `/user` の guardian である（原典の `system.guardian`）。

注10（ケース20〜23）：原典の Props を受ける4つの `apply` を、`Props` を受ける4つの口（§3.3 の `from_props` 系）で写す。factory の口とは引数の型が違う別の口であり、同じ形の口の重複ではない。

作らない・除外の確定は、この文書では1件もしない。数から外す・弱めるおそれのあるものは §8 にまとめた。

## 2. 対応する本体が fraktor-ng-rs にすでにあるか

2026-11-27 の main（`53014ed`。#200 の [PR #283](https://github.com/j5ik2o/fraktor-ng-rs/pull/283) と #234 段4a の [PR #291](https://github.com/j5ik2o/fraktor-ng-rs/pull/291) が入った後）で確かめた。

| 原典の本体 | fraktor-ng-rs の現状 |
|:---|:---|
| `TestActorRef`（本体） | 無い。本件で用意する |
| `CallingThreadDispatcher` の実行 | 無い。`DispatcherKind::CallingThread` の列挙子だけがある（`crates/fraktor-actor/src/kernel/dispatch/dispatcher_kind.rs`）。実行は #234 の段7b（#259）の担当であり、本件では作らない。台帳の `ledger/capabilities.yaml` は `calling-thread-dispatcher` の `provided_by` を `G0c` としているが、実行は入っていない（§9 で報告する） |
| testkit の `reference.conf` の `pekko.test.calling-thread-dispatcher` | 無い。`FraktorSpec` のシステムは既定の dispatcher（`pekko.actor.default-dispatcher`）しか知らない。本件で `FraktorSpec` に足す（§3.3） |
| dispatcher の表 | ある。`kernel::dispatch::Dispatchers`（`DispatcherConfig { id, kind }` の列、`has_dispatcher`、`kind_of`）が `ActorSystemSetup` の `Setup` としてシステムへ入る（`actor_system_value.rs` の `with_setup`）。`DispatchSettings` はメールボックスと stash の容量（`MailboxDefinition`）だけを持ち、種類を持たない。初版はこの2つを取り違えていた |
| 生成時の設定の検証 | ある。`Executor::verify_configuration` が、Props の dispatcher 名（指定が無ければ既定の名前）が `Dispatchers` か `DispatchSettings` に定義されているかを見て、無ければ `ActorCreationError::Configuration` で拒否する。`try_spawn_user`（`ActorSystem::try_actor_of`／`actor_of_named` の経路）と `try_spawn_child_from_weak`（`WeakActorSystem::try_spawn_child`）が通る。`ActorContext::actor_of` は通らない（既知のずれ、[Issue #247](https://github.com/j5ik2o/fraktor-ng-rs/issues/247)、open） |
| 任意の親の下に子を作る口 | ある。`WeakActorSystem::try_spawn_child(parent, props, name, strategy)`。渡した `strategy` を子が自分のものとして持ち（`strategy_inherited = false`）、親の戦略を引き継がない。本件の supervisor 指定の生成には使えない（§3.4） |
| アクターの中から子を作る口 | ある。`ActorContext::actor_of(props, name)`（親の戦略を引き継ぐ、`strategy_inherited = true`、設定は検証しない）と `actor_of_with_strategy`。`ActorContext` は実行器への `&mut Executor` を持つ。アクターの処理の間は実行器のロックが取られているので、その中から `WeakActorSystem` の口を呼ぶと `already borrowed` で panic する（`kernel/sync.rs` の `lock`） |
| 親の戦略の表 | ある。実行器の `ParentEntry` が親ごとの戦略を持つ。アクターは開始（`ActorCell::create`）と再起動（`ActorCell::restart`）のたびに自分の `supervisor_strategy()` を登録する。引き継ぐ子の失敗は、失敗した時点の親の戦略で決まる（`strategy_for`） |
| 開始の非同期 | #234 段4a で、`pre_start` は子のメールボックスの `Create` で、生成の呼び出しが返った後に走る。`TestFsmRef` は、生成の直後に開始するまで `run_one` を回してから開始の失敗を確かめる |
| testkit の test actor | ある。`TestKitBase` が function ref として作る（`test_kit_base.rs` の `spawn_function_ref`）。`child_actor_of` は stopping strategy を明示して渡す。function ref 自身には親の戦略の登録が無い |
| receive timeout | ある（#200）。`ActorContext::set_receive_timeout`、`ActorContext::receive_timeout() -> ReceiveTimeoutSetting`（`Undefined`、`Infinite`、`Finite`）、`ReceiveTimeout`。実行器がアクターごとの設定を持つ |
| `EventFilter` | ある。`EventFilter::typed(cause_name)`、`with_message`、`intercept`（件数1を確かめる）。実行器は失敗を `ActorKilledException`（`Kill`）と `RuntimeException`（振る舞いの panic）の原因名で発行する。既存の移植（`actor_life_cycle_spec.rs`、`supervisor_misc_spec.rs`）が使う。`occurrences` の既定（`Int.MaxValue`）を写す口は無い |
| `PoisonPill` と `Kill` | ある。文字列のペイロードとして実行器が配送の層で処理する（`POISON_PILL`、`KILL`） |
| watch と `Terminated` | ある。`ActorContext::watch`、`WeakActorSystem::watch(watcher, target)`、`WeakActorSystem::unwatch(watcher, target)`。`ActorSystem::watch` は `&mut self` を取り、`ActorSystem` に `unwatch` は無い |
| ask | ある。adaptor の `ActorRef::ask` が返す `AskFuture`（`is_completed`、`await_result`、`try_await_result`） |
| `TestFsmRef` | ある（#181）。生成、開始の確認、番兵、guardian の解決を持つ。supervisor 指定の生成は `try_spawn_child` に既定の戦略を渡す（§9 で別件として報告する） |
| アクター本体を外から読む口（`underlyingActor`） | 無い。所有権モデル（実行器がアクター本体を単独で所有する）の下で、本体への参照は返さない（#181 の設計の §2 と同じ）。本件はスナップショットで表す（§3.2） |
| アクターの振る舞いを直接呼ぶ口（`TestActorRef.receive`） | 無い。本件で `test-support` の下に足す（§3.5） |

## 3. 新しく足すものと、公開 API の形

置き場所の決め方を先に書く。`TestActorRef` は testkit の型なので `crates/fraktor-testkit` に置く。本体側（`crates/fraktor-actor` の kernel）に足すのは、feature `test-support` の下でだけ見える testkit 向けの面だけであり、既定の公開 API には出さない（#181 と同じ。約束を足さないため）。新しい `pub` の型は1型1ファイルで置く。新しいファイルには Apache-2.0 のヘッダーを付ける。シグネチャは設計の形であり、名前の細部は実装のときに既存の命名へ合わせる。ただし、§3.6 の一覧に無い `pub` を足すときは、既存の `pub` と同じ働きのものが無いかを先に探す。

### 3.1 fraktor-actor（kernel、`test-support` の下）に足すもの

#### `ActorHandle`（共有のハンドル）

```rust
// crates/fraktor-actor/src/kernel/actor/test_support/actor_handle.rs
/// The handle a test shares with one actor it started through a
/// `TestActorRef`. The executor writes it; the test reads it.
#[derive(Clone)]
pub struct ActorHandle { /* kernel の Arc<spin::Mutex<ActorHandleState>> */ }

impl ActorHandle {
  pub fn new() -> Self;
  /// Whether the first instance has run `pre_start`.
  pub fn has_started(&self) -> bool;
  /// Whether the actor has stopped for good. Set before any watcher is told.
  pub fn is_terminated(&self) -> bool;
  /// The generation of the instance; it rises at every restart.
  pub fn generation(&self) -> u64;
  /// The highest probe number the actor has processed.
  pub fn processed_sequence(&self) -> u64;
  /// Sends the next probe to `target` and returns its number.
  pub fn send_probe(&self, target: &ActorRef) -> u64;
  /// The snapshot the last processed probe took, cloned, when its type is `T`.
  pub fn read_snapshot<T: Any + Clone>(&self) -> Option<T>;
  /// The receive timeout the actor had when the last probe was processed.
  pub fn receive_timeout(&self) -> Option<ReceiveTimeoutSetting>;
  /// The kind the executor resolved for the actor's dispatcher at creation;
  /// `None` when only a `DispatchSettings` defines the name.
  pub fn dispatcher_kind(&self) -> Option<DispatcherKind>;
}
```

- 中身は `has_started`、`terminated`、`generation`、`next_sequence`、`processed_sequence`、スナップショット（`Option<Box<dyn Any + Send + Sync>>`）、プローブの時点の receive timeout、dispatcher の種類である。スナップショットはハンドルの中に1つだけ持ち、`read_snapshot` はロックの中で `downcast_ref::<T>()` して複製を返す。何度読んでもよい（初版の `Box` を返す形では2回目が読めなかった）。
- ハンドルは `TestActorRef` の生成ごとに1つ作り、実行器がそのアクターのセルに付ける（下の「セルの側」）。再起動で個体が替わってもハンドルは同じであり、番号と世代は戻らない（I6）。

#### `ActorControl`（プローブ、`pub(crate)`）

```rust
// crates/fraktor-actor/src/kernel/actor/test_support/actor_control.rs
/// The numbered probe a read of the underlying actor sends.
pub(crate) struct ActorControl { sequence: u64 }
```

- `pub(crate)` にする。testkit は `ActorHandle::send_probe` だけを使い、型そのものは公開しない（公開の面を増やさないため）。
- 実行器の配送の層（`PoisonPill` と `Kill` を見るのと同じ層、`Executor::deliver` の中）で、振る舞いへ渡す前に横取りする。原典の `TestActorRef` の `ActorCell` が `autoReceiveMessage` で `InternalGetActor` を横取りするのと同じである。振る舞いを `change_behavior` で替えても、`stash` しても、プローブは振る舞いへ届かずに処理される。
- 処理：セルにハンドルが付いていれば、`processed_sequence` をその番号にし、そのときの個体の `underlying_snapshot()` と、実行器が持つそのアクターの receive timeout の設定を、ハンドルへ書く。ハンドルが付いていなければ何もしない。
- 順序：利用者のメッセージと同じ通常のキューに並ぶ。制御を先に出すキュー（control-aware の形）で追い越させない（I4。Quint の負の対照 `bugProbeOvertakes`）。
- receive timeout：原典の `InternalGetActor` は `AutoReceivedMessage` であり、`NotInfluenceReceiveTimeout` ではないので、受信の期限を張り直す。プローブも同じに扱う（実行器の既定の分類のとおり、特別扱いを足さない）。
- 止まったアクター宛て：止まったアクターのメールボックスに残っていたプローブと、止まった後に届くプローブは、dead letters として発行せずに捨てる（I7。Quint の `noProbeLeak`）。テストのための内部のメッセージが `DeadLetter` のイベントとして漏れると、dead letters を見るテストを乱すからである。捨てる判定は、実行器の dead letters の経路でペイロードの型が `ActorControl` であることで行い、feature `test-support` の下だけに置く。

#### `Actor` trait の既定メソッド

```rust
// crates/fraktor-actor/src/kernel/actor/actor_trait.rs（既存の trait に足す）
pub trait Actor: Send {
  // ...既存のメソッド...

  /// The copy of this actor's state that a test reads through
  /// `TestActorRef::underlying_snapshot`. The default has none.
  #[cfg(feature = "test-support")]
  fn underlying_snapshot(&self) -> Option<alloc::boxed::Box<dyn core::any::Any + Send + Sync>> {
    None
  }
}
```

- `Props` は factory を `Arc<dyn Fn() -> Box<dyn Actor>>` に包み、具体型を消している（`props.rs`）。別のトレイトを実装しているかを実行時に見ることも、testkit から factory を包み直すことも、`Props` を受ける口ではできない。trait の既定メソッドなら、どの経路で作った個体でも呼べる。`Fsm` の `attach_handle` と同じく feature `test-support` の下に置くので、feature が切れているときは存在しない。既定の実装は `None` で、既存のアクターは変えなくてよい。
- 実装しないアクター（`None` を返す）の `underlying_snapshot` を読むと、testkit が「このアクターは underlying の複製を出していない」という理由で panic する（§3.3）。

#### セルの側（`ActorCell`、`pub(crate)`）

- `ActorCell` に `#[cfg(feature = "test-support")] handle: Option<ActorHandle>` を足す。生成の口（下の `try_spawn_test_actor`）が付ける。
- `ActorCell::create`（`pre_start` の後）で `has_started` を立てる。`ActorCell::restart`（`post_restart` の後）で世代を1つ増やす。スナップショットは書かない（スナップショットはプローブのときにだけ取る）。
- 停止：実行器がそのアクターを最後に止める経路（セルを取り除き、watcher に `Terminated` を送る前）で、ハンドルの `terminated` を立てる。`WeakActorSystem::on_termination` の登録は使わない（ほかのコールバックや `Terminated` の配信との順序に頼らないため。I7。Quint の `handleBeforeWatchers`）。
- dispatcher の種類：生成の口が、検証を通った Props の dispatcher 名（指定が無ければ既定の名前）を `Dispatchers::kind_of` で解決し、ハンドルに書く。`DispatchSettings` にだけ定義がある名前は `None` になる。

#### 生成の口（`try_spawn_test_actor`）

```rust
// WeakActorSystem（テストのスレッドから）と ActorContext（アクターの処理の中から）の両方に、
// feature `test-support` の下で足す。どちらも Executor の同じ関数1つを呼ぶ。
impl WeakActorSystem {
  pub fn try_spawn_test_actor(&self, supervisor: Option<&ActorRef>, props: Props,
    name: Option<&str>, handle: ActorHandle) -> Result<ActorRef, ActorCreationError>;
}
impl ActorContext<'_> {
  pub fn try_spawn_test_actor(&mut self, supervisor: Option<&ActorRef>, props: Props,
    name: Option<&str>, handle: ActorHandle) -> Result<ActorRef, ActorCreationError>;
}
```

- 検査は `try_spawn_child_from_weak` と同じ順で、同じ実行器の関数の中で行う：名前、システムと親が子を受け付けるか、名前の空き（`/user` の下なら最上位の名前の表も）、設定の検証（`verify_configuration`）。`ActorContext` の側も `ActorContext::actor_of` へ委ねず、この関数を直接呼ぶ。こうして、同じ `TestActorRef` の生成が入口によって検査を変えることを防ぐ（#278 の欠陥を作らない。#247 の未検証の経路に入らない）。
- `supervisor` が `None` なら、親は `/user` の guardian である（原典の `TestActorRef(props, name)` は `system.guardian` を supervisor にする）。アクターの処理の中から呼んでも同じであり、親は作ったアクターにならない（I9）。
- 監督戦略の選び方は §3.4 に書く。
- 失敗の扱い：名前と設定の拒否は `Err`。親がアクターでまだ開始していない場合は、原典の `IllegalStateException("cannot attach a TestActor to an unstarted top-level actor, ensure that it is started by sending a message and observing the reply")` に当たり、何も変えずにこの文言で panic する（原典でも名前や設定の誤りとは別の例外であり、`ActorCreationError` に新しい列挙子は足さない）。
- 生成名：名前が `None` なら既存の生成名の仕組み（`$` と通し番号）に乗る。原典の `randomName`（`$` と base64）の書式そのものは写さない。生成名の書式は原典の利用者が頼れる約束ではないためである。

#### アクターの処理の中からシステムを取る口

```rust
impl ActorContext<'_> {
  /// The system this actor runs in, as a non-owning handle.
  /// Mirrors `ActorContext.system`.
  #[cfg(feature = "test-support")]
  pub fn system(&self) -> WeakActorSystem;
}
```

- アクターの処理の中で作った `TestActorRef`（ケース1、3、5）が、後でテストのスレッドから読まれるときに使うシステムへの弱い参照を持つために要る。既定の公開 API には足さない（`ActorContext` の `system` を公開するかは本件の範囲の外）。

#### 振る舞いを直接呼ぶ口（`receive_directly`）

```rust
impl WeakActorSystem {
  /// Runs `target`'s behaviour once with `message`, outside its mailbox.
  /// Mirrors `TestActorRef.receive`. See section 3.5.
  #[cfg(feature = "test-support")]
  pub fn receive_directly(&self, target: &ActorRef, message: Message, sender: ActorRef) -> RunOutcome;
}
```

- 戻り値は既存の公開型 `RunOutcome`（`Completed` か `Failed { message }`）であり、新しい型は足さない。
- 中身は §3.5 に書く。

#### 親の戦略を登録する口

```rust
impl WeakActorSystem {
  /// Records `strategy` as the one `target` applies to the children it
  /// supervises. For an actor this is what `supervisor_strategy` does at
  /// its start; the test kit uses it for its test actor.
  #[cfg(feature = "test-support")]
  pub fn register_supervisor_strategy(&self, target: &ActorRef, strategy: SupervisorStrategy);
}
```

- 実行器の既存の `register_parent_strategy` を、test actor（function ref）にも使えるようにする口である。testkit の `TestKitBase::new` が、作った test actor に stopping strategy を登録する（§3.4）。

#### ファイルの置き場所（syn チェッカーの `type-per-file` と `module-wiring`）

| ファイル | 中身 |
|:---|:---|
| `crates/fraktor-actor/src/kernel/actor/test_support/actor_handle.rs` | `pub struct ActorHandle` と、その `impl` |
| `crates/fraktor-actor/src/kernel/actor/test_support/actor_control.rs` | `pub(crate) struct ActorControl` |
| `crates/fraktor-actor/src/kernel/actor/test_support.rs` | 既存の `mod` の並びに `mod actor_control;` と `mod actor_handle;` を足し、`pub use actor_handle::ActorHandle;` と `pub(crate) use actor_control::ActorControl;` を足す。このファイル自身には型を置かない |
| `crates/fraktor-actor/src/kernel/actor/actor_trait.rs` | 既存の `Actor` trait に `underlying_snapshot` を足す |
| `crates/fraktor-actor/src/kernel/actor/actor_context.rs` | `try_spawn_test_actor` と `system` を足す |
| `crates/fraktor-actor/src/kernel/actor_system/weak_actor_system.rs` | `try_spawn_test_actor`、`receive_directly`、`register_supervisor_strategy` を足す |
| `crates/fraktor-actor/src/kernel/executor/actor_cell.rs`、`executor_value.rs` | `pub(crate)` の変更だけ（ハンドルの保持、プローブの横取り、停止の印、直接の受信、生成の関数） |

### 3.2 underlying の面：本体を返さず、プローブの時点のスナップショットで表す

原典の `underlyingActor` は本体そのものを返し、ケース8はそのフィールド `s` を読む。移植では本体を返さない（§2 の所有権モデルの行）。

- 読み取り（`TestActorRef::underlying_snapshot::<T>()`）の手順：(1) ハンドルが停止を示していれば、原典の `IllegalActorStateException("underlying actor is terminated")` と同じ文言で panic する。(2) `ActorHandle::send_probe` でプローブを送る。(3) `processed_sequence() >= その番号` か `is_terminated()` が真になるまで、`WeakActorSystem::run_one` を回して待つ。待ちの上限は testkit の既定の timeout（`DefaultTimeout`）であり、超えたら panic する。(4) 停止で終わったら (1) と同じ文言で panic する。(5) `read_snapshot::<T>()` を返す。スナップショットが無い（アクターが `underlying_snapshot` を実装していない）か型が違えば、その理由で panic する。
- この手順は、原典の `underlyingActor` が個体を見られないときに使う `this ? InternalGetActor` と同じく、メールボックスの順序で個体に追いつく形である（§1 の注5）。プローブを処理した個体のスナップショットなので、処理のたびに書き直す必要は無い。再起動の後の読み取りは、新しい個体のスナップショットを返す（I6）。
- `receive_timeout()` と、ケース16〜23が使う記録した `self_ref` の読み取りも、同じ手順で同期する。`dispatcher_kind()` は生成時に決まる値なので、同期しない。
- 同期の待ちは `run_one` を回すので、アクターの処理の中からは呼べない（実行器のロックの中なので `already borrowed` で panic する）。原典ではアクターの中から `underlyingActor` を読める。本 Spec のケースはアクターの中から読まないので、ケースへの影響は無い。この違いは §5 に記録する。
- 再起動の前の個体の状態は保持しない。原典の API 文書は「再起動で underlying の参照は捨てられる。古い個体は死後の確認に使える」と書く。移植で写せるのは「捨てられる」ほうだけである（§5）。

### 3.3 fraktor-testkit に足すもの

#### `TestActorRef`

```rust
// crates/fraktor-testkit/src/test_actor_ref.rs（mod と pub use だけ）
// crates/fraktor-testkit/src/test_actor_ref/test_actor_ref_value.rs
pub struct TestActorRef {
  actor_ref: ActorRef,
  handle: ActorHandle,
  name: String,
  supervisor: ActorRef,
  system: WeakActorSystem,
  dead_letters: ActorRef,
}

impl TestActorRef {
  // factory を受ける4つ。原典の TestActorRef(factory)、TestActorRef(factory, name) と、
  // TestActorRef[T]、TestActorRef[T](name)、TestActorRef[T](supervisor)、
  // TestActorRef[T](supervisor, name)（ClassTag の形は factory で写す。§1 の注9）。
  pub fn new<F, A>(system: &mut ActorSystem, factory: F) -> Self
  where F: Fn() -> A + Send + Sync + 'static, A: Actor + 'static;
  pub fn with_name<F, A>(system: &mut ActorSystem, factory: F, name: &str)
    -> Result<Self, ActorCreationError> where /* 同じ */;
  pub fn with_supervisor<F, A>(system: &mut ActorSystem, factory: F, supervisor: &ActorRef) -> Self
  where /* 同じ */;
  pub fn with_supervisor_and_name<F, A>(system: &mut ActorSystem, factory: F,
    supervisor: &ActorRef, name: &str) -> Result<Self, ActorCreationError> where /* 同じ */;

  // Props を受ける4つ。原典の TestActorRef(props)、(props, name)、(props, supervisor)、
  // (props, supervisor, name)。
  pub fn from_props(system: &mut ActorSystem, props: Props) -> Self;
  pub fn from_props_with_name(system: &mut ActorSystem, props: Props, name: &str)
    -> Result<Self, ActorCreationError>;
  pub fn from_props_with_supervisor(system: &mut ActorSystem, props: Props, supervisor: &ActorRef) -> Self;
  pub fn from_props_with_supervisor_and_name(system: &mut ActorSystem, props: Props,
    supervisor: &ActorRef, name: &str) -> Result<Self, ActorCreationError>;

  // アクターの処理の中から作る口。原典の同じ apply を、アクターの構築の中で
  // 暗黙の context.system を使って呼ぶ形（ケース1、3、5）。
  pub fn in_context(context: &mut ActorContext<'_>, props: Props,
    supervisor: Option<&ActorRef>, name: Option<&str>) -> Result<Self, ActorCreationError>;

  pub fn actor_ref(&self) -> ActorRef;
  /// 原典の `!`。
  pub fn tell<M: Any + Send + Sync>(&self, message: M, sender: ActorRef);
  pub fn name(&self) -> &str;
  pub fn supervisor(&self) -> ActorRef;
  /// 原典の `isTerminated`。
  pub fn is_terminated(&self) -> bool;

  /// 原典の `receive(o)`。sender は dead letters。
  pub fn receive<M: Any + Send + Sync>(&self, message: M);
  /// 原典の `receive(o, sender)`。
  pub fn receive_with_sender<M: Any + Send + Sync>(&self, message: M, sender: ActorRef);

  /// 原典の `underlyingActor` から読むものの複製（§3.2）。
  pub fn underlying_snapshot<T: Any + Clone + Send + Sync>(&self) -> T;
  /// 原典の `underlyingActor.context.receiveTimeout`（§3.2）。
  pub fn receive_timeout(&self) -> ReceiveTimeoutSetting;
  /// 原典の `underlying.dispatcher` の種類（クラスの代わり。§1 の注6）。
  pub fn dispatcher_kind(&self) -> Option<DispatcherKind>;

  /// 原典の `watch`／`unwatch`。包んだアクター自身が監視者になり、`subject` を返す。
  pub fn watch(&self, subject: &ActorRef) -> ActorRef;
  pub fn unwatch(&self, subject: &ActorRef) -> ActorRef;
}

/// 原典の `toString`。`TestActor[<path>]`。
impl core::fmt::Display for TestActorRef { /* ... */ }
```

- 名前を受けない口と supervisor だけを受ける口は、原典で例外になる場面（設定の拒否、親の拒否）で panic する。名前を受ける口は `Err` を返す。原典はどちらも例外であり、`TestFsmRef` と同じ分け方である（`TestFsmRef::new` は panic、`with_name` は `Result`）。`in_context` は、アクターの中での失敗を監督へ渡せるよう、常に `Result` を返す。
- `TestActorRef` は型引数を持たない。スナップショットの型はアクターごとに違い、読み取りの側で `T` を指定する。`Props` を受ける口では個体の型が分からないので、型引数を持たせても運べない（初版の §8 の問いへの答え）。
- `is_terminated` は保持した `WeakActorSystem` の `is_actor_terminated` を呼ぶ。引数にシステムを取らない。
- `watch`／`unwatch` は `self.system.watch(&self.actor_ref, subject)`／`unwatch` を呼び、`subject.clone()` を返す。原典の `underlying.watch(subject)` と同じく、包んだアクターが `Terminated` を受ける（アクターがそれを扱わなければ、原典と同じく `DeathPactException` に当たる既存の扱いになる）。
- 非公開の補助：生成の8つの口と `in_context` は、1つの非公開の関数に寄せる（dispatcher の差し替え、ハンドルの用意、kernel の `try_spawn_test_actor` の呼び出し、supervisor の解決）。テストのスレッドからの口は、その後で開始を待つ（§3.4）。

#### `CALLING_THREAD_DISPATCHER_ID`

```rust
// crates/fraktor-testkit/src/test_actor_ref/calling_thread_dispatcher_id.rs
/// Mirrors `CallingThreadDispatcher.Id`.
pub const CALLING_THREAD_DISPATCHER_ID: &str = "pekko.test.calling-thread-dispatcher";
```

`test_actor_ref.rs` から `pub use calling_thread_dispatcher_id::CALLING_THREAD_DISPATCHER_ID;` で出す。

#### `FraktorSpec` の変更

- `FraktorSpec` が作るシステムの `ActorSystemSetup` に、`Dispatchers::new(vec![DispatcherConfig { id: CALLING_THREAD_DISPATCHER_ID.into(), kind: DispatcherKind::CallingThread }])` を入れる。原典の testkit の `reference.conf` が `pekko.test.calling-thread-dispatcher` を定義するのと同じである。システムは `ActorSystem::with_setup_and_failure_catcher` で作る（今は `with_failure_catcher`）。
- 新しい口 `FraktorSpec::with_dispatchers(system_name: &str, configs: Vec<DispatcherConfig>) -> Self` を足す。`configs` に CallingThread の定義を加えた `Dispatchers` を入れる（`configs` に同じ id があれば、利用者の定義を先に置き、`Dispatchers::has_dispatcher` が先に見つけるほうを使う）。原典の `PekkoSpec("disp1.type=Dispatcher")` に当たる（ケース11）。任意の `ActorSystemSetup` を受ける口は足さない。`Dispatchers` に後から定義を足す口が無く、足すと既定の公開 API が増えるからである。必要になったら別件にする。
- `FraktorSpec` を使わない `ActorSystem::new` のシステムで `TestActorRef` を作ると、CallingThread の名前が未定義なので `ActorCreationError::Configuration` で拒否される。原典で testkit の `reference.conf` が無いときに `ConfigurationException` になるのと同じであり、`TestActorRef` の API 文書に書く。

#### `EventFilter::intercept_any`

```rust
// crates/fraktor-testkit/src/event_filter.rs（既存の impl に足す）
impl EventFilter {
  /// Runs `body` while muting the events this filter matches, without
  /// checking how many there were.
  ///
  /// Mirrors `EventFilter.intercept` with `occurrences = Int.MaxValue`,
  /// the default of `EventFilter[A]()`.
  pub fn intercept_any<R>(&self, system: &mut ActorSystem, body: impl FnOnce() -> R) -> R;
}
```

- 既存の `intercept`（件数1を確かめる）は変えない。既存の移植は件数1で書かれており、既定を変えると既存のテストの強さが変わるためである。`EventFilter` の列挙子に `occurrences` の欄を足す形も考えたが、列挙子の欄の変更は既存の利用を全部書き換えるので採らない。

#### `TestKitBase` の変更

- `TestKitBase::new` が test actor を作った直後に、`WeakActorSystem::register_supervisor_strategy(&test_actor, SupervisorStrategy::stopping_strategy())` を呼ぶ。原典の `TestActor` の `DelegatingSupervisorStrategy` が、登録の無い子に `stoppingStrategy` を使うのと同じである。`child_actor_of` 系は今までどおり戦略を明示して渡し、子は自分の戦略を持つ（原典の `delegates` の登録に当たる）ので、振る舞いは変わらない。

### 3.4 生成の規則

原典の `TestActorRef` のコンストラクタと `apply` の規則をそのまま写す。

1. **dispatcher の差し替え（I2）**：Props に dispatcher の指定が無ければ（`props.dispatcher()` が `None`）、`props.with_dispatcher(CALLING_THREAD_DISPATCHER_ID)` に差し替える。指定があれば差し替えない。`Props::with_dispatcher(DEFAULT_DISPATCHER_ID)` のように既定の名前を明示した場合も指定ありであり、種類は `Default` になる。原典の `_props.deploy.dispatcher == Deploy.NoDispatcherGiven` の判定と同じである。差し替えは testkit 側で行う（kernel は TestActorRef の規則を知らない）。
2. **supervisor の解決**：`supervisor` を省いた口は、kernel の生成の口に `None` を渡し、親は `/user` の guardian になる。`TestActorRef::supervisor()` は `resolve_actor_ref` で解決した guardian の参照を返す。
3. **監督戦略（ケース5、14）**：kernel の `try_spawn_test_actor` が、親の種類で選ぶ。
   - 親が `/user` の guardian（`supervisor` が `None` の場合を含む）：既定の戦略を子が持つ。`ActorSystem::actor_of` と同じであり、原典の user guardian の既定の戦略に当たる。
   - 親が開始したアクター：子は親の戦略を引き継ぐ（`strategy_inherited = true`）。失敗した時点の親の `supervisor_strategy()` が決める。ケース5の `boss` が `OneForOneStrategy(5, 1.second)(List(classOf[ActorKilledException]))` を返せば、`Kill` された子は再起動する。原典で `TestActorRef(..., self, "child")` の子の失敗を `boss` の `supervisorStrategy` が扱うのと同じである。
   - 親が開始していないアクター：§3.1 のとおり panic する。
   - 親が親の戦略を登録した function ref（testkit の test actor）：その戦略を引き継ぐ。test actor は stopping strategy を登録しているので（§3.3）、ケース14の子は止まる。
   - 親が戦略を登録していない function ref：既定の戦略を子が持つ。原典では「unknown type of supervisor」というエラーのログを出して続ける場面であり、移植はログを出さない（§5）。
4. **開始の確認**：テストのスレッドからの8つの口は、生成の後で `ActorHandle::has_started()` か停止になるまで `WeakActorSystem::run_one` を回し、止まっていたら「the actor at <path> failed to start」で panic する。原典では CallingThreadDispatcher のため、構築の失敗がその場で例外になる。#181 の `TestFsmRef::start` と同じ契約である。`in_context` は待たない（アクターの処理の中では実行器を回せない）。入れ子の開始は、外側のアクターの処理が終わった後の子のメールボックスの `Create` で走り、失敗は監督が扱う。`ActorContext::actor_of` と同じであり、違いは §5 に記録する。
5. **名前**：名前の検証と一意性は kernel の生成の口に委ね、testkit 側で独自の検証は足さない。

### 3.5 `receive`：振る舞いを1回直接呼ぶ

`TestActorRef::receive(o)` と `receive_with_sender(o, sender)` は、`WeakActorSystem::receive_directly(&actor_ref, Message::new(o), sender)` を呼ぶ。sender を省いた形は dead letters の参照を渡す（原典の `underlying.system.deadLetters`）。`WeakActorSystem` には dead letters を返す口が無いので、`TestActorRef` は生成のときに `ActorSystem::dead_letters`（アクターの中からは `ActorContext::dead_letters`）で取った参照を持っておく。

kernel の `receive_directly` の中身：

1. 対象が止まっていれば、`"underlying actor is terminated"` で panic する（原典ではセルの `actor` が無く、呼び出しが例外になる）。
2. 対象のメールボックスを、実行器の通常の実行と同じ予約で取る（同じメールボックスを2つの実行が同時に回さない）。セルを取り出し、`ActorContext` に sender を入れて、振る舞いの `receive` を1回呼ぶ（`run_behavior`）。メールボックスのキューは触らない。`PoisonPill`、`Kill`、プローブの横取りも通さない（原典の `receive` は `autoReceiveMessage` を通さず、`receiveMessage` を直接呼ぶ）。受信の期限も張り直さない（原典の `receive` は `ActorCell.invoke` を通らない）。
3. 振る舞いの失敗は失敗の捕まえ手（`FailureCatcher`）で受け、**監督へは渡さず**、`RunOutcome::Failed { message }` として返す。原典の `receive` で投げられた例外は呼び出し側へ届き、アクターは再起動しない。testkit の `receive` は、`Failed` を受け取ったら、実行器のロックを離した後でその文言で panic する（呼び出し側へ失敗が届く）。捕まえ手を持たないシステムでは、panic は既存の扱い（実行器の外へ巻き戻り、次の呼び出しでシステムが終わる）になる。`FraktorSpec` のシステムは捕まえ手を持つ。
4. セルをメールボックスへ戻す。対象の dispatcher の種類が `CallingThread` なら、この呼び出しが積んだ対象自身のシステムメッセージ（`context.stop(self)` の `Terminate` など）を、返る前に処理する（メールボックスの `process_all_system_messages`）。原典の CallingThreadDispatcher が、`systemDispatch` で対象のメールボックスを送り手のスレッドでその場で処理するのと同じである。ほかのアクターのメールボックスは回さない。種類が `CallingThread` でなければ、システムメッセージは対象の通常の実行を待つ（原典の別の dispatcher と同じ）。
5. 子を持つアクターの停止は、子の終了を待つので、4. の後もすぐには終わらない。原典でも子が別の dispatcher なら同じである（§5）。

アクターの処理の中から `receive` を呼ぶと、実行器のロックの中なので `already borrowed` で panic する（§5）。

### 3.6 足す `pub` の一覧（既存の `pub` との重なりの確認）

| クレート | 足す `pub` | 既存との重なり |
|:---|:---|:---|
| fraktor-actor（`test-support` の下だけ） | `kernel::actor::test_support::ActorHandle` とそのメソッド | 無い。`FsmHandle` は FSM 専用で型引数を持つ |
| 同上 | `Actor::underlying_snapshot` | 無い |
| 同上 | `ActorContext::try_spawn_test_actor`、`ActorContext::system` | `ActorContext::actor_of` は設定を検証せず戦略の選び方も違う。`system` は無い |
| 同上 | `WeakActorSystem::try_spawn_test_actor` | `try_spawn_child` は渡した戦略を子が持つ形であり、親の戦略を引き継げない。`try_spawn_child` は変えない |
| 同上 | `WeakActorSystem::receive_directly`、`WeakActorSystem::register_supervisor_strategy` | 無い |
| fraktor-testkit | `test_actor_ref::TestActorRef` とそのメソッド、`Display` | 無い |
| 同上 | `test_actor_ref::CALLING_THREAD_DISPATCHER_ID` | 無い |
| 同上 | `FraktorSpec::with_dispatchers` | 無い（`with_hooks`、`with_time_factor` とは別の入力） |
| 同上 | `EventFilter::intercept_any` | `intercept` は件数1を確かめる。別の意味の口である |

既定の公開 API（feature を切った fraktor-actor）には何も足さない。

### 3.7 `TestFsmRef` との関係

本件では `TestFsmRef` を変えない。原典では `TestFSMRef` が `TestActorRef` を継承するので、`TestFsmRef` も本件の生成の規則（supervisor の戦略、CallingThread の差し替え）に従うのが原典どおりである。今の `TestFsmRef::with_supervisor` は `try_spawn_child` に既定の戦略を渡し、dispatcher も差し替えない。これを直すと #181 で入った `test_fsm_ref_spec` の前提が変わるので、本件には混ぜず、別件として報告する（§9）。`TestFsmRef` を `TestActorRef` の kernel の口の上に組み直すのも、その別件で扱う。

### 3.8 no_std で書けるか

- fraktor-actor 側の追加（`ActorHandle`、`ActorControl`、`underlying_snapshot`、生成の口、直接の受信）は `core` と `alloc` と、kernel がすでに使う `spin::Mutex` と kernel の `Arc` だけで書ける。`std` の import は足さない。時刻は使わない（待ちは testkit 側にある）。
- スレッドの同一性（§1 の共有の状態）は、テスト側だけが使う。
- fraktor-testkit はもともと `std` のクレートである。

## 4. 守る性質（不変条件）

| 番号 | 不変条件 |
|:---:|:---|
| I1 | 生成の所属：`TestActorRef` で作ったアクターは、指定した supervisor の子として、指定した名前（指定が無ければ生成名）で作られる。`actor_ref().path().parent()` は supervisor のパスと等しく、そのパスを解決した参照は supervisor と等しい。supervisor を省いた生成の親は `/user` の guardian である |
| I2 | dispatcher の選択：Props に dispatcher の指定が無ければ、生成したアクターの dispatcher の名前は `CALLING_THREAD_DISPATCHER_ID` になり、`FraktorSpec` のシステムでは種類が `CallingThread` に解決される。指定があれば（既定の名前の明示を含む）その指定が勝ち、差し替えない。未定義の名前は `ActorCreationError::Configuration` で拒否され、既存のアクターに影響しない |
| I3 | receive timeout：`TestActorRef` は receive timeout の設定を触らない。生成した直後のアクターの設定は `Undefined` である。アクターが自分で張った期限（ケース7）は、通常のアクターと同じに発火する |
| I4 | プローブの順序：プローブは利用者のメッセージと同じキューに並び、追い越さない。処理済みの番号は単調に増え、送った最後の番号を超えない。番号 n のプローブが処理された時点で、それより前にそのアクターへ送った利用者のメッセージはすべて処理済みである |
| I5 | 読み取りの一致：`underlying_snapshot` が返す値は、そのプローブを処理した個体の、プローブの時点の `underlying_snapshot()` の値である |
| I6 | 再起動：再起動で個体が替わったら、その後の読み取りは新しい個体の値を返す。世代は再起動のたびに増え、処理済みの番号は再起動をまたいで戻らない |
| I7 | 停止：アクターが最後に止まると、ハンドルの停止の印が、どの watcher が `Terminated` を受けるよりも先に立つ。止まった後の読み取りは `"underlying actor is terminated"` で panic し、配送中のプローブを待ち続けない。止まったアクターのプローブは dead letters として発行されない。`is_terminated` は停止の後で真になる |
| I8 | 名前の拒否：使用中の名前、空の名前、不正な名前での生成は `Err`（名前を受けない口では panic）になり、既存のアクターには影響しない。止まったアクターが持っていた名前は、既存の生成の経路の意味論どおりに再利用できる |
| I9 | 入れ子の生成：アクターの処理の中から `TestActorRef::in_context` で作った子の親は、supervisor を省けば `/user`、指定すればその supervisor である。`ActorContext::actor_of` で作った子の親は作ったアクターである。どちらも作った側と別の参照を持つ |
| I10 | 監督：supervisor を指定して作った子の失敗は、supervisor の戦略が決める。アクターの supervisor なら失敗した時点のその `supervisor_strategy()`、test actor なら stopping strategy、guardian なら既定の戦略である |
| I11 | 直接の受信：`receive` は振る舞いを1回だけ、メールボックスのキューを触らずに呼ぶ。振る舞いの失敗は呼び出し側へ panic として届き、監督へは渡らない。dispatcher が `CallingThread` の種類なら、その呼び出しが積んだ対象自身のシステムメッセージは返る前に処理されている（自分を止めたアクターは、返った時点で `is_terminated` が真） |
| I12 | ハンドル無しのアクターの不変：ハンドルを付けない通常の生成の振る舞いは、既存のテストが見る範囲で変わらない。feature `test-support` が切れているときは、testkit 向けの面は存在しない |

## 5. 壊しうる入力と、原典と違える点

| 入力 | 何が起きうるか | 扱い |
|:---|:---|:---|
| 空文字のアクター名、使用中の名前、ワイルドカードを含む名前、非常に長い名前 | 名前の検証と一意性は kernel の生成の口の仕事である | その経路に委ねる。`Err`（名前を受けない口は panic） |
| 止まった supervisor | 親が子を受け付けない | `try_spawn_test_actor` が `Err`（`terminating`）を返す。名前を受けない口は panic |
| 開始していないアクターの supervisor | 原典は `IllegalStateException`（"cannot attach a TestActor to an unstarted top-level actor ..."） | 同じ文言で panic する（§3.1） |
| 戦略を登録していない function ref の supervisor | 原典はエラーのログを出して続ける | 既定の戦略で作り、ログは出さない。原典でも「this is not going to end well」と警告する場面であり、利用者が頼る振る舞いではない |
| `FraktorSpec` を使わないシステムでの生成 | CallingThread の名前が未定義 | `ActorCreationError::Configuration`。原典で testkit の `reference.conf` が無いときと同じ |
| Props に未定義の dispatcher 名 | 設定の検証で拒否 | `Err`（I2）。#247 の未検証の経路は通らない |
| factory が panic する、または `pre_start` で失敗するアクター | 開始が失敗する | テストのスレッドの口は開始を待ち、止まっていたら panic する（§3.4）。`in_context` は待たず、失敗は監督が扱う（原典では構築の失敗がその場で例外になる。下の行） |
| アクターの処理の中で作った `TestActorRef`（`in_context`） | 原典は CallingThreadDispatcher のため、入れ子の構築がその場で終わり、失敗はその場で例外になる | 入れ子は外側の処理の後で開始する。本 Spec のケース（1、3、5）は入れ子の開始の失敗を見ないので、ケースへの影響は無い。同期で開始する形は段7b の CallingThreadDispatcher の後に戻す |
| アクターの処理の中から `underlying_snapshot`、`receive`、`receive_timeout` を呼ぶ | 実行器のロックの中で実行器を回す | `already borrowed` で panic する。原典ではアクターの中から呼べる。本 Spec のケースはアクターの中から呼ばない |
| `receive` で panic するメッセージを入れる | 原典では例外が呼び出し側へ届き、アクターは再起動しない | 同じに写す（I11）。文言は捕まえ手が受けた panic の文言である |
| `receive` で `PoisonPill` や `Kill` の文字列を入れる | 原典の `receive` は `autoReceiveMessage` を通らないので、振る舞いの `receive` に普通のメッセージとして届く | 同じに写す（§3.5 の 2.） |
| 止まったアクターへの `receive`、`underlying_snapshot` | 原典は例外 | `"underlying actor is terminated"` で panic する |
| 止まったアクターへの `tell` | dead letters へ行く | 既存の意味論のまま。例外にしない |
| 読み取りの待ちの間にアクターが止まる（プローブが配送中） | プローブが処理されない | 停止の印で待ちを終え、panic する。待ち続けない（I7、Quint の `readNeverStuck`） |
| 読み取りの待ちの間にアクターが再起動する | プローブは新しい個体が処理する | 新しい個体の値を返す（I6） |
| `change_behavior` や `stash` をするアクター | プローブが振る舞いに渡ると、横取りが効かない | 配送の層で振る舞いの前に横取りする（§3.1） |
| receive timeout を張ったアクターを読む | プローブが期限を張り直す | 原典の `InternalGetActor` と同じに扱う（§3.1）。本 Spec のケース7は読まない |
| CallingThread でない dispatcher のアクターを読む | 原典の読み取りは同期しないことがある | 移植は常に同期する。原典が許す範囲の中の狭めであり、利用者への約束としては書かない（§1 の注5） |
| CallingThread でない dispatcher のアクターへ `receive` して自分を止める | 原典では止まるのは対象の dispatcher の実行の後 | 同じに写す（§3.5 の 4.） |
| 子を持つアクターへ `receive` して自分を止める | 子の終了を待つ | 返った時点ではまだ止まっていないことがある。原典でも子が別の dispatcher なら同じ |
| 再起動の前の個体を読む | 原典は古い個体の参照を保持できる | 移植は保持しない。死後の確認に古い個体を使うテストは書けない |
| 同じアクターに複数のハンドルを付ける | スナップショットが食い違う | 1つの生成に1つのハンドルだけを付ける。ハンドルはセルに付くので、再起動で factory が呼ばれても同じハンドルのままである |
| 処理が止まらないアクターを読む | 待ちが続く | 待ちの上限は testkit の既定の timeout。番号は `u64` で、現実の件数では回らない |
| テストの並列実行 | 共有の状態が競合する | §1 の共有の状態のとおり、テスト関数ごとにローカルに作る |

## 6. PBT と Quint

### PBT は足す

原典外の性質テストを `crates/fraktor-testkit/tests/test_actor_ref_property_test.rs` に足す。依存は足さず、既存の移植（#177、#181）と同じく固定 seed の SplitMix64 で抽選する（`proptest` の採否は [Issue #212](https://github.com/j5ik2o/fraktor-ng-rs/issues/212) の判断に従う）。

| 番号 | 性質 | 確かめる不変条件 |
|:---:|:---|:---|
| P1 | ランダムな生成の組み合わせ（supervisor の有無と種類：省略、明示した `/user` の guardian、開始したアクター、test actor、止まったアクター。名前の有無：空文字、長い文字列、重複、ワイルドカード文字、止まったアクターの名前の再利用）について、成功した生成は I1 のパス条件と参照の条件を満たし、拒否された生成は既存のアクターのパスと状態を変えない | I1、I8、I9 |
| P2 | ランダムなメッセージ列（内部状態を数えるアクターへ、文字列と数値の列。途中で `change_behavior` と `stash`／`unstash_all` を挟む）を送り、`underlying_snapshot` が、同じ列を単純なモデルで追った内部状態と等しい。途中の状態が最後と同じ値になる列（行って戻る列）を含める | I4、I5 |
| P3 | ランダムな再起動の列（`Kill` を挟む）について、世代が単調に増え、再起動の後の読み取りが新しい個体の開始状態から追った値になり、処理済みの番号が戻らない | I6、I4 |
| P4 | dispatcher の指定の抽選（指定なし、`DEFAULT_DISPATCHER_ID` の明示、`with_dispatchers` で登録した名前、未定義の名前）について、解決した種類が I2 の規則どおりになり、未定義の名前は `ActorCreationError::Configuration` で拒否される | I2 |
| P5 | ランダムな時点で止めたアクター（処理の途中、プローブの配送中、再起動の後）について、その後の読み取りは `"underlying actor is terminated"` で panic し、`is_terminated` が真であり、dead letters のイベントにプローブが現れない | I7 |
| P6 | ランダムなメッセージ列を `receive` で直接入れたとき、メールボックスに積んであったメッセージの数と順序が変わらず、自分を止めるメッセージの後は（CallingThread の種類のとき）`is_terminated` が真である | I11 |

### Quint は書いた

判断：書く。`TestActorRef` の中核は状態遷移である。個体は「開始前→稼働→（再起動で世代が変わる）→停止」と移り、プローブの処理済み番号、読み取りの結果、停止の印は、その遷移と送信と読み取りが交差する列の全体で成り立つ必要がある。既存の仕様（`crates/fraktor-actor/specs/test_fsm_ref.qnt` は FSM のタイマと状態、`crates/fraktor-testkit/specs/watch_handle.qnt` は watch のハンドル）は、汎用のアクターの個体とプローブを捉えていない。

置き場所は、状態の実体（`ActorHandle`、`ActorCell`）がある fraktor-actor の `crates/fraktor-actor/specs/test_actor_ref.qnt` にした（`test_fsm_ref.qnt` と並べる）。CI の決まり（`docs/design/quint-in-ci.md`）に合わせ、不変条件は `val invariant` にまとめ、`run` の名前は `Test` で終え、負の対照は `run bug…Test` で `.expect(not(...))` を期待する。`bash scripts/quint-check-specs.sh` で、typecheck、13件の `run`、`invariant` の模擬（20手、10000トレース）が通ることを確かめた。

- 状態：dispatcher が CallingThread か（`ctd`、実行ごとに固定）、段階（開始前、稼働、停止）、世代、今の個体の抽象の状態、メールボックス（利用者のメッセージとプローブの列）、積まれた停止、ハンドル（処理済み番号、最後に送った番号、スナップショットとその世代、停止の印）、watcher への通知、読み手（待っているか、待つ番号）
- 遷移：`start`、`startFails`、`sendUser`、`startRead`（プローブを送る）、`deliverHead`（先頭を配送する。プローブならスナップショットを取る）、`requestStop` と `processStop`（積まれた停止の処理。プローブを捨てる）、`restart`、`restartFails`（`post_restart` の失敗で止まる）、`notifyWatchers`、`directReceive`（振る舞いの直接の呼び出し。CallingThread なら自分の停止をその場で処理する）、`finishRead`（読み取りが返る。返れる状態になったら、ほかの遷移より先に起きる）
- 不変条件：`sequenceMonotonic`（I4）、`probesKeepOrder`（I4）、`readsSeeCurrent`（I5、I6）、`readNeverStuck`（I7。待っている読み手は、プローブがまだ処理されうるか、処理済みか、停止の印がある）、`noProbeLeak`（I7）、`handleBeforeWatchers`（I7）、`directStopSettles`（I11）
- 負の対照：`bugProbeToDeadLetters`（停止でプローブを dead letters に出す）、`bugStopWithoutMark`（停止の印を立てない。配送中のプローブの待ちが終わらない）、`bugReadWithoutProbe`（プローブを送らずに古い番号で読む。再起動の後に古い個体を返す）、`bugSequenceRewind`（再起動で番号を戻す）、`bugProbeOvertakes`（プローブが利用者のメッセージを追い越す）、`bugNotifyBeforeHandle`（印を立てる前に watcher へ知らせる）、`bugDirectSkipsSystemRun`（直接の受信で積んだ停止を処理しない）。それぞれ狙った不変条件が落ちることを `run` で確かめた
- 正の `run`：読み取りが処理済みのメッセージを見る、開始前に送ったプローブは開始を待つ、再起動の後の読み取りが新しい個体を見て番号が続く、配送中のプローブがある停止で読み取りが終わる、再起動の失敗で止まる、直接の受信での停止（CallingThread なら止まり、そうでなければ積まれたまま）

モデルにしないもの：実時間、ほかのアクター、メッセージの中身（抽象の状態の数としてだけ扱う）、監督の方針の選択（再起動するか止めるかは遷移として与える）、子を持つアクターの停止の待ち。

## 7. 段7b が前提のケースと、台帳の扱い

### 7.1 切り分け

| ケース | 前提 | 扱い |
|:---:|:---|:---|
| 3（support reply via sender()） | 段7b（#259）の CallingThreadDispatcher。4回の `!` の直後の `counter == 0` と `assertThread()` は、クライアント、サーバー、ワーカーの3つのアクターの処理が送り手のスレッドで同期して進むことそのもの | unported のまま残す。弱めた版を書かない。数からは外さない |
| 5（restart when Kill:ed） | 同上。`boss ! "sendKill"` の直後の `counter == 0`（子の `preRestart` と `postRestart`）と `assertThread()` は、`boss` と子の処理と監督の判断が同期して進むこと | 同上。監督戦略の引き継ぎ（§3.4）と `in_context` は本件で用意するので、段7b の後は dispatcher の差し替えだけで書ける |
| 6（support futures） | 同上。`f.isCompleted` が ask の直後に真であること（"CallingThreadDispatcher means that there is no delay"）が本体である | 同上 |
| そのほかの20件 | 段7b を前提にしない。読み取り（ケース8、9、16〜23）は §3.2 のとおり対象のアクター1つのメールボックスで同期し、`receive`（ケース12、13）は §3.5 のとおり対象自身のシステムメッセージだけを処理する。ケース4、7、13〜15は原典自身が `expectMsg` 系で待つ | 第2段で green にする |

ケース4の `assertThread()` は、§1 の注3のとおり原典でも書かれない値を見るので、そのまま写す。ケース10は種類だけを見るので、段7b を待たない（§1 の注6）。

### 7.2 台帳の扱い

[Issue #200](https://github.com/j5ik2o/fraktor-ng-rs/issues/200) の `actor-tests:ReceiveTimeoutSpec`（15/14/14、`status: red`）と `actor-tests:ExtensionSpec`（8/5/5、`status: red`）の前例どおりにする。第2段の終わりで、対象ノードを次のとおり記録する。

- `status: red`、`cases.total: 23`、`ported: 20`、`passing: 20`、`excluded: []`
- `triage.kind: pending`、`triage.reason` に「ケース3、5、6（送ったその場で処理が終わることそのものを見る）は #234 の段7b（#259）の CallingThreadDispatcher に依存するため後続の issue へ分け、数からは外さない。弱めた版は書かない」と、決めた人と日付を書く（決めた人は §8 の伺いの答えによる）
- `rust.crate: fraktor-testkit`、`rust.test_file: crates/fraktor-testkit/tests/test_actor_ref_spec.rs`
- `needs.testkit` は今のままで足りる。`calling-thread-dispatcher` は、本件では名前と種類の登録だけを用意し、実行は段7b である（§9）
- `ledger/capabilities.yaml` の `test-actor-ref` の `provided_by` は、#181 の `test-fsm-ref`（`null` のまま）の扱いにそろえる。本件で新しい決め方は作らない
- 状態遷移の規約（unported → red → green）に合う。ケース3、5、6 を移す後続の issue は、段7b（#259）に依存させて作る（作るのは上司）

issue の文面の「台帳の対象ノードを green にする」は、この形では満たせない。#200 と同じく、ノードを red のまま #182 を完了とするかどうかを §8 の伺い2で上げる。

## 8. 伺うこと

この文書では決めない。決めるのは上司である。どれも、承認が台帳に記録されるまでは未承認として扱う（移植の規約どおり、`cases.excluded` と `cases.not_built` への記入は人間だけが行う）。下の3件は「除外」ではなく「段7b の後に原典どおりの形で移す」ための保留であり、`cases.excluded`／`cases.not_built` には書かない見込みである。

### 伺い1：ケース3、5、6 を、弱めた版を書かずに段7b の後へ回すこと

| ケース | 原典の行 | 今作れない理由 | Rust で代わりに確かめること（あれば） |
|:---:|:---|:---|:---|
| 3 | `TestActorRefSpec.scala:156-179`。特に `:162-165` の4回の `clientRef ! ...` の直後の `:167` `counter should ===(0)`、`:171-174` の4回の後の `:176` の2回目、`:178` の `assertThread()` | 今の実行器は送信をキューに入れ、運転するまで処理しない。待ちなしで `counter == 0` を見る形は、CallingThreadDispatcher（段7b、#259）が無いと書けない。待ちを入れると即時性と送り手のスレッドでの処理を外した弱いテストになる。送信のたびに実行器を空になるまで回す形は、今の1スレッドの実行器の作りに頼る強さであり、`docs/decisions/pekko-semantics-first.md` が直すと決めた作りのずれに当たる | 本件の範囲では、返信の数え（`counter` が4から0になること）を `FraktorSpec::await_cond` で待って確かめることはできる。ただしこれは弱めた版なので、台帳の `passing` には数えない。書く場合も、ケースのテストではなく `cases_outside_test_file` の補助の確認として置く案になる（書くかどうかも伺う） |
| 5 | `TestActorRefSpec.scala:200-222`。特に `:217` `boss ! "sendKill"` の直後の `:219` `counter should ===(0)` と `:220` `assertThread()` | 同上。`boss` の処理、子への `Kill`、`boss` の `OneForOneStrategy` による判断、子の `preRestart` と `postRestart` が送ったその場で終わることを見る | 監督戦略の引き継ぎ（`boss` の `OneForOneStrategy` で子が再起動すること）は、本件の `in_context` と §3.4 で書ける。`await_cond` で `counter == 0` を待つ版は弱めた版なので、ケース3と同じく `passing` に数えない |
| 6 | `TestActorRefSpec.scala:224-230`。特に `:227` の注釈 "CallingThreadDispatcher means that there is no delay" と `:228` `f.isCompleted should be(true)` | ask の直後に future が完了していることそのものが本体であり、段7b が無いと作れない | `:229` の `Await.result(f, timeout.duration) should ===("workDone")` は `AskFuture::await_result` で書けるが、本体（即時性）を外した版なので、ケースとしては書かない |

推奨（参考）：3件とも unported のまま残し、段7b（#259）に依存する後続の issue で、原典どおりの形（待ちなし）で移す。#200 のケース9と同じ形である。

### 伺い2：ノードを red（23/20/20）のまま #182 を完了とすること

- 原典の行：issue #182 の完了の条件「対象ノードの台帳 `status` がすべて `green` になる」
- 満たせない理由：伺い1の3件が段7b に依存するため
- 代わりに記録すること：§7.2 の台帳の書き方（`status: red`、23/20/20、`triage.reason`）。#200（中将の決定、2026-11-27）と `ExtensionSpec` の前例と同じ形

### 伺い3：`TestFsmRef` の監督戦略と dispatcher のずれを別件にすること

- 原典の行：`TestFSMRef.scala:44-46` の `class TestFSMRef ... extends TestActorRef[T](system, props, supervisor, name)`（`TestActorRef` のコンストラクタの規則をそのまま使う）
- 本件で直さない理由：`TestFsmRef::with_supervisor` は `try_spawn_child` に既定の戦略を渡し、dispatcher を差し替えない（§3.7）。直すと #181 で入った `test_fsm_ref_spec` の前提が変わり、本件の範囲が #181 のノードへ広がる
- 代わりに確かめること：本件の `test_fsm_ref_spec` は今のまま green であることを第2段の検査で確かめる。別件では、`TestFsmRef` を本件の `try_spawn_test_actor` の上に組み直し、TestFSMRefSpec の supervisor 指定のケースが原典どおりの監督で動くことを確かめる
- これはケースを弱める伺いではない（TestActorRefSpec の23件には影響しない）。範囲の線引きの確認である

## 9. 影響

- **公開 API**：§3.6 の一覧のとおり。既定の公開 API（feature を切った fraktor-actor）には何も足さない。fraktor-testkit は `TestActorRef`、`CALLING_THREAD_DISPATCHER_ID`、`FraktorSpec::with_dispatchers`、`EventFilter::intercept_any` が増える。既存の公開 API の変更と削除は無い。
- **`FraktorSpec` の振る舞い**：システムの `ActorSystemSetup` に `Dispatchers`（CallingThread の定義）が入る。`system.setup().get::<Dispatchers>()` が `Some` を返すようになる。既存のテストがこれに頼っていないことを、第2段の `cargo test --workspace` で確かめる。
- **test actor の戦略の登録**：`TestKitBase::new` が test actor に stopping strategy を登録する。`child_actor_of` 系は戦略を明示するので変わらない。test actor の下に戦略を明示せずに子を作る既存の口は無い（`WeakActorSystem::spawn_child` は戦略を必ず受ける）。既存の testkit のテストが green のままであることで確かめる。
- **段7b（#259）との関係**：`receive_directly` の「CallingThread の種類なら対象自身のシステムメッセージを返る前に処理する」は、段7b の CallingThreadDispatcher が入れば、その dispatcher の `systemDispatch` に置き換わる。プローブの同期（§3.2）も、段7b の後は CallingThread の種類のアクターについては不要になる（残しても I5 は変わらない）。この置き換えを #259 の設計に書いてもらう必要がある（上司へ伝える）。ケース3、5、6 は段7b の後に移す。
- **台帳の `calling-thread-dispatcher`**：`ledger/capabilities.yaml` は `provided_by: G0c` としているが、実行する CallingThreadDispatcher は入っていない（`DispatcherKind::CallingThread` の列挙子だけ）。本件の範囲の外の食い違いとして報告する。
- **`TestFsmRef`**：変えない。§8 の伺い3。
- **新しいコードファイルのヘッダー**：第2段で足す `crates/**/*.rs` の新しいファイルには Apache-2.0 のヘッダーを付け、Pekko の移植であるファイルには原典の指定を入れる。設計の Markdown には付けない（既存の設計文書の形）。
- **保存済みのデータと上流**：影響は無い。
- **守りの仕組み**：CI の必須チェックは変えない。新しい Quint の仕様は既存の `quint` ジョブが拾う。

## 10. 第2段の進め方（この設計が通ってから）

1. red：テストの担当が `crates/fraktor-testkit/tests/test_actor_ref_spec.rs` に20件を書く。ケース3、5、6 は書かず、冒頭の注釈に段7b の後に移すことをケース名つきで書く。`#[ignore]` の red のテストは置かない。台帳の対象ノードを red にする。失敗が「型や口が無い」ことによることを確かめる。
2. 本体：§3 の順に、fraktor-actor の `test-support` の面（`ActorHandle`、`ActorControl` の横取りと捨て方、`underlying_snapshot`、セルの側、`try_spawn_test_actor`、`ActorContext::system`、`receive_directly`、`register_supervisor_strategy`）、fraktor-testkit の `TestActorRef`、`CALLING_THREAD_DISPATCHER_ID`、`FraktorSpec` の変更、`EventFilter::intercept_any`、`TestKitBase` の登録を足す。§3.6 の一覧に無い `pub` を足す前に、同じ働きの既存の関数を探し、並行の別の PR と同じ口を足していないかも確かめる。レビューの経過をコードのコメントに書かない。理由は設計の文書と PR に書く。
3. green：20件を通し、§6 の PBT を足す。Quint の仕様が実装と食い違ったら、仕様か実装のどちらを直すかを設計へ戻して決める。
4. 台帳：§7.2 のとおり記録し、`docs/baseline/v2.0.0-undocumented-nodes.tsv` をほかの移植の前例に合わせて作り直す。
5. 検査：CI と同じ検査を流す（`cargo fmt --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`、ledger/tools の Python テスト、`bash scripts/quint-check-specs.sh`、syn チェッカー、`python3 scripts/check_src_hygiene.py`）。加えて、dev-dependency を引かない `cargo clippy -p fraktor-actor --lib -- -D warnings` を流し、feature を切った形で testkit 向けの面が既定の公開 API に出ていないことを確かめる（#181 の前例）。既存の `test_fsm_ref_spec` と testkit のテストが green のままであることを確かめる（§9）。

## 11. 設計の独立レビューへの回答表

| 指摘 | 重さ | 対応 | 中身 |
|:---:|:---:|:---:|:---|
| H1 | 高 | 直した | アクターの中から作る口 `TestActorRef::in_context` と、kernel の `ActorContext::try_spawn_test_actor`、`ActorContext::system` を足した。テストのスレッドの口と同じ実行器の関数を通り、`ActorContext::actor_of` へは委ねない（§3.1、§3.3）。I9 を「supervisor を省けば `/user`、指定すればその supervisor、`context.actor_of` は作ったアクター」に直した |
| H2 | 高 | 直した | 既定の戦略を固定で渡す形をやめた。親がアクターなら戦略を引き継ぎ、test actor は stopping strategy を登録し、guardian は既定の戦略にした（§3.4、I10）。`try_spawn_child` は変えず、別の口 `try_spawn_test_actor` にした |
| H3 | 高 | 直した（形は一部変えた） | 別トレイトをやめ、`Actor` に `test-support` の既定メソッド `underlying_snapshot` を足した。ハンドルは複製で読ませるので何度でも読める。書く時点は「プローブを処理したとき」の1か所にした（処理のたび・`post_restart` の後に書く形をやめ、原典の `InternalGetActor` に寄せた）。再起動の後は新しい個体がプローブを処理するので、未更新の穴は無い（§3.2、I5、I6） |
| H4 | 高 | 直した | `TestActorRef` が `WeakActorSystem` を持つ。`receive` は振る舞いを直接呼ぶ口 `receive_directly` で写し、番兵を使わない。CallingThread の種類なら対象自身のシステムメッセージを返る前に処理するので、ケース12、13 の自分を止めるアクターでも待ちは要らない。読み取りの待ちは「処理済み、または停止」で終わり、止まったアクターのプローブは dead letters に出さない（§3.2、§3.5、I7、I11）。`is_terminated` は引数を取らない |
| H5 | 高 | 直した | `Dispatchers`／`DispatcherConfig` に直した。`FraktorSpec` が `pekko.test.calling-thread-dispatcher`（`CallingThread`）を既定で登録し、`FraktorSpec::with_dispatchers` を足した。`FraktorSpec` を使わないシステムでは `ActorCreationError::Configuration` になることと、既定の名前の明示は差し替えないことを I2 に書いた。解決した種類は生成時にハンドルへ記録する（§3.1、§3.3、§3.4） |
| H6 | 高 | 直した | 提案2を取り下げた。ケース14、15は既存の `EventFilter::typed(...).with_message(...).intercept(...)`（件数1）、ケース4は新しい `intercept_any`（`occurrences = Int.MaxValue`）で写す（§1 の注3、注8、§3.3） |
| H7 | 高 | 直した | 弱めた版を green に数える形をやめた。ケース3、5、6 は unported のまま残し、ノードは `status: red`（23/20/20）で管理する（§7）。ケース4に `assertThread()` を入れた（注3）。ケース10は分けない（注6）。扱いは §8 で伺う |
| M1 | 中 | 直した | `watch`／`unwatch` は包んだアクター自身を監視者にし、`subject` を返す（§3.3） |
| M2 | 中 | 直した | `receive_timeout_is_none` をやめた。#200 が入ったので、プローブの時点で実行器が持つ設定を `TestActorRef::receive_timeout() -> ReceiveTimeoutSetting` で読み、ケース9で `Undefined` を見る（注4） |
| M3 | 中 | 直した | factory を受ける4つ（`new` 系）と `Props` を受ける4つ（`from_props` 系）に分けた（§3.3） |
| M4 | 中 | 直した | `Display` を `TestActor[<path>]` にした。停止後の読み取りの文言を `"underlying actor is terminated"` にそろえた。停止の印は `on_termination` ではなく、実行器の停止の経路で watcher への通知より前に立てる（§3.1、I7、Quint の `handleBeforeWatchers`） |
| M5 | 中 | 直した | 足す型とファイルの一覧を §3.1 の表に書いた。`pub` の型は `ActorHandle` だけで、`ActorControl` は `pub(crate)` にした |
| L1 | 低 | 直した | テストファイルに red のテストを置かない。unported の3件は冒頭の注釈に書く（§1、§10） |
| L2 | 低 | 直した | Quint に、プローブが配送中の停止、再起動の失敗による停止、直接の受信での停止を入れ、負の対照を7つ置いた（§6）。PBT に P5（停止）と P6（直接の受信）を足し、P1 と P4 の抽選を広げた |
| L3 | 低 | 直した | 共有の状態をテスト関数ごとにローカルに作ることを §1 に書いた |
| L4 | 低 | 直した | 型引数を持たない形のまま、複製で読む形にした（§3.1、§3.3） |
