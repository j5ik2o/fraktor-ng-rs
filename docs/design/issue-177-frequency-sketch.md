# 設計：#177 FrequencySketch／FastFrequencySketch（PR #205）

[Issue #177](https://github.com/j5ik2o/fraktor-ng-rs/issues/177) と [PR #205](https://github.com/j5ik2o/fraktor-ng-rs/pull/205) の設計の記録である。Pekko v2.0.0（`9fbd2283`）の classic `FrequencySketchSpec` を移植し、`kernel::frequency_sketch` に `FrequencySketch` と `FastFrequencySketch` を足した。

2026-11-25 のオーナーの決定で、実装の独立レビューを廃止し、設計の独立レビューに置き換えた。この PR は実装が先に済んでいたので、出来上がった時点（HEAD `148eaec`）の設計をあとから書いた。実装に関わっていない担当が、この文書を読んで設計をレビューする。#176 の前例は [PR #178](https://github.com/j5ik2o/fraktor-ng-rs/pull/178) の設計である。原典との対応と検証の記録は [ゴール文書](../goals/issue-177-frequency-sketch.md) にある。

## 1. 何をどう変えたか

kernel に、TinyLFU のための count-min sketch を 2 つ足した。Pekko v2.0.0 の `org.apache.pekko.util.FrequencySketch` と `FastFrequencySketch` の移植である。

| ファイル | 中身 |
|:---|:---|
| `crates/fraktor-actor/src/kernel.rs` | `pub mod frequency_sketch;` の 1 行 |
| `crates/fraktor-actor/src/kernel/frequency_sketch.rs` | モジュールの宣言と再エクスポート |
| `.../frequency_sketch/frequency_sketch_value.rs` | `FrequencySketch<A, H>` |
| `.../frequency_sketch/fast_frequency_sketch.rs` | `FastFrequencySketch<A, H>` |
| `.../frequency_sketch/counter_matrix.rs` | 両者が共有するカウンタの行列 `CounterMatrix` |
| `.../frequency_sketch/sketch_width.rs` | 検証済みの幅 `SketchWidth` |
| `.../frequency_sketch/sketch_depth.rs` | 検証済みの深さ `SketchDepth` |
| `.../frequency_sketch/counter_bits.rs` | 検証済みのカウンタの大きさ `CounterBits` |
| `.../frequency_sketch/reset_size.rs` | リセットの大きさ `ResetSize` |
| `.../frequency_sketch/bits.rs` | `Bits`（2 の冪の 3 関数。`pub(crate)`、§8） |
| `.../frequency_sketch/murmur3.rs` | `MurmurHash3` の 3 歩 |
| `.../frequency_sketch/string_hasher.rs`、`string_hash_code.rs` | 文字列の 2 つのハッシュ |
| `.../frequency_sketch/frequency_sketch_hasher.rs` | ハッシュの trait `FrequencySketchHasher`（封じた trait、§8） |
| `.../frequency_sketch/hasher_seal.rs` | 封じるための supertrait `HasherSeal`（非公開のモジュール、§8） |
| `.../frequency_sketch/frequency_sketch_settings.rs`、`fast_frequency_sketch_settings.rs` | 設定 |
| `.../frequency_sketch/frequency_sketch_error.rs` | `FrequencySketchError` |
| `.../frequency_sketch/*_test.rs` | 原典外の単体テスト（JVM の値に固定） |
| `crates/fraktor-actor/tests/frequency_sketch_spec.rs` | 原典 22 ケースのうち 20 件。残る 2 件は `src/` の単体テスト（§8） |
| `crates/fraktor-actor/tests/frequency_sketch_property_test.rs` | PBT（§3 の P1〜P5、P7）。P6 は `bits_test.rs`（§8）。原典外で、台帳の件数に含めない |
| `crates/fraktor-actor/tests/frequency_sketch_allocation_test.rs` | 深さ 0 の構築が行を確保しないことの確認（最大の確保要求を記録する global allocator を持つ単独のバイナリ）。原典外で、台帳の件数に含めない |
| `docs/design/specs/frequency_sketch.qnt`、`frequency_sketch_test.qnt` | Quint のモデルとインスタンス（§4） |
| `ledger/nodes.yaml` | 対象ノードを green（22/22/22、`verified_against: v2.0.0`）に |
| `docs/baseline/v2.0.0-undocumented-nodes.tsv` | 移植した仕様を `no-report` として 1 行追加 |

### 形

2 つの sketch は、値から各行の位置を決めるハッシュだけが違う。カウンタの行列、加算、飽和、リセット、`size` は共通なので、`CounterMatrix` にまとめた。

- `CounterMatrix` は `rows: Vec<Vec<u64>>`（行ごとの 64 ビット語の列）、`row_sizes: Vec<i32>`、`updated_size: i32`、`reset_size`、語とカウンタを取り出すためのマスクとシフトを持つ。
- 1 つのカウンタは 2、4、8、16、32、64 ビット。1 語に `slots = 64 / counterBits` 個入る。行の語数は `row_width = max(1, width / slots)`。
- 位置からカウンタを引く `locate` は原典の `getCounter`／`incrementCounter` のままである。語は `(hash & (width - 1)) >>> slotShift` で選び、語の中のカウンタは `(hash & (slots - 1)) << counterShift` の位置のビットで選ぶ。**カウンタの添字は幅でマスクしない。** 幅が 1 語より小さいときも、幅より上のビットだけが違う位置は、同じ語の別のカウンタに落ちる。原典と同じく、これを崩すと JVM と同じビット列にならない。
- `increment` は行ごとに、カウンタが最大値でなければ 1 を足して `row_sizes` を 1 増やす。1 行でも変わったら `updated_size` を 1 増やし、`updated_size == reset_size` ならリセットする。全行が飽和していたら、その更新は数えない。
- `frequency` は全行のカウンタの最小値。行が無いときは `i32::MAX`。
- リセットは、各語の奇数ビット（`odd_mask`）を数え、語を `>>> 1` して `reset_mask` でマスクする。行の大きさは `(row_sizes - odd) >>> 1`、`updated_size` は行の大きさの最大にする。奇数を先に引くのは、切り捨て（`>>> 1`）で失う 0.5 を相殺するためである。
- `FrequencySketch` のハッシュは `MurmurHash3.stringHash`（種 135283237、UTF-16 の符号単位を 2 つずつ）で、`rehash = finalizeHash(mixLast(hash, hash), 2)`、行の位置は `hash1 + row * hash2`（`i32` の折り返し）。
- `FastFrequencySketch` のハッシュは `String.hashCode`（`31` の多項式、UTF-16）で、独自の `rehash`（hash-prospector）と行ごとの大きな素数 `SEEDS` を使う。位置は `(hash + seed) * seed` を 64 ビットで計算し、上位 32 ビットを足して 32 ビットに畳み、幅でマスクする。
- 設定の検査は `SketchWidth`（あふれ、2 の冪、正）、`CounterBits`（2/4/8/16/32/64）、`SketchDepth`（負でない）の順である。`ResetSize` は `min(i32::MAX, (reset_multiplier * capacity) as i64) as i32`。

### 原典と違えたところ

- 原典の `require` の例外は、すべて `FrequencySketchError` の値にした。最初に失敗した検査を返す。
- 原典が幅 0 以下を受け付けて最初の更新で配列の範囲外になるのに対し、移植は作成時に `WidthNotPositive` を返す。2 の冪の検査は 0 と `i32::MIN` を通るので、その後ろに置いた。
- 容量が `2^30` を超えると、`ceiling_power_of_two` は原典どおり `i32::MIN` になる。既定の倍率では 64 ビットの積が負になり、下位 32 ビットが 0 になる。原典のあふれの検査もこれを通すので、`WidthOverflow` ではなく `WidthNotPositive { width: 0 }` を返す。
- 深さ 0 は原典どおり受け付ける。`frequency` は `i32::MAX`、`size` は 0。
- 容量 0 以下も原典どおり受け付ける。幅は `width_multiplier`。リセットは、積を原典どおり `i32` に切り詰めた値が正のときだけ走る。容量 0 では走らず、容量が負なら倍率しだいで走りうる（§5）。`i32::MIN` だけは `WidthNotPositive`（§8）。
- 原典の `Hasher[A]` は暗黙引数なので、`FrequencySketchHasher<A>` の trait にし、`with_hasher` で渡せるようにした。既定はそれぞれ `StringHasher` と `StringHashCode`。原典の `Hasher` は `sealed trait` で、利用者は実装できない。移植の trait も、ハッシュ型と値の型の組ごとに封じた（§8）。
- 深さは `SketchDepth` という検証済みの型にした。`rows: usize` を設定から直接持ち回らず、負の検査と 0 の扱いを 1 箇所に閉じるためである。
- クラス名は原典のまま（`FrequencySketch`、`FastFrequencySketch`）。製品名の改名の規則は Pekko という製品名が対象で、原典のクラス名は対象外である（#126・#136・#139 と同じ判断）。

### 作らないもの

`toDebugString` と `FrequencySketchUtil.debugString`（デバッグ用の表）は作らない。原典のケースは使っておらず、ケースの「作らない」区分にも当たらない（22 件は全件移植した）。

### 変えないこと

- 既存の公開 API、型、モジュールの公開面には触れていない。追加は `kernel::frequency_sketch` だけである。
- `FrequencyList`（#128）、`RecencyList`、`SegmentedRecencyList`、`DoubleLinkedList`（#176）は変えていない。TinyLFU のキャッシュ本体もまだ無く、この型を利用する側はこの PR に含めない。

### 範囲の外

- 原典の型は Pekko の内部 API（`@InternalApi private[pekko]`）である。移植は kernel の公開型にしたが、Pekko の利用者向け API に当たるものではない。
- スレッド安全ではない。原典と同じく `&mut self` で更新するので、共有して書くには外側で排他する。
- 文字列以外の型。ハッシュは `FrequencySketchHasher<A>` で差し替えられるが、原典が同梱する実装は文字列の 2 つだけなので、同じ範囲にした。
- キャッシュの適応（TinyLFU の admission）と `size` を使う追い出し。これらは利用側の仕事である。

## 2. 守る不変条件

`M` を 1 つの行列、`r` を行、`i` をカウンタの位置、`max(counterBits)` をその大きさのカウンタの最大値（2、4、8、16 ビットは `2^bits - 1`、32 ビットは `2^32 - 1`、64 ビットは `i64::MAX`）とする。

| 番号 | 不変条件 |
|:---|:---|
| I1 | カウンタの範囲：どのカウンタも `0 <= c <= max(counterBits)`。加算は最大値のカウンタを増やさない（飽和） |
| I2 | 行の大きさ：`row_sizes[r]` は、その行でこれまでに増えたカウンタの数（リセットの半減を反映した値）に等しい。つねに `row_sizes[r] == sum(counters[r])` である（加算は両方を 1 増やし、リセットは両方を `(x - odd) / 2` にする） |
| I3 | 全体の大きさ：`updated_size` は、前回のリセット以降に「1 行でもカウンタが増えた」更新の数。`updated_size >= row_sizes[r]`（全行）。リセットの直後は `updated_size == max(row_sizes)`。深さ 0 なら常に 0 |
| I4 | リセット：`reset_size >= 1` のとき、`updated_size` が `reset_size` に達した更新の直後に 1 回だけ走る。全カウンタを `>>> 1` し、行の大きさを「奇数カウンタの数」だけ先に引いてから `>>> 1` する。その後 `updated_size` を最大の行の大きさにする。リセットの大きさは積の下位 32 ビットなので、負の倍率でも 1 以上になりうる（例：容量 1、倍率 `-4294967295.0` は 1 になり、数えた更新のたびに走る）。`reset_size <= 0`（0、NaN の `as i64` は 0、下位 32 ビットが 0 以下の積）のときは、`updated_size` が `i32` で一巡するまで（数えた更新が `2^31` 回に届くまで）走らない。どれも原典の `.toLong.toInt` と同じ |
| I5 | 幅：`width = (width_multiplier * ceiling_power_of_two(capacity)) as i64`。`width > i32::MAX` は `WidthOverflow`。`width as i32`（下位 32 ビット）が 2 の冪でなければ `WidthNotPowerOfTwo`（0 と `i32::MIN` は 2 の冪として通す）。0 以下なら `WidthNotPositive`。`ceiling_power_of_two` は 32 ビットの折り返しで、`1 << -(leadingZeros(value - 1))`。1 以下は 1（ただし `i32::MIN` だけは引き算が `i32::MAX` に折り返して `i32::MIN` 自身）、`2^30` 超は `i32::MIN` |
| I6 | 深さ：`depth >= 0`。0 を許し、`frequency` は `i32::MAX`、`size` は 0（I3） |
| I7 | ハッシュ：`FrequencySketch` は `MurmurHash3.stringHash` の種 135283237、`rehash = finalizeHash(mixLast(hash, hash), 2)`、行の位置は `hash1 + row * hash2`。`FastFrequencySketch` は `String.hashCode`、hash-prospector の `rehash`、行ごとの `SEEDS`、位置は `((hash + seed) * seed + high32) as i32 & (width - 1)`。同じハッシュと設定なら、JVM と同じカウンタに落ちる |
| I8 | 折り返し：数と計数は原典の `Int` と同じく `i32` で持ち、折り返す。`u64` の語のシフト量は 64 の剰余、`i32` のシフト量は 32 の剰余（JVM と同じ）。64 ビットから `i32` への変換は下位 32 ビット（`toInt`）。`f64` から `i64` は飽和し、NaN は 0。したがって 32／64 ビットのカウンタは `i32` に切り詰められ、`2^31` を超えると負になりうる（原典の `toInt` と同じ） |
| I9 | 意味：`frequency(v)` は全行の `v` のカウンタの最小値。`size()` は I3 の `updated_size`。作成は不正な設定を `Err` で返し、成功した sketch の `increment` と `frequency` は panic しない（位置は必ず語の範囲に収まる）。ただし、有効でも行列が巨大な設定（幅が大きい、深さが大きい）は、確保の失敗でプロセスが abort しうる（32 ビットの対象では capacity overflow の panic）。`Err` にするかは [#220](https://github.com/j5ik2o/fraktor-ng-rs/issues/220) で決める |

I2 と I3 は Quint で確かめる。I3 の前半（数えるのは 1 行でもカウンタが増えた更新だけ）は、深さ 1 のインスタンス `single` の `oneRowSizeIsTheSize` と、PBT の P1 の完全一致で確かめる。I5、I7 は原典のソースと単体テスト（JVM の値）で、I1、I6、I9 は PBT と Quint で確かめる。I9 の「`frequency` は最小値」は Quint の `frequencyIsTheSmallestCounter` で確かめる。カウンタの位置（幅が 1 語より狭いときに、カウンタの添字を幅でマスクしないこと）は、Quint が位置を抽象化しているので捉えられず、単体テスト（JVM の値）で確かめる（§4、§5）。I8 の「32／64 ビットのカウンタは `2^31` で負になりうる」は、原典の `getCounter` が `Long` を `.toInt` するためである。この切り詰めを `2^31` 超えで実行する検査は無い（§5 の範囲の外）。

## 3. PBT の性質

PBT は、固定 seed の SplitMix64 で設定と入力を選び、ランダムな更新の列を流して、毎回の更新の後で性質を確かめる形にした。失敗したときは seed、手の番号、設定を表示するので、同じ入力で再現できる。外部の依存は足していない（`proptest` を workspace の dev-dependency にするかは [#212](https://github.com/j5ik2o/fraktor-ng-rs/issues/212) で決める）。

| 番号 | 性質 | 確かめる不変条件 |
|:---|:---|:---|
| P1 | 毎回の更新の後で、`size` は負でなく、更新の回数を超えず、有効なリセットの大きさより小さい。深さ 0 なら 0。リセットの直後はリセットの大きさの半分以下 | I3、I4、I6 |
| P2 | 毎回の更新の後で、各値の `frequency` は 0 以上で、カウンタの最大値以下。32／64 ビットは最大値が `i32` に収まらないので 0 以上だけ。深さ 0 なら `i32::MAX` | I1、I6 |
| P3 | 最初のリセットより前は、各値の `frequency` は真の回数を下回らない（カウンタの最大値まで。32／64 ビットは最大値が `i32` に収まらないので真の回数まで） | I1、I9 |
| P4 | リセットの手で、各値の `frequency` は `floor(f / 2) <= f' <= floor((f + 1) / 2)` を満たす（`f` はリセットの直前の値。更新の対象になったカウンタは先に 1 増えるので、上界は `+1`） | I4 |
| P5 | 任意の文字列について、`StringHasher` と `StringHashCode` は原典の定義に等しい。空文字、ASCII、2・3 バイトの UTF-8、サロゲートペア、長い文字列を含む | I7 |
| P6 | 任意の `i32` について、`Bits` の 3 関数は定義に等しい。`i32::MIN`、`i32::MAX`、`2^30` の前後を含む | I5 |
| P7 | 2 つの構築子は、原典が受け付けて添字できる設定だけを受け付け、最初に失敗する検査の値を返す。検査の順は、原典の順に、原典に無い `WidthNotPositive` を 2 の冪の検査の直後に足した順である（§1）。そのため、幅 0 と不正な `counterBits` が重なると、原典の `IllegalArgumentException`（`counterBits`）ではなく `WidthNotPositive` を返す。容量と倍率の境界（`0`、負、`i32::MIN`、`2^30`、`2^30 + 1`、`i32::MAX`）、すべての `counterBits`、負の深さを含む | I5、I6、I9 |

除外は付けない。深さ 0、リセットの無効（0、負、NaN、1e12）、すべてのカウンタの大きさ、空文字、非 ASCII、サロゲートペア、長い文字列を、入力の抽選にすべて入れた。32／64 ビットのカウンタで `2^31` を超える値は、1 回の実行の更新が最大 2000 回なので PBT でも Quint でも届かず、単体テストも踏まない。I8 の切り詰めは原典の `toInt` を写したもので、この範囲の実行の検査は無い。

**実装（案 1、依存なし）。** `crates/fraktor-actor/tests/frequency_sketch_property_test.rs`。

- `check_run` を 300 seed 流す。設定は、容量 1〜200（4 分の 1 は 1〜3）、倍率 1/2/4/8/16、リセット倍率 0／負／0.5／1／2／10／NaN／1e12、カウンタ 2/4/8/16/32/64 ビット、深さ 0/1/2/3/4/5/8。3 分の 1 は `FastFrequencySketch`。
- 値は最大 40 件の文字列の組から選び、50／300／2000 手を流す。1 手ごとに P1〜P4 を確かめる。
- P5 は 3000 件のランダムな文字列、P6 は境界 11 件と 20,000 件の `i32`、P7 は 20,000 件の設定で確かめる。P6 は `Bits` が `pub(crate)` なので、`src/kernel/frequency_sketch/bits_test.rs` の単体テスト `bits_follow_their_definitions` に置いた（同じ SplitMix64、seed `0xB175`、同じ境界と抽選。§8）。
- P7 は、構築すると行列を確保するので、原典が拒む設定か、幅が 1 以上 `2^16` 以下の設定だけを実際に構築する。幅は、構築子と同じく 64 ビットの積を `i32` に切り詰めた値で判定する（負の積が、切り詰めで `2^30` などの大きな正の幅になるため。固定 seed で 234 件）。2 つのスケッチは、それぞれ自分の誤りで判定する。`FastFrequencySketch` はカウンタの大きさも深さも受け取らないので、一般版の誤り（`UnsupportedCounterBits` など）を条件にすると、有効な幅 `2^29` か `2^30` の Fast を何度も構築する（固定 seed で 79 件、1 件あたり 1〜2 GiB）。PR #205 の CI は、この確保でランナーが止まった。
- 結果（2026-11-26、上の修正の後）：`cargo test -p fraktor-actor --release --test frequency_sketch_property_test` は 4 passed（0.71 秒）。P7 が実際に構築したのは、一般版 511 件、Fast 版 1560 件で、幅はどれも `2^16` 以下。

## 4. 状態遷移と Quint

**Quint は要る。** `FrequencySketch` は状態を持つデータ構造で、更新（加算、飽和、リセット）が状態を変える。I2 の「行の大きさがカウンタの和に等しい」と I4 の「リセットで奇数を先に引く」は、遷移の列の全体で成り立つ必要があり、固定のテストでは取りこぼしうる。実際、リセットの奇数補正を落とす変異は PBT の P1〜P4 では落ちない（P4 の半減の範囲は満たしたまま、行の大きさだけがずれる）。この誤りは Quint の `rowSizeIsCounterSum` で落ちる（原典のケースのうちリセットの 2 件と、JVM の値の単体テストも落とす）。リセットの後の大きさを最大の行ではなく 1 行目の大きさにする誤りは、PBT も原典のケースも落とさず、JVM の値の単体テストと Quint だけが落とす（下の表）。並行は無いので、仕様は 1 スレッドの遷移だけを扱う。

仕様は `docs/design/specs/frequency_sketch.qnt`（本体）と `docs/design/specs/frequency_sketch_test.qnt`（インスタンスと負の対照）。

- 状態：`counters`（行ごとのカウンタを語ごとに並べた `List[List[int]]`）、`rowSizes`、`updatedSize`、`place`（値から各行の位置への写像）、`ghost`（真の回数。リセットで半減）、`saturated`（加算が最大値で出会ったカウンタの集合。行と行の中の添字の組）。
- 遷移：`init` と `increment(v)` だけ。`init` は `place` を非決定的に選ぶので、衝突も幅を超える位置もすべて探索する。ハッシュの値そのものはモデルにせず、P5／P6 と単体テストで JVM に固定する。
- 64 ビット語は展開して、カウンタの列として持つ。位置からカウンタを引く `cellOf` は、コードの `locate` と同じ（語は幅でマスクし、カウンタは下位ビットだけで選ぶ）である。ただし、`place` を任意に選ぶので、`cellOf` を変えても置き方が違うだけの正しい count-min sketch になり、不変条件は落ちない（設計レビューの QM1。カウンタの添字を幅でマスクしても `[ok]`）。カウンタの位置そのものは、単体テストで JVM の値に固定する（§5）。
- 不変条件：`positionsInRange`（位置が行に収まる）、`countersBounded`（I1）、`rowSizeIsCounterSum`（I2）、`rowSizesWithinSize`（I3）、`sizeBelowResetSize`（I4。有効なリセットより小さい）、`noRowsNoSize`（I6）、`oneRowSizeIsTheSize`（I3 の前半。深さ 1 なら `updatedSize == rowSizes[0]`。飽和した更新を数えると落ちる）、`frequencyIsTheSmallestCounter`（I9。`frequency` はどの行のカウンタも超えず、どれかに等しい）、`noUnderestimateWhileUnsaturated`（飽和したことの無いカウンタは、そこに載った値のゴーストの回数以上で、どのカウンタも飽和したことの無い値の `frequency` はゴーストを下回らない。飽和はカウンタごとに持つので、1 つが飽和しても、ほかのカウンタの検査は続く）。まとめて `inv`。
- インスタンス：`narrow`（2 ビット、幅 4、1 語 32 個。幅より上の添字と早い飽和の遷移を通す。ただし `place` を任意に選ぶので、添字の選び方（カウンタの添字を幅でマスクしないこと）は確かめない。それは単体テストが確かめる。再レビュー N1）、`fast`（4 ビット、幅 16、1 語 16 個）、`wide`（8 ビット、幅 32、1 語 8 個。語の添字が効く）、`single`（深さ 1、2 ビット、幅 4。`oneRowSizeIsTheSize` が効く）、`noReset`（リセットの大きさ 0）、`noRows`（深さ 0）。
- 負の対照：`bugNoOddAdjust`（リセットの奇数補正を落とす）、`bugNoSaturation`（飽和を落とす）、`bugStaleSize`（リセットの後に `updatedSize` を更新しない）、`bugFirstRowSize`（リセットの後の大きさを最大の行ではなく 1 行目の大きさにする）、`bugCountSaturated`（飽和した更新も数える。深さ 1）、`bugMaxFrequency`（`frequency` が最大値を取る）。それぞれ `inv` が落ちることを確かめる。

モデルにしていないもの：ハッシュの値（抽象化して任意の位置を許す）、`i32` の加算の折り返し（更新は最大 40 手なので届かない）、64 ビット語のビット演算そのもの（展開して同じ添字の計算だけを写す）、スレッド安全、`toDebugString`。

### 検査の結果（2026-11-26、Quint 0.32.0。設計レビュー F1〜F3、F8 の対応の後）

`quint run` はどれも `--max-samples=10000 --max-steps=40 --seed=0x1` で、6 本を並べて流した（時間は並列のときの値）。

| コマンド | 結果 |
|:---|:---|
| `quint typecheck docs/design/specs/frequency_sketch.qnt`、`..._test.qnt` | 成功 |
| `quint test --main=narrow --match='^(incrementCountersToMaxValue\|resetHalvesAndAdjustsForOddCounters)$' docs/design/specs/frequency_sketch_test.qnt` | 2 passing（各 10,000 test、7.9 秒） |
| `quint run --main=narrow --invariant=inv ...` | `[ok] No violation found`（10,000 トレース、183 秒） |
| `quint run --main=fast ...` | `[ok]`（213 秒） |
| `quint run --main=wide ...` | `[ok]`（245 秒） |
| `quint run --main=single ...` | `[ok]`（111 秒） |
| `quint run --main=noReset ...` | `[ok]`（178 秒） |
| `quint run --main=noRows ...` | `[ok]`（22 秒） |
| `quint run --main=bugNoOddAdjust --invariant=inv ...` | `[violation]` |
| `quint run --main=bugNoSaturation --invariant=inv ...` | `[violation]` |
| `quint run --main=bugStaleSize --invariant=inv ...` | `[violation]` |
| `quint run --main=bugFirstRowSize --invariant=inv ...` | `[violation]` |
| `quint run --main=bugCountSaturated --invariant=inv ...` | `[violation]` |
| `quint run --main=bugMaxFrequency --invariant=inv ...` | `[violation]` |
| `quint run --main=bugNoOddAdjust --invariant=rowSizeIsCounterSum ...` | `[violation]`（狙った不変条件で落ちる） |
| `quint run --main=bugNoSaturation --invariant=countersBounded ...` | `[violation]`（同上） |
| `quint run --main=bugStaleSize --invariant=sizeBelowResetSize ...` | `[violation]`（同上） |
| `quint run --main=bugFirstRowSize --invariant=rowSizesWithinSize ...` | `[violation]`（同上） |
| `quint run --main=bugCountSaturated --invariant=oneRowSizeIsTheSize ...` | `[violation]`（同上） |
| `quint run --main=bugMaxFrequency --invariant=frequencyIsTheSmallestCounter ...` | `[violation]`（同上） |

コードの負の対照（実装に 1 つずつ一時的に入れ、毎回元に戻した。`cargo test -p fraktor-actor --release` の `--lib frequency_sketch`（JVM の値の単体テスト）、`--test frequency_sketch_spec`（原典のケース）、`--test frequency_sketch_property_test`（PBT）を流した。上の 3 行の PBT と Quint の欄は実装の時点の値で、ほかの欄は設計レビュー F1、F3、F5 の対応で測った）：

| 変異 | 単体テスト（JVM の値） | 原典のケース | PBT | Quint |
|:---|:---|:---|:---|:---|
| リセットで奇数補正を落とす（`row_sizes - odd` を `row_sizes` に） | 3 件が失敗 | 2 件が失敗（`frequency_sketch_reset_counters_when_reset_size_is_reached` と Fast 版） | **4 passed（落ちない）** | `rowSizeIsCounterSum` が `[violation]` |
| 飽和を落とす（最大値のカウンタにも足す） | — | — | P3 が失敗（`estimated 0 below 3`） | `countersBounded` が `[violation]` |
| リセットの後に `updated_size` を更新しない | — | — | P1 が失敗（`size at or above the reset size`） | `sizeBelowResetSize` が `[violation]` |
| リセットの後の大きさを 1 行目の大きさにする（`max()` を `first()` に） | 3 件が失敗 | **22 passed（落ちない）** | **4 passed（落ちない）** | `bugFirstRowSize` の `inv` が `[violation]`（`rowSizesWithinSize`） |
| カウンタの添字を幅でマスクする（`locate` の `slot`） | `a_width_narrower_than_a_word_places_counters_as_the_original` が失敗 | 落ちない（設計レビュー CM1） | 落ちない（同） | 捉えられない（位置の抽象化。QM1） |
| リセットの大きさを切り詰めずに `i32` の範囲に飽和させる | `a_negative_reset_product_truncated_to_one_resets_every_update` が失敗 | 落ちない（設計レビュー CM7） | 落ちない（同） | モデルの外（`ResetSize` は定数） |

「—」は、実装の時点では流していない欄である。リセットの後の大きさの行が、Quint を要る理由の実測である。PBT と原典のケースはこの誤りを通し、JVM の値の単体テスト（決まった 1 本の列）と Quint の `inv`（任意の置き方と列）だけが落とす。奇数補正の行は、PBT が通すが、原典のケースと単体テストも落とす。PBT の P1〜P4 は行の大きさの整合（I2）を直接は見ていないので、どちらの誤りも通す。

**`quint test` には必ず `--match` を付ける。** Quint 0.32.0 では、`--match` が無いとテストを 0 件流して exit 0 で終わる（#176 の設計レビュー F8 と同じ）。`--match='.*'` のように広く取ると、`import ... .*` で取り込んだ定義も拾って失敗する。付けたときの件数（2 件）を確かめる。

## 5. 壊しうる入力

| 入力 | 何が起きうるか | 確かめているもの |
|:---|:---|:---|
| 容量 `0`、負 | `ceiling_power_of_two` が 1 になり、幅は `width_multiplier`（既定で 4）で、構築は成功する。リセットの大きさは、積を原典どおり `i32` に切り詰めた値（`i64` への飽和、`i32::MAX` での頭打ち、下位 32 ビット）で、それが正のときだけリセットが走る（I4）。容量 0 では積が 0 なので走らない。容量が負なら、切り詰めた値が正になりうる。倍率が負なら積が正になる（再レビュー N2。例：容量 -1、倍率 -2.0 は 2 で、2 回目の更新で走る）。倍率が正でも、負の積の下位 32 ビットが正になりうる（PR #223 の設計レビュー F2。例：容量 -1、倍率 4294967295.0 は積が -4294967295 で、下位 32 ビットが 1 なので、更新のたびに走る）。原典と同じく受け付ける（副元帥の決定、§8） | P7、単体テスト `a_capacity_of_zero_or_below_is_accepted_as_the_original`（両方の sketch。倍率 -2.0 と 4294967295.0 の両方） |
| 容量 `i32::MIN` | `ceiling_power_of_two` が `i32::MIN` になり、幅が 0 以下になる（`WidthNotPositive`） | P7 |
| 容量 `2^30`、`2^30 + 1`、`i32::MAX` | 幅が `2^30` か、`i32::MIN` への丸めで 0 になる（`WidthNotPositive { 0 }`） | P7、単体テスト（`2^30 + 1`） |
| 幅の倍率 `0`、負、`i32::MIN`、`i32::MAX` | 幅 0（`WidthNotPositive`）、2 の冪でない幅、64 ビットの積のあふれ（`WidthOverflow`） | P7、単体テスト |
| `counterBits` の各値（2、4、8、16、32、64）と不正値（-1、0、1、3、128） | 1 語のカウンタ数、飽和の最大値、読み出しの切り詰めが変わる。不正値は `UnsupportedCounterBits` | P7、単体テスト 5 通り、Quint の 2 ビットと 8 ビット |
| 深さ 0 | 行が無く、`frequency` は `i32::MAX`、`size` は 0。行のバッファも確保しない（`vec![row; 0]` でも `row` は一度作られるので、行は 1 行ずつ作る） | P1、P2、P7、Quint の `noRows`、`frequency_sketch_allocation_test.rs` |
| 深さが負 | `NegativeDepth`。原典は `NegativeArraySizeException` | P7、単体テスト |
| 深さが非常に大きい（`i32::MAX` など）、幅が非常に大きい（例：容量 `2^28` と 64 ビットのカウンタで、幅 `2^30`、1 行 8 GiB） | 行列の確保で失敗し、プロセスが abort しうる（原典の配列確保と同じ）。PBT は深さ 8、構築する幅は `2^16` までにした | 範囲の外（この文書に記載）。`Err` にするかは [#220](https://github.com/j5ik2o/fraktor-ng-rs/issues/220) |
| リセット倍率 `0`、負、NaN | 積の下位 32 ビットがリセットの大きさになる。0 以下ならリセットは走らない。負の倍率でも、下位 32 ビットが 1 以上なら走る（容量 1、倍率 `-4294967295.0` は 1 で、毎回走る） | P1、P4、Quint の `noReset`、単体テスト `a_negative_reset_product_truncated_to_one_resets_every_update`（JVM の値） |
| リセット倍率 `1e12` | 積が `i32::MAX` に飽和し、`2^31 - 1` 回まで走らない | P1 |
| リセットの直前と直後 | 奇数補正のずれ、`size` の取り違え、半減の境界 | P4、Quint（`resetHalvesAndAdjustsForOddCounters` と `inv`） |
| 空文字 | `StringHasher` は長さ 0 で finalize し、`StringHashCode` は 0 | P5、単体テスト |
| 非 ASCII、サロゲートペア | UTF-16 の符号単位を 2 つずつ混ぜる位置と、`char` ではなく符号単位で数える長さ | P5（😀、𝄞、é、ß、日、本） |
| 長い文字列（200〜400 片） | 2 符号単位ずつのループと、余りの 1 符号単位の扱い | P5 |
| 32／64 ビットのカウンタで `2^31` を超える | `toInt` の切り詰めで `frequency` が負になりうる（原典と同じ） | 単体テストも PBT も Quint も `2^31` に届かない。範囲の外と明記 |
| 幅が 1 語より小さい | カウンタの添字を幅でマスクすると、別のカウンタに落ちる | 単体テスト `a_width_narrower_than_a_word_places_counters_as_the_original`（容量 1 と 4、2 ビットと 4 ビットのカウンタ。`frequency` と `size` を JVM の値に固定）。Quint の `narrow` は位置を抽象化するので、この誤りを捉えられない（§4） |
| 同じ位置に落ちる値（衝突） | 頻度が実際より大きく出る（count-min sketch の性質） | Quint の `place`、P3（下回らないこと） |

## 6. 影響

- **公開 API**：追加だけ。`kernel::frequency_sketch` に `FrequencySketch<A, H>`、`FastFrequencySketch<A, H>`、`FrequencySketchSettings`、`FastFrequencySketchSettings`、`FrequencySketchError`、`FrequencySketchHasher`（封じた trait）、`StringHasher`、`StringHashCode`。`Bits` は `pub(crate)` にした（§8）。既存の型、関数、モジュールの公開面は変えていない。
- **原典にない `WidthNotPositive`**：原典は幅 0 以下を作成時に通し、最初の更新で配列の範囲外になる。Rust では作成時に `Result` で返す方が、失敗を早く、型で見えるようにできる。この PR の新しい型なので、既存の利用者への影響は無い。
- **原典にない `with_hasher` と `FrequencySketchHasher`**：原典の `Hasher[A]`（暗黙引数）を trait にした。既定のハッシュで足りる利用者は `new` だけでよい。原典の `Hasher` は `sealed trait` なので、移植の trait も封じた。文字列以外の型を数えるハッシュは、この crate の中に足す（§8）。
- **`Bits` の `pub(crate)`**：原典の `Bits` は `FrequencySketch` の内側の object で、ほかと同じく内部 API である。前の版はここに「原典の spec は `Bits` を使わない」と書いたが、誤りだった。原典の spec の 2 件（`FrequencySketchSpec.scala` の 278・281 行と 405・407 行）が `FrequencySketch.Bits.ceilingPowerOfTwo` を直接呼ぶ。`pub(crate)` にして、その 2 件と P6 を `src/` の単体テストに移した（§8）。
- **`SketchDepth`**：深さを検証済みの型にした（`148eaec`）。`rows: usize` を設定から直接持ち回らず、負の検査と 0 の扱いを 1 箇所に閉じるためである。`FrequencySketchError::NegativeDepth` はこの型の構築で返る。
- **`A: ?Sized` と `PhantomData<fn(&A)>`**：値を持たず、`str` のような unsized な型もハッシュに渡せる。`PhantomData<fn(&A)>` は `Send`／`Sync` の自動導出に影響しない（`A` を所有しない）。
- **no_std**：モジュールは `alloc::vec::Vec` と `core`（`core::marker`、`core::fmt`、`core::error::Error`）だけを使い、`std` を import しない。`fraktor-actor` の `extern crate alloc` の範囲に収まる（クレート全体は `#![no_std]` ではない）。
- **既存の契約**：無し。kernel の既存の型はこの型を使っていない。`FrequencyList` は内部の実装のままである。
- **保存済みのデータ・上流・守りの仕組み**：無し。台帳の対象ノードを green（22/22/22）にし、`docs/baseline/v2.0.0-undocumented-nodes.tsv` に 1 行足しただけである。
- **計算量**：`increment` と `frequency` は `depth` 回のカウンタ操作で、`depth` は設定（既定 4）。`size` は定数。リセットは語数 `depth * row_width` に比例し、`reset_size` 回に 1 回。原典と同じである。

## 7. レビューで見てほしい点

- **PBT（§3）の P1〜P7 が、§2 の不変条件を取りこぼしていないか。** 特に、P1〜P4 が行の大きさの整合（I2）を直接は見ていないこと（§4 の Quint が担う）と、32／64 ビットのカウンタの切り詰めを P2 が「0 以上」とだけ置いていることを、妥当と見るか。
- **Quint（§4）のモデルが実物と合っているか。** 64 ビット語を展開していること、位置の抽象化（ハッシュを任意の位置にする）、リセットの奇数補正と `updatedSize = max(rowSizes)`、飽和の扱い。負の対照 3 つが、狙った不変条件で落ちること。
- **公開 API の差分**：`WidthNotPositive`（原典は作成を通す）、`with_hasher`、`FrequencySketchHasher`、`SketchDepth` を、追加として妥当と見るか。
- **no_std** と、`core::error::Error` の実装。
- **PBT のライブラリ**：`proptest` を workspace の dev-dependency にするかは #212 で決める。この PR は依存を足していない。
- **範囲の外と明記したもの**：キャッシュの適応、スレッド安全、深さが非常に大きいときの確保、32／64 ビットのカウンタの `2^31` 超え。

## 8. PR #205 の後の追従（副元帥の決定）

PR #205 のマージの後、設計レビューで保留にした 3 点を副元帥が決めた。ここに書いたのは、その追従で変えた点だけである。

### 容量 0 以下は受け付ける

原典どおり受け付ける。コードは変えず、`FrequencySketch::new` と `FastFrequencySketch::new` の API 文書に書いた。容量は 1 に切り上がるので、幅は `width_multiplier` になる。リセットの大きさは、積を原典どおり `i32` に切り詰めた値で、それが正のときだけリセットが走る（§5）。容量 0 では積が 0 なので走らない。容量が負なら、倍率が負で積が正になる場合（再レビュー N2）と、倍率が正で負の積の下位 32 ビットが正になる場合（PR #223 の設計レビュー F2）に走る。`i32::MIN` だけは `ceiling_power_of_two` が `i32::MIN` になるので、正の幅の倍率では `WidthNotPositive` を返す（偶数の倍率では幅 0、奇数では幅 `i32::MIN`）。両方の sketch の単体テスト `a_capacity_of_zero_or_below_is_accepted_as_the_original` で、容量 0 でリセットが走らないこと、容量 -1 と倍率 -2.0 で 2 回目の更新でリセットが走ること、容量 -1 と倍率 4294967295.0 で 1 回目の更新からリセットが走ること（4 回の更新のどの後も `frequency` と `size` が 0）、`i32::MIN` の誤りを確かめる。

### `FrequencySketchHasher` を封じた

原典の `Hasher` は `sealed trait` で、実装は `StringHasher` だけである。移植の trait も、非公開のモジュール `hasher_seal` にある `HasherSeal<A>` を supertrait にして封じた（`FrequencySketchHasher<A: ?Sized>: HasherSeal<A>`）。`HasherSeal` は公開の trait の上界に使うので `pub` だが、モジュールが非公開なので、crate の外からは名前を書けず、実装もできない。実装は `impl HasherSeal<str> for StringHasher` と `impl HasherSeal<str> for StringHashCode` の 2 つだけである。

`HasherSeal` に値の型を持たせたのは、PR #223 の設計レビュー F1（Cursor と Devin の指摘も同じ）による。前の版の `HasherSeal` は値の型を持たなかったので、`StringHasher` と `StringHashCode` はどの `A` についても supertrait を満たした。Rust の孤児規則では、trait の型引数に自分の crate の型があれば、外の trait を外の型に実装できる。そのため、crate の外で `impl FrequencySketchHasher<MyKey> for StringHashCode` が書け、両方の sketch がその値を数えた。今の形では、その実装は `StringHashCode: HasherSeal<MyKey>` が満たされない E0277 で落ちる（`StringHasher` も同じ。`fraktor-actor` に path で依存する外部の crate で確かめた）。

trait の API 文書の例は 4 つで、コンパイルされる例と `compile_fail` の例を 2 組並べる。1 組目は新しいハッシュ型 `ByLength` で `str` を数える形、2 組目は既存のハッシュ型 `StringHashCode` で crate の外の値の型 `Key` を数える形である。各組は、封じた実装の有無のほかは同じ import、同じ型、同じ sketch の呼び出しを使う。stable の rustdoc は `compile_fail,E0277` の誤りの番号を照合しないので、import や綴りの誤りで落ちても doctest は通ってしまう。直前のコンパイルされる例が同じパスを使うので、その誤りはそちらで落ちる。外部の crate で 4 つを build すると、`compile_fail` の 2 つはどちらも `HasherSeal` が満たされない E0277 だけで落ち、ほかの 2 つはコンパイルされて動いた。

**Fast 版の扱い。** 原典の `FastFrequencySketch[A]` は `Hasher` を取らず、任意の `A` を `hashCode` で数える。封じた trait では「任意の型」を写せないので、封じる前に原典の契約と利用者を確かめた。

- 原典の `FastFrequencySketch` は `@InternalApi private[pekko]` で、Pekko の外の利用者への公開の契約は無い。
- Pekko の中の利用者が数える型は 2 つである。cluster-sharding の `EntityPassivationStrategy`（`EntityId`、つまり `String`）と、remote の artery の `InboundCompressions`（`FastFrequencySketchWrapper[T]` の `T` は `ActorRef` と、クラスのマニフェストの `String`。`InboundCompressions.scala` の 67・68 行、237 行、282 行、399 行）。
- `String` は `StringHashCode` が写す。`ActorRef` は fraktor-actor の中の型なので、remote の圧縮を移植するときに、`ActorRef.hashCode` を写したハッシュをこの crate の中に足せる。workspace の crate は `fraktor-actor` と `fraktor-testkit` だけである。
- `ActorRef` のハッシュはまだ無い（#226）。原典の `hashCode` は path の uid から作り、移植の `ActorRef` の等価性は id と世代で決まるので、何を写すかは足すときに決める。

したがって、crate の中の実装で原典の利用者の型をすべて写せるので、封じた。Fast 版の公開面は変えていない（`with_hasher` の `H` は封じた trait を満たす型だけになる）。

### `Bits` を `pub(crate)` にした

原典の `Bits` は `private[pekko]` で、spec から呼べるのは同じ package だからである。Rust では、それは crate の中の単体テストにあたる。狭いほうで出せば、後から互換を壊さずに広げられる。

- 原典の spec の 2 件（「FrequencySketch must」と「FastFrequencySketch must」の「detect width overflow that would have been missed by Int arithmetic」）は、本番の `Bits::ceiling_power_of_two` を直接呼ぶ形のまま、`frequency_sketch_value_test.rs` と `fast_frequency_sketch_test.rs` に移した。写しの関数にはしていない。`tests/frequency_sketch_spec.rs` は 20 件になった。台帳の `rust.cases_outside_test_file` と、ゴール文書の対応表に、ケースごとの置き場所を書いた。台帳の件数（22/22/22）は変わらない。欄の定義（要素は `case` と `file`、件数は `test_file` との和）は `docs/decisions/ledger-schema-draft.md` に、移植の規約の例外は `.takt/facets/knowledge/fraktor-porting-rules.md` に書いた（PR #223 の設計レビュー F3、Devin の指摘）。検査はまだこの欄を読まない（#225）。`verify_red` と `verify_green` は `test_file` だけを流すので、このノードの遷移は TAKT の already_green では記録しない（定義の文書に書いた）。
- PBT の P6 は `bits_test.rs` の `bits_follow_their_definitions` に移した。性質、seed（`0xB175`）、境界 11 件、20,000 件の抽選は変えていない。SplitMix64 と `reference_ceiling_power_of_two` は、同じ定義を単体テストの側にも置いた（P7 が `tests/` の側で使い続けるため）。
