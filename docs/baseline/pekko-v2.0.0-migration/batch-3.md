# Pekko v2.0.0 への移行（第3まとまり）：テストが変わらないか機械的にだけ変わった 39 ノード

第1まとまり（`batch-1.md`）と第2まとまり（`batch-2.md`）の残りで、v2.0.0 でテストのケースと入力が変わらない green のノードを照合した記録である（Orca 項目258）。照合の方法、原典の SHA、共通祖先 `0ca804c7af` は第1まとまりと同じである。

## 照合の方法

- 各ノードのテストファイルについて、両方の系統のコミット（`git log <共通祖先>..<タグ> -- <path>`）と、その `git patch-id --stable` を出した。そのうえで、`git diff -M -w v1.7.1 v2.0.0 -- <path>` から import、package、空行を除いた差分を読んだ。道具は `tools/node_diff.sh`（作業用。repo には入れていない）で、出力は 39 ノード分ある。
- 「同じ blob」と書いたノードは、`git rev-parse v1.7.1:<path>` と `v2.0.0:<path>` が一致することを確かめた（12 ノード）。
- ケースの印（`" in {"`、`@Test` など）の数は、39 ノードすべてで v1.7.1 と v2.0.0 が同じだった。台帳の `cases` は変えていない。
- 差分の中身は、import の並びと書き方（`870d4499cc` #2235、`9dfca01566` #3348）、JUnit 5 化（`ec27d6dd8c` #2724）、javafmt（`00b0a99278` #2081）、`assertInstanceOf`（`4d942a0ca7` #3088）、`@nowarn` の削除（`08ea94bb4b` #2001）、Scala 3 の `?` ワイルドカード（`29dc5e8df3` #3048）、`StandardCharsets` と `Seq` 型（`8353a4f16c` #3301、`fef71efd85` #3539）、テストの時計の共通化と `RecencyList(clock)` の生成関数（`ab12a8f5ec` #2766）だけだった。ケースの入力と期待値を変えるものはない。
- 実装側は、項目256の報告書と補足（記録済みの差異 13 件の再判定）に加えて、移植済みの観測に掛かりうるコミットを読んだ。`fea9938cfa` #3622（dispatcher の拒否時の再帰的な報告を防ぐ）、`42a4596780` #3022（巨大な `withinTimeRange` の桁あふれ対策。ケースは 500ms と 1s を使う）、`8c48393cd3` #2208（内部の ExecutionContext の削除）は、どれも移植したケースの観測を変えない。
- Rust のテストは、すべて変更なしで pass する（workspace の結果は下の「検証」）。

## ノード

| ノード（`.../org/apache/pekko/` 以下） | ゴール | ケース | v2.0.0 系統でテストファイルに入ったコミット | Rust |
|:---|:---|---:|:---|:---|
| `actor/ActorLifeCycleSpec.scala` | G0d | 7 | `870d4499cc` #2235 | `crates/fraktor-actor/tests/actor_life_cycle_spec.rs` |
| `actor/setup/ActorSystemSetupSpec.scala` | G0issue150 | 5 | `870d4499cc` #2235 | `crates/fraktor-actor/tests/actor_system_setup_spec.rs` |
| `actor/setup/ActorSystemSetupTest.java` | G0issue150 | 1 | `ec27d6dd8c` #2724、`00b0a99278` #2081 | `crates/fraktor-actor/tests/actor_system_setup_test.rs` |
| `actor/ActorTimeoutSpec.scala` | G0issue101 | 2 | 同じ blob | `crates/fraktor-actor/tests/actor_timeout_spec.rs` |
| `actor/AddressSpec.scala` | AddressSpec | 1 | `9dfca01566` #3348 | `crates/fraktor-actor/tests/address_spec.rs` |
| `actor/AddressTest.java` | None | 1 | `ec27d6dd8c` #2724、`00b0a99278` #2081 | `crates/fraktor-actor/tests/address_test.rs` |
| `util/ByteStringBuilderSpec.scala` | G0issue133 | 3 | 同じ blob | `crates/fraktor-actor/tests/byte_string_builder_spec.rs` |
| `util/ByteStringInputStreamSpec.scala` | G0issue130 | 3 | 同じ blob | `crates/fraktor-actor/tests/byte_string_input_stream_spec.rs` |
| `util/ByteStringTest.java` | G0issue133 | 2 | `ec27d6dd8c` #2724、`00b0a99278` #2081 | `crates/fraktor-actor/tests/byte_string_test.rs` |
| `io/dns/DnsSettingsSpec.scala` | G0issue154 | 10 | `870d4499cc` #2235 | `crates/fraktor-actor/tests/dns_settings_spec.rs` |
| `io/dns/internal/DomainNameSpec.scala` | G0issue142 | 6 | 同じ blob | `crates/fraktor-actor/tests/domain_name_spec.rs` |
| `event/EventBusSpec.scala` | G0f | 42 | `870d4499cc` #2235、`08ea94bb4b` #2001 | `crates/fraktor-actor/tests/event_bus_spec.rs` |
| `actor/ForwardActorSpec.scala` | G0c | 2 | 同じ blob | `crates/fraktor-actor/tests/forward_actor_spec.rs` |
| `util/FrequencyListSpec.scala` | G0issue128 | 3 | `ab12a8f5ec` #2766、`870d4499cc` #2235 | `crates/fraktor-actor/tests/frequency_list_spec.rs` |
| `actor/HotSwapSpec.scala` | G0e | 7 | `9dfca01566` #3348 | `crates/fraktor-actor/tests/hot_swap_spec.rs` |
| `io/dns/IdGeneratorSpec.scala` | G0issue154 | 2 | `9dfca01566` #3348 | `crates/fraktor-actor/tests/id_generator_spec.rs` |
| `actor/routing/ListenerSpec.scala` | G0issue113 | 1 | 同じ blob | `crates/fraktor-actor/tests/listener_spec.rs` |
| `event/LoggingAdapterTest.java` | G0issue110 | 4 | `4d942a0ca7` #3088、`ec27d6dd8c` #2724、`00b0a99278` #2081 | `crates/fraktor-actor/tests/logging_adapter_test.rs` |
| `io/dns/internal/MessageSpec.scala` | G0issue142 | 1 | `870d4499cc` #2235 | `crates/fraktor-actor/tests/message_spec.rs` |
| `io/dns/NameserverAddressParserSpec.scala` | G0issue136 | 4 | 同じ blob | `crates/fraktor-actor/tests/nameserver_address_parser_spec.rs` |
| `PekkoVersionSpec.scala` | G0issue139 | 13 | 同じ blob | `crates/fraktor-actor/tests/fraktor_version_spec.rs` |
| `util/RecencyListSpec.scala` | G0issue124 | 1 | `ab12a8f5ec` #2766、`870d4499cc` #2235 | `crates/fraktor-actor/tests/recency_list_spec.rs` |
| `actor/RelativeActorPathSpec.scala` | G0a | 4 | `fef71efd85` #3539、`8353a4f16c` #3301 | `crates/fraktor-actor/tests/relative_actor_path_spec.rs` |
| `io/dns/internal/ResolvConfParserSpec.scala` | G0issue136 | 9 | 同じ blob | `crates/fraktor-actor/tests/resolv_conf_parser_spec.rs` |
| `actor/RestartStrategySpec.scala` | G0issue117 | 5 | `870d4499cc` #2235 | `crates/fraktor-actor/tests/restart_strategy_spec.rs` |
| `util/SegmentedRecencyListSpec.scala` | G0issue126 | 4 | `ab12a8f5ec` #2766、`870d4499cc` #2235 | `crates/fraktor-actor/tests/segmented_recency_list_spec.rs` |
| `pattern/StatusReplyTest.java` | G0issue103 | 6 | `ec27d6dd8c` #2724、`00b0a99278` #2081 | `crates/fraktor-actor/tests/status_reply_test.rs` |
| `actor/SupervisorMiscSpec.scala` | G0e | 6 | `870d4499cc` #2235 | `crates/fraktor-actor/tests/supervisor_misc_spec.rs` |
| `actor/SupervisorTreeSpec.scala` | G0issue96 | 1 | 同じ blob | `crates/fraktor-actor/tests/supervisor_tree_spec.rs` |
| `actor/Ticket669Spec.scala` | G0issue98 | 2 | `870d4499cc` #2235 | `crates/fraktor-actor/tests/ticket669_spec.rs` |
| `actor/UidClashTest.scala` | G0issue115 | 1 | 同じ blob | `crates/fraktor-actor/tests/uid_clash_test.rs` |
| `testkit/DefaultTimeoutSpec.scala` | G0b | 1 | `9dfca01566` #3348、`870d4499cc` #2235 | `crates/fraktor-testkit/tests/default_timeout_spec.rs` |
| `testkit/ExplicitlyTriggeredSchedulerSpec.scala` | G0issue119 | 8 | 同じ blob | `crates/fraktor-testkit/tests/explicitly_triggered_scheduler_spec.rs` |
| `testkit/ImplicitSenderSpec.scala` | G0b | 1 | `9dfca01566` #3348、`870d4499cc` #2235 | `crates/fraktor-testkit/tests/implicit_sender_spec.rs` |
| `testkit/PekkoSpec.scala` | G0b | 0 | `29dc5e8df3` #3048、`870d4499cc` #2235、`8254acc0e8` #2234 | `crates/fraktor-testkit/src/fraktorspec/fraktor_spec.rs` |
| `testkit/TestActorsSpec.scala` | G0b | 2 | 同じ blob | `crates/fraktor-testkit/tests/test_actors_spec.rs` |
| `testkit/TestEventListenerSpec.scala` | G0d | 9 | 同じ blob | `crates/fraktor-testkit/tests/test_event_listener_spec.rs` |
| `testkit/TestProbeSpec.scala` | G0c | 16 | `870d4499cc` #2235 | `crates/fraktor-testkit/tests/test_probe_spec.rs` |
| `testkit/TestTimeSpec.scala` | G0issue121 | 3 | 同じ blob | `crates/fraktor-testkit/tests/test_time_spec.rs` |

- 各 Rust テストの冒頭に、v2.0.0（`9fbd2283b1cf701c90a1223bf621942c191db61f`）と照合したことと上のコミットを書いた。v1.7.1 の出典の行は履歴として残した。
- `testkit/.../PekkoSpec.scala` は fixture（ケース 0）で、Rust 側の `crates/fraktor-testkit/src/fraktorspec/fraktor_spec.rs` の冒頭には原典の版の記載がない。そのため、冒頭には何も足していない。差分は Scala 3 のワイルドカードと JavaConverters の置き換えだけである。
- 台帳は 39 ノードを `verified_against: v2.0.0` にした。第2まとまり（#168）と合わせると、green の 49 ノードはすべて v2.0.0 で照合済みになる。

## ゴール文書の出典の更新（補足 13 件）

項目256 の補足で再判定した記録済みの差異 13 件について、次のゴール文書の末尾に「Pekko v2.0.0 との照合（2026-11-25 追記）」を足した。v1.7.1 当時の本文は書き換えていない。行番号は v2.0.0 の blob で確かめた。

`issue-108`、`issue-110`、`issue-113`、`issue-124`、`issue-126`、`issue-128`、`issue-130`、`issue-133`、`issue-136`（#159 の JDK 8 の留保は v2.0.0 で解消。v1.7.1 当時の記録は残し、v2.0.0 の JDK 17、21、25 を明記した）、`issue-142`、`issue-150`、`issue-154`、`issue-162`（FSM の listener の自動解除と `setTimer` の削除を、後続の suite の要件として書いた）。

## ActorSystemScala213PlusSpec の退役と引き継ぎ

- `actor-tests/src/test/scala-2.13+/org/apache/pekko/actor/ActorSystemScala213PlusSpec.scala` は v2.0.0 に無い（`batch-2.md` の訂正のとおり、削除ではなく、2.0 系統では最初から `ActorSystemSpec` にケースが入っていた）。このノードを、台帳の最上位の `retired` へ移した。欄は v1.7.1 で照合した当時のまま残し、`retired.at`、`reason`、`successor` を足した。凍結スナップショット `docs/baseline/pekko-v1.7.1/ledger.yaml` と Rust の回帰テストは、そのまま残した。
- 後継の `actor-tests/src/test/scala/org/apache/pekko/actor/ActorSystemSpec.scala` には、引き継いだ 1 件を `cases.carried` に記録した。記録したのは、ケース名、退役したノードの id、v2.0.0 の原典の位置（L345–351、`d5ee8a63e3` #2486）、Rust のテスト、照合したタグ v2.0.0、実行の結果（1 passed）である。`cases` は 20/1/1 で status は unported のままにし、残りの 19 ケースが未移植だとわかるようにした。ノード最上位の `verified_against` は、unported には書けない（#166 の規則）ので書いていない。
- 既存のスキーマでは、部分移植の根拠と退役を表せない。そのため、最小の欄（`retired`、`cases.carried`）と検査（`rollup.py` の `lineage_problems`、退役したゴールの扱い）を足した。規則は `docs/decisions/ledger-schema-draft.md` の「退役と引き継ぎ」にある。`goal_complete_gate_test.py` に 2 件を足し、実物のゴール文書の検査は、属するノードがすべて退役したゴール（G0issue99）を、退役したノードと表の id の一致と、関門が完了にしないことで確かめるようにした。
- 集計：rollup の N は 1261 から 1260 に、green は 49 から 48 になった（退役したノードがクラス数から外れる）。ケースの移植数は変わらない（二重に数えない）。`check-ledger --pekko pekko` で「v2.0.0 の原典にファイルが無いノード」は 24 から 23 になり、残りはすべて unported である。

## 第1まとまりの記録の訂正（#167 の最終レビュー）

- `batch-1.md`：`toString` を上書きした例外では型名が「出ない」を「出ないことがある」に直した。
- `batch-1.md`：表示から型名を外す負の対照の件数を、`dd536e3` での「unit テスト 2 件と spec 1 件」に直した（手元でも同じ結果を確かめた）。
