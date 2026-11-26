# 設計：#185 util の3 Spec（ByteStringInitializationSpec、AsciiStringCopySpec、DurationSpec）

[Issue #185](https://github.com/j5ik2o/fraktor-ng-rs/issues/185) の設計の記録である。対象は Pekko v2.0.0（`9fbd2283b1cf701c90a1223bf621942c191db61f`）の actor-tests にある util の3つの Spec で、依存する issue は無い。この文書は第1段（設計だけ）であり、実装と台帳の更新は、設計レビューが通ってから第2段で行う。

結論を先に書く。**本体の新規追加は無い。** 3つの Spec が検証する対象は、fraktor-ng-rs のコードではなく、JVM と Scala 標準ライブラリの仕組み（クラスローダによるクラス初期化の順序、JDK の `String` の内部表現を読む最適化、`scala.concurrent.duration.Duration`）である。fraktor-actor の中で対応する役割を担うのは、既存の `ByteString` と、kernel 全体が使う `core::time::Duration` と、adaptor が期限に使う `std::time::Instant` であり、この3つで足りる。残るケースは、作らない区分（`not_built`）と除外（`excluded`）の提案として §7 にまとめ、レビューで決めてほしい点として §8 に挙げた。提案が承認されれば、台帳の3ノードのうち DurationSpec は green に、ByteStringInitializationSpec と AsciiStringCopySpec は全件が作らない区分になるため green ではなく `not_built` になる（§7）。issue の文面は「対象ノードを green にする」だが、作らない区分は rollup が処理済みとして数える区分であり、状態の名前が変わる点をレビューで確認してほしい。

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
| DurationSpec「respect correct treatment of infinities」 | **除外を提案する**（§7） | `Duration.Inf`、`Duration.MinusInf`、`Duration.Undefined` とその代数（`MinusInf + Inf = Undefined` など）を検証する。`core::time::Duration` には無限と未定義の値が無く、fraktor のコードは「上限が無い」ことを `Option<Duration>` の `None`（`RestartWindow::duration()` など）と `Duration::MAX`（adaptor の ask の期限切れの表現）で表しており、無限の代数を必要とする利用者はいない。等価には書けないため、作らない区分ではなく除外として提案する |
| DurationSpec「support fromNow」 | `crates/fraktor-actor/tests/duration_spec.rs` の `duration_from_now_time_left_decreases_across_a_sleep` へ移す（適応あり） | 原典は `2.seconds.fromNow` が作る `Deadline` について、直後は残りが1秒より大きく、1秒眠った後は残りが1秒より小さいことを確かめる。移植は adaptor が実際に期限を表す方法（`std::time::Instant` に `Duration` を足す）で同じ形にし、眠る時間と境界は原典のままにする。kernel に `Deadline` 型は足さない。適応の差は §2 に書く |

DurationSpec は原典で `PekkoSpec` を継承し、台帳の `needs` に `extends-pekko-spec` と `pekko-spec`、`premise_flags` に `thread-sleep` が付いている。移植する2件はどちらも ActorSystem を使わず、`fromNow` の件が `std::thread::sleep` を使うだけなので、TestKit の前提は要らない。第2段で台帳を更新するときは、`needs.testkit` を空にし、`premise_flags` の `thread-sleep` だけを残す。

## 2. 対応する本体がすでにあるか

| 原典の対象 | fraktor-actor の現状 | この issue で足すもの |
|:---|:---|:---|
| `org.apache.pekko.util.ByteString` と `CompactByteString` | `crates/fraktor-actor/src/kernel/byte_string/byte_string_value.rs` の `ByteString` がすでにある。`ByteString::empty() -> Self`、`from_string(&str) -> Self`、`from_array(&[u8], usize, usize) -> Self`、`from_ints(&[i32]) -> Self`、`concat(&self, &ByteString) -> Self` などを持つ。型の文書に書かれているとおり、Pekko の `ByteString1C`、`ByteString1`、`ByteStrings` の3表現を、共有するバイト配列の列で1つの型が担う。`CompactByteString` に当たる型は無い | 無し |
| `org.apache.pekko.util.Unsafe`（Java、`@InternalApi`） | 対応する型も関数も無い。US-ASCII への変換は `ByteString::from_string` が UTF-8 の `as_bytes()` で行っており、US-ASCII 専用の写し関数の利用者は crate の中にいない | 無し |
| `scala.concurrent.duration.Duration`、`FiniteDuration`、`Deadline` | Pekko のコードではなく Scala 標準ライブラリである。fraktor-actor は `core::time::Duration` を時間の型として使い、`crates/fraktor-actor/src/kernel/timeout/timeout_value.rs` の `Timeout { duration: Duration }`（`Timeout::new(Duration)`、`duration() -> Duration`）が Pekko の `Timeout` に対応する。期限は adaptor の `ask_future.rs` が `std::time::Instant::now().checked_add(timeout.duration())` で表す。`Deadline` に当たる公開型は無い | 無し |

移植する DurationSpec の2件が使う演算と、`core::time::Duration` の対応は次のとおりである。どれも既存の演算であり、新しい公開 API は要らない。

| 原典 | Rust |
|:---|:---|
| `0.seconds`、`1.second`、`3 * one` などの構築と scalar 倍 | `Duration::ZERO`、`Duration::from_secs(1)`、`one * 3`（`Mul<u32>`） |
| `three - two`、`one + zero` | `three - two`、`one + zero`（`Add`、`Sub`） |
| `three / 3`、`one / 1000000`（結果は `1.micro`） | `three / 3`、`one / 1_000_000`（`Div<u32>`。1秒を100万で割ると1マイクロ秒ちょうどになる） |
| `two / one`（結果は `2d`、Duration 同士の比） | `two / one`（`Div<Duration> for Duration` が `f64` を返す） |
| `2.seconds.fromNow`、`deadline.timeLeft` | `Instant::now() + Duration::from_secs(2)`、`deadline.saturating_duration_since(Instant::now())` |

原典と違える点は2つある。1つ目は `timeLeft` の符号である。Scala の `Deadline.timeLeft` は期限を過ぎると負の `FiniteDuration` を返すが、Rust の `Instant` の差は符号を持たないため、`saturating_duration_since` は期限の後で `Duration::ZERO` に張り付く。移植するケースの検査は「1秒より大きい」「1秒より小さい」の2つとも正の範囲にあり、この差には触れない。2つ目はあふれの扱いである。Scala の `FiniteDuration` の演算は範囲を超えると `IllegalArgumentException` を投げ、Rust の `Duration` の演算は panic する（検査つきの形は `checked_add` などが `None` を返す）。移植するケースは範囲内の値だけを使うので、この差にも触れない。どちらの差も、ケースを弱めたわけではない。

### no_std

本体を足さないので、kernel の no_std 性質は変わらない。移植するテストが使う `core::time::Duration` は core の型であり、`std::time::Instant` と `std::thread::sleep` は統合テスト（`crates/fraktor-actor/tests/`）の中だけで使う。統合テストは std で動くため、この使い方は既存の移植テストと同じである。

## 3. 守る性質（不変条件）

| 番号 | 性質 |
|:---|:---|
| I1 | `ByteString::empty()` は、プロセスの中で最初に触る操作が何であっても、長さ0の値を返す。Rust には初期化の順序という概念が無く、この性質は言語の意味論から直接に従う。原典の ByteStringInitializationSpec が守ろうとした性質はこれであり、作らない区分の代替例（§7）で形を示す |
| I2 | US-ASCII の文字列 `s` について、`s.as_bytes()` の長さは文字数に等しく、そのバイト列を US-ASCII として読み戻すと `s` に等しい。原典の AsciiStringCopySpec の写しのケースが守る性質はこれである |
| I3 | 有限の `Duration` について、原典のベクトル空間の等式が成り立つ。`0 * d = ZERO`、`2 * d = d + d`、`(3 * d) - (2 * d) = d`、`(3 * d) / 3 = d`、`(2 * d) / d = 2.0`、`d + ZERO = d`、`1秒 / 1_000_000 = 1マイクロ秒`。ただし等式は、演算が `Duration` の範囲（符号なし、最大で約5.8e11年）に収まる入力でのみ主張する |
| I4 | 期限 `deadline = 開始時刻 + d` の残り時間は、時刻が進むにつれて単調に減る（増えない）。開始の直後は `d` 以下で、眠った時間の分だけ減る。原典の `fromNow` のケースは `d = 2秒`、睡眠1秒、境界1秒でこの性質を確かめる |

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
| P3 | 任意の `Duration` について、`d + ZERO = d`、`d - ZERO = d`、`d / d = 1.0`（`d` が0でないとき）である | I3 |

ByteString の初期化と Ascii の写しには PBT を足さない。どちらも作らない区分の提案であり、代替例は固定の小さな例で足りる。`fromNow` の性質（I4）は実時間に依存するため、PBT の対象にしない。原典のケース自体が1秒の睡眠を含む実行時間の長いテストであり、回数を増やすと CI の時間が延びるだけである。

## 6. Quint

Quint は要らない。3つの Spec はどれも状態遷移を持たず（純粋な値の演算、バイト列の写し、1回の時刻読みだけ）、確かめる性質は固定のテストと §5 の PBT で覆えるためである。

## 7. 台帳の見込みと、区分の提案

第2段で、設計レビューの承認を台帳へ写す。ゴールの識別子は、#177 の命名に倣って `G0issue185` とする。承認の欄（`approved_by` と `approved_at`）は、移植の規約により人間とレビューの決定を写す欄であり、この設計文書の作成者が自分で書くものではない。

| ノード | 総数 | 移植 | 作らない区分の提案 | 除外の提案 | 承認後の状態の見込み |
|:---|---:|---:|---:|---:|:---|
| `actor-tests/src/test/scala/org/apache/pekko/util/ByteStringInitializationSpec.scala` | 1 | 0 | 1 | 0 | `not_built` |
| `actor-tests/src/test/scala/org/apache/pekko/util/AsciiStringCopySpec.scala` | 3 | 0 | 3 | 0 | `not_built` |
| `actor-tests/src/test/scala/org/apache/pekko/util/DurationSpec.scala` | 3 | 2 | 0 | 1 | `green` |

作らない区分の提案は次のとおりである。要素の形（`name`、`reason`、`alternative`、承認の欄）は `docs/decisions/ledger-schema-draft.md` の定義に従う。

| ケース | 理由 | 代替例 |
|:---|:---|:---|
| ByteStringInitializationSpec「not get confused by initializing CompactByteString before ByteString」 | クラスローダでクラス群を読み直し、静的初期化の順序を試す JVM 専用の補助そのものの検査である。Rust には静的初期化が無く、`CompactByteString` という型も無い | `assert_eq!(ByteString::empty().iterator().count(), 0)` を、ByteString に触る最初の操作として実行しても同じ結果になる。順序を入れ替える対象の型が無いため、順序の指定自体が不要である |
| AsciiStringCopySpec「select working algorithm」 | JDK の `String` の内部表現（`value` の `byte[]`）へ `VarHandle` で触れる方式が動くかを試す、JVM 専用の探りである。Rust の `str` には内部表現の分岐が無い | `"abc".as_bytes() == b"abc"` であり、バイト列を得る方法は1つだけである |
| AsciiStringCopySpec「copy string internal representation successfully」 | 観測できる振る舞いは `str::as_bytes()` と `copy_from_slice` で満たせる。最適化の本体（`Unsafe`）は JVM の内部表現に依存し、fraktor に対応する利用者はいない | `let ascii = "abcdefghijklmnopqrstuvxyz"; let mut bytes = [0u8; 25]; bytes.copy_from_slice(ascii.as_bytes()); assert_eq!(std::str::from_utf8(&bytes), Ok(ascii))`。原典の文字列は `w` を含まない25文字であり、代替例もそのままの文字列を使う |
| AsciiStringCopySpec「hash strings successfully」 | 戻り値への検査が無く、確かめているのは JDK の内部表現を読む分岐で落ちないことだけである。`fastHash` の利用者は remote の未移植のキャッシュだけである | 代替例は無い。remote のキャッシュを移植するときは、`Unsafe.fastHash`（xorshift128+、初期状態 391408 と 601258）と JVM で一致する実装と、その期待値のテストが別途必要になる。この義務はこの提案の理由に残し、今回の範囲では作らない |

除外の提案は次のとおりである。

| ケース | 理由 |
|:---|:---|
| DurationSpec「respect correct treatment of infinities」 | `core::time::Duration` には無限と未定義の値が無く、等価には書けない。fraktor のコードは上限の無い時間を `Option<Duration>` の `None` と `Duration::MAX` で表しており、無限同士の演算（`Inf - Inf = Undefined` など）を必要とする利用者はいない。無限の代数を持つ公開型を、このケースのためだけに新しく足すことはしない |

## 8. レビューで決めてほしい点

判断材料は次の4点である。どれも、この設計の作成者は決めず、設計レビューで決めてほしい。

1. **作らない区分の提案（ByteStringInitializationSpec の1件と AsciiStringCopySpec の3件）を認めるか。** 認めれば、2つのノードの状態は green ではなく `not_built` になる。issue の文面の「green にする」とは名前が違うが、rollup は作らない区分を処理済みとして数える。認めない場合は、少なくとも AsciiStringCopySpec の写しのケースは §1 の移し先へ移植できる（その場合、そのノードは移植1件と作らない区分2件の混在で green になる）。
2. **DurationSpec の無限のケースを除外として認めるか。** 認めない場合は、無限と未定義を持つ時間の型を kernel に新しく足すことになり、公開 API の追加として範囲が大きく変わる。その場合は第2段へ進まず、型の設計を別途起こす必要がある。
3. **DurationSpec のコメントの中のケース（「check its range」）を総数に入れない扱いでよいか。** 入れる場合は総数が4件になり、そのケースは「Scala の `FiniteDuration` は範囲を超えると例外を投げ、Rust の `Duration` は panic するか `checked_*` が `None` を返す」という理由で、除外の提案がもう1件増える。
4. **DurationSpec を `core::time::Duration` のテストとして移植する形でよいか。** 対象は Pekko のコードではなく Rust の標準の型だが、この型は fraktor-actor の kernel 全体が使う時間の型であり、その演算の契約は fraktor の契約として台帳に残す価値がある、というのがこの設計の立場である。AsciiStringCopySpec の写しのケースを作らない区分とする根拠（標準ライブラリだけを確かめるテストにしない）と一見で食い違うため、この区別（`Duration` は fraktor が採用した型であり、US-ASCII の写し関数は fraktor のどこからも使われない）を妥当と見るかを確認してほしい。

## 9. 第2段の進め方（設計レビューが通ってから）

1. `crates/fraktor-actor/tests/duration_spec.rs` に移植する2件を書き、台帳の DurationSpec を red にして、失敗を確認する（red）。
2. テストが既存の型だけで通ることを確認し、足りないものがあれば実装ではなく設計への差し戻しとして報告する（green）。本体の追加は無い見込みである。
3. `crates/fraktor-actor/tests/duration_property_test.rs` に §5 の PBT を足す。
4. 台帳の3ノードを §7 の見込みどおりに更新し、`docs/baseline/v2.0.0-undocumented-nodes.tsv` を作り直す。
5. CI と同じ検査を流す。`cargo fmt --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`、台帳の Python テスト（`ledger/tools`）である。Quint の仕様は無いので、Quint の検査は無い。
6. PR をドラフトから外し、必須のチェックが通る状態にする。マージはしない。
