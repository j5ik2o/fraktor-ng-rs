# 設計：#194 actor の FSMTransitionSpec／LocalActorRefProviderSpec／ActorConfigurationVerificationSpec

[Issue #194](https://github.com/j5ik2o/fraktor-ng-rs/issues/194) の設計の記録である。Pekko v2.0.0（commit `9fbd2283b1cf701c90a1223bf621942c191db61f`）の actor-tests にある `FSMTransitionSpec`、`LocalActorRefProviderSpec`、`ActorConfigurationVerificationSpec` の 3 本を Rust へ移植し、台帳の対象ノード 3 件を green にする。

この文書は第 1 段（設計）の記録であり、第 2 段（実装）は設計レビューを通ってから red → green の順に行う。台帳は第 2 段で更新する。依存する issue は無い。ただし `LocalActorRefProviderSpec` の 1 件だけが複数スレッドの実行器の設計（[issue #234](https://github.com/j5ik2o/fraktor-ng-rs/issues/234)、設計は [PR #236](https://github.com/j5ik2o/fraktor-ng-rs/pull/236)）を前提にするため、その 1 件を分けて書き、残りの 25 件は #234 を待たずに進められるようにする（§1.4）。

進め方の規約は `.takt/facets/knowledge/fraktor-porting-rules.md`、設計文書の前例は [PR #205](https://github.com/j5ik2o/fraktor-ng-rs/pull/205) の [issue-177-frequency-sketch.md](issue-177-frequency-sketch.md) と、直近の [issue-191-specs.md](issue-191-specs.md)、[issue-188-specs.md](issue-188-specs.md) に従う。ノード 1 件＝テストファイル 1 件、ファイル冒頭に出典（Pekko v2.0.0 のファイルパス）をコメントで残す。新しい Rust ファイルには、自作のものも含めて Apache-2.0 のヘッダーを付ける（決まりの見直しは [issue #241](https://github.com/j5ik2o/fraktor-ng-rs/issues/241) で進行中であり、この件では現行の決まりに従う）。

オーナーの指針（`docs/decisions/pekko-semantics-first.md`、#236 で追加中）は、速さより Pekko の意味論を優先することである。JVM・Scala に固有の振る舞いだけを見るケースの作らない・除外の提案は、決めるのはオーナーであるため、この文書では理由を添えた提案として書くだけにし、台帳へは書かない。

## 1. 何を移植するか

### 1.1 件数の数え直し

原典の `" in {"` の数を数え直した。`FSMTransitionSpec` は台帳の目安（7 件）と違い、数え直しは 8 件であり、issue の目安（8 件）と一致する。台帳の目安 7 は誤りであり、第 2 段で台帳を更新するときに `cases.total: 8` として記録する。ほかの 2 本は issue の目安、台帳の目安、数え直しが一致する。

| 原典 | 目安（issue／台帳） | 数え直し |
|:---|---:|---:|
| `actor-tests/src/test/scala/org/apache/pekko/actor/FSMTransitionSpec.scala` | 8／7 | **8** |
| `actor-tests/src/test/scala/org/apache/pekko/actor/LocalActorRefProviderSpec.scala` | 8／8 | **8** |
| `actor-tests/src/test/scala/org/apache/pekko/actor/ActorConfigurationVerificationSpec.scala` | 10／10 | **10** |
| 合計 | 26／25 | **26** |

台帳の対象ノードは次の 3 件で、どれも `status: unported`、`triage.kind: pending` である。

- `actor-tests/src/test/scala/org/apache/pekko/actor/FSMTransitionSpec.scala`（needs: extends-pekko-spec、pekko-spec、test-actor、implicit-sender、expect-msg、expect-no-message、within）
- `actor-tests/src/test/scala/org/apache/pekko/actor/LocalActorRefProviderSpec.scala`（needs: extends-pekko-spec、pekko-spec、test-actor、expect-msg、expect-terminated、event-filter、await-assert）
- `actor-tests/src/test/scala/org/apache/pekko/actor/ActorConfigurationVerificationSpec.scala`（needs: extends-pekko-spec、pekko-spec、default-timeout、event-filter）

移す先はどれも `crates/fraktor-actor/tests/` で、ノード 1 件＝ファイル 1 件である。

- `crates/fraktor-actor/tests/fsm_transition_spec.rs`
- `crates/fraktor-actor/tests/local_actor_ref_provider_spec.rs`
- `crates/fraktor-actor/tests/actor_configuration_verification_spec.rs`

例外が 1 件ある。`LocalActorRefProviderSpec` の「not retain its original Props when terminated」は、原典が `LocalActorRef.underlying.props` という内部の値を読むケースであり、fraktor-ng-rs の `ActorCell` は `pub(crate)` であるため、`tests/` の統合テストからは読めない。規約の例外（内部 API を呼ぶケースは単体テストに置き、台帳の `rust.cases_outside_test_file` に書く）に従い、この 1 件は `crates/fraktor-actor/src/kernel/executor/executor_value_test.rs` の単体テストに置く（§2.2）。

### 1.2 対応表：FSMTransitionSpec（8 件）

移す先は `crates/fraktor-actor/tests/fsm_transition_spec.rs` である。原典のアクターは 3 種類ある。遷移をすべて testActor へ送る `SendAnyTransitionFSM`（状態とデータはどちらも `Int`）、tick で 0 と 1 を往復する `MyFSM`（`whenUnhandled` で reply とリスナー数を答える）、遷移時に前のデータと次のデータを送る `OtherFSM` である。移植のアクターはどれも本体の `Fsm`（§2.1）を `Props` のファクトリの中で組む形にし、原典のコンストラクタの呼び出し（`startWith`、`when`、`onTransition`、`initialize`）をそのまま写す。

| # | 原典のケース | Rust のテスト名 | 移す／作らない |
|:---:|:---|:---|:---|
| 1 | not trigger onTransition for stay | `does_not_trigger_on_transition_for_stay` | 移す。`initialize` の遷移（0→0）は届き、`stay()` では届かず、`goto(0)`（同じ状態への goto）では届くことを確かめる |
| 2 | notify listeners | `notifies_transition_listeners` | 移す（本体の追加が要る。§2.1 の遷移リスナー）。購読すると `CurrentState(fsm, 0)` が届き、tick ごとに `Transition(fsm, 0, 1)`、`Transition(fsm, 1, 0)` が届く |
| 3 | not fail when listener goes away | `does_not_fail_when_a_listener_goes_away` | 移す（本体の追加が要る）。リスナー（forwarder）を止めた後に tick しても FSM は動き続け、何も届かない |
| 4 | remove stopped listeners | `removes_stopped_listeners` | 移す（本体の追加が要る）。リスナーを止めた後、`awaitAssert` 相当でリスナー数が 0 になることを確かめる。原典は `whenUnhandled` の `ListenerCount` で数を聞くため、`when_unhandled` と `replying` の追加が要る（§2.1） |
| 5 | make previous and next state data available in onTransition | `makes_previous_and_next_state_data_available_in_on_transition` | 移す（本体の追加が要る）。`onTransition` の中で前のデータ（0）と次のデータ（1）の組が届く。現行の `on_transition` は状態名しか渡さないため、データを渡す形の追加が要る（§2.1） |
| 6 | trigger transition event when goto() the same state | `triggers_a_transition_event_when_goto_targets_the_same_state` | 移す。同じ状態への `goto` でもリスナーへ `Transition(fsm, 1, 1)` が届き、`onTransition` にも（1, 1）が届く |
| 7 | not trigger transition event on stay() | `does_not_trigger_a_transition_event_on_stay` | 移す。`stay()` ではリスナーへ何も届かない |
| 8 | not leak memory in nextState | `does_not_leak_next_state_data_outside_on_transition` | 移す（形を変える。§2.1）。原典は `onTransition` の中では `nextStateData` を読め、状態関数の中では `IllegalStateException` になることを確かめる。移植は、次データを遷移ハンドラの引数としてだけ渡し、状態関数へは渡さない形にし、「遷移の外では次データが観測できない」ことを型の形で確かめる。実行時の例外を捕まえる形を残すかどうかは、設計レビューで見てほしい点に挙げる（§8） |

ケース 1、6、7 が守る通知の規則（`stay` は通知しない、同じ状態への `goto` は通知する、`initialize` は初期状態への遷移を通知する）は、現行の `Fsm::make_transition` がすでに持つ（`fsm_test.rs` の回帰ケースが一部を固定している）。不足はリスナーとデータと `whenUnhandled` であり、§2.1 に書く。

### 1.3 対応表：LocalActorRefProviderSpec（8 件）

移す先は `crates/fraktor-actor/tests/local_actor_ref_provider_spec.rs` である（ケース 5 だけ §1.1 の例外で単体テストに置く）。原典は設定文字列でスレッドプールの dispatcher を差し替えるが、その設定はケースの振る舞いを決めていない（同時実行のケース 6 の土台であるだけである）。移植は設定文字列を使わず、プログラムから作る通常のシステムで書く。同時実行のケース 6 だけは §1.4 で分ける。

| # | 原典のケース | Rust のテスト名 | 移す／作らない |
|:---:|:---|:---|:---|
| 1 | find child actor with URL encoded name | `finds_a_child_actor_with_a_url_encoded_name` | 移す（本体の追加が要る）。子を URL エンコード済みの名前（`pekko%3A%2F%2FClusterSystem%40127.0.0.1%3A7355`）で作り、その子のパスから解決し直した参照が、停止しておらず、名前が元の文字列と一致することを確かめる。現行にはパスから同期的に解決して `ActorRef` を返す公開 API が無いため、`ActorSystem::resolve_actor_ref` を足す（§2.2） |
| 2 | not handle messages other than those it will act upon（root guardian） | `root_guardian_reports_messages_it_does_not_handle_as_unhandled` | 移す（本体の追加が要る）。`/` への送信が dead letter ではなく `UnhandledMessage` の警告（送信者は dead letters）になることを `EventFilter` 相当で確かめる。現行は guardian のパスが実在のアクターでないため、未解決パスとして dead letter へ流れる（§2.2） |
| 3 | not handle messages other than those it will act upon（user guardian） | `user_guardian_reports_messages_it_does_not_handle_as_unhandled` | 移す（本体の追加が要る）。`/user` についてケース 2 と同じことを確かめる |
| 4 | not handle messages other than those it will act upon（system guardian） | `system_guardian_reports_messages_it_does_not_handle_as_unhandled` | 移す（本体の追加が要る）。`/system` についてケース 2 と同じことを確かめる |
| 5 | not retain its original Props when terminated | `does_not_retain_the_original_props_when_terminated` | 移す（単体テストに置く。§1.1）。子が停止した後、そのセルの `Props` が元の実体ではなく終了用の番兵（Pekko の `ActorCell.terminatedProps`）に置き換わることを確かめる。現行のセルは停止後も元の `Props` を保持するため、置き換えの追加が要る（§2.2） |
| 6 | only create one instance of an actor with a specific address in a concurrent environment | —（#234 の後。§1.4） | この issue では移さない。複数スレッドから同時に同じ名前で作っても成功が 1 件だけで、残りが `InvalidActorNameException` になることを確かめるケースであり、前提は §1.4 に書く |
| 7 | only create one instance of an actor from within the same message invocation | `creates_only_one_instance_from_within_the_same_message_invocation` | 移す（形を変える）。1 回のメッセージ処理の中で同じ名前（`duplicate`）で 2 回作ると、2 回目が `InvalidActorNameException` になる。原典は投げられた例外を `EventFilter` で傍受するが、移植の `actor_of` は例外を投げず `Result` を返すため、アクターが 2 回目の結果を probe へ送って確かめる形にする。例外のログを傍受する形まで写すかどうかは、設計レビューで見てほしい点に挙げる（§8） |
| 8 | throw suitable exceptions for malformed actor names | `rejects_malformed_actor_names_with_suitable_messages` | 一部を作らない提案として挙げ、残りを移す。名前の検証そのもの（`ActorPath::validate_path_element`）はすでにあるが、生成の経路へつながっていないため、つなぐ追加が要る（§2.2）。原典の小ケースのうち、名前が `null` の小ケースだけは、Rust の `&str` が null を表現できないため作らない提案とする（決めるのはオーナー）。空文字、先頭の `$`、不正な `%` エスケープ（`a%`、`%3`、`%xx`、`%0G`、`%gg`、`%`、`%1t`）、`a?`、非 ASCII（`üß`）、引用符・`$`・`>` の位置の表示はすべて移す |

ケース 8 の文面は 2 つの層から来ることに注意する。`Children.checkName` の層（null、空、先頭の `$` についての文面）と、`ActorPath.validatePathElement` の層（`Invalid actor path element [...]、illegal character [...] at position: N` と ASCII の条件）である。原典の期待値は両方の層の文面を含んでおり、移植も 2 層の文面を再現する（§2.2）。

### 1.4 #234 を前提にするケース（LocalActorRefProviderSpec のケース 6）

ケース 6 は、100 個の名前について、4 本の `Future` が同時に `system.actorOf(Props, name)` を呼び、成功（1）と `InvalidActorNameException`（2）の集合がちょうど `{1, 2}` になることを確かめる。守る性質は「同じアドレスのアクターは同時に作っても 1 つだけである」ことであり、その性質が意味を持つのは、生成が実際に複数のスレッドから競合するときだけである。

fraktor-ng-rs の現行の `ActorSystem` は 1 シャード＝1 executor で、executor は `Rc<RefCell<Executor>>` の後ろにあり、システムは `Send` でも `Sync` でもない（#191 の設計で試作により確認済み）。単一スレッドで 4 回続けて作る形に写すと、2 回目以降が名前の重複で失敗するのは自明であり、同時実行の競合（検査と登録の間に割り込まれること）は何も確かめられない。移植したテストを弱める形になるため、この issue では写さない。

置き場所は次のとおりとする。

- このケースは、複数スレッドの実行器の設計（[issue #234](https://github.com/j5ik2o/fraktor-ng-rs/issues/234)、設計は [PR #236](https://github.com/j5ik2o/fraktor-ng-rs/pull/236)）の、アクター生成を複数スレッドから同時に呼べるようになる段の後に置く。#236 の段の番号は、設計レビューで #236 の本文と照らして確定する（この文書では段の番号を断定しない）。
- 台帳の `LocalActorRefProviderSpec` ノードは、第 2 段では `total: 8`、`ported: 7`、`passing: 7` とし、ケース 6 は除外（`cases.excluded`）にも作らない区分（`cases.not_built`）にも書かない。未移植の 1 件として残し、ノードの `status` は green にしない。ほかの 2 ノードは green にする。ノードを green にできないことは issue の完了条件（対象ノードを green にする）との食い違いであるため、設計レビューで扱いを決めてほしい点に挙げる（§8）。案は 2 つある。1 つは、この issue では 2 ノードを green にし、`LocalActorRefProviderSpec` は 7 件移植の状態で残して #234 の後の後続 issue で green にする案である。もう 1 つは、ケース 6 をこの issue の第 2 段から外すことをオーナーが決め、後続 issue へ分ける案である。どちらの案でも、ケース 6 を弱めた形で写すことはしない。
- 残りの 25 件は #234 に依存しない。FSM のケースは単一シャードの決定的な実行で確かめられ、guardian と名前と設定検証のケースも同じである。

### 1.5 対応表：ActorConfigurationVerificationSpec（10 件）

移す先は `crates/fraktor-actor/tests/actor_configuration_verification_spec.rs` である。原典は HOCON の設定文字列で `balancing-dispatcher`（クラス名で BalancingDispatcherConfigurator を指す）と `pinned-dispatcher`（PinnedDispatcher）を定義し、ルーターの pool と組み合わせて `system.actorOf` が `ConfigurationException` を投げるかどうかを見る。

fraktor-ng-rs には、ルーター、dispatcher の登録表、設定の検証、`ConfigurationException` のどれも無い（§2.3）。そのため、この Spec の移植はテストだけではなく、本体の新しい部分（型付きの dispatcher 登録と、型付きのルーター設定と、生成時の検証）を足す作業になる。HOCON の設定文字列とクラス名は写さない。前例（#154 の D1、G0b の決定 E）どおり、設定は型付きの入力にし、dispatcher の種類はクラス名ではなく列挙型で表す。これは移植の忠実さの判断であり、理由は次のとおりである。原典のケースが守る性質は「BalancingDispatcher とルーターの組み合わせは作れない」「存在しない dispatcher を指すと作れない」という意味論であり、クラス名から configurator を読み込む仕組みそのものではない。型付きの入力でも意味論はそのまま確かめられる。逆に、HOCON の文字列解釈を足すと、設定ファイルの解釈という別の本体がこの issue の範囲に入る。

| # | 原典のケース | Rust のテスト名 | 移す／作らない |
|:---:|:---|:---|:---|
| 1 | fail verification with a ConfigurationException if also configured with a RoundRobinPool | `fails_verification_when_a_balancing_dispatcher_is_configured_with_a_round_robin_pool` | 移す（本体の追加が要る。§2.3） |
| 2 | fail verification with a ConfigurationException if also configured with a BroadcastPool | `fails_verification_when_a_balancing_dispatcher_is_configured_with_a_broadcast_pool` | 移す（本体の追加が要る） |
| 3 | fail verification with a ConfigurationException if also configured with a RandomPool | `fails_verification_when_a_balancing_dispatcher_is_configured_with_a_random_pool` | 移す（本体の追加が要る） |
| 4 | fail verification with a ConfigurationException if also configured with a SmallestMailboxPool | `fails_verification_when_a_balancing_dispatcher_is_configured_with_a_smallest_mailbox_pool` | 移す（本体の追加が要る） |
| 5 | fail verification with a ConfigurationException if also configured with a ScatterGatherFirstCompletedPool | `fails_verification_when_a_balancing_dispatcher_is_configured_with_a_scatter_gather_first_completed_pool` | 移す（本体の追加が要る）。pool は `within`（原典は 2 秒）を持つ型付きの値にする |
| 6 | not fail verification with a ConfigurationException also not configured with a Router | `does_not_fail_verification_when_a_balancing_dispatcher_is_used_without_a_router` | 移す。BalancingDispatcher だけの通常のアクターは作れる |
| 7 | not fail verification with a ConfigurationException if also configured with a Router | `does_not_fail_verification_when_a_non_balancing_dispatcher_is_configured_with_a_router` | 移す。ルーターの routee 側に非 balancing の dispatcher（原典の pinned）を指しても作れる |
| 8 | fail verification if the dispatcher cannot be found | `fails_verification_if_the_dispatcher_cannot_be_found` | 移す（本体の追加が要る）。存在しない名前（原典の `"does not exist"`）を指すと `ConfigurationException` になる |
| 9 | fail verification if the dispatcher cannot be found for the head of a router | `fails_verification_if_the_dispatcher_cannot_be_found_for_the_head_of_a_router` | 移す（本体の追加が要る）。ルーター本体（head）の dispatcher が存在しない場合である。原典の文面は routees 向けの文面と同じ形だが、本体（`LocalActorRefProvider.actorOf`）は head 側を別の検査で見るため、移植も検査を分ける（§2.3） |
| 10 | fail verification if the dispatcher cannot be found for the routees of a router | `fails_verification_if_the_dispatcher_cannot_be_found_for_the_routees_of_a_router` | 移す（本体の追加が要る）。routee の dispatcher が存在しない場合である |

検証の入口の形が原典と違う点が 1 つある。原典の `system.actorOf` は例外を投げる。移植の現行の `ActorSystem::actor_of` は `ActorRef` を返し、失敗を返す場所が無い。§2.3 で、検証を行う入口として `try_actor_of` を足し、ケースはそちらで `Err(ConfigurationException)` を確かめる形にする。`actor_of` 自体は検証に失敗したら panic する（Pekko が例外を投げることの写しであり、名前の重複で panic する既存の `spawn_child_from_weak` の前例と同じ扱いである）。

ルーターそのものの振る舞い（routee への振り分け、pool の増減）はこの Spec の対象ではない。ケースが要るのは「ルーター付きの `Props` を作り、検証を通す／落とす」ことだけであるため、第 2 段で足すルーターは、検証と生成の入口までの最小の形とし、振り分けの意味論は別の issue に置く。この線引きは設計レビューで見てほしい点に挙げる（§8）。

## 2. 本体の有無と、足すものの形

### 2.0 既存実装の有無

`crates/fraktor-actor` と `crates/fraktor-testkit` を読んだ結果は次のとおりである。

| 対象 | 有無 | 根拠 |
|:---|:---|:---|
| FSM の状態遷移（`goto`、`stay`、`using`、`onTransition`、`initialize`） | ある | `crates/fraktor-actor/src/kernel/actor/fsm.rs` の `Fsm` と `fsm_state.rs` の `FsmState`。通知の規則（`stay` は通知しない、同じ状態への `goto` は通知する）も `make_transition` にある |
| FSM の遷移リスナー（`SubscribeTransitionCallBack`、`CurrentState`、`Transition`、`UnsubscribeTransitionCallBack`） | **無い** | `fsm.rs` の冒頭が「`SubscribeTransitionCallBack` listeners are not part of it」と明記する。リスナーの集合も、購読への応答も、停止したリスナーの除去も無い |
| FSM の `whenUnhandled` と `stay().replying(...)` | **無い** | `fsm.rs` の冒頭が「`whenUnhandled` ... are not part of it」と明記する。未処理のイベントは既定で現状態を保つだけであり、状態関数が sender へ返信する経路は `FsmState` に無い |
| FSM の `stateData`／`nextStateData`（遷移ハンドラの中で前のデータと次のデータを読む） | **無い** | `on_transition` のハンドラは `&S`（前）と `&S`（次）しか受け取らない。データ `D` は状態関数へだけ渡る |
| 名前の検証（`ActorPath.isValidPathElement`／`validatePathElement`） | ある（ただし生成経路へ未接続） | `crates/fraktor-actor/src/kernel/actor_path/actor_path_value.rs` の `ActorPath::validate_path_element`。位置と不正な文字を返す。ただし `Executor::spawn_named_child_under` は名前の重複と親の終了しか見ず、この検証を呼ばない。システム直下の生成（`spawn_user`）は名前を自動生成するため検証の対象が無い |
| 名前の重複の拒否 | ある | `Executor::spawn_named_child_under` が `InvalidActorNameException::name_taken` を返す。`InvalidActorNameException` には生成理由が名前の重複と親の終了の 2 種類しか無い |
| パスから `ActorRef` を同期的に解決する（`provider.resolveActorRef(path)`） | **無い** | `ActorSystem::actor_selection` は配送時に解決する選択であり、解決した参照を返す API は無い。executor の内部にはパスでセルを引く仕組み（未解決パスの dead letter 振り分け）がある |
| guardian（`/`、`/user`、`/system`）への未処理メッセージの報告 | **無い** | guardian は実在のアクターではなく、パスもセルも無い。guardian のパスへの選択は未解決として dead letter へ流れる。`UnhandledMessage` の発行そのもの（`Executor::publish_unhandled`）と警告文（`unhandled message from <sender>: <message>`）はある |
| 停止したセルの `Props` の解放（`ActorCell.terminatedProps` への置き換え） | **無い** | `crates/fraktor-actor/src/kernel/executor/actor_cell.rs` のセルは再起動のために `props` を保持し続け、停止後も置き換えない |
| testkit（`FraktorSpec`、`TestProbe` の expect 系、`EventFilter`、`await_assert`） | ある | `crates/fraktor-testkit` に `FraktorSpec`、`TestProbe::expect_msg_type`、`expect_no_message`、`watch` と終了の期待、`EventFilter::intercept`、`await_assert` がある。この 3 本の needs はおおむね満たされている |
| stash | ある（容量の上限は無い） | `ActorContext::stash`／`unstash_all`。容量の上限と `StashOverflowException` は未移植であり、文書にもそう書かれている。この 3 本のケースは stash を使わないため、この issue では触れない（§5） |
| ルーター（pool、`Props.withRouter`） | **無い** | `crates/` 全体に router／pool の型が無い。`Props` が持つのは factory と dispatcher 名だけである |
| dispatcher の登録表と存在の検査（`Dispatchers.hasDispatcher`） | **無い** | dispatcher は `Props::with_dispatcher` で名前を記録するだけであり、名前が実在するかどうかを誰も見ない |
| 設定の検証と `ConfigurationException` | **無い** | 型そのものが無い。生成時の検証を行う場所も無い |

### 2.1 FSM に足すもの

置き場所は既存の `crates/fraktor-actor/src/kernel/actor/fsm.rs` と `fsm_state.rs` であり、新しいモジュールは作らない。メッセージ型は `fsm.rs` と同じモジュールに置く。

遷移リスナーのメッセージ型（Pekko の `FSM` のコンパニオンにある型の写し）：

```rust
/// Mirrors `FSM.SubscribeTransitionCallBack`.
pub struct SubscribeTransitionCallBack(pub ActorRef);
/// Mirrors `FSM.UnsubscribeTransitionCallBack`.
pub struct UnsubscribeTransitionCallBack(pub ActorRef);
/// Mirrors `FSM.CurrentState`: sent to a new listener as the reference point.
pub struct CurrentState<S> { pub fsm: ActorRef, pub state: S }
/// Mirrors `FSM.Transition`: sent to listeners on every reported transition.
pub struct Transition<S> { pub fsm: ActorRef, pub from: S, pub to: S }
```

`Fsm` への追加：

```rust
impl<S, D> Fsm<S, D> {
  /// Adds a handler that also receives the previous and next state data.
  /// Mirrors reading `stateData` and `nextStateData` inside `onTransition`.
  pub fn on_transition_with_data<F>(&mut self, handler: F)
  where F: FnMut(&mut ActorContext<'_>, &S, &S, &D, &D) + 'static;

  /// Adds the fallback state function. Mirrors `FSM.whenUnhandled`.
  pub fn when_unhandled<F>(&mut self, function: F)
  where F: FnMut(&mut ActorContext<'_>, &Message, &D) -> Option<FsmState<S, D>> + 'static;

  /// Returns the number of transition listeners. (test support for
  /// `listeners.size`, read through `when_unhandled` in the cases)
  fn listener_count(&self) -> usize;
}
```

決まりは次のとおりである。

- リスナーは `Fsm` が `Vec<ActorRef>` で持つ。`SubscribeTransitionCallBack` は、`Fsm` がメッセージとして受ける。現行の `Fsm::receive` は状態関数へだけ振るため、購読・解除のメッセージは状態関数より先に `receive` の中でさばく（Pekko の `FSM.receive` がそうする）。購読した相手へは、その場で `CurrentState` を送る。重複した購読は 1 件として数える（Pekko の `listeners` は集合である）。
- 遷移の通知は、`make_transition` が通知する条件（`goto`、または状態が変わる場合）と同じ条件で、リスナー全員へ `Transition` を送る。`stay` では送らない。通知の順序は、`onTransition` ハンドラの実行と同じ遷移の中で行う。
- リスナーの停止の検出は DeathWatch を使う。購読時に `context.watch(listener)` で監視し、`Terminated` を受けたらリスナーを外して監視も外す（Pekko の `ListenerTerminated` と `watchedListeners` の写し）。ケース 3 と 4 はこの経路で成り立つ。すでに FSM が別の理由で監視している相手については、Pekko は警告を出して自動除去をしないが、移植の `Fsm` は FSM の外の監視を知る手段が無いため、この分岐は写さない（ケースにも無い）。
- `when_unhandled` は、現在の状態の状態関数がすべて `None` を返したときにだけ呼ばれる。現行の「未処理なら現状態を保つ」既定は、`when_unhandled` が登録されていない場合の振る舞いとして残る。
- `FsmState` に返信を足す。Pekko の `State.replying(msg)` の写しである。

```rust
impl<S, D> FsmState<S, D> {
  /// Mirrors `FSM.State.replying(msg)`: after the transition, sends `msg`
  /// to the sender of the event being handled.
  pub fn replying<M: core::any::Any + Send + Sync>(mut self, message: M) -> Self;
}
```

返信は `make_transition` の後、イベントの sender へ送る。返信を持つ `FsmState` が `stay` と組になっている場合も同じである（ケース 4 の `ListenerCount` への応答がこの形である）。

- データの渡し方。ケース 5 は `onTransition` の中で前のデータと次のデータの組を読む。現行の `on_transition`（状態名だけ）を壊さず、`on_transition_with_data` を足す。2 種類のハンドラは同じ遷移で、登録した順に呼ばれる。`make_transition` は、遷移前のデータと遷移後のデータの両方をその間だけ保持してハンドラへ渡す。
- ケース 8（`nextStateData` の漏れ）への形。次データは `on_transition_with_data` の引数としてだけ存在し、遷移が終わればハンドラの外からは読めない。状態関数は現在のデータしか受け取らない。したがって「遷移の外で次データを読むと例外になる」という原典の性質は、移植では「遷移の外では次データを指す値そのものが無い」という形になる。ケース 8 はこの形で書き、原典との形の違いをテストファイルの冒頭に記録する。実行時の例外（panic）で確かめる形にするかどうかは §8 に挙げる。
- `no_std`：追加はすべて `core` と `alloc` で書ける。リスナーは `alloc::vec::Vec`、メッセージ型は通常の構造体であり、`std` の機能は要らない。

### 2.2 LocalActorRefProvider に足すもの

provider という型は作らない。fraktor-ng-rs には `LocalActorRefProvider` に当たる型が無く、provider の仕事（生成、解決、guardian）は `Executor` と `ActorSystem` が分け持つ。原典が `provider.resolveActorRef` と呼ぶ場所は、移植では `ActorSystem` の公開 API として足す。provider をクラス名で選ぶ仕組み（`pekko.actor.provider` の設定）は JVM の設定ファイルに固有のものであり、この Spec のケースは見ていないため、足さない。

`ActorSystem` への追加：

```rust
impl ActorSystem {
  /// Mirrors `ActorRefProvider.resolveActorRef(path)`: returns the actor
  /// at `path`, or the dead letters handle when nothing lives there.
  pub fn resolve_actor_ref(&self, path: &ActorPath) -> ActorRef;

  /// Starts a top-level actor under `name`. Mirrors
  /// `ActorSystem.actorOf(props, name)`; the name goes through the same
  /// validation and uniqueness checks as a child name.
  pub fn actor_of_named(&mut self, props: Props, name: &str)
    -> Result<ActorRef, InvalidActorNameException>;
}
```

決まりは次のとおりである。

- `resolve_actor_ref` は、executor がパスでセルを引く既存の仕組みを使い、生きているセルがあればその参照を返し、無ければ dead letters の参照を返す（Pekko の `resolveActorRef` が見つからない場合に dead letters を返すことの写し）。URL エンコード済みの名前は、エンコード済みの文字列のまま要素として保持される（現行の `ActorPath` は要素をデコードしない）ため、ケース 1 の名前はそのまま一致する。
- guardian の未処理の報告。executor が選択の解決をする場所で、解決先が root（要素なし）、`user`、`system` の guardian のパスそのものである場合は、dead letter へ流さず、その guardian を宛先とする `UnhandledMessage` を発行する。送信者は原典どおり dead letters であり、警告文は既存の `unhandled message from <sender>: <message>` の形を使う。guardian の下の実在の子への配送は変えない。
- 名前の検証の接続。`Executor::spawn_named_child_under` と、新しい `actor_of_named` の経路で、重複の検査の前に名前を検証する。検証は既存の `ActorPath::validate_path_element` を使い、失敗は新しい生成子で `InvalidActorNameException` にする。

```rust
impl InvalidActorNameException {
  /// Creates the error for a name that fails validation, carrying the
  /// validation message (the `ActorPath` layer of Pekko's messages).
  pub fn invalid_name(message: String) -> Self;
}
```

文面は 2 層を再現する。生成の入口（Pekko の `Children.checkName` の層）では、空文字は「actor name must not be empty」、先頭の `$` は「actor name must not start with `$`」とする。要素の検証の層（`ActorPath.validatePathElement` の層）では、既存の `validate_path_element` の文面（`Invalid actor path element [...], illegal character [...] at position: N` と、ASCII のみという条件の文）を使う。現行の `validate_path_element` は先頭の `$` も要素の層の文面で返すため、入口の層と要素の層のどちらが先に見るかを決める必要がある。原典のケース 8 の期待値は、先頭の `$` の名前（`$hallo`）では入口の層の文面（`not start with $`）を、文中の `$`（`$hello` は先頭であるため入口の層ではなく要素の層の位置表示 `[ $ ] at position: 0`）ではなく、ケースの並びどおりに両方を求める。実装では、入口の層は「空」と「先頭が `$` で、かつ要素の層でも位置 0 の `$` として報告される場合」を原典の期待値と照らして割り当て、第 2 段の red の段階で期待値ごとに固定する。この割り当ての確定は設計レビューで見てほしい点に挙げる（§8）。
- null の小ケースは作らない提案である（§1.3）。Rust では名前は `&str` であり、null は型で表現できない。`Option<&str>` を受ける入口をこのケースのためだけに足すことはしない。
- 停止したセルの `Props` の解放。executor がセルを停止させる経路で、watcher へ `Terminated` を送った後に、そのセルの `props` を終了用の番兵へ置き換える。番兵は、作り直しを試みると失敗する factory を持つ `Props` とし、`ActorCell` の側に置く（Pekko の `ActorCell.terminatedProps` の写し）。再起動の経路では置き換えない（再起動は元の `Props` が要る）。ケース 5 は単体テストで、セルの `props` が元の実体と同一でなく、番兵と同一であることを確かめる。番兵の同一性は、番兵を 1 つの共有実体として置くことで確かめられる形にする（`Props` は現在 `Arc` の factory を持つため、番兵の factory の `Arc` の同一性で比べる）。
- `no_std`：追加はすべて `core` と `alloc` で書ける。解決と検証は文字列と `Vec` の操作だけであり、`std` の機能は要らない。

### 2.3 設定の検証に足すもの

置き場所は新しいモジュール `crates/fraktor-actor/src/kernel/routing.rs`（ルーター設定と pool）と、`crates/fraktor-actor/src/kernel/actor/configuration_exception.rs`（例外型）、dispatcher の登録は `ActorSystem` の生成時の型付き入力とする。

例外型（既存の `InvalidActorNameException` と同じ形で、`core` と `alloc` で書く）：

```rust
/// Mirrors `org.apache.pekko.ConfigurationException`.
#[derive(Clone, PartialEq, Eq)]
pub struct ConfigurationException { cause: String }
impl ConfigurationException {
  pub fn new(message: String) -> Self;
}
impl core::fmt::Display for ConfigurationException { /* writes the cause */ }
impl core::error::Error for ConfigurationException {}
```

dispatcher の型付き登録：

```rust
/// The kind of a dispatcher, as the verification rules see it.
/// Mirrors the distinction Pekko draws from the configurator class:
/// a balancing dispatcher shares one mailbox between its actors.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DispatcherKind { Balancing, Pinned, Default }

/// One dispatcher definition handed to the system as a typed value.
/// Replaces the HOCON block and the configurator class name of the original.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DispatcherDefinition { pub id: String, pub kind: DispatcherKind }

impl ActorSystem {
  /// Creates a system with typed dispatcher definitions, in addition to
  /// the built-in default dispatcher.
  pub fn with_dispatchers(name: &str, dispatchers: &[DispatcherDefinition]) -> Self;
}
```

dispatcher の実行そのもの（スレッドプール、mailbox の共有）はこの issue では作らない。現行の移植が dispatcher 名を「記録するだけ」としている段階に合わせ、登録表は「名前が存在するか」と「balancing かどうか」だけを答える。存在しない名前と balancing の判定が、この Spec の検証の意味論のすべてである。dispatcher の実行を作る段では、この登録表に実行の実体を結び付ける。

ルーター設定：

```rust
/// Mirrors the pool router configurations of `org.apache.pekko.routing`.
#[derive(Clone, PartialEq, Debug)]
pub enum PoolKind {
  RoundRobin,
  Broadcast,
  Random,
  SmallestMailbox,
  ScatterGatherFirstCompleted { within: Duration },
}

/// Mirrors `RouterConfig` as the verification rules see it: a pool, the
/// number of routees, and the dispatcher of the router head.
#[derive(Clone, PartialEq, Debug)]
pub struct RouterConfig {
  pub pool: PoolKind,
  pub nr_of_instances: usize,
  pub router_dispatcher: Option<String>,
}

impl Props {
  /// Mirrors `Props.withRouter(router)`.
  pub fn with_router(mut self, router: RouterConfig) -> Self;
  /// Returns the router the recipe records, when one was set.
  pub fn router(&self) -> Option<&RouterConfig>;
}
```

検証の入口：

```rust
impl ActorSystem {
  /// Starts an actor after verifying its configuration.
  /// Mirrors the checks `LocalActorRefProvider.actorOf` and
  /// `RoutedActorRef` perform, which throw `ConfigurationException`.
  pub fn try_actor_of(&mut self, props: Props)
    -> Result<ActorRef, ConfigurationException>;
}
```

検証の規則は、Pekko の本体（`LocalActorRefProvider.actorOf` と `RoutedActorRef` のコンストラクタ）の順で書く。

1. ルーターが無い場合：`Props` の dispatcher 名（未指定なら既定）が登録表に無ければ、`Dispatcher [<name>] not configured for path <path>` で `ConfigurationException` とする。dispatcher の種類は問わない（ケース 6 と 8）。
2. ルーターがある場合：routee 側の dispatcher 名が登録表に無ければ、`Dispatcher [<name>] not configured for routees of <path>` とする（ケース 10）。head 側の dispatcher 名（`router_dispatcher`、未指定なら既定）が登録表に無ければ、head 向けの文面で `ConfigurationException` とする（ケース 9）。
3. ルーターがあり、head の dispatcher が balancing の場合：`RoutedActorRef` の検査により `ConfigurationException` とする（ケース 1〜5）。原典のケース 1〜5 は、pool の dispatcher を routee 側ではなくルーター全体の dispatcher として `RoundRobinPool(2).withDispatcher("balancing-dispatcher")` と書く。Pekko では pool の `withDispatcher` は head の dispatcher を指すため、移植でも pool 側の `withDispatcher` 相当は head の dispatcher として記録する形にする。ケース 7 は head が既定（非 balancing）で routee が pinned のため通る。
4. 検証を通った場合：アクターを作る。ルーター付きの場合は head のアクターを作り、routee は `nr_of_instances` の数だけ作る。振り分けはしない（§1.5 の線引き）。

`ActorSystem::actor_of` は `try_actor_of` を呼び、失敗したらその文面で panic する。`ActorContext::actor_of` の経路（子アクターの生成）にも同じ検証を入れるかどうかは、この Spec のケースがシステム直下しか見ていないため、第 2 段ではシステム直下だけに留め、子への適用は後続に置くことを設計レビューで見てほしい点に挙げる（§8）。

- `no_std`：追加はすべて `core` と `alloc` で書ける。登録表は `Vec<DispatcherDefinition>`、`Duration` は `core::time::Duration` である。

## 3. 守る性質（不変条件）

### FSM の遷移とリスナー

- F1：`stay()` は遷移を通知しない。`onTransition` ハンドラにも、遷移リスナーにも届かない。
- F2：同じ状態への `goto` は遷移を通知する。状態名が等しくても、ハンドラとリスナーへ届く。
- F3：`initialize()` は初期状態から初期状態への遷移を 1 回だけ通知する。
- F4：購読したリスナーへは、購読の直後に `CurrentState` が 1 回届き、それ以降の通知される遷移ごとに `Transition` がちょうど 1 回ずつ、遷移の順序どおりに届く。重複して購読しても二重には届かない。
- F5：停止したリスナーは、次の遷移までに集合から外れる。停止したリスナーへの送信で FSM が失敗することはない。
- F6：遷移ハンドラの中で読める前のデータは遷移前のデータであり、次のデータは遷移後のデータである。`using` で差し替えた場合は差し替え後の値が次のデータになる。
- F7：次データは遷移の間だけ存在する。遷移の外（状態関数の中、通常のメッセージ処理の中）では、次データを指す値が無い。
- F8：状態関数がすべて断ったイベントは `whenUnhandled` へ渡り、`whenUnhandled` が断れば現状態と現データを保つ。`replying` の返信は、遷移の後に、イベントの sender へちょうど 1 回届く。

### 生成、解決、guardian、名前

- L1：生きている子のパスを解決すると、その子自身の参照が返る。名前が URL エンコード済みの文字列でも、要素はエンコード済みのまま一致する。
- L2：存在しないパスを解決すると dead letters の参照が返り、panic しない。
- L3：guardian（`/`、`/user`、`/system`）へのメッセージは dead letter にならず、`UnhandledMessage` として発行される。guardian の下の実在の子へのメッセージは通常どおり届く。
- L4：子が停止した後は、そのセルの `Props` は元の実体ではなく終了用の番兵である。再起動したセルの `Props` は元のままである。
- L5：同じ親の下で、生きている子と同じ名前は作れない。子が停止すれば同じ名前を再び使える（既存の性質であり、検証の接続で壊さない）。
- L6：不正な名前は、生成の入口すべて（親からの生成、システム直下の名前付き生成）で、アクターを作る前に拒否される。拒否された名前の子は残らない。
- L7：名前の検証は、Pekko の 2 層の文面のどちらかを含む文面で拒否する。空、先頭の `$`、不正な `%` エスケープ、非 ASCII、禁止文字とその位置が、原典の期待値どおりに伝わる。

### 設定の検証

- C1：ルーターと balancing の head dispatcher の組み合わせは、pool の種類によらず作れない。
- C2：ルーターが無ければ、balancing の dispatcher でも作れる。
- C3：ルーターがあり、head と routee の dispatcher がどちらも存在し、head が balancing でなければ作れる。
- C4：存在しない dispatcher 名は、指す場所（通常のアクター、head、routee）によらず作れない。文面は指す場所を区別する。
- C5：検証に失敗した生成は、アクターを 1 つも作らない（head だけが残る、routee の一部だけが残る、という状態を作らない）。
- C6：dispatcher 未指定は既定の dispatcher を指すものとして扱い、常に存在する。

## 4. 壊しうる入力

- 名前：空文字、1 文字の `$`、`$` で始まる名前、末尾の `%`、`%` の後に 16 進数でない文字（`%xx`、`%0G`、`%gg`、`%1t`）、`%3` のような不足したエスケープ、正しいエスケープ（`%3A`、`%2F`、`%40`）を含む長い名前、非 ASCII の文字（`üß`）、引用符と `>` のような禁止文字とその位置、非常に長い名前（検証が長さで失敗したり、切り詰めたりしないこと）、`%` の連続（`%%`、`%%%41`）。
- パス：要素が無いパス（root）、guardian のパスちょうど、guardian の下の存在しない子、dead letters 自身のパス、解決の対象が解決の間に停止した場合。
- FSM：購読を送る前に停止したリスナー、同じリスナーの二重の購読、購読の直後に解除、遷移の最中に止まるリスナー、データが大きい値の場合の `using` の差し替え、状態関数が登録されていない状態への `goto`（既存の振る舞いを壊さないこと）、`whenUnhandled` が `replying` 付きの `stay` を返す場合の sender が dead letters の場合。
- 設定：dispatcher 名が空文字、空白だけの名前（`"does not exist"` のように空白を含む名前）、pool の個数が 0、`ScatterGatherFirstCompleted` の `within` が 0 と極端に大きい値、head と routee に同じ存在しない名前を指した場合にどちらの文面が出るか（head を先に見る順序で固定する）、登録表に同じ id を 2 回登録した場合（後の定義が勝つか、拒否するかは第 2 段で固定し、テストに書く）。

## 5. Quint を書くかどうか

**FSM の遷移リスナーには Quint を書く。ほかの 2 本には書かない。**

FSM に書く理由：

- リスナーの集合は、購読、解除、リスナーの停止、遷移という 4 種類の出来事で変わる状態であり、守る性質（F4、F5）は 1 回の遷移ではなく遷移の列の全体で成り立つ。特に「停止したリスナーが次の遷移までに外れる」ことは、停止の通知と次の遷移の順序に依存し、単純なケースでは通り、順序を取り違えた実装でもケース 3 と 4 の写し方によっては通ってしまう余地がある。FrequencySketch の前例（PR #205）でも、誤りの種類によっては原典のケースと PBT を通り、Quint だけが落とした。
- モデルは小さくできる。状態は、現在の状態名（0 と 1 の 2 値で足りる）、リスナーの集合、各リスナーが生きているかどうか、最後に通知した遷移である。遷移は `subscribe`、`unsubscribe`、`stopListener`、`tick`（goto）、`stay` の 5 つである。
- 置き場所は `crates/fraktor-actor/specs/fsm_transition.qnt` の 1 ファイル、1 モジュールにする。CI（`scripts/quint-check-specs.sh`）が見るのは `crates/*/specs/*.qnt` であり、#188 の設計レビューで確定した形に従う。不変条件は F1（`stay` は通知しない）、F2（同じ状態への `goto` は通知する）、F4（生きている購読者へは遷移ごとにちょうど 1 回）、F5（停止したリスナーは通知の宛先に入らない）を書く。負の対照は、`stay` でも通知する誤りと、停止したリスナーを外さない誤りを用意し、不変条件で落ちることを確かめる。
- モデルにしないもの：データ `D` の中身（F6 は Rust のテストで確かめる）、実時間（この Spec の遷移は時間に依存しない）。

ほかの 2 本に書かない理由：

- 名前の検証と設定の検証は、どちらも入力から結果が一意に決まる純粋な判定であり、履歴を持つ状態遷移ではない。その性質は、入力の組み合わせを一般化する PBT（§6）が直接確かめる。Quint で書いても同じ照合を別の記法で書くことになる。
- guardian の報告と `Props` の解放は、状態の列ではなく「停止の後に番兵である」「guardian への送信は unhandled になる」という 1 段の性質であり、原典のケースと単体テストで足りる。
- stash について：この 3 本のケースは stash を使わない。現行の stash には容量の上限が無く（`stash-capacity` と `StashOverflowException` は未移植）、あふれたときの振る舞い自体が存在しないため、この issue で Quint の対象にはしない。stash の容量を移植する issue が立つときに、その設計で判断する。

## 6. PBT を足すか

**足す。対象は名前の検証と設定の検証であり、FSM は Quint と原典のケースで足りるため足さない。**

- 性質 1（名前の検証）：ランダムな文字列（ASCII の英数字、記号、`%` と 16 進数、非 ASCII、空文字を混ぜる）について、`validate_path_element` の結果が、テスト側に書いた素朴な規則（空でない、先頭が `$` でない、すべての文字が許可文字か正しい `%` エスケープである）による判定と一致すること。置き場所は `crates/fraktor-actor/tests/local_actor_ref_provider_property_test.rs` とする。
- 性質 2（設定の検証の行列）：pool の有無と種類、head と routee の dispatcher の種類（balancing、非 balancing、存在しない、未指定）のすべての組み合わせについて、検証の結果が §2.3 の規則 1〜3 から作った表と一致し、失敗した場合にアクターが増えていないこと（C5）。置き場所は `crates/fraktor-actor/tests/actor_configuration_verification_property_test.rs` とする。
- 乱数は固定 seed の SplitMix64 とし、失敗時は seed と入力を表示する。外部の依存は足さない（#188 の前例と同じ）。
- FSM に PBT を足さない理由：通知の規則は Quint が遷移の列で確かめ、データの受け渡し（F6）は決まったケースで足りる。ランダムなイベント列とモデルを比べる PBT は、Quint のモデルと二重になる。

## 7. 影響

- 既存の公開 API の変更：`Fsm` へのメソッドの追加（`on_transition_with_data`、`when_unhandled`）、`FsmState::replying` の追加、遷移リスナーのメッセージ型の追加、`ActorSystem::resolve_actor_ref` と `actor_of_named` と `try_actor_of` と `with_dispatchers` の追加、`Props::with_router` と `router` の追加、`InvalidActorNameException::invalid_name` の追加、`ConfigurationException`、`DispatcherDefinition`、`DispatcherKind`、`RouterConfig`、`PoolKind` の追加。既存のメソッドの署名は変えない。
- 振る舞いの変更：名前の検証が生成経路へつながるため、これまで通っていた不正な名前の生成が失敗するようになる。既存のテストが不正な名前を使っていないかは、第 2 段の red の前に `cargo test --workspace` で確かめる。guardian のパスへの送信が dead letter から `UnhandledMessage` へ変わるため、dead letter の件数を数える既存のテスト（DeadLetter 系の Spec）への影響を同じく確かめる。
- 台帳：第 2 段で対象ノード 3 件を更新する。`FSMTransitionSpec` は total／ported／passing を 8／8／8、`ActorConfigurationVerificationSpec` は 10／10／10 とし、`rust.crate` に `fraktor-actor`、`rust.test_file` に各テストファイル、`status: green`、`verified_against: v2.0.0` を書く。`LocalActorRefProviderSpec` の扱いは §1.4 の判断に従う。ケース 5 は `rust.cases_outside_test_file` に単体テストの置き場所を書く。

## 8. レビューで見てほしい点

1. `LocalActorRefProviderSpec` のケース 6（同時実行）を #234 の後に置くことと、それに伴う台帳ノードの扱い（§1.4 の 2 案のどちらにするか）。この issue の完了条件（対象ノードを green にする）と食い違うため、判断が要る。
2. 名前が `null` の小ケースを作らない提案（§1.3、§2.2）。Rust の型で表現できないことが理由であり、決めるのはオーナーである。
3. ケース 8 の文面の 2 層の割り当て（§2.2）。入口の層（`Children.checkName`）と要素の層（`ActorPath.validatePathElement`）のどちらが先頭の `$` を報告するかで、期待値の写し方が変わる。
4. FSM のケース 8（`nextStateData`）を、実行時の例外ではなく型の形で確かめること（§2.1）。原典の「遷移の外で読むと `IllegalStateException`」を、panic を投げる公開 API として残すかどうか。
5. FSM のケース 7（LocalActorRefProviderSpec）で、例外を `EventFilter` で傍受する形を、`Result` を probe へ送る形に変えること（§1.3）。
6. 設定の検証で足すルーターを、検証と生成の入口までの最小の形に留めること（§1.5）。振り分けの意味論をこの issue に含めるかどうか。
7. 設定の検証を `ActorContext::actor_of`（子の生成）へ広げるかどうか（§2.3）。この Spec のケースはシステム直下だけである。
8. dispatcher の実行を作らず、登録表は存在と種類だけを答える段階に留めること（§2.3）。

## 9. 第 2 段ですること（本書の範囲の外）

- red（失敗するテスト）→ green の順に実装する。先に 3 つの spec ファイル（と単体テスト 1 件、PBT 2 件、Quint 1 件）を書き、コンパイルの失敗かテストの失敗で red を確認してから本体を足す。#234 を前提にするケース 6 は書かない。
- 台帳の対象ノードを §7 のとおりに更新する。
- CI と同じ検査を流す（`cargo fmt --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`、ledger/tools の Python テスト、Quint は CI の `scripts/quint-check-specs.sh` をそのまま流し、`crates/fraktor-actor/specs/fsm_transition.qnt` が対象に入っていることを確かめる）。
- PR をドラフトから外し、マージできる状態まで持つ。マージはしない。
