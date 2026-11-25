# Pekko v2.0.0 への移行（第1まとまり）：移植済み 9 ノード

移植済み（green）のノードを Pekko v2.0.0 の原典と照合し、v2.0.0 で変わったケースや増えたケースを移した記録である（Orca 項目258）。基準の切り替えそのものは PR #166（項目257）が担当する。この記録は、その上に載せる台帳の更新の根拠になる。

## 照合の方法

- 原典：v1.7.1 は `9a54d1294c3ac7821bbed3c940769c3d5027d1ff`、v2.0.0 は `9fbd2283b1cf701c90a1223bf621942c191db61f`。apache/pekko の完全な履歴の clone で確かめた。
- 祖先関係：`git merge-base --is-ancestor v1.7.1 v2.0.0` は偽で、v1.7.1 は v2.0.0 の祖先ではない。共通祖先は `0ca804c7afe9b785ab9e4b5d043e62672d2355e0`（2025-07-27、#1963）である。そのため、`0ca804c7af..v1.7.1`（175 コミット）と `0ca804c7af..v2.0.0`（1154 コミット）を両側から比べた。
- 各ノードについて、テストのファイルと対応する実装のファイルを `git log --follow -M <範囲> -- <path>` と `git diff -M v1.7.1 v2.0.0 -- <path>` で読んだ。両側に入っている同じ修正は `git patch-id` で同一と確かめた。
- 増えたケースと変わったケースは、上流のコミットごとに移した。先に v2.0.0 のテストを置いて実行し、赤か「最初から緑」かを記録した。そのうえで製品を直して緑にした。最初から緑のケースは、負の対照（製品を一時的に壊し、元に戻す）で、テストがその契約を見ていることを確かめた。負の対照の変更は最終の差分に含めていない。

## まとめ

| ノード（`actor-tests/src/test/...`） | v1.7.1 → v2.0.0 のケース数 | 上流のコミット | 結果 |
|:---|:---:|:---|:---|
| `java/.../actor/AbstractFSMActorTest.java` | 1 → 1 | `ec27d6dd8c` #2724、`96b716b6fb` #3191、`4ae53394ac` #3295 | 差分なし（JUnit 5 化と Java 構文） |
| `java/.../actor/AbstractFSMWithStashActorTest.java` | 1 → 1 | `90b02d6068` #3475（1.x 側は `32f6cb2c01` #3477。`AbstractFSM.scala` の変更だけが同じ patch-id） | 差分なし |
| `scala/.../util/VersionSpec.scala` | 8 → 8（アサーション 1 行増） | `640e790be5` #3154 | 最初から緑 |
| `scala/.../event/EventStreamSpec.scala` | 17 → 18 | `25f8b12436` #3658 | 最初から緑 |
| `scala/.../event/MarkerLoggingSpec.scala` | 2 → 4 | `c0e9406bfd` #3263 | 赤 → 製品を直して緑 |
| `scala/.../util/ByteIteratorSpec.scala` | 1 → 2 | `f481c59dd9` #2952 | 赤 → 製品を直して緑 |
| `scala/.../actor/ActorPathSpec.scala` | 15 → 18 | `e336453c66` #3542 | 赤（公開関数がない）→ 1 関数を足して緑 |
| `scala/.../pattern/StatusReplySpec.scala` | 9 → 20 | `6287b9de07` #3271、`f1a36ba76b` #2796、`037b88ad29` #2812 | toString は最初から緑。生成関数の 10 件は赤 → 製品を直して緑 |
| `scala-2.13+/.../util/ByteStringBuilderScala213PlusSpec.scala` | 3 → 5 | `f2b677dfa0` #1986（`scala/` へ移動）、`fef71efd85` #3539 | 移動。増えた 2 件は最初から緑 |

各 Rust テストの冒頭の出典は v2.0.0 の SHA に改め、最初に移植した v1.7.1 の SHA と、v2.0.0 で入った上流のコミットを書き添えた。

## ノードごとの記録

### AbstractFSMActorTest と AbstractFSMWithStashActorTest（G0issue162）

- テスト：v2.0.0 の変更は JUnit 5 化（`@RegisterExtension`、`PekkoJUnitJupiterActorSystemResource`）、`String.format` から `formatted`、`@SuppressWarnings` だけである。ケースと観測は変わらない。stash 版のテストは、同じ修正をそれぞれの系統に入れたコミットが追加した（1.x は `32f6cb2c01` #3477、2.0 は `90b02d6068` #3475）。`git patch-id --stable` が一致するのは `AbstractFSM.scala` の変更だけである（両方 `5e5df93d037d`）。コミット全体は `4769252bc6cf` と `94748091843a` で、テストのファイルも `ed281470c285` と `694536f1f2de` で違う。テストの差は JUnit 4（`JUnitSuite`、`@ClassRule`）と JUnit 5（`@RegisterExtension`）の枠と `@SuppressWarnings` だけで、ケース 1 件と観測は同じである。#3477 には、#3475 にある Java のコンパイル検査のファイル `actor-tests/.../JavaSubclassCompilationCheck.java` が含まれない。
- 実装：`AbstractFSM.scala` の変更は、Java の関数型の名前の置き換え（`FI` の削除 `8e21fd0269`、`japi.Pair` `7209bc8fd7`）と非推奨の `setTimer` の削除（`2323878ae1`）である。`Stash.scala` は `prepend` の型と typo だけ。`DeathWatch.scala` は `@unused` だけ。移植した 2 ケースの観測に掛かる変更はない。
- 後続の FSM suite に引き継ぐ要件（今回の 2 ケースには掛からない）：
  - `63d22b60fe` #3096：`SubscribeTransitionCallBack` と `Listen` で登録した listener を、FSM が `watchWith` で見張る。listener が止まると自動で外す。すでに watch している相手なら警告を出し、自動では外さない。`FSMTransitionSpec` を移植するときは観測の対象になる。
  - `2323878ae1`：非推奨の `setTimer` が消えた。`FSMTimingSpec` を移植する前に、v2.0.0 のケース数を数え直す。
- 結果：`cargo test -p fraktor-actor --test abstract_fsm_actor_test --test abstract_fsm_with_stash_actor_test` は 1 件ずつ pass。

### VersionSpec（G0issue139）

- `640e790be5` #3154 は、`Version.compareTo` で release と pre-release の比べ方を直した（`if` を `else if` に変えた）。`compare extra` に `Version("1.2.0") should be > Version("1.2.0-M1")` が 1 行増えた。ケース数は 8 のまま。
- Rust の `Version` は、最初の移植のときから qualifier のない版を大きいとしていたので、最初から緑だった（8 passed）。
- 負の対照：release と pre-release の比較を v1.7.1 と同じ文字列比較に戻すと、`compare_extra` が増えた行（`version_spec.rs:83`）で失敗する。

### EventStreamSpec（G0f）

- `25f8b12436` #3658 が「remove one unrelated subscription or all subscriptions for the same actor」を足した。上流の実装の変更は doc コメントだけである。
- 原典と同じく `new EventStream(system, false)`（debug なし、unsubscriber なし）で組み立てた。`M` と `String` の購読から `M` だけを外し、その後 `unsubscribe_all` ですべて外す。
- 最初から緑（18 passed）。負の対照：`unsubscribe(subscriber, class)` で全部の購読を外す変異では、新しいケースが失敗する（既存のケースもいくつか失敗する）。

### MarkerLoggingSpec（G0issue108）

- `c0e9406bfd` #3263 は、`MarkerLoggingAdapter.format1` が配列の引数を要素ごとに展開するようにした（`: _*`）。2 ケースが増えた。`info(LogMarker.Security, "{} {} {}", Array("a", "b", "c"))` は `a b c` に、`Array(1, 2, 3)` は `1 2 3` になる。
- 赤：`cargo test -p fraktor-actor --test marker_logging_spec` は終了値 101、E0599 が 2 件（`LogMarkerAdapter::info_with_args` がない）。テストは `0db2c2c`。
- 緑：`a7380bb` で 4 passed。
- 負の対照：スライス全体を 1 つの引数（`ArraySeq(a, b, c)`）として最初の `{}` に入れる変異では、`ArraySeq(a, b, c) {} {}` になり、2 件が失敗する。
- `TestEvent.Mute(EventFilter.info())` は、既存の「add markers to logging」と同じ理由で省いた。この port の `EventFilter` には `Info` の種類がない。Mute は購読者への配送を変えない出力の抑制なので、省いても観測は同じである。新しい 2 ケースにも同じ扱いを広げた。除外ではない。
- 追加した公開面：`LogMarkerAdapter::info_with_args<A: fmt::Display>(&self, marker: LogMarker, template: &str, args: &[A])`。
  - 公開範囲：`kernel::actor::Logging::LogMarkerAdapter` のメソッドを 1 つ足しただけである。`log_publisher::format_template` は `pub(crate)` にして再利用した。crate の外には出していない。
  - 既存の呼び出しへの影響：なし。`info(marker, message)` と `error` のシグネチャと挙動は変えていない。
  - 既存 API で表せない理由：`LogMarkerAdapter::info` は書式化を済ませた文字列しか受けない。`LogPublisher::log` は実行時のテンプレートを書式化するが、マーカーを付けられない。Rust の `format!` は書式文字列をコンパイル時の定数に限るので、実行時のテンプレートの `{}` を埋められない。Pekko の `{}` の規則（足りない `{}` は残し、余った引数は ` WARNING arguments left: <n>`）も `format!` にはない。
  - 意図した差異：Pekko は「配列 1 個」を引数に取る。Rust には可変長引数も実行時に配列と判る `Any` もないので、引数は最初からスライスで受ける。非 primitive と primitive の配列は、`&[&str]` と `&[i32]` に当たる。

### ByteIteratorSpec（G0issue130）

- `f481c59dd9` #2952 は、`MultiByteArrayIterator` の `getShort`、`getInt`、`getLong` に、1 つの断片の中で読める場合の速い経路を足した。そして「どの分割点、どのオフセット、どちらのバイト順でも `ByteArrayIterator` と同じ値になる」ケースを足した。同じファイルの `246fae4fb8` #3300（`otherIndexOf` を method handle にした）、`9dfca01566` #3348、`870d4499cc` #2235（import の並び）は、1 件目のアサーションを変えない。
- 赤：終了値 101、E0432（`ByteOrder` がない）と E0599（`get_short`、`get_int`、`get_long` がない）。テストは `a4e0da4`。
- 緑：`7b97929` で spec は 2 passed。unit テストも 3 件足した。
- 追加した公開面：`kernel::byte_iterator::ByteOrder`（`BigEndian`、`LittleEndian`）、`ByteIterator::get_short`、`get_int`、`get_long`（それぞれ `Option<i16>`、`Option<i32>`、`Option<i64>` を返す）。
- 意図した差異：Pekko は残りのバイトが足りないと `NoSuchElementException` を投げる。port は `None` を返し、何も消費しない（Pekko の `ByteArrayIterator` も、投げる前には何も読まない）。Pekko の `drop(n)` は、テストの補助関数で `n` バイト読み捨てて表した。
- 負の対照：
  - 断片をまたいで読めない（速い経路だけの）変異では、spec の新しいケースが `split=1, off=0` で失敗する。
  - バイト順を取り違える変異は、spec では見つからない。参照の値も同じ関数で読むからである。unit テストの `numbers_are_read_in_the_given_byte_order_across_arrays` と `a_read_consumes_the_number_and_the_next_read_continues_after_it` が失敗する。

### ActorPathSpec（G0a）

- `e336453c66` #3542 は、パス要素の検査を、`String.charAt` から表による検索に変えた（性能の改善）。そして 3 ケースを足した：「accept valid path elements」「reject invalid path elements and report the position」「classify every char like the reference predicate」。
- 赤：終了値 101、E0599（`ActorPath::is_valid_path_element` がない）。テストは `8bed429`。
- 緑：`aee26c0` で 18 passed。既存の `validate_path_element` は、新しい原典の受理と拒否、位置の報告を最初から満たしていた。足りなかったのは、例外を作らずに真偽を返す公開関数（Pekko の `isValidPathElement`）だけである。
- 意図した差異：
  - 原典は UTF-16 の 65,536 単位をすべて試す。Rust の `str` は単独のサロゲートを持てないので、U+D800〜U+DFFF の 2,048 個を除いた 63,488 個を試す。サロゲートの対は、拒否のケースの `actor-\u{1f600}` で確かめている。
  - 位置は、Java では UTF-16 の単位で、port では char で数える。報告される文字より前はすべて ASCII なので、値は一致する（ASCII 以外の文字は、最初の 1 つで拒否される）。
- 負の対照：ASCII 以外の英数字を受け入れる変異と、先頭の `$` を許す変異では、どちらも拒否と全文字の 2 ケースが失敗する。

### StatusReplySpec（G0issue105）

設計の判断は 2026-11-25 に起動元から受けた（案 A：`Result` を受ける生成関数を 2 つ足す。既存のシグネチャと挙動は保つ。panic は捕まえない）。

- `6287b9de07` #3271「include exception type in toString for non text-error」：Pekko の `toString` は、テキスト以外のエラーを `Error(<例外>)` と表す。JVM の例外の `toString` はクラス名で始まるので、エラーが何を出力しても表示に型名が入る。port の `StatusReply` には `Display` がない（成功の値は型を消した `Message` で、テキストを持たない）。表示には `Debug` が当たる。最初の移植では、derive した `Debug` がエラー自身の `Debug` を出すだけだった。テストのエラー（derive の `Debug`）がたまたま型名を出していたため、テストは最初から緑で、返信が型名を付ける契約は確かめていなかった（PR #167 のレビュー指摘）。修正では、`Debug` と `Display` がメッセージだけを出すエラーでテストを書き直した。赤は `StatusReply { result: Err(boho!) }` で、型名が出ない。そのうえで `StatusReply::error` が `core::any::type_name::<E>()` を記録し、`Debug` を手で実装して `Success(<値>)`、`Error(<テキスト>)`、`Error(<型>: <エラー>)` と表すようにした。型名は安定した文字列としては保証しない。`error` の引数は `impl Error` から型引数 `E` に変えたが、呼び出し側から見て受け付ける値は同じである。テキストのエラーの表示 `Error(boho!)` も、同じテストで確かめる。
- `f1a36ba76b` #2796（`fromTry`、`fromTryKeepException`、5 ケース）と `037b88ad29` #2812（`fromCallable`、`fromCallableKeepException`、5 ケース）：
  - 赤：終了値 101、E0599 が 10 件（`from_result` が 6 件、`from_result_keep_error` が 4 件）。テストは `9e4a7d2`。
  - 緑：`bab92f1` で 20 passed。
  - 追加した公開面：`StatusReply::from_result<T, E: Error>(Result<T, E>)`（Pekko の `fromTry`。`Err` はテキストだけを `ErrorMessage` に残す）と `StatusReply::from_result_keep_error<T, E: Error + Send + Sync + 'static>(Result<T, E>)`（`fromTryKeepException`。エラーをそのまま残す）。既存の関数のシグネチャと挙動は変えていない。
  - Callable の 5 ケース：Java の `Callable` は失敗を例外で伝える。Rust では `Result` を返すクロージャに当たる。各ケースはクロージャを呼び、その結果を上の 2 関数に渡して、ケースごとに確かめる。`from_callable` は作っていない。Pekko は `NonFatal` だけを捕まえるが、port は panic を捕まえない。Rust の panic は回復できる失敗ではないからである。
  - 意図した差異：Pekko はメッセージが `null` の例外にクラス名（`java.lang.RuntimeException`）を使う。Java では `null` と空文字列は別物で、空のメッセージはそのまま残る。Rust のエラーは必ず `Display` を持ち、`null` に当たる値がない。そこで port は空の `Display` を `null` とみなし、`core::any::type_name::<E>()` に置き換える。型名はコンパイラの表示であり、安定した文字列としては保証しない。テストも、型名の文字列を直接書かずに `type_name` と比べる。
  - 負の対照（`bab92f1` の実装に対して）：空のときに型名へ置き換えない変異では、null のケース 2 件（`create_from_try_failure_with_null_message`、`create_from_callable_failure_with_null_message`）が失敗する。`from_result` がエラーをそのまま残す変異では 4 件が失敗する。内訳は、`ErrorMessage` への downcast を見る 2 件（`create_from_try_failure_with_message`、`create_from_callable_failure_with_message`）と、型名に置き換えたかを見る null の 2 件である。`from_result_keep_error` がテキストだけを残す変異では、エラーを保つ 2 件（`create_from_try_failure_keeping_exception`、`create_from_callable_keep_exception_failure_keeping_exception`）が失敗する。これらは生成関数の対照で、下の表示の修正（`fc33b60`）の赤と緑とは別である。

### ByteStringBuilderScala213PlusSpec（G0issue133）

- `f2b677dfa0` #1986 で Scala 2.12 の対応がなくなり、ファイルが `actor-tests/src/test/scala-2.13+/` から `actor-tests/src/test/scala/` に移った（`git diff -M` の類似度 64%）。`fef71efd85` #3539 は、`ByteStringBuilder.addAll` が `ArraySeq.ofByte` を受けたときに配列を複製するようにした。そして「元の配列を書き換えても結果が変わらない」ケースを、不変と可変の `ArraySeq` で 1 件ずつ足した。
- Rust の `add_all` はバイトをコピーするので、最初から緑だった（5 passed、`02d9f34`）。`ArraySeq.unsafeWrapArray`（不変の見え方）は `array.iter().copied()` で、`mutable.ArraySeq.make` は `array.iter_mut().map(|b| *b)` で表した。
- 負の対照：作っていない。安全な Rust では、builder が呼び出し元の配列を別名で持つことができない。借用は、配列を書き換える前に終わる。テストでは、配列が書き換わったこと（`zbc`）と、結果が `abc` であることを確かめる。
- 台帳のノード id は、原典のパスなので v2.0.0 の実在するパスに付け替えた（起動元の決定、2026-11-25）。
  - 旧 id：`actor-tests/src/test/scala-2.13+/org/apache/pekko/util/ByteStringBuilderScala213PlusSpec.scala`
  - 新 id：`actor-tests/src/test/scala/org/apache/pekko/util/ByteStringBuilderScala213PlusSpec.scala`
  - 根拠：`f2b677dfa0` #1986（移動）、`fef71efd85` #3539（2 ケースの追加）。ケースは 3 件から 5 件になった。
  - 旧 id を参照していた現役の箇所は、ゴール文書 `docs/goals/issue-133-byte-builder.md` の対象ノード表だけだった（`goal_complete_gate_test.py` が表の id と台帳の所属を突き合わせる）。表の id を新しいパスに直し、追記で旧 id を残した。`needs` と `depends_on_impl` に旧 id への参照はない。
  - 凍結スナップショット `docs/baseline/pekko-v1.7.1/ledger.yaml` の旧パスは、そのまま残した。

## 追加した公開面のまとめ

| 公開面 | ノード | 既存の呼び出しへの影響 |
|:---|:---|:---|
| `LogMarkerAdapter::info_with_args` | MarkerLoggingSpec | なし（追加のみ） |
| `byte_iterator::ByteOrder`、`ByteIterator::get_short`、`get_int`、`get_long` | ByteIteratorSpec | なし（追加のみ） |
| `ActorPath::is_valid_path_element` | ActorPathSpec | なし（追加のみ） |
| `StatusReply::from_result`、`from_result_keep_error` | StatusReplySpec | なし（追加のみ） |

kernel の追加は `alloc` と `core` だけで、no_std のままである。

## 検証（PR #167 の head。台帳 2 コミットとレビュー対応を含む）

CI の必須検査を手元ですべて実行し、終了値はすべて 0 だった。

| コマンド | 結果 |
|:---|:---|
| python のテスト 8 本（`goal_complete_gate_test.py`、`verify_red_test.py`、`verify_green_test*.py` の 3 本、`pekko_baseline_test.py`、`prepare_pekko_source_test.py`、`knowledge_budget_test.py`） | 成功 |
| `pekko_baseline.py check-ledger`（`--pekko pekko --list-unverified` 付きでも）と `check-refs` | 違反なし。green は v2.0.0 が 9、v1.7.1 が 41 |
| `cargo test --workspace` | 0 failed |
| `cargo clippy --workspace --all-targets -- -D warnings` | 警告なし |
| `cargo fmt --check` | 差分なし |
| `cargo check -p fraktor-actor --lib --no-default-features` | 成功 |
| syn-check（CI と同じく `crates/*` ごと、release、locked） | violations 0、parse failures 0 |
| `npm run check:aislop` | accepted と一致 |
| `git diff --check origin/main...HEAD` | 指摘なし |

### PR #167 のレビュー対応

- Devin の指摘（`StatusReply` が型名を付ける契約を確かめていない）：`f339979` でテストを書き直して赤を確かめ、`fc33b60` で表示を直した（上の StatusReply の項）。表示と、型を保つ経路の回帰は、`adaptor/status_reply_test.rs` の unit テスト 3 件で確かめる。内容は、成功の表示と値の downcast、`error` と `from_result_keep_error` の型名の表示と元のエラーへの downcast、`error_message` と `from_result` の `Error(<テキスト>)` と `ErrorMessage` への downcast である。表示から型名を外す変異では、unit テスト 1 件と spec 1 件が失敗する。既存の spec の downcast と ask の経路（20 件）は、修正後も成功する。
- 独立レビュー（e7c7973）：照合の記録 3 件を直した。FSM の patch-id の範囲、StatusReply の負の対照の内訳、リベース後の SHA と台帳 2 コミットである。

## 台帳への反映（PR #166 の後）

台帳（`ledger/nodes.yaml`）の更新は、製品とテストのコミットとは別の 2 コミットにした。`834302d` は 8 ノードの `verified_against: v2.0.0` とケース数、`e7c7973` は ByteStringBuilderScala213PlusSpec の id の付け替えと 5 件である。どちらも #166 の形式（ノード最上位の `verified_against`）に合わせ、main の `62673f7`（#166 のマージ）の上に載せた。

反映後の `uv run --with pyyaml python3 ledger/tools/pekko_baseline.py check-ledger --ledger ledger/nodes.yaml --pekko pekko --list-unverified` は終了値 0 である。green は v2.0.0 が 9、v1.7.1 が 41、do_not_port は v1.7.1 が 173 になった。

| ノード | `cases`（total/ported/passing） | `verified_against` |
|:---|:---|:---|
| AbstractFSMActorTest | 1/1/1（変更なし） | v2.0.0 |
| AbstractFSMWithStashActorTest | 1/1/1（変更なし） | v2.0.0 |
| VersionSpec | 8/8/8（変更なし） | v2.0.0 |
| EventStreamSpec | 18/18/18 | v2.0.0 |
| MarkerLoggingSpec | 4/4/4 | v2.0.0 |
| ByteIteratorSpec | 2/2/2 | v2.0.0 |
| ActorPathSpec | 18/18/18 | v2.0.0 |
| StatusReplySpec | 20/20/20 | v2.0.0 |
| ByteStringBuilderScala213PlusSpec（id を付け替え） | 5/5/5 | v2.0.0 |
