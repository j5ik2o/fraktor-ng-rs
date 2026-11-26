# 設計：#190 util の ByteStringSpec を移植する（分け方と第1群の設計）

[Issue #190](https://github.com/j5ik2o/fraktor-ng-rs/issues/190) の設計の記録である。対象は Pekko v2.0.0（`9fbd2283b1cf701c90a1223bf621942c191db61f`）の `actor-tests/src/test/scala/org/apache/pekko/util/ByteStringSpec.scala`（3933行）である。実行器（#234）には依存しない。この文書は第0段（ケースの分け方）と第1段（最初の群の設計）であり、実装は設計レビューが通ってから第2段で行う。第2段では、テスト（移植テストと PBT）は Claude Opus の担当が先に red で書き、muse はそのテストを通す実装だけを書く。

結論を先に書く。原典のケースは、数え直すと **246件**であり、依頼の目安249件とは3件違う（§1）。246件を、原典の節（`must` のまとまりと `when` のまとまり）に沿って **7群** に分ける。各群は32〜43件であり、依存の順は 群1（`ByteString` 本体の基本操作）→ 群2（範囲操作と等価性）→ 群3（内部表現ごとの検索）→ 群4（`ByteStringIterator`）→ 群5（`ByteStringBuilder` の基礎と符号化の基本と整数の性質群）→ 群6（`ByteStringBuilder` の浮動小数点と `Short` の性質群）→ 群7（直列化と数値の読み取りと表現の横断）である（§2）。台帳のノードは1件だけなので、群ごとに `ported` と `passing` を足していき、最後の群7で初めて `green` にする（§6）。

最初の群である群1は、ほかの群に依存しない。既存の `ByteString`（`crates/fraktor-actor/src/kernel/byte_string/byte_string_value.rs`）に、長さと要素の取得と切り出し（`take`、`drop`、`slice` など）と、文字列と Base64 の変換と、配列の並びとしての見せ方を足す設計を §4 に書く。足す本体はすべて `core` と `alloc` だけで書けるため、kernel の no_std 性質は変わらない。

数から外す提案は2件だけである。群7の Java 直列化の3件（原典2475行、2486行、2497行）は、JVM の直列化の仕組みそのものを確かめるケースであり、Rust で同じ振る舞いを作れないため、作らない区分（`not_built`）として提案する。群3の `CompactByteString.apply` の同一性（`theSameInstanceAs`）を見る2件は、値の等価性への適応を提案する。どちらも決めるのはレビューであり、この文書は提案として書く（§5）。それ以外の JVM と Scala の癖（`ByteBuffer`、Base64、負の添字、見つからない場合の `-1` など）は、対応表（`docs/decisions/scala-java-to-rust-correspondence.md`。ただしこの文書を書いた時点の main にはまだ入っておらず、他の作業ブランチにある版を引いた）と `docs/goals/issue-130-byte-reading.md` の前例に従い、Rust で同じ振る舞いを作れるものは伺わずに写す。

作業の基点は `origin/main` の `f08cc4a`（プルリクエスト #245 のマージ）である。この文書を書いた作業フォルダのローカルの `main` 参照は古かったため、基点は `origin/main` で確かめた。

## 1. 件数の数え直し

原典の `"ケース名" in {` の数を数えた。ブロックコメントの中のケースは無く（ファイルのブロックコメントはライセンスの頭書きだけである）、`ignore` と `pending` も無い（`pending` の文字列は2171行の `prepending` の一部である）。したがって、数えた246件がすべて実行されるケースである。

| 項目 | 件数 |
|:---|---:|
| 依頼の目安 | 249 |
| 数え直した実数（`"..." in {`） | **246** |
| 差 | -3 |
| 台帳 `ledger/nodes.yaml` の `test_cases_approx`（現行） | 121 |

台帳の `test_cases_approx` の121件は、Pekko v1.7.1 の台帳を引き継いだ概数であり、v2.0.0 の実数ではない。第2段の最初の群で、台帳の `cases.total` を246に直し、`test_cases_approx` も246へそろえる（§6）。

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
| 1 | 群1：`ByteString` 本体の基本操作 | 43 | `A ByteString` の前半（2140–2459行）。大きさ、連続性、連結の向き、圧縮と再結合、文字列と Base64、Vector としての振る舞い | 無し | `ByteString` に長さ、要素の取得、切り出し、走査、変換を足す（§4） |
| 2 | 群2：範囲操作と等価性と `byteAtUnchecked` | 32 | `ByteStrings` の切り出し系6件と、範囲つき `copyToArray`、範囲つき `decodeString`、等価性、`byteAtUnchecked` の節 | 群1 | 開始位置つきの写し、範囲の復号、`Hash`、内部向けの要素の取得を足す |
| 3 | 群3：内部表現ごとの検索 | 32 | `ByteString1`、`ByteString1C`、`CompactByteString.apply`、`ByteStrings` の検索系19件 | 群1、群2 | `ByteString` に検索（`index_of` 系、部分列の検索、前方一致と後方一致）を足す。表現ごとの型は作らず、既存の1型で担う |
| 4 | 群4：`ByteStringIterator` | 37 | `A ByteStringIterator` の全部 | 群1（`ByteString` の切り出しと長さを使う） | `ByteIterator` に切り出し、走査、`getBytes` 系、浮動小数点の読み取り、バッファへの写しを足す |
| 5 | 群5：`ByteStringBuilder` の基礎と符号化の基本と整数の性質群 | 37 | `A ByteStringBuilder` の3422–3664行。追加の基本、`putInt`、`putLong`（`LongPart` を含む） | 群1（`result()` が返す `ByteString` の基本操作で確かめる） | `ByteStringBuilder` に1バイトの追加、`putByte` と `putBytes`、整数と浮動小数点と `Short` の書き込みの基本形、空かどうかの判定を足す |
| 6 | 群6：`ByteStringBuilder` の浮動小数点と `Short` の性質群 | 32 | `A ByteStringBuilder` の3676–3913行。`putFloat`、`putDouble`、`putShort` | 群5 | 新しい公開 API は足さない。群5で足した書き込みの境界値と往復を確かめる |
| 7 | 群7：直列化と数値の読み取りと表現の横断 | 33 | `A ByteString` の後半（2461–3027行）。直列化、unsafe な共有、数値の読み取り、境界、表現をまたぐ写像と走査、ビルダーの `result` の受け渡し | 群1、群4（イテレータの数値の読み取り）、群5（ビルダーの `result`） | `ByteString` に数値の読み取り（`readShort` 系）を足す。直列化の3件は作らない区分の提案（§5） |

依存の順の根拠は次のとおりである。群2の範囲つきの写しと復号は、群1の `slice` と `copy_to_array` と等価性の上で定義できる。群3の検索は、群1の切り出しと群2の等価性を使って確かめる。群4のイテレータのケースは、比較の相手として群1の `ByteString` の切り出しを使う。群5と群6のビルダーのケースは、`result()` が返す `ByteString` を群1の操作で確かめる。群7の数値の読み取りは群4のイテレータの読み取りと同じ意味論であり、ビルダーの `result` の受け渡しのケースは群5のビルダーの内部に触れるため、群7を最後に置く。

## 3. 群ごとのケースの一覧と、足す本体

### 群1：`ByteString` 本体の基本操作（43件、原典2140–2459行）

既存の本体は `ByteString` の `empty`、`from_string`、`from_array`、`from_ints`、`concat`、`iterator`、`as_input_stream` と、バイト列としての等価性（`PartialEq` と `Eq`）である。足す本体の設計は §4 にまとめた。ここでは、長さと要素の取得、切り出し（`take`、`drop`、`take_right`、`drop_right`、`slice`）、走査（`span`、`take_while`、`drop_while`、`index_where`）、写し（`to_vec`、`copy_to_array`）、分割（`grouped`）、圧縮（`compact`）、文字列と Base64 の変換、配列の並びの見せ方（`slices`）を `ByteString` に足す。

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

既存の本体では、等価性はすでにあるが、`Hash` は無い。範囲の扱いについては、`ByteStringInputStream::read_range` が、範囲が宛先をはみ出す場合に `IndexOutOfBoundsError` を返す形をすでに持っている。足す本体は、開始位置を指定する `copy_to_array`（原典の `copyToArray(xs, sourceOffset, destPos, length)` の形）、範囲を指定する `decode_string`（範囲を切り出してから復号するのと同じ結果を返す形）、内容に基づく `Hash` の実装、および `byteAtUnchecked` に対応するクレート内部の要素の取得である。`byteAtUnchecked` は Pekko の private な最適化であり、その覚え書き（直前に解決した断片）の共有は Rust では作らない。観測できる性質（前後どちらの向きに読んでも、並行に読んでも、正しいバイトを返すこと）は、安全な要素の取得でそのまま確かめる。範囲の切り出しと等価性は群1の `slice` と `Eq` の上に乗り、切り出し系の6件（`drop`、`dropRight`、`slice`、`take`、`copyToArray`、`return same hashCode`）は `ByteStrings` の節からこの群へ置いた。

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

既存の本体では、検索は `ByteIterator` の `index_of` と `index_of_from` だけがあり、`ByteString` 自体は検索を持たない。足す本体は、`ByteString` の `index_of`、`last_index_of`、`contains`、`index_of_slice`、`last_index_of_slice`、`contains_slice`、`starts_with`、`ends_with` であり、どれも複数の配列をまたいで探す。原典の `indexOf (specialized)` の節が確かめる SWAR の最適化は実装の内部であり、観測できる性質（通常の実装と同じ位置を返すこと）だけを写す。`ByteString1`、`ByteString1C`、`ByteStrings` の表現ごとの型は作らず、#130 の前例どおり既存の `ByteString` の1型で担い、テストは `from_array` のずらしと `concat` で各表現に当たる値を作る。`CompactByteString.apply(IterableOnce[Byte])` は、イテレータを1回だけたどる構築（群1で足す `from_iterator`）に対応させる。同一性（`theSameInstanceAs`）の扱いは §5 の提案2に書いた。

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

既存の本体は `ByteStringBuilder`（`crates/fraktor-actor/src/kernel/byte_string/byte_string_builder.rs`）であり、`add_all`、`append`、`result` だけを持つ。足す本体は、1バイトの追加、`put_byte` と `put_bytes`、符号化（`put_short`、`put_int`、`put_long`、`put_long_part`、`put_float`、`put_double`。どれも `ByteOrder` を取る形とし、既存の `ByteOrder` を使う）、空かどうかの判定（`is_empty` と長さ）である。`OutputStream` のラッパーのケースは、ビルダーへバイト列を書く小さなアダプタで確かめる。`putInt` と `putLong` の性質群が確かめる `ByteBuffer` との往復は、JDK の型を作らず、値の `to_be_bytes` と `to_le_bytes` が作る参照バイト列との一致として写す（§5 の注記）。

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

### 群7：直列化と数値の読み取りと表現の横断（33件、原典2461–3027行）

既存の本体では、数値の読み取りは `ByteIterator` だけが持ち、`ByteString` は直接の読み取りを持たない。足す本体は、`ByteString` の `read_short`、`read_int`、`read_long`（どちらもバイト順を取り、データが足りない場合は既存の `IndexOutOfBoundsError` を返す形）であり、意味論は群4のイテレータの読み取りとそろえる。そのほかに、表現をまたぐ `map`、ビルダーの `size_hint`、範囲外の `apply` の扱い（群1の `get` が `None` を返す形への適応）もこの群で扱う。直列化の3件（2475行、2486行、2497行）は作らない区分の提案であり、§5 の提案1に書いた。ビルダーの `result` の受け渡しの5件（2835–2891行）は、ビルダーが一時バッファをどう手放すかという内部の性質を確かめるため、群5のビルダーの実装が固まってから扱う。

| 原典の行 | 原典の節（must／when） | ケース |
|---:|:---|:---|
| 2462 | A ByteString／serialize correctly | given all types of ByteString |
| 2468 | A ByteString／serialize correctly | with a large concatenated bytestring |
| 2475 | A ByteString／serialize correctly | ByteString1C serializes via SerializationProxy not directly |
| 2486 | A ByteString／serialize correctly | CompactByteString instances serialize via SerializationProxy |
| 2497 | A ByteString／serialize correctly | each ByteString concrete type round-trips via serialization |
| 2506 | A ByteString／serialize correctly | given ByteString2 |
| 2515 | A ByteString／serialize correctly | given ByteString2 with sliced fragments |
| 2529 | A ByteString／serialize correctly | unsafely wrap and unwrap bytes |
| 2541 | A ByteString／serialize correctly | read short values |
| 2563 | A ByteString／serialize correctly | read int values |
| 2587 | A ByteString／serialize correctly | read long values |
| 2611 | A ByteString／serialize correctly | read primitive values across ByteString2 fragment boundaries |
| 2623 | A ByteString／serialize correctly | throw IndexOutOfBoundsException for readShortBE/LE with insufficient data |
| 2643 | A ByteString／serialize correctly | throw IndexOutOfBoundsException for readIntBE/LE with insufficient data |
| 2663 | A ByteString／serialize correctly | throw IndexOutOfBoundsException for readLongBE/LE with insufficient data |
| 2683 | A ByteString／serialize correctly | have correct takeRight behaviour for boundary values |
| 2709 | A ByteString／serialize correctly | throw IndexOutOfBoundsException when apply is called with an out-of-bounds index |
| 2722 | A ByteString／serialize correctly | return 0 from copyToArray when start >= destination length |
| 2737 | A ByteString／serialize correctly | correctly handle sizeHint with value smaller than already committed bytes |
| 2747 | A ByteString／serialize correctly | correctly handle grouped with size larger than length |
| 2756 | A ByteString／serialize correctly | map bytes correctly on each concrete type |
| 2773 | A ByteString／serialize correctly | foreach visits each byte in order on each concrete type |
| 2793 | A ByteString／serialize correctly | slice returns correct result on ByteString1 with all boundary combinations |
| 2811 | A ByteString／serialize correctly | indexOfSlice handles non-Byte typed Seq safely |
| 2821 | A ByteString／serialize correctly | lastIndexOfSlice handles non-Byte typed Seq safely |
| 2829 | A ByteString／serialize correctly | ByteString1.apply factory returns canonical empty for non-positive length |
| 2835 | A ByteString／serialize correctly | ByteStringBuilder.result hands over a mostly used temp buffer without copying |
| 2846 | A ByteString／serialize correctly | ByteStringBuilder.result hands over an exactly filled temp buffer without copying |
| 2856 | A ByteString／serialize correctly | ByteStringBuilder.result copies when only a small part of the temp buffer is used |
| 2868 | A ByteString／serialize correctly | ByteStringBuilder.result is not affected by later writes to the builder |
| 2881 | A ByteString／serialize correctly | ByteStringBuilder.result returns the written bytes for any fill level of the temp buffer |
| 2891 | A ByteString／serialize correctly | ByteStringBuilder.sizeHint does not shrink existing capacity |
| 2901 | A ByteString／serialize correctly | ByteString2 must behave exactly like ByteString1C across every read/transform op |

## 4. 群1の設計（`ByteString` 本体の基本操作、43件）

### 4.1 対応表

足す本体はすべて `crates/fraktor-actor/src/kernel/byte_string/byte_string_value.rs` の `ByteString` に置き、`core` と `alloc` だけで書く。新しい型と新しいモジュールは作らない。既存の関数と同じ働きの関数が無いことは、追加の前に同ファイルと `byte_iterator_value.rs` と `byte_string_input_stream.rs` で確かめた（`len`、`get`、`slice`、`take`、`drop` に当たる公開関数は無い）。

| 原典（Pekko の `ByteString`） | Rust の対応（この群で足す形） | 備考 |
|:---|:---|:---|
| `size`、`length` | `len() -> usize` | 合計は `saturating_add` で数え、32ビットの対象でもあふれない（`ByteIterator::remaining` の前例） |
| `isEmpty`、`nonEmpty` | `is_empty() -> bool` | 既存の不変条件（空の配列を持たない）により、配列の列が空であることと等しい |
| `apply(i)` | `get(index: usize) -> Option<u8>` | 範囲外は例外ではなく `None` である（#130 の「見つからないは `None`」と同じ方針）。群7の範囲外のケースは、この形への適応として扱う。レビューで決めてほしい点1に挙げた |
| `head`、`last` | `head() -> Option<u8>`、`last() -> Option<u8>` | 空の場合は `None` である。原典は空で例外を投げるが、この群のケースは空でない値にだけ使う |
| `tail`、`init` | `tail() -> Self`、`init() -> Self` | 空の場合は空を返す。原典は空で例外を投げる点が違うため、レビューで決めてほしい点1に挙げた |
| `take(n)`、`drop(n)`、`takeRight(n)`、`dropRight(n)` | `take(n: usize) -> Self`、`drop(n: usize) -> Self`、`take_right(n: usize) -> Self`、`drop_right(n: usize) -> Self` | 負の数は型で表せない（#130 の前例）。`n` が長さ以上でも張り付いて例外を投げない点は Scala と同じであり、`usize::MAX` でもあふれないように `min` で抑える |
| `slice(from, until)` | `slice(from: usize, until: usize) -> Self` | `from > until` の場合は空になり、範囲は長さで抑える（Scala の意味論どおり） |
| `span(p)`、`takeWhile(p)`、`dropWhile(p)` | `span(pred) -> (Self, Self)`、`take_while(pred) -> Self`、`drop_while(pred) -> Self`。`pred: impl Fn(u8) -> bool` | 述語はバイトを受け取るクロージャで写す（対応表の前例） |
| `indexWhere(p, from)`、`indexOf(elem)`、`indexOf(elem, from)` | `index_where(pred, from: usize) -> Option<usize>`、`index_of(elem: u8) -> Option<usize>`、`index_of_from(elem: u8, from: usize) -> Option<usize>` | 見つからない場合は `None` である。この群では基本形だけを足し、`last_index_of` と部分列の検索は群3で足す |
| `foreach(f)`、`foldLeft(z)(op)` | 既存の `iterator()` が返す `ByteIterator`（`Iterator<Item = u8>`）の `for_each` と `fold` で写す | 新しい API は足さない。ケースは `iterator()` 経由で確かめる |
| `toArray` | `to_vec() -> Vec<u8>` | 全バイトを平らにした写しを返す |
| `copyToArray(xs, start)` | `copy_to_array(dest: &mut [u8], start: usize) -> usize` | 写した件数を返す。`start` が宛先の長さ以上なら0を返して何も書かない（群7の2722行のケースと意味論をそろえる）。範囲指定のある形は群2で足す |
| `grouped(size)` | `grouped(size: usize) -> impl Iterator<Item = ByteString>` | `size` が0の場合は契約違反として panic する提案である。レビューで決めてほしい点3に挙げた |
| `compact` | `compact() -> Self` | 内容を変えず、配列を最大で1つにまとめる。すでに1つ以下なら共有したまま返す |
| `++`（連結）、前に足す、後ろに足す | 既存の `concat(&self, &ByteString) -> Self` | 新しい API は足さない。連結の向きと、2つの断片から複合表現へ進むケースは `concat` で確かめる |
| `fromString(s)`、`decodeString`（UTF-8） | 既存の `from_string(&str) -> Self` と、足す `decode_string() -> String` | この Spec の復号はすべて UTF-8 である（原典で文字コードを指定する箇所は無い）。`decode_string` は、JDK の `String` の復号と同じく、不正な並びを置換文字へ置き換える形（`String::from_utf8_lossy`）を提案する。レビューで決めてほしい点2に挙げた |
| Base64 の生成と復号（2235行） | `to_base64() -> String`、`from_base64(s: &str) -> Option<Self>` | 外部の crate は足さず、kernel の中で表引きの小さな実装にする。不正な入力は `None` である。レビューで決めてほしい点4に挙げた |
| イテレータからの生成（2243行） | `from_iterator(iter: impl IntoIterator<Item = u8>) -> Self` | イテレータを1回だけたどる。群3の `CompactByteString.apply` も同じ形を使う |
| `asByteBuffers`、`asByteBuffersUnsafe`（2261–2309行） | `slices() -> impl Iterator<Item = &[u8]>` | `ByteBuffer` の型は作らない。各配列をスライスとして順に返すことで、内容と順序と件数を確かめる。unsafe 版の「写さずに包む」性質は、スライスの先頭ポインタが内部の配列と一致することで確かめる。`ByteBuffer` の位置と残量の意味論そのものは写さない |
| `toString` が `ByteString(` で始まること（2310行） | `Display` の実装 | 表示は `ByteString(` に10進のバイトの並びを続け、`)` で閉じる形にする。この群のケースが確かめるのは接頭辞だけである |

### 4.2 守る性質と、壊しうる入力

守る性質は次の5つである。

1. **平らなバイト列としての意味論。** 群1のすべての操作は、同じ内容を持つ `Vec<u8>` への同じ操作と同じ結果を返す。原典の `likeVector` の形（`ByteString` と `Vector` に同じ操作をして比べる）を、そのまま移植の物差しにする。
2. **不変条件の維持。** `ByteString` は空の配列を持たない（既存の不変条件）。切り出しの結果が空になる場合は、空の配列を入れずに `empty()` を返す。
3. **不変性と共有。** 操作は元の値を変えず、新しい値は元の配列を `Arc` で共有する。連結と切り出しは、バイトを写さずに済む範囲で写さない。
4. **張り付き（例外を投げない切り出し）。** `take`、`drop`、`take_right`、`drop_right`、`slice` は、どんな `usize` の入力でも panic せず、Scala と同じ張り付き方をする。
5. **往復。** `from_string(s).decode_string()` は `s` に等しく、`from_base64(bs.to_base64())` は `bs` に等しい。

壊しうる入力と、その扱いは次のとおりである。

| 入力 | 扱い |
|:---|:---|
| 空の `ByteString` への `head`、`last`、`tail`、`init` | `head` と `last` は `None` を返し、`tail` と `init` は空を返す（§4.1 の判断材料） |
| `len()` と等しい添字、長さ以上の `take` と `drop` の件数、`usize::MAX` の件数 | `get` は `None` を返し、切り出しは張り付く。加算は `min` と `saturating_add` で抑え、あふれで panic しない |
| `from > until` の `slice` | 空を返す |
| `grouped(0)` | 契約違反として panic する提案（判断材料3） |
| 不正な Base64 文字列 | `from_base64` は `None` を返す |
| 不正な UTF-8 のバイト列の `decode_string` | 置換文字へ置き換える（判断材料2） |
| 大きな連結（配列の数が多い値）への `len` と `to_vec` | `len` は飽和して数え、`to_vec` は内容のとおりに作る。テストの規模では問題にならない |

### 4.3 PBT の要否

**PBT を足す。** 群1の原典のケースは、大半が ScalaCheck の `check` で書かれた性質そのものである（「have correct size」の6件、「be sequential」の2件、「behave like a Vector」の21件など）。移植では、外部の依存を足さず、既存の前例（`frequency_sketch_property_test.rs` と同じ固定 seed の SplitMix64）で入力を作る PBT として書く。入力の作り方は原典の `Arbitrary[ByteString]`（原典47–110行）を写し、1つの配列から作る値、`from_array` でずらした値、`concat` でつないだ値の3つの作り方を混ぜる。試行の回数は、原典の設定（`PropertyCheckConfiguration` の `minSuccessful = 1000`、原典40–41行）に合わせて性質ごとに1000回とする。台帳の件数には、PBT の性質をケースの件数として数えず、原典のケース43件への対応で数える（#185 の扱いと同じ）。

### 4.4 Quint の要否

**Quint は書かない。** 群1の対象は不変の値に対する純粋な関数であり、状態の遷移も、複数の主体のやり取りも、時間で変わる前提も無い。守る性質は §4.2 の代数的な性質であり、PBT が直接に確かめる。形式仕様を書いても、PBT の性質の言い換えにしかならず、確かめる力が増えない。状態を持つ対象（イテレータの消費とビルダーの蓄積）は群4から群6に入るが、それらも単一の対象の直線的な変化であり、Quint を書くかどうかは各群の設計で改めて判断する。

## 5. 数から外す提案と、適応の提案

決めるのはレビューである。この節の提案を認めるかどうかで、台帳の区分が変わる。

### 提案1：群7の Java 直列化の3件を作らない区分にする

対象は、2475行「ByteString1C serializes via SerializationProxy not directly」、2486行「CompactByteString instances serialize via SerializationProxy」、2497行「each ByteString concrete type round-trips via serialization」の3件である。3件とも、JDK の `ObjectOutputStream` による直列化と、`SerializationProxy` への置き換え（`writeReplace`）という JVM の仕組みそのものを確かめる。Rust には Java の直列化が無く、fraktor-actor は `ByteString` の独自の直列化の形式を持たない。同じ振る舞いを作るには Java の直列化の形式そのものを実装することになり、それは Pekko の移植ではなく JDK の再実装である。したがって3件を、作らない区分（`not_built`）として提案する。代替例は無い。将来、remote の直列化を扱う Spec を移すときに、fraktor 自身の形式を決めて往復を確かめることが、この3件の代わりになる。

### 提案2：群3の同一性（`theSameInstanceAs`）を、値の等価性への適応にする

対象は、507行「return empty for an empty collection」と532行「traverse an empty single-use IterableOnce only once」の2件の中で、空の結果が `CompactByteString.empty` と同一の実体であることを見る部分である。JVM のオブジェクトの同一性は、Rust の値には対応する観測手段が無い（`ByteString::empty()` は、そのたびに空の値を新しく作る）。提案は、同一性の部分を値の等価性（`empty()` と等しいこと）へ適応し、イテレータを1回だけたどることの検査はそのまま残す形である。ケースの件数は外さず、2件とも移植として数える。

### 注記：提案にしないもの

次のものは、Rust で同じ振る舞いを作れるため、提案にせずに写す。群5と群6の `ByteBuffer` との往復は、値の `to_be_bytes` と `to_le_bytes` が作る参照バイト列で写す。群2の `byteAtUnchecked` の並行読みのケースは、安全な要素の取得を複数のスレッドから読む形で写す（統合テストは std を使える。本体の no_std 性質には触れない）。負の添字と負の件数は、型で表せないため、原典の負の入力のケースは `usize` の境界（0と長さと `usize::MAX`）で写す（#130 の前例）。

## 6. 台帳の見込み

台帳のノードは `actor-tests/src/test/scala/org/apache/pekko/util/ByteStringSpec.scala` の1件だけであり、現行は `status: unported`、`test_cases_approx: 121`、`cases.total/ported/passing: null`、`premise_flags: ['java-serialization']`、`goal: null` である。第2段では、群ごとに次のように進める。

| 段 | `cases.total` | `cases.ported` | `cases.passing` | `status` |
|:---|---:|---:|---:|:---|
| 群1の完了 | 246 | 43 | 43 | green にはしない（残りの群が未了） |
| 群2の完了 | 246 | 75 | 75 | green にはしない |
| 群3の完了 | 246 | 107 | 107 | green にはしない |
| 群4の完了 | 246 | 144 | 144 | green にはしない |
| 群5の完了 | 246 | 181 | 181 | green にはしない |
| 群6の完了 | 246 | 213 | 213 | green にはしない |
| 群7の完了 | 246 | 243（提案1が認められれば、残り3件は `not_built`） | 243 | **green** |

最初の群で `test_cases_approx` を246へ直し、`verified_against: v2.0.0` と `goal: G0issue190` を付ける。`premise_flags` の `java-serialization` は、提案1が決まるまでは残し、認められれば群7で外す。移植するテストの置き場所は、ノードが1件なので `crates/fraktor-actor/tests/byte_string_spec.rs` の1ファイルとし、群ごとにモジュールを足していく形を提案する。

## 7. 第2段への引き継ぎ

1. Claude Opus の担当が、群1の43件を `crates/fraktor-actor/tests/byte_string_spec.rs`（と、必要な PBT）へ red で書く。muse はテストを書き換えず、§4 の対応表の実装だけを書く。
2. 実装の置き場所は `crates/fraktor-actor/src/kernel/byte_string/byte_string_value.rs` であり、単体テストは同ファイルの `byte_string_value_test.rs` に足す。
3. 検査は CI と同じく、`cargo fmt --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`、および台帳の Python テストである。Quint の仕様は無いので、Quint の検査は無い（§4.4）。
4. 群1が終わったら、台帳を §6 の見込みどおりに更新し、次の群の設計をこの文書へ足すか、群ごとの文書に分けるかをレビューで決める。

## 8. レビューで決めてほしい点

1. **範囲外と空の扱い。** `get` が `Option` を返す形、`head` と `last` が `None` を返す形、`tail` と `init` が空では空を返す形でよいか。原典は例外を投げるため、3つとも原典と違う観測になる。却下する場合は、既存の `IndexOutOfBoundsError` を返す `Result` の形が対案になる。
2. **`decode_string` の不正な UTF-8 の扱い。** JDK と同じ置換（`from_utf8_lossy`）でよいか。却下する場合は、`Result<String, FromUtf8Error>` を返す厳密な形が対案になる。
3. **`grouped(0)` の扱い。** 契約違反として panic する形でよいか。
4. **Base64 の置き場所と失敗の形。** 外部の crate を足さず kernel の中で実装する形と、失敗を `Option` で返す形でよいか。
5. **提案1（直列化の3件を作らない区分にすること）を認めるか。** 認めれば、台帳の最終形は移植243件と作らない区分3件で green になる。認めない場合は、Java の直列化の形式の実装が範囲に入り、規模が大きく変わる。
6. **提案2（同一性を等価性へ適応すること）を認めるか。**
7. **`asByteBuffers` の対応。** `ByteBuffer` の型を作らず、`slices()` で内容と順序と共有を確かめる形でよいか。
