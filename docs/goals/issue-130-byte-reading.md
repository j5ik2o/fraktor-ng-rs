# ゴール G0issue130：classic ByteIteratorSpec と ByteStringInputStreamSpec

[Issue #130](https://github.com/j5ik2o/fraktor-ng-rs/issues/130) と [Issue #131](https://github.com/j5ik2o/fraktor-ng-rs/issues/131)（Orca 項目157）の対象は、Pekko v1.7.1 の classic `ByteIteratorSpec` と `ByteStringInputStreamSpec` で、どちらもバイト列の検索と読み出しを確かめる。`ByteIteratorSpec` は「A ByteIterator should」の下に1ケースを持ち、`indexOf` の検索結果を確かめる。`ByteStringInputStreamSpec` は3ケースを持ち、単一のバイト列、スライス、連結のそれぞれで `asInputStream` の EOF・内容・順序を確かめる。2つのノードを1つのゴールにまとめた。

- `ByteIteratorSpec`: [原典のテスト](https://github.com/apache/pekko/blob/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/actor-tests/src/test/scala/org/apache/pekko/util/ByteIteratorSpec.scala)、[原典の実装](https://github.com/apache/pekko/blob/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/actor/src/main/scala-2.13/org/apache/pekko/util/ByteIterator.scala)
- `ByteStringInputStreamSpec`: [原典のテスト](https://github.com/apache/pekko/blob/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/actor-tests/src/test/scala/org/apache/pekko/util/ByteStringInputStreamSpec.scala)、[原典の実装](https://github.com/apache/pekko/blob/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/actor/src/main/scala-2.13/org/apache/pekko/util/ByteString.scala)、[原典の入力ストリーム](https://github.com/apache/pekko/blob/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/actor/src/main/java/org/apache/pekko/io/UnsynchronizedByteArrayInputStream.java)

## 対象ノード

| id | Rust の統合テスト | 原典のケース数 |
|:---|:---|---:|
| `actor-tests/src/test/scala/org/apache/pekko/util/ByteIteratorSpec.scala` | `crates/fraktor-actor/tests/byte_iterator_spec.rs` | 1 |
| `actor-tests/src/test/scala/org/apache/pekko/util/ByteStringInputStreamSpec.scala` | `crates/fraktor-actor/tests/byte_string_input_stream_spec.rs` | 3 |

台帳はノードごとに次のとおり。合計は原典4件・移植4件・成功4件・除外0件。`needs` はどちらも空で、TestKit の機能は使わない。

| ノード | 原典 | 移植 | 成功 | 除外 |
|:---|---:|---:|---:|---:|
| `ByteIteratorSpec` | 1 | 1 | 1 | 0 |
| `ByteStringInputStreamSpec` | 3 | 3 | 3 | 0 |

原典のケース数は、原典ファイルの `" in {"` の数で数えた（1件と3件）。`ignore` と `pending` はない。`ledger/tools/verify_red.py` の `count_original_cases` は、この WordSpec の書式（`"..." should {` と `"..." must {`）を数えきれず `None` を返すため、手で数えた。

## 実行経路

当初は正規 TAKT の2工程で進める予定だった。ゴールを発行する前に、開始点のブランチ `j5ik2o/issue-130-131-byte-reading` で、台帳の両ノードの `goal:` 行だけを `G0issue130` にした（`3678593`、`ledger/README.md` のゴール発行手順）。そのうえで、担当の端末で takt-mcp（`~/local/takt-custom/bin/takt-mcp`、f6c0d505、serverInfo takt 0.76.1）を stdio で起動し、ゴール `662cbac2-3bc3-4157-9a44-a823aca1f921`（ブランチ `takt/20261124T1032-goal-662cbac2`、統合先 main）を作った。ワークフローは `simple`（親 simple、修正は子 review-remediation）。第1工程はテスト移植と red の実測で、公開 API 名を投入文で固定した手順固定の作業として、runtimeAssignment に `spark-low` を指定した。

2026-11-24T10:33:04Z、第1工程（workKey `issue-130-131/01-source-red`）の `takt_enqueue_goal_task` が「Managed provider opencode is not installed or failed an integrity check」で拒否された。共有の `~/.takt` に opencode の管理ランタイムがなかった。タスクは1件も積まれておらず、実行も0件である（`takt_list_tasks` の全127件に該当なし）。調整担当の指示に従い、共有の `~/.takt` と daemon は変更せず、このゴールにも再投入しなかった。その旨をゴールの作業記録（`ea0a18d8-61b2-42ff-997f-c2ec48eb8168`）に残した。そのあとは同じ担当（Claude Opus 5.5）が直接実装した。手順は TAKT の2工程と同じで、テストの先行、red の実測、実装、等価性レビュー、台帳 green、回帰の順に進めた。

| 段階 | コミット | 内容 |
|:---|:---|:---|
| タグ付け | `3678593` | 両ノードの `goal: G0issue130`（台帳2行） |
| テストと台帳 red | `328f3d3` | 移植テスト2件、両ノードの status `red` と rust の欄。製品差分0 |
| 実装と台帳 green | `38ce78e` | kernel の `byte_iterator`・`byte_string`、単体テスト、両ノードの green、本書 |

## 原典との照合

### ByteIteratorSpec

| 原典 | 移植 |
|:---|:---|
| `freshIterator()` = `ByteArrayIterator(Array(0x20, 0x20, 0x10, 0x20, 0x20, 0x10))`（L27） | `fresh_iterator()` = `ByteIterator::from_array(&[0x20, 0x20, 0x10, 0x20, 0x20, 0x10])` |
| `indexOf(0x20)`→0、`indexOf(0x10)`→2（L28–29） | `index_of(0x20)`→`Some(0)`、`index_of(0x10)`→`Some(2)` |
| `indexOf(0x20, 1)`→1、`indexOf(0x10, 1)`→2、`indexOf(0x10, 3)`→5（L31–33） | `index_of_from(0x20, 1)`→`Some(1)`、`(0x10, 1)`→`Some(2)`、`(0x10, 3)`→`Some(5)` |
| `otherIndexOf`（リフレクションで `indexOf(Byte, Int)` を呼ぶ、L36–40） | `other_index_of`（中身は `index_of_from`） |
| `otherIndexOf(…, 0x20, 1)`→1、`(0x10, 1)`→2、`(0x10, 3)`→5（L42–44） | `other_index_of(…, 0x20, 1)`→`Some(1)`、`(0x10, 1)`→`Some(2)`、`(0x10, 3)`→`Some(5)` |

8つのアサーションを原典と同じ順に並べ、どれも新しいイテレータで検索する（原典 L25–26 のコメントどおり、検索はイテレータを消費する）。原典の `indexOf(elem, from)` には2つのオーバーロードがある。普通の呼び出しが解決する総称の `indexOf[B >: Byte](elem: B, from: Int)` と、リフレクションでしか呼んでいない `indexOf(elem: Byte, from: Int)` で、どちらも `indexWhere(_ == elem, from)` を呼ぶ（実装 L502–506）。Rust にはオーバーロードもリフレクションもないので、どちらも `index_of_from` に対応させた。後半3件は、原典と同じくヘルパー `other_index_of` を通して呼ぶ。原典の「見つからない」は `-1`、移植では `None` である。このケースの期待値はすべて見つかる値なので、`Some(n)` と比べる。

### ByteStringInputStreamSpec

| 原典 | 移植 |
|:---|:---|
| `ByteString1.empty` | `ByteString::empty()` |
| `ByteString1.fromString(s)`、`ByteString1C.fromString(s)`（どちらも UTF-8） | `ByteString::from_string(s)`（UTF-8） |
| `ByteString.fromArray("abcdef".getBytes(US_ASCII), 1, 3)` | `ByteString::from_array(b"abcdef", 1, 3)` |
| `ByteStrings(a, b)` | `a.concat(&b)` |
| `asInputStream` | `as_input_stream()`（`ByteStringInputStream` を返す） |
| `InputStream.read()` の EOF `-1` | `read()` の `None` |
| `InputStream.read(Array(1))` の EOF `-1` | `read_into(&mut [1])` の `None` |
| `toUtf8String`・`toByteArray`・`copy`（L57–80、4096 バイトのバッファで `-1` まで読み、UTF-8 として解釈） | 同名のヘルパー（4096 バイトのバッファで `None` まで読み、`String::from_utf8`） |

| ケース | 原典のアサーション（順） | 移植 |
|:---|:---|:---|
| 「ByteString1 must support asInputStream」 | 空の `read()`→-1、空の `read(Array(1))`→-1、空→""、"abc"→"abc"（L32–35） | `byte_string1_must_support_as_input_stream`。`None`、`None`、`""`、`"abc"` |
| 「ByteString1C must support asInputStream」 | ""→""、"abc"→"abc"、`fromArray(abcdef, 1, 3)`→"bcd"（L40–43） | `byte_string1c_must_support_as_input_stream`。`""`、`"abc"`、`"bcd"` |
| 「ByteStrings must support asInputStream」 | 空2つの連結の `read()`→-1、`read(Array(1))`→-1、→""、"a" と "bc" の連結→"abc"（L48–53） | `byte_strings_must_support_as_input_stream`。`None`、`None`、`""`、`"abc"` |

各アサーションで `as_input_stream()` を新しく呼ぶ点も、原典と同じである。原典の `copy` が返す件数はどこでも使われていないが、移植でも返す形にした。

## 実装

kernel に2つのモジュールを足した。どちらも `core` と `alloc` だけを使い、kernel に std は持ち込んでいない（syn-check の `cfg-std-forbid` は違反0）。

- `fraktor_actor::kernel::byte_iterator::ByteIterator`：1つ以上のバイト配列（`Arc<[u8]>`）を順にたどる、消費型のイテレータ。原典の `ByteArrayIterator`（配列1つ）と `MultiByteArrayIterator`（配列が複数）の両方に当たる。`Iterator<Item = u8>` を実装する。`index_of`/`index_of_from` は原典の `indexWhere`（実装 L487–500）を写した。先に `from` 件を読み飛ばし、そのあと最初に一致したバイトの位置を、読み始めからの番号で返す。読み飛ばした分も番号に数える。共通のバイト走査として、クレート内向けの `has_next` と `copy_into`（配列の境界をまたいでバッファへ移す）も持つ。
- `fraktor_actor::kernel::byte_string::ByteString`：不変のバイト列。原典の3つの表現（配列全体の `ByteString1C`、配列の一部の `ByteString1`、連結の `ByteStrings`）を、共有の配列の列という1つの型で表す。`concat` は両方の配列を共有し、コピーしない（`ByteStrings` と同じ）。空の配列は持たない。`from_array(array, offset, length)` は原典の `CompactByteString.fromArray`（実装 L1234–1243）と同じく、存在するバイトだけをコピーする。`offset` が末尾より後ろなら空になり、`length` が末尾を越えれば末尾で止まる。`iterator()` と `as_input_stream()` は、どちらも先頭から読む。
- `fraktor_actor::kernel::byte_string::ByteStringInputStream`：`ByteIterator` の上で読む。`read()` は次の1バイトを返し、EOF では `None` を返す。`read_into(buffer)` は、読める分を `buffer` の先頭へ移してその件数を返し、EOF では `None` を返す。

| 原典 | 移植 |
|:---|:---|
| `ByteArrayIterator(array)` | `ByteIterator::from_array(&[u8])`（コピーする） |
| `ByteIterator.indexOf(elem)` / `indexOf(elem, from)`（2つのオーバーロード） | `index_of(u8)` / `index_of_from(u8, usize)`。見つからないときは `None` |
| `ByteString.empty`、`fromString(s)`、`fromArray(array, offset, length)` | `ByteString::empty()`、`from_string(&str)`、`from_array(&[u8], usize, usize)` |
| `ByteStrings(a, b)`、`a ++ b` | `a.concat(&b)` |
| `ByteString.iterator`、`asInputStream` | `iterator()`、`as_input_stream()` |
| `InputStream.read()`、`read(byte[])` | `read()`、`read_into(&mut [u8])`。EOF は `None` |

## 等価性レビュー

### 独立レビュー（別系統のモデル）

固定 HEAD で、調整担当が手配する読み取り専用の独立レビューを受ける。結果は PR に記録し、指摘があればこの節に追記する。

### 実装者自身のレビュー

実施者は本 issue の担当（Claude Opus 5.5）。Pekko 固定 SHA の原典テスト2件、`ByteIterator.scala`、`ByteString.scala`、`UnsynchronizedByteArrayInputStream.java` と、移植の実装・テストを突き合わせた。判定は「等価」。原典4ケースは観測上等価で、除外0。

| 原典の振る舞い | 原典の根拠 | 移植で確かめているテスト |
|:---|:---|:---|
| `indexOf(elem)` は `indexWhere(_ == elem, 0)`。読み始めから最初に一致した位置を返す | `ByteIterator.scala` L487–502 | 統合テストの `index_of(0x20)`→0、`index_of(0x10)`→2 |
| `indexOf(elem, from)` は `from` 件を読み飛ばしてから探し、位置は読み始めから数える | L487–500、L503、L506 | 統合テストの `(0x20,1)`→1、`(0x10,1)`→2、`(0x10,3)`→5 を2系統。単体テスト `a_byte_before_the_start_position_is_not_found` |
| 見つからなければ `-1`（`from` が末尾より後ろでも同じ） | L499 | 単体テスト `a_byte_that_is_absent_is_not_found`、`a_start_position_past_the_end_finds_nothing`（`None`） |
| 検索はイテレータを一致した位置まで消費する | L487–500、Spec L25–26 | 単体テスト `a_search_consumes_the_bytes_up_to_the_match` |
| 複数の配列にまたがる位置も通しで数える（`MultiByteArrayIterator` は同じ `indexWhere` を使う） | L206 以降、L487 | 単体テスト `the_index_counts_across_arrays` |
| `fromString` は UTF-8 でエンコードする | `ByteString.scala` L171、L314 | 統合テストの `"abc"`、単体テスト `from_string_encodes_as_utf8` |
| `fromArray(array, offset, length)` は範囲内のバイトだけをコピーする | L100、L1234–1243 | 統合テストの `"bcd"`、単体テスト `from_array_stops_at_the_end_of_the_array` |
| 連結は左のバイトを右より前に並べ、読み出しは部分ごとに続けて読む（`SequenceInputStream`） | L637–638 | 統合テストの `"abc"`、単体テスト `single_byte_reads_cross_concatenated_arrays_in_order`、`a_buffer_read_returns_what_fits_then_the_rest_then_the_end` |
| 空のバイト列は、1バイト読みもバッファ読みも EOF（`-1`） | `UnsynchronizedByteArrayInputStream.java` L123–124・L139–146、`SequenceInputStream` | 統合テストのケース1・3の `None` |
| 1バイト読みとバッファ読みは同じ読み位置を進める | 同 L123–124・L139–158 | 単体テスト `single_byte_and_buffer_reads_share_one_position` |

原典との差異は次のとおりで、原典テストが観測する値は変わらない。

- EOF は `-1` ではなく `None` で返す。見つからない検索も `-1` ではなく `None` で返す。
- 長さ0のバッファへの `read_into` は、EOF かどうかに関係なく `Some(0)` を返す。`InputStream` の契約と、原典の単一表現が使う `UnsynchronizedByteArrayInputStream`（L140–142）に合わせた。原典の連結表現が使う JDK の `SequenceInputStream` は、EOF のときに長さ0でも `-1` を返す。原典の中で表現によって結果が異なる場合で、原典テストはどちらも使わない。単体テスト `an_empty_buffer_reads_zero_bytes_before_and_at_the_end` で移植の振る舞いを確かめている。
- 原典の3つの表現は1つの型にまとめた。`ByteString1.fromString` も `ByteString1C.fromString` も同じ `from_string` である。`ByteString1`（配列の一部を共有する表現）は作っていない。`fromArray` は原典でもコピーしてコンパクトな表現（`ByteString1C`）を返すので、スライスのケースの観測は変わらない。
- `ByteArrayIterator(array)` は原典では配列を共有するが、移植の `from_array` はコピーする。原典テストは配列を書き換えないので、観測は変わらない。
- `Int` の位置と長さは `usize` にした。負の値は型で表せない。原典の `fromArray` が負の `offset`/`length` を0とみなす分岐は、移植には存在しない。
- `ByteString` 全体（`ByteStringSpec` が使う API 群、ビルダー、文字コードの指定、シリアライズ、`ByteBuffer`）や、JVM の `InputStream` の残りの API（`skip`、`available`、`mark`/`reset`、`close` など）は、このノードの契約（テスト）が使わないため移植していない。issue の受け入れ条件どおりである。

## 製品編集前の実測

テストと台帳 red のコミット `328f3d3103b60207a08ee2e77a91e9770cd3406b`（`3678593` からの製品差分0、`git diff --stat 3678593 -- 'crates/*/src'` が空）で次を実行した。テストの blob は `byte_iterator_spec.rs` が `c9fe316e4f6adbe7ded0640bc1bd946857fba8be`、`byte_string_input_stream_spec.rs` が `33bd5146c8e5c37b32a2d93b6abe6241190aaa3a`。

| コマンド | 終了値 | 結果 |
|:---|---:|:---|
| `cargo test -p fraktor-actor --test byte_iterator_spec -- --test-threads=1` | 101 | compile-red。`E0432 unresolved import fraktor_actor::kernel::byte_iterator`（`tests/byte_iterator_spec.rs:30`）の1件。宣言1件、実行0件、除外0件 |
| `cargo test -p fraktor-actor --test byte_string_input_stream_spec -- --test-threads=1` | 101 | compile-red。`E0432 unresolved import fraktor_actor::kernel::byte_string`（`tests/byte_string_input_stream_spec.rs:36`）の1件。宣言3件、実行0件、除外0件 |
| `verify_red.py`（ノードごと。原典を `PEKKO_ORIGINAL_ROOT` に置いて実行） | 0 | 両ノードとも `expected_compile_error`、`excluded: 0`。台帳はすでに red で、書き換えなし |
| `rollup.py --goal G0issue130` | 1 | ノード2件、`{'red': 2}` |

already_green ではない。製品にバイト列の型がなく、両テストとも未解決の import でコンパイルに失敗した。

## 実装後の実測

実装と台帳 green のコミット `38ce78e9d69b1a78054720beb581bb141e15bb49` で次を実行した。テストの blob は製品編集前から変えていない。

| コマンド | 終了値 | 結果 |
|:---|---:|:---|
| `cargo test -p fraktor-actor --test byte_iterator_spec -- --test-threads=1` | 0 | 1 passed / 0 failed / 0 ignored |
| `cargo test -p fraktor-actor --test byte_string_input_stream_spec -- --test-threads=1` | 0 | 3 passed / 0 failed / 0 ignored |
| `cargo test -p fraktor-actor --lib byte_` | 0 | 16 passed（`ByteIterator` 7件、`ByteString` 5件、`ByteStringInputStream` 4件） |
| `cargo test --workspace` | 0 | 57 スイート、480 passed / 0 failed / 0 ignored |
| `cargo fmt --check` | 0 | 差分なし |
| `cargo clippy --workspace --all-targets -- -D warnings` | 0 | 警告なし |
| `cargo run --quiet --release --locked --manifest-path tools/syn-check/Cargo.toml -- --format json crates/fraktor-actor` | 0 | `clean: true`、161 files、違反0、抑制0 |
| `npm ci` → `npm run check:aislop` | 0 | warn 以上の指摘は受理済み一覧と一致 |
| `verify_green.py --crate fraktor-actor` | 0 | `all_pass: true` |

台帳は `G0issue130` と `G0a`〜`G0f` の各々で `check_needs.py` と `rollup.py` を実行し、14コマンドすべて終了値0だった（needs 閉性 OK、全対象ノード green。`G0issue130` は2件で `{'green': 2}`）。

## 再現手順

```text
cargo test -p fraktor-actor --test byte_iterator_spec -- --test-threads=1
cargo test -p fraktor-actor --test byte_string_input_stream_spec -- --test-threads=1
cargo test -p fraktor-actor --lib byte_
cargo test --workspace
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
uv run --with pyyaml python3 ledger/tools/check_needs.py --ledger ledger/nodes.yaml --capabilities ledger/capabilities.yaml --goal G0issue130
uv run --with pyyaml python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal G0issue130
```
