# 設計：#190 util の ByteStringSpec を移植する（分け方と第1群の設計）

[Issue #190](https://github.com/j5ik2o/fraktor-ng-rs/issues/190) の設計の記録である。対象は Pekko v2.0.0（`9fbd2283b1cf701c90a1223bf621942c191db61f`）の `actor-tests/src/test/scala/org/apache/pekko/util/ByteStringSpec.scala`（3933行）である。実行器（#234）には依存しない。この文書は第0段（ケースの分け方）と第1段（最初の群の設計）であり、実装は設計レビューが通ってから第2段で行う。第2段では、テスト（移植テストと PBT）は Claude Opus の担当が先に red で書き、muse はそのテストを通す実装だけを書く。

**#190 の範囲は246件のうち239件である。** 群7の Java 直列化7件（原典2461–2527行）は #190 では移植せず、後続の issue へ分ける。その後続の issue は `actor-tests:PrimitivesSerializationSpec`（`ledger/nodes.yaml` の同名ノード、`status: unported`）を移植する前提になる。7件は除外にせず、台帳の `cases` のどの区分にも書かず、`cases.total` からも外さない（2026-11-27 の元帥の決定。§5.1）。

結論を先に書く。原典のケースは、数え直すと **246件**であり、依頼の目安249件とは3件違う（§1）。246件を、原典の節（`must` のまとまりと `when` のまとまり）に沿って **7群** に分ける。各群は32〜43件であり、依存の順は 群1（`ByteString` 本体の基本操作）→ 群2（範囲操作と等価性）→ 群3（内部表現ごとの検索）→ 群4（`ByteStringIterator`）→ 群5（`ByteStringBuilder` の基礎と符号化の基本と整数の性質群）→ 群6（`ByteStringBuilder` の浮動小数点と `Short` の性質群）→ 群7（直列化と数値の読み取りと表現の横断）である（§2）。台帳のノードは1件だけなので、群1〜6 は `status: red`（`verified_against` 未設定）のまま `ported` と `passing` を積み上げる。#190 の完了時（群7のうち直列化以外の26件を移した時点）も `status: red`（246/239/239）のままとし、`verified_against` は付けない。`status: green` と `verified_against: v2.0.0` は、後続の issue で直列化7件を移したときに付ける（§6）。

最初の群である群1は、ほかの群に依存しない。既存の `ByteString`（`crates/fraktor-actor/src/kernel/byte_string/byte_string_value.rs`）の内部表現をオフセット付きチャンク列（`ByteChunk { bytes: Arc<[u8]>, start, len }`）へ拡張したうえで、呼び出し側の配列をコピーせずに包む生成（`from_shared` / `from_shared_range`、原典の `fromArrayUnsafe`）・長さ・要素取得・ゼロコピー切り出し（`take`、`drop`、`slice`、`split_at` など）・同じ配列で隣り合う断片を1つに戻す連結（`concat`）・`is_compact`・`compact`・3引数 `copy_to_array`・UTF-8 と RFC 4648 Base64 変換（`encode_base64` / `decode_base64`）・空の振る舞いを分けた2系統のスライス取得（`as_byte_slices` / `as_non_empty_byte_slices`）・原典準拠の `Display` を足す設計を §4 に書く。足す本体はすべて `core` と `alloc` だけで書けるため、kernel の no_std 性質は変わらない。

後続へ分けるものと適応の扱いは §5 にまとめた。群7の Java 直列化テスト **全7件**（原典2461–2527行）は、`fraktor-ng-rs` の現行コードベースに直列化の枠組み（`SerializationExtension` や `serde` 等）が存在しない。そのため #190 では移植せず、除外にもせず、どの区分にも数えずに後続の issue へ分ける（2026-11-27 の元帥の決定。§5.1）。7件はすべて群7に属するので、群1の第2段はこの扱いに左右されない。同一性検査（`eq` / `theSameInstanceAs`）については、空の実体の一致を確かめるケース（群3の507行・532行、群7の2830–2832行）は空の値等価＋`is_empty()`＋`is_compact()`＋非空断片数0へ適応し、**非空の `eq` / `theSameInstanceAs`**（群1の2231行・2254/2257行・2301行、群7の2534行・2796行・2807行）は値等価へ弱めずバッキング配列のポインタ共有で検証する（§5.2）。それ以外の JVM と Scala の癖（`ByteBuffer`、Base64、負の添字、見つからない場合の `-1` など）は、`docs/decisions/scala-java-to-rust-correspondence.md`、`docs/decisions/pekko-semantics-first.md`、および `docs/goals/issue-130-byte-reading.md` の前例に従い、Rust で同じ振る舞いを作れるものは伺わずに写す。

作業の基点は、最新の `origin/main`（`68a48a7`、[PR #306](https://github.com/j5ik2o/fraktor-ng-rs/pull/306) のマージ）を取り込んだ状態である。`docs/decisions/scala-java-to-rust-correspondence.md` は `origin/main` に取り込み済みであり、この文書はその最新版を参照している。

## 1. 件数の数え直し

原典の `"ケース名" in {` の数を数えた。ブロックコメントの中のケースは無く（ファイルのブロックコメントはライセンスの頭書きだけである）、`ignore` と `pending` も無い（`pending` の文字列は2171行の `prepending` の一部である）。したがって、数えた246件がすべて実行されるケースである。

| 項目 | 件数 |
|:---|---:|
| 依頼の目安 | 249 |
| 数え直した実数（`"..." in {`） | **246** |
| 差 | -3 |
| 台帳 `ledger/nodes.yaml` の `test_cases_approx`（現行） | 121 |

台帳の `pekko.test_cases_approx` の121件は、凍結済み baseline から機械変換された概数であり、`cases.total` とは別の欄であるため **手で書き換えない**（`docs/decisions/ledger-schema-draft.md:90,157,207`）。第2段の最初の群では `cases.total` だけを246に設定する（§6）。

原典の節ごとの件数は次のとおりである。分け方は、この節の境界と `when` のまとまりを崩さないことを原則にした。

| 原典の節（`must`） | 行 | 件数 |
|:---|---:|---:|
| `ByteString1` | 372–441 | 4 |
| `ByteString1C` | 442–505 | 4 |
| `CompactByteString.apply(IterableOnce[Byte])` | 506–544 | 5 |
| `ByteStrings` | 545–1746 | 25 |
| `ByteString.copyToArray with a source offset` | 1747–1844 | 5 |
| `ByteString.decodeString over a range` | 1845–1925 | 5 |
| `ByteString equality` | 1926–2044 | 8 |
| `ByteStrings.byteAtUnchecked` | 2045–2139 | 8 |
| `A ByteString` | 2140–3027 | 76 |
| `A ByteStringIterator` | 3028–3419 | 37 |
| `A ByteStringBuilder` | 3420–3933 | 69 |
| 合計 |  | **246** |

## 2. 分け方と依存の順

7群への分け方と、群ごとに足す本体の概要は次のとおりである。各群のケースの一覧は §3 に行番号つきで置く。

| 順 | 群 | 件数 | 中身 | 先に要る群 | 足す本体の概要 |
|---:|:---|---:|:---|:---|:---|
| 1 | 群1：`ByteString` 本体の基本操作 | 43 | `A ByteString` の前半（2140–2459行）。大きさ、連続性、連結の向き、圧縮と再結合、文字列と Base64、Vector としての振る舞い | 無し | 内部表現を `ByteChunk` 列へ拡張し、`ByteString` にコピーせずに包む生成（`from_shared`、`from_shared_range`）、長さ、要素取得、ゼロコピー切り出し、`split_at`、`is_compact`、`compact`、3引数 `copy_to_array`、走査、UTF-8/Base64 変換、2系統のスライス取得、`Display` を足す（§4） |
| 2 | 群2：範囲操作と等価性と `byteAtUnchecked` | 32 | `ByteStrings` の切り出し系6件と、範囲つき `copyToArray`、範囲つき `decodeString`、等価性、`byteAtUnchecked` の節 | 群1 | 4引数版 `copy_to_array_from`、範囲の復号、`Hash`、内部向けの要素取得を足す |
| 3 | 群3：内部表現ごとの検索 | 32 | `ByteString1`、`ByteString1C`、`CompactByteString.apply`、`ByteStrings` の検索系19件 | 群1、群2 | `ByteString` に検索（`index_of` 系、部分列の検索、前方一致と後方一致）を足す。表現ごとの型は作らず、既存の1型で担う |
| 4 | 群4：`ByteStringIterator` | 37 | `A ByteStringIterator` の全部 | 群1（`ByteString` の切り出しと長さを使う） | `ByteIterator` に切り出し、走査、`getBytes` 系、浮動小数点の読み取り、バッファへの写しを足す |
| 5 | 群5：`ByteStringBuilder` の基礎と符号化の基本と整数の性質群 | 37 | `A ByteStringBuilder` の3422–3664行。追加の基本、`putInt`、`putLong`（`LongPart` を含む） | 群1（`result()` が返す `ByteString` の基本操作で確かめる） | `ByteStringBuilder` に1バイトの追加、`putByte` と `putBytes`、整数と浮動小数点と `Short` の書き込みの基本形、空かどうかの判定を足す |
| 6 | 群6：`ByteStringBuilder` の浮動小数点と `Short` の性質群 | 32 | `A ByteStringBuilder` の3676–3913行。`putFloat`、`putDouble`、`putShort` | 群5 | 新しい公開 API は足さない。群5で足した書き込みの境界値と往復を確かめる |
| 7 | 群7：直列化と数値の読み取りと表現の横断 | 33 | `A ByteString` の後半（2461–3027行）。直列化、unsafe な共有、数値の読み取り、境界、表現をまたぐ写像と走査、ビルダーの `result` の受け渡し | 群1、群4（イテレータの数値の読み取り）、群5（ビルダーの `result`） | `ByteString` に数値の読み取り（`read_short` 系）を足す。Java 直列化の全7件は後続の issue へ分け、#190 では移植しない（§5.1）。群7のうち #190 で移植するのは26件 |

依存の順の根拠は次のとおりである。群2の範囲つきの写しと復号は、群1の `slice` と `copy_to_array` と等価性の上で定義できる。群3の検索は、群1の切り出しと群2の等価性を使って確かめる。群4のイテレータのケースは、比較の相手として群1の `ByteString` の切り出しを使う。群5と群6のビルダーのケースは、`result()` が返す `ByteString` を群1の操作で確かめる。群7の数値の読み取りは群4のイテレータの読み取りと同じ意味論であり、ビルダーの `result` の受け渡しのケースは群5のビルダーの内部に触れるため、群7を最後に置く。

## 3. 群ごとのケースの一覧と、足す本体

### 群1：`ByteString` 本体の基本操作（43件、原典2140–2459行）

既存の本体は `ByteString` の `empty`、`from_string`、`from_array`、`from_ints`、`concat`、`iterator`、`as_input_stream` と、バイト列としての等価性（`PartialEq` と `Eq`）である。足す本体の設計は §4 にまとめた。ここでは内部表現をオフセット付きチャンク列（`ByteChunk`）へ拡張したうえで、長さと要素取得、ゼロコピー切り出し（`take`、`drop`、`take_right`、`drop_right`、`slice`、`split_at`）、走査（`span`、`take_while`、`drop_while`、`index_where`、`index_where_from`、`index_of`、`index_of_from`）、写し（`to_vec`、3引数 `copy_to_array`）、分割（`grouped`）、圧縮判定と圧縮（`is_compact`、`compact`）、UTF-8 と RFC 4648 Base64 変換（`decode_string`、`encode_base64`、`decode_base64`）、イテレータからの生成（`from_iterator`）、2系統のスライス取得（`as_byte_slices`、`as_non_empty_byte_slices`）、および原典準拠の `Display` を `ByteString` に足す。

| 原典の行 | 原典の節（must／when） | ケース |
|---:|:---|:---|
| 2142 | A ByteString／have correct size | concatenating |
| 2143 | A ByteString／have correct size | dropping |
| 2144 | A ByteString／have correct size | taking |
| 2145 | A ByteString／have correct size | takingRight |
| 2146 | A ByteString／have correct size | dropping then taking |
| 2149 | A ByteString／have correct size | droppingRight |
| 2153 | A ByteString／be sequential | taking |
| 2154 | A ByteString／be sequential | dropping |
| 2158 | A ByteString／preserve concatenation direction | creating ByteString2 from two simple fragments |
| 2171 | A ByteString／preserve concatenation direction | promoting ByteString2 to ByteStrings when prepending or appending another fragment |
| 2187 | A ByteString／be equal to the original | compacting |
| 2192 | A ByteString／be equal to the original | recombining |
| 2204 | A ByteString／be equal to the original | recombining - edge cases |
| 2223 | A ByteString／behave as expected | created from and decoding to String |
| 2229 | A ByteString／behave as expected | taking its own length |
| 2235 | A ByteString／behave as expected | created from and decoding to Base64 |
| 2243 | A ByteString／behave as expected | created from iterator |
| 2250 | A ByteString／behave as expected | compacting |
| 2261 | A ByteString／behave as expected | asByteBuffers |
| 2280 | A ByteString／behave as expected | asByteBuffersUnsafe |
| 2294 | A ByteString／behave as expected | asByteBuffersUnsafe wraps the backing arrays without copying |
| 2310 | A ByteString／behave as expected | toString should start with ByteString( |
| 2317 | A ByteString／behave like a Vector | concatenating |
| 2323 | A ByteString／behave like a Vector | calling apply |
| 2334 | A ByteString／behave like a Vector | calling head |
| 2339 | A ByteString／behave like a Vector | calling tail |
| 2344 | A ByteString／behave like a Vector | calling last |
| 2349 | A ByteString／behave like a Vector | calling init |
| 2354 | A ByteString／behave like a Vector | calling length |
| 2360 | A ByteString／behave like a Vector | calling span |
| 2366 | A ByteString／behave like a Vector | calling takeWhile |
| 2371 | A ByteString／behave like a Vector | calling dropWhile |
| 2376 | A ByteString／behave like a Vector | calling indexWhere |
| 2381 | A ByteString／behave like a Vector | calling indexWhere(p, idx) |
| 2386 | A ByteString／behave like a Vector | calling indexOf |
| 2393 | A ByteString／behave like a Vector | calling indexOf(elem, idx) |
| 2399 | A ByteString／behave like a Vector | calling foreach |
| 2406 | A ByteString／behave like a Vector | calling foldLeft |
| 2411 | A ByteString／behave like a Vector | calling toArray |
| 2417 | A ByteString／behave like a Vector | calling slice |
| 2428 | A ByteString／behave like a Vector | calling take and drop |
| 2439 | A ByteString／behave like a Vector | calling grouped |
| 2447 | A ByteString／behave like a Vector | calling copyToArray |

### 群2：範囲操作と等価性と `byteAtUnchecked`（32件、原典545–2139行のうち該当分）

既存の本体では、等価性はすでにあるが、`Hash` は無い。範囲の扱いについては、`ByteStringInputStream::read_range` が、範囲が宛先をはみ出す場合に `IndexOutOfBoundsError` を返す形をすでに持っている。足す本体は、4引数版の `copy_to_array_from(src_offset, dest, dest_offset, len)`（原典の `copyToArray(srcOffset, dest, destOffset, len)` に対応。群1で足す3引数版 `copy_to_array(dest, start, len)` とは区別する）、範囲を指定する `decode_string`（範囲を切り出してから復号するのと同じ結果を返す形）、内容に基づく `Hash` の実装、および `byteAtUnchecked` に対応するクレート内部の要素の取得である。`byteAtUnchecked` は Pekko の private な最適化であり、その覚え書き（直前に解決した断片）の共有は Rust では作らない。観測できる性質（前後どちらの向きに読んでも、並行に読んでも、正しいバイトを返すこと）は、安全な要素の取得でそのまま確かめる。範囲の切り出しと等価性は群1の `slice` と `Eq` の上に乗り、切り出し系の6件（`drop`、`dropRight`、`slice`、`take`、`copyToArray`、`return same hashCode`）は `ByteStrings` の節からこの群へ置いた。

| 原典の行 | 原典の節（must／when） | ケース |
|---:|:---|:---|
| 546 | ByteStrings | drop |
| 588 | ByteStrings | dropRight |
| 634 | ByteStrings | slice |
| 683 | ByteStrings | take |
| 1216 | ByteStrings | copyToArray |
| 1727 | ByteStrings | return same hashCode |
| 1770 | ByteString.copyToArray with a source offset | copy the same bytes as slice(...).copyToArray, for every offset and length |
| 1793 | ByteString.copyToArray with a source offset | clamp out of range arguments instead of throwing |
| 1805 | ByteString.copyToArray with a source offset | not write past the end of the destination |
| 1812 | ByteString.copyToArray with a source offset | leave an empty ByteString as a no-op |
| 1822 | ByteString.copyToArray with a source offset | copy correctly under repeated access on a fragmented ByteString |
| 1861 | ByteString.decodeString over a range | decode the same string as slice(...).decodeString |
| 1880 | ByteString.decodeString over a range | return the empty string for an empty or inverted range |
| 1889 | ByteString.decodeString over a range | clamp a range that runs past the end |
| 1895 | ByteString.decodeString over a range | decode a multi-byte character that spans two fragments |
| 1907 | ByteString.decodeString over a range | decode every range of a fragmented ByteString, within and across fragments |
| 1954 | ByteString equality | hold between all internal representations of the same content |
| 1966 | ByteString equality | agree with hashCode for all internal representations |
| 1977 | ByteString equality | distinguish content that differs in the first byte |
| 1992 | ByteString equality | distinguish content that differs in the last byte |
| 2006 | ByteString equality | distinguish content of different lengths |
| 2017 | ByteString equality | hold against other Seq[Byte] implementations |
| 2030 | ByteString equality | not consider a ByteString equal to a non-Seq value |
| 2037 | ByteString equality | let equal ByteStrings of different representations share a Set entry |
| 2063 | ByteStrings.byteAtUnchecked | return the right byte when read forwards |
| 2067 | ByteStrings.byteAtUnchecked | return the right byte when read backwards |
| 2071 | ByteStrings.byteAtUnchecked | return the right byte for repeated reads of the same index |
| 2078 | ByteStrings.byteAtUnchecked | return the right byte when alternating between the ends |
| 2086 | ByteStrings.byteAtUnchecked | produce the same bytes as toArray and the iterator |
| 2091 | ByteStrings.byteAtUnchecked | return the right byte when stepping backward onto fragment boundaries |
| 2107 | ByteStrings.byteAtUnchecked | still reject out of range indices |
| 2112 | ByteStrings.byteAtUnchecked | return the right byte when read concurrently from several threads |

### 群3：内部表現ごとの検索（32件、原典372–1746行のうち該当分）

既存の本体では、検索は `ByteIterator` の `index_of` と `index_of_from` だけがあり、`ByteString` 自体は検索を持たない。足す本体は、`ByteString` の `last_index_of`、`contains`、`index_of_slice`、`last_index_of_slice`、`contains_slice`、`starts_with`、`ends_with`（`index_of` と `index_of_from` は群1で追加済み）であり、どれも複数の配列をまたいで探す。原典の `indexOf (specialized)` の節が確かめる SWAR の最適化は実装の内部であり、観測できる性質（通常の実装と同じ位置を返すこと）だけを写す。`ByteString1`、`ByteString1C`、`ByteStrings` の表現ごとの型は作らず、#130 の前例どおり既存の `ByteString` の1型で担い、テストは群1の `ByteChunk` に基づく `.slice(from, until)` と `concat` で各表現（非コンパクト単一スライス・コンパクト単一・多断片）に当たる値を作る。`CompactByteString.apply(IterableOnce[Byte])` は、イテレータを1回だけたどる構築（群1で足す `from_iterator`）に対応させる。空の同一性（`theSameInstanceAs`）の扱いは §5.2 に書いた。

| 原典の行 | 原典の節（must／when） | ケース |
|---:|:---|:---|
| 373 | ByteString1 | drop |
| 390 | ByteString1 | dropRight |
| 407 | ByteString1 | take |
| 420 | ByteString1 | copyToArray |
| 443 | ByteString1C | drop |
| 460 | ByteString1C | dropRight |
| 477 | ByteString1C | take |
| 484 | ByteString1C | copyToArray |
| 507 | CompactByteString.apply(IterableOnce[Byte]) | return empty for an empty collection |
| 512 | CompactByteString.apply(IterableOnce[Byte]) | return the correct bytes for a non-empty collection |
| 517 | CompactByteString.apply(IterableOnce[Byte]) | return a CompactByteString |
| 520 | CompactByteString.apply(IterableOnce[Byte]) | traverse the iterator only once |
| 532 | CompactByteString.apply(IterableOnce[Byte]) | traverse an empty single-use IterableOnce only once |
| 697 | ByteStrings | indexOf |
| 726 | ByteStrings | lastIndexOf |
| 754 | ByteStrings | indexOf from offset |
| 820 | ByteStrings | lastIndexOf from offset |
| 885 | ByteStrings | lastIndexOf (specialized) |
| 1002 | ByteStrings | indexOf (specialized) |
| 1044 | ByteStrings | indexOf (specialized) from offset |
| 1115 | ByteStrings | contains |
| 1128 | ByteStrings | contains (specialized) |
| 1141 | ByteStrings | indexOf from/to |
| 1238 | ByteStrings | containsSlice |
| 1252 | ByteStrings | indexOfSlice |
| 1301 | ByteStrings | indexOfSlice (specialized) |
| 1357 | ByteStrings | lastIndexOfSlice |
| 1409 | ByteStrings | lastIndexOfSlice (specialized) |
| 1473 | ByteStrings | startsWith |
| 1519 | ByteStrings | startsWith (specialized) |
| 1595 | ByteStrings | endsWith |
| 1646 | ByteStrings | endsWith (specialized) |

### 群4：`ByteStringIterator`（37件、原典3028–3419行）

既存の本体は `ByteIterator`（`crates/fraktor-actor/src/kernel/byte_iterator/byte_iterator_value.rs`）であり、`from_array`、複製（`Clone`）、`index_of` と `index_of_from`、`get_short` と `get_int` と `get_long`、クレート内部の `copy_into` と `skip_bytes` を持つ。足す本体は、`get_float` と `get_double`、公開の `get_byte` と `get_bytes` と `get_bytes_as`（`ByteString` を返す形）、バッファへの写し（原典の `copyToBuffer`。`ByteBuffer` ではなく `&mut [u8]` へ写し、写した件数を返す形）、イテレータ自身の切り出しと走査（`slice`、`take`、`drop`、`span`、`take_while`、`drop_while`、`index_where`）である。`duplicate` は既存の `Clone` に対応させる。`InputStream` のラッパーのケースは、既存の `ByteStringInputStream` で確かめる。

| 原典の行 | 原典の節（must／when） | ケース |
|---:|:---|:---|
| 3030 | A ByteStringIterator／behave like a buffered Vector Iterator | concatenating |
| 3038 | A ByteStringIterator／behave like a buffered Vector Iterator | calling head |
| 3043 | A ByteStringIterator／behave like a buffered Vector Iterator | calling next |
| 3048 | A ByteStringIterator／behave like a buffered Vector Iterator | calling hasNext |
| 3053 | A ByteStringIterator／behave like a buffered Vector Iterator | calling length |
| 3058 | A ByteStringIterator／behave like a buffered Vector Iterator | calling duplicate |
| 3067 | A ByteStringIterator／behave like a buffered Vector Iterator | calling span |
| 3073 | A ByteStringIterator／behave like a buffered Vector Iterator | calling takeWhile |
| 3078 | A ByteStringIterator／behave like a buffered Vector Iterator | calling dropWhile |
| 3083 | A ByteStringIterator／behave like a buffered Vector Iterator | calling indexWhere |
| 3088 | A ByteStringIterator／behave like a buffered Vector Iterator | calling indexOf |
| 3093 | A ByteStringIterator／behave like a buffered Vector Iterator | calling toSeq |
| 3098 | A ByteStringIterator／behave like a buffered Vector Iterator | calling foreach |
| 3105 | A ByteStringIterator／behave like a buffered Vector Iterator | calling foldLeft |
| 3110 | A ByteStringIterator／behave like a buffered Vector Iterator | calling toArray |
| 3116 | A ByteStringIterator／behave like a buffered Vector Iterator | calling slice |
| 3127 | A ByteStringIterator／behave like a buffered Vector Iterator | calling take and drop |
| 3138 | A ByteStringIterator／behave like a buffered Vector Iterator | calling copyToArray |
| 3153 | A ByteStringIterator／function as expected | getting Bytes, using getByte and getBytes |
| 3166 | A ByteStringIterator／function as expected | getting Bytes with a given length |
| 3174 | A ByteStringIterator／function as expected | getting ByteString with a given length |
| 3182 | A ByteStringIterator／function as expected | getting Bytes, using the InputStream wrapper |
| 3210 | A ByteStringIterator／function as expected | calling copyToBuffer |
| 3222 | A ByteStringIterator／function as expected | copyToBuffer returns the number of bytes written and respects buffer capacity |
| 3287 | A ByteStringIterator／function as expected | copyToBuffer(buffer, offset) copies from the given offset for different ByteString types |
| 3348 | A ByteStringIterator／function as expected | copying chunks to an array |
| 3357 | A ByteStringIterator／function as expected | calling copyToArray with length passing end of destination |
| 3367 | A ByteStringIterator／decode data correctly | decoding Short in big-endian |
| 3372 | A ByteStringIterator／decode data correctly | decoding Short in little-endian |
| 3377 | A ByteStringIterator／decode data correctly | decoding Int in big-endian |
| 3382 | A ByteStringIterator／decode data correctly | decoding Int in little-endian |
| 3387 | A ByteStringIterator／decode data correctly | decoding Long in big-endian |
| 3392 | A ByteStringIterator／decode data correctly | decoding Long in little-endian |
| 3397 | A ByteStringIterator／decode data correctly | decoding Float in big-endian |
| 3402 | A ByteStringIterator／decode data correctly | decoding Float in little-endian |
| 3407 | A ByteStringIterator／decode data correctly | decoding Double in big-endian |
| 3412 | A ByteStringIterator／decode data correctly | decoding Double in little-endian |

### 群5：`ByteStringBuilder` の基礎と符号化の基本と整数の性質群（37件、原典3422–3664行）

既存の本体は `ByteStringBuilder`（`crates/fraktor-actor/src/kernel/byte_string/byte_string_builder.rs`）であり、`add_all`、`append`、`result` だけを持つ。足す本体は、1バイトの追加、`put_byte` と `put_bytes`、符号化（`put_short`、`put_int`、`put_long`、`put_long_part`、`put_float`、`put_double`。どれも `ByteOrder` を取る形とし、既存の `ByteOrder` を使う）、空かどうかの判定（`is_empty` と長さ）である。`OutputStream` のラッパーのケースは、ビルダーへバイト列を書く小さなアダプタで確かめる。`putInt` と `putLong` の性質群が確かめる `ByteBuffer` との往復は、JDK の型を作らず、値の `to_be_bytes` と `to_le_bytes` が作る参照バイト列との一致として写す（§5.3）。

| 原典の行 | 原典の節（must／when） | ケース |
|---:|:---|:---|
| 3422 | A ByteStringBuilder／function like a VectorBuilder | adding various contents using ++= and += |
| 3439 | A ByteStringBuilder／function as expected | putting Bytes, using putByte and putBytes |
| 3451 | A ByteStringBuilder／function as expected | putting Bytes, using the OutputStream wrapper |
| 3465 | A ByteStringBuilder／encode data correctly | encoding Short in big-endian |
| 3470 | A ByteStringBuilder／encode data correctly | encoding Short in little-endian |
| 3475 | A ByteStringBuilder／encode data correctly | encoding Int in big-endian |
| 3480 | A ByteStringBuilder／encode data correctly | encoding Int in little-endian |
| 3485 | A ByteStringBuilder／encode data correctly | encoding Long in big-endian |
| 3490 | A ByteStringBuilder／encode data correctly | encoding Long in little-endian |
| 3495 | A ByteStringBuilder／encode data correctly | encoding LongPart in big-endian |
| 3500 | A ByteStringBuilder／encode data correctly | encoding LongPart in little-endian |
| 3505 | A ByteStringBuilder／encode data correctly | encoding Float in big-endian |
| 3510 | A ByteStringBuilder／encode data correctly | encoding Float in little-endian |
| 3515 | A ByteStringBuilder／encode data correctly | encoding Double in big-endian |
| 3520 | A ByteStringBuilder／encode data correctly | encoding Double in little-endian |
| 3528 | A ByteStringBuilder／have correct empty info | is empty |
| 3533 | A ByteStringBuilder／have correct empty info | is nonEmpty |
| 3542 | A ByteStringBuilder／putInt | big-endian |
| 3547 | A ByteStringBuilder／putInt | little-endian |
| 3552 | A ByteStringBuilder／putInt | max value big-endian |
| 3557 | A ByteStringBuilder／putInt | min value big-endian |
| 3562 | A ByteStringBuilder／putInt | max value little-endian |
| 3567 | A ByteStringBuilder／putInt | min value little-endian |
| 3572 | A ByteStringBuilder／putInt | result has length 4 |
| 3578 | A ByteStringBuilder／putInt | multiple puts accumulate correctly |
| 3584 | A ByteStringBuilder／putInt | round-trips with ByteBuffer big-endian |
| 3593 | A ByteStringBuilder／putInt | round-trips with ByteBuffer little-endian |
| 3605 | A ByteStringBuilder／putLong | big-endian |
| 3611 | A ByteStringBuilder／putLong | little-endian |
| 3617 | A ByteStringBuilder／putLong | max value big-endian |
| 3623 | A ByteStringBuilder／putLong | min value big-endian |
| 3629 | A ByteStringBuilder／putLong | max value little-endian |
| 3635 | A ByteStringBuilder／putLong | min value little-endian |
| 3641 | A ByteStringBuilder／putLong | result has length 8 |
| 3647 | A ByteStringBuilder／putLong | multiple puts accumulate correctly |
| 3655 | A ByteStringBuilder／putLong | round-trips with ByteBuffer big-endian |
| 3664 | A ByteStringBuilder／putLong | round-trips with ByteBuffer little-endian |

### 群6：`ByteStringBuilder` の浮動小数点と `Short` の性質群（32件、原典3676–3913行）

この群は新しい公開 API を足さない。群5で足した `put_float`、`put_double`、`put_short` について、境界値（最大値、最小値、`NaN`）と、参照バイト列との往復と、複数回の書き込みの蓄積を確かめるケースだけが入る。`NaN` のケースは、ビット列がそのまま書かれることを確かめるものであり、`f32::to_be_bytes` と `f64::to_be_bytes` の意味論でそのまま写せる。

| 原典の行 | 原典の節（must／when） | ケース |
|---:|:---|:---|
| 3676 | A ByteStringBuilder／putFloat | big-endian |
| 3683 | A ByteStringBuilder／putFloat | little-endian |
| 3690 | A ByteStringBuilder／putFloat | max value big-endian |
| 3697 | A ByteStringBuilder／putFloat | min value big-endian |
| 3704 | A ByteStringBuilder／putFloat | max value little-endian |
| 3711 | A ByteStringBuilder／putFloat | min value little-endian |
| 3718 | A ByteStringBuilder／putFloat | NaN big-endian |
| 3725 | A ByteStringBuilder／putFloat | result has length 4 |
| 3731 | A ByteStringBuilder／putFloat | multiple puts accumulate correctly |
| 3742 | A ByteStringBuilder／putFloat | round-trips with ByteBuffer big-endian |
| 3751 | A ByteStringBuilder／putFloat | round-trips with ByteBuffer little-endian |
| 3763 | A ByteStringBuilder／putDouble | big-endian |
| 3770 | A ByteStringBuilder／putDouble | little-endian |
| 3777 | A ByteStringBuilder／putDouble | max value big-endian |
| 3784 | A ByteStringBuilder／putDouble | min value big-endian |
| 3791 | A ByteStringBuilder／putDouble | max value little-endian |
| 3798 | A ByteStringBuilder／putDouble | min value little-endian |
| 3805 | A ByteStringBuilder／putDouble | NaN big-endian |
| 3814 | A ByteStringBuilder／putDouble | result has length 8 |
| 3820 | A ByteStringBuilder／putDouble | multiple puts accumulate correctly |
| 3831 | A ByteStringBuilder／putDouble | round-trips with ByteBuffer big-endian |
| 3840 | A ByteStringBuilder／putDouble | round-trips with ByteBuffer little-endian |
| 3852 | A ByteStringBuilder／putShort | big-endian |
| 3857 | A ByteStringBuilder／putShort | little-endian |
| 3862 | A ByteStringBuilder／putShort | max value big-endian |
| 3867 | A ByteStringBuilder／putShort | min value big-endian |
| 3872 | A ByteStringBuilder／putShort | max value little-endian |
| 3877 | A ByteStringBuilder／putShort | min value little-endian |
| 3882 | A ByteStringBuilder／putShort | result has length 2 |
| 3888 | A ByteStringBuilder／putShort | multiple puts accumulate correctly |
| 3894 | A ByteStringBuilder／putShort | round-trips with ByteBuffer big-endian |
| 3903 | A ByteStringBuilder／putShort | round-trips with ByteBuffer little-endian |

### 群7：直列化と数値の読み取りと表現の横断（33件、原典2461–3027行。#190 で移植するのは26件）

既存の本体では、数値の読み取りは `ByteIterator` だけが持ち、`ByteString` は直接の読み取りを持たない。足す本体は、`ByteString` の `read_short`、`read_int`、`read_long`（どちらもバイト順を取り、データが足りない場合は既存の `IndexOutOfBoundsError` を返す `Result` の形）であり、意味論は群4のイテレータの読み取りとそろえる。そのほかに、表現をまたぐ `map`、ビルダーの `size_hint`、範囲外の `apply` の扱い（群1の `get` が `None` を返す形）、非空のゼロコピー共有検証（2534行 `toArrayUnsafe`、2796行・2807行 `slice(0, len) eq bs`）、空の同一性適応（2830–2832行 `ByteString1.apply`）もこの群で扱う。Java 直列化の全7件（2461–2527行）は、現行の `fraktor-ng-rs` に直列化の仕組みが無いため #190 では移植せず、後続の issue へ分ける（除外にせず、どの区分にも数えない。§5.1）。したがって #190 で移植する群7のケースは、下の表の2529行以降の26件である。ビルダーの `result` の受け渡しの5件（2835–2891行）は、ビルダーが一時バッファをどう手放すか（部分使用バッファが `is_compact() == false` の単一断片になる等）を確かめるため、群1の `ByteChunk` と群5のビルダー実装が固まってから扱う。

| 原典の行 | 原典の節（must／when） | ケース |
|---:|:---|:---|
| 2462 | A ByteString／serialize correctly | given all types of ByteString |
| 2468 | A ByteString／serialize correctly | with a large concatenated bytestring |
| 2475 | A ByteString／serialize correctly | ByteString1C serializes via SerializationProxy not directly |
| 2486 | A ByteString／serialize correctly | CompactByteString instances serialize via SerializationProxy |
| 2497 | A ByteString／serialize correctly | each ByteString concrete type round-trips via serialization |
| 2506 | A ByteString／serialize correctly | given ByteString2 |
| 2515 | A ByteString／serialize correctly | given ByteString2 with sliced fragments |
| 2529 | A ByteString | unsafely wrap and unwrap bytes |
| 2541 | A ByteString | read short values |
| 2563 | A ByteString | read int values |
| 2587 | A ByteString | read long values |
| 2611 | A ByteString | read primitive values across ByteString2 fragment boundaries |
| 2623 | A ByteString | throw IndexOutOfBoundsException for readShortBE/LE with insufficient data |
| 2643 | A ByteString | throw IndexOutOfBoundsException for readIntBE/LE with insufficient data |
| 2663 | A ByteString | throw IndexOutOfBoundsException for readLongBE/LE with insufficient data |
| 2683 | A ByteString | have correct takeRight behaviour for boundary values |
| 2709 | A ByteString | throw IndexOutOfBoundsException when apply is called with an out-of-bounds index |
| 2722 | A ByteString | return 0 from copyToArray when start >= destination length |
| 2737 | A ByteString | correctly handle sizeHint with value smaller than already committed bytes |
| 2747 | A ByteString | correctly handle grouped with size larger than length |
| 2756 | A ByteString | map bytes correctly on each concrete type |
| 2773 | A ByteString | foreach visits each byte in order on each concrete type |
| 2793 | A ByteString | slice returns correct result on ByteString1 with all boundary combinations |
| 2811 | A ByteString | indexOfSlice handles non-Byte typed Seq safely |
| 2821 | A ByteString | lastIndexOfSlice handles non-Byte typed Seq safely |
| 2829 | A ByteString | ByteString1.apply factory returns canonical empty for non-positive length |
| 2835 | A ByteString | ByteStringBuilder.result hands over a mostly used temp buffer without copying |
| 2846 | A ByteString | ByteStringBuilder.result hands over an exactly filled temp buffer without copying |
| 2856 | A ByteString | ByteStringBuilder.result copies when only a small part of the temp buffer is used |
| 2868 | A ByteString | ByteStringBuilder.result is not affected by later writes to the builder |
| 2881 | A ByteString | ByteStringBuilder.result returns the written bytes for any fill level of the temp buffer |
| 2891 | A ByteString | ByteStringBuilder.sizeHint does not shrink existing capacity |
| 2901 | A ByteString | ByteString2 must behave exactly like ByteString1C across every read/transform op |

## 4. 群1の設計（`ByteString` 本体の基本操作、43件）

### 4.1 内部表現の拡張（`ByteChunk` とゼロコピー共有）

既存の `ByteString`（`crates/fraktor-actor/src/kernel/byte_string/byte_string_value.rs:39`）は `arrays: Vec<Arc<[u8]>>` を保持しており、各断片に `(start, len)` を持たない。このままでは (1) チャンクの途中で `take`・`drop`・`slice` するときに必ずバイト列コピーが起きる、(2) バッキング配列の一部だけを参照する非コンパクトな単一断片（`is_compact() == false`、原典の `ByteString1`）を表現できない、(3) 群1の2250行（`compacting`）・2294–2308行（`asByteBuffersUnsafe wraps the backing arrays without copying`）および群7の2835–2844行（部分使用バッファのゼロコピー受け渡し）の契約を満たせない。

そこで群1で、非公開（`pub(crate)`）のチャンク表現を導入し、`ByteString`・`ByteStringBuilder`・`ByteIterator` の3型を同じチャンク表現へそろえる。公開型は増やさず、`tools/syn-check` の `type-per-file`・`module-wiring`・`cfg-std-forbid` を守る。

```rust
#[derive(Clone, Debug)]
pub(crate) struct ByteChunk {
  bytes: Arc<[u8]>,
  start: usize,
  len: usize,
}
```

- **不変条件**：すべての `ByteChunk` は `len > 0 && start.checked_add(len).is_some_and(|end| end <= bytes.len())` を満たす。空のチャンクは作らず、空の `ByteString` は `chunks: Vec::new()`（ヒープ確保0）で表す。
- **スライス参照とコンパクト判定**：
  - `ByteChunk::as_slice(&self) -> &[u8]` は `&self.bytes[self.start..self.start + self.len]` を返す。
  - `ByteChunk::is_full(&self) -> bool` は `self.start == 0 && self.len == self.bytes.len()` を返す。
  - `ByteString::is_compact(&self) -> bool` は「空（`chunks.is_empty()`）」または「単一チャンクかつ `chunks[0].is_full()`」のとき `true`、それ以外（複数チャンク、または単一チャンクで `len < bytes.len()` の部分スライス）のとき `false` とする（原典 `ByteString1C.isCompact == true`、`ByteString1.isCompact == (length == bytes.length)`、`ByteString2.isCompact == false`、`ByteStrings.isCompact == false` に完全一致）。
- **ゼロコピーの切り出しと連結**：
  - `take(n)`：`n == 0` なら `Self::empty()`、`n >= self.len()` なら `self.clone()`（バッキング `Arc<[u8]>` をそのまま共有）。途中の位置なら末尾チャンクの `len` だけ縮め、バイト列は写さない。
  - `drop(n)`：`n == 0` なら `self.clone()`、`n >= self.len()` なら `Self::empty()`。途中の位置なら先頭チャンクの `start` を進めて `len` を縮め、バイト列は写さない。
  - `slice(from, until)`：`from == 0 && until >= self.len()` なら `self.clone()`（共有維持）。`from >= self.len() || until == 0 || from >= until` なら `Self::empty()`。それ以外は `self.drop(from).take(until.min(self.len()) - from)` でゼロコピー切り出しする。
  - `split_at(at)`：`(self.take(at), self.drop(at))` とし、両側とも元の `Arc<[u8]>` を共有する。
  - `concat(other)`：次の順に決める（原典 `ByteString.scala:219-229, 660-672, 1152-1162, 1714` の `++`）。バイト列はどの場合も写さない。
    1. `other` が空なら `self.clone()`、`self` が空なら `other.clone()` を返す。
    2. **両側がともに単一チャンクで、同じ配列の隣り合う範囲なら、1チャンクに結合する。** 条件は `Arc::ptr_eq(&l.bytes, &r.bytes) && l.start + l.len == r.start`（`l = self.chunks[0]`、`r = other.chunks[0]`）であり、結果は `ByteChunk { bytes: l.bytes.clone(), start: l.start, len: l.len + r.len }` の1チャンクである。原典 `ByteString.scala:667-669` の `if ((bytes eq b.bytes) && (startIndex + length == b.startIndex)) new ByteString1(bytes, startIndex, length + b.length)` を写す。
    3. それ以外は、両側のチャンク列をそのまま連結する。
  - 結合が起きるのは、原典では `ByteString1 ++ ByteString1` だけである。`ByteString1C ++ _`（219-229行）、`ByteString2 ++ _`（1152-1162行）、`ByteStrings ++ _`（1714行）は結合しない。Rust では単一チャンクが `ByteString1C` と `ByteString1` のどちらにも当たるが、規則2をどちらにも当ててよい。フルチャンク（`start == 0 && len == bytes.len()`）の右に同じ配列で隣り合う非空チャンクは存在せず（`r.start == bytes.len()` になり不変条件に反する）、左に隣り合う非空チャンクも存在しない（`l.start + l.len == 0` は `l.len > 0` に反する）。そのため、規則2が成り立つのは原典で両側が `ByteString1` の場合に限られ、原典と同じ結果になる。複数チャンクの値の端の断片どうしは、隣り合っていても結合しない（原典2294–2308行の `sliced ++ ByteString1C(other) ++ ByteString1(array, 4, 2)` が3断片 `Seq(array, other, array)` になることと一致する）。
  - **結合の結果として観測できるもの**：`let bs = ByteString::from_string("abcd"); let (a, b) = bs.split_at(2); let c = a.concat(&b);` は、原典では `ByteString1C.take(2)`／`drop(2)` が同じ配列の `ByteString1(bytes, 0, 2)` と `ByteString1(bytes, 2, 2)` を返し、その `++` が `ByteString1(bytes, 0, 4)` になる。設計でも `c` は1チャンク `{start: 0, len: 4}` になり、次の3点が原典と一致する。
    - `c.is_compact() == true`（原典 `ByteString1.isCompact` は `length == bytes.length`、560行）。
    - `c.as_byte_slices().count() == 1`（原典 `ByteString1.asByteBuffers` は `List(asByteBuffer)`、622行）。
    - `c.as_input_stream()` の `mark_supported == true`（原典 `ByteString1.asInputStream` は `UnsynchronizedByteArrayInputStream`、903-904行でマーク可。`ByteString2` と `ByteStrings` は `SequenceInputStream`、1205-1206行・1774-1775行でマーク不可）。
    この3点は `ByteStringSpec` のケースでは直接は観測されない（生成器は断片ごとに別の配列を作り、2901行は `copyOfRange` で写す）。そのため、単体テスト（`byte_string_value_test.rs`）で、上の入力と `Arc::ptr_eq`、および部分スライスどうしの結合（`from_shared_range(arr, 1, 2)` と `from_shared_range(arr, 3, 2)` の連結が1チャンク `{start: 1, len: 4}` で `is_compact() == false` になること）と、隣り合わない場合・別の配列の場合に結合しないことを確かめる。
  - `compact()`：`self.is_compact()` が `true` なら `self.clone()` を O(1) で返す（再確保しない）。`false` のときだけ全バイトを1つの新しい `Arc<[u8]>` へ写し、`start = 0, len = total_len` のコンパクトな `ByteString` を返す。
- **既存モジュールへの整合**：
  - `ByteString::from_array(array, offset, length)` は既存どおり指定範囲をコピーしたコンパクトな1チャンク（空なら `empty()`）を返す（既存 API 互換を維持）。
- **コピーせずに包む生成（原典 `fromArrayUnsafe`、`ByteString.scala:96, 120`）**：群1で次の2つの公開関数を足す。`Arc` は `ByteChunk` が持つのと同じ `crate::kernel::sync::Arc`（ポインタ幅の原子操作を持つ対象では `alloc::sync::Arc`、持たない対象では `portable_atomic_util::Arc`）である。
  - `from_shared(bytes: Arc<[u8]>) -> Self`：原典 `fromArrayUnsafe(array)` = `ByteString1C(array)` に当たる。`bytes` が空なら `Self::empty()`、非空なら `bytes` をそのまま持つフルチャンク `{bytes, start: 0, len: bytes.len()}` の1チャンクを返す。新しい配列は作らず、バイト列も写さない。
  - `from_shared_range(bytes: Arc<[u8]>, offset: usize, length: usize) -> Self`：原典 `fromArrayUnsafe(array, offset, length)` = `ByteString1(array, offset, length)` に当たる。範囲は既存の `from_array` と同じ規則で配列の長さに張り付ける（`let start = offset.min(bytes.len()); let len = length.min(bytes.len() - start);`）。`len == 0` なら `Self::empty()`（原典 `ByteString1.apply` の `length <= 0` で `empty`、523-525行）、それ以外は `{bytes, start, len}` の1チャンクを返す。原典は上端を検査せず、配列を超える範囲を渡すと読んだときに `ArrayIndexOutOfBoundsException` になる値を作る。Rust では不変条件を守るために張り付ける。原典で定義された入力（配列の中の範囲）の結果は変わらない。
  - 原典は「渡した配列を後から書き換えてはならない」という約束を利用者に課す（`ByteString.scala:85-95`）。Rust の `Arc<[u8]>` は共有中に書き換えられない（`Arc::get_mut` は参照が1つのときだけ成功し、`ByteString` が複製を持つ間は失敗する）ため、この約束は型で守られる。したがって関数名に `unsafe` を付けず、`unsafe fn` にもしない。
  - 呼び出し側は `Arc<[u8]>` の複製（`Arc::clone`）を渡し、手元に残した `Arc` を同一性の比較の基準に使える。これが原典の「呼び出し側が持つ配列 `array` と同じインスタンスであること」（`theSameInstanceAs`）の写しになる。群7の2529行（`fromArrayUnsafe` と `toArrayUnsafe` の同一性）も、この生成関数を使う。
  - 別名 `crate::kernel::sync::Arc` は `pub(crate)` のままでよい。型の別名は展開されるので、公開関数の引数に使ってもコンパイルできる（`rustc` で確かめた）。統合テストが走る対象はポインタ幅の原子操作を持つので、テストは `std::sync::Arc<[u8]>` を渡す。
  - `ByteString::as_input_stream(&self)` の `mark_supported` は引き続き `self.chunks.len() <= 1` とする（原典の単一断片＝マーク可、複数断片＝マーク不可と同じ）。
  - `ByteIterator`（`crates/fraktor-actor/src/kernel/byte_iterator/byte_iterator_value.rs`）は `Vec<ByteChunk>` を走査するよう改め、`ByteStringBuilder`（`byte_string_builder.rs`）も `Vec<ByteChunk>` を保持して `append` のゼロコピー共有を維持する。既存の公開 API と既存テストの意味論は一切変えない。
- **非空の `eq` / `theSameInstanceAs` の検証方法**：
  - 統合テスト（`tests/byte_string_spec.rs`）では、`as_non_empty_byte_slices()` が返す各 `&[u8]` の先頭ポインタ（`slice.as_ptr()`）と長さ（`slice.len()`）で、コピーせず同じバッキング配列を共有していることを検証する（値等価に弱めない）。比較の基準 `base_ptr` は、**呼び出し側が `from_shared` / `from_shared_range` に渡した `Arc<[u8]>` の先頭** `Arc::as_ptr(&array).cast::<u8>()` とする（`ByteString` の中から取り出したポインタを基準にはしない）。部分スライスは `base_ptr.wrapping_add(offset)` に一致すること、長さが原典の `limit - position` に一致することを確かめる。
  - 原典2294–2308行は次のように写す。`let array: Arc<[u8]> = Arc::from(&[0u8, 1, 2, 3, 4, 5][..]); let base = Arc::as_ptr(&array).cast::<u8>();` とする。
    - `ByteString1C(array)` は `from_shared(array.clone())` で作り、非空断片が1つで、その先頭が `base`、長さが6であることを確かめる（原典の `map(_.array()) == Seq(array)`）。
    - `ByteString1(array, 1, 3)` は `from_shared_range(array.clone(), 1, 3)` で作り、先頭が `base.wrapping_add(1)`、長さが3であることを確かめる（原典の `theSameInstanceAs(array)` と `(position, limit) == (1, 4)`）。
    - `sliced ++ ByteString1C(other) ++ ByteString1(array, 4, 2)` は `concat` で作り、非空断片が3つで、順に `(base + 1, 3)`、`(Arc::as_ptr(&other) の先頭, 2)`、`(base + 4, 2)` であることを確かめる（原典の `Seq(array, other, array)`）。1つ目と3つ目は同じ配列だが隣り合っていないので、`concat` の規則2では結合しない。
    - `ByteString::empty().as_non_empty_byte_slices()` が0要素であることを確かめる。
  - 単体テスト（`byte_string_value_test.rs`）では、内部の `ByteChunk` について `Arc::ptr_eq(&a.bytes, &b.bytes)` を直接検証する。

### 4.2 公開 API の対応表

足す公開メソッドはすべて `crates/fraktor-actor/src/kernel/byte_string/byte_string_value.rs` の `ByteString` に置き、`core` と `alloc` だけで書く。新しい公開型と新しい公開モジュールは作らない。既存の関数と同じ働きの公開関数が無いことは、追加の前に同ファイルと `byte_iterator_value.rs` と `byte_string_input_stream.rs` で確かめた。

| 原典（Pekko の `ByteString`） | Rust の対応（この群で足す形） | 備考 |
|:---|:---|:---|
| `size`、`length` | `len(&self) -> usize` | 各チャンクの `len` を `saturating_add` で合計し、32ビット対象でもあふれない（`ByteIterator::remaining` の前例） |
| `isEmpty`、`nonEmpty` | `is_empty(&self) -> bool` | 不変条件（空チャンクを持たない）により `self.chunks.is_empty()` と等価 |
| `apply(i)` | `get(&self, index: usize) -> Option<u8>` | 失敗理由は範囲外の1通りだけなので `Option<u8>` とする（`ByteIterator::get_short` 等および `slice::get` と整合）。群7の範囲外ケース（2709行）も `None` で漏れなく観測する。基準：**単一要素取得（`get`・`head`・`last`）は `Option<u8>`、範囲／複数バイト読み取り（既存の `ByteStringInputStream::read_range` や群7の `read_short` 系）は `Result<T, IndexOutOfBoundsError>`** とする（§8.1） |
| `head`、`last` | `head(&self) -> Option<u8>`、`last(&self) -> Option<u8>` | 空の場合は `None`（原典 `ByteString.scala:2268,2270` は `apply(0)` / `apply(length - 1)` で例外）。単一要素取得の基準に従う（§8.1） |
| `tail`、`init` | `tail(&self) -> Self`、`init(&self) -> Self` | 空の場合は `Self::empty()` を返す。原典 `ByteString.scala:2269,2271` 自身が `tail = drop(1)`、`init = dropRight(1)` であり、**空でも例外を投げず `ByteString.empty` を返す原典と同一の振る舞い**である |
| `take(n)`、`drop(n)`、`takeRight(n)`、`dropRight(n)` | `take(&self, n: usize) -> Self`、`drop(&self, n: usize) -> Self`、`take_right(&self, n: usize) -> Self`、`drop_right(&self, n: usize) -> Self` | 負の数は型で表せない（#130 の前例）。`n >= len` でも張り付いて panic しない。`take_right` / `drop_right` は `self.len().saturating_sub(n)` を使い、`usize::MAX` でもあふれない。`take(n >= self.len())` は `self.clone()` でバッキング共有を維持する（2231行） |
| `slice(from, until)` | `slice(&self, from: usize, until: usize) -> Self` | `from >= len || until == 0 || from >= until` なら `Self::empty()`、`from == 0 && until >= len` なら `self.clone()`（共有維持）。範囲は長さで抑え、ゼロコピーで切り出す |
| `splitAt(n)` | `split_at(&self, at: usize) -> (Self, Self)` | `(self.take(at), self.drop(at))` を返す（群1の2192行・2204行が直接呼ぶ）。ゼロコピー共有を維持する |
| `span(p)`、`takeWhile(p)`、`dropWhile(p)` | `span(&self, pred: impl FnMut(u8) -> bool) -> (Self, Self)`、`take_while(&self, pred: impl FnMut(u8) -> bool) -> Self`、`drop_while(&self, pred: impl FnMut(u8) -> bool) -> Self` | 述語は `impl FnMut(u8) -> bool` とし、Rust 標準ライブラリ（`Iterator::take_while` 等）と整合させ、状態を持つテスト用クロージャも受けられるようにする |
| `indexWhere(p)`、`indexWhere(p, from)`、`indexOf(elem)`、`indexOf(elem, from)` | `index_where(&self, pred: impl FnMut(u8) -> bool) -> Option<usize>`、`index_where_from(&self, pred: impl FnMut(u8) -> bool, from: usize) -> Option<usize>`、`index_of(&self, elem: u8) -> Option<usize>`、`index_of_from(&self, elem: u8, from: usize) -> Option<usize>` | 見つからない場合は `None`（#130 の前例）。1引数版と2引数版（`*_from`）の命名を `index_where` と `index_of` で対称にそろえる（原典2376行・2381行・2386行・2393行）。`last_index_of` と部分列検索は群3で足す |
| `foreach(f)`、`foldLeft(z)(op)` | 既存の `iterator()` が返す `ByteIterator`（`Iterator<Item = u8>`）の `for_each` と `fold` で写す | 新しい API は足さない。群1の2399行・2406行のテストでは、デバッグビルドの加算あふれ panic を避けるため、`u8` のまま足さず `u32` または `i32`/`i64` に足し込んで `ByteString` と `Vec<u8>` の両側を同じ型で比較する |
| `toArray` | `to_vec(&self) -> Vec<u8>` | 全バイトを平らにした写しを返す |
| `copyToArray(dest, start, len)` | `copy_to_array(&self, dest: &mut [u8], start: usize, len: usize) -> usize` | **3引数版**（群1の2447行、群2の1216行、群3の420/484行、群7の2722/3006行が呼ぶ）。クランプ式 `let to_copy = len.min(self.len()).min(dest.len().saturating_sub(start));` により、`start >= dest.len()` や `start = usize::MAX` でもあふれず0を返して何も書かない。`src_offset` を取る4引数版（`copy_to_array_from`）は群2で別に足す |
| `grouped(size)` | `grouped(&self, size: usize) -> impl Iterator<Item = ByteString>` | `assert!(size > 0, "size={size} must be positive")` を**空かどうかの判定より先に**検査して panic する（原典 `ByteString.scala:2676` と同じ。空の `ByteString` に対する `grouped(0)` でも即 panic することを単体テストの `#[should_panic]` で確かめる。§8.3）。非空に対する `grouped(usize::MAX)` は1要素を返して終了する |
| `isCompact`、`compact` | `is_compact(&self) -> bool`、`compact(&self) -> Self` | `is_compact()` は空または単一フルチャンクのとき `true`、それ以外は `false`（群1の2187行・2250行・2261行、群3の517行、群7の2835–2866行・3022行）。`compact()` はすでに `is_compact()` なら `self.clone()` で再確保せず同じ `Arc<[u8]>` を共有し、非コンパクトなら1つのフルチャンクへ写す |
| `++`（連結） | 既存の `concat(&self, other: &ByteString) -> Self` | 片方が空なら他方を `clone()`。両側がともに単一チャンクで同じ配列の隣り合う範囲なら1チャンクに結合し（原典 `ByteString.scala:667-669`）、それ以外はチャンク列をゼロコピー連結する（§4.1）。連結の向きと2断片／多断片のケース（2158行・2171行）は `concat` で確かめる。具象型の検査の写し方は表の下の「内部表現の検査の写し方」に書く |
| `fromString(s)`、`decodeString`（UTF-8） | 既存の `from_string(&str) -> Self` と、足す `decode_string(&self) -> String` | この Spec の復号はすべて UTF-8。JDK の `new String(bytes, StandardCharsets.UTF_8)` と同じく Unicode 標準の maximal subpart 置換則（`String::from_utf8_lossy`）で U+FFFD へ置換する（群2の1861行・1907行のマルチバイト途中切り出しにも必須。§8.2） |
| `encodeBase64`、`decodeBase64`（2235行） | `encode_base64(&self) -> Self`、`decode_base64(&self) -> Option<Self>` | 外部クレートは足さず kernel 内の `core`/`alloc` で実装し no_std を保つ。型は原典 `ByteString.scala:2926-2931` に合わせて入出力とも `ByteString` とする（§8.4）。厳密な仕様は表の下の「Base64 の仕様」に定義する |
| `ByteString(iterator)`（2243行） | `from_iterator(iter: impl IntoIterator<Item = u8>) -> Self` | イテレータを1回だけたどり、空なら `Self::empty()`、非空ならコンパクトな単一チャンクを返す。群3の `CompactByteString.apply` も同じ形を使う |
| `fromArrayUnsafe(array)`、`fromArrayUnsafe(array, offset, length)`（2294行、群7の2529行。テストでは `ByteString1C(array)`・`ByteString1(array, offset, length)` の作り方にも使う） | `from_shared(bytes: Arc<[u8]>) -> Self`、`from_shared_range(bytes: Arc<[u8]>, offset: usize, length: usize) -> Self` | 渡された `Arc<[u8]>` をコピーせずに持つ。前者はフルチャンク（`is_compact() == true`）、後者は範囲を張り付けた部分チャンク（配列全体なら `is_compact() == true`、それ以外は `false`）を返し、どちらも長さ0なら `Self::empty()`。詳細は §4.1 |
| `asByteBuffers`、`getByteBuffers`（2261–2278行） | `as_byte_slices(&self) -> impl Iterator<Item = &[u8]>` | `ByteBuffer` 型は作らず read-only の `&[u8]` スライス列で写す。原典 `ByteString.scala:2843` と同じく**常に1要素以上**を返す。空の `ByteString` では空スライス1要素 `&[][..]` を返し、`is_compact() == true` なら常に1要素（全バイト）を返す（2263行・2272行）。`getByteBuffers` は原典でも `asByteBuffers.asJava`（2863-2865行）であり、同じメソッドに写す。部分検査の写し方は表の下に書く。§8.7 |
| `asByteBuffersUnsafe`（2280–2308行） | `as_non_empty_byte_slices(&self) -> impl Iterator<Item = &[u8]>` | 非空断片の `&[u8]` のみを返し、**空の `ByteString` では0要素**を返す（2287行・2290行・2307行）。各スライスは内部の `ByteChunk` のバッキング配列をコピーせず直接借用するため、先頭ポインタと長さでゼロコピー共有を検証できる（2294–2308行）。`as_non_empty_byte_slices().count() == as_byte_slices().filter(|s| !s.is_empty()).count()` を満たす。部分検査の写し方（`hasArray`・`!isReadOnly`・`hasRemaining`）は表の下に書く。§8.7 |
| `toString`（2310行） | `core::fmt::Display` の実装 | 原典 `ByteString.scala:2203, 2683-2689` に完全準拠する。空は `"ByteString()"`、要素は**符号付き8ビット10進（`b as i8`、例：`0xFF` は `"-1"`）**、区切りは `", "`（カンマ＋半角スペース）。**長さ100超の切り詰め規則**：`len > 100` のとき先頭100バイトを `"ByteString(b0, ..., b99)"` と出した直後に `... and [<len - 100>] more` を付与する（例：長さ101なら `"... and [1] more"`） |

#### `asByteBuffers` と `asByteBuffersUnsafe` の部分検査の写し方（2261–2292行）

`ByteBuffer` を作らないので、原典の各 `check` を次のように写す。型で保証されるものも、テストの中に何で保証されるかを書いておく。

| 原典の行 | 原典の検査 | 写し方 |
|---:|:---|:---|
| 2263 | `isCompact` なら `asByteBuffers.size == 1 && asByteBuffers.head == asByteBuffer`、そうでなければ `size > 0` | `asByteBuffer`（単数形、原典 `ByteString.scala:2839`。断片が複数なら `compact` して1つにする）は公開 API に足さない。`is_compact()` のとき `as_byte_slices().count() == 1` かつ `as_byte_slices().next() == Some(&a.to_vec()[..])` で写す（コンパクトな値の `asByteBuffer` は全バイトを包むので、内容の比較で等しい。`ByteBuffer.equals` も残りの内容の比較である）。そうでなければ `as_byte_slices().count() >= 1` |
| 2266 | 断片を `ByteString(bb)` で写して連結すると元に戻る | `as_byte_slices()` の各要素を `ByteString::from_array(s, 0, s.len())` で写して `concat` で畳み、元と等しいことを確かめる |
| 2272 | すべての断片が `isReadOnly` | `as_byte_slices()` は `&[u8]`（共有の借用）を返すので、書き込めないことは型で保証される。実行時の検査は書かず、テストにその旨を書く |
| 2276 | `asByteBuffers` と `getByteBuffers` が要素ごとに等しい | `getByteBuffers` は Java API の別名なので同じメソッドに写す。`as_byte_slices()` を2回呼んで要素ごとに等しいことを確かめる（同じ値から同じ断片列が得られること） |
| 2281 | `asByteBuffersUnsafe` の断片を写して連結すると元に戻る | 2266行と同じ形で `as_non_empty_byte_slices()` について確かめる |
| 2287 | すべての断片が `hasArray && !isReadOnly && hasRemaining` | `hasRemaining` は `!s.is_empty()` で写す。`hasArray`（配列に裏打ちされること）は、`&[u8]` が連続した記憶域の借用であることで型により保証される。`!isReadOnly`（書き込めること）は写さない。共有中の `Arc<[u8]>` に `&mut [u8]` を渡すことは安全な Rust ではできず、原典も呼び出し側に「書いてはならない」と課している（`ByteString.scala:2850-2854`）。この1点は原典の検査を弱める適応なので §5.4 の伺うことに挙げる |
| 2290 | `asByteBuffersUnsafe.size == asByteBuffers.count(_.hasRemaining)` | `as_non_empty_byte_slices().count() == as_byte_slices().filter(\|s\| !s.is_empty()).count()` |

#### 内部表現の検査の写し方（2158行・2171行）

原典は `expectByteString2`・`expectByteStrings`（`ByteStringSpec.scala:108-119`）で実行時の型を検査する。Rust の `ByteString` は1つの型なので、断片の数で写す。

- `ByteString2` は「`as_byte_slices().count() == 2`」、`ByteStrings` は「`as_byte_slices().count() >= 3`」として確かめる。原典の `ByteString2` は非空の2断片だけを持ち、`ByteStrings` は `++` で作られるとき3断片以上を持つ（`ByteString.scala:950-962` の `ByteStrings.apply` は2断片なら `ByteString2` を返す）。
- 2158行の `asByteBuffers.map(_.remaining()).toSeq should ===(Seq(2, 2))` は、`as_byte_slices().map(<[u8]>::len).collect::<Vec<_>>() == [2, 2]` で写す。
- 値の作り方は原典に合わせる。`ByteString1C(Array[Byte](1, 2))` は `from_shared(Arc::from(&[1u8, 2][..]))`、`ByteString1(Array[Byte](0, 3, 4, 5), 1, 2)` は `from_shared_range(Arc::from(&[0u8, 3, 4, 5][..]), 1, 2)` で作る。2171行の `ByteString1C(...)` もすべて `from_shared` で作る。どの値も別々の配列から作るので、`concat` の結合（§4.1 の規則2）は起きず、原典と同じ断片の数になる。

#### Base64 の仕様（RFC 4648 §4・JDK `java.util.Base64` 互換）

原典は `java.util.Base64.getEncoder()` / `getDecoder()` を呼ぶ（`ByteString.scala:213-217, 638-658`）。Pekko 互換のため、以下の仕様を厳密に満たす。

- **アルファベット**：RFC 4648 §4 標準アルファベット（`A-Z` = 0..25、`a-z` = 26..51、`0-9` = 52..61、`+` = 62、`/` = 63）。URL-safe 文字（`-`, `_`）は出力せず、復号でも拒否する。
- **符号化（`encode_base64(&self) -> Self`）**：
  - 空入力は `Self::empty()` を返す。
  - 改行・空白は一切挿入しない。
  - 常に `=` パディングを付け、出力長は4の倍数（`((len + 2) / 3) * 4`）とする。末尾量子（quantum）の未使用ビットは 0 で埋める。
  - 出力はコンパクトな `ByteString`（`is_compact() == true`）とする。
- **復号（`decode_base64(&self) -> Option<Self>`）**（JDK `Base64.getDecoder()` と同一の受理・拒否規則、JDK 実機でも照合済み）：
  - **受理する入力**：
    - 空入力 → `Some(Self::empty())`。
    - 標準アルファベット文字のみで構成され、正規の `=` パディングを持つ長さ `4k` の入力（末尾が `==`、`=`、またはパディング無し）。
    - パディング省略の末尾量子（長さ `4k + 2` で1バイト出力、長さ `4k + 3` で2バイト出力。例：`b"QQ"` → `Some([0x41])`、`b"QUI"` → `Some([0x41, 0x42])`）。
    - 末尾の未使用ビット（non-canonical trailing bits、例：`b"AB=="`・`b"ABB="`）：JDK `Base64.getDecoder()` と同じく未使用ビットをマスクして受理する（`b"AB=="` → `Some([0x00])`、`b"ABB="` → `Some([0x00, 0x10])`）。
  - **拒否する入力（すべて `None`）**：
    - 長さが `4k + 1` の入力（例：`b"A"`, `b"AAAAA"`）。
    - 標準アルファベットと `'='` 以外のあらゆるバイト（ASCII 空白 `' '`・`'\t'`・`'\r'`・`'\n'`、URL-safe の `'-'`・`'_'`、NUL、非 ASCII バイト `>= 0x80`）。
    - 不正な `'='` の位置：4バイト組の0番目・1番目にある `'='`（`b"===="`, `b"A==="`, `b"=AAA"`, `b"A=AA"`）、2番目に `'='` があって3番目が `'='` でない入力（`b"AA="`, `b"AA=A"`）、およびパディングの後ろに文字が続く入力（`b"QQ==A"`, `b"QQ==\n"`, `b"QQ=A"`）。
  - 出力はコンパクトな `ByteString` とする。
- **独立参照による検証**：原典 `ByteStringSpec.scala:2238` が `Base64.getEncoder.encode` と照合するのと同様に、テストでは往復（`encoded.decode_base64() == Some(a)`）だけでなく、(1) **RFC 4648 §10 の既知ベクタ**（`""`→`""`, `"f"`→`"Zg=="`, `"fo"`→`"Zm8="`, `"foo"`→`"Zm9v"`, `"foob"`→`"Zm9vYg=="`, `"fooba"`→`"Zm9vYmE="`, `"foobar"`→`"Zm9vYmFy"`）および `+`/`/` 境界バイト（`[0xFB, 0xFF]`→`"+/8="`、`[0xFF, 0xEF]`→`"/+8="`、`[0x00]`→`"AA=="`）、(2) テストハーネス側に独立に書いた参照エンコーダとの PBT 全件一致、(3) 上記の受理・拒否境界ケース一式を確かめる。

### 4.3 守る性質と、壊しうる入力

守る性質は次の6つである。

1. **平らなバイト列としての意味論。** 群1のすべての操作は、同じ内容を持つ `Vec<u8>` への同じ操作と同じ結果を返す。原典の `likeVector` の形（`ByteString` と `Vector` に同じ操作をして比べる）を、そのまま移植の物差しにする。
2. **不変条件の維持。** `ByteString` の全 `ByteChunk` は `len > 0 && start + len <= bytes.len()` を満たす（空チャンクを持たない）。切り出しの結果が空になる場合は `empty()` を返す。
3. **不変性とゼロコピー共有。** 操作は元の値を変えず、`take`・`drop`・`take_right`・`drop_right`・`slice`・`split_at`・`concat` はバイトを写さずに `Arc<[u8]>` を共有する。非空に対する `take(len)`、`compact()`（すでにコンパクトなとき）、`slice(0, len)`、`from_shared` / `from_shared_range`、および `as_non_empty_byte_slices()` はバッキング配列のポインタ共有を保つ。`concat` は、両側が同じ配列の隣り合う単一チャンクのときに限り1チャンクへ結合し、それ以外では結合しない（原典 `ByteString.scala:667-669`）。
4. **張り付き（例外を投げない切り出しとあふれ防止）。** `take`、`drop`、`take_right`、`drop_right`、`slice`、`split_at`、`copy_to_array` は、どんな `usize` の入力（`usize::MAX` を含む）でも算術あふれで panic せず、Scala と同じ張り付き方・クランプ方をする。
5. **往復と独立参照一致。** `from_string(s).decode_string() == s`、`bs.encode_base64().decode_base64() == Some(bs)` を満たし、さらに `encode_base64` の出力は RFC 4648 §10 既知ベクタおよびテスト側の独立参照エンコーダと一致する。
6. **原典準拠の表示形式。** `Display` は空の `"ByteString()"`、`b as i8` の符号付き10進、`", "` 区切り、および `len > 100` での `... and [<len - 100>] more` 切り詰めを正確に満たす。

壊しうる入力と、その扱いは次のとおりである。

| 入力 | 扱い |
|:---|:---|
| 空の `ByteString` への `head`・`last`・`tail`・`init` | `head` と `last` は `None`、`tail` と `init` は `Self::empty()`（原典と同一） |
| `len()` と等しい添字、長さ以上の `take`・`drop`、`usize::MAX` | `get` は `None`、切り出しは張り付き。`take_right(usize::MAX)` / `drop_right(usize::MAX)` は `len.saturating_sub(n)` でアンダーフローしない |
| `from > until` の `slice`、`slice(usize::MAX, usize::MAX)` | どちらもあふれず `Self::empty()` を返す |
| `copy_to_array(dest, start, len)` での `start >= dest.len()`、`start = usize::MAX, len = 1`、`len < self.len()` | クランプ式 `len.min(self.len()).min(dest.len().saturating_sub(start))` によりあふれず `0` または `len` バイトだけ写す |
| `grouped(0)`（非空および**空の `ByteString` に対しても**） | 空判定より先に `size == 0` を検査し契約違反として即 panic する（`#[should_panic]` で検証） |
| `grouped(usize::MAX)`（非空） | 1要素（自身と等しい `ByteString`）を返して終了し、無限ループしない |
| `ByteString::empty()` に対する `as_byte_slices()` と `as_non_empty_byte_slices()` | 前者は空スライス1要素 `[&[]]`、後者は0要素 `[]` |
| 非コンパクト単一スライス（`from_shared(array.clone()).slice(1, 4)` と `from_shared_range(array.clone(), 1, 3)`、`array` は6バイトの `Arc<[u8]>`） | どちらも `is_compact() == false`、呼び出し側の `array` を共有（先頭ポインタ = `Arc::as_ptr(&array).cast::<u8>().wrapping_add(1)`、長さ 3）、`compact()` で初めて `is_compact() == true` の新規配列になる |
| `split_at` で分けた2つを `concat` し直す（`from_string("abcd").split_at(2)`） | 同じ配列で隣り合うので1チャンクに戻る。`is_compact() == true`、`as_byte_slices().count() == 1`、`mark_supported == true`（原典 `ByteString1 ++ ByteString1` の結合、`ByteString.scala:667-669`） |
| 同じ配列だが隣り合わない断片の連結（`from_shared_range(a, 0, 2)` と `from_shared_range(a, 3, 2)`）、別の配列の断片の連結、複数チャンクの値への連結 | 結合せず、チャンク列の連結になる（2断片以上、`is_compact() == false`、`mark_supported == false`） |
| `from_shared_range` に配列を超える範囲（`offset > len`、`offset + length > len`、`usize::MAX`）を渡す | `from_array` と同じく配列の長さに張り付け、あふれない。張り付けた結果が長さ0なら `Self::empty()` |
| `from_shared` に空の `Arc<[u8]>` を渡す | `Self::empty()`（空のチャンクを作らない不変条件を守る） |
| `Display` の境界入力（`[]`, `[0xFF]`, `[1, 2]`, 長さ 100, 長さ 101） | それぞれ `"ByteString()"`, `"ByteString(-1)"`, `"ByteString(1, 2)"`, 省略なし 100 要素, 先頭 100 要素＋`"... and [1] more"` |
| Base64 の境界入力（`[0xFB, 0xFF]`, `b"QQ"`, `b"AB=="`, `b"QQ==\n"`, `b"QQ =="`, `b"A"`, `b"A==="`, `b"AA="`, `b"A=AA"`, `b"QQ==A"`, `b"-_8="`, `b"\xff\xff\xff\xff"`） | `[0xFB, 0xFF]` は `b"+/8="`、`b"QQ"`→`Some([0x41])`、`b"AB=="`→`Some([0x00])`、空白・改行・URL-safe・不正長・不正 `=`・非 ASCII はすべて `None` |
| 不正な UTF-8 バイト列の `decode_string` | `String::from_utf8_lossy` で U+FFFD へ置換する（JDK 互換） |
| `for_each` / `fold` での加算（2399行・2406行） | テスト側のアキュムレータに `u32` / `i32` / `i64` を使い、`u8` のデバッグ加算あふれを起こさない |
| 大きな連結（チャンク数が多い値）への `len` と `to_vec` | `len` は `saturating_add` で数え、`to_vec` は全バイトを順に写す |

### 4.4 PBT の要否

**PBT を足す。** 群1の原典のケースは、大半が ScalaCheck の `check` で書かれた性質そのものである（「have correct size」の6件、「be sequential」の2件、「behave like a Vector」の21件など）。移植では外部依存を足さず、既存の前例（`crates/fraktor-actor/tests/frequency_sketch_property_test.rs` と同じ固定 seed の SplitMix64）で入力を作る PBT として書き、**失敗時のアサーションメッセージに必ずシード値と反復番号を出力する**（再現可能にする）。

- **原典の生成器の参照行**：`genSimpleByteString`（**L43–49**）、`arbitraryByteString`（**L51–58**）、`arbitraryByteStringSlice`（**L62–71**）、`arbitraryByteStringGrouped`（**L73–80**）。（L92–110 は直列化と型検査のヘルパであり生成器ではない。）
- **生成器の構成**：§4.1 の `ByteChunk` 拡張と `.slice(from, until)` を使い、`genSimpleByteString`（`ByteString(b).slice(from, until)`）をそのまま写すことで、**空・コンパクト単一チャンク・非コンパクト単一スライス（`is_compact() == false`）・2断片連結・多断片連結（3断片以上）**の5形態をまんべんなく生成する。
- **エッジケース入力の写し方（弱めない）**：
  - L2204–2219（`recombining - edge cases`、全8サブケース）：原典の負の `Int`（`-2147483648`, `-324234`）は張り付き先頭の `0`、大きい正の `Int`（`112121212`, `234232`, `2147483647`, `Int.MaxValue`）は同値および `usize::MAX`、空になる `drop(2)`、`from > until`（`usize::MAX, 1`）、部分スライス断片（`from_string("ab").drop(1)`）を含め、全8組み合わせをそのまま検証する。
  - L2192–2198（`recombining`、`check { (xs: ByteString, from: Int, until: Int) => ... }`）：原典の `from` と `until` は任意の `Int` であり、負の値と長さを超える値を含む。`splitAt` の負の引数は `take` が空、`drop` が全体を返すので、`0` と同じ結果になる。そこで `from` と `until` を、それぞれ独立に `0`、内部の添字（`1..len`）、`len`、`len + 1`、`usize::MAX` から生成する（L2204 と L2381 と同じ境界の写し方）。`from > until` の組み合わせも生成から外さない。
  - L2381–2397（`indexWhere(p, idx)` / `indexOf(elem, idx)`）：原典の `math.max(0, idx)` に対応し、`0`・内部の添字・`len`・`len + 1`・`usize::MAX` を満遍なく振る。
- **試行回数と台帳の扱い**：試行回数は原典の設定（`PropertyCheckConfiguration` の `minSuccessful = 1000`、原典 L40–41）に合わせて性質ごとに1000回とする。台帳の件数には PBT の補助性質を別建てで数えず、原典のケース43件への対応で数える（#185 の扱いと同じ）。

### 4.5 Quint の要否

**Quint は書かない。** 群1の対象は不変の値に対する純粋な関数であり、状態の遷移も、複数の主体のやり取りも、時間で変わる前提も無い。守る性質は §4.3 の代数的な性質であり、PBT が直接に確かめる。形式仕様を書いても PBT の性質の言い換えにしかならず、確かめる力が増えない。状態を持つ対象（イテレータの消費とビルダーの蓄積）は群4から群6に入るが、それらも単一の対象の直線的な変化であり、Quint を書くかどうかは各群の設計で改めて判断する。

## 5. 後続の issue へ分けるもの（群7の Java 直列化7件）と、同一性検査の扱い

### 5.1 群7の Java 直列化7件：後続の issue へ分ける（どの区分にも数えない）

群7の `"serialize correctly" when { ... }`（原典 `ByteStringSpec.scala:2461–2527`）に含まれる **全7件** は、いずれも `java.io.ObjectOutputStream` / `ObjectInputStream`（`serialize` / `deserialize` / `testSer`、L92–107）を通じて `ByteString.*.writeReplace()` → `SerializationProxy.writeObject` / `readObject` / `readResolve`（`ByteString.scala:2150–2177`）を実行するテストである。

**決定（2026-11-27、元帥）**：

- 7件は除外（`cases.excluded`）にも、作らない区分（`cases.not_built`）にもしない。台帳の `cases` にも今は書かず、`cases.total`（246）からも外さない。どの区分にも数えずに後に回す。前例は `ExtensionSpec` である（判断待ちのケースを、承認済みとも除外とも記録せず、数からも外していない。`ledger/nodes.yaml` の同ノードの `triage.reason`）。
- #190 は、群1〜6 の213件と群7の残り26件、合わせて **239件** を移した時点で閉じる。7件は後続の issue へ分ける。その後続の issue は `actor-tests:PrimitivesSerializationSpec`（`ledger/nodes.yaml` の同名ノード、`status: unported`）を移植する前提になる。後続の issue は #190 を閉じるときに起こし、その番号を台帳の `triage.reason` に書く（§6）。
- 台帳の終わりの形は `ReceiveTimeoutSpec`（#200）の前例と同じにする。#200 は、後続の issue #293 へ分けたケース9を数から外さず、red（15/14/14）のまま完了とした。#190 も red（246/239/239）のまま完了とする（§6）。
- 直列化の仕組みが入ったときに7件をどう移すか（移植するか、除外を改めて諮るかを含む）は、後続の issue の範囲とする。この設計では決めない。下の表の「後続の issue への注記」は、その判断の材料として残すだけである。

**現行 `fraktor-ng-rs` の調査結果（`origin/main` `68a48a7` 時点で再確認）**：

- ワークスペースの2クレート（`crates/fraktor-actor`、`crates/fraktor-testkit`）を全検索した結果、`serde` の依存も、`SerializationExtension` / `Serializer` 等の直列化モジュール・拡張も **一切存在しない**（`crates/` 内に現れる `Serializer` 語は `kernel/io/dns/` の DNS ワイヤ形式パーサのコメント参照のみ）。
- `docs/goals/issue-192-specs.md:32`（`FunctionRefSpec` の E3）でも「fraktor にはメッセージの直列化が無い」ことが確認されている。また `docs/decisions/crate-structure-comparison.md:58` でも直列化バインディングは後段の任意クレートとして位置づけられている。

**`not_built` に入れない理由**（決定の背景）：

- `docs/decisions/ledger-schema-draft.md:120–128` および `ledger/README.md:142–146` により、`cases.not_built` は (1) 原典言語のための補助そのものの検証かつ (2) 観測できる振る舞いを Rust の言語機能・標準ライブラリ・既存 API で満たせ、ケースごとの `alternative` コード例を示せる場合に限られる。
- 機械検査でも `ledger/tools/rollup.py`（`is_valid_not_built_case`）と `ledger/tools/excluded_approvals.py` が空でない `alternative` を要求する。代替例の無い Java 直列化テストを `not_built` に入れると、規約違反になり CI／完了判定も失敗する。

**`actor-tests:PrimitivesSerializationSpec` との対応**（`PrimitivesSerializationSpec.scala:164–214` の `"ByteStringSerializer" must`。後続の issue の判断材料）：

`PrimitivesSerializationSpec` の `ByteStringSerializer` の節は、6つの場面（`empty string`、`simple content`、`two-part content`、`concatenated content`、`sliced content`、`large concatenated`。169–175行）ごとに、直列化器の解決（177行）、`toBinary` / `fromBinary` の値の往復（182行、`verifySerialization`）、`ByteBuffer` を通した値の往復（186行）を確かめ、さらに `ByteString2` を連続したバイト列として書くこと（191行）と、その逆（197行・203行）を確かめる。この直列化器は `ByteString` を内部表現によらず平らなバイト列として書く。そのため、7件のうち値の往復だけを確かめるものはこの節で拾えるが、JVM の `SerializationProxy` の内部の形を確かめるものは拾えない。

| 原典行 | ケース名 | 原典の検証内容 | `PrimitivesSerializationSpec` との対応 | 後続の issue への注記 |
|---:|:---|:---|:---|:---|
| 2462 | `given all types of ByteString` | 全形態の `ByteString` を `ObjectOutputStream` / `ObjectInputStream` で往復し値等価を検証 | 拾える：182行・186行の値の往復を、169–175行の全形態（空、コンパクト、2断片、連結、部分スライス、大きな連結）について確かめる | 直列化の仕組みが入れば、値の往復として移せる見込みがある |
| 2468 | `with a large concatenated bytestring` | 1000+1000 バイトの連結 `ByteString` を往復し、部分読みの回帰（#20901、`readFully`）を検証 | 拾える：`large concatenated`（174–175行）の場面が同じ値を往復する | 同上 |
| 2475 | `ByteString1C serializes via SerializationProxy not directly` | `ObjectOutputStream` の出力バイト列（ISO-8859-1）に JVM クラス記述子 `"ByteString1C"` が含まれないこと（`writeReplace` 経由）と往復を検証 | 拾えない：JVM の `SerializationProxy` の内部の形（`writeReplace` による置き換えと、直列化ストリームのクラス記述子）の検査である | Rust に同じ仕組みが無い。後続の issue で、移植・除外のどちらにするかを改めて決める |
| 2486 | `CompactByteString instances serialize via SerializationProxy` | `CompactByteString(Array)`・`CompactByteString("hello")` の `ObjectOutputStream` 出力に `"ByteString1C"` が含まれないことと往復を検証 | 拾えない：2475行と同じく `SerializationProxy` の内部の形の検査である | 同上 |
| 2497 | `each ByteString concrete type round-trips via serialization` | `ByteString1C`・`ByteString1`・複合型それぞれを往復し値等価を検証 | 拾える：`simple content`（コンパクト）、`sliced content`（部分スライス）、`two-part content`・`concatenated content`（複合）の値の往復が同じ内部形態を覆う | 直列化の仕組みが入れば、値の往復として移せる見込みがある |
| 2506 | `given ByteString2` | 2断片の `ByteString2` を往復し、復元結果の実行時型が `ByteString2` であることと値等価を検証 | 拾えない：復元後も `ByteString2` であることは、`SerializationProxy` が内部形態ごとの `SerializationIdentity` と断片を書くこと（`ByteString.scala:2150–2177`）による。`ByteStringSerializer` は `ByteString2` を連続したバイト列として書く（191行） | Rust に同じ仕組みが無い。後続の issue で、移植・除外のどちらにするかを改めて決める |
| 2515 | `given ByteString2 with sliced fragments` | 部分スライス2断片の `ByteString2` を往復し、`ByteString2` 型・値等価・断片長 `Seq(2, 3)` の維持を検証 | 拾えない：2506行と同じく `SerializationProxy` の内部の形（断片の境界を保つこと）の検査である | 同上 |

### 5.2 同一性検査（`eq` / `theSameInstanceAs`）の扱い

原典の参照同一性検査は「空の結果に対する同一性」と「非空のバッキング共有に対する同一性」の2種類に分けて扱い、後者は決して値等価へ弱めない。

1. **空の実体に対する同一性（適応して移植・件数は外さない）**：
   - 対象：群3の507行（`return empty for an empty collection`）、532行（`traverse an empty single-use IterableOnce only once`）、および群7の2830–2832行（`ByteString1.apply factory returns canonical empty for non-positive length`）。
   - 理由：Rust の `ByteString::empty()` は `chunks: Vec::new()`（ヒープ確保0）の値型であり、JVM のような単一ヒープ参照のアドレス同一性を持たない。
   - 適応形：`== ByteString::empty()` に加えて `is_empty()`・`is_compact()`・`as_non_empty_byte_slices().count() == 0` を同時に確かめる（イテレータの1回走査検査等はそのまま残す）。原典の意味論を弱めないため、すべて移植として数える。
2. **非空の実体に対する同一性（弱めずポインタ共有で検証）**：
   - 対象：群1の2231行（`b.take(b.length) eq b`）、2254行・2257行（`compact` の `(!wasCompact) || (b eq a)` と `b.compact eq b`）、2301行（`asByteBuffersUnsafe` のバッキング配列共有）、および群7の2534行（`toArrayUnsafe`）、2796行・2807行（`slice(0, len) eq bs`）。
   - 検証方法：値等価に落とさず、§4.1 のとおり `as_non_empty_byte_slices()` が返す各スライスの先頭ポインタ（`as_ptr()`）と長さの一致（部分スライスは `base_ptr.wrapping_add(offset)`）、および単体テストでの `Arc::ptr_eq` でゼロコピー共有を厳密に確かめる。`base_ptr` は、呼び出し側が `from_shared` / `from_shared_range` に渡した `Arc<[u8]>` の先頭（`Arc::as_ptr(&array).cast::<u8>()`）である。原典が値を `ByteString1C(array)` や `fromArrayUnsafe(array)` で作るケース（2301行、群7の2534行）は、`from_shared` で作って呼び出し側の配列との同一性を確かめる。原典が値を別の操作の結果として作るケース（2231行、2254行・2257行、群7の2796行・2807行）は、操作の前の値から `as_non_empty_byte_slices()` で取った先頭を基準に、操作の後も同じ先頭と長さであることを確かめる（原典の `eq` は操作の前後の値の同一性であり、呼び出し側の配列は登場しない）。

### 5.3 注記：提案にしないもの

次のものは Rust で同じ振る舞いを作れるため、保留・除外にせずそのまま写す。群5と群6の `ByteBuffer` との往復は、値の `to_be_bytes` と `to_le_bytes` が作る参照バイト列で写す。群2の `byteAtUnchecked` の並行読みのケースは、安全な要素の取得を複数のスレッドから読む形で写す（統合テストは std を使える。本体の no_std 性質には触れない）。負の添字と負の件数は型で表せないため、原典の負の入力のケースは `usize` の境界（`0`・`len`・`len + 1`・`usize::MAX`）で写す（#130 の前例、§4.4）。

### 5.4 伺うこと（原典の振る舞いを弱める点）

この設計では決めず、オーナーに伺う点である。群1の第2段のテストは、下の「設計の案」の形で書いておき、答えに合わせて直す。

1. **群1の2286–2288行の `!bb.isReadOnly`（`asByteBuffersUnsafe` の断片が書き込めること）。** `as_non_empty_byte_slices()` は `&[u8]` を返すので、書き込めることは写せない。共有中の `Arc<[u8]>` に `&mut [u8]` を渡すことは安全な Rust ではできない。原典も、この断片に書いてはならないと呼び出し側に課している（`ByteString.scala:2850-2854`）。同じ検査の `hasArray` と `hasRemaining` は写す（§4.2 の表の下）。
   - 設計の案：この1点だけを適応とし、2280行のケースは移植として数える（群1の43件に含める）。
   - 弱める点：原典は「断片が書き込める」ことを確かめているが、移植はそれを確かめない。
   - 答えが「適応とは認めない」の場合：2280行のケースをどう数えるか（移植から外すか、別の扱いにするか）を決め直す必要がある。群1の `ported` / `passing` は42件になりうる。

## 6. 台帳の見込み

台帳のノードは `actor-tests/src/test/scala/org/apache/pekko/util/ByteStringSpec.scala` の1件だけであり、現行は `status: unported`、`pekko.test_cases_approx: 121`、`cases.total/ported/passing: null`、`needs.premise_flags: ['java-serialization']`、`goal: null`、`verified_against` 未設定である。

台帳の規約（`docs/decisions/ledger-schema-draft.md`、`ledger/tools/pekko_baseline.py`、`ledger/tools/report_ledger_1to1.py`、`ledger/tools/extract_needs.py`）に従い、次を守る。

- **`verified_against` は #190 の範囲では付けない**（欄なし／`null` のまま）。`pekko_baseline.py` は判定済み以外の `status`（`unported`・`red`）に `verified_against` が付くと即エラーにする。`verified_against: v2.0.0` は、後続の issue で直列化7件を移して `status: green` にするときに付け、あわせて `report_ledger_1to1.py check`（`docs/baseline/v2.0.0-undocumented-nodes.tsv` または報告書）を同期する。
- **`pekko.test_cases_approx`（121）と `needs.premise_flags`（`['java-serialization']`）は手で変更しない。** 前者は凍結済み baseline 由来の概数、後者は `extract_needs.py` が原典から機械抽出する欄であり、手編集は規約違反となる。更新するのは `cases.total`（246）と `cases.ported` / `cases.passing` だけである。`cases.excluded` は `[]` のままにし、`cases.not_built` の欄は足さない。
- **`cases.total` は246のまま減らさない。** `total` は Pekko 側のケースの総数である（`docs/decisions/ledger-schema-draft.md:89`）。直列化7件は数からは外さない（§5.1）。
- **群1〜6 と #190 の完了時の `status` は `red` とする**（`ExtensionSpec`・`ReceiveTimeoutSpec` の先例と同じ）。群1完了時に `rust.crate: fraktor-actor`、`rust.test_file: crates/fraktor-actor/tests/byte_string_spec.rs`、`goal: G0issue190`、`triage.kind: pending`、`triage.reason`（完了済みの群と残りの群の記録）を設定する。なお、台帳に `status: red` のノードが複数ある状態で `verify_green.py` を呼ぶとフォールバック検索が曖昧エラーになるため、実行時は `--crate fraktor-actor`（および対象ノード指定）を明示する。
- **#190 の完了時の `triage.reason`** は、`ReceiveTimeoutSpec`（`ledger/nodes.yaml` の同ノード）の書き方にならって次の趣旨を書く。「移植239件は成功している。Java 直列化7件（原典2461–2527行）は後続の issue #NNN（`actor-tests:PrimitivesSerializationSpec` の移植の前提）へ分け、数からは外さない。2026-11-27 に元帥が、台帳のノードを red（246/239/239）のまま #190 を完了とすることを決めた（`docs/design/issue-190-bytestring.md` §5.1）」。`#NNN` は #190 を閉じるときに起こす後続の issue の番号である。

| 段 | `cases.total` | `cases.ported` | `cases.passing` | `cases.excluded` | `status` | `verified_against` |
|:---|---:|---:|---:|:---|:---|:---|
| 群1の完了 | 246 | 43 | 43 | `[]` | `red` | 未設定（`null`） |
| 群2の完了 | 246 | 75 | 75 | `[]` | `red` | 未設定（`null`） |
| 群3の完了 | 246 | 107 | 107 | `[]` | `red` | 未設定（`null`） |
| 群4の完了 | 246 | 144 | 144 | `[]` | `red` | 未設定（`null`） |
| 群5の完了 | 246 | 181 | 181 | `[]` | `red` | 未設定（`null`） |
| 群6の完了 | 246 | 213 | 213 | `[]` | `red` | 未設定（`null`） |
| #190 の完了（群7の26件） | 246 | 239 | 239 | `[]` | `red` | 未設定（`null`） |

後続の issue で直列化7件を移したら、`ported = passing = 246`、`status: green`、`verified_against: v2.0.0` にし、`report_ledger_1to1.py check` を同期する。7件の一部を除外にすると後続の issue で決めた場合は、承認（`approved_by`・`approved_at`）を得た `cases.excluded` と `ported` の合計が246になったときに `green` にする。

**#190 の完了は、台帳の道具ではなく決定の記録（§5.1）で判断する。** `origin/main` の道具で次を確かめた。

- `pekko_baseline.py check-ledger`：`red` で `verified_against` が無いノードは合格する（`JUDGED_STATUSES` の外のノードには値が無いことだけを求める）。
- `report_ledger_1to1.py check`：判定済みでないノードは未記載の一覧（規則5）の対象外である。報告書に載っていても、照合タグの欄が空欄（`null`）なら規則1を満たす。
- `rollup.py`（ゴールを指定しない全体の検査）：合格する。`mixed_not_built_problems` は `green` だけを見る。
- `rollup.py --goal G0issue190`：`red` は `DONE_STATUSES` に入らないので、終了コード1（未完了）になる。red のまま完了とした `G0issue200`（`ReceiveTimeoutSpec`）と `G0issue193`（`ExtensionSpec`）も同じ状態である。`.github/workflows/ci.yml` は `rollup.py --goal` と `goal_complete_gate.py` の本体を実行しない（実行するのは `ledger/tools/*_test.py`、`check-ledger`、`check-refs`、`relocate_nodes.py check`、`report_ledger_1to1.py check`）ので、CI は落ちない。
- **`green` にしてはいけない。** green の条件は `passing == ported` かつ `ported + 承認済みの excluded == total`（`docs/decisions/ledger-schema-draft.md:111-112, 190-191`）であり、239 + 0 ≠ 246 なので満たさない。ところが、このノードには `cases.not_built` の欄が無いので、`rollup.py` の件数の検査（`mixed_not_built_problems`）は走らず、`pekko_baseline.py` も `green`＋`v2.0.0` を通す。誤って `green` にしても機械では止まらないので、#190 を閉じるときの台帳の更新では、上の表の最後の行の形を手で確かめる。

移植するテストの置き場所は、ノードが1件なので `crates/fraktor-actor/tests/byte_string_spec.rs` の1ファイルとし、群ごとにモジュールを足していく形とする。

## 7. 第2段への引き継ぎ

1. Claude Opus の担当が、群1の43件と PBT（§4.4）を `crates/fraktor-actor/tests/byte_string_spec.rs` へ red で書く（あわせて `ByteChunk` の `Arc::ptr_eq` や `ByteString::empty().grouped(0)` の `#[should_panic]` を `byte_string_value_test.rs` に置く）。muse はテストを書き換えず、§4 の設計どおりの実装だけを書く。単体テストには、§4.1 の `concat` の結合規則（`split_at` で分けて連結し直すと1チャンクに戻り、`is_compact()`・断片の数・`mark_supported` が原典と一致すること、隣り合わない場合と別の配列の場合は結合しないこと）と、`from_shared` / `from_shared_range` の範囲の張り付けと空の扱いも置く。群7の直列化7件は #190 の範囲外（後続の issue）であり、群1の第2段はその扱いに左右されない。
2. 実装の置き場所は `crates/fraktor-actor/src/kernel/byte_string/byte_string_value.rs`（必要に応じて `ByteChunk` 用の非公開子モジュール）、`byte_string_builder.rs`、および `ByteChunk` 対応のための `crates/fraktor-actor/src/kernel/byte_iterator/byte_iterator_value.rs` とする。新規ファイル `crates/fraktor-actor/tests/byte_string_spec.rs` には Pekko 由来の Apache-2.0 ライセンスヘッダを付け、`docs/design/issue-206-file-origin.tsv` にも登録する。
3. 必須検査は `.github/workflows/ci.yml` と同一の以下すべてを通す（Quint の仕様は無いため Quint の検査は不要、§4.5）：
   - `cargo fmt --check`
   - `cargo clippy --workspace --all-targets -- -D warnings`
   - `cargo test --workspace`
   - `cargo check -p fraktor-actor --no-default-features --target thumbv7em-none-eabihf`（kernel の no_std 維持）
   - `tools/syn-check`（`for dir in crates/*/; do cargo run --quiet --release --locked --manifest-path tools/syn-check/Cargo.toml -- --format json "${dir%/}"; done`）
   - `npm run check:aislop`（`node scripts/aislop-check-bootstrap.mjs check`）
   - 台帳・基準検査：`uv run --with pyyaml python3 ledger/tools/pekko_baseline.py check-ledger --ledger ledger/nodes.yaml`、`uv run python3 ledger/tools/pekko_baseline.py check-refs`、`uv run --with pyyaml python3 ledger/tools/relocate_nodes.py check`、`uv run --with pyyaml python3 ledger/tools/report_ledger_1to1.py check`、および `ledger/tools/*_test.py` などの Python テスト群
   - ライセンスヘッダ検査：`uv run python3 tools/license-headers/license_headers.py --check --manifest docs/design/issue-206-file-origin.tsv`
4. 群1が終わったら、台帳を §6 のとおりに更新する（`status: red`、`cases.total: 246`、`ported/passing: 43`、`verified_against` は付けず、`test_cases_approx` と `premise_flags` は触らない）。
5. 群7の26件を移したら #190 を閉じる。その前に、直列化7件を移す後続の issue（`actor-tests:PrimitivesSerializationSpec` の移植の前提）を起こし、台帳を §6 の表の最後の行（246/239/239、`excluded: []`、`red`、`verified_against` なし）にして、`triage.reason` に後続の issue の番号と決定を書く。`green` にはしない（§6）。

## 8. レビュー論点の決定と保留事項

独立レビュー（固定 HEAD `2fdf7072c8ac7970cc07b54edca9d6199ee9954a`、2026-11-27）の推奨を反映した各論点の結論である。

1. **範囲外と空の扱い（§8.1：条件付き承認を反映）。** `tail`・`init` は原典 `ByteString.scala:2269,2271` 自身が `drop(1)` / `dropRight(1)` であり空でも例外を投げず空を返すという事実誤認を訂正した。`get`・`head`・`last` は失敗理由が範囲外／空の1通りだけのため `Option<u8>` とし、「単一要素取得は `Option<u8>`、範囲／複数バイト読み取りは `Result<T, IndexOutOfBoundsError>`」という基準を §4.2 に明記した。
2. **`decode_string` の不正な UTF-8 の扱い（§8.2：承認）。** `String::from_utf8_lossy` による U+FFFD 置換（`-> String`）とする。JDK 8+ と同じ maximal subpart 置換則であり、群2 L1861・L1907 のマルチバイト途中切り出しテストとも整合する。
3. **`grouped(0)` の扱い（§8.3：承認）。** 契約違反として `assert!(size > 0, ...)` で panic する。空の `ByteString` に対する `grouped(0)` でも空判定より先に即 panic することを §4.2・§4.3 に明記し、単体テストの `#[should_panic]` で確かめる。
4. **Base64 の置き場所と失敗の形（§8.4：修正反映）。** 外部クレートを足さず kernel 内の `core`/`alloc` で実装する方針を維持したうえで、公開 API を原典に合わせて `encode_base64(&self) -> Self` と `decode_base64(&self) -> Option<Self>` に改め、RFC 4648 §4 標準アルファベット・パディング・JDK `Base64.getDecoder()` 互換の受理／拒否規則と、RFC 4648 §10 既知ベクタおよび独立参照エンコーダとの照合を §4.2・§4.3 に明記した。
5. **群7の Java 直列化（§8.5：元帥の決定（2026-11-27）を反映）。** 対象漏れ4件を補って L2461–2527 の全7件とし、`fraktor-ng-rs` に直列化の仕組みが無いことを確かめた。代替例が無いので `not_built` には入れない。元帥が 2026-11-27 に、7件を除外にせず、台帳の `cases` にも書かず、数から外さずに後続の issue（`actor-tests:PrimitivesSerializationSpec` の移植の前提）へ分けること、#190 は239件を移した時点で red（246/239/239）のまま閉じることを決めた。§5.1 と §6 をこの決定に合わせた。群1の第2段はこの扱いに左右されない。
6. **同一性検査 `eq` / `theSameInstanceAs` の扱い（§8.6：条件付き承認を反映）。** 空の実体に対する同一性（群3の507行・532行、群7の2830–2832行）は空の値等価＋`is_empty()`＋`is_compact()`＋非空断片数0へ適応し、非空の `eq` / `theSameInstanceAs`（群1の2231行・2254/2257行・2301行、群7の2534行・2796行・2807行）は弱めずバッキング配列のポインタ共有で検証する方針を §4.1・§5.2 に明記した。
7. **`asByteBuffers` の対応（§8.7：修正反映）。** `ByteBuffer` 型を作らない方針を維持しつつ単一 `slices()` 統合を取り下げ、内部表現を `ByteChunk` 列へ拡張したうえで、空で1要素 `&[]` を返す `as_byte_slices()`（`asByteBuffers` 相当）と、非空断片のみ返し空で0要素となる `as_non_empty_byte_slices()`（`asByteBuffersUnsafe` 相当、ゼロコピー共有検証対応）の2系統に分けた（§4.1・§4.2）。

## 9. 変更の記録（独立レビューへの回答）

### 9.1 独立レビュー（固定 HEAD `2fdf707`）への回答

固定 HEAD `2fdf7072c8ac7970cc07b54edca9d6199ee9954a` に対する独立レビュー（2026-11-27、判定：要修正。高7件・中3件・低4件、および §8 の推奨表）への対応記録である。全14件の指摘および §8 の推奨をすべて採り入れ、採らなかった指摘は無い。

| 指摘 | 対応 | 内容 |
|:---|:---|:---|
| 高 H-1（`Display` の原典仕様の簡略化） | 直した | `ByteString.scala:2203, 2683-2689` に合わせ、空の `"ByteString()"`、`b as i8` 符号付き10進、`", "` 区切り、`len > 100` での `... and [<len - 100>] more` 切り詰めを §4.2・§4.3 に明記した |
| 高 H-2（Base64 の型・仕様の簡略化と独立参照検証の欠落） | 直した | API を `encode_base64(&self) -> Self` と `decode_base64(&self) -> Option<Self>` に改め、RFC 4648 §4 標準アルファベット・JDK `Base64.getDecoder()` 互換の受理／拒否規則（パディング省略・末尾未使用ビット・空白／不正 `=` 拒否）と、RFC 4648 §10 既知ベクタおよび独立参照エンコーダとの照合を §4.2・§4.3・§8.4 に明記した |
| 高 H-3（群1の `copyToArray` が3引数を要求） | 直した | 群1の §4.2 に3引数版 `copy_to_array(&self, dest: &mut [u8], start: usize, len: usize) -> usize` と飽和クランプ式を置き、群2で足す4引数版 `copy_to_array_from` と明確に分けた |
| 高 H-4（`split_at`・`is_compact`・`asByteBuffers` 2系統・`ByteChunk` 内部表現・非空 `eq` の欠落） | 直した | §4.1 に `ByteChunk { bytes: Arc<[u8]>, start, len }` の内部表現拡張（`ByteIterator` / `ByteStringBuilder` の追随を含む）とポインタ共有検証を新設し、§4.2 に `split_at`・`is_compact`・`as_byte_slices`・`as_non_empty_byte_slices` を追加した |
| 高 H-5（`tail`・`init` の原典挙動の事実誤認） | 直した | `ByteString.scala:2269,2271` を照合し、`tail = drop(1)`・`init = dropRight(1)` が空でも例外を投げず空を返す原典どおりの挙動であることを §4.2・§8.1 で訂正した |
| 高 H-6（Java 直列化の4件漏れと `not_built` 規約違反） | 直した | `fraktor-ng-rs` に直列化の仕組みが無いことを調査確認したうえで、`not_built` を取り下げ、L2461–2527 の全7件をケースごとに整理した。当初は `cases.excluded` の候補としたが、再レビューの H-1 を受けて、元帥の決定（2026-11-27）どおり「除外にせず、どの区分にも数えずに後続の issue へ分ける」形へ §5.1・§6・§8.5 を改めた（§9.2） |
| 高 H-7（群1〜6 で `verified_against: v2.0.0` を付けると CI 即死） | 直した | 群1〜6 では `verified_against` を付けない手順へ §6 を書き直した。当初は群7の完了時に `green`＋`v2.0.0` を付けるとしたが、再レビューの H-1 を受けて、#190 の完了時も `red` で `verified_against` なしとし、`green`＋`v2.0.0` は後続の issue で直列化7件を移したときに付ける形へ改めた（§9.2） |
| 中 M-8（`test_cases_approx`・`premise_flags` の手動変更と群1〜6 の `status` 未規定） | 直した | `test_cases_approx`（121）と `needs.premise_flags` は手で変更しない方針へ §1・§6 を修正し、群1〜6 の `status` を `red` と明記、`verify_green.py` の `--crate fraktor-actor` 明示も注記した |
| 中 M-9（PBT 生成器の参照行誤記、非コンパクト単一スライス欠落、エッジケース不足） | 直した | 参照行を L43–80 へ訂正し、`ByteChunk`＋`.slice` による5形態生成・固定シードと失敗時シード出力・L2204 全8件と L2381–2397 の `usize` 境界写像を §4.4 に明記した |
| 中 M-10（群7テーブルの節ラベル誤記26件と古い基点 `f08cc4a`） | 直した | 群7テーブルの L2529–2901（26行）の節ラベルを `A ByteString` に修正し、最新の `origin/main`（`77575ba`）を取り込んで冒頭の基点と対応表の参照記述を更新した |
| 低 L-11（`index_where` / `index_of` 命名非対称と `Fn` vs `FnMut`） | 直した | `index_where(pred)` と `index_where_from(pred, from)` の対称な2メソッドにそろえ、述語トレイト境界をすべて `impl FnMut(u8) -> bool` へ改めた（§4.2） |
| 低 L-12（`usize` オーバーフロー境界と空の `grouped(0)` の記載漏れ） | 直した | `copy_to_array(_, usize::MAX, 1)`、`take_right(usize::MAX)`、`slice(usize::MAX, usize::MAX)`、`grouped(usize::MAX)`、および空に対する `grouped(0)` の即 panic を §4.3 の表へ追記した |
| 低 L-13（§7 の CI 必須検査コマンドの記載漏れ） | 直した | §7 に `no_std`（`thumbv7em`）、`tools/syn-check`、`npm run check:aislop`、`pekko_baseline.py`、`relocate_nodes.py`、`report_ledger_1to1.py`、および `license_headers.py`＋`issue-206-file-origin.tsv` 登録を追記した |
| 低 L-14（`for_each` / `fold` の加算あふれ注記） | 直した | 群1の2399行・2406行のテストで `u8` のまま足さず `u32` / `i32` / `i64` を使うことを §4.2・§4.3 に明記した |

### 9.2 設計の再レビュー（固定 HEAD `188fc0c`）への回答

固定 HEAD `188fc0c579d8ecfeba7631410c948b17e3d4b968` に対する再レビュー（2026-11-27、判定：要修正。高1件・中2件・低3件）への対応記録である。6件すべてを採り入れ、採らなかった指摘は無い。原典の振る舞いを弱める点が1つ残ったので、§5.4 に伺うこととして挙げた。あわせて最新の `origin/main`（`68a48a7`）を取り込み、冒頭の基点と §5.1 の調査の時点を改めた。

| 指摘 | 対応 | 内容 |
|:---|:---|:---|
| 高 H-1（群7の直列化7件を「`cases.excluded` 候補・オーナー承認待ち」とし、群7の完了を `green`＋`v2.0.0` としていた。2026-11-27 の元帥の決定と食い違う） | 直した | 元帥の決定どおり、7件は除外にせず、台帳の `cases` にも書かず、`cases.total` からも外さずに後続の issue（`actor-tests:PrimitivesSerializationSpec` の移植の前提）へ分ける形にした。#190 は239件で閉じる。冒頭（#190 の範囲と結論）、§2 の群7の行、§3 の群7の見出しと本文、§5.1（見出し・決定・表の列）、§6（`verified_against`・更新する欄・`status`・`triage.reason`・表の最後の行・後続の issue での `green`・道具の挙動と `green` にしてはいけない理由）、§7（項目1と新しい項目5）、§8.5、§9.1 の H-6・H-7 の行を直した。台帳の終わりの形は `ReceiveTimeoutSpec` の前例どおり `red`、246/239/239、`excluded: []`、`verified_against` なしで、`triage.reason` に後続の issue を書く。§5.1 には、`PrimitivesSerializationSpec.scala:164–214` で拾える3件（2462・2468・2497行、値の往復）と、JVM の `SerializationProxy` の内部の形を確かめる4件（2475・2486・2506・2515行）の対応を表にした |
| 中 M-1（`concat` が、同じ配列で隣り合う断片の結合を写していない） | 直した | §4.1 の `concat` に、原典 `ByteString.scala:667-669` の結合規則（両側が単一チャンクで `Arc::ptr_eq` かつ `l.start + l.len == r.start` なら1チャンクに結合）を入れた。結合が起きるのは原典では `ByteString1 ++ ByteString1` だけで、Rust の単一チャンクに規則を当てても同じ結果になることを書いた。`from_string("abcd").split_at(2)` を連結し直すと1チャンクに戻り、`is_compact() == true`、断片の数1、`mark_supported == true` になることを原典の行と照らして書き、§4.2・§4.3・§7 に単体テストを足した |
| 中 M-2（2294–2308行のゼロコピーの検証で、`base_ptr` の取り方と、コピーせずに包む生成が無い） | 直した（原典の振る舞いを弱めない形） | 原典 `fromArrayUnsafe` に当たる `from_shared(bytes: Arc<[u8]>)` と `from_shared_range(bytes, offset, length)` を群1で足すことにした（§4.1・§4.2）。`base_ptr` は呼び出し側が渡した `Arc<[u8]>` の先頭 `Arc::as_ptr(&array).cast::<u8>()` とし、2294–2308行の各検査の写し方を §4.1 に書いた。原典の「渡した配列を書き換えてはならない」約束は `Arc<[u8]>` の型で守られるので、`unsafe` は付けない。範囲は `from_array` と同じく配列の長さに張り付ける |
| 低 L-1（`asByteBuffer` と、`isReadOnly`・`!isReadOnly`・`hasArray`・`getByteBuffers` の部分検査の写し方が無い） | 直した | §4.2 の表の下に、2263–2290行の検査ごとの写し方を表にした。`asByteBuffer` は公開 API に足さず、`is_compact()` のとき `as_byte_slices().next()` が `to_vec()` と等しいことで写す。`isReadOnly` と `hasArray` は `&[u8]` の型で保証される。`getByteBuffers` は同じメソッドに写す。`!isReadOnly`（書き込めること）は安全な Rust では写せないので、§5.4 の伺うことに挙げた |
| 低 L-2（2158行・2171行の具象型の検査の写し方が無い） | 直した | §4.2 の表の下に、`ByteString2` を断片の数2、`ByteStrings` を断片の数3以上として確かめること、`Seq(2, 2)` を断片の長さで確かめること、`ByteString1C(...)` と `ByteString1(Array(0, 3, 4, 5), 1, 2)` を `from_shared` と `from_shared_range` で作ることを書いた |
| 低 L-3（2192行の `recombining` の `from`/`until` の生成範囲が無い） | 直した | §4.4 に、`from` と `until` をそれぞれ `0`、内部の添字、`len`、`len + 1`、`usize::MAX` から独立に生成し、`from > until` も外さないことを書いた |
