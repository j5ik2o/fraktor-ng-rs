# #206：`MIT OR Apache-2.0` の候補（照合前）

- **段 B は当面行わない**（オーナーの指示、2026-11-26。[設計](./issue-206-license-policy.md) の前提の 7.5）。55 件は当面 `保留`（ヘッダーなし、クレートの `license = "Apache-2.0"`）のままにする。
- 状態: **照合前。この一覧のファイルには、まだ何も付けない。** 1 件ずつの照合は、実装に関わっていない別の担当が行う。照合の結果を元帥が読んでから、`MIT OR Apache-2.0` を push する（[設計](./issue-206-license-policy.md) §8.1 の段 B）
- 対象: [Issue #206](https://github.com/j5ik2o/fraktor-ng-rs/issues/206)、[PR #215](https://github.com/j5ik2o/fraktor-ng-rs/pull/215)
- 原典: Apache Pekko v2.0.0（`9fbd2283b1cf701c90a1223bf621942c191db61f`）

## 何が候補か

線引きの一覧（[TSV](./issue-206-file-origin.tsv)）で、H1 の規則 3 つと規則 4 を足したあとも「自分で書いた」に残った `crates/**/*.rs` の **55 件**である。TSV では `付けるSPDX` が `保留` の行にあたる。

| | 件数 |
|:---|--:|
| `crates/fraktor-actor` | 47 |
| `crates/fraktor-testkit` | 8 |
| 実装 | 25 |
| 単体テスト（`src` の下の `*_test.rs`） | 26 |
| 統合テスト（`tests/`） | 4 |
| **合計** | **55** |

前の版（`b6a4d69`）の 60 件から、H1 の規則で 6 件が要確認へ移った（`address_from_uri_string.rs`、`executor_value_test.rs`、`restart_limit_zero_no_clock.rs`、`manual_scheduler_test.rs`、`test_actors_props.rs`、`test_time_contract.rs`）。main を取り込んで増えたファイルのうち、1 件（`string_hash_code_test.rs`）がここに入った。

一覧は次のコマンドで作り直せる。

```bash
awk -F'\t' 'NR > 1 && $7 == "crates" && $8 == "保留" {print $1}' docs/design/issue-206-file-origin.tsv
```

## 照合の基準（元帥の決定 1）

`MIT OR Apache-2.0` を付けてよいのは、次を 1 件ずつ確かめたものだけである。

- Pekko 2.0.x（上の固定 SHA）の**どのファイルにも**、そのファイルの型・関数・テストの形に対応するものが無い。
- 見るもの: 原典で同じ役割の型と spec、テストケースの名前と並び、コメントで引いている名前、エラーの文言（例：`"Malformed URL"` は `MalformedURLException` から来ていた）。

「Pekko に対応が無い」と 1 件ずつ決めるのは、muse か Claude の 1 本が行う。pi は材料を集めるところまでで、決めない（元帥の決定、2026-11-26 第 2 回。設計の前提の 7.4）。

照合する担当は、表の「照合の結果」に `自分で書いた` / `Pekko 由来` / `迷う` のどれかを、「見た原典と根拠」に見た原典のパスと理由を書く。迷ったら `迷う` にする。`迷う` は `Pekko 由来` と同じ扱い（`Apache-2.0`）になる。

表の「宣言する公開型」と「名前の近い原典」は機械が出した手がかりで、照合の結果ではない。「レビュー 2 回目の標本」は、設計レビュー 2 回目（`pr215-design-review2.md`）が 20 件の標本で見た結果である。標本で「自分で書いた」と見たものも、照合を省かない。

## 結果の反映

1. 元帥が、照合の結果（件数と各ファイルの根拠）を読む。
2. ファイルごとの行を [決定のファイル](./issue-206-origin-decisions.tsv) に足す。`自分で書いた` と確かめたものは `MIT OR Apache-2.0`、それ以外は `Apache-2.0`。`理由` には照合の根拠を書く。
3. `classify_file_origin.py` で TSV を作り直し、`付けるSPDX` に `保留` が残っていないことを確かめる。
4. ここで初めて、`MIT OR Apache-2.0` のヘッダーを付けて push する。**push した時点で公開され、取り消せない**（設計 §8.2）。

## crates の外で `MIT OR Apache-2.0` を宣言するもの

| 対象 | 宣言 | 確かめること |
|:---|:---|:---|
| `tools/syn-check`（`src/**/*.rs` 33 件。`Cargo.toml` に `license` を足す） | `MIT OR Apache-2.0` | Pekko・Akka・Lightbend への言及は 0 件（設計 §1.1）。`src/rules/*.rs` の「Ported from the old `*-lint`」の移植元が、オーナーが自分で書いたもので、他人の貢献を含まないこと（設計レビュー 2 回目の L1）。**確かめた**（設計の前提の 7.3）：移植元はオーナーの公開リポジトリ `j5ik2o/fraktor-rs` の `lints/`（`LICENSE-MIT` あり）。`lints/` へのコミットは j5ik2o 52、renovate[bot] 9（依存の更新）、Copilot のエージェント 1（j5ik2o との共著）。このリポジトリの `tools/syn-check` のコミットは 15 件すべて Junichi Kato。MIT のまま進める |

## 候補の一覧（55 件）

| # | パス | 種別 | 宣言する公開型 | 名前の近い原典（手がかり） | レビュー 2 回目の標本 | 照合の結果 | 見た原典と根拠 |
|--:|:---|:---|:---|:---|:---|:---|:---|
| 1 | `crates/fraktor-actor/src/adaptor.rs` | 実装 |  | `Adapter.scala` |  | | |
| 2 | `crates/fraktor-actor/src/adaptor/ask_error.rs` | 実装 | `AskError` |  |  | | |
| 3 | `crates/fraktor-actor/src/adaptor/ask_future_test.rs` | 単体テスト |  | `Future.scala` |  | | |
| 4 | `crates/fraktor-actor/src/adaptor/channel_boundary.rs` | 実装 | `ChannelBoundary` |  |  | | |
| 5 | `crates/fraktor-actor/src/adaptor/channel_boundary_test.rs` | 単体テスト |  |  |  | | |
| 6 | `crates/fraktor-actor/src/adaptor/status_ask_error.rs` | 実装 | `StatusAskError` |  |  | | |
| 7 | `crates/fraktor-actor/src/adaptor/status_ask_future.rs` | 実装 | `StatusAskFuture` |  | 迷う（`askWithStatus` の API の移植） | | |
| 8 | `crates/fraktor-actor/src/adaptor/unsupported_host_dns_platform_error.rs` | 実装 | `UnsupportedHostDnsPlatformError` |  |  | | |
| 9 | `crates/fraktor-actor/src/adaptor/windows_ip_helper.rs` | 実装 |  |  | 自分で書いた | | |
| 10 | `crates/fraktor-actor/src/adaptor/windows_ip_helper_test.rs` | 単体テスト |  |  |  | | |
| 11 | `crates/fraktor-actor/src/kernel.rs` | 実装 |  |  |  | | |
| 12 | `crates/fraktor-actor/src/kernel/actor/terminated_test.rs` | 単体テスト |  | `TerminatedSpec.scala` |  | | |
| 13 | `crates/fraktor-actor/src/kernel/actor_ref/actor_ref_value_test.rs` | 単体テスト |  | `ActorRef.scala` |  | | |
| 14 | `crates/fraktor-actor/src/kernel/address/address_value_test.rs` | 単体テスト |  |  |  | | |
| 15 | `crates/fraktor-actor/src/kernel/address/uri_parts.rs` | 実装 | `UriParts`, `RelativeUriParts` |  | 自分で書いた | | |
| 16 | `crates/fraktor-actor/src/kernel/address/uri_parts_test.rs` | 単体テスト |  |  |  | | |
| 17 | `crates/fraktor-actor/src/kernel/byte_iterator/byte_iterator_value_test.rs` | 単体テスト |  | `ByteIterator.scala`, `ByteIteratorSpec.scala` | 自分で書いた | | |
| 18 | `crates/fraktor-actor/src/kernel/byte_string/byte_string_builder_test.rs` | 単体テスト |  | `ByteStringBuilderSpec.scala`, `ByteStringUtils.scala` |  | | |
| 19 | `crates/fraktor-actor/src/kernel/byte_string/byte_string_value_test.rs` | 単体テスト |  | `ByteString.scala`, `ByteStringParser.scala` | 自分で書いた | | |
| 20 | `crates/fraktor-actor/src/kernel/double_linked_list/double_linked_list_value_test.rs` | 単体テスト |  | `DoubleLinkedList.scala`, `DoubleLinkedListSpec.scala` | 自分で書いた | | |
| 21 | `crates/fraktor-actor/src/kernel/event.rs` | 実装 |  | `Events.scala`, `EventBus.scala` |  | | |
| 22 | `crates/fraktor-actor/src/kernel/event/actor_event_bus_test.rs` | 単体テスト |  | `EventBus.scala` |  | | |
| 23 | `crates/fraktor-actor/src/kernel/event/event_stream_bus_test.rs` | 単体テスト |  | `EventStream.scala`, `EventStreamTest.java` |  | | |
| 24 | `crates/fraktor-actor/src/kernel/executor.rs` | 実装 |  |  |  | | |
| 25 | `crates/fraktor-actor/src/kernel/frequency_sketch/string_hash_code_test.rs` | 単体テスト |  |  |  | | |
| 26 | `crates/fraktor-actor/src/kernel/io.rs` | 実装 |  | `IO.scala` |  | | |
| 27 | `crates/fraktor-actor/src/kernel/io/dns.rs` | 実装 |  | `Dns.scala` |  | | |
| 28 | `crates/fraktor-actor/src/kernel/io/dns/id_generation_error_test.rs` | 単体テスト |  | `IdGenerator.scala` |  | | |
| 29 | `crates/fraktor-actor/src/kernel/io/dns/message_flags_test.rs` | 単体テスト |  | `Messages.scala`, `MessageFormats.java` | 自分で書いた | | |
| 30 | `crates/fraktor-actor/src/kernel/io/dns/record_class_code_test.rs` | 単体テスト |  | `RecordClass.scala` |  | | |
| 31 | `crates/fraktor-actor/src/kernel/io/dns/record_type_code_test.rs` | 単体テスト |  | `RecordType.scala` |  | | |
| 32 | `crates/fraktor-actor/src/kernel/io/dns/resolv_conf_test.rs` | 単体テスト |  | `ResolvConfParser.scala` | 自分で書いた | | |
| 33 | `crates/fraktor-actor/src/kernel/message/message_value_test.rs` | 単体テスト |  |  |  | | |
| 34 | `crates/fraktor-actor/src/kernel/recency_list/recency_list_value_test.rs` | 単体テスト |  | `RecencyList.scala`, `RecencyListSpec.scala` | 自分で書いた | | |
| 35 | `crates/fraktor-actor/src/kernel/segmented_recency_list/segmented_recency_list_value_test.rs` | 単体テスト |  | `SegmentedRecencyList.scala`, `SegmentedRecencyListSpec.scala` |  | | |
| 36 | `crates/fraktor-actor/src/kernel/setup.rs` | 実装 |  |  |  | | |
| 37 | `crates/fraktor-actor/src/kernel/supervision/restart_window.rs` | 実装 | `RestartWindow` | `RestartSink.scala`, `RestartFlow.scala` |  | | |
| 38 | `crates/fraktor-actor/src/kernel/timeout/timeout_value_test.rs` | 単体テスト |  |  |  | | |
| 39 | `crates/fraktor-actor/src/kernel/version/pre_release_kind.rs` | 実装 | `PreReleaseKind` |  |  | | |
| 40 | `crates/fraktor-actor/src/kernel/version/pre_release_kind_test.rs` | 単体テスト |  |  |  | | |
| 41 | `crates/fraktor-actor/src/kernel/version/release_version_test.rs` | 単体テスト |  |  | 自分で書いた | | |
| 42 | `crates/fraktor-actor/src/kernel/version/version_parse_error_test.rs` | 単体テスト |  |  |  | | |
| 43 | `crates/fraktor-actor/src/lib.rs` | 実装 |  |  |  | | |
| 44 | `crates/fraktor-actor/tests/g0f_review_d_republished_message.rs` | 統合テスト |  |  |  | | |
| 45 | `crates/fraktor-actor/tests/g0f_review_g_dropped_bus.rs` | 統合テスト |  |  |  | | |
| 46 | `crates/fraktor-actor/tests/independent_watch_compatibility.rs` | 統合テスト |  |  |  | | |
| 47 | `crates/fraktor-actor/tests/unhandled_message_forwarder_ownership.rs` | 統合テスト |  |  | 自分で書いた | | |
| 48 | `crates/fraktor-testkit/src/default_timeout.rs` | 実装 |  | `DefaultTimeoutSpec.scala` |  | | |
| 49 | `crates/fraktor-testkit/src/fraktorspec.rs` | 実装 |  | `FSMActorSpec.scala` |  | | |
| 50 | `crates/fraktor-testkit/src/implicit_sender.rs` | 実装 |  | `ImplicitSenderSpec.scala` |  | | |
| 51 | `crates/fraktor-testkit/src/manual_scheduler/manual_scheduler_task.rs` | 実装 | `ManualSchedulerTask` |  | 自分で書いた | | |
| 52 | `crates/fraktor-testkit/src/manual_scheduler/registration.rs` | 実装 | `Registration` |  | 自分で書いた | | |
| 53 | `crates/fraktor-testkit/src/manual_scheduler/registration_id.rs` | 実装 | `RegistrationId` |  |  | | |
| 54 | `crates/fraktor-testkit/src/test_kit/expected_messages.rs` | 実装 |  |  |  | | |
| 55 | `crates/fraktor-testkit/src/test_kit_extension/test_time_factor.rs` | 実装 | `TestTimeFactor` | `TestActor.scala`, `TestActors.scala` | 自分で書いた | | |
