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
| `java/.../actor/AbstractFSMWithStashActorTest.java` | 1 → 1 | `90b02d6068` #3475（v1.7.1 側の `32f6cb2c01` #3477 と同じ patch-id） | 差分なし |
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

- テスト：v2.0.0 の変更は JUnit 5 化（`@RegisterExtension`、`PekkoJUnitJupiterActorSystemResource`）、`String.format` から `formatted`、`@SuppressWarnings` だけである。ケースと観測は変わらない。stash 版は、両方の系統で同じ patch のコミットが追加した。
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
- 赤：`cargo test -p fraktor-actor --test marker_logging_spec` は終了値 101、E0599 が 2 件（`LogMarkerAdapter::info_with_args` がない）。テストは `6f89346`。
- 緑：`ae2f53d` で 4 passed。
- 負の対照：スライス全体を 1 つの引数（`ArraySeq(a, b, c)`）として最初の `{}` に入れる変異では、`ArraySeq(a, b, c) {} {}` になり、2 件が失敗する。
- `TestEvent.Mute(EventFilter.info())` は、既存の「add markers to logging」と同じ理由で省いた。この port の `EventFilter` には `Info` の種類がない。Mute は購読者への配送を変えない出力の抑制なので、省いても観測は同じである。新しい 2 ケースにも同じ扱いを広げた。除外ではない。
- 追加した公開面：`LogMarkerAdapter::info_with_args<A: fmt::Display>(&self, marker: LogMarker, template: &str, args: &[A])`。
  - 公開範囲：`kernel::actor::Logging::LogMarkerAdapter` のメソッドを 1 つ足しただけである。`log_publisher::format_template` は `pub(crate)` にして再利用した。crate の外には出していない。
  - 既存の呼び出しへの影響：なし。`info(marker, message)` と `error` のシグネチャと挙動は変えていない。
  - 既存 API で表せない理由：`LogMarkerAdapter::info` は書式化を済ませた文字列しか受けない。`LogPublisher::log` は実行時のテンプレートを書式化するが、マーカーを付けられない。Rust の `format!` は書式文字列をコンパイル時の定数に限るので、実行時のテンプレートの `{}` を埋められない。Pekko の `{}` の規則（足りない `{}` は残し、余った引数は ` WARNING arguments left: <n>`）も `format!` にはない。
  - 意図した差異：Pekko は「配列 1 個」を引数に取る。Rust には可変長引数も実行時に配列と判る `Any` もないので、引数は最初からスライスで受ける。非 primitive と primitive の配列は、`&[&str]` と `&[i32]` に当たる。

### ByteIteratorSpec（G0issue130）

- `f481c59dd9` #2952 は、`MultiByteArrayIterator` の `getShort`、`getInt`、`getLong` に、1 つの断片の中で読める場合の速い経路を足した。そして「どの分割点、どのオフセット、どちらのバイト順でも `ByteArrayIterator` と同じ値になる」ケースを足した。同じファイルの `246fae4fb8` #3300（`otherIndexOf` を method handle にした）、`9dfca01566` #3348、`870d4499cc` #2235（import の並び）は、1 件目のアサーションを変えない。
- 赤：終了値 101、E0432（`ByteOrder` がない）と E0599（`get_short`、`get_int`、`get_long` がない）。テストは `a86fa88`。
- 緑：`8886550` で spec は 2 passed。unit テストも 3 件足した。
- 追加した公開面：`kernel::byte_iterator::ByteOrder`（`BigEndian`、`LittleEndian`）、`ByteIterator::get_short`、`get_int`、`get_long`（それぞれ `Option<i16>`、`Option<i32>`、`Option<i64>` を返す）。
- 意図した差異：Pekko は残りのバイトが足りないと `NoSuchElementException` を投げる。port は `None` を返し、何も消費しない（Pekko の `ByteArrayIterator` も、投げる前には何も読まない）。Pekko の `drop(n)` は、テストの補助関数で `n` バイト読み捨てて表した。
- 負の対照：
  - 断片をまたいで読めない（速い経路だけの）変異では、spec の新しいケースが `split=1, off=0` で失敗する。
  - バイト順を取り違える変異は、spec では見つからない。参照の値も同じ関数で読むからである。unit テストの `numbers_are_read_in_the_given_byte_order_across_arrays` と `a_read_consumes_the_number_and_the_next_read_continues_after_it` が失敗する。

### ActorPathSpec（G0a）

- `e336453c66` #3542 は、パス要素の検査を、`String.charAt` から表による検索に変えた（性能の改善）。そして 3 ケースを足した：「accept valid path elements」「reject invalid path elements and report the position」「classify every char like the reference predicate」。
- 赤：終了値 101、E0599（`ActorPath::is_valid_path_element` がない）。テストは `d5d6764`。
- 緑：`8d3b514` で 18 passed。既存の `validate_path_element` は、新しい原典の受理と拒否、位置の報告を最初から満たしていた。足りなかったのは、例外を作らずに真偽を返す公開関数（Pekko の `isValidPathElement`）だけである。
- 意図した差異：
  - 原典は UTF-16 の 65,536 単位をすべて試す。Rust の `str` は単独のサロゲートを持てないので、U+D800〜U+DFFF の 2,048 個を除いた 63,488 個を試す。サロゲートの対は、拒否のケースの `actor-\u{1f600}` で確かめている。
  - 位置は、Java では UTF-16 の単位で、port では char で数える。報告される文字より前はすべて ASCII なので、値は一致する（ASCII 以外の文字は、最初の 1 つで拒否される）。
- 負の対照：ASCII 以外の英数字を受け入れる変異と、先頭の `$` を許す変異では、どちらも拒否と全文字の 2 ケースが失敗する。

### StatusReplySpec（G0issue105）

設計の判断は 2026-11-25 に起動元から受けた（案 A：`Result` を受ける生成関数を 2 つ足す。既存のシグネチャと挙動は保つ。panic は捕まえない）。

- `6287b9de07` #3271「include exception type in toString for non text-error」：Pekko の `toString` は、テキスト以外のエラーを `Error(<例外>)` と表す。port の `StatusReply` には `Display` がない（成功の値は型を消した `Message` で、テキストを持たない）。`Debug` が返信の表示に当たり、エラーの型名を含むので、最初から緑だった（`43e6730`）。負の対照：`Debug` を `Error(<Display>)` に変えると失敗する。
- `f1a36ba76b` #2796（`fromTry`、`fromTryKeepException`、5 ケース）と `037b88ad29` #2812（`fromCallable`、`fromCallableKeepException`、5 ケース）：
  - 赤：終了値 101、E0599 が 10 件（`from_result` が 6 件、`from_result_keep_error` が 4 件）。テストは `0b894e9`。
  - 緑：`0cbb4e0` で 20 passed。
  - 追加した公開面：`StatusReply::from_result<T, E: Error>(Result<T, E>)`（Pekko の `fromTry`。`Err` はテキストだけを `ErrorMessage` に残す）と `StatusReply::from_result_keep_error<T, E: Error + Send + Sync + 'static>(Result<T, E>)`（`fromTryKeepException`。エラーをそのまま残す）。既存の関数のシグネチャと挙動は変えていない。
  - Callable の 5 ケース：Java の `Callable` は失敗を例外で伝える。Rust では `Result` を返すクロージャに当たる。各ケースはクロージャを呼び、その結果を上の 2 関数に渡して、ケースごとに確かめる。`from_callable` は作っていない。Pekko は `NonFatal` だけを捕まえるが、port は panic を捕まえない。Rust の panic は回復できる失敗ではないからである。
  - 意図した差異：Pekko はメッセージが `null` の例外にクラス名（`java.lang.RuntimeException`）を使う。Java では `null` と空文字列は別物で、空のメッセージはそのまま残る。Rust のエラーは必ず `Display` を持ち、`null` に当たる値がない。そこで port は空の `Display` を `null` とみなし、`core::any::type_name::<E>()` に置き換える。型名はコンパイラの表示であり、安定した文字列としては保証しない。テストも、型名の文字列を直接書かずに `type_name` と比べる。
  - 負の対照：空のときに型名へ置き換えない変異では、null のケース 2 件が失敗する。`from_result` がエラーをそのまま残す変異では、`ErrorMessage` への downcast を見る 4 件が失敗する。`from_result_keep_error` がテキストだけを残す変異では、エラーを保つ 2 件が失敗する。

### ByteStringBuilderScala213PlusSpec（G0issue133）

- `f2b677dfa0` #1986 で Scala 2.12 の対応がなくなり、ファイルが `actor-tests/src/test/scala-2.13+/` から `actor-tests/src/test/scala/` に移った（`git diff -M` の類似度 64%）。`fef71efd85` #3539 は、`ByteStringBuilder.addAll` が `ArraySeq.ofByte` を受けたときに配列を複製するようにした。そして「元の配列を書き換えても結果が変わらない」ケースを、不変と可変の `ArraySeq` で 1 件ずつ足した。
- Rust の `add_all` はバイトをコピーするので、最初から緑だった（5 passed、`f68e338`）。`ArraySeq.unsafeWrapArray`（不変の見え方）は `array.iter().copied()` で、`mutable.ArraySeq.make` は `array.iter_mut().map(|b| *b)` で表した。
- 負の対照：作っていない。安全な Rust では、builder が呼び出し元の配列を別名で持つことができない。借用は、配列を書き換える前に終わる。テストでは、配列が書き換わったこと（`zbc`）と、結果が `abc` であることを確かめる。
- 台帳のノード id は v1.7.1 のパス（`scala-2.13+/`）のままである。移動したノードの扱い（id の付け替え、または旧 id のまま移動先を記録するか）は、基準 PR #166 の台帳の形式に合わせて決める。それまでは `verified_against` を変えない。

## 追加した公開面のまとめ

| 公開面 | ノード | 既存の呼び出しへの影響 |
|:---|:---|:---|
| `LogMarkerAdapter::info_with_args` | MarkerLoggingSpec | なし（追加のみ） |
| `byte_iterator::ByteOrder`、`ByteIterator::get_short`、`get_int`、`get_long` | ByteIteratorSpec | なし（追加のみ） |
| `ActorPath::is_valid_path_element` | ActorPathSpec | なし（追加のみ） |
| `StatusReply::from_result`、`from_result_keep_error` | StatusReplySpec | なし（追加のみ） |

kernel の追加は `alloc` と `core` だけで、no_std のままである。

## 検証（HEAD `0cbb4e0` の時点）

| コマンド | 終了値 | 結果 |
|:---|---:|:---|
| `cargo test --workspace` | 0 | 871 passed、0 failed、0 ignored（75 結果） |
| `cargo clippy --workspace --all-targets -- -D warnings` | 0 | 警告なし |
| `cargo fmt --check` | 0 | 差分なし |
| `cargo check -p fraktor-actor --lib --no-default-features` | 0 | 成功 |
| syn-check（CI と同じく `crates/*` ごと、release、locked） | 0 | fraktor-actor 257 ファイル、fraktor-testkit 46 ファイル、violations 0、parse failures 0 |
| `npm run check:aislop` | 0 | accepted と一致 |
| `git diff --check origin/main...HEAD` | 0 | 指摘なし |

## 台帳への反映（PR #166 の後）

台帳（`ledger/nodes.yaml`）は、このまとまりのコミットでは変えていない。PR #166 が main に入った後、その形式（ノード最上位の `verified_against`）に合わせて次を反映する。

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
| ByteStringBuilderScala213PlusSpec | 5/5/5 | 移動の扱いが決まるまで v1.7.1 |
