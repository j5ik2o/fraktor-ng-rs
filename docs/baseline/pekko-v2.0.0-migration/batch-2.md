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

2026-11-25、起動元から案 A の承認を受けた（`ByteStringInputStream` に足す。既存のシグネチャは保つ。符号付きのアダプターと共有の可変バッファは足さない）。丸ごと表せない 2 ケースを作らない区分にすることも、起動元経由で承認された（Orca 質問 `msg_3d3a2ef9adf0`）。台帳には起動元から受けた文言を写した。approved_by は「副元帥（オーナーが認めた作らない区分のケース単位適用、Orca msg_3d3a2ef9adf0）」、approved_at は 2026-11-25 である。オーナー本人がこの 2 件を個別に承認した記録ではない。

### 移植した 12 ケース

| ケース | 赤（`42753e8`） | 緑（`915652b`） |
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
- `35d3b05` は、テスト内の借用の誤り（E0502）を直した。名前の解決の誤りに隠れていたもので、テストの意味は変えていない。
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

### 作らない区分にする 2 ケース

| ケース（v2.0.0） | 原典の位置 | 理由 | 代わりの確認 |
|:---|:---|:---|:---|
| reject negative skip | `UnsynchronizedByteArrayInputStreamSpec.scala` L66–70（`a5cf13f26c`）、実装 `skip` の `n < 0` の検査 | `skip` は `usize` を受けるので、負の値を表せない。型が拒否を担う | `support_skip_with_large_value` が `skip(50)` の残量で頭打ちになることと、末尾での `skip(usize::MAX) == 0` を確かめる。`support_skip` は `skip(1)`。randomized の op 7 は `skip(0) == 0` を確かめる |
| not copy the backing array | 同 L165–171、実装のコンストラクタが配列を `this.data` に持つこと | 原典は、ストリームを作った後で配列を書き換え、新しい値を読む。安全な Rust では、ストリームが借りている間は配列を書き換えられない。port の stream は `ByteString` の配列を `Arc` で持ち、`from_array` はコピーする | port の公開契約に「コピーしない」はない。`ByteString::from_array` の rustdoc は、範囲のバイトのコピーから作ると定めている（Pekko の `ByteString.fromArray` も同じくコピーする）。このため、同じスライスを読む代わりの検証は足さない |

台帳には `cases.not_built` の 2 件として記録した（#165 の形式）。ノードは 14/12/12、`verified_against: v2.0.0` で、`rollup.py --goal G0issue147` は混在の勘定（12 + 作らない 2 = 14）で完了と判定する。

### 負の対照

| 変異 | 結果 |
|:---|:---|
| `read_n_bytes` が長さを無視する | readNBytes(int) と randomized が失敗 |
| `skip_n_bytes` が足りなくても `Ok` を返す | skipNBytes と randomized が失敗 |
| 範囲の検査を `<=` から `<` にずらす | read into array、readNBytes(byte[], int, int)、randomized が失敗 |
| `read_range` がオフセットを無視する | read into array、readNBytes(byte[], int, int)、randomized が失敗 |
| `available` が今の配列の残りだけを数える | spec は通る（原典は配列 1 つ）。unit テストの連結の 3 件が失敗 |

### 原典外の回帰（台帳の件数に含めない）

`byte_string_input_stream_test.rs` に 4 件を足した（`b2ce830`）。連結した `ByteString` で、`available`、まとめ読み、`skip_n_bytes` が配列の境目をまたぐこと、範囲外の指定でストリームが動かないことを確かめる。

## ActorSystemScala213PlusSpec（G0issue99）

- 項目256 の報告書は「v2.0.0 でファイルが削除された」としていたが、訂正する。共通祖先 `0ca804c7af` にこのファイルはない。上流の #2486（ActorSystem の `close`）は、1.x 系統では `3be76d1be7` で scala-2.13+ の新しいファイルとして入った。2.0 系統では `d5ee8a63e3` で `actor-tests/src/test/scala/org/apache/pekko/actor/ActorSystemSpec.scala` の中に入った。ケース「Scala's Using automatically terminates ActorSystem」の本体は、両方で同じである。
- Rust のテスト `crates/fraktor-actor/tests/actor_system_scala213_plus_spec.rs` は残した。冒頭の出典に移動先を書いた（`2281fe2`）。1 passed。
- 台帳：旧 id のノードは green、v1.7.1、1/1/1 のまま、履歴として残す。`--list-unverified` には「v2.0.0 で未照合」として出続ける。`ActorSystemSpec` のノード（unported、v2.0.0 で 20 ケース）は unported のままにする。移植済みの 1 件は、この記録にだけ書く。二重には数えない。退役と承継を機械でどう集計するかは、#166 が main に入った後に別の差分で決める。

## 混在のノードの勘定（台帳の検査）

`UnsynchronizedByteArrayInputStreamSpec` は、一部のケースだけを作らない区分にした最初のノード（混在）である。#165 の schema は、混在の green の条件を「`ported` と承認済みの `excluded` と `not_built` の件数の和が `total` に等しい」と定めているが、道具には入っていなかった。このまとまりで、次の 2 つに同じ規則を入れた。

- `rollup.py`：green で `cases.not_built` を持つノードは、要素の記録が揃い、`passing == ported` で、和が `total` に等しく、除外と同じケース名がないときだけ完了に数える。満たさなければ未完了の一覧に理由を出す。作らないの有効な件数にも、記録の揃った混在のノードの分を入れる。`goal_complete_gate_test.py` に 2 件を足した。変更前の `rollup.py` では、承認のない混在のノードが完了に数えられ、この 2 件が失敗する。
- `verify_red.py`：already_green の式を `passed + excluded + not_built == original_cases` にした。作らないの件数は `excluded_approvals.count_not_built` が数える。要素の検査（`is_valid_not_built_case`）は rollup と共有する。記録不足、重複、除外との重なりがあれば件数を返さず、already_green にしない。式が変わらないままだと、記録の揃った混在のノードは毎回 `unexpected_pass` になり、port-test に戻り続ける。`verify_red_test.py` に 2 件を足した（承認済みの 2 件で already_green になること、承認がなければ `unexpected_pass` のままであること）。式を戻すと前者が失敗する。`record_already_green.py` は total に原典の件数、ported と passing に通過数を書くので、変えなくても混在の勘定と合う。
- `verify_green.py` はケースを数えないので、変えていない。

## PR #168 のレビュー対応（固定 HEAD `9014034`）

| 指摘 | 分類 | 対応 |
|:---|:---|:---|
| 独立レビュー P1：実原典で already_green に届かない。外側の `"UnsynchronizedByteArrayInputStream" must {` が件数の候補に入り、15 対 14 で `original_cases` が None になる | 直す | `verify_red.py` の `count_original_cases` は、WordSpec の見出し（`"…" must/should/can/when/which {`）を、その波括弧のブロックに既知の書式のケースが 1 件以上あるときだけ見出しとして認め、ケースには数えないようにした。空の見出しと、未知の書式だけを持つ見出しは、今までどおり数えられない扱いにする。先に足したテスト（実原典と同じ構造の fixture。`b10fb1d`）は修正前に失敗した。固定の v2.0.0 checkout で `verify_red.py` を実行すると、修正前は `unexpected_pass`・`original_cases=null`、修正後は `already_green`・14 = 12 + 作らない 2 だった。green の全ノードを実原典で数え直すと、数えられなかったノードは 42 から 6 に減った。数えられたノードの件数は、すべて台帳の total と一致した（件数が変わったノードはない） |
| Devin r4688743287、独立レビュー P2：混在のノードで、未承認の除外を rollup が無視する | 直す | `excluded_approvals.excluded_problems` を足し、rollup の混在の勘定と `count_excluded` が同じ検査を使うようにした。未承認の要素があれば、rollup は未完了、verify_red は件数を返さない |
| Devin r4688743446、独立レビュー P2：同名の承認済みの除外を 2 回数える | 直す | 同じ `excluded_problems` で、除外の中のケース名の重複を拒否する。先に足した回帰テスト（`goal_complete_gate_test.py` 45a・45b、`verify_red_test.py` 19d）は修正前に失敗した |
| Devin r4688743567：移植と等価性レビューの facet が、混在の作らないケースを扱わない | 直す | `fraktor-port-test.md` と `fraktor-review-equivalence.md` に、承認済みの作らないケースは移植せず、件数の照合から引くことを書いた |
| Devin r4688743654：記録の不足した混在のノードを人間の確認へ回す分岐がない | 直す | `fraktor-port-node.yaml` の triage の needs_human の条件と、`fraktor-triage.md` の判定の優先順位に、混在の green のノードを含めた |
| Devin r4688743740、独立レビュー 3：テストの注記が古い（承認を未記録と書く、v1.7.1 の行番号、旧 API の赤） | 直す | Rust テストの冒頭を、v2.0.0 の行（`ByteString.scala` L484、Spec の L45）、v2.0.0 の赤（E0432 と E0599）、台帳の承認済みの作らないケースに合わせて直した |
