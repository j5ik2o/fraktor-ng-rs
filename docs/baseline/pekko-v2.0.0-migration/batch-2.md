# Pekko v2.0.0 への移行（第2まとまり）：UnsynchronizedByteArrayInputStreamSpec と ActorSystemScala213PlusSpec

第1まとまり（`batch-1.md`）に続く記録である（Orca 項目258）。照合の方法、原典の SHA、共通祖先は第1まとまりと同じである。

## UnsynchronizedByteArrayInputStreamSpec（G0issue147）

### 上流の変更

- テスト：`a5cf13f26c` #3541 で 3 ケースが 14 ケースになった。既存の「support skip with large value」にも、`available() == 0`、`skip(Long.MaxValue) == 0`、`available() == 0` のアサーションが増えた。
- 実装（`actor/src/main/java/org/apache/pekko/io/UnsynchronizedByteArrayInputStream.java`）：
  - `skip` は末尾で止まる。G0issue147 のゴール文書に記録した差異「残量を超える skip の後の位置」（原典は位置を `n` だけ進める）は、v2.0.0 で解消した。port は最初から末尾で止めている。
  - 負の `skip` は `IllegalArgumentException` を投げる。
  - `readAllBytes`、`readNBytes`、`skipNBytes`、`transferTo` を上書きした。`(data, offset, length)` のコンストラクタは範囲を切り詰め、負の値を拒む。

### 判断

2026-11-25、起動元から案 A の承認を受けた（`ByteStringInputStream` に足す。既存のシグネチャは保つ。符号付きのアダプターと共有の可変バッファは足さない）。

丸ごと表せない 2 ケースは、オーナー本人（j5ik2o）が除外として承認した。出所は元帥の項目283（e4453）の回答「2件とも除外として承認し、代わりのテストを足す」で、台帳の `cases.excluded` の `approved_by: j5ik2o`、`approved_at: 2026-11-25` はその転記である。経緯：最初は 2 件を作らない区分（`cases.not_built`）として記録したが、その判断は撤回され、いったん未判定に戻した（ノードは red、14/12/12）。そのうえで、オーナー本人の決定によって除外になった。作らない区分の記録と以前の承認の記録は、すべて外した。

### 移植した 12 ケース

| ケース | 赤（`ac213f2`） | 緑（`0e546f4`） |
|:---|:---|:---|
| support mark and reset | 既存 | pass |
| support skip | 既存 | pass |
| support skip with large value（アサーション増） | E0599（`available` がない） | pass |
| support skipNBytes | E0599 | pass |
| support read into array | E0599 | pass |
| support readAllBytes | E0599 | pass |
| support readNBytes(int) | E0599 | pass |
| support readNBytes(byte[], int, int) | E0599 | pass |
| support transferTo | E0599 | pass |
| support offset and length constructor | E0599 | pass |
| clamp offset and length to the array bounds | E0599 | pass |
| behave like java.io.ByteArrayInputStream under randomized operation sequences | E0432 と E0599 | pass |

- 赤：`cargo test -p fraktor-actor --test unsynchronized_byte_array_input_stream_spec` は終了値 101。E0432 が 1 件（`EndOfStreamError`、`IndexOutOfBoundsError` がない）、E0599 が 39 件。
- `9ed149b` は、テスト内の借用の誤り（E0502）を直した。名前の解決の誤りに隠れていたもので、テストの意味は変えていない。
- 緑：12 passed。

### 追加した公開面と根拠

既存の関数のシグネチャと挙動は変えていない。kernel は no_std のままで、`alloc` の `Vec` だけを使う。

| 公開面 | Java の対応 | 根拠（使うケース） |
|:---|:---|:---|
| `ByteStringInputStream::available` | `available()` | skip with large value、skipNBytes、readAllBytes、constructor、clamp、randomized |
| `skip_n_bytes(usize) -> Result<(), EndOfStreamError>` | `skipNBytes(n)` | skipNBytes、randomized（op 8） |
| `read_range(&mut [u8], usize, usize) -> Result<Option<usize>, IndexOutOfBoundsError>` | `read(b, off, len)` | read into array、randomized（op 2） |
| `read_all_bytes() -> Vec<u8>` | `readAllBytes()` | readAllBytes、constructor、clamp、randomized（op 3） |
| `read_n_bytes(usize) -> Vec<u8>` | `readNBytes(len)` | readNBytes(int)、constructor、randomized（op 4） |
| `read_n_bytes_into(&mut [u8], usize, usize) -> Result<usize, IndexOutOfBoundsError>` | `readNBytes(b, off, len)` | readNBytes(byte[], int, int)、randomized（op 5） |
| `transfer_to(&mut Vec<u8>) -> usize` | `transferTo(out)` | transferTo、randomized（op 9）。kernel には `std::io::Write` がないので、出力はバイトの Vec にした |
| `byte_string::EndOfStreamError` | `EOFException` | `skip_n_bytes` の失敗 |
| `byte_string::IndexOutOfBoundsError` | `IndexOutOfBoundsException` | `read_range`、`read_n_bytes_into` の範囲外 |

`(data, offset, length)` のコンストラクタは、既存の `ByteString::from_array(data, offset, length).as_input_stream()` で表した。範囲を切り詰める点は同じである。

### randomized のケース

- 原典は、同じ操作をこのストリームと `java.io.ByteArrayInputStream` の両方に当て、戻り値、例外、バッファの中身、`available()` を比べる。
- port は、テストの中に 2 つを持つ。
  - `JavaRandom`：`java.util.Random`（`scala.util.Random` が包むもの）の再現。種は原典と同じ 20260922。最初の 3 回の繰り返しの引き（`nextInt(48)`、`nextBytes`、`nextBoolean`、`nextInt(13)`、`nextInt(8)`、`nextInt(6)`）は、Temurin 26.0.2 の `java.util.Random` の出力と一致した。
  - `ByteArrayInputStreamModel`：JDK の `ByteArrayInputStream` の参照モデル。`skipNBytes` は `InputStream` の既定の実装に合わせた。
- 型の範囲の外になる負の値の分岐は、引きを取ったうえで操作だけを省く。後の引きはずれない。原典では、どの分岐も両方のストリームを動かさない（両方が投げるか、両方が何もしない）。そのため、省いても 2 つの状態は同じに保たれる。

| op | 省く条件 | 原典での結果 |
|:--|:---|:---|
| 2（`read(b, off, len)`） | `off == -1` または `len == -1` | 両方が `IndexOutOfBoundsException` |
| 4（`readNBytes(len)`） | `len == -1` | 両方が `IllegalArgumentException` |
| 5（`readNBytes(b, off, len)`） | `off == -1` または `len == -1` | 両方が `IndexOutOfBoundsException` |
| 7（負の `skip`） | `n > 0` | 参照は 0 を返し、原典のストリームは `IllegalArgumentException`（原典が記録した差） |
| 8（`skipNBytes(n)`） | `n == -1` | 両方が何もしない |

範囲の外でも負でない引数、長さ 0、ストリームの末尾での読み出し、mark と reset は、すべて残した。

### 除外した 2 ケース（オーナー本人の承認）

| ケース（v2.0.0） | 原典の位置と観測 | Rust で等価に書けない理由 | 代わりの確認（件数に含めない） |
|:---|:---|:---|:---|
| not copy the backing array | `actor-tests/src/test/scala/org/apache/pekko/io/UnsynchronizedByteArrayInputStreamSpec.scala` L165–171。配列からストリームを作った後に配列の先頭を書き換え、`read()` が新しい値を返す | port のストリームは `ByteString::from_array` が作った時点のコピーを読むので、外部の書き換えは見えない。安全な Rust では、借りている配列を書き換えられない。共有の可変バッファは公開 API に足さない | `crates/fraktor-actor/tests/byte_string_input_stream_no_copy_test.rs` の `building_a_stream_does_not_copy_the_bytes_of_the_byte_string`。公開 API だけを使い、`as_input_stream` がバイト列をコピーしないことを、テストのバイナリ専用の計数アロケータで確かめる。1 ファイル 1 テストで隔離し、上限は入力長の 1/16 とした（正確なバイト数は約束しない）。1 配列でも 2 配列の連結でも成り立つ |
| reject negative skip | 同 L66–70。`skip(-1)` が `IllegalArgumentException` を投げる | `skip` は `usize` を受けるので、負の値を渡すコードはコンパイルできない。実行時の拒否は書けない | `ByteStringInputStream::skip` の rustdoc の `compile_fail` 例（`skip(-1)`。`cargo test --doc` で実行）と、`byte_string_input_stream_test.rs` の `skip_at_its_boundaries_zero_exactly_the_rest_and_past_the_rest`（`skip(0)`、残りちょうど、残り超過、`usize::MAX`） |

- 実測（公開 API のみ、計数アロケータ）：1 MiB の `ByteString::from_array` は 1,048,608 バイトを確保する（範囲のコピー）。そのうえで `as_input_stream` は 32 バイトしか確保せず、配列を `Arc` で共有する。
- 負の対照：`as_input_stream` が配列をコピーする変異では、補助テストが失敗する（1 MiB に対して 2,097,200 バイトを確保）。`skip(-1)` は E0600 で失敗し、`skip(1)` ならコンパイルできることを手元で確かめた（`compile_fail` が型の理由で失敗していることの確認）。
- 台帳：ノードは green、`verified_against: v2.0.0`、`cases` は total 14、ported 12、passing 12。除外は上の 2 件で、緑の条件（ported + 承認済みの除外 = total）を満たす。

### 負の対照

| 変異 | 結果 |
|:---|:---|
| `read_n_bytes` が長さを無視する | readNBytes(int) と randomized が失敗 |
| `skip_n_bytes` が足りなくても `Ok` を返す | skipNBytes と randomized が失敗 |
| 範囲の検査を `<=` から `<` にずらす | read into array、readNBytes(byte[], int, int)、randomized が失敗 |
| `read_range` がオフセットを無視する | read into array、readNBytes(byte[], int, int)、randomized が失敗 |
| `available` が今の配列の残りだけを数える | spec は通る（原典は配列 1 つ）。unit テストの連結の 3 件が失敗 |

### 原典外の回帰（台帳の件数に含めない）

`byte_string_input_stream_test.rs` に 4 件を足した（`8be22e8`）。連結した `ByteString` で、`available`、まとめ読み、`skip_n_bytes` が配列の境目をまたぐこと、範囲外の指定でストリームが動かないことを確かめる。

## ActorSystemScala213PlusSpec（G0issue99）

- 項目256 の報告書は「v2.0.0 でファイルが削除された」としていたが、訂正する。共通祖先 `0ca804c7af` にこのファイルはない。上流の #2486（ActorSystem の `close`）は、1.x 系統では `3be76d1be7` で scala-2.13+ の新しいファイルとして入った。2.0 系統では `d5ee8a63e3` で `actor-tests/src/test/scala/org/apache/pekko/actor/ActorSystemSpec.scala` の中に入った。ケース「Scala's Using automatically terminates ActorSystem」の本体は、両方で同じである。
- Rust のテスト `crates/fraktor-actor/tests/actor_system_scala213_plus_spec.rs` は残した。冒頭の出典に移動先を書いた（`6dcfe2d`）。1 passed。
- 台帳：旧 id のノードは green、v1.7.1、1/1/1 のまま、履歴として残す。`--list-unverified` には「v2.0.0 で未照合」として出続ける。`ActorSystemSpec` のノード（unported、v2.0.0 で 20 ケース）は unported のままにする。移植済みの 1 件は、この記録にだけ書く。二重には数えない。退役と承継を機械でどう集計するかは、#166 が main に入った後に別の差分で決める。

## PR #168 のレビュー対応

範囲を絞った（2026-11-25、起動元の決定）。#168 は、Rust の 12 ケース、オーナー承認の除外 2 件、その代わりの確認、台帳、記録だけにした。作らない区分の混在の勘定と Scala の件数判定の改善（rollup.py、verify_red.py、excluded_approvals.py、scala_headings.py、facet）は、保全用の枝 `j5ik2o/pekko-2-ported-migration-2-tools`（`3093f84`）に移した。12 ケースの記録には要らず、まだ判断を待つからである。

| 指摘 | 対応 |
|:---|:---|
| CodeRabbit r4689121567：`available()` の残量の合計が 32 ビットで溢れる | 直す。`ByteIterator::remaining` を飽和加算にした（`usize` を超えれば `usize::MAX`）。テストは `9570f7f`（赤）、修正は `606610b` |
| CodeRabbit r4689121572：小さい `read_n_bytes` で全部の配列を走る | 直す。`remaining_up_to(limit)` で要求の長さに届いたら数えるのを止める。0 バイトなら走らない。同じコミット |
| CodeRabbit r4689121577：「not copy the backing array」を除外に | 除外にした。オーナー本人の承認による（上の「判断」） |
| CodeRabbit r4689121595、r4689121586、独立レビュー（Scala の件数判定、除外の検査） | この PR の範囲外。保全用の枝に移した変更が対象なので、そちらで扱う |
