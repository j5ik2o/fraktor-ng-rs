# 設計：#194 actor の FSMTransitionSpec／LocalActorRefProviderSpec／ActorConfigurationVerificationSpec

[Issue #194](https://github.com/j5ik2o/fraktor-ng-rs/issues/194) の設計の記録である。Pekko v2.0.0（commit `9fbd2283b1cf701c90a1223bf621942c191db61f`）の actor-tests にある `FSMTransitionSpec`、`LocalActorRefProviderSpec`、`ActorConfigurationVerificationSpec` の 3 本を Rust へ移植する。完了条件は、元帥の決定（2026-11-26、§1.4）により「2 ノードを green にし、`LocalActorRefProviderSpec` は 8 件中 7 件を移す」である。

この文書は第 1 段（設計）の記録であり、第 2 段（実装）は設計レビューを通ってから red → green の順に行う。台帳は第 2 段で更新する。依存する issue は無い。ただし `LocalActorRefProviderSpec` の 1 件だけが複数スレッドの実行器の設計（[issue #234](https://github.com/j5ik2o/fraktor-ng-rs/issues/234)、設計は [PR #236](https://github.com/j5ik2o/fraktor-ng-rs/pull/236)）を前提にするため、その 1 件を分けて書き、残りの 25 件は #234 を待たずに進められるようにする（§1.4）。

進め方の規約は `.takt/facets/knowledge/fraktor-porting-rules.md`、設計文書の前例は [PR #205](https://github.com/j5ik2o/fraktor-ng-rs/pull/205) の [issue-177-frequency-sketch.md](issue-177-frequency-sketch.md) と、直近の [issue-191-specs.md](issue-191-specs.md)、[issue-188-specs.md](issue-188-specs.md) に従う。ノード 1 件＝テストファイル 1 件、ファイル冒頭に出典（Pekko v2.0.0 のファイルパス）をコメントで残す。新しい Rust ファイルには、自作のものも含めて Apache-2.0 のヘッダーを付ける（決まりの見直しは [issue #241](https://github.com/j5ik2o/fraktor-ng-rs/issues/241) で進行中であり、この件では現行の決まりに従う）。

オーナーの指針（`docs/decisions/pekko-semantics-first.md`、承認済み（accepted）で main に入っている。#236 はマージ済みである）は、速さより Pekko の意味論を優先することである。kernel の `Arc` は別名 `kernel::sync::Arc` を使う決まり（main の現行の規約）に従う。JVM・Scala に固有の振る舞いだけを見るケースの作らない・除外の提案は、決めるのはオーナーであるため、この文書では理由を添えた提案として書くだけにし、台帳へは書かない。

## 1. 何を移植するか

### 1.1 件数の数え直し

原典の `" in {"` の数を数え直した。`FSMTransitionSpec` は台帳の目安（7 件）と違い、数え直しは 8 件であり、issue の目安（8 件）と一致する。台帳の目安 7 は誤りではなく、旧基準 v1.7.1 の件数である（v2.0.0 で「remove stopped listeners」が足された）。第 2 段で台帳を更新するときに、基準の版が v2.0.0 であることに合わせて `cases.total: 8` として記録する。ほかの 2 本は issue の目安、台帳の目安、数え直しが一致する。

| 原典 | 目安（issue／台帳） | 数え直し |
|:---|---:|---:|
| `actor-tests/src/test/scala/org/apache/pekko/actor/FSMTransitionSpec.scala` | 8／7 | **8** |
| `actor-tests/src/test/scala/org/apache/pekko/actor/LocalActorRefProviderSpec.scala` | 8／8 | **8** |
| `actor-tests/src/test/scala/org/apache/pekko/actor/ActorConfigurationVerificationSpec.scala` | 10／10 | **10** |
| 合計 | 26／25 | **26** |

台帳の対象ノードは次の 3 件で、どれも `status: unported`、`triage.kind: pending` である。

- `actor-tests/src/test/scala/org/apache/pekko/actor/FSMTransitionSpec.scala`（台帳の needs は v1.7.1 からの抽出値であり、extends-pekko-spec、pekko-spec、test-actor、implicit-sender、expect-msg、expect-no-message、within が書かれている。v2.0.0 の原典はケース 3 と 4 で watch、expect-terminated、await-assert も使うため、第 2 段で needs に足す）
- `actor-tests/src/test/scala/org/apache/pekko/actor/LocalActorRefProviderSpec.scala`（needs: extends-pekko-spec、pekko-spec、test-actor、expect-msg、expect-terminated、event-filter、await-assert）
- `actor-tests/src/test/scala/org/apache/pekko/actor/ActorConfigurationVerificationSpec.scala`（needs: extends-pekko-spec、pekko-spec、default-timeout、event-filter）

移す先はどれも `crates/fraktor-actor/tests/` で、ノード 1 件＝ファイル 1 件である。

- `crates/fraktor-actor/tests/fsm_transition_spec.rs`
- `crates/fraktor-actor/tests/local_actor_ref_provider_spec.rs`
- `crates/fraktor-actor/tests/actor_configuration_verification_spec.rs`

当初の設計では、`LocalActorRefProviderSpec` の「not retain its original Props when terminated」を単体テストに置く案だったが、独立レビュー（H2）が前提の誤りを確かめたため、その案は取り下げた。この 1 件も `tests/` の結合テストに置く（§1.3、§2.2）。

### 1.2 対応表：FSMTransitionSpec（8 件）

移す先は `crates/fraktor-actor/tests/fsm_transition_spec.rs` である。原典のアクターは 3 種類ある。遷移をすべて testActor へ送る `SendAnyTransitionFSM`（状態とデータはどちらも `Int`）、tick で 0 と 1 を往復する `MyFSM`（`whenUnhandled` で reply とリスナー数を答える）、遷移時に前のデータと次のデータを送る `OtherFSM` である。移植のアクターはどれも本体の `Fsm`（§2.1）を `Props` のファクトリの中で組む形にし、原典のコンストラクタの呼び出し（`startWith`、`when`、`onTransition`、`initialize`）をそのまま写す。

| # | 原典のケース | Rust のテスト名 | 移す／作らない |
|:---:|:---|:---|:---|
| 1 | not trigger onTransition for stay | `does_not_trigger_on_transition_for_stay` | 移す。`initialize` の遷移（0→0）は届き、`stay()` では届かず、`goto(0)`（同じ状態への goto）では届くことを確かめる |
| 2 | notify listeners | `notifies_transition_listeners` | 移す（本体の追加が要る。§2.1 の遷移リスナー）。購読すると `CurrentState(fsm, 0)` が届き、tick ごとに `Transition(fsm, 0, 1)`、`Transition(fsm, 1, 0)` が届く |
| 3 | not fail when listener goes away | `does_not_fail_when_a_listener_goes_away` | 移す（本体の追加が要る）。リスナー（forwarder）を止めた後に tick しても FSM は動き続け、何も届かない |
| 4 | remove stopped listeners | `removes_stopped_listeners` | 移す（本体の追加が要る）。リスナーを止めた後、`awaitAssert` 相当でリスナー数が 0 になることを確かめる。原典は `whenUnhandled` の中で `listeners.size` を読むため、状態関数と `when_unhandled` へ FSM の読み取り専用の面（`FsmView`）を渡す追加と、`replying` の追加が要る（§2.1） |
| 5 | make previous and next state data available in onTransition | `makes_previous_and_next_state_data_available_in_on_transition` | 移す（本体の追加が要る）。`onTransition` の中で前のデータ（0）と次のデータ（1）の組が届く。現行の `on_transition` は状態名しか渡さないため、データを渡す形の追加が要る（§2.1） |
| 6 | trigger transition event when goto() the same state | `triggers_a_transition_event_when_goto_targets_the_same_state` | 移す。同じ状態への `goto` でもリスナーへ `Transition(fsm, 1, 1)` が届き、`onTransition` にも（1, 1）が届く |
| 7 | not trigger transition event on stay() | `does_not_trigger_a_transition_event_on_stay` | 移す。`stay()` ではリスナーへ何も届かない |
| 8 | not leak memory in nextState | `does_not_leak_next_state_data_outside_on_transition` | 移す（形を変える。§2.1）。原典が確かめるのは「遷移のあとに次のデータを保持し続けない（漏れない）」ことである。移植は、データを `Arc` にして `using` で渡し、遷移の前半では次データに当たる送信先へ `(0, 1)` が届くこと、遷移のあとでは `Arc::strong_count` がテストの分と FSM の現在の分の 2 つだけであることを、状態関数が数を見て `ok` を返す形で確かめる。panic する公開 API は足さない（独立レビューの推奨を採用した） |

ケース 1、6、7 が守る通知の規則（`stay` は通知しない、同じ状態への `goto` は通知する、`initialize` は初期状態への遷移を通知する）は、現行の `Fsm::make_transition` がすでに持つ（`fsm_test.rs` の回帰ケースが一部を固定している）。不足はリスナーとデータと `whenUnhandled` と、状態関数から FSM の面を読む手段であり、§2.1 に書く。

### 1.3 対応表：LocalActorRefProviderSpec（8 件）

移す先は `crates/fraktor-actor/tests/local_actor_ref_provider_spec.rs` である（ケース 5 を含む全件）。原典は設定文字列でスレッドプールの dispatcher を差し替えるが、その設定はケースの振る舞いを決めていない（同時実行のケース 6 の土台であるだけである）。移植は設定文字列を使わず、プログラムから作る通常のシステムで書く。同時実行のケース 6 だけは §1.4 で分ける。

| # | 原典のケース | Rust のテスト名 | 移す／作らない |
|:---:|:---|:---|:---|
| 1 | find child actor with URL encoded name | `finds_a_child_actor_with_a_url_encoded_name` | 移す（本体の追加が要る。形を変える）。子を URL エンコード済みの名前（`pekko%3A%2F%2FClusterSystem%40127.0.0.1%3A7355`）で作り、その子のパスから解決し直した参照が、停止しておらず、名前が元の文字列と一致することを確かめる。解決は、原典のようにアクターの `receive` の中からは呼ばない。`ActorSystem` は `RefCell` で executor を借りており、処理中にもう一度借りると panic するためである。アクターは子のパス（または子の参照）を probe へ送り、テストの側で `ActorSystem::resolve_actor_ref` を呼ぶ形にする（§2.2） |
| 2 | not handle messages other than those it will act upon（root guardian） | `root_guardian_reports_messages_it_does_not_handle_as_unhandled` | 移す（本体の追加が要る）。`/` への送信が dead letter ではなく `UnhandledMessage` の警告（送信者は dead letters）になることを `EventFilter` 相当で確かめる。現行は guardian のパスが実在のアクターでないため、未解決パスとして dead letter へ流れる（§2.2） |
| 3 | not handle messages other than those it will act upon（user guardian） | `user_guardian_reports_messages_it_does_not_handle_as_unhandled` | 移す（本体の追加が要る）。`/user` についてケース 2 と同じことを確かめる |
| 4 | not handle messages other than those it will act upon（system guardian） | `system_guardian_reports_messages_it_does_not_handle_as_unhandled` | 移す（本体の追加が要る）。`/system` についてケース 2 と同じことを確かめる |
| 5 | not retain its original Props when terminated | `does_not_retain_the_original_props_when_terminated` | 移す（形を変える）。番兵の案は取り下げた（H2）。実物では、停止したセルは表から外れて捨てられるため、停止後にセルの `Props` を読む対象が無い。性質は結合テストで確かめる。factory のクロージャに `Arc` を捕まえさせ、テストはその `Weak` を持つ。停止の前に `weak.upgrade()` が `Some` であることを確かめてから（これが無いと、資源がはじめから捕まえられていなかった場合でも通ってしまう）、子が停止した後、`await_assert` で `weak.upgrade()` が `None` になる（元の `Props` が捕まえた資源が解放される）ことを確かめる。本体の追加は要らない |
| 6 | only create one instance of an actor with a specific address in a concurrent environment | —（#234 の後。§1.4） | この issue では移さない。複数スレッドから同時に同じ名前で作っても成功が 1 件だけで、残りが `InvalidActorNameException` になることを確かめるケースであり、前提は §1.4 に書く |
| 7 | only create one instance of an actor from within the same message invocation | `creates_only_one_instance_from_within_the_same_message_invocation` | 移す（形を変える）。1 回のメッセージ処理の中で同じ名前（`duplicate`）で 2 回作ると、2 回目が `InvalidActorNameException` になる。原典は投げられた例外を `EventFilter` で傍受するが、移植の `actor_of` は例外を投げず `Result` を返すため、アクターが 2 回目の結果を probe へ送って確かめる形にする。加えて、子が 1 つだけ残ること（子のパスを解決すると 1 回目に作った参照が返ること）も確かめる |
| 8 | throw suitable exceptions for malformed actor names | `rejects_malformed_actor_names_with_suitable_messages` | 残りを移す。名前の検証そのもの（`ActorPath::validate_path_element`）はすでにあるが、生成の経路へつながっていないため、つなぐ追加が要る（§2.2）。原典の小ケースのうち、名前が `null` の確かめは、None で移した（元帥の決定 2026-11-26、#289）。ケース 8 の小ケースであるため独立のケースとは数えず、件数は変えない。空文字、先頭の `$`、不正な `%` エスケープ（`a%`、`%3`、`%xx`、`%0G`、`%gg`、`%`、`%1t`）、`a?`、非 ASCII（`üß`）、引用符・`$`・`>` の位置の表示はすべて移す |

ケース 8 の文面は 1 本で足りる（H3）。`Children.checkName` が見るのは null と空だけであり、先頭の `$` は `validatePathElement` が位置 0 として報告する。「not start with `$`」と「include only ASCII」は、不正な名前の文面に毎回入る固定の規則の文であり、`$hallo` と `$hello` と `üß` の期待値はすべて、現行の `ActorPath::validate_path_element` が出す 1 本の文面で満たされる。移植はこの文面をそのまま使い、新しい文面は作らない（§2.2）。

### 1.4 #234 を前提にするケース（LocalActorRefProviderSpec のケース 6）

ケース 6 は、100 個の名前について、4 本の `Future` が同時に `system.actorOf(Props, name)` を呼び、成功（1）と `InvalidActorNameException`（2）の集合がちょうど `{1, 2}` になることを確かめる。守る性質は「同じアドレスのアクターは同時に作っても 1 つだけである」ことであり、その性質が意味を持つのは、生成が実際に複数のスレッドから競合するときだけである。

fraktor-ng-rs の現行の `ActorSystem` は 1 シャード＝1 executor で、executor は `Rc<RefCell<Executor>>` の後ろにあり、システムは `Send` でも `Sync` でもない（#191 の設計で試作により確認済み）。単一スレッドで 4 回続けて作る形に写すと、2 回目以降が名前の重複で失敗するのは自明であり、同時実行の競合（検査と登録の間に割り込まれること）は何も確かめられない。移植したテストを弱める形になるため、この issue では写さない。

この扱いは、元帥が 2026-11-26 に案 1 で決めた。#194 の完了条件は「2 ノードを green、`LocalActorRefProviderSpec` は 8 件中 7 件を移し、同時実行の 1 件は #234 の段 5（[issue #256](https://github.com/j5ik2o/fraktor-ng-rs/issues/256)）のあと」に変わる。

- ケース 6 は、#234 の段 5（std のスレッドプール、#256）の後に置く。段 5 より前は、別のスレッドから同時に使うと panic すると #234 の設計が rustdoc に書く決まりにしており、意味のある競合を作れない。名前の予約が原子的になるのは段 4a（生成と停止、子の名前）である。
- 台帳の `LocalActorRefProviderSpec` ノードは、`status: unported` のまま、`cases.total: 8`、`ported: 7`、`passing: 7` とし、最上位の `verified_against` は書かない。ケース 6 は除外（`cases.excluded`）にも作らない区分（`cases.not_built`）にもしない。
- 後続の issue は [issue #265](https://github.com/j5ik2o/fraktor-ng-rs/issues/265) である。ケース 6 は #265 で扱う。
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

検証の入口の形が原典と違う点が 1 つある。原典の `system.actorOf` は例外を投げる。移植の現行の `ActorSystem::actor_of` は `ActorRef` を返し、失敗を返す場所が無い。§2.3 で、名前の検査と設定の検査の両方を行う入口として `try_actor_of` を足し（M5）、ケースはそちらで `Err` を確かめる形にする。`actor_of` 自体は検証に失敗したら panic する（Pekko が例外を投げることの写しであり、名前の重複で panic する既存の `spawn_child_from_weak` の前例と同じ扱いである）。

ルーターそのものの振る舞い（routee への振り分け、pool の増減）はこの Spec の対象ではない。ケースが要るのは「ルーター付きの `Props` を作り、検証を通す／落とす」ことだけであるため、第 2 段で足すルーターは、検証と生成の入口までの最小の形とする。ただし「動くように見えるルーター」を公開しないため、独立レビューの推奨の条件 3 つを付ける。(a) head への送信は、届け先が無いメッセージとして `UnhandledMessage` で報告する扱いに決め、rustdoc とテストに書く。(b) routee は作らない。振り分けが無い以上、routee を作っても仕事をしないためであり、そのため §3 の C5 がいう「routee の一部だけが残る」状態はこの段では起きない。(c) 振り分けの後続 issue を先に立て、rustdoc からリンクする。順序は #234 の設計 §8 の「deployment と router のあと」に合わせる。

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
| パスから `ActorRef` を同期的に解決する（`provider.resolveActorRef(path)`） | **無い** | `ActorSystem::actor_selection` は配送時に解決する選択であり、解決した参照を返す API は無い。ただし、パスを保った空の参照を作る `next_actor_ref(path, boundary)` はすでにあり、`deliver_selection` が使っている。Pekko の `EmptyLocalActorRef` に当たる形である（M2） |
| guardian（`/`、`/user`、`/system`）への未処理メッセージの報告 | **無い** | guardian は実在のアクターではなく、パスもセルも無い。guardian のパスへの選択は未解決として dead letter へ流れる。`UnhandledMessage` の発行そのもの（`Executor::publish_unhandled`）と警告文（`unhandled message from <sender>: <message>`）はある |
| 停止したセルの `Props` の解放（`ActorCell.terminatedProps` への置き換え） | 対象が無い | 実物では、停止したセルは `Executor::stop_with_sender` が表から外して捨てる（`executor_value.rs` の 931 行付近）。`ActorRef` は id、世代、path、boundary だけを持ち、セルを持たない。したがって停止後に残るセルは無く、番兵へ置き換える対象も無い（H2） |
| testkit（`FraktorSpec`、`TestProbe` の expect 系、`EventFilter`、`await_assert`） | ある | `crates/fraktor-testkit` に `FraktorSpec`、`TestProbe::expect_msg_type`、`expect_no_message`、`watch` と終了の期待、`EventFilter::intercept`、`await_assert` がある。この 3 本の needs はおおむね満たされている |
| stash | ある（容量の上限は無い） | `ActorContext::stash`／`unstash_all`。容量の上限と `StashOverflowException` は未移植であり、文書にもそう書かれている。この 3 本のケースは stash を使わないため、この issue では触れない（§5） |
| ルーター（pool、`Props.withRouter`） | **無い** | `crates/` 全体に router／pool の型が無い。`Props` が持つのは factory と dispatcher 名だけである |
| dispatcher の登録表と存在の検査（`Dispatchers.hasDispatcher`） | **無い** | dispatcher は `Props::with_dispatcher` で名前を記録するだけであり、名前が実在するかどうかを誰も見ない。#234 の設計は段 5 で `kernel/dispatch/dispatchers.rs` に `Dispatchers` を置く計画であり、この issue の登録表はその計画と重なる（M1） |
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

状態関数と `when_unhandled` は、FSM 自身の読み取り専用の面を受け取る必要がある（H1）。原典のケース 4 は `whenUnhandled` の中で `listeners.size` を読むが、現行のクロージャの型は `ActorContext` と `Message` と `D` しか受け取らず、`Fsm` を読む手段が無い。そのため、状態関数の署名に `&FsmView<S>` を足す。

```rust
/// A read-only view of the machine, handed to state functions.
/// Mirrors what an FSM subclass reads from itself: `listeners.size`,
/// `stateName`, and `stateData`.
pub struct FsmView<S> { /* snapshot taken before the function runs */ }
impl<S> FsmView<S> {
  pub fn listener_count(&self) -> usize;
  pub fn state_name(&self) -> &S;
}

impl<S, D> Fsm<S, D> {
  /// Adds a handler that also receives the previous and next state data.
  /// Mirrors reading `stateData` and `nextStateData` inside `onTransition`.
  pub fn on_transition_with_data<F>(&mut self, handler: F)
  where F: FnMut(&mut ActorContext<'_>, &S, &S, &D, &D) + 'static;

  /// Adds the fallback state function. Mirrors `FSM.whenUnhandled`.
  /// The function receives the same `FsmView` as a state function.
  pub fn when_unhandled<F>(&mut self, function: F)
  where F: FnMut(&mut ActorContext<'_>, &FsmView<S>, &Message, &D) -> Option<FsmState<S, D>> + 'static;
}
```

`FsmView` は、状態関数を呼ぶ直前に作るスナップショットである（リスナーの数と状態名）。状態関数の実行中にリスナーの集合は変わらない。購読と解除は状態関数より先にさばくメッセージであり、リスナーの停止は、`watch_with` の専用の内部メッセージの処理で変わるが、それも 1 件のメッセージ処理の中では状態関数の前後どちらかで起きる。したがってスナップショットの数は、そのメッセージの処理中は正しい。`when` の署名も同じく `&FsmView<S>` を足す形に変わる。これは既存の公開 API の署名の変更であり、既存の利用（`fsm_test.rs`、`abstract_fsm_actor_test.rs`、`abstract_fsm_with_stash_actor_test.rs`）は引数を 1 つ受ける形へ直す（§7）。

決まりは次のとおりである。

- リスナーは `Fsm` が `Vec<ActorRef>` で持つ。`SubscribeTransitionCallBack` は、`Fsm` がメッセージとして受ける。現行の `Fsm::receive` は状態関数へだけ振るため、購読・解除のメッセージは状態関数より先に `receive` の中でさばく（Pekko の `FSM.receive` がそうする）。購読した相手へは、その場で `CurrentState` を送る。重複した購読は 1 件として数える（Pekko の `listeners` は集合である）。
- 遷移の通知は、`make_transition` が通知する条件（`goto`、または状態が変わる場合）と同じ条件で、リスナー全員へ `Transition` を送る。`stay` では送らない。通知の順序は、`onTransition` ハンドラの実行と同じ遷移の中で行う。
- リスナーの停止の検出は、利用者の監視と混ざらない形にする（M4）。fraktor の `watch` の表は（監視する側、される側）の組で重複を省くため、FSM が素の `watch` を使うと、利用者が同じ相手を監視していた場合に、FSM の `unwatch` が利用者の監視まで消し、利用者向けの `Terminated` を FSM が横取りする。Pekko は `watchWith(ListenerTerminated)` と `isWatching` の分岐でこれを避ける。移植も同じく、`ActorContext::watch_with(target, message)` を足し（Pekko の公開 API の写し）、FSM は専用の内部メッセージで監視する。購読の解除（`UnsubscribeTransitionCallBack`）のときは、Pekko の `removeListener` と同じく監視も外す。FSM が監視を始めた相手かどうかは `watchedListeners` に当たる集合で覚える。
```rust
impl ActorContext<'_> {
  /// Mirrors `ActorContext.watchWith`: when `target` stops, this actor
  /// receives `message` instead of the plain `Terminated`.
  pub fn watch_with<M: core::any::Any + Send + Sync>(&mut self, target: &ActorRef, message: M) -> ActorRef;
  /// Mirrors `ActorContext.isWatching` (Pekko's `DeathWatch.isWatching`).
  pub fn is_watching(&self, target: &ActorRef) -> bool;
}
```

重なったときの決まり（R1）：

- fraktor の watch の表は（監視する側、される側）の組で 1 件であり、1 つの組には届くメッセージが 1 つだけである。`watch` と `watch_with` が同じ相手について重なった場合の扱いは、Pekko の `checkWatchingSame` を写す。すでに監視している相手へ、違うメッセージで `watch_with` を呼ぶことと、すでに `watch_with` で監視している相手へ素の `watch` を呼ぶことは、どちらも契約違反であり panic する（Pekko の `IllegalStateException` に当たる）。同じメッセージでの再呼び出しと、素の `watch` の再呼び出しは冪等である。
- FSM は、購読を受けるたびに `is_watching` で確かめる（Pekko の `FSM.addListener` の分岐の写し）。まだ監視していない相手だけを `watch_with` で監視し、`watchedListeners` に当たる集合へ入れる。すでに監視している相手（利用者が状態関数の中で `context.watch` した場合を含む）は、警告をログへ出し、自動では外さない。解除と停止のどちらでも、その相手の監視は FSM の側からは触らない。
- したがって、状態関数が `context.watch(L)` した後に L を購読しても、購読した後に状態関数が `context.watch(L)` を呼んでも、FSM が利用者の監視を消したり、利用者向けの `Terminated` を横取りしたりすることはない。
- `when_unhandled` は、現在の状態の状態関数がすべて `None` を返したときにだけ呼ばれる。現行の「未処理なら現状態を保つ」既定は、`when_unhandled` が登録されていない場合の振る舞いとして残る。
- `FsmState` に返信を足す。Pekko の `State.replying(msg)` の写しである。

```rust
impl<S, D> FsmState<S, D> {
  /// Mirrors `FSM.State.replying(msg)`: after the transition, sends `msg`
  /// to the sender of the event being handled.
  pub fn replying<M: core::any::Any + Send + Sync>(mut self, message: M) -> Self;
}
```

返信の順序は原典に合わせる（M6）。Pekko の `makeTransition` は、返信を遷移ハンドラとリスナーへの通知より前に送る。そのため移植も、返信は遷移ハンドラの実行とリスナーへの `Transition` の送信より前に、イベントの sender へ送る。返信の値は `Vec` で持ち、`replying` を何度か呼ぶと呼んだ順に全部送る。次の状態が存在しない（登録されていない状態への `goto`）ときは、原典どおり返信しない。ケース 4 の `ListenerCount` への応答（`stay().replying(...)`）もこの経路である。

- データの渡し方。ケース 5 は `onTransition` の中で前のデータと次のデータの組を読む。ハンドラは 1 本の `Vec` で持ち、データ付きのハンドラを正とする（L5）。`on_transition` は、データを捨てる包みとして同じ `Vec` へ登録する。こうすると 2 種類のハンドラが登録した順に呼ばれる。`make_transition` は、遷移前のデータと遷移後のデータの両方をその間だけ保持してハンドラへ渡す。メッセージとして送る `CurrentState<S>` と `Transition<S>` は `Message::new` の境界から `S: core::any::Any + Send + Sync` を要するため、`Send + Sync` の境界は `Fsm` の impl 全体（通常の impl、`Default`、`Actor` の実装）に要る（再レビューの試作で、`make_transition` を含む通常の impl と `Default` にも境界が要ることが E0277 で確かめられた）。既存の利用の `S` は `String` と `&'static str` であり、どちらも境界を満たすため壊れない。
- ケース 8（`nextStateData` の漏れ）への形。次データは `on_transition_with_data` の引数としてだけ存在し、遷移が終わればハンドラの外からは読めない。panic する公開 API は足さない（§8 の項目 4）。ケースが空の確かめにならないよう、 §1.2 のとおり `Arc` の `strong_count` で、遷移のあとに次データを保持し続けていないことを確かめる。原典との形の違いはテストファイルの冒頭に記録する。
- `no_std`：追加はすべて `core` と `alloc` で書ける。リスナーは `alloc::vec::Vec`、メッセージ型は通常の構造体であり、`std` の機能は要らない。

### 2.2 LocalActorRefProvider に足すもの

provider という型は作らない。fraktor-ng-rs には `LocalActorRefProvider` に当たる型が無く、provider の仕事（生成、解決、guardian）は `Executor` と `ActorSystem` が分け持つ。原典が `provider.resolveActorRef` と呼ぶ場所は、移植では `ActorSystem` の公開 API として足す。provider をクラス名で選ぶ仕組み（`pekko.actor.provider` の設定）は JVM の設定ファイルに固有のものであり、この Spec のケースは見ていないため、足さない。

`ActorSystem` への追加：

```rust
impl ActorSystem {
  /// Mirrors `ActorRefProvider.resolveActorRef(path)`.
  pub fn resolve_actor_ref(&self, path: &ActorPath) -> ActorRef;

  /// Starts a top-level actor under `name`, running both the name checks
  /// and the configuration checks. Mirrors `ActorSystem.actorOf(props, name)`.
  pub fn actor_of_named(&mut self, props: Props, name: &str)
    -> Result<ActorRef, ActorCreationError>;
}

/// The two ways starting an actor can be refused, as one error.
/// Pekko throws both from the same `actorOf` call.
#[derive(Clone, PartialEq, Eq)]
pub enum ActorCreationError {
  InvalidName(InvalidActorNameException),
  Configuration(ConfigurationException),
}
impl core::fmt::Display for ActorCreationError { /* delegates to the inner error */ }
impl core::fmt::Debug for ActorCreationError { /* delegates to the inner error */ }
impl core::error::Error for ActorCreationError {}
```

決まりは次のとおりである。

- `resolve_actor_ref` の意味は原典どおりにする（M2）。見つからないパスについては dead letters ではなく、パスを保った空の参照（Pekko の `EmptyLocalActorRef`）を返す。作り方は既存の `next_actor_ref(path, boundary)` を使う。dead letters を返すのは、要素が空のときと、ルートが別のシステムのときだけである。guardian のパス（`/user` など）は guardian の参照を返す。解決の対象には、実行中のフレーム（表から外れている処理中のアクター自身と、その祖先）を含める。executor は実行中のアクターをセルの表から外すため、表だけを引くと処理中のアクターが「無い」と判定されてしまう。ケース 1 は、§1.3 のとおり `receive` の中からではなくテストの側から呼ぶ。URL エンコード済みの名前は、エンコード済みの文字列のまま要素として保持される（現行の `ActorPath` は要素をデコードしない）ため、ケース 1 の名前はそのまま一致する。
- guardian の未処理の報告。executor が選択の解決をする場所で、解決先が root（要素なし）、`user`、`system` の guardian のパスそのものである場合は、dead letter へ流さず、その guardian を宛先とする `UnhandledMessage` を発行する。送信者は原典どおり dead letters であり、警告文は既存の `unhandled message from <sender>: <message>` の形を使う。guardian の下の実在の子への配送は変えない。`resolve_actor_ref` が返した guardian の参照へ直接送った場合も、選択の経路と同じく `UnhandledMessage` になるようにそろえる（この扱いを確かめるケースは原典には無い）。なお、fraktor の警告文は `unhandled message from {path}: {message:?}` であり、Pekko の `unhandled message from Actor[{path}]: {message}` と文面が違う。この違いはテストファイルの冒頭に記録する（L11）。
- 名前の検証の接続。`Executor::spawn_named_child_under` と、新しい `actor_of_named` の経路で、重複の検査の前に名前を検証する。検証は既存の `ActorPath::validate_path_element` をそのまま使い、失敗は新しい生成子で `InvalidActorNameException` にする。入口で別に見るのは空文字だけであり、文面は「actor name must not be empty」とする。null は `actor_of_named` が `Option` の `None` として受け、文面「actor name must not be null」で拒む。それ以外（先頭の `$`、不正な `%` エスケープ、非 ASCII、禁止文字とその位置）はすべて `validate_path_element` の 1 本の文面で足りる（H3）。Pekko に無い文面（たとえば「actor name must not start with `$`」）は作らない。

```rust
impl InvalidActorNameException {
  /// Creates the error for a name that fails validation, carrying the
  /// message of `ActorPath::validate_path_element` unchanged.
  pub fn invalid_name(message: String) -> Self;
}
```

- `actor_of_named` の一意の検査が使う表を決める（L6）。システム直下の名前は user guardian の子の名前である。executor は、システムの生成時に user guardian の `ParentEntry` に当たる登録を用意し、`spawn_user`（名前の自動生成）と `actor_of_named`（名前の指定）の両方がその登録へ名前を記録する形にする。こうすると、一意の検査と、停止した後の名前の再利用が、親からの生成と同じ仕組みで成り立つ。
- null の小ケースは None で移した（元帥の決定 2026-11-26、#289）。名前を受ける入口が `Option` で受け、`None` を `InvalidActorNameException` 相当で拒む。ケース 8 の小ケースであるため、件数は変えない。
- 停止したセルの `Props` の解放には、本体の追加は要らない（H2）。実物では停止したセルは表から外れて捨てられるため、元の `Props` はセルの破棄と一緒に解放される。ケース 5 は §1.3 のとおり、`Weak` で解放を確かめる結合テストにする。番兵（`terminatedProps`）は作らない。
- `no_std`：追加はすべて `core` と `alloc` で書ける。解決と検証は文字列と `Vec` の操作だけであり、`std` の機能は要らない。

### 2.3 設定の検証に足すもの

登録表の名前と置き場所と入口は、#234 の設計（main）にそろえる（M1）。当初の設計が置いた `DispatcherKind`、`DispatcherDefinition`、`ActorSystem::with_dispatchers` は、#234 の段 5 が作り直す公開 API になるため取り下げる。置き場所は `crates/fraktor-actor/src/kernel/dispatch/dispatchers.rs`、型は `Dispatchers` と `DispatcherConfig`、種類は #234 の設計 §3.5 の 6 つ（既定、fork-join、Pinned、Balancing、CallingThread、Manual）を使う。入力は型付きの Setup として `ActorSystemSetup` に載せる（Pekko の BootstrapSetup に当たる入口）。既存の生成子（`ActorSystem::new(name, boundary)`、`with_setup(name, boundary, setup)`）は boundary を取るため、新しい入口も boundary を取る形を変えない。この issue が段 5 より前に登録表を足す場合は、同じ名前と置き場所で最小の形（id の表、別名、既定の id、`has_dispatcher`、種類の問い合わせ）を先に置き、段 5 がそれを広げる形にする。作り直しにはしない。

例外型（既存の `InvalidActorNameException` と同じ形で、`core` と `alloc` で書く）。置き場所は `crates/fraktor-actor/src/kernel/actor/configuration_exception.rs` とする。

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

dispatcher の登録表（#234 の設計の形）：

```rust
// crates/fraktor-actor/src/kernel/dispatch/dispatchers.rs
/// The kind of a dispatcher, as #234 design section 3.5 defines it.
/// The verification rules only ask whether a kind is `Balancing`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DispatcherKind { Default, ForkJoin, Pinned, Balancing, CallingThread, Manual }

/// One dispatcher definition, built in code (no config file).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DispatcherConfig { pub id: String, pub kind: DispatcherKind }

/// Mirrors `org.apache.pekko.dispatch.Dispatchers`: the table of ids,
/// aliases, and the default id.
pub struct Dispatchers { /* Vec<DispatcherConfig>, default id */ }
impl Dispatchers {
  pub fn has_dispatcher(&self, id: &str) -> bool;
  pub fn kind_of(&self, id: &str) -> Option<DispatcherKind>;
}
```

登録表の初期の内容は、既定の dispatcher の id（`pekko.actor.default-dispatcher`）を含む（L13）。Pekko では internal と blocking-io の dispatcher も既定で登録されている。この移植でその 2 つを初期登録に含めるかどうかは、#234 の段 5 の定義にそろえる。dispatcher の実行そのもの（スレッドプール、mailbox の共有）はこの issue では作らない。登録表が答えるのは「名前が存在するか」と「balancing かどうか」だけであり、この Spec の検証の意味論はその 2 つで足りる。

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

検証の入口は、名前の検査と設定の検査を 1 つにまとめる（M5）。

```rust
impl ActorSystem {
  /// Starts an actor after both the name checks and the configuration
  /// checks. Mirrors `ActorSystem.actorOf(props, name)`, which throws
  /// both `InvalidActorNameException` and `ConfigurationException`.
  pub fn try_actor_of(&mut self, props: Props, name: Option<&str>)
    -> Result<ActorRef, ActorCreationError>;
}
```

`actor_of_named(props, name)` は `try_actor_of(props, Some(name))` の包みであり、`actor_of(props)` は `try_actor_of(props, None)` が失敗したらその文面で panic する形にする。名前を付けて存在しない dispatcher を指す入力でも、どちらの検査が走るかが決まっている（両方走り、名前の検査が先である）。

検証の規則は、Pekko の本体（`LocalActorRefProvider.actorOf` と `RoutedActorRef` のコンストラクタ）の順で書く。

1. ルーターが無い場合：`Props` の dispatcher 名（未指定なら既定）が登録表に無ければ、`Dispatcher [<name>] not configured for path <path>` で `ConfigurationException` とする。dispatcher の種類は問わない（ケース 6 と 8）。
2. ルーターがある場合：**routee を先に見る**（M9）。routee 側の dispatcher 名が登録表に無ければ、`Dispatcher [<name>] not configured for routees of <path>` とする（ケース 10）。その後に head 側の dispatcher 名（`router_dispatcher`、未指定なら既定）が登録表に無ければ head 向けの文面で `ConfigurationException` とする（ケース 9）。head 向けの文面について、Pekko の本体は `Dispatcher [${p.dispatcher}] not configured for router of $path` と書き、中に入るのは head の名前ではなく routee の dispatcher 名である（原典の誤り）。移植はこの誤りを写さず、head の dispatcher 名を入れる。文面の形（`not configured for router of`）は保ち、この違いをテストファイルの冒頭に記録する。
3. ルーターがあり、head の dispatcher が balancing の場合：`RoutedActorRef` の検査により `ConfigurationException` とする（ケース 1〜5）。原典のケース 1〜5 は、pool の `withDispatcher` で head の dispatcher を指すため、移植でも pool 側の dispatcher の指定は head の dispatcher として記録する形にする。ケース 7 は head が既定（非 balancing）で routee が pinned のため通る。
4. 検証を通った場合：head のアクターを作る。routee は作らない（§1.5 の条件 (b)）。head への送信は `UnhandledMessage` で報告する（§1.5 の条件 (a)）。

設定の検証を `ActorContext::actor_of`（子の生成）へ広げるかどうかは、この issue ではシステム直下に留め、ずれとして記録する（独立レビューの推奨を採用した）。理由は 2 つある。子へ広げると `ActorContext::actor_of` のエラーの型（公開の署名）が変わることと、`supervisor_misc_spec.rs` が子に `pinned-dispatcher` と `test-dispatcher` を使っているため、その登録が要ることである。ただしエラーの型は今のうちに両方を表せる `ActorCreationError` にしておく（M5）。子の生成への適用は後続の issue を立てる。

- `no_std`：追加はすべて `core` と `alloc` で書ける。登録表は `Vec<DispatcherConfig>`、`Duration` は `core::time::Duration` である。kernel の `Arc` が要る場所では別名 `kernel::sync::Arc` を使う。

## 3. 守る性質（不変条件）

### FSM の遷移とリスナー

- F1：`stay()` は遷移を通知しない。`onTransition` ハンドラにも、遷移リスナーにも届かない。
- F2：同じ状態への `goto` は遷移を通知する。状態名が等しくても、ハンドラとリスナーへ届く。
- F3：`initialize()` は初期状態から初期状態への遷移を 1 回だけ通知する。
- F4：購読したリスナーへは、購読のメッセージが来るたびに `CurrentState` が届く（Pekko の `FSM.receive` が購読のたびに送る）。それ以降の通知される遷移ごとに `Transition` がちょうど 1 回ずつ、遷移の順序どおりに届く。重複して購読しても、`Transition` が二重には届かない（リスナーの集合は重複を持たない）。
- F5：停止したリスナーは、その停止の通知（`Terminated`）を FSM が処理した後は集合に無い。停止と通知の処理の間に遷移が来た場合、その遷移はまだ集合に残っている停止済みのリスナーへ送られ、dead letters へ流れる。これは Pekko でも起きることであり、生きた配送にはならない。停止したリスナーへの送信で FSM が失敗することはない。
- F6：遷移ハンドラの中で読める前のデータは遷移前のデータであり、次のデータは遷移後のデータである。`using` で差し替えた場合は差し替え後の値が次のデータになる。
- F7：次データは遷移の間だけ存在する。遷移の外（状態関数の中、通常のメッセージ処理の中）では、次データを指す値が無い。
- F8：状態関数がすべて断ったイベントは `whenUnhandled` へ渡り、`whenUnhandled` が断れば現状態と現データを保つ。`replying` の返信は、遷移ハンドラとリスナーへの通知より前に、イベントの sender へ届く。`replying` を何度か呼んだ場合は、呼んだ順に全部届く。次の状態が存在しない場合は届かない。

### 生成、解決、guardian、名前

- L1：生きている子のパスを解決すると、その子自身の参照が返る。名前が URL エンコード済みの文字列でも、要素はエンコード済みのまま一致する。
- L2：存在しないパスを解決すると、そのパスを保った空の参照（`EmptyLocalActorRef` 相当）が返り、panic しない。dead letters が返るのは、要素が空のときと、ルートが別のシステムのときだけである。guardian のパスは guardian の参照を返す。実行中のアクター（処理中のフレーム）のパスも、生きているものとして解決される。
- L3：guardian（`/`、`/user`、`/system`）へのメッセージは dead letter にならず、`UnhandledMessage` として発行される。guardian の下の実在の子へのメッセージは通常どおり届く。
- L4：子が停止した後は、元の `Props` が捕まえていた資源は解放される（テストが持つ `Weak` が昇格できなくなる）。再起動したアクターの `Props` は元のままである。
- L5：同じ親の下で、生きている子と同じ名前は作れない。子が停止すれば同じ名前を再び使える（既存の性質であり、検証の接続で壊さない）。
- L6：不正な名前は、生成の入口すべて（親からの生成、システム直下の名前付き生成）で、アクターを作る前に拒否される。拒否された名前の子は残らない。
- L7：名前の検証は、空については入口の文面で、それ以外は `validate_path_element` の 1 本の文面で拒否する。先頭の `$`、不正な `%` エスケープ、非 ASCII、禁止文字とその位置が、原典の期待値どおりに伝わる。

### 設定の検証

- C1：ルーターと balancing の head dispatcher の組み合わせは、pool の種類によらず作れない。
- C2：ルーターが無ければ、balancing の dispatcher でも作れる。
- C3：ルーターがあり、head と routee の dispatcher がどちらも存在し、head が balancing でなければ作れる。
- C4：存在しない dispatcher 名は、指す場所（通常のアクター、head、routee）によらず作れない。文面は指す場所を区別する。
- C5：検証に失敗した生成は、アクターを 1 つも作らない（head だけが残る、という状態を作らない）。routee はこの段では作らないため、routee の一部だけが残る状態は起きない。
- C6：dispatcher 未指定は既定の dispatcher を指すものとして扱い、常に存在する。登録表の初期の内容には、既定の dispatcher の id（`pekko.actor.default-dispatcher`）が入っている。

## 4. 壊しうる入力

- 名前：空文字、1 文字の `$`、`$` で始まる名前、末尾の `%`、`%` の後に 16 進数でない文字（`%xx`、`%0G`、`%gg`、`%1t`）、`%3` のような不足したエスケープ、正しいエスケープ（`%3A`、`%2F`、`%40`）を含む長い名前、非 ASCII の文字（`üß`）、引用符と `>` のような禁止文字とその位置、非常に長い名前（検証が長さで失敗したり、切り詰めたりしないこと）、`%` の連続（`%%`、`%%%41`）。
- パス：要素が無いパス（root）、guardian のパスちょうど、guardian の下の存在しない子、dead letters 自身のパス、解決の対象が解決の間に停止した場合。
- FSM：購読を送る前に停止したリスナー、同じリスナーの二重の購読、購読の直後に解除、遷移の最中に止まるリスナー、データが大きい値の場合の `using` の差し替え、状態関数が登録されていない状態への `goto`（既存の振る舞いを壊さないこと）、`whenUnhandled` が `replying` 付きの `stay` を返す場合の sender が dead letters の場合。
- 設定：dispatcher 名が空文字、空白だけの名前（`"does not exist"` のように空白を含む名前）、pool の個数が 0、`ScatterGatherFirstCompleted` の `within` が 0 と極端に大きい値、head と routee に同じ存在しない名前を指した場合にどちらの文面が出るか（routee を先に見る順序で固定する。§2.3 の規則 2）、登録表に同じ id を 2 回登録した場合（後の定義が勝つか、拒否するかは第 2 段で固定し、テストに書く）。
- 監視の範囲：watch の範囲の外のリスナー（`ask` の返信先、別のシステムのアクター）は、購読した時点で停止扱いになり、一度も通知されずに外れる。

## 5. Quint を書くかどうか

**FSM の遷移リスナーには Quint を書く。ほかの 2 本には書かない。**

FSM に書く理由：

- リスナーの集合は、購読、解除、リスナーの停止、遷移という 4 種類の出来事で変わる状態であり、守る性質（F4、F5）は 1 回の遷移ではなく遷移の列の全体で成り立つ。特に「停止したリスナーが次の遷移までに外れる」ことは、停止の通知と次の遷移の順序に依存し、単純なケースでは通り、順序を取り違えた実装でもケース 3 と 4 の写し方によっては通ってしまう余地がある。FrequencySketch の前例（PR #205）でも、誤りの種類によっては原典のケースと PBT を通り、Quint だけが落とした。
- モデルは小さくできる。状態は、現在の状態名（0 と 1 の 2 値で足りる）、リスナーの集合、各リスナーが生きているかどうか、最後に通知した遷移である。遷移は `subscribe`、`unsubscribe`、`stopListener`、`processNotice`、`tick`（goto）、`stay` の 6 つである。停止と、その通知の処理は別の遷移にする（M3）。停止と通知の処理の間に `tick` が来る列をモデルが表せなければ、言い直した F5 を確かめられない。`subscribe` は、相手がすでに停止している場合は通知をその場で積む（fraktor の `watch` が停止済みの相手へその場で `Terminated` を積むことの写し）。これが無いと、言い直した F5 がモデルでも破れる。
- 置き場所は `crates/fraktor-actor/specs/fsm_transition.qnt` の 1 ファイル、1 モジュールにし、モジュール名はファイル名と同じ `fsm_transition` にする。CI（`scripts/quint-check-specs.sh`）が見るのは `crates/*/specs/*.qnt` である。CI の形の決まり 3 点を守る。全体の不変条件は `val invariant` にまとめること、run の名前は `Test` で終わること、設定は `pure val` に置くこと（`const` を持つ本体は `quint run` が通らない）。不変条件は F1（`stay` は通知しない）、F2（同じ状態への `goto` は通知する）、F4（生きている購読者へは遷移ごとにちょうど 1 回）、F5（言い直した形。通知を処理した後は集合に無く、停止済みのリスナーへの配送は生きた配送にならない）を書く。負の対照は、#188 の前例と同じく同じモジュールの中で誤りの種類を引数で選び、`Test` で終わる run に `.expect(not(...))` を書く形にする。誤りは、`stay` でも通知する誤りと、停止したリスナーを外さない誤りである。
- モデルにしないもの：データ `D` の中身（F6 は Rust のテストで確かめる）、実時間（この Spec の遷移は時間に依存しない）。

ほかの 2 本に書かない理由：

- 名前の検証と設定の検証は、どちらも入力から結果が一意に決まる純粋な判定であり、履歴を持つ状態遷移ではない。その性質は、入力の組み合わせを一般化する PBT（§6）が直接確かめる。Quint で書いても同じ照合を別の記法で書くことになる。
- guardian の報告と `Props` の解放は、状態の列ではなく「停止の後に元の `Props` の資源が解放される」「guardian への送信は unhandled になる」という 1 段の性質であり、原典のケースで足りる。
- stash について：この 3 本のケースは stash を使わない。現行の stash には容量の上限が無く（`stash-capacity` と `StashOverflowException` は未移植）、あふれたときの振る舞い自体が存在しないため、この issue で Quint の対象にはしない。stash の容量を移植する issue が立つときに、その設計で判断する。

## 6. PBT を足すか

**足す。対象は FSM の実物、名前の検証の接続、設定の検証である。**

- 性質 1（FSM の実物と参照モデルの一致）：実物の `Fsm` に、購読、解除、リスナーの停止、tick（goto）、stay のランダムな列を流し、テスト側に書いた素朴な参照モデル（リスナーの集合と、通知される遷移の列）が予測する通知と、probe が受け取る通知が一致することを確かめる。Quint が確かめるのはモデルであり、Rust の `Fsm` ではない。順序を取り違えた実装の誤りは Quint では捉えられないため、この性質が要る（M7）。置き場所は `crates/fraktor-actor/tests/fsm_transition_property_test.rs` とする。
- 性質 2（名前の検証の接続）：ランダムな名前（ASCII の英数字、記号、`%` と 16 進数、非 ASCII、空文字を混ぜる）について、すべての生成の入口（親からの生成、システム直下の名前付き生成）が、「名前が有効なときだけ成功し、拒否したときはアクターを作らない」ことを確かめる（L6 の接続の性質であり、`validate_path_element` そのものは変えないコードなので、それだけを確かめる性質にはしない）。置き場所は `crates/fraktor-actor/tests/local_actor_ref_provider_property_test.rs` とする。
- 設定の検証の行列：pool の有無と種類、head と routee の dispatcher の種類（balancing、非 balancing、存在しない、未指定）のすべての組み合わせについて、検証の結果が §2.3 の規則 1〜3 から作った表と一致し、失敗した場合にアクターが増えていないこと（C5）。これは有限の全数の表であるため、PBT ではなく「全数の表のテスト」と呼ぶ。置き場所は `crates/fraktor-actor/tests/actor_configuration_verification_property_test.rs` とする。
- 乱数は固定 seed の SplitMix64 とし、失敗時は seed と入力を表示する。外部の依存は足さない（#188 の前例と同じ）。

## 7. 影響

- 既存の公開 API の追加：`Fsm` へのメソッドの追加（`on_transition_with_data`、`when_unhandled`）、`FsmState::replying` の追加、遷移リスナーのメッセージ型の追加、`ActorContext::watch_with` の追加、`ActorSystem::resolve_actor_ref` と `actor_of_named` と `try_actor_of` の追加、`Props::with_router` と `router` の追加、`InvalidActorNameException::invalid_name` の追加、`ConfigurationException`、`ActorCreationError`、`Dispatchers`、`DispatcherConfig`、`DispatcherKind`、`RouterConfig`、`PoolKind` の追加。
- 既存の公開 API の署名の変更：`Fsm::when` と新しく足す `when_unhandled` のクロージャは、`&FsmView<S>` を引数に取る（H1）。`Fsm` の impl 全体（通常の impl、`Default`、`Actor` の実装）の境界には `S: Send + Sync` が増える（L4）。既存の利用（`fsm_test.rs`、`abstract_fsm_actor_test.rs`、`abstract_fsm_with_stash_actor_test.rs`）は、引数を 1 つ受ける形へ直す。`S` の既存の値は `String` と `&'static str` であり、境界は満たす。
- 振る舞いの変更：名前の検証が生成経路へつながるため、これまで通っていた不正な名前の生成が失敗するようになる。独立レビューの試作では、既存の 1169 件のうち 1 件が落ちた。`actor_system_value_test.rs` の 2035 行付近の単体テストが、空白を含む名前（`first child`、`second child`）を使っているためである（L7）。第 2 段ではこの単体テストの名前を有効な名前へ直す。guardian のパスへの送信が dead letter から `UnhandledMessage` へ変わるため、dead letter の件数を数える既存のテスト（DeadLetter 系の Spec）への影響も、第 2 段の red の前に `cargo test --workspace` で確かめる。
- 台帳：第 2 段で対象ノード 3 件を更新する。`FSMTransitionSpec` は total／ported／passing を 8／8／8、`ActorConfigurationVerificationSpec` は 10／10／10 とし、`rust.crate` に `fraktor-actor`、`rust.test_file` に各テストファイル、`status: green`、`verified_against: v2.0.0` を書く。`LocalActorRefProviderSpec` は §1.4 の決定のとおり、`status: unported` のまま 8／7／7 とし、`verified_against` は書かない。null の名前は None で移した（元帥の決定 2026-11-26、#289）。ケース 8 の小ケースであるため件数は変えず、台帳の `not_built` にも `excluded` にも書かない。

## 8. レビューで見てほしい点への決定

初版の §8 の 8 項目は、独立レビューの推奨を受けて次のとおりに決めた。項目 2 はその後、元帥の決定（2026-11-26）で決まった。

1. ケース 6 を #234 の後に置くこと：§1.4 のとおり、元帥が 2026-11-26 に案 1 で決めた。段 5（#256）の後に置き、後続は [issue #265](https://github.com/j5ik2o/fraktor-ng-rs/issues/265) である。
2. null の名前：None で移した（元帥の決定 2026-11-26、#289）。名前を受ける入口が `Option` で受け、`None` を `InvalidActorNameException` 相当で拒む。ケース 8 の小ケースであるため、件数は変えない。
3. 文面の層：H3 のとおり、層は 1 つで足りる。入口で見るのは空（と null）だけであり、残りは既存の `validate_path_element` の文面をそのまま使う。
4. FSM のケース 8（`nextStateData`）を型の形で確かめること：型の形でよい。panic する公開 API は足さない。ケースが空の確かめにならないよう、`Arc` の `strong_count` で保持の有無を確かめる形にした（§1.2）。
5. Local のケース 7 を `Result` で確かめること：`Result` を probe へ送る形でよい。加えて、子が 1 つだけ残ることを確かめる。
6. ルーターを最小に留めること：留める。条件は §1.5 の 3 つ（head への送信は `UnhandledMessage`、routee は作らない、振り分けの後続 issue を先に立てる）である。
7. 設定の検証を子の生成へ広げること：この issue ではシステム直下に留め、ずれとして記録する。エラーの型は両方を表せる `ActorCreationError` にしておく。後続の issue を立てる。
8. 登録表を存在と種類だけにすること：最小に留める。ただし型の名前、置き場所、入口は #234 の `Dispatchers` にそろえ、型付きの Setup から渡す（§2.3）。

## 9. 第 2 段ですること（本書の範囲の外）

- red（失敗するテスト）→ green の順に実装する。先に 3 つの spec ファイル（と PBT 2 件、全数の表のテスト 1 件、Quint 1 件）を書き、コンパイルの失敗かテストの失敗で red を確認してから本体を足す。#234 の段 5（#256）を前提にするケース 6 は書かない（後続は [issue #265](https://github.com/j5ik2o/fraktor-ng-rs/issues/265)）。
- 既存の 4 つのテストファイル（`fsm_test.rs`、`abstract_fsm_actor_test.rs`、`abstract_fsm_with_stash_actor_test.rs` の `when` の引数と、`actor_system_value_test.rs` の名前）の機械的な直しは、red の工程（テストを書く工程）で行う。移植の規約は「実装ステップはテストファイルを編集しない」と定めているため、署名の変更で既存のテストがコンパイルできなくなる直しを実装の工程へ残さない。
- ルーターの振り分けと、子の生成への設定の検証の、後続の issue 2 件は、実装に入る前に立て、rustdoc からリンクする。
- 台帳の対象ノードを §7 のとおりに更新する。null の名前は None で移した（元帥の決定 2026-11-26、#289）ため、作らない・除外の承認待ちは残っていない。
- CI と同じ検査を流す（`cargo fmt --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`、ledger/tools の Python テスト、Quint は CI の `scripts/quint-check-specs.sh` をそのまま流し、`crates/fraktor-actor/specs/fsm_transition.qnt` が対象に入っていることを確かめる）。
- PR をドラフトから外し、マージできる状態まで持つ。マージはしない。

## 10. 設計レビュー（PR #244、HEAD f4eb2b0）への回答

独立レビューの判定は「要修正」（高 3 件、中 9 件、低 13 件）であった。全件について、直すか、理由を書いて採らないかを次のとおりに決めた。採らなかった指摘は無い。実装には入っていない。

### 高

| 指摘 | 対応 |
|:---|:---|
| H1：FSM のケース 4 は、設計の API では書けない（`listener_count` が `when_unhandled` のクロージャから読めない） | 直した。状態関数と `when_unhandled` の署名に `&FsmView<S>` を足し、`listener_count` と `state_name` を読めるようにした（§2.1）。署名の変更であることを §7 に書いた |
| H2：Local のケース 5 は前提が崩れる（停止したセルは表から外れて捨てられる） | 直した。番兵の案を取り下げ、本体の追加は無しにした。ケース 5 は、`Weak` で元の `Props` の資源の解放を確かめる結合テストにし、`cases_outside_test_file` の例外も取り下げた（§1.1、§1.3、§2.0、§2.2、§3 の L4） |
| H3：名前の文面の「2 層」は誤りである | 直した。入口で見るのは空（と null）だけとし、それ以外は既存の `validate_path_element` の 1 本の文面をそのまま使うことにした。Pekko に無い文面は作らない（§1.3、§2.2、§3 の L7、§8 の項目 3） |

### 中

| 指摘 | 対応 |
|:---|:---|
| M1：dispatcher の登録表が #234 の設計と重なる | 直した。当初の `DispatcherKind`、`DispatcherDefinition`、`with_dispatchers` を取り下げ、名前と置き場所と入口を #234 の `Dispatchers`、`DispatcherConfig`、型付きの Setup にそろえた。boundary の引数も既存の生成子に合わせた（§2.0、§2.3） |
| M2：`resolve_actor_ref` の意味が原典と違う | 直した。見つからないパスはパスを保った空の参照を返し、dead letters は要素が空のときとルートが別のときだけとした。解決の対象に実行中のフレームを含め、ケース 1 はテストの側から呼ぶ形にした（§1.3、§2.2、§3 の L2） |
| M3：不変条件 F5 は正しい実装でも破れる | 直した。F5 を「通知を処理した後は集合に無い。停止済みのリスナーへの配送は生きた配送にならない」と言い直し、Quint のモデルで停止と通知の処理を別の遷移に分けた。購読の時点で停止している相手には通知をその場で積むこともモデルに書いた（§3 の F5、§5） |
| M4：リスナーの監視が利用者の監視と混ざる | 直した。`ActorContext::watch_with` を足し、FSM は専用の内部メッセージで監視する設計にした。購読の解除では監視も外す（§2.1） |
| M5：生成の入口 2 つが検査を半分ずつしか持たない | 直した。エラーを `ActorCreationError`（名前の誤り、設定の誤り）にまとめ、`try_actor_of` と `actor_of_named` の両方で両方の検査をすることにした（§1.5、§2.2、§2.3） |
| M6：`replying` の返信の順序が原典と逆である | 直した。返信は `Vec` で持ち、遷移ハンドラとリスナーへの通知より前に送る。次の状態が存在しないときは送らない（§2.1、§3 の F8） |
| M7：FSM に PBT を足さない理由が成り立たない | 直した。実物の `Fsm` と参照モデルを比べる PBT を足した（§6 の性質 1） |
| M8：ルーターを最小の形にすることが意味論の指針と衝突する | 直した。§8 の項目 6 の推奨の条件 3 つ（head への送信は `UnhandledMessage`、routee は作らない、振り分けの後続 issue を先に立てる）を付けた（§1.5、§2.3） |
| M9：§4 の検査の順序が原典とも §2.3 とも合わない | 直した。routee を先に見る順序で統一した。head 向けの文面は、原典の誤り（中身が routee の dispatcher 名）を写さず、head の dispatcher 名を入れることに決め、違いをテストファイルの冒頭に記録する（§2.3、§4） |

### 低

| 指摘 | 対応 |
|:---|:---|
| L1：§8 は 8 項目あり、項目 5 の Spec 名が違う | 直した。§8 を「決定」の節に書き直し、項目 5 は LocalActorRefProviderSpec のケース 7 とした |
| L2：台帳の目安 7 は誤りではなく旧基準 v1.7.1 の件数である。needs も v1.7.1 の抽出値である | 直した（§1.1）。v2.0.0 の FSMTransitionSpec が使う watch、expect-terminated、await-assert を第 2 段で needs に足すことも書いた |
| L3：`pekko-semantics-first.md` は承認済みで main にある。`kernel::sync::Arc` の決まりがある | 直した（冒頭、§2.3） |
| L4：`CurrentState<S>` と `Transition<S>` には `S: Send + Sync` が要る | 直した。境界が増えることを §2.1 と §7 に書いた |
| L5：ハンドラは 1 本の `Vec` で持つ必要がある | 直した。データ付きのハンドラを正とし、`on_transition` はその包みにした（§2.1） |
| L6：`actor_of_named` の一意の検査が使う表が決まっていない | 直した。user guardian の登録を用意し、`spawn_user` と `actor_of_named` の両方が記録する形にした（§2.2） |
| L7：名前の検証をつなぐと既存の 1 件が落ちる | 直した。対象の単体テスト（`actor_system_value_test.rs` の空白を含む名前）を §7 に名前で書いた |
| L8：Quint の CI の形の決まりが書かれていない | 直した。`val invariant`、run の名前は `Test` で終わる、設定は `pure val` の 3 点と、負の対照の置き方を §5 に書いた |
| L9：PBT の性質 1 は変えないコードを確かめている。性質 2 は全数の表である | 直した。性質を「すべての入口が有効なときだけ成功し、拒否したときは作らない」という接続の性質に変え、設定の行列は「全数の表のテスト」と呼ぶことにした（§6） |
| L10：Local のケース 7 では子が 1 つだけ残ることも確かめる | 直した（§1.3、§8 の項目 5） |
| L11：警告の文面が Pekko と違う | 直した。違いをテストファイルの冒頭に記録することを §2.2 に書いた |
| L12：§4 に watch の範囲の外のリスナーを足す | 直した（§4） |
| L13：既定の dispatcher の id が登録表に入るかを書く | 直した（§2.3、§3 の C6） |

### 上司の判断

- §1.4（同時実行の 1 件を分けたときのノードの扱い）：元帥が 2026-11-26 に案 1 で決めた。完了条件は「2 ノードを green、`LocalActorRefProviderSpec` は 8 件中 7 件を移し、同時実行の 1 件は #234 の段 5（#256）のあと」に変わり、後続の issue は #265 である。設計の §1.4 を決定のとおりに書き直した。
- null の名前：None で移した（元帥の決定 2026-11-26、#289）。ケース 8 の小ケースであるため、件数は変えない。

## 11. 設計の再レビュー（PR #244、HEAD 9126560）への回答

再レビューの判定は「要修正（軽い）」であり、中の 2 点を書き足せば実装に入ってよいとされた。高の 3 件は直っており、`FsmView` の形でケース 2 とケース 4 がコンパイルできて通ることが試作で確かめられている。

| 指摘 | 対応 |
|:---|:---|
| R1：`watch_with` と利用者の watch が重なったときの決まりが無く、M4 が半分しか直っていない | 書き足した。`ActorContext::is_watching` を足し、`watch` と `watch_with` が違うメッセージで重なった場合は Pekko の `checkWatchingSame` を写して panic すること、FSM は `is_watching` の分岐で、すでに監視している相手を自動では外さないことを、§2.1 に決まりとして書いた |
| R2：既存のテストの書き換えをどの工程で行うかが書かれていない | 書き足した。既存の 4 つのテストファイルの機械的な直しは red の工程で行うことを §9 に書いた |
| L-a：`S: Send + Sync` の境界は `Fsm` の impl 全体に要る | 直した（§2.1、§7） |
| L-b：冒頭 3 行目の完了条件が古い | 直した。元帥の決定の完了条件に合わせた（冒頭） |
| L-c：後続の issue 2 件に番号が無い | 実装に入る前に立て、rustdoc からリンクすることを §9 に書いた。番号は第 2 段の報告で知らせる |
| L-d：ケース 5 で停止の前に `weak.upgrade()` が `Some` であることを確かめる | 直した（§1.3） |
| L-e：guardian の参照へ直接送ったときの扱いが書かれていない | 直した。選択の経路と同じく `UnhandledMessage` になるようにそろえる（§2.2） |
| L-f：F4 の二重に届かないのは `Transition` のことである | 直した。`CurrentState` は購読のたびに届くと書き分けた（§3 の F4） |
| L-g：`ActorCreationError` には `Display` と `Debug` が要る | 直した（§2.2） |
| L-h：リスナーの停止は専用の内部メッセージの処理になる | 直した（§2.1） |
