# #206 / #218 既知の例外の一覧（原典のファイルが決まらない 85 件）

- 対象: [Issue #206](https://github.com/j5ik2o/fraktor-ng-rs/issues/206)、[Issue #218](https://github.com/j5ik2o/fraktor-ng-rs/issues/218)、[PR #215](https://github.com/j5ik2o/fraktor-ng-rs/pull/215)
- 根拠: 設計 [`docs/design/issue-206-license-policy.md`](./issue-206-license-policy.md) 前提 7.2・§1.3・§3.1・§3.4.4（C6）
- 件数: 全 85 件（要確認 76 件、強い根拠あり・原典ファイル未特定 9 件）

以下の 85 件は、いずれも Pekko 由来（`Apache-2.0`）としてヘッダーを付けているが、対応する原典の単一ファイルを機械で特定できなかったものである。
すべてのファイルに、少なくとも ASF の帰属表示（Apache Pekko の派生物であること）と、原典 `NOTICE` に合わせた既定の表示（Akka 由来の形と `Copyright (C) 2009-2022 Lightbend Inc. <https://www.lightbend.com>`。第三者の表示があるものはそのブロックも併記）を付けている。
原典のファイルを 1 件ずつ確かめていないため、85 件すべてについて**年と著作権者は未確認**である。あとで原典のファイルが特定できたら、[`issue-206-file-origin.tsv`](./issue-206-file-origin.tsv) の `原典ファイル` を埋めてヘッダーを付け直せる（`--remove` と `--apply --manifest`）。

| # | パス | 根拠の種類 | 原典のファイルが決まらない理由 | 年と著作権者 |
|--:|:---|:---|:---|:---|
| 1 | `crates/fraktor-actor/src/adaptor/ask_driver.rs` | Pekko言及 | Pekko への言及はあるが、原典との対応の明記が無い（設計の参考か写しか未確認） | 未確認 |
| 2 | `crates/fraktor-actor/src/adaptor/random_id_generator_test.rs` | 原典への言及（Pekko の明記なし） | 「original」「upstream」への言及はあるが、原典が Pekko だと明記が無い | 未確認 |
| 3 | `crates/fraktor-actor/src/adaptor/status_reply_ack.rs` | Pekko言及 | Pekko への言及はあるが、原典との対応の明記が無い（設計の参考か写しか未確認） | 未確認 |
| 4 | `crates/fraktor-actor/src/adaptor/system_clock.rs` | Mirrors | 強い根拠はあるが、原典のファイルに行き着かない | 未確認 |
| 5 | `crates/fraktor-actor/src/adaptor/system_host_dns_configuration.rs` | Mirrors（原典の明記なし） | 「original」への対応は書かれているが、それが Pekko だと明記が無い | 未確認 |
| 6 | `crates/fraktor-actor/src/adaptor/system_host_dns_configuration_test.rs` | 原典への言及（Pekko の明記なし） | 「original」「upstream」への言及はあるが、原典が Pekko だと明記が無い | 未確認 |
| 7 | `crates/fraktor-actor/src/adaptor/unsupported_id_generator_policy_error.rs` | 原典への言及（Pekko の明記なし） | 「original」「upstream」への言及はあるが、原典が Pekko だと明記が無い | 未確認 |
| 8 | `crates/fraktor-actor/src/adaptor/unwind_catcher.rs` | Pekko言及 | Pekko への言及はあるが、原典との対応の明記が無い（設計の参考か写しか未確認） | 未確認 |
| 9 | `crates/fraktor-actor/src/adaptor/windows_adapter.rs` | 原典への言及（Pekko の明記なし） | 「original」「upstream」への言及はあるが、原典が Pekko だと明記が無い | 未確認 |
| 10 | `crates/fraktor-actor/src/adaptor/windows_adapter_test.rs` | 原典への言及（Pekko の明記なし） | 「original」「upstream」への言及はあるが、原典が Pekko だと明記が無い | 未確認 |
| 11 | `crates/fraktor-actor/src/kernel/actor_system/actor_system_value_test.rs` | Pekko言及 | Pekko への言及はあるが、原典との対応の明記が無い（設計の参考か写しか未確認） | 未確認 |
| 12 | `crates/fraktor-actor/src/kernel/address/address_value.rs` | Pekko言及 | Pekko への言及はあるが、原典との対応の明記が無い（設計の参考か写しか未確認） | 未確認 |
| 13 | `crates/fraktor-actor/src/kernel/byte_iterator/byte_order.rs` | Mirrors | 強い根拠はあるが、原典のファイルに行き着かない | 未確認 |
| 14 | `crates/fraktor-actor/src/kernel/byte_string/byte_string_input_stream_test.rs` | Pekko言及 | Pekko への言及はあるが、原典との対応の明記が無い（設計の参考か写しか未確認） | 未確認 |
| 15 | `crates/fraktor-actor/src/kernel/byte_string/end_of_stream_error.rs` | Mirrors（原典の明記なし） | 「original」への対応は書かれているが、それが Pekko だと明記が無い | 未確認 |
| 16 | `crates/fraktor-actor/src/kernel/byte_string/index_out_of_bounds_error.rs` | Mirrors（原典の明記なし） | 「original」への対応は書かれているが、それが Pekko だと明記が無い | 未確認 |
| 17 | `crates/fraktor-actor/src/kernel/byte_string/mark_not_supported_error.rs` | Mirrors | 強い根拠はあるが、原典のファイルに行き着かない | 未確認 |
| 18 | `crates/fraktor-actor/src/kernel/clock.rs` | Mirrors | 強い根拠はあるが、原典のファイルに行き着かない | 未確認 |
| 19 | `crates/fraktor-actor/src/kernel/double_linked_list/node_id.rs` | Pekko言及 | Pekko への言及はあるが、原典との対応の明記が無い（設計の参考か写しか未確認） | 未確認 |
| 20 | `crates/fraktor-actor/src/kernel/executor/function_boundary.rs` | Pekko言及 | Pekko への言及はあるが、原典との対応の明記が無い（設計の参考か写しか未確認） | 未確認 |
| 21 | `crates/fraktor-actor/src/kernel/executor/noop_boundary.rs` | Pekko言及 | Pekko への言及はあるが、原典との対応の明記が無い（設計の参考か写しか未確認） | 未確認 |
| 22 | `crates/fraktor-actor/src/kernel/executor/shard_boundary.rs` | Pekko言及 | Pekko への言及はあるが、原典との対応の明記が無い（設計の参考か写しか未確認） | 未確認 |
| 23 | `crates/fraktor-actor/src/kernel/frequency_list/frequency_list_value_test.rs` | 原典への言及（Pekko の明記なし） | 「original」「upstream」への言及はあるが、原典が Pekko だと明記が無い | 未確認 |
| 24 | `crates/fraktor-actor/src/kernel/frequency_sketch/bits_test.rs` | 原典への言及（Pekko の明記なし） | 「original」「upstream」への言及はあるが、原典が Pekko だと明記が無い | 未確認 |
| 25 | `crates/fraktor-actor/src/kernel/frequency_sketch/counter_bits.rs` | 原典への言及（Pekko の明記なし） | 「original」「upstream」への言及はあるが、原典が Pekko だと明記が無い | 未確認 |
| 26 | `crates/fraktor-actor/src/kernel/frequency_sketch/counter_matrix.rs` | 原典への言及（Pekko の明記なし） | 「original」「upstream」への言及はあるが、原典が Pekko だと明記が無い | 未確認 |
| 27 | `crates/fraktor-actor/src/kernel/frequency_sketch/fast_frequency_sketch_test.rs` | 原典への言及（Pekko の明記なし） | 「original」「upstream」への言及はあるが、原典が Pekko だと明記が無い | 未確認 |
| 28 | `crates/fraktor-actor/src/kernel/frequency_sketch/frequency_sketch_value_test.rs` | 原典への言及（Pekko の明記なし） | 「original」「upstream」への言及はあるが、原典が Pekko だと明記が無い | 未確認 |
| 29 | `crates/fraktor-actor/src/kernel/frequency_sketch/hasher_seal.rs` | 原典への言及（Pekko の明記なし） | 「original」「upstream」への言及はあるが、原典が Pekko だと明記が無い | 未確認 |
| 30 | `crates/fraktor-actor/src/kernel/frequency_sketch/murmur3.rs` | 第三者のコードへの言及 | Pekko が使う第三者のコード（scala-library、hash-prospector）を引く（規則 4） | 未確認 |
| 31 | `crates/fraktor-actor/src/kernel/frequency_sketch/reset_size.rs` | 原典への言及（Pekko の明記なし） | 「original」「upstream」への言及はあるが、原典が Pekko だと明記が無い | 未確認 |
| 32 | `crates/fraktor-actor/src/kernel/frequency_sketch/sketch_depth.rs` | 原典への対応・移植の語（Pekko の明記なし） | 「corresponding to」「the ported」「original `X`」、または行をまたいだ「the original」がある（H1 の規則 3） | 未確認 |
| 33 | `crates/fraktor-actor/src/kernel/frequency_sketch/sketch_width.rs` | 原典への言及（Pekko の明記なし） | 「original」「upstream」への言及はあるが、原典が Pekko だと明記が無い | 未確認 |
| 34 | `crates/fraktor-actor/src/kernel/frequency_sketch/string_hash_code.rs` | 原典パス | 強い根拠はあるが、原典のファイルに行き着かない | 未確認 |
| 35 | `crates/fraktor-actor/src/kernel/frequency_sketch/string_hasher_test.rs` | 第三者のコードへの言及 | Pekko が使う第三者のコード（scala-library、hash-prospector）を引く（規則 4） | 未確認 |
| 36 | `crates/fraktor-actor/src/kernel/io/dns/dns_settings_config.rs` | Mirrors（原典の明記なし） | 「original」への対応は書かれているが、それが Pekko だと明記が無い | 未確認 |
| 37 | `crates/fraktor-actor/src/kernel/io/dns/dns_settings_config_test.rs` | 原典への言及（Pekko の明記なし） | 「original」「upstream」への言及はあるが、原典が Pekko だと明記が無い | 未確認 |
| 38 | `crates/fraktor-actor/src/kernel/io/dns/dns_settings_error.rs` | Mirrors（原典の明記なし） | 「original」への対応は書かれているが、それが Pekko だと明記が無い | 未確認 |
| 39 | `crates/fraktor-actor/src/kernel/io/dns/dns_settings_test.rs` | 原典への言及（Pekko の明記なし） | 「original」「upstream」への言及はあるが、原典が Pekko だと明記が無い | 未確認 |
| 40 | `crates/fraktor-actor/src/kernel/io/dns/dns_settings_values.rs` | Mirrors（原典の明記なし） | 「original」への対応は書かれているが、それが Pekko だと明記が無い | 未確認 |
| 41 | `crates/fraktor-actor/src/kernel/io/dns/dns_settings_values_test.rs` | 原典への言及（Pekko の明記なし） | 「original」「upstream」への言及はあるが、原典が Pekko だと明記が無い | 未確認 |
| 42 | `crates/fraktor-actor/src/kernel/io/dns/domain_name_test.rs` | 原典への言及（Pekko の明記なし） | 「original」「upstream」への言及はあるが、原典が Pekko だと明記が無い | 未確認 |
| 43 | `crates/fraktor-actor/src/kernel/io/dns/host_dns_configuration.rs` | Mirrors（原典の明記なし） | 「original」への対応は書かれているが、それが Pekko だと明記が無い | 未確認 |
| 44 | `crates/fraktor-actor/src/kernel/io/dns/id_generation_error.rs` | Mirrors（原典の明記なし） | 「original」への対応は書かれているが、それが Pekko だと明記が無い | 未確認 |
| 45 | `crates/fraktor-actor/src/kernel/io/dns/message_test.rs` | 原典への言及（Pekko の明記なし） | 「original」「upstream」への言及はあるが、原典が Pekko だと明記が無い | 未確認 |
| 46 | `crates/fraktor-actor/src/kernel/io/dns/nameservers_setting.rs` | Mirrors（原典の明記なし） | 「original」への対応は書かれているが、それが Pekko だと明記が無い | 未確認 |
| 47 | `crates/fraktor-actor/src/kernel/io/dns/ndots_setting.rs` | Mirrors（原典の明記なし） | 「original」への対応は書かれているが、それが Pekko だと明記が無い | 未確認 |
| 48 | `crates/fraktor-actor/src/kernel/io/dns/record_class_test.rs` | 原典への言及（Pekko の明記なし） | 「original」「upstream」への言及はあるが、原典が Pekko だと明記が無い | 未確認 |
| 49 | `crates/fraktor-actor/src/kernel/io/dns/resolv_conf_parser_test.rs` | 原典への言及（Pekko の明記なし） | 「original」「upstream」への言及はあるが、原典が Pekko だと明記が無い | 未確認 |
| 50 | `crates/fraktor-actor/src/kernel/io/dns/resource_record_test.rs` | 原典への言及（Pekko の明記なし） | 「original」「upstream」への言及はあるが、原典が Pekko だと明記が無い | 未確認 |
| 51 | `crates/fraktor-actor/src/kernel/io/dns/search_domains_setting.rs` | Mirrors（原典の明記なし） | 「original」への対応は書かれているが、それが Pekko だと明記が無い | 未確認 |
| 52 | `crates/fraktor-actor/src/kernel/io/dns/ttl_setting.rs` | Mirrors（原典の明記なし） | 「original」への対応は書かれているが、それが Pekko だと明記が無い | 未確認 |
| 53 | `crates/fraktor-actor/src/kernel/io/dns/ttl_test.rs` | 原典への言及（Pekko の明記なし） | 「original」「upstream」への言及はあるが、原典が Pekko だと明記が無い | 未確認 |
| 54 | `crates/fraktor-actor/src/kernel/io/dns/wire_reader.rs` | 原典パス | 強い根拠はあるが、原典のファイルに行き着かない | 未確認 |
| 55 | `crates/fraktor-actor/src/kernel/io/dns/wire_reader_test.rs` | Pekko言及 | Pekko への言及はあるが、原典との対応の明記が無い（設計の参考か写しか未確認） | 未確認 |
| 56 | `crates/fraktor-actor/src/kernel/supervision/failure_catcher.rs` | Pekko言及 | Pekko への言及はあるが、原典との対応の明記が無い（設計の参考か写しか未確認） | 未確認 |
| 57 | `crates/fraktor-actor/src/kernel/supervision/restart_limit.rs` | Mirrors | 強い根拠はあるが、原典のファイルに行き着かない | 未確認 |
| 58 | `crates/fraktor-actor/src/kernel/supervision/run_outcome.rs` | Pekko言及 | Pekko への言及はあるが、原典との対応の明記が無い（設計の参考か写しか未確認） | 未確認 |
| 59 | `crates/fraktor-actor/src/kernel/version/fraktor_version_test.rs` | Pekko言及 | Pekko への言及はあるが、原典との対応の明記が無い（設計の参考か写しか未確認） | 未確認 |
| 60 | `crates/fraktor-actor/src/kernel/version/unsupported_fraktor_version_test.rs` | 原典への言及（Pekko の明記なし） | 「original」「upstream」への言及はあるが、原典が Pekko だと明記が無い | 未確認 |
| 61 | `crates/fraktor-actor/src/kernel/version/version_value_test.rs` | 原典への言及（Pekko の明記なし） | 「original」「upstream」への言及はあるが、原典が Pekko だと明記が無い | 未確認 |
| 62 | `crates/fraktor-actor/tests/byte_string_input_stream_no_copy_test.rs` | 原典への言及（Pekko の明記なし） | 「original」「upstream」への言及はあるが、原典が Pekko だと明記が無い | 未確認 |
| 63 | `crates/fraktor-actor/tests/context_stash_test.rs` | Pekko言及 | Pekko への言及はあるが、原典との対応の明記が無い（設計の参考か写しか未確認） | 未確認 |
| 64 | `crates/fraktor-actor/tests/double_linked_list_model_test.rs` | Pekko言及 | Pekko への言及はあるが、原典との対応の明記が無い（設計の参考か写しか未確認） | 未確認 |
| 65 | `crates/fraktor-actor/tests/event_stream_lower_restart_spec.rs` | Pekko言及 | Pekko への言及はあるが、原典との対応の明記が無い（設計の参考か写しか未確認） | 未確認 |
| 66 | `crates/fraktor-actor/tests/frequency_sketch_allocation_test.rs` | Pekko言及 | Pekko への言及はあるが、原典との対応の明記が無い（設計の参考か写しか未確認） | 未確認 |
| 67 | `crates/fraktor-actor/tests/frequency_sketch_property_test.rs` | 原典への言及（Pekko の明記なし） | 「original」「upstream」への言及はあるが、原典が Pekko だと明記が無い | 未確認 |
| 68 | `crates/fraktor-actor/tests/fsm_test.rs` | Pekko言及 | Pekko への言及はあるが、原典との対応の明記が無い（設計の参考か写しか未確認） | 未確認 |
| 69 | `crates/fraktor-actor/tests/g0f_review_a_no_duplicate_failure_delivery.rs` | Pekko言及 | Pekko への言及はあるが、原典との対応の明記が無い（設計の参考か写しか未確認） | 未確認 |
| 70 | `crates/fraktor-actor/tests/g0f_review_a_system_failure_log.rs` | Pekko言及 | Pekko への言及はあるが、原典との対応の明記が無い（設計の参考か写しか未確認） | 未確認 |
| 71 | `crates/fraktor-actor/tests/g0f_review_c1_intermediate_unsubscribe.rs` | Pekko言及 | Pekko への言及はあるが、原典との対応の明記が無い（設計の参考か写しか未確認） | 未確認 |
| 72 | `crates/fraktor-actor/tests/g0f_review_c2_late_hierarchy_descendant_propagation.rs` | Pekko言及 | Pekko への言及はあるが、原典との対応の明記が無い（設計の参考か写しか未確認） | 未確認 |
| 73 | `crates/fraktor-actor/tests/g0f_review_c2_late_hierarchy_reaches_an_existing_channel.rs` | Pekko言及 | Pekko への言及はあるが、原典との対応の明記が無い（設計の参考か写しか未確認） | 未確認 |
| 74 | `crates/fraktor-actor/tests/g0f_review_c2_parent_first_and_removal.rs` | Pekko言及 | Pekko への言及はあるが、原典との対応の明記が無い（設計の参考か写しか未確認） | 未確認 |
| 75 | `crates/fraktor-actor/tests/g0f_review_c3_other_parent_path.rs` | Pekko言及 | Pekko への言及はあるが、原典との対応の明記が無い（設計の参考か写しか未確認） | 未確認 |
| 76 | `crates/fraktor-actor/tests/g0f_review_e1_level_before_loggers.rs` | Pekko言及 | Pekko への言及はあるが、原典との対応の明記が無い（設計の参考か写しか未確認） | 未確認 |
| 77 | `crates/fraktor-actor/tests/g0f_review_e2_logger_set_change.rs` | Pekko言及 | Pekko への言及はあるが、原典との対応の明記が無い（設計の参考か写しか未確認） | 未確認 |
| 78 | `crates/fraktor-actor/tests/g0f_review_f_terminated_subscription.rs` | Pekko言及 | Pekko への言及はあるが、原典との対応の明記が無い（設計の参考か写しか未確認） | 未確認 |
| 79 | `crates/fraktor-actor/tests/supervision_all_for_one_regression.rs` | Pekko言及 | Pekko への言及はあるが、原典との対応の明記が無い（設計の参考か写しか未確認） | 未確認 |
| 80 | `crates/fraktor-testkit/src/fraktorspec/fraktor_spec_test.rs` | Pekko言及 | Pekko への言及はあるが、原典との対応の明記が無い（設計の参考か写しか未確認） | 未確認 |
| 81 | `crates/fraktor-testkit/src/lib.rs` | 原典パス | 強い根拠はあるが、原典のファイルに行き着かない | 未確認 |
| 82 | `crates/fraktor-testkit/src/manual_scheduler/shared_state.rs` | Pekko言及 | Pekko への言及はあるが、原典との対応の明記が無い（設計の参考か写しか未確認） | 未確認 |
| 83 | `crates/fraktor-testkit/src/manual_scheduler_test.rs` | 原典への対応・移植の語（Pekko の明記なし） | 「corresponding to」「the ported」「original `X`」、または行をまたいだ「the original」がある（H1 の規則 3） | 未確認 |
| 84 | `crates/fraktor-testkit/src/test_kit/expected_messages/class_of.rs` | Mirrors | 強い根拠はあるが、原典のファイルに行き着かない | 未確認 |
| 85 | `crates/fraktor-testkit/src/test_kit/test_kit_value_test.rs` | Pekko言及 | Pekko への言及はあるが、原典との対応の明記が無い（設計の参考か写しか未確認） | 未確認 |
