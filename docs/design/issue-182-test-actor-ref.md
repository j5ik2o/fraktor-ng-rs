# 設計：#182 TestActorRefSpec（testkit に TestActorRef の能力を用意する）

状態：**未承認（設計の独立レビュー第1回の指摘と、第2回の敵対的読みの指摘を反映した改訂版。伺うことは §8 に3件ある）**。

[Issue #182](https://github.com/j5ik2o/fraktor-ng-rs/issues/182) の第1段（設計だけ）の記録である。実装は、この設計が通ってから行う。この文書は実装を含まない。第2段では、テストを Claude Opus の担当が先に red で書き、実装の担当はそのテストを書き換えずに通す実装だけを書く。設計に付く Quint の仕様（`crates/fraktor-actor/specs/test_actor_ref.qnt`）は、この段で置く（§6）。

対象は Pekko v2.0.0 の `testkit/src/test/scala/org/apache/pekko/testkit/TestActorRefSpec.scala` である。原典の本体は `testkit/src/main/scala/org/apache/pekko/testkit/TestActorRef.scala`（`LocalActorRef` を継承し、Props に dispatcher の指定が無ければ `CallingThreadDispatcher` へ差し替え、`underlyingActor` と `receive` の直接呼び出しと `watch`／`unwatch` を足したもの。API 文書 `TestActorRef.scala:27-31` は「`receiveTimeout` を `None` にする」と書くが、v2.0.0 の本体にはその処理が無い。移植も触らない（§1 の注4、I3））と、`testkit/src/main/scala/org/apache/pekko/testkit/CallingThreadDispatcher.scala`、testkit の `reference.conf`（`pekko.test.calling-thread-dispatcher` の定義）である。

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

### 0.3 敵対的読み（2回目、`a2e77a2`）の53項目を受けて変えたこと（2026-11-27）

指摘ごとの対応は §12 の表にある。重い順に：

1. **I3 の矛盾を1つに揃えた**：I3 は「`TestActorRef` が receive timeout の設定を書き換えない」に限定し、プローブが期限を張り直すことは原典の `InternalGetActor` のとおり（`ActorCell.scala:568-585`。`TestActorRef.scala:159`）と書いた。ケース9が見る「生成直後の `Undefined`」は、最初のプローブまでハンドルの初期値で返す（§1 の注4、§3.1、§3.2、I3）。
2. **読み取りの停止の列を分けた**：§3.2 の手順に (2a) を足し、止まった後の読み取り（プローブを送らない）、送った直後に止まっているのを見た読み取り、待ちの間に止まった読み取りの3列を区別した。I7 と §5 も同じ3列に直した。
3. **I9 のケース1の観測を書いた**：`in_context` の外側のアクターの親が `/user` であることから、子の親も `/user` であることを観測する形にした（§4 の I9 の行）。
4. **観測できない主張を落とした・弱めた**：I4 の「処理済み」を「配送済み」に弱め、I6 の「再起動直後の番号」を落とし、`receive` の失敗の文言はテストの追加をしない。観測の表を §4 に、落とした理由を §4.1 に書いた。
5. **Quint とのずれを整理した**：I3 と I10 は Quint で検査しない（I10 は遷移として与える入力、I3 は時刻が無い）。I1・I2・I8・I9・I12 がモデルに無いことも書いた（I2 は P4）。`directReceive` の `not(reading)` は、読み取りの待ちと `receive` の交差を設計でも作らないこと（§3.2 と §3.5 の「アクターの処理の中からは呼べない」）で揃え、§5 に記録した。`bugReadWithoutProbeTest` の狙いを再起動後の古い個体の返却に限ると書いた（§6）。
6. **原典の行を引き直した**：注5 の内部読み取り（`TestActorRef.scala:124-133`）、注6 の `classOf[Dispatcher]`（`TestActorRefSpec.scala:266` と `:21`）、注1 の null（`docs/decisions/scala-java-to-rust-correspondence.md:10`、`:9-12`）、`CallingThreadDispatcher.scala:136-138`、`reference.conf:31-33`、`TestKit.scala:78-85`、`TestEventListener.scala:107-110`、`:126-138`、`:194-204`、`ActorCell.scala:568-585`、`dungeon/ReceiveTimeout.scala:44-64`、`:88-94`。
7. **「13件の run が通る」に流し方を書いた**：`npm ci`、`bash scripts/quint-check-specs.sh`、1本だけ流す場合の3つのコマンドを §6 に書いた。この環境で流して13件と `invariant` の模擬が通った。
8. **その他の矛盾を直した**：`ActorHandle::receive_timeout` を `Option` から `ReceiveTimeoutSetting` にした（ケース9の `Undefined` と合う）。`supervisor: Option` の4分岐を実行器が親の種類で分けると書いた（§3.1）。I1 に参照の同一性を書いた。§3.6 の公開 API の表に既存の `pub`（`WeakActorSystem::is_actor_terminated`、`run_one`）を足した。§5 に `receive` の失敗の文言と、`Dispatcher` の狭めの行を足した。§9 に保存データと秘密の行を足した。

### 0.4 答え合わせの指摘を受けて変えたこと（2026-11-27、3回目）

別の会話の答え合わせで出た「答えの無い項目・根拠のずれ・red のテストが書けない主張」だけを直した。§12 の表に、答えの無かった 20、6、17、19、23、30、32、33、44、45、48 の行を足した。

1. **根拠を原典・対応表に合わせた**：注1 の `context.actorOf(Props(...))` は名前を渡さない（`TestActorRefSpec.scala:145`）ことを書いた。注9 の参照の同一性を、`ActorRef` の等値（`actor_ref_value.rs:225-229`）に合わせた。注4 の張り直しの行を `Executor::receive_timeout_before`／`receive_timeout_after`（`executor_value.rs:952-997`）に合わせた。注6 の対応表は `:27`（`:51` は現 main で別の行）に直した。注5 の API 文書の約束の範囲を、原典の API 文書が同期を書かないこと（`TestActorRef.scala:124-133`）と、Spec の「単一スレッド」の前提（`TestActorRefSpec.scala:128`）で書き直した。§9 と §12 の 50 の `quint-in-ci.md:136-139` を `:135-140` にした（`:135-140` は `docs/design/specs/` の範囲外の記述。`:134` は空白行、`:135` は「範囲外：」の行、`:136-140` がその中身である）。
2. **red のテストが書けない主張を、書ける形にするか、§7.3 で「確かめない」とした**：I7 の (b) と (c) の区別は確かめない、(a) の「プローブを送らない」は dead letters に `ActorControl` が出ないことで見る、I9 のケース1の親はケース1では確かめず P1 の `in_context` の抽選で見る、プローブの期限の張り直しは分類（`is_not_influence_receive_timeout`）で見る、`receive` の失敗は `#[should_panic]` で見る。I4 の「配送済み」はケースのテストでは見ず、P2 で見る（段7b のケース3は引き続き unported）。

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

共有の状態：原典は companion object の可変変数（`counter`、`thread`、`otherthread`）をケースの間で共有し、ScalaTest の逐次実行に頼る。`cargo test` は `#[test]` を並列に流すので、`static` やグローバルの状態にしない。各テスト関数の中で `Arc<AtomicI32>` と `Arc<spin::Mutex<Option<std::thread::ThreadId>>>` を作り、テストのアクターに渡す。テストのスレッドの `ThreadId` は、そのテスト関数の冒頭で `std::thread::current().id()` を取る（原典の `val thread = Thread.currentThread` に当たる。`TestActorRefSpec.scala:33` はクラス本文の内側（companion object `TestActorRefSpec.scala:30-118` の一部）で一度だけ取る。各テスト関数で取るのは、`cargo test` が同じプロセスでテストを並列に流すためであり、原典より強い形にはならない：`assertThread()` が見るのは「特に記録が無いか、テストのスレッドと等しい」の1点である（`TestActorRefSpec.scala:124-126`））。

注1（ケース1と2、アクターの中からの生成）：原典は、`TestActorRef(Props(...))` で作るアクターの構築の中で、入れ子のアクターを作る。ケース1は入れ子も `TestActorRef(Props(...))`（supervisor を省くので親は `/user`）、ケース2は `context.actorOf(Props(...))`（親は外側のアクター）である。Rust のアクターの構築（factory）は `ActorContext` を持たないので、入れ子は `pre_start` で作る。ケース1は §3.3 の `TestActorRef::in_context`（supervisor なし、名前なし）、ケース2は `ActorContext::actor_of` で作る。原典のケース2は名前を渡さない `context.actorOf(Props(...))`（`TestActorRefSpec.scala:145`）であり、生成名は Pekko の `actorOf` が作る（`ActorRef.scala` の `actorOf(props)` の既定の名前の系列）。移植の `ActorContext::actor_of` は名前を必須にしているので、テストが名前を作って渡す（`actor_of` の API 文書のとおり）。原典の生成名の書式（`$` と通し番号）自体は写さないので、名前の値は原典と違う。原典のケース2は名前を読まないので、ケースの確かめる内容は変わらない。外側は `?`（`ActorRef::ask`）で入れ子の参照を受け取り、`AskFuture::await_result` で待つ。原典の `Await.result` も待つので、待ち方は弱めていない。`a should not be null` と `nested should not be null` は、Rust の参照が null を取れないので型で保証される（`TestActorRefSpec.scala:137-139`、`:148-150` の null の検査は、`Option` を必ず開いてから読む形になることで落とせない。対応表の null の行（`docs/decisions/scala-java-to-rust-correspondence.md:10`。`:9-12` が null の行のまとまりである）と同じ種類の判断として「null を表す値を型の上で作れない」で写し、ケースからは外さない）。テストは、ask の返事が `ActorRef` として取り出せることと、`a` と別の参照であること（`a should not be theSameInstanceAs(nested)`、`:140`）を確かめる。`nested` の親は、ケース1でもケース2でも、テストから観測しない（原典も見ない。ユーザー guardian は子の生成を数え上げる口を持たない）。

ケース1の入れ子の親（`/user`）をケース1の中で観測するには、外側のアクターが入れ子の `TestActorRef` を保持して ask の返事で返す形が要るが、原典のケース1のアクターは `sender() ! nested` で `ActorRef` だけを返し、`TestActorRef` を返さない（`TestActorRefSpec.scala:133-136`）。入れ子の `ActorRef` だけでも `path()` は見えるが、`path().parent()` が `/user` の guardian のパスと等しいことは、外側のアクターが `/user` の直下にいること（ケース1の外側は `TestActorRef::from_props` で作るので `/user` の直下である）から従うだけで、`in_context` が親を正しく選んだことの検査にはならない（外側が誰の下でも同じ結果になる）。したがって、**入れ子の親はケース1では確かめない**。§4 の I9 の行に書いた形で、ケース1とは別に確かめる（P1 の `in_context` の抽選と、第2段で `in_context` の呼び出しを1つ書くテスト）。

注2（段7b が前提の3件）：ケース3、5、6 は、送ったその場で処理が終わることそのものを確かめる。ケース3は4回の `!` の直後に待ちなしで `counter should ===(0)` を2回見て、`assertThread()` で処理が別のスレッドで起きていないことを見る。ケース5は `boss ! "sendKill"` の直後に待ちなしで `counter should ===(0)`（子の `preRestart` と `postRestart` が呼ばれたこと）を見て、`assertThread()` を見る。ケース6は `a ? "work"` の直後に `f.isCompleted should be(true)`（"CallingThreadDispatcher means that there is no delay"）を見る。どれも複数のアクター（クライアント、サーバー、ワーカー、boss と子、ask の一時の宛先）の処理が送り手のスレッドで同期して進むことに頼る。今の実行器では送信はキューに入り、誰かが運転するまで処理されない。待ちを入れて書くと、原典の確かめる内容（即時性と、送り手のスレッドで処理されること）を外した弱いテストになる。今の1スレッドの実行器を送信のたびに空になるまで回す形（`run_until_idle`）で見かけを合わせることもできるが、それは今の実行器の作りから生まれる強さ（全体が1スレッドで同期して進むこと）に頼る形であり、`docs/decisions/pekko-semantics-first.md` が「直す」と決めた作りのずれに当たる。したがって、この3件は弱めた版を書かずに unported のまま残し、数からは外さない。段7b の CallingThreadDispatcher が入った後に、原典どおりの形で移す。扱いは §8 の伺い1にまとめる。

注3（ケース4）：原典は `EventFilter[ActorKilledException]().intercept { ... }` で囲む。`EventFilter[A]()` の `occurrences` の既定は `Int.MaxValue`（`TestEventListener.scala:194-204`）であり、`TestEventListener.scala:107-110` の `awaitDone` が `todo == Int.MaxValue || todo == 0` で判定するとおり、件数を見ずに黙らせるだけである。`PoisonPill` による停止は `ActorKilledException` を出さないので、原典でも0件で通る。移植は `EventFilter::typed("ActorKilledException").intercept_any(...)`（§3.3）で写す。既存の `intercept` は件数1を確かめるので、使うと0件で必ず落ちる。`PoisonPill` は既存の移植の前例どおり文字列のペイロード `"PoisonPill"` で送り、sender は test actor にする。watch するアクターは `pre_start` で `context.watch(&a)` を呼び、`Terminated` を包んで test actor へ転送する。`expect_msg` 系で `WrappedTerminated(Terminated(a))` を待ち、その後で `a.is_terminated()` が真であることと、`assertThread()`（記録された別のスレッドが無いか、テストのスレッドと等しいこと）を確かめる。ケース4の `WorkerActor` は利用者のメッセージを1件も受けない（`PoisonPill` は振る舞いに届かない）ので、原典でも `otherthread` は書かれない（`TestActorRefSpec.scala:181-198`）。移植でも同じ形で写すので、弱めていない。

注4（ケース7とケース9、receive timeout）：#200 が main に入ったので、ケース7は今の本体で移す。原典の `ReceiveTimeoutActor` は構築で `context.setReceiveTimeout(1.second)` を呼ぶ。移植は `pre_start` で `context.set_receive_timeout(ReceiveTimeoutSetting::Finite(Duration::from_secs(1)))` を呼び、`ReceiveTimeout` を受けたら test actor へ `"timeout"` を送って自分を止める。テストは `expect_msg("timeout")` で待つ（既定の3秒の中に1秒の期限が入る）。`FraktorSpec` のシステムは単調時計を持つので、有限の期限を張れる（#200 の `receive_timeout_spec.rs` と同じ）。ケース9は `underlyingActor.context.receiveTimeout` が `Duration.Undefined` であることを見る（`TestActorRefSpec.scala:254-257`）。移植は `TestActorRef::receive_timeout()` が `ReceiveTimeoutSetting::Undefined` を返すことを見る。この値は、プローブを処理した時点で実行器がそのアクターについて持つ設定（`ActorContext::receive_timeout` が返すものと同じ。`Executor::receive_timeout_setting`）であり、生成時に testkit が書いた値ではない（§3.2）。なお、原典の `TestActorRef` の API 文書（`TestActorRef.scala:27-31`）は「receiveTimeout を None にする」と書くが、v2.0.0 の本体にその処理は無く、ケース9が見るのはセルの既定値の `Undefined` である。移植も触らない（I3、§3.1 の `ActorControl` の項）。

ケース9が「触らない」ことと、プローブが期限を張り直すことは両立する。I3 が言うのは、`TestActorRef` が（testkit の側でも kernel の生成の口でも）receive timeout の設定を書き換えない、ということであり、プローブは原典の `InternalGetActor` と同じく、届いた1件のメッセージとして期限を張り直す（`ActorCell.invoke` の `cancelReceiveTimeoutIfNeeded` と `checkReceiveTimeoutIfNeeded`。`ActorCell.scala:568-585`、`dungeon/ReceiveTimeout.scala:44-64` の `checkReceiveTimeout`、`:88-94` の `cancelReceiveTimeoutIfNeeded`）。`InternalGetActor` は `AutoReceivedMessage` で `NotInfluenceReceiveTimeout` ではない（`TestActorRef.scala:159`、`Actor.scala:82` の `Identify` と対比）ので、原典でも張り直す。移植の実行器では、プローブは既存の `Executor::invoke` の「利用者のメッセージ」の枝を通り（`executor_value.rs:2442-2449`。`Kill` と `PoisonPill` の枝の後、`target.invoke` の前）、`is_not_influence_receive_timeout` が偽を返すので `influences = true` になり、`receive_timeout_before` が予約を取り消し（`executor_value.rs:952-963` の `cancel_task`）、`receive_timeout_after` が張り直す（`:969-975` と `:977-997` の `check_receive_timeout`。期限が有限でなければ何も予約しない）。**観測は「張り直しの有無を直接見る」形では書かない**：外から見えるのは「プローブの約1秒後に `ReceiveTimeout` が届く」かどうかだけで、張り直しと最初の予約の区別が時計の精度に依存する。代わりに、`ActorSystem::is_not_influence_receive_timeout`（`actor_system_value.rs:504-507`、`test-support`）がプローブの `Message` に `false` を返すこと（`NotInfluenceReceiveTimeout` の分類に入っていないこと）を確かめる。分類が同じなら、期限の計算は既存の `Identify` 以外のメッセージと同じ道を通る（P1 の分類の抽選に1件足す）。

注5（ケース8と、underlying の読み取りの同期）：原典は `ref ! "hallo"` の直後に `ref.underlyingActor.s` を読む（`TestActorRefSpec.scala:249-251`）。原典の `underlyingActor` は、個体がまだ見えないときだけ `this ? InternalGetActor` でメールボックスを通って個体を受け取り、既に見えていればセルが持つ個体をそのまま返す（`TestActorRef.scala:124-133`）。移植の `underlying_snapshot` は、個体が見えているかを見分けられない（本体を外に出さない。§2）ので、この `InternalGetActor` の経路を常に通す形にする（§3.2）。これは原典が許す2つの経路のうちの片方に狭めることであり、`docs/decisions/pekko-semantics-first.md` の「狭めはずれにしない」に当たる。API 文書に書いてよいのは、原典の API 文書が同期を約束している範囲だけである。原典の `underlyingActor` の API 文書は `Await.result(this.?(InternalGetActor)(t), t.duration)` を書くだけで（`TestActorRef.scala:124-133`）、「どの dispatcher なら同期する」とは書いていない。それでも、`TestActorRefSpec.scala:128` が「`CallingThreadDispatcher` により単一のスレッドで動く」前提を名乗り、ケース8が送った直後に読むので、原典の約束は CallingThread の種類のアクターについてだけと読んで、API 文書に書く同期の保証もそこに限る。`underlyingActor` をいつもメールボックス経由にする形は、§3.2 で段7b（#259）に合わせて戻す約束として記録する。ケース8のテストのアクター `TA` は、フィールド `s: String` を持ち、`underlying_snapshot` で `s` の複製を返す（`TestActorRefSpec.scala:242-247`）。

注6（ケース10と11、dispatcher の種類）：原典は dispatcher のクラス（`getClass`）を見る（`TestActorRefSpec.scala:259-262`、`:264-267`）。対応表の「Scala の実行時クラス階層で分類すること」の行（現 main の `docs/decisions/scala-java-to-rust-correspondence.md:27`。分類子は型で表し、上位型の関係は明示的に登録するモデルにする）に従い、移植は生成のときに実行器が解決した dispatcher の種類（`DispatcherKind`）を見る。原典の `classOf[CallingThreadDispatcher]` は、この行の「分類子を型で表す」に写す：`DispatcherKind::CallingThread` という種類が、`CallingThreadDispatcher` のクラスに当たる。ケース11の `classOf[Dispatcher]`（`TestActorRefSpec.scala:266`）は、`Dispatcher` が `org.apache.pekko.dispatch.Dispatcher`（`TestActorRefSpec.scala:21` の import）であり、`DispatcherKind` のような列挙子ではなく「クラスの等値」である。Rust には実行時のクラスの等値が無いので、この行を「登録した名前の種類が、登録したとおりに解決される」（`Dispatchers::kind_of("disp1") == Some(DispatcherKind::Default)`）に写す。`DispatcherKind` の列挙子そのものが分類子である（上の行の「上位型の関係を登録する」形は使わない）。原典が `Dispatcher` のサブクラスも受けるのに対し、移植は `Default` だけを受けるので、ケース11の入力（`type=Dispatcher`）については原典と同じ結果になる。この狭めは §5 に記録する。対応表に同じ種類の決定があるので、伺いはしない（初版の提案3は取り下げた）。ケース10が見るのは種類だけであり、原典も実行そのもの（送り手のスレッドで処理されること）はこのケースでは見ない。CallingThread の種類の実行そのものは段7b の仕事である（§9 に記録する）。ケース11の `PekkoSpec("disp1.type=Dispatcher")` は、前例（#154 の D1、G0b の決定 E、#192）どおり文字列の設定体系を作らず、`FraktorSpec::with_dispatchers(name, vec![DispatcherConfig { id: "disp1".into(), kind: DispatcherKind::Default }])` で写す（原典の `type=Dispatcher` は既定の種類の dispatcher である。原典の `TestActorRefSpec.scala:120` の設定文字列は、対応表の HOCON の行（`docs/decisions/scala-java-to-rust-correspondence.md:19`）のとおり Rust の値で与える）。

注7（ケース12と13、`receive`）：原典の `receive(o)` と `receive(o, sender)` は、メールボックスと `autoReceiveMessage` を通さずに `underlying.receiveMessage(o)` を直接呼ぶ。sender を省くか `null` を渡すと `deadLetters` になる。ケース12と13の `WorkerActor` は `"work"` を受けて `sender() ! "workDone"` を送り、`context.stop(self)` を呼ぶ。原典は直後に `ref.isTerminated should ===(true)` を見る。これが真になるのは、`TestActorRef` の dispatcher が CallingThread で、`context.stop(self)` が積んだ `Terminate` が送り手のスレッドでその場で処理されるからである。移植の `receive` は、振る舞いを1回直接呼ぶ口（§3.5）で写し、dispatcher の種類が CallingThread のときは、その呼び出しが積んだ対象のアクター自身のシステムメッセージを返る前に処理する。ほかのアクターのメールボックスは回さないので、注2の「複数のアクターが同期して進むこと」には頼らない。ケース13は、その後で test actor が `"workDone"` を受けることを `expect_msg` で見る。

注8（ケース14と15）：原典の確認は2つある。(1) 親を指定した生成（ケース14は `TestActorRef(props, parent.ref, "Child")`、ケース15は `TestProbe.childActorOf(props, "Child")`）が例外を投げないこと（`TestActorRefSpec.scala:282-294`、`:295-307`）。(2) 子が `1` を受けて `RuntimeException("expected")` を投げても送った側へは伝わらず、失敗は監督とログで処理され、`EventFilter[RuntimeException](occurrences = 1, message = "expected")` がちょうど1件を見ること（`TestActorRefSpec.scala:283`、`:296`）。移植は (1) を `TestActorRef::with_supervisor_and_name`（`TestProbe::test_actor` を supervisor に渡す）と既存の `TestProbe::child_actor_of_named` で写し、(2) を既存の `EventFilter::typed("RuntimeException").with_message("expected").intercept(...)`（件数1）で写す。実行器は振る舞いの panic を原因名 `RuntimeException` とその文言で発行する（`executor_value.rs` の `RUNTIME_CAUSE`）。ケース14の子は、原典どおり test actor の stopping strategy で止まる（§3.4。原典の `TestActor` の `DelegatingSupervisorStrategy` は、登録の無い子に `stoppingStrategy` を使う。`TestKit.scala:78-85`、`:120`）。`EventFilter` の件数1の一致は、`filter-leeway` の後に `awaitDone` で判定される（`TestEventListener.scala:107-110` の `awaitDone`、`:194-204` の `occurrences` の既定 `Int.MaxValue`、`:126-138` の `intercept` が `filter-leeway` の後に数える形である）。原典の `Logger`（`TestActorRefSpec.scala:95-101`）は `Warning` の集計で、どのケースからも作られない。§1 の注2 で触れる「ログを出す」は `TestActorRef.scala:44-53` の `log.error` を指し、`Logger` とは無関係である。

注9（ケース16〜19）：原典の型引数だけで作る形（`TestActorRef[WorkerActor]`）は、`ClassTag` とリフレクションでコンストラクタを探す形である。対応表の `classOf[…]` の行どおり、リフレクションでのコンストラクタの検索は移さず、factory を受ける4つの口（§3.3）で写す。原典の `WorkerActor` は構築で `val supervisor = context.parent` と `val name = context.self.path.name` を持つ（`TestActorRefSpec.scala:76-77`）。テストが確かめるのは、`ref.underlyingActor.supervisor should be(system.guardian)`、`...name should be(...)`、`...supervisor should be(parent)` の3種類である（`:315`、`:320`、`:326`、`:332-333`、`:338`、`:343`、`:349`、`:355-356`）。原典の `should be` は `ActorRef` の等値であり、`LocalActorRef` の等値は `path` の比較である（`ActorRef` は `path` で等しいと決まる）。移植の `ActorRef` の等値は `id` と `generation` の比較（`actor_ref_value.rs:225-229`）なので、原典の `should be` は移植の `ActorRef` の等値にそのまま写る。`ActorPath::parent` は要素を1つ落とすだけで（`actor_path_value.rs:152-156`）、親のパスから正しい参照を選ぶのは `resolve_actor_ref` の仕事である。`resolve_actor_ref`（`actor_system_value.rs:292`）は、`/user` の guardian と開始した子の参照を、今の表から返す。started な子の親は自分自身の参照を記録しているので、記録した `supervisor` と `path().parent()` を解決した参照は同じ個体を指す（`id` と `generation` が等しい）。**検査は `path` の等値1つにしない**：`ActorPath` の等値は「同じ名前の下に別の個体がいる」ことを見分けないので、`ActorRef` の等値（`id` と `generation`）まで見る。これは原典の `should be(parent)` が `ActorRef` の等値（`LocalActorRef` は `path` で等しい）であるのと同じ強さである。移植の `WorkerActor` は `pre_start` で `context.self_ref()` を記録し、`underlying_snapshot` でそれを返す。テストは、記録した参照の名前が指定の名前であることと、記録した参照の `path().parent()` を `resolve_actor_ref` で解決した参照が、記録した supervisor の参照と `ActorRef` の等値で等しいことを確かめる。既定の supervisor のケース（16、17、20、21）では、supervisor は `resolve_actor_ref` で解決した `/user` の guardian である（原典の `system.guardian`）。`resolve_actor_ref` は `ActorSystem` にあり、`TestActorRef` を持つテストでは `FraktorSpec::system()` から取る。

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
| watch と `Terminated` | ある。`ActorContext::watch`、`WeakActorSystem::watch(watcher, target)`、`WeakActorSystem::unwatch(watcher, target)`。`ActorSystem::watch` は `&mut self` を取り、`ActorSystem` に `unwatch` は無い。`WeakActorSystem::is_actor_terminated(&ActorRef)` と `run_one` は既に `pub`（`weak_actor_system.rs:172`、`:195`） |
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
  /// The receive timeout the actor had when the last probe was processed;
  /// `Undefined` before the first probe.
  pub fn receive_timeout(&self) -> ReceiveTimeoutSetting;
  /// The kind the executor resolved for the actor's dispatcher at creation;
  /// `None` when only a `DispatchSettings` defines the name.
  pub fn dispatcher_kind(&self) -> Option<DispatcherKind>;
}
```

- 中身は `has_started`、`terminated`、`generation`、`next_sequence`、`processed_sequence`、スナップショット（`Option<Box<dyn Any + Send + Sync>>`）、プローブの時点の receive timeout（初期値は `ReceiveTimeoutSetting::Undefined`）、dispatcher の種類である。スナップショットはハンドルの中に1つだけ持ち、`read_snapshot` はロックの中で `downcast_ref::<T>()` して複製を返す。何度読んでもよい（初版の `Box` を返す形では2回目が読めなかった）。
- ハンドルは `TestActorRef` の生成ごとに1つ作り、実行器がそのアクターのセルに付ける（下の「セルの側」）。再起動で個体が替わってもハンドルは同じであり、番号と世代は戻らない（I6）。

#### `ActorControl`（プローブ、`pub(crate)`）

```rust
// crates/fraktor-actor/src/kernel/actor/test_support/actor_control.rs
/// The numbered probe a read of the underlying actor sends.
pub(crate) struct ActorControl { sequence: u64 }
```

- `pub(crate)` にする。testkit は `ActorHandle::send_probe` だけを使い、型そのものは公開しない（公開の面を増やさないため）。`send_probe` は `target` へ `Message::new(ActorControl { sequence })` を dead letters を送り手として送る（送り手はプローブの処理で使わない。原典の横取り `sender() ! actor`（`TestActorRef.scala:92-98`）の宛先は、移植ではハンドルが受け持つ）。止まったアクターへ送ると、既存の `ActorRef::send_message` が dead letters の経路へ渡すので、その経路の `ActorControl` を捨てる（下の「止まったアクター宛て」）。
- 実行器の配送の層（`PoisonPill` と `Kill` を見るのと同じ層、`Executor::deliver` の中）で、振る舞いへ渡す前に横取りする。原典の `TestActorRef` の `ActorCell` が `autoReceiveMessage` で `InternalGetActor` を横取りするのと同じである（`TestActorRef.scala:85-98`）。振る舞いを `change_behavior` で替えても、`stash` しても、プローブは振る舞いへ届かずに処理される。
- 処理：セルにハンドルが付いていれば、`processed_sequence` をその番号にし、そのときの個体の `underlying_snapshot()` と、実行器が持つそのアクターの receive timeout の設定を、ハンドルへ書く。receive timeout は実行器が既に持つ `Executor::receive_timeout_setting(key)`（`ActorContext::receive_timeout()` が使うものと同じ）で読む。新しい読み取りの口は足さない。ハンドルが付いていなければ何もしない。
- 順序：利用者のメッセージと同じ通常のキューに並ぶ。制御を先に出すキュー（control-aware の形）で追い越させない（I4。Quint の負の対照 `bugProbeOvertakes`）。
- receive timeout：原典の `InternalGetActor` は `AutoReceivedMessage`（`TestActorRef.scala:159`）であり、`NotInfluenceReceiveTimeout`（`Actor.scala:164`）ではないので、受信の期限を張り直す（`ActorCell.scala:568-585` の `cancelReceiveTimeoutIfNeeded` と `checkReceiveTimeoutIfNeeded`。`dungeon/ReceiveTimeout.scala:88-94`、`:44-64`）。プローブも同じに扱う（実行器の既定の分類 `Executor::is_not_influence_receive_timeout` に任せ、特別扱いを足さない。`Identify` だけが分類に入っている）。期限が有限でないときは、原典と同じく何も起きない。
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
- `supervisor` が `None` なら、親は `/user` の guardian である（原典の `TestActorRef(props, name)` は `system.guardian` を supervisor にする。`TestActorRef.scala:175-176`）。アクターの処理の中から呼んでも同じであり、親は作ったアクターにならない（I9）。
- `supervisor` を渡した場合の3つの分かれ方は、`Option` だけでは区別できない（`Some(guardian)` と `None`、`Some(開始していないアクター)`、`Some(戦略を登録していない function ref)`）。実行器が親の参照の種類で分ける：`/user` の guardian と `None` は同じ扱い、開始していないアクターは panic、戦略を登録していない function ref は既定の戦略（§3.4 の4分岐）。testkit はどれも同じ引数（`&ActorRef`）で渡すので、`TestActorRef` の口に新しい型は足さない。
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

- 読み取り（`TestActorRef::underlying_snapshot::<T>()`）の手順は、表の3つの列に分かれる。`receive_timeout()` も、プローブを送って待つ点と待ちの終わり方は同じで、読む値だけが違う。

  | 番号 | 手順 | 停止の列へ入らない理由 |
  |:---:|:---|:---|
  | (1) | ハンドルが停止を示していれば、原典の `IllegalActorStateException("underlying actor is terminated")` と同じ文言で panic する（`TestActorRef.scala:126`） | ここで止まる。プローブは送らない（下の (1) の列） |
  | (2) | `ActorHandle::send_probe` で、ハンドルが持つ次の番号のプローブを送る | — |
  | (2a) | (2) の直後にもう一度 `handle.is_terminated()` を見る。真なら「配送中のプローブがある停止」の列であり、送ったばかりのプローブを待たずに (1) と同じ文言で panic する | ここで止まる。プローブがまだキューに残っていても、原典の `underlyingActor` は `isTerminated` を見た後に `? InternalGetActor` を送るので、同じく待たない |
  | (3) | `processed_sequence() >= その番号` か `is_terminated()` が真になるまで、`WeakActorSystem::run_one` を回して待つ。待ちの上限は testkit の既定の timeout（`DefaultTimeout`。`TestActorRef.scala:129` の `DefaultTimeout` に当たる）であり、超えたら panic する | (3) の待ちの間に止まった場合は「待ちの間の停止」の列であり、止まった時点で (4) へ進む（待ち続けない） |
  | (4) | 停止で終わったら (1) と同じ文言で panic する | — |
  | (5) | `read_snapshot::<T>()` を返す。スナップショットが無い（アクターが `underlying_snapshot` を実装していない）か型が違えば、その理由で panic する | — |

- 停止の3つの列は、`is_terminated()` を2か所で見ることで区別する。(1) の列は**止まった後の読み取り**であり、プローブを1つも送らない。(2a) の列は**送った直後に止まっていることを見た**読み取りであり、プローブは送ったが待たない。(3) の列は**待ちの間に止まった**読み取りであり、`run_one` が止めた後に (3) の条件が真になって (4) へ進む。どの列も「配送中のプローブを待ち続けない」という同じ I7 の結論に着くが、起きる場面が違う。Quint の `readNeverStuck`（`test_actor_ref.qnt:432-434`）は、`startRead` が停止後の読み取りではプローブを積まない（`:215`）ので (1) の列を、`processStop` が停止の印を立てる（`:285-289`）ので (2a) と (3) の列を、それぞれ別の遷移として持つ。**テストは (2a) と (3) を区別しない**（§4 の I7 の段落）。
- この手順は、原典の `underlyingActor` が個体を見られないときに使う `this ? InternalGetActor` と同じく、メールボックスの順序で個体に追いつく形である（§1 の注5）。プローブを処理した個体のスナップショットなので、処理のたびに書き直す必要は無い。再起動の後の読み取りは、新しい個体のスナップショットを返す（I6）。
- `receive_timeout()` は生成直後（プローブを1つも処理していない間）に `ReceiveTimeoutSetting::Undefined` を返す（ハンドルの初期値。I3）。プローブを処理した後は、そのプローブを処理した時点で実行器が持つ設定を返す。ケース16〜23が使う記録した `self_ref` の読み取りも、プローブで同期する。`dispatcher_kind()` は生成時に決まる値なので、同期しない。
- 同期の待ちは `run_one` を回すので、アクターの処理の中からは呼べない（実行器のロックの中なので `already borrowed` で panic する）。原典ではアクターの中から `underlyingActor` を読める（個体が既に見えていれば `underlying.actor` をそのまま返す。`TestActorRef.scala:124-133`）。本 Spec のケースはアクターの中から読まないので、ケースへの影響は無い。この違いは §5 に記録する。
- 段7b（#259）の後に、原典の「個体が既に見えていれば `underlying.actor` をそのまま返す」経路を戻すかは、段7b の CallingThreadDispatcher が「対象のアクター自身の実行だけを、呼び出しのスレッドで回す」ことを使えるかで決める。使えるなら、`underlying_snapshot` は「ハンドルのスナップショットが最新の世代のものであれば、プローブを送らずに返す」形に戻せる。使えない場合は今の形のままとする。どちらでも I5 は成り立つ。（§1 の注5）
- 再起動の前の個体の状態は保持しない。原典の API 文書は「再起動で underlying の参照は捨てられる。古い個体は死後の確認に使える」と書く（`TestActorRef.scala:119-123`）。移植で写せるのは「捨てられる」ほうだけである（§5）。

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

  /// 原典の `receive(o)`。sender は dead letters。振る舞いが失敗したら、実行器の
  /// ロックを離した後で、その失敗（`RunOutcome::Failed { message }` の文言）で panic する。
  pub fn receive<M: Any + Send + Sync>(&self, message: M);
  /// 原典の `receive(o, sender)`。失敗の扱いは `receive` と同じ。
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
- 差し替え（dispatcher の差し替え）は testkit の側だけが行い、kernel の `try_spawn_test_actor` は props をそのまま受けて `verify_configuration` で検証するだけとする（§3.4 の 1.）。§3.1 のとおり、検査はテストのスレッドからの口と `in_context` で同じ実行器の関数1つを通る。
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
3. 振る舞いの失敗は失敗の捕まえ手（`FailureCatcher`）で受け、**監督へは渡さず**、`RunOutcome::Failed { message }` として返す。原典の `receive` で投げられた例外は呼び出し側へ届き、アクターは再起動しない。testkit の `receive` は、`Failed` を受け取ったら、実行器のロックを離した後でその文言で panic する（呼び出し側へ失敗が届く）。**テストで確かめるのは、この panic そのものだけにする**：振る舞いが `panic!("boom")` した `receive` を `#[should_panic(expected = "boom")]` で呼ぶ（原典の `receive` も `try`/`finally` で例外をそのまま通すので、同じ形で落ちることだけを見る）。"失敗の文言が文言として何か」に入る主張（panic の文言が `RunOutcome::Failed` の `message` と一致すること）は、`RunOutcome` の `message` を読む口が `WeakActorSystem::receive_directly` の戻り値だけで、testkit の `receive` がそれを外に出さないので、テストでは観測しない（§4.1 の 3.）。捕まえ手を持たないシステムでは、panic は既存の扱い（実行器の外へ巻き戻り、次の呼び出しでシステムが終わる）になる。`FraktorSpec` のシステムは捕まえ手を持つ。
4. セルをメールボックスへ戻す。対象の dispatcher の種類が `CallingThread` なら、この呼び出しが積んだ対象自身のシステムメッセージ（`context.stop(self)` の `Terminate` など）を、返る前に処理する（メールボックスの `process_all_system_messages`）。原典の CallingThreadDispatcher が、`systemDispatch` で対象のメールボックスを送り手のスレッドでその場で処理するのと同じである（`CallingThreadDispatcher.scala:223-230` の `systemDispatch` が `runQueue` を呼ぶ）。ほかのアクターのメールボックスは回さない。種類が `CallingThread` でなければ、システムメッセージは対象の通常の実行を待つ（原典の別の dispatcher と同じ。`CallingThreadDispatcher.scala:232-246` の `dispatch` は、CallingThread でない mailbox へは積むだけである）。
5. 子を持つアクターの停止は、子の終了を待つので、4. の後もすぐには終わらない。原典でも子が別の dispatcher なら同じである（§5）。

`CallingThreadDispatcher.Id` の実値は `"pekko.test.calling-thread-dispatcher"`（`CallingThreadDispatcher.scala:136-138`）であり、testkit の `reference.conf` は `pekko.test.calling-thread-dispatcher` の設定の節をその名前で定義する（`reference.conf:31-33`）。`CALLING_THREAD_DISPATCHER_ID`（§3.3）はこの2つを合わせたものである。

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

`TestActorRef` が使う `WeakActorSystem::is_actor_terminated(&ActorRef)` と `WeakActorSystem::run_one` は、既存の `pub` である（`weak_actor_system.rs:172`、`:195`）。足さない。`ActorContext::receive_timeout()`（`actor_context.rs:475`）と `ActorContext::dead_letters()`（`:119`）も既存である。`Actor::underlying_snapshot` は `#[cfg(feature = "test-support")]` の既定メソッドなので、feature を切ったビルドでは trait の vtable に存在せず、既定の公開 API の型は変わらない。

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
| I1 | 生成の所属：`TestActorRef` で作ったアクターは、指定した supervisor の子として、指定した名前（指定が無ければ生成名）で作られる。`actor_ref().path().parent()` は supervisor のパスと等しく、そのパスを `resolve_actor_ref` で解決した参照は、参照の同一性（`id` と `generation` の等値。`ActorRef` の `PartialEq`）で supervisor と等しい。パスの等値だけでは足りない：`ActorPath::parent` は要素を1つ落とすだけ（`actor_path_value.rs:152-156`）なので、同じパスを使って、止まった名時のアクターと今のアクターを取り違えうる。参照の同一性まで見ることで、原典の `should be(parent)`（`TestActorRefSpec.scala:315`、`:326`、`:333`、`:338`、`:349`、`:356`）と同じ強さになる。supervisor を省いた生成の親は `/user` の guardian である |
| I2 | dispatcher の選択：Props に dispatcher の指定が無ければ、生成したアクターの dispatcher の名前は `CALLING_THREAD_DISPATCHER_ID` になり、`FraktorSpec` のシステムでは種類が `CallingThread` に解決される。指定があれば（既定の名前の明示を含む）その指定が勝ち、差し替えない。未定義の名前は `ActorCreationError::Configuration` で拒否され、既存のアクターに影響しない。P4 の「同じ種類が、重ねた定義のどちらから来るか」も確かめる（§3.3 の `with_dispatchers`） |
| I3 | receive timeout：`TestActorRef` は receive timeout の設定を書き換えない（testkit の生成の口も kernel の `try_spawn_test_actor` も、`set_receive_timeout` を呼ばない）。生成した直後のアクターの設定は `Undefined` である（`TestActorRef::receive_timeout()` は最初のプローブまでハンドルの初期値 `Undefined` を返す）。アクターが自分で張った期限（ケース7）は、通常のアクターと同じに発火する。**プローブは期限を張り直す**（原典の `InternalGetActor` と同じ。§1 の注4、§3.1）。この2つは両立する：I3 は「設定を書き換えない」ことを言い、張り直しは届いた1件のメッセージによる既存の扱いである |
| I4 | プローブの順序：プローブは利用者のメッセージと同じキューに並び、追い越さない。処理済みの番号は単調に増え、送った最後の番号を超えない。番号 n のプローブが処理された時点で、それより前にそのアクターへ送った利用者のメッセージはすべてそのアクターに配送済みである（「処理済み（振る舞いが実行を終えた）」ではない。最後の利用者のメッセージが n を処理した個体の `receive` の中で走っている場合があるため、外から見えるのは配送済みの点までである。原典の `InternalGetActor` も同じキューを通るので同じ強さ） |
| I5 | 読み取りの一致：`underlying_snapshot` が返す値は、そのプローブを処理した個体の、プローブの時点の `underlying_snapshot()` の値である。`receive_timeout` も同じ（そのプローブを処理した時点の設定） |
| I6 | 再起動：再起動で個体が替わったら、その後の読み取りは新しい個体の値を返す。世代は再起動のたびに増える（`ActorCell::restart` の `post_restart` の後）。処理済みの番号は再起動をまたいで戻らない。**再起動直後の番号そのものは、原典にも移植にも外から見る口が無い**：原典の `TestActorRefSpec` は `counter`（`preRestart` と `postRestart` の呼ばれた回数。`TestActorRefSpec.scala:207-208`）だけを見て、番号を見ない。移植のテストで観測するのは「再起動の後の読み取りが新しい個体の値を返す」と「世代が増える」の2つである（P3） |
| I7 | 停止：アクターが最後に止まると、ハンドルの停止の印が、どの watcher が `Terminated` を受けるよりも先に立つ。止まった後の読み取りは次の3つの列のどれかで `"underlying actor is terminated"` になり、配送中のプローブを待ち続けない：(a) 止まった後に読み始めた（プローブを送らない）、(b) 送った直後に止まっているのを見た（待たない）、(c) 待ちの間に止まった（止まった時点で待ちを終える）。止まったアクターのプローブは dead letters として発行されない。`is_terminated` は停止の後で真になる |
| I8 | 名前の拒否：使用中の名前、空の名前、不正な名前での生成は `Err`（名前を受けない口では panic）になり、既存のアクターには影響しない。止まったアクターが持っていた名前は、既存の生成の経路の意味論どおりに再利用できる |
| I9 | 入れ子の生成：`TestActorRef::in_context` で作った子の親は、supervisor を省けば `/user`、指定すればその supervisor である。`ActorContext::actor_of` で作った子の親は作ったアクターである。どれも作った側と別の参照（`id` と `generation` のどちらかが違い、`ActorRef` の `PartialEq` で等しくない）を持つ。親は次の2とおりで観測する：省いた場合は `child_ref.path().parent()` が `/user` の guardian のパスと等しく、`resolve_actor_ref` で解決した参照が guardian と等しい（**`in_context` の外側のアクターは、`context.self_ref().path().parent()` が `/user/…` の直下でなければならない。ケース1の外側は `TestActorRef::from_props` で `/user` の直下に作るので、外側の親が `/user` であることから子の親も `/user` である**）；指定した場合は §1 の注9 と同じく、子の `pre_start` で記録した `context.self_ref().path().parent()` で観測する |
| I10 | 監督：supervisor を指定して作った子の失敗は、supervisor の戦略が決める。アクターの supervisor なら失敗した時点のその `supervisor_strategy()`、test actor なら stopping strategy、戦略を登録していない function ref なら既定の戦略である。supervisor を省いた子（guardian の下）は、原典の user guardian と同じく guardian の戦略に従う。観測は「ケース5の `boss` の `OneForOneStrategy` で子が再起動する（`preRestart` と `postRestart` が呼ばれる。`TestActorRefSpec.scala:204-221`）」と「ケース14の子が止まる」で行う（§1 の注8、§3.4） |
| I11 | 直接の受信：`receive` は振る舞いを1回だけ、メールボックスのキューを触らずに呼ぶ。振る舞いの失敗は呼び出し側へ panic として届き、監督へは渡らない。dispatcher が `CallingThread` の種類なら、その呼び出しが積んだ対象自身のシステムメッセージは返る前に処理されている（自分を止めたアクターは、返った時点で `is_terminated` が真） |
| I12 | ハンドル無しのアクターの不変：ハンドルを付けない通常の生成の振る舞いは、既存のテストが見る範囲で変わらない。feature `test-support` が切れているときは、testkit 向けの面は存在しない |

**それぞれをテストでどう観測するか（観測できないものは落とした）**

| 番号 | 観測するテスト | どう観測するか | 落とした・弱めた主張と理由 |
|:---:|:---|:---|:---|
| I1 | ケース16〜23 | §1 の注9。記録した `self_ref` のパスの親を解決した参照の `PartialEq` | 無し |
| I2 | ケース10、11、P4 | `TestActorRef::dispatcher_kind()` | 無し |
| I3 | ケース9と P1 の分類の抽選 | `TestActorRef::receive_timeout()`（生成直後は `Undefined`、アクターが `pre_start` で `Finite` を張った後は `Finite`）と、`ActorSystem::is_not_influence_receive_timeout` がプローブの `Message` に `false` を返すこと（張り直しの有無そのものは見ない。§1 の注4） | 張り直しの有無を直接見る主張は落とした（時計の精度に依存する。§1 の注4） |
| I4 | P2 | 「すべて処理済み」の形では観測しない（§4.1 の弱めた主張を参照） | 「処理済み」を「配送済み」に弱めた。原典の `InternalGetActor` も同じキューを1回通るだけであり、最後の利用者のメッセージがプローブの個体の `receive` の中で走っている場合、プローブの処理済み番号は進んでいてもその `receive` はまだ終わっていない。原典のケースでは、この差が見える入力が無い |
| I5 | ケース8、P2 | `underlying_snapshot` の戻り値 | 無し |
| I6 | ケース5（段7b の後）と P3 | 再起動の後の読み取りの値と、`ActorHandle::generation` の増加 | 「再起動直後の処理済み番号」は落とした（テストで観測できない。§4.1） |
| I7 | ケース4、12、13と P5 | `is_terminated()` と panic のメッセージ、dead letters のイベントにプローブが無いこと。(b) と (c) の列は区別せず、P5 で「止まった時点で待ちが終わる」を見る（下の段落） | (b) と (c) の区別は落とした（テストで入力を作れない） |
| I8 | P1 | `Err` と既存アクターの不変 | 無し |
| I9 | P1 の `in_context` の抽選と、第2段の小さなテスト（`tests/` の単体テストではなく、`test_actor_ref_spec.rs` のケース1・2の並びに置く観測） | 外側のアクターが `in_context` で作った子の `path().parent()` が `/user` の guardian と等しいこと（P1）、指定した supervisor の子は `pre_start` で記録した `self_ref` の親で見ること | ケース1そのものでは観測しない（外側のアクターがハンドルを返さない。§1 の注1）。**決め方**：P1 は `ActorHandle` を自分で作って `TestActorRef::in_context` を呼ぶので、`in_context` の外側のアクターをテストが書く必要がある。外側は `TestActorRef::from_props` で `/user` の直下に作り、その `receive` で `in_context` を呼び、返した子の参照を ask で返す（原典のケース1と同じ形で、外側のアクターの親が `/user` であることをテストが決める）。この外側のアクターが `pre_start` で子を作れば、原典のケース1の形（構築の中で作る）になる。どちらの形でも、子の親を観測できるのは `in_context` が返す `TestActorRef` を持つ側だけである |
| I10 | ケース5（段7b の後）、14、15 | 再起動の回数と停止 | 無し |
| I11 | ケース12、13、P6 | `is_terminated()`、投げ返された失敗の文言、メールボックスの残りの数 | 無し |
| I12 | `cargo test --workspace` と feature を切った clippy（§10） | 既存のテストが緑のまま | 無し |

I7 の3列（(a) 止まった後に読み始めた、(b) 送った直後に止まっているのを見た、(c) 待ちの間に止まった）は、**どの列でも `"underlying actor is terminated"` に着く**ので、端点だけを見るテストでは3列を区別できない。(a) は「停止後の読み取りが、プローブを1つも送らない」こと（dead letters のイベントに `ActorControl` が現れない。P5）、(b) と (c) は「止まった時点で待ちが終わる」こと（待ちの上限より早く panic する。P5 の時点の抽選）で確かめる。3列を別々に作る原典の行は無い（原典の `underlyingActor` は `isTerminated` を1回だけ見る。`TestActorRef.scala:124-133`）ので、テストは列をまとめて P5 で見る。

ただし、(b) と (c) は **テストで区別できない**。(2a) は「送った直後にハンドルが止まっている」、(3) は「待ちの間にハンドルが止まった」であり、どちらもハンドルが `is_terminated` を返すかを1回見る同じ検査になる。実行器の `run_one` は1件運ぶたびに戻る（`weak_actor_system.rs:195`）ので、テストの側から「(2a) の時点で止まっていたか、(3) の途中で止まったか」を分ける入力を作れない（送った直後の印は、待ちの1手目に見る印と同じである）。したがって、**(b) と (c) を別々に確かめる主張は確かめない**。確かめるのは「(a) はプローブを送らない（dead letters に `ActorControl` が現れない）」と「(b) か (c) のどちらかで、止まった時点で待ちが終わる（待ちの上限を超えない）」の2つだけである。設計の3列は、`is_terminated` を2か所で見る**実装の手順**として残す（止まった後にプローブを送らないために要る）が、テストの区別としては持たない。

### 4.1 テストで観測できない、または弱めざるを得ない主張（落としたものと理由）

原典の行を引いて、主張を落とすか弱めたものを並べる。

1. **I4 の「番号 n が処理された時点で、それより前の利用者メッセージは処理済み」**：落として「配送済み」に弱めた。プローブは利用者メッセージと同じキューに並び、1回の実行で配送される（`test_actor_ref.qnt:235-262` の `deliverAt` は先頭から1件ずつ）。原典の `underlyingActor` も同じキューを通る `InternalGetActor` を1回待つだけ（`TestActorRef.scala:124-133`）なので、処理済み（振る舞いの実行が終わった）を待つ口は原典にも無い。テストが確かめられるのは、ケース3の `counter == 0`（段7b の後の同期実行では、送った4件の処理が終わっている）と、ケース8の内部状態が `"hallo"` であることの2つである。
2. **I6 の「再起動直後の処理済み番号が戻らない」**：番号を外から読む口は `ActorHandle::processed_sequence()` だが、原典の Spec は番号を見ない（`TestActorRefSpec.scala:200-222` は `counter` だけを見る）。再起動直後に番号を読むテストを書くと、原典に無い観測を足すことになり、原典のケースを1対1で写す形が崩れる。したがってケースとしては観測せず、I6 の観測は「新しい個体の値を返す」「世代が増える」の2つにした。番号が戻らないことは、P3 で確かめる：`TestActorRef` は自分のハンドルを保持しており、`ActorHandle::processed_sequence()` は `test-support` の `pub` なので PBT から読める。P3 を「再起動の前後で `processed_sequence()` が減らない」も見る形に広げる（原典外の性質テストなので、観測を足してよい場所である）。Quint の `sequenceMonotonic` も同じ主張を持つ。
3. **`receive` の失敗の文言**（§3.5 の 3. と §5 の行）：同じ文言であることを見るテストは原典に無い（`TestActorRef.scala:100-117` の `receive` は、投げられた例外をそのまま呼び出し側へ伝え、テストはそれを見ない）。移植のテストで、ケース12・13と別に「`receive` で投げた失敗の文言がそのまま panic の文言になる」ことを1件足すと、原典の23件の外に出る。ここでは、I11 の観測をケース12・13（`is_terminated` と `"workDone"`）と P6（振る舞いの失敗が `RunOutcome::Failed { message }` の文言で返る）に置き、`receive` の失敗の文言が `RunOutcome::Failed { message }` の `message` であることを §3.5 の 3. で定義する（テストの追加はしない）。
4. **ケース1・2の `nested should not be null` と `a should not be null`**：null を表す値を型で作れないので、`Option` を開く形そのものが確かめになる。ケースからは外さない（§1 の注1）。
5. **ケース4の `EventFilter` の0件一致**：原典は `occurrences = Int.MaxValue` で数を見ない（`TestEventListener.scala:194-204`、`:107-110`）。移植の `intercept_any` で同じに写すので、弱めていない。
6. **I7 の (b) と (c) の区別、(a) の「プローブを送らない」を呼び出しの記録で見ること、プローブによる期限の張り直しの有無、`receive` の失敗の文言**: テストで入力を作れないか、外から見る口が無い。観測できる形に置き換えたものと、確かめないものは §7.3 にまとめた。


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
| アクターの処理の中から `underlying_snapshot`、`receive`、`receive_timeout` を呼ぶ | 実行器のロックの中で実行器を回す | `already borrowed` で panic する。原典ではアクターの中から読める（`TestActorRef.scala:124-133`）。本 Spec のケースはアクターの中から呼ばない。段7b の後で戻せるかは §3.2 に書いた |
| `receive` で panic するメッセージを入れる | 原典では例外が呼び出し側へ届き、アクターは再起動しない（`TestActorRef.scala:100-117`） | 同じに写す（I11）。文言は、`RunOutcome::Failed { message }` の `message`（捕まえ手が受けた panic の文言）である。原典はこの文言を見るテストを持たないので、テストでは観測しない（§4.1 の 3.） |
| `receive` で `PoisonPill` や `Kill` の文字列を入れる | 原典の `receive` は `autoReceiveMessage` を通らないので、振る舞いの `receive` に普通のメッセージとして届く | 同じに写す（§3.5 の 2.） |
| 止まったアクターへの `receive`、`underlying_snapshot` | 原典は例外 | `"underlying actor is terminated"` で panic する |
| 止まったアクターへの `tell` | dead letters へ行く | 既存の意味論のまま。例外にしない |
| 止まった名時のアクターと、同じ名前で作り直したアクター | パスは同じ | 参照の同一性（`id` と `generation`）で区別する。`ActorPath::parent` の等値だけでは区別できない（I1） |
| dispatcher を差し替えたアクターの dispatcher の種類 | 原典は `getClass` で `Dispatcher` の等値を見る | `DispatcherKind::Default` との等値で写す。原典が `Dispatcher` のサブクラスも受けるのに対し、移植は `Default` だけを受ける。ケース11の入力では同じ結果になる（§1 の注6） |
| 読み取りの待ちの間にアクターが止まる（プローブが配送中） | プローブが処理されない | 停止の印で待ちを終え、panic する。待ち続けない（I7、Quint の `readNeverStuck`）。送った直後と待ちの途中で印をもう一度見る（§3.2 の (2a) と (3)） |
| 止まった後に読み始める | プローブを送らない | (1) で `"underlying actor is terminated"` で panic する。配送中のプローブは作らない（§3.2 の (1)）。(1) で止まるので、プローブを送る前に panic する。「送らなかった」ことは、テストの側から `send_probe` の呼び出しを数える口が無いので直接は見られない。**もし (1) を飛ばしてプローブを送れば、止まったアクターのメールボックスが閉じているので dead letters に `ActorControl` が出る**（`ActorRef::send_message` の閉じたメールボックスの経路）。したがって観測できるのは「(a) の読み取りの後に、dead letters のイベントに `ActorControl` が現れない」で、これは (1) があることの検査になる（§7 の P5） |
| 読み取りの待ちの間にアクターが再起動する | プローブは新しい個体が処理する | 新しい個体の値を返す（I6） |
| `change_behavior` や `stash` をするアクター | プローブが振る舞いに渡ると、横取りが効かない | 配送の層で振る舞いの前に横取りする（§3.1） |
| receive timeout を張ったアクターを読む | プローブが期限を張り直す | 原典の `InternalGetActor` と同じに扱う（§3.1）。本 Spec のケース7は読まない |
| CallingThread でない dispatcher のアクターを読む | 原典の読み取りは同期しないことがある | 移植は常に同期する。原典が許す範囲の中の狭めであり、利用者への約束としては書かない（§1 の注5） |
| 読み取りの待ちと `receive` が交差する | 原典の CallingThreadDispatcher では、`receive` が（テストのスレッドで既に動いている読み取りの待ちの最中に）同じアクターの振る舞いを呼びうる | 設計では交差を作らない。どちらも実行器のロックを取るので、テストのスレッドからは1つずつしか呼べない（`ActorHandle` のメソッドも同じロックを取る）。Quint の `directReceive` の `not(reading)`（`test_actor_ref.qnt:343-344`）と同じにした（§3.2 の 3 列と §3.5） |
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
| P1 | ランダムな生成の組み合わせ（supervisor の有無と種類：省略、明示した `/user` の guardian、開始したアクター、test actor、止まったアクター。名前の有無：空文字、長い文字列、重複、ワイルドカード文字、止まったアクターの名前の再利用）について、成功した生成は I1 のパス条件と参照の条件を満たし、拒否された生成は既存のアクターのパスと状態を変えない。`in_context` の生成（supervisor 省略と指定）の親も I9 の観測で確かめる | I1、I8、I9 |
| P2 | ランダムなメッセージ列（内部状態を数えるアクターへ、文字列と数値の列。途中で `change_behavior` と `stash`／`unstash_all` を挟む）を送り、`underlying_snapshot` が、同じ列を単純なモデルで追った内部状態と等しい。途中の状態が最後と同じ値になる列（行って戻る列）を含める | I4、I5 |
| P3 | ランダムな再起動の列（`Kill` を挟む）について、世代が単調に増え、再起動の後の読み取りが新しい個体の開始状態から追った値になり、`ActorHandle::processed_sequence()` が再起動の前後で減らない | I6、I4 |
| P4 | dispatcher の指定の抽選（指定なし、`DEFAULT_DISPATCHER_ID` の明示、`with_dispatchers` で登録した名前、`with_dispatchers` の定義と CallingThread の定義が同じ id で重なったとき、未定義の名前）について、解決した種類が I2 の規則どおりになり、未定義の名前は `ActorCreationError::Configuration` で拒否される。重なった場合は、利用者の定義を先に見るか CallingThread を先に見るかを実装で1つに決め、その決めたほうを記録する（原典の `Dispatchers.lookup` は、設定に同じ名前が2つあれば後に読んだほうが勝ちであり、どちらを勝たせるかは Spec が確かめていない） | I2 |
| P5 | ランダムな時点で止めたアクター（処理の途中、プローブの配送中、再起動の後）について、その後の読み取りは `"underlying actor is terminated"` で panic し、`is_terminated` が真であり、dead letters のイベントにプローブが現れない。止まった後の読み取りは、待ちの上限（`DefaultTimeout`）より早く panic する（止まった時点で待ちが終わる。§4 の I7 の段落） | I7 |
| P6 | ランダムなメッセージ列を `receive` で直接入れたとき、メールボックスに積んであったメッセージの数と順序が変わらず、自分を止めるメッセージの後は（CallingThread の種類のとき）`is_terminated` が真である | I11 |

### Quint は書いた

判断：書く。`TestActorRef` の中核は状態遷移である。個体は「開始前→稼働→（再起動で世代が変わる）→停止」と移り、プローブの処理済み番号、読み取りの結果、停止の印は、その遷移と送信と読み取りが交差する列の全体で成り立つ必要がある。既存の仕様（`crates/fraktor-actor/specs/test_fsm_ref.qnt` は FSM のタイマと状態、`crates/fraktor-testkit/specs/watch_handle.qnt` は watch のハンドル）は、汎用のアクターの個体とプローブを捉えていない。

置き場所は、状態の実体（`ActorHandle`、`ActorCell`）がある fraktor-actor の `crates/fraktor-actor/specs/test_actor_ref.qnt` にした（`test_fsm_ref.qnt` と並べる）。CI の決まり（`docs/design/quint-in-ci.md`）に合わせ、不変条件は `val invariant` にまとめ、`run` の名前は `Test` で終え、負の対照は `run bug…Test` で `.expect(not(...))` を期待する。

**流し方（Quint と `jq` が要る）**：リポジトリの根で次を流す。`package.json` は `@informalsystems/quint` を `0.32.0` に固定しており、`scripts/quint-check-specs.sh` は `./node_modules/.bin/quint` を使う。`quint` と `jq` が入った環境（CI の `quint` ジョブと同じ）では `npm ci` の後に流せばそのまま通る。`node_modules` が無い環境では、先に `npm ci` を流す（`npm ci --ignore-scripts` でも足りる）。`jq` が無いと `main_module` が空になるので、`jq` も要る。

```
npm ci                    # ./node_modules/.bin/quint (0.32.0) と jq が要る
bash scripts/quint-check-specs.sh
# 1本だけ流すなら
./node_modules/.bin/quint typecheck crates/fraktor-actor/specs/test_actor_ref.qnt
./node_modules/.bin/quint test crates/fraktor-actor/specs/test_actor_ref.qnt
./node_modules/.bin/quint run --invariant=invariant --max-steps=20 --max-samples=10000 --verbosity=2 crates/fraktor-actor/specs/test_actor_ref.qnt
```

2026-11-27 にこの手順で流し、`quint test` が13件の `run` を全部通し（`13 passing`）、`quint run --invariant=invariant --max-steps=20 --max-samples=10000` が `Trace length statistics: max=21` で通り、スクリプト全体（`crates/*/specs/*.qnt` の15本）が exit 0 になった。前の版の「13件が通る」は、`node_modules` が無い環境で流せていなかった（Quint 不在で exit 2）ので、上の手順を書いた。

- 状態：dispatcher が CallingThread か（`ctd`、実行ごとに固定）、段階（開始前、稼働、停止）、世代、今の個体の抽象の状態、メールボックス（利用者のメッセージとプローブの列）、積まれた停止、ハンドル（処理済み番号、最後に送った番号、スナップショットとその世代、停止の印）、watcher への通知、読み手（待っているか、待つ番号）
- 遷移：`start`、`startFails`、`sendUser`、`startRead`（プローブを送る。止まった後はプローブを積まない。§3.2 の (1) の列）、`deliverHead`（先頭を配送する。プローブならスナップショットを取る）、`requestStop` と `processStop`（積まれた停止の処理。プローブを捨てる。§3.2 の (2a) と (3) の列）、`restart`、`restartFails`（`post_restart` の失敗で止まる）、`notifyWatchers`、`directReceive`（振る舞いの直接の呼び出し。CallingThread なら自分の停止をその場で処理する。読み取りの待ちとは交差しない。`test_actor_ref.qnt:343-344` の `not(reading)`）、`finishRead`（読み取りが返る。返れる状態になったら、ほかの遷移より先に起きる）
- 不変条件と、それがどの設計の番号に当たるか：`sequenceMonotonic`（I4）、`probesKeepOrder`（I4）、`readsSeeCurrent`（I5、I6）、`readNeverStuck`（I7。待っている読み手は、プローブがまだ処理されうるか、処理済みか、停止の印がある）、`noProbeLeak`（I7）、`handleBeforeWatchers`（I7）、`directStopSettles`（I11）
- モデルに持たない番号と、設計での扱い：I1（パスと親。モデルは1アクターだけでパスが無い）、I2（dispatcher の名前と種類。`ctd` の真偽でだけ表す）、I3（receive timeout。モデルは期限を持たない）、I8（名前の拒否）、I9（入れ子の親）、I10（監督の戦略の選択。§6 の「モデルにしないもの」のとおり、再起動と停止は遷移として与える）、I12（feature の有無）。**このうち I3 と I10 を「Quint で検査する」とは書かない**。I3 は「設定を書き換えない」こと（PBT の I3 の観測と §1 の注4）で確かめ、張り直しはモデルに時刻が無いので Quint には入れない。I10 の選択は Quint の遷移として与える入力である（モデルが検査する対象ではない）。I2 は P4 で確かめる（`ctd` の真偽の2分岐は Quint にもある）
- 負の対照：`bugProbeToDeadLetters`（停止でプローブを dead letters に出す）、`bugStopWithoutMark`（停止の印を立てない。配送中のプローブの待ちが終わらない）、`bugReadWithoutProbe`（プローブを送らずに古い番号で読む。再起動の後に古い個体を返す）、`bugSequenceRewind`（再起動で番号を戻す）、`bugProbeOvertakes`（プローブが利用者のメッセージを追い越す）、`bugNotifyBeforeHandle`（印を立てる前に watcher へ知らせる）、`bugDirectSkipsSystemRun`（直接の受信で積んだ停止を処理しない）。それぞれ狙った不変条件が落ちることを `run` で確かめた。`bugReadWithoutProbeTest`（`test_actor_ref.qnt:582-591`）は、再起動の後に古い番号で読む列であり、`staleRead` は「`snapGen` が `generation` に等しく、`snapState` が `instState` に等しい」を要求する（`:380`）ので、利用者のメッセージが1つでも処理されていれば再起動なしでも立つ。落としたいのは「古い番号で読む」ことなので、`run` は再起動の後（`:588` の `restart`）に置き、狙いを再起動後の古い個体の返却に限っている（§5 の「再起動の前の個体を読む」の行に当たる）。
- 正の `run`：読み取りが処理済みのメッセージを見る、開始前に送ったプローブは開始を待つ、再起動の後の読み取りが新しい個体を見て番号が続く、配送中のプローブがある停止で読み取りが終わる、再起動の失敗で止まる、直接の受信での停止（CallingThread なら止まり、そうでなければ積まれたまま）

モデルにしないもの：実時間、ほかのアクター、メッセージの中身（抽象の状態の数としてだけ扱う）、監督の方針の選択（再起動するか止めるかは遷移として与える）、子を持つアクターの停止の待ち。

## 7. 段7b が前提のケースと、台帳の扱い

### 7.1 切り分け

| ケース | 前提 | 扱い |
|:---:|:---|:---|
| 3（support reply via sender()） | 段7b（#259）の CallingThreadDispatcher。4回の `!` の直後の `counter == 0` と `assertThread()` は、クライアント、サーバー、ワーカーの3つのアクターの処理が送り手のスレッドで同期して進むことそのもの | unported のまま残す。弱めた版を書かない。数からは外さない |
| 5（restart when Kill:ed） | 同上。`boss ! "sendKill"` の直後の `counter == 0`（子の `preRestart` と `postRestart`）と `assertThread()` は、`boss` と子の処理と監督の判断が同期して進むこと | 同上。監督戦略の引き継ぎ（§3.4）と `in_context` は本件で用意するので、段7b の後は dispatcher の差し替えだけで書ける |
| 6（support futures） | 同上。`f.isCompleted` が ask の直後に真であること（"CallingThreadDispatcher means that there is no delay"）が本体である | 同上 |
| そのほかの20件 | 段7b を前提にしない。読み取り（ケース8、9、16〜23）は §3.2 のとおり対象のアクター1つのメールボックスで同期し、`receive`（ケース12、13）は §3.5 のとおり対象自身のシステムメッセージだけを処理する。ケース4、7、13〜15は原典自身が `expectMsg` 系で待つ | 第2段で green にする（`ported: 20`、`passing: 20`） |

ケース4の `assertThread()` は、§1 の注3のとおり原典でも書かれない値を見るので、そのまま写す。ケース10は種類だけを見るので、段7b を待たない（§1 の注6）。

### 7.2 台帳の扱い

[Issue #200](https://github.com/j5ik2o/fraktor-ng-rs/issues/200) の `actor-tests:ReceiveTimeoutSpec`（15/14/14、`status: red`）と `actor-tests:ExtensionSpec`（8/5/5、`status: red`）の前例どおりにする。第2段の終わりで、対象ノードを次のとおり記録する。

- `status: red`、`cases.total: 23`、`ported: 20`、`passing: 20`、`excluded: []`
- `triage.kind: pending`、`triage.reason` に「ケース3、5、6（送ったその場で処理が終わることそのものを見る）は #234 の段7b（#259）の CallingThreadDispatcher に依存するため後続の issue へ分け、数からは外さない。弱めた版は書かない」と、決めた人と日付を書く（決めた人は §8 の伺いの答えによる）。後続の issue を作る条件（段7b に依存させる）は `triage.reason` に書かず、この節の最後の項にだけ書く
- `rust.crate: fraktor-testkit`、`rust.test_file: crates/fraktor-testkit/tests/test_actor_ref_spec.rs`
- `needs.testkit` は今のままで足りる。`calling-thread-dispatcher` は、本件では名前と種類の登録だけを用意し、実行は段7b である（§9）
- `ledger/capabilities.yaml` の `test-actor-ref` の `provided_by` は、#181 の `test-fsm-ref`（`null` のまま）の扱いにそろえる。本件で新しい決め方は作らない
- 状態遷移の規約（unported → red → green）に合う。ケース3、5、6 を移す後続の issue は、段7b（#259）に依存させて作る（作るのは上司）

issue の文面の「台帳の対象ノードを green にする」は、この形では満たせない。#200 と同じく、ノードを red のまま #182 を完了とするかどうかを §8 の伺い2で上げる。

### 7.3 §7 から外した主張（確かめないものと理由）

設計の本文（§3・§4）にあるが、テストで確かめないものと、その理由を書く。どれも原典の23件のケースから外すものではない（ケースの数は変わらない）。「確かめない」と決めた以上、その主張を「テストで確かめる」と読める形で §1・§4 に残さない。

| 主張 | どこにあったか | 確かめない理由 | 原典の行 |
|:---|:---|:---|:---|
| I4 の「番号 n のプローブが処理された時点で、それより前の利用者メッセージはすべて処理済み」 | §4 の I4、§4.1 の 1. | 処理済み（振る舞いの実行が終わった）を待つ口が原典にも移植にも無い。プローブは同じキューを1回通るだけなので、最後の利用者メッセージがプローブの個体の `receive` の中で走っている場合、番号は進んでいても `receive` は終わっていない。テストで見えるのは「配送済み」までである | `TestActorRef.scala:124-133`、`TestActorRefSpec.scala:156-179`（ケース3は `counter` で見る） |
| I6 の「再起動直後の処理済み番号」をケースのテストで見ること | §4 の I6、§4.1 の 2. | 原典の Spec は番号を見ない（`counter` だけ）。ケースを1対1で写す形を崩さないため、ケースでは観測せず、P3 の性質テストで見る | `TestActorRefSpec.scala:200-222` |
| I7 の (b)（送った直後に止まっているのを見た）と (c)（待ちの間に止まった）の区別 | §3.2 の表、§4 の I7 | どちらもハンドルの `is_terminated` を1回見る同じ検査になり、`run_one` が1件ずつ運ぶので、テストの側から2つを分ける入力を作れない。実装の手順としては2か所で見るが、テストの区別としては持たない | `TestActorRef.scala:124-133`（原典は `isTerminated` を1回だけ見る） |
| 「(a) 止まった後に読み始めた読み取りは、`send_probe` を呼ばない」を呼び出しの記録で見ること | §5 の「止まった後に読み始める」 | ハンドルの外から `send_probe` の呼び出しを数える口が無い。(1) で止まるので、送ったかどうかは外から見えない。代わりに「(a) の読み取りの後に dead letters のイベントに `ActorControl` が現れない」で見る（P5） | `TestActorRef.scala:126`（停止後の読み取りは例外） |
| `receive` の失敗の文言が `RunOutcome::Failed { message }` の `message` と一致すること | §3.5 の 3.、§4.1 の 3. | `receive` は `() ` を返して panic するだけなので、`message` を読む口がテストに無い。見るのは `#[should_panic(expected = ...)]` で落ちることだけである。文言が `message` であることは実装の約束であり、テストの主張にしない | `TestActorRef.scala:100-117` |
| プローブによる期限の張り直しの有無 | §1 の注4、§4 の I3 | 外から見えるのは「約1秒後に `ReceiveTimeout` が届く」かどうかで、張り直しと最初の予約の区別が時計の精度に依存する。分類（`is_not_influence_receive_timeout` がプローブに `false`）を確かめるだけにする | `Cell.scala` の `NotInfluenceReceiveTimeout`、`ActorCell.scala:568-585` |
| ケース1で入れ子の親（`/user`）を見ること | §1 の注1、§4 の I9 | 外側のアクターが `TestActorRef` を返さず、`ActorRef` の `path().parent()` が `/user` であることは外側の位置から従うだけで、`in_context` が親を選んだことの検査にならない。P1 の `in_context` の抽選と、その中で外側のアクターを `TestActorRef::from_props` で作る形で見る | `TestActorRefSpec.scala:133-136`、`TestActorRef.scala:175-176`（`apply(props, name)` は guardian を supervisor にする） |

## 8. 伺うこと

この文書では決めない。決めるのは上司である。どれも、承認が台帳に記録されるまでは未承認として扱う（移植の規約どおり、`cases.excluded` と `cases.not_built` への記入は人間だけが行う）。下の3件は「除外」ではなく「段7b の後に原典どおりの形で移す」ための保留であり、`cases.excluded`／`cases.not_built` には書かない見込みである。

### 伺い1：ケース3、5、6 を、弱めた版を書かずに段7b の後へ回すこと

| ケース | 原典の行 | 今作れない理由 | Rust で代わりに確かめること（あれば） |
|:---:|:---|:---|:---|
| 3 | `TestActorRefSpec.scala:156-179`。特に `:162-165` の4回の `clientRef ! ...` の直後の `:167` `counter should ===(0)`、`:171-174` の4回の後の `:176` の2回目、`:178` の `assertThread()` | 今の実行器は送信をキューに入れ、運転するまで処理しない。待ちなしで `counter == 0` を見る形は、CallingThreadDispatcher（段7b、#259）が無いと書けない。待ちを入れると即時性と送り手のスレッドでの処理を外した弱いテストになる。送信のたびに実行器を空になるまで回す形は、今の1スレッドの実行器の作りに頼る強さであり、`docs/decisions/pekko-semantics-first.md` が直すと決めた作りのずれに当たる | 本件の範囲では、返信の数え（`counter` が4から0になること）を `FraktorSpec::await_cond` で待って確かめることはできる。ただしこれは弱めた版なので、台帳の `passing` には数えない。テストファイルには書かない（§1 のとおり、3件は冒頭の注釈にだけ書く） |
| 5 | `TestActorRefSpec.scala:200-222`。特に `:217` `boss ! "sendKill"` の直後の `:219` `counter should ===(0)` と `:220` `assertThread()` | 同上。`boss` の処理、子への `Kill`、`boss` の `OneForOneStrategy` による判断、子の `preRestart` と `postRestart` が送ったその場で終わることを見る | 監督戦略の引き継ぎ（`boss` の `OneForOneStrategy` で子が再起動すること）は、本件の `in_context` と §3.4 で書ける。`await_cond` で `counter == 0` を待つ版は弱めた版なので、ケース3と同じく `passing` に数えない |
| 6 | `TestActorRefSpec.scala:224-230`。特に `:227` の注釈 "CallingThreadDispatcher means that there is no delay" と `:228` `f.isCompleted should be(true)` | ask の直後に future が完了していることそのものが本体であり、段7b が無いと作れない | `:229` の `Await.result(f, timeout.duration) should ===("workDone")` は `AskFuture::await_result` で書けるが、本体（即時性）を外した版なので、ケースとしては書かない |
| — | 台帳（`cases.total: 23` のうち `ported: 20`、`passing: 20` とするか） | 原典の Spec ファイルは23件あり、そのうち段7b・`#200` に依存するのはケース3、5、6 の3件である。`ported: 20` と `passing: 20` は、**本件の設計（読取のプローブ・`receive` の直接呼び出し）で段7b を待たずに移せる**件数である。第2段の実装が設計どおりに書けなかった場合（たとえば読み取りの待ちや `receive` の自己停止がケース12・13で成立しない）は、そのケースも unported のまま残し、`ported` と `passing` を実数まで落とす。これは第2段の判断であり、設計の形を弱めない | 第2段の実装の結果で `ported`／`passing` を決める。設計の見込みは 20 である |

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
- **保存済みのデータ**：影響は無い（保存するデータの形を変えない。`ActorHandle` はテストの間だけのメモリ上の値である）。
- **守りの仕組み**：CI の必須チェックは変えない。新しい Quint の仕様は既存の `quint` ジョブが拾う（`scripts/quint-check-specs.sh` の `crates/*/specs/*.qnt` の glob に当たる。この仕様の検査は `quint typecheck`、`quint test`、`quint run --invariant=invariant --max-steps=20 --max-samples=10000` の3つで、§6 の流し方のとおり流せる）。`docs/design/quint-in-ci.md:135-140` の「負の対照（`buggy` で違反が出ること）も要る」は `docs/design/specs/` の範囲外の話として書かれており、本件の `crates/fraktor-actor/specs/` の負の対照は `docs/design/specs/` の形（`fixed`／`buggy` の2モジュール）を使っておらず、1モジュールの `run` で書いた7つはこの範囲の外である。ただし、`crates/*/specs/*.qnt` に負の対照を置くこと自体は `docs/design/quint-in-ci.md` が要求していない。本件が7つ置くのは、設計の穴（停止でプローブが漏れる、番号が戻る、追い越す）を先に落としておくためであり、スクリプトは負の対照の有無を見ない（`scripts/quint-check-specs.sh:43-60` は `run` の名前と件数だけを見る）。
- **秘密**：この設計は秘密の値・鍵・資格情報に触れない。影響は無い。

## 10. 第2段の進め方（この設計が通ってから）

1. red：テストの担当が `crates/fraktor-testkit/tests/test_actor_ref_spec.rs` に20件（23件から段7b のケース3、5、6を除いた残り）を書く。ケース3、5、6 は書かず、冒頭の注釈に段7b の後に移すことをケース名つきで書く。`#[ignore]` の red のテストは置かない。台帳の対象ノードを red にする。失敗が「型や口が無い」ことによることを確かめる。
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

## 12. 敵対的読み（2回目、`a2e77a2`）への回答表

指摘は `pr287-design-adversarial-read-2.md` の番号で引く。「直した」は本文に反映したもの、「採らない」は理由を本文に書いて反映していないもの、「範囲外」は設計の判断ではなく第2段・別件の判断にしたもの。

| 番号 | 中身 | 対応 | どこに書いたか・理由 |
|:---:|:---|:---|:---|
| 1〜5 | 前レビュー H1〜H5 の確認（`:326-327`、`:211-231`、`:186-188`、`:236-248`、`:376-378`） | 範囲外（確認のみ。答えは §11 の H1〜H5） | 第1回の回答を §11 に残した。H3 の 5 は「`with_setup_and_failure_catcher` と `TestKit` の作る順」と「任意の setup を受ける口を足さない」の両立が本文に無い、という確認であり、答えは §3.3 の `FraktorSpec` の変更（`TestKit` を先に作らない）と §3.3 の最後の項（`with_dispatchers` だけを足す）である |
| 6 | 前レビュー H6 のうち `test_actor()` との噛み合い（`:380-394`、`:78`、`:612`） | 範囲外（確認のみ。答えは §11 の H6 と §3.3 の `intercept_any`） | `intercept_any` は `ActorSystem` を借り直さない（`&mut ActorSystem` を1回取る）ので、`FraktorSpec` の `test_actor` と同時に使える。本文は §3.3 の `EventFilter::intercept_any` と §1 の注3 に書いた |
| 7〜16 | 前レビュー H7・M1〜M5・L1〜L4 の確認 | 範囲外（確認のみ。答えは §11 の同じ行） | 第1回の回答を §11 に残した。`:613` の回答表からケース6 が辿れない件は、§11 の L2 と §6 の P5 で辿れる |
| 17 | 伺い1の「`await_cond` で待つ版（書くかどうかも伺う）」と「テストファイルに書かない」、§7.1 の表と §10 の手順1の件数の食い違い | 直した | §8 の伺い1 から「書くかどうかも伺う」を外し、テストファイルには書かないと決めた（§1、§8）。§7.1 の「そのほかの20件」は `ported: 20`、`passing: 20` と書き、§10 の手順1は「20件（23件からケース3、5、6を除いた残り）」と書いた（原典の Spec は `TestActorRef.scala:156-179`、`:200-222`、`:224-230` の3件が段7b に依存し、残り20件が本件で移せる） |
| 18 | `triage.reason` の内容が2つある | 直した | §7.2 の `triage.reason` を「後続の issue へ分け、数からは外さない。弱めた版は書かない」の1つにし、後続 issue の作り方は §7.2 の最後の項にだけ書いた |
| 19 | 伺い3 の「23件に影響しない」の読み、`:117` と `:580` の一貫 | 直した | §8 の伺い3 の最後の行を「TestActorRefSpec の23件には影響しない」に限定した。`:117` の `TestFsmRef` と `:580` の別件の記述は、どちらも「`TestFsmRef` を変えない・別件で扱う」で一貫している（§3.7、§8 の伺い3） |
| 20 | 前レビュー L2 の P1/P4 追加（`:509`、`:512`）と M2 のケース9 を #200 依存に揃える案（`:56`、`:20`） | 範囲外（確認のみ。前回の決定のまま） | P1 と P4 は §6 の表に追加済み。ケース9 は「#200 が main に入ったので今の本体で移す」という前回の決定（§0.1 の 2.、§1 の注4）を採り、`ReceiveTimeoutSetting::Undefined` をプローブで読む形にした（M2 の案「#200 依存で unported」は採らない。理由：原典 `TestActorRefSpec.scala:254-257` はセルの既定値を見るのであって、`ActorContext::receive_timeout()` を呼ぶので、#200 が入った今の本体で `Undefined` を返せる） |
| 21 | 差し替えの持ち主が二重に読める | 直した | §3.1 の生成の口に「差し替えは testkit の1つの非公開の関数で行い、kernel は TestActorRef の規則を知らない」と括り、§3.4 の 1. と §3.3 の非公開の補助を同じ文にそろえた |
| 22 | `supervisor: Option` の4分岐が区別できない | 直した | §3.1 に「実行器が親の参照の種類で分ける」を足し、`Some(guardian)` と `None` は同じ扱いと書いた。§3.4 の4分岐も「親の種類で選ぶ」と明記した |
| 23 | I1 の「等しい」と参照の同一性 | 直した（第2回で根拠を引き直した） | I1 を「パスが等しく、解決した参照は `id` と `generation` の等値で等しい」に直した。`ActorPath::parent` は要素を1つ落とすだけ（`actor_path_value.rs:152-156`）、`resolve_actor_ref` は今の表から返す（`actor_system_value.rs:292`）、`ActorRef` の等値は `id` と `generation`（`actor_ref_value.rs:225-229`）を引いた。§1 の注9 も同じ言葉にそろえた |
| 24 | 読み取りの手順 (1) がプローブを送らない列の区別 | 直した | §3.2 の手順を表にして (2a) を足し、I7 と §5 を3列に直した |
| 25 | `receive` の戻り値の表 | 直した | §3.3 の `receive` と `receive_with_sender` に「失敗したら `RunOutcome::Failed { message }` の文言で panic する」と書いた。表の外の型は `RunOutcome`（既存）だけである |
| 26 | `ActorControl` は `pub(crate)`、`send_probe` は `ActorControl` を取らない | 直した | §3.1 に `send_probe` が `ActorControl` を組み立てて送ることと、送り手を dead letters にすることを書いた。原典の `sender() ! actor`（`TestActorRef.scala:92-98`）の宛先はハンドルが受け持つ |
| 27 | I3 とプローブの張り直しの矛盾 | 直した | I3 を「設定を書き換えない」に限定し、張り直しは原典のとおりと書いた（§1 の注4、§3.1、I3） |
| 28 | I4 の「処理済み」を外から見る口が無い | 直した（弱めた） | I4 を「配送済み」に弱め、§4.1 の 1. に原典の行と理由を書いた |
| 29 | I6 の再起動直後の番号 | 直した（落とした。観測は P3 へ） | I6 から「再起動直後の番号」の**ケースの**テストを外し、§4.1 の 2. に理由を書いた。P3 を「`ActorHandle::processed_sequence()` が再起動の前後で減らない」も見る形に広げ（§6）、Quint の `sequenceMonotonic` でも同じ主張を持つ。ケースで見ない理由は §7.3 の2行目 |
| 30 | I9 のケース1で親を観測する手段が無い | 直した（(b) は確かめないと決めた） | §1 の注1 に、ケース1では入れ子の親を観測しない理由（外側のアクターが `TestActorRef` を返さない。`TestActorRefSpec.scala:133-136`）と、`ActorRef` の `path().parent()` だけでは `in_context` の選択の検査にならないことを書いた。§4 の I9 の行に、P1 の `in_context` の抽選と、その外側のアクターを `TestActorRef::from_props` で作る決め方を書いた。**30(b)（ケース1そのもので親を見る）は `TestActorRef` を返さない原典の形のため確かめない**（§7.3） |
| 31 | `receive` の失敗の文言が読めない | 直した | §3.5 の 3. と §5 の行で、`RunOutcome::Failed { message }` の `message` と定めた。テストで見ない理由は §4.1 の 3. |
| 32 | プローブの横取りで receive timeout を読む口（`:80`、`:274` の `pub(crate)` の変更から辿れない） | 直した | §3.1 の処理の項に、実行器が既に持つ `pub(crate)` の `Executor::receive_timeout_setting(key)`（`executor_value.rs:931-936`）で読むと書いた。§3.1 のファイルの一覧も `executor_value.rs` の `pub(crate)` の変更だけであり、`pub` は増えない |
| 33 | `WeakActorSystem::is_actor_terminated` と `run_one` の `pub`（`:360`、`:280`） | 直した | §3.6 の表の下に「既存の `pub` で足りる（`weak_actor_system.rs:172` の `is_actor_terminated`、`:195` の `run_one`。`ActorSystem::run_one` は `actor_system_value.rs:570`）」を足し、§2 の watch の行にも書いた。`Executor::is_actor_terminated` は `pub(crate)` のままで、testkit は `WeakActorSystem` の `pub` を通る（`weak_actor_system.rs:172-174`） |
| 34 | `directReceive` の `not(reading)` と設計の交差 | 直した | §5 に行を足し、`receive` と読み取りの待ちは同じスレッドで交差しない（どちらも実行器のロックを取る）ため、設計でも交差を認めないと書いた。§6 にも書いた |
| 35 | `bugReadWithoutProbeTest` の狙いの読み | 直した | §6 に、`run` は再起動の後に置き、狙いを再起動後の古い個体の返却に限ると書いた |
| 36 | `restartFails` と `noProbeLeak` の整合 | 範囲外（確認のみ。ずれなし） | — |
| 37 | 負の対照が落ちる対象 | 範囲外（確認のみ） | — |
| 38 | 正6・負7で13件 | 範囲外（確認のみ） | §6 の「13件」は変わらない |
| 39 | I2・I3・I8・I10 が Quint にも PBT にも無い | 直した | §6 に「モデルに持たない番号と、設計での扱い」を足した。I2 は P4、I8 と I9 は P1、I3 は PBT の I3 の観測と注4、I10 は遷移として与える入力。I3・I10 を「Quint で検査する」とは書かない |
| 40 | `probesKeepOrder` と I4 の強さ | 直した | I4 を「配送済み」に弱めた（§4.1 の 1.）。ケースのテストでは見ず、P2 で見る（§7.3 の1行目） |
| 41 | `readNeverStuck` の `phase != "stopped"` と読み手の待ち | 直した | §3.2 の3列と §6 に、どの列がどの遷移（`startRead` の停止後の不積み、`processStop`）で表れるかを書いた |
| 42 | `TestActorRef.scala:27-31` の「sets the receiveTimeout to None」を落とす | 直した | 冒頭と §1 の注4 に引いた |
| 43 | `:82` と `:493` で `InternalGetActor` の扱いが割れる | 直した（第2回で観測の形も直した） | §1 の注5 を、原典の `underlyingActor` が2つの経路を持つこと（`TestActorRef.scala:124-133`）、移植は常にメールボックス経由に狭めること、API 文書に書く約束の範囲（CallingThread の種類だけ）と段7b の後で戻す約束を §3.2 に書いたことに直した。`TestActorRef.scala:27-31` の API 文書は同期を明示しないが、`TestActorRefSpec.scala:128` の「単一スレッド」の前提とケース8（`:249-251`）から、原典の約束は CallingThread の種類だけと読んだ |
| 44 | `:424` の `systemDispatch` に行が無い | 直した | §3.5 の 4. に、`CallingThreadDispatcher.scala:223-230` の `systemDispatch`（`mbox.systemEnqueue` の後に `runQueue`）と、`:232-246` の `dispatch`（CallingThread の mailbox でなければ積むだけ）を引いた |
| 45 | `CallingThreadDispatcher.Id` の実値の照合 | 直した | §3.5 に `CallingThreadDispatcher.scala:136-138`（`val Id = "pekko.test.calling-thread-dispatcher"`）と `reference.conf:31-33`（`calling-thread-dispatcher { type = ...Configurator }` を `pekko.test` の下に定義）を引いた |
| 46 | ケース11 の種類の等値の根拠 | 直した（第2回で対応表の行を `:27` に直した） | §1 の注6 を、原典の `classOf[Dispatcher]`（`:266`、`:21`）と対応表の行（`:27`）を引き、Rust にはクラスの等値が無いので種類の等値に写すと書いた。§5 に狭めの行を足した |
| 47 | `EventFilter[A]()` の既定と `awaitDone` の行 | 直した | §1 の注3 と注8 に `TestEventListener.scala:194-204`、`:107-110`、`:126-138` を引いた |
| 48 | `:74` が引く対応表の null の行 | 直した | §1 の注1 を `docs/decisions/scala-java-to-rust-correspondence.md:10`（「`Props(classOf[…], null)` の `null` 引数」）と `:9-12`（null の行のまとまり。`:9` の購読者、`:11` の StatusReply、`:12` の MDC）を引く形に直した。`:10` の行が「引数の有無は型で決まる」であり、「null を表す値を型の上で作れない」に当たる行である。第2回で `:5` から `:10` に直した |
| 49 | 原典の `Logger` と `:411`／`:479` のログ | 直した | §1 の注8 の最後に、`Logger` と `log.error` は無関係と書いた |
| 50 | 守り（CI のジョブ） | 直した（第2回で行を引き直した） | §9 の守りの仕組みに、スクリプトの3つの検査と `docs/design/quint-in-ci.md:135-140` の範囲外の記述、負の対照の有無をスクリプトが見ないこと（`scripts/quint-check-specs.sh:43-60`）を書いた |
| 51 | 公開 API の表に `is_actor_terminated`・`run_one`・`underlying_snapshot` の feature | 直した | §3.6 の表の下に書いた。`underlying_snapshot` は `#[cfg(feature = "test-support")]` の既定メソッドで、vtable は feature の有無で別のクレートになるため、既定の公開 API では存在しない |
| 52 | 保存データと `cargo test --workspace` | 直した | §9 に保存データの行を分けて書いた |
| 53 | 秘密 | 直した | §9 に秘密の行を足した |
