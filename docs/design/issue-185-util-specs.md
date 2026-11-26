# 設計：#185 util の3 Spec（ByteStringInitializationSpec、AsciiStringCopySpec、DurationSpec）

[Issue #185](https://github.com/j5ik2o/fraktor-ng-rs/issues/185) の設計の記録である。対象は Pekko v2.0.0（`9fbd2283b1cf701c90a1223bf621942c191db61f`）の actor-tests にある util の3つの Spec で、依存する issue は無い。この文書は第1段（設計だけ）であり、実装と台帳の更新は、設計レビューが通ってから第2段で行う。

結論を先に書く。**本体の新規追加は無い。** 3つの Spec が検証する対象は、fraktor-ng-rs のコードではなく、JVM と Scala 標準ライブラリの仕組み（クラスローダによるクラス初期化の順序、JDK の `String` の内部表現を読む最適化、`scala.concurrent.duration.Duration`）である。fraktor-actor の中で対応する役割を担うのは、既存の `ByteString` と、kernel 全体が使う `core::time::Duration` と、kernel の `Clock` の port を実装する手動の時計であり、この3つで足りる。残るケースは、作らない区分（`not_built`）と除外（`excluded`）の提案として §7 にまとめ、レビューで決めてほしい点として §8 に挙げた。作らない区分と除外の最終的な承認は上司が取るため、この設計ではどちらも「承認待ち」と記す。提案が承認されれば、台帳の3ノードのうち DurationSpec は green に、ByteStringInitializationSpec と AsciiStringCopySpec は全件が作らない区分になるため green ではなく `not_built` になる（§7）。issue の文面は「対象ノードを green にする」だが、作らない区分は rollup が処理済みとして数える区分であり、状態の名前が変わる点をレビューで確認してほしい。

## 1. 何を移植するか

### 件数の数え直し

原典の `" in {"` の数を数え、コメントの中のケースを除いた。依頼の目安は ByteStringInitializationSpec が1件、AsciiStringCopySpec が3件、DurationSpec が4件で、合計8件である。数え直した結果は次のとおりで、DurationSpec だけが目安と違う。

| Spec | 目安 | 原典の `" in {"` の数 | 実際に動くケース |
|:---|---:|---:|---:|
| ByteStringInitializationSpec | 1 | 1 | 1 |
| AsciiStringCopySpec | 3 | 3 | 3 |
| DurationSpec | 4 | 4 | **3** |
| 合計 | 8 | 8 | **7** |

DurationSpec の4件目の「check its range」は、原典でブロックコメント（`/* ... */`）の中にあり、Scala でも実行されない。台帳の規約は「ケース数は近似ではなく実数を数える」なので、この設計は実行される3件を総数とし、コメントの中のケースは数に入れない。この扱いを変える場合は §8 の判断材料3を見てほしい。台帳の `test_cases_approx` は ByteStringInitializationSpec が1、AsciiStringCopySpec が3、DurationSpec が3 であり、DurationSpec の概数は、結果として実行件数と一致する。

### ケースごとの対応表

| 原典のケース | 移す先と Rust のテスト名、または区分の提案 | 理由 |
|:---|:---|:---|
| ByteStringInitializationSpec「not get confused by initializing CompactByteString before ByteString」 | **作らない区分を提案する**（§7） | 原典は、専用のクラスローダで Pekko のクラス群を読み直し、`CompactByteString.empty` を先に触ってから `ByteString.empty` を触り、クラスの静的初期化の順序で壊れないことを確かめる。Rust にはクラスローダも静的初期化も無く、fraktor-actor には `CompactByteString` という型自体が無い（`ByteString` が Pekko の3表現を1つの型で担う。§2）。検証する仕組みが存在しないため、等価なテストは書けない |
| AsciiStringCopySpec「select working algorithm」 | **作らない区分を提案する**（§7） | 原典は、`org.apache.pekko.util.Unsafe`（Java）の2つのアルゴリズム（`String.getBytes(US_ASCII)` を使う方式0と、`VarHandle` で `String.value` の `byte[]` を直接読む方式1）がどちらも動くことを確かめる。JDK の `String` の内部表現に依存する探りであり、Rust の `str` は UTF-8 で `as_bytes()` が内部のバイト列そのものを返すため、アルゴリズムの選択自体が無い |
| AsciiStringCopySpec「copy string internal representation successfully」 | **作らない区分を提案する**（§7）。却下された場合の移し先は `crates/fraktor-actor/tests/ascii_string_copy_spec.rs` の `copy_us_ascii_str_to_bytes_copies_the_ascii_bytes` | 原典の観測できる振る舞い（US-ASCII 文字列のバイト列が宛先の配列へそのまま写り、US-ASCII として読み戻すと元の文字列に等しい）は、`str::as_bytes()` と `copy_from_slice` で満たせる。ただしその形のテストが確かめるのは Rust 標準ライブラリであり、fraktor-actor のコードは通らない。`Unsafe.copyUSAsciiStrToBytes` の利用者は Pekko の remote（artery の `EnvelopeBufferPool`）だけであり、fraktor-actor に対応する利用者はいない。この最適化のためだけに公開 API を新しく足すことはしない |
| AsciiStringCopySpec「hash strings successfully」 | **作らない区分を提案する**（§7） | 原典は `Unsafe.fastHash("abcdefghijklmnopqrstuvxyz")` を呼ぶだけで、戻り値への検査は無い。確かめているのは「JDK の内部表現を読む分岐で落ちないこと」であり、Rust にはその分岐が無い。`fastHash` 自体（xorshift128+、初期状態 `s0 = 391408`、`s1 = 601258`、戻り値は `(int)(s0 + s1)`）の利用者は remote のキャッシュ（`ActorRefResolveCache`、`LruBoundedCache`）であり、fraktor には未移植である。remote のキャッシュを移植するときに JVM と一致するハッシュが必要になる点は、§7 の提案理由に残す |
| DurationSpec「form a one-dimensional vector field」 | `crates/fraktor-actor/tests/duration_spec.rs` の `duration_forms_a_one_dimensional_vector_field` へ移す | `core::time::Duration` は fraktor-actor の kernel 全体が使う時間の型であり（`Timeout` が包み、`Clock` の port が返す）、その加減乗除の契約は fraktor の契約である。原典の演算はすべて `core::time::Duration` の演算で表せる（§2） |
| DurationSpec「respect correct treatment of infinities」 | **除外を提案する**（§7、承認待ち） | `Duration.Inf`、`Duration.MinusInf`、`Duration.Undefined` とその代数（`MinusInf + Inf = Undefined` など）を検証する。`core::time::Duration` には無限と未定義の値が無く、fraktor のコードは「上限が無い」ことを、使う場所ごとの型で表している（`RestartWindow::duration()` の `Option<Duration>`、adaptor の ask の期限の `AskDeadline::BeyondClockRange` と `AskDeadline::Unspecified`）。今の fraktor-actor には、無限の代数を必要とする利用者はいない。等価には書けないため、作らない区分ではなく除外として提案する |
| DurationSpec「support fromNow」 | `crates/fraktor-actor/tests/duration_spec.rs` の `duration_from_now_time_left_decreases_when_a_manual_clock_advances` へ移す（適応あり） | 原典は `2.seconds.fromNow` が作る `Deadline` について、直後は残りが1秒より大きく、1秒眠った後は残りが1秒より小さいことを確かめる。issue #185 の注意は「原典は Thread.sleep を使う。実時間に頼らず決定的に検証する（待ちを弱めない）」であり、前例（#117、#119）も手動の時計を使う形である。移植は、公開の `fraktor_actor::kernel::Clock`（`monotonic() -> Duration`）を実装した手動の時計をテストの中に置き、期限を `clock.monotonic() + 2秒`、残りを `deadline.saturating_sub(clock.monotonic())` で求める。実時間では眠らない。適応は §2 に書く。原典は `2.seconds.fromNow` と `2.seconds(fromNow)` の2つの書き方で同じ値を作っているが、これは Scala の DSL の書き方が2つあるだけであり、Rust では1つの作り方になる |

DurationSpec は原典で `PekkoSpec` を継承し、台帳の `needs` に `extends-pekko-spec` と `pekko-spec`、`premise_flags` に `thread-sleep` が付いている。移植する2件はどちらも ActorSystem を使わず、TestKit の前提は要らない。第2段で台帳を更新するときは、`needs.testkit` を空にし、`premise_flags` も空にする（#119 と同じ扱いである。手動の時計を使うため、`thread-sleep` の前提は残らない）。

## 2. 対応する本体がすでにあるか

| 原典の対象 | fraktor-actor の現状 | この issue で足すもの |
|:---|:---|:---|
| `org.apache.pekko.util.ByteString` と `CompactByteString` | `crates/fraktor-actor/src/kernel/byte_string/byte_string_value.rs` の `ByteString` がすでにある。`ByteString::empty() -> Self`、`from_string(&str) -> Self`、`from_array(&[u8], usize, usize) -> Self`、`from_ints(&[i32]) -> Self`、`concat(&self, &ByteString) -> Self` などを持つ。型の文書に書かれているとおり、Pekko の `ByteString1C`、`ByteString1`、`ByteStrings` の3表現を、共有するバイト配列の列で1つの型が担う。`CompactByteString` に当たる型は無い | 無し |
| `org.apache.pekko.util.Unsafe`（Java、`@InternalApi`） | 対応する型も関数も無い。US-ASCII への変換は `ByteString::from_string` が UTF-8 の `as_bytes()` で行っており、US-ASCII 専用の写し関数の利用者は crate の中にいない | 無し |
| `scala.concurrent.duration.Duration`、`FiniteDuration`、`Deadline` | Pekko のコードではなく Scala 標準ライブラリである。fraktor-actor は `core::time::Duration` を時間の型として使い、`crates/fraktor-actor/src/kernel/timeout/timeout_value.rs` の `Timeout { duration: Duration }`（`Timeout::new(Duration)`、`duration() -> Duration`）が Pekko の `Timeout` に対応する。単調時刻は kernel の `Clock` の port（`monotonic() -> Duration`）が担い、adaptor の `SystemClock` が実装する。テストでは手動の時計が同じ port を実装する（`frequency_list_spec.rs`、`recency_list_spec.rs`、`restart_strategy_spec.rs` の `TestClock` と `ControlledClock` が前例である）。`Deadline` に当たる公開型は無い | 無し |

移植する DurationSpec の2件が使う演算と、`core::time::Duration` の対応は次のとおりである。どれも既存の演算であり、新しい公開 API は要らない。

| 原典 | Rust |
|:---|:---|
| `0.seconds`、`1.second`、`3 * one` などの構築と scalar 倍 | `Duration::ZERO`、`Duration::from_secs(1)`、`one * 3`（`Mul<u32>`） |
| `three - two`、`one + zero` | `three - two`、`one + zero`（`Add`、`Sub`） |
| `three / 3`、`one / 1000000`（結果は `1.micro`） | `three / 3`、`one / 1_000_000`（`Div<u32>`。1秒を100万で割ると1マイクロ秒ちょうどになる） |
| `two / one`（結果は `2d`、Duration 同士の比） | `two.div_duration_f64(one)`。`Duration` 同士の `Div` 実装は Rust に無い（`two / one` と書くと E0308 になる）。`div_duration_f64` は core の関数で、Rust 1.80 から stable であり、この repo のツールチェーン（stable）で使える |
| `2.seconds.fromNow`、`deadline.timeLeft` | 手動の時計 `clock` に対して `clock.monotonic() + Duration::from_secs(2)`、`deadline.saturating_sub(clock.monotonic())` |

原典と違える点は3つある。1つ目は `fromNow` の進め方である。手動の時計を1秒ちょうど進めると、残りは1秒ちょうどになり、原典の「1秒より小さい」が落ちる。原典で残りが厳密に1秒未満になるのは、sleep が1秒以上かかり、そのうえ検査の時間が足されるためである。移植は時計を「1秒 + 1ナノ秒」進め、最初の検査にかかった時間を最小の1刻みで表す。これで判定の向きと境界は原典のままになり、実時間の混み具合で落ちることも無くなる。2つ目は `timeLeft` の符号である。Scala の `Deadline.timeLeft` は期限を過ぎると負の `FiniteDuration` を返すが、移植の `saturating_sub` は期限の後で `Duration::ZERO` に張り付く。移植するケースの検査は「1秒より大きい」「1秒より小さい」の2つとも正の範囲にあり、この差には触れない。3つ目はあふれの扱いである。Scala の `FiniteDuration` の演算は範囲を超えると `IllegalArgumentException` を投げ、Rust の `Duration` の演算は panic する（検査つきの形は `checked_add` などが `None` を返す）。移植するケースは範囲内の値だけを使うので、この差にも触れない。どの差も、ケースを弱めたわけではない。

### no_std

本体を足さないので、kernel の no_std 性質は変わらない。移植するテストが使う `core::time::Duration` と `Duration::div_duration_f64` は core の型と関数であり、手動の時計は `Clock` の port（core の `Duration` だけを使う）を統合テスト（`crates/fraktor-actor/tests/`）の中で実装する。`std::time::Instant` と `std::thread::sleep` は使わない。

## 3. 守る性質（不変条件）

| 番号 | 性質 |
|:---|:---|
| I1 | `ByteString::empty()` は、プロセスの中で最初に触る操作が何であっても、長さ0の値を返す。Rust には初期化の順序という概念が無く、この性質は言語の意味論から直接に従う。原典の ByteStringInitializationSpec が守ろうとした性質はこれであり、作らない区分の代替例（§7）で形を示す |
| I2 | US-ASCII の文字列 `s` について、`s.as_bytes()` の長さは文字数に等しく、そのバイト列を US-ASCII として読み戻すと `s` に等しい。原典の AsciiStringCopySpec の写しのケースが守る性質はこれである |
| I3 | 有限の `Duration` について、原典のベクトル空間の等式が成り立つ。`0 * d = ZERO`、`2 * d = d + d`、`(3 * d) - (2 * d) = d`、`(3 * d) / 3 = d`、`d + ZERO = d`、`1秒 / 1_000_000 = 1マイクロ秒`。ただし等式は、演算が `Duration` の範囲（符号なし、最大で約5.8e11年）に収まる入力でのみ主張する。Duration 同士の比は、この限りではない。`div_duration_f64` は `f64` で計算するため、`(2 * d).div_duration_f64(d) = 2.0` は一般には成り立たない（実測の反例は `d = 14600049718.950967893秒` で、結果は `1.9999999999999998` になる）。この等式は、原典の固定値（`two` と `one`）に限って主張し、性質として残すときは「`2 * d` の総ナノ秒が 2^53 以下」の範囲を明記する。その範囲では総ナノ秒が `f64` で正確に表せる |
| I4 | 期限 `deadline = 時計の読み + d` の残り時間は、時計が進むにつれて単調に減る（増えない）。開始の直後は `d` に等しく、進めた時間の分だけ減る。原典の `fromNow` のケースは `d = 2秒`、境界1秒でこの性質を確かめる。移植は手動の時計を1秒 + 1ナノ秒進めるため、残りは決定的に `999.999999ミリ秒` になり、境界の向きは原典のままである |

## 4. 壊しうる入力

| 入力 | 何が起きうるか | どう扱うか |
|:---|:---|:---|
| 空文字列（Ascii の写しとハッシュ） | 写すバイトが0個になる。原典のアルゴリズムはどちらも長さ0でそのまま終わる | 作らない区分の代替例に空文字列を含める。移植に変わった場合も、空文字列のケースをテストに含める |
| US-ASCII でない文字列（Ascii の写し） | 原典の方式0（`getBytes(US_ASCII)`）は範囲外の文字を `?` に置き換え、方式1（内部の `byte[]` を直接読む）は UTF-16 の符号単位や Latin-1 の表現をそのまま写すため、方式によって結果が違う。原典のケースは ASCII の文字列だけを使い、この入力は契約の外にある | 移植しない。設計に契約外と明記するだけにし、ケースの追加はしない |
| 宛先の配列が文字列より短い（Ascii の写し） | 原典は `System.arraycopy` が範囲外で例外を投げる。Rust の `copy_from_slice` は長さが合わなければ panic する | 前提として「宛先の長さは文字列のバイト数以上」を代替例と、移植に変わった場合のテストの文書に書く |
| 極端に長い文字列 | 原典の方式1は `String` の内部配列の長さと文字数がずれる符号化（Latin-1 でない文字）で写す量を取り違える。Rust の `as_bytes()` は UTF-8 のバイト列をそのまま返すので、この種の取り違えは起きない | 扱いは I2 のとおり。専用のケースは足さない |
| 境界の時間の値（`Duration::ZERO`、`Duration::MAX` の近傍） | 加算と乗算は panic し、Scala の `IllegalArgumentException` とは失敗の形が違う。除算は、0で割ると panic する | 移植するケースは原典の値だけを使い、境界の値は PBT の入力に含めるが、等式の主張は `checked_*` で範囲に収まる入力に限る（§5） |
| 負の時間 | Scala の `FiniteDuration` は負を持てるが、`core::time::Duration` は符号が無い。原典の動くケースは、無限のケースを除いて負の値を使わない | 無限のケースは除外の提案に含める（§7）。有限のケースには影響しない |
| ワイルドカードの連続、空文字（Glob のような入力） | この3つの Spec には文字列のパターン照合が無いため、当てはまらない | 対象外と明記する |

## 5. PBT を足すか

足す。対象は DurationSpec のベクトル空間の性質（I3）だけであり、置き場所は `crates/fraktor-actor/tests/duration_property_test.rs` とする。原典外のテストなので、台帳の件数には含めない。外部の依存は足さず、`frequency_sketch_property_test.rs` と同じく固定 seed の SplitMix64 で入力を作る。

| 番号 | 性質 | 確かめる不変条件 |
|:---|:---|:---|
| P1 | 範囲に収まる2つの `Duration` について、`a + b = b + a` であり、`(a + b) - b = a` である。入力は `checked_add` が成功する組に限る | I3 |
| P2 | 範囲に収まる `Duration` と `u32` の係数について、`(d * n) / n = d`（`n >= 1`）であり、`d * 0 = ZERO` である。入力は `checked_mul` が成功する組に限る | I3 |
| P3 | 任意の `Duration` について、`d + ZERO = d`、`d - ZERO = d`、`d.div_duration_f64(d) = 1.0`（`d` が0でないとき）である。0でないどの `d` でもこの等式は成り立つ（実測で違いは0件だった）。`(2 * d).div_duration_f64(d) = 2.0` は、`2 * d` の総ナノ秒が 2^53 以下の入力に限って確かめる（I3 の反例が示すとおり、その範囲の外では成り立たない） | I3 |
| P4 | P1〜P3 が外した入力の中でも守る性質を確かめる。`ZERO.div_duration_f64(ZERO)` は NaN である（実測）。`a.checked_add(b)` が `None` になるのは、`b > Duration::MAX - a` のときに限る（同値である）。`d.checked_mul(n)` が `None` のとき、`d.saturating_mul(n) = Duration::MAX` である | I3 |

入力の作り方も決めておく。一様な `u64` の秒数だけでは、P2 の `checked_mul` がほとんど失敗し、有効な試行が少ないまま性質が通ったように見える。入力は、境界の値（`ZERO`、`Duration::MAX`、ナノ秒が `999_999_999` の値、ナノ秒の繰り上がりが起きる値）、小さな値、大きな値を混ぜて作る。検査は、有効な試行（`checked_*` が成功した試行）の数が下限（全試行の半分）を下回ったら失敗させる。

ByteString の初期化と Ascii の写しには PBT を足さない。どちらも作らない区分の提案であり、代替例は固定の小さな例で足りる。`fromNow` の性質（I4）は、手動の時計を進める量だけが入力であり、固定の手順（読む、1秒 + 1ナノ秒進める、読む）で足りるため、PBT の対象にしない。

## 6. Quint

Quint は要らない。3つの Spec はどれも状態遷移を持たず（純粋な値の演算、バイト列の写し、1回の時刻読みだけ）、確かめる性質は固定のテストと §5 の PBT で覆えるためである。

## 7. 台帳の見込みと、区分の提案

第2段で、承認を台帳へ写す。ゴールの識別子は、#177 の命名に倣って `G0issue185` とする。承認の欄（`approved_by` と `approved_at`）は、移植の規約により人間とレビューの決定を写す欄であり、この設計文書の作成者が自分で書くものではない。作らない区分と除外の最終的な承認は上司が取るため、この節の提案はすべて「承認待ち」である。独立レビューの推奨は §8 と §11 の回答表に書いた。

| ノード | 総数 | 移植 | 作らない区分の提案 | 除外の提案 | 承認後の状態の見込み |
|:---|---:|---:|---:|---:|:---|
| `actor-tests/src/test/scala/org/apache/pekko/util/ByteStringInitializationSpec.scala` | 1 | 0 | 1 | 0 | `not_built` |
| `actor-tests/src/test/scala/org/apache/pekko/util/AsciiStringCopySpec.scala` | 3 | 0 | 3 | 0 | `not_built` |
| `actor-tests/src/test/scala/org/apache/pekko/util/DurationSpec.scala` | 3 | 2 | 0 | 1 | `green` |

作らない区分の提案は次のとおりである。要素の形（`name`、`reason`、`alternative`、承認の欄）は `docs/decisions/ledger-schema-draft.md` の定義に従う。

| ケース | 理由 | 代替例 |
|:---|:---|:---|
| ByteStringInitializationSpec「not get confused by initializing CompactByteString before ByteString」 | クラスローダでクラス群を読み直し、Pekko の `ByteString` と `CompactByteString` の object の静的初期化が循環しないことを試す、JVM の言語と実行時のための補助そのものの検査である。Rust には静的初期化が無く、`CompactByteString` という型も無い。さらに、対応する `ByteString::empty()` は static も遅延初期化も持たず、`Self { arrays: Vec::new() }` を返すだけである（`crates/fraktor-actor/src/kernel/byte_string/byte_string_value.rs` の `empty`）。初期化の順序に当たるもの自体が無いため、作らない区分の条件（原典の言語のための補助そのものであること、その振る舞いを代替例で示せること、実行時の契約を自動では入れない但し書きへの答え）のすべてに当たる | `assert_eq!(ByteString::empty().iterator().count(), 0)` を、ByteString に触る最初の操作として実行しても同じ結果になる。順序を入れ替える対象の型が無いため、順序の指定自体が不要である |
| AsciiStringCopySpec「select working algorithm」 | JDK の `String` の内部表現（`value` の `byte[]`）へ `VarHandle` で触れる方式が動くかを試す、JVM 専用の探りである。Rust の `str` には内部表現の分岐が無い | `"abc".as_bytes() == b"abc"` であり、バイト列を得る方法は1つだけである |
| AsciiStringCopySpec「copy string internal representation successfully」 | 観測できる振る舞いは `str::as_bytes()` と `copy_from_slice` で満たせる。最適化の本体（`Unsafe`）は JVM の内部表現に依存し、fraktor に対応する利用者はいない | `let ascii = "abcdefghijklmnopqrstuvxyz"; let mut bytes = [0u8; 25]; bytes.copy_from_slice(ascii.as_bytes()); assert_eq!(std::str::from_utf8(&bytes), Ok(ascii))`。原典の文字列は `w` を含まない25文字であり、代替例もそのままの文字列を使う |
| AsciiStringCopySpec「hash strings successfully」 | 戻り値への検査が無く、観測できる振る舞いは「25文字の ASCII 文字列をハッシュすると、例外を出さずに値が返る」だけである。確かめている本体は JDK の内部表現を読む分岐で落ちないことであり、Rust にはその分岐が無い。`fastHash` の利用者は remote の未移植のキャッシュ（`ActorRefResolveCache`、`LruBoundedCache`）だけである | `let hasher = std::hash::BuildHasherDefault::<std::hash::DefaultHasher>::default(); let _value: u64 = std::hash::BuildHasher::hash_one(&hasher, "abcdefghijklmnopqrstuvxyz");`。標準のハッシュでも、同じ観測できる振る舞い（例外なく値が返る）が示せる。値は一致させない。`ActorRefResolveCache` のハッシュはプロセスの中のキャッシュのキーであり、通信には乗らないため、JVM と同じ値は要らない。JVM と同じ値が要るのは、ハッシュで決まる順序を見る remote の Spec を移すときだけであり、そのときは `Unsafe.fastHash`（xorshift128+、初期状態 391408 と 601258）と一致する実装が別途必要になる。置き場所の前例は、JVM の `String.hashCode` と一致する `kernel::frequency_sketch::StringHashCode` である |

除外の提案は次のとおりである。

| ケース | 理由 |
|:---|:---|
| DurationSpec「respect correct treatment of infinities」 | `core::time::Duration` には無限と未定義の値が無く、等価には書けない。今の fraktor-actor には、無限同士の演算（`Inf - Inf = Undefined` など）を必要とする利用者はいない。ただし、無限を扱う移植が後続に無いわけではない。#186 の PrettyDurationSpec には `Inf`、`MinusInf`、`Undefined` の3件があり、Pekko 本体でも FSM（`Duration.Inf` で timeout を解除する）、ReceiveTimeout（`Undefined` で解除する）、SupervisorStrategy（`withinTimeRange = Inf`）が無限を使っている。後続の移植は、汎用の無限の代数を持つ型を作らず、`RestartWindow` の `Option` や `AskDeadline` の enum のように、使う場所ごとの型で表す。理由は、Pekko の `Inf` と `Undefined` が「解除」と「上限なし」の目印として場所ごとに使われており、汎用の型を持ち込むと `Undefined` の性質（演算に混ざると結果も未定義になり、比較も全順序にならない）まで持ち込むことになるためである。後続への注意として、`Option<Duration>` を順序で比べると `None < Some(_)` になり、「無限は有限より大きい」とは逆になる。FSM と ReceiveTimeout を移すときは、`Option` を比較に使わず、専用の enum にする。無限の代数を持つ公開型を、このケースのためだけに新しく足すことはしない |

## 8. レビューで決めてほしい点

判断材料は次の4点である。独立レビュー（PR #228 の設計レビュー）は、1 について「3件は認める。ハッシュの1件は代替例を足してから認める」、2 について「認める。kernel に型は足さない」と推奨した。この版でハッシュの代替例を足した（§7）ため、残るのは上司による最終的な承認であり、提案は「承認待ち」である。

1. **作らない区分の提案（ByteStringInitializationSpec の1件と AsciiStringCopySpec の3件）を認めるか。** 認めれば、2つのノードの状態は green ではなく `not_built` になる。issue の文面の「green にする」とは名前が違うが、rollup は作らない区分を処理済みとして数える。認めない場合は、少なくとも AsciiStringCopySpec の写しのケースは §1 の移し先へ移植できる（その場合、そのノードは移植1件と作らない区分2件の混在で green になる）。ただし、混在の勘定は台帳のツールにまだ入っていない。`docs/decisions/ledger-schema-draft.md` によると、混在の勘定は `verify_red.py` と `verify_green.py` に未実装であり、「最初に混在のノードを扱うゴールで入れる」とされている。そのため、却下された場合は、台帳のツール2本への混在の勘定の追加と、その Python テストの追加も範囲に入る。規模の見込みは、ツールの変更が2ファイルとテストが2ファイルであり、本体の実装より小さく、第2段の検査時間に Python テストの実行が加わる程度である。
2. **DurationSpec の無限のケースを除外として認めるか。** 認めない場合は、無限と未定義を持つ時間の型を kernel に新しく足すことになり、公開 API の追加として範囲が大きく変わる。その場合は第2段へ進まず、型の設計を別途起こす必要がある。
3. **DurationSpec のコメントの中のケース（「check its range」）を総数に入れない扱いでよいか。** 入れる場合は総数が4件になり、そのケースは「Scala の `FiniteDuration` は範囲を超えると例外を投げ、Rust の `Duration` は panic するか `checked_*` が `None` を返す」という理由で、除外の提案がもう1件増える。
4. **DurationSpec を `core::time::Duration` のテストとして移植する形でよいか。** 対象は Pekko のコードではなく Rust の標準の型だが、この型は fraktor-actor の kernel 全体が使う時間の型であり、その演算の契約は fraktor の契約として台帳に残す価値がある、というのがこの設計の立場である。AsciiStringCopySpec の写しのケースを作らない区分とする根拠（標準ライブラリだけを確かめるテストにしない）と一見で食い違うため、この区別（`Duration` は fraktor が採用した型であり、US-ASCII の写し関数は fraktor のどこからも使われない）を妥当と見るかを確認してほしい。

## 9. 影響

- **公開 API**：追加は無い。既存の型、関数、モジュールの公開面は変えない。移植するテストは、既存の `ByteString`、`core::time::Duration`、公開の `Clock` の port だけを使う。
- **台帳**：対象の3ノード（ByteStringInitializationSpec、AsciiStringCopySpec、DurationSpec）の状態、件数、`verified_against: v2.0.0` を更新する。`docs/baseline/v2.0.0-undocumented-nodes.tsv` は3行増える見込みである。
- **台帳のツール**：変えない。ただし §8 の判断材料1で作らない区分の提案が却下され、移植と作らない区分が混在するノードが生じる場合は、`verify_red.py` と `verify_green.py` への混在の勘定の追加と、そのテストが範囲に加わる（§8）。
- **保存済みのデータと上流**：影響は無い。保存済みの形式は変えず、Pekko の上流へ送るものも無い。
- **守りの仕組み**：CI の必須チェックは変えない。第2段で、移植したテストと PBT が必須チェックの中で動くようになる。
- **実行時間**：`fromNow` のケースは手動の時計を使うため、実時間の待ちは増えない。原典が1件で1秒眠っていた時間は、移植では無くなる。

## 10. 第2段の進め方（設計レビューが通ってから）

1. `crates/fraktor-actor/tests/duration_spec.rs` に移植する2件を書き、台帳の DurationSpec を red にして、失敗を確認する（red）。`fromNow` の件は、手動の時計（`Clock` の実装）をテストの中に置き、実時間では眠らない。
2. テストが既存の型だけで通ることを確認し、足りないものがあれば実装ではなく設計への差し戻しとして報告する（green）。本体の追加は無い見込みである。
3. `crates/fraktor-actor/tests/duration_property_test.rs` に §5 の PBT を足す。
4. 台帳の3ノードを §7 の見込みどおりに更新し、`docs/baseline/v2.0.0-undocumented-nodes.tsv` を作り直す。台帳の DurationSpec は、`needs.testkit` を空にし、`premise_flags` も空にする。
5. CI と同じ検査を流す。`cargo fmt --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`、台帳の Python テスト（`ledger/tools`）である。Quint の仕様は無いので、Quint の検査は無い。
6. PR をドラフトから外し、必須のチェックが通る状態にする。マージはしない。

## 11. 設計レビュー（PR #228）への回答

独立レビュー（判定は「要修正」、高1件、中4件、低6件）への回答である。指摘の番号はレビューの番号であり、内容は1行で言い直した。実測が根拠の指摘は、この版の作成時に Mac mini の rustc 1.100.0 で再現を確認した（`div_duration_f64` の反例、0同士の比が NaN になること、手動の時計を1秒 + 1ナノ秒進めた残りが `999.999999ミリ秒` になること、ハッシュの代替例がコンパイルできて値が返ること）。

| 番号 | 指摘 | 回答 |
|:---|:---|:---|
| H1 | `fromNow` を実時間の sleep で移植する案は、issue の注意と前例（#117、#119、手動の Clock）に反し、CI の混み具合で落ちるテストになる | **直した。** `Clock` の port を実装する手動の時計に変え、期限は `clock.monotonic() + 2秒`、残りは `saturating_sub` で求める形にした。適応として、時計は1秒 + 1ナノ秒進める（§1、§2、§3 の I4）。`premise_flags` は空にする |
| M1 | `two / one`（`Div<Duration>`）は Rust に無く、E0308 になる | **直した。** `two.div_duration_f64(one)` に変えた（§2、§5 の P3） |
| M2 | I3 の `(2 * d) / d = 2.0` は大きな `d` で成り立たない（反例は `14600049718.950967893秒` で `1.9999999999999998`） | **直した。** 等式は原典の固定値に限り、性質として残すときは「`2 * d` の総ナノ秒が 2^53 以下」の範囲を明記した（§3 の I3、§5 の P3） |
| M3 | ハッシュのケースには代替例が無く、作らない区分の条件 (2) に合わない | **直した。** 標準の `DefaultHasher` で同じ観測できる振る舞い（例外なく値が返る）を示す代替例を足した。理由の「JVM と同じ値が要る」も、ハッシュで決まる順序を見る remote の Spec を移すときに限る、と直した（§7） |
| M4 | 作らない区分が却下されて混在のノードになると、台帳のツールへの混在の勘定の追加が範囲に入ることが書かれていない | **直した。** §8 の判断材料1に、却下時の道として `verify_red.py` と `verify_green.py` への混在の勘定の追加と、その Python テストが範囲に入ることと、規模の見込みを書いた |
| L1 | 除外した入力（`checked_*` が失敗する入力、`d = 0`）の中で守る性質が無い | **直した。** §5 に P4 を足した。0同士の比が NaN になること、`checked_add` が失敗する条件が同値であること、`checked_mul` が失敗したとき `saturating_mul` が `Duration::MAX` になることを確かめる |
| L2 | PBT の入力の分布が書かれておらず、有効な試行が少ないまま通ったように見える | **直した。** §5 に、境界の値と小さな値と大きな値を混ぜる作り方と、有効な試行数の下限を下回ったら失敗させることを書いた |
| L3 | 「`Duration::MAX` は adaptor の ask の期限切れの表現」が実物と違う。実物は `AskDeadline::BeyondClockRange` である | **直した。** §1 と §7 の記述を、範囲を超えた期限は `AskDeadline::BeyondClockRange`、期限なしは `AskDeadline::Unspecified` で表す、に変えた |
| L4 | 除外の理由が「無限の代数を必要とする利用者はいない」と言い切っているが、後続（#186 など）には無限を扱う移植がある | **直した。** 理由を「今の fraktor-actor には無い。後続の移植は、使う場所ごとの `Option` か enum で表す」に書き換え、#186 の PrettyDurationSpec と Pekko 本体の利用者を挙げた。`Option` を比較に使うと無限の大小が逆になる注意も足した（§7） |
| L5 | 「影響」の節が無い | **直した。** §9 に影響の節を足した。公開 API の追加は無し、台帳の3ノードと `v2.0.0-undocumented-nodes.tsv` の3行、台帳のツールは変えない（M4 の却下の道を除く） |
| L6 | ByteStringInitializationSpec を作らない区分にする理由が、条件 (1) と合っていない | **直した。** 理由に「`ByteString::empty()` は static も遅延初期化も持たず、`Vec::new()` だけである。初期化の順序に当たるものが無い」を足し、条件 (1) と (2) と但し書きのすべてに答える形にした（§7） |

2つの提案へのレビューの推奨には、どちらも従う。作らない区分の4件は、ハッシュの1件に代替例を足したうえで、3件とあわせて認める推奨であり、無限のケースの除外は、kernel に型を足さない形で認める推奨である。最終的な承認は上司が取るため、§7 の提案は「承認待ち」のままにする。採らなかった指摘は無い。
