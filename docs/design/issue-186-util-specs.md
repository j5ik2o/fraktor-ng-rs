# 設計：#186 util の PrettyDurationSpec／GlobSpec／SwitchSpec

[Issue #186](https://github.com/j5ik2o/fraktor-ng-rs/issues/186) の設計の記録である。第1段はこの設計だけであり、実装は設計の独立レビューが通ってから別の担当が行う。この文書がレビュー対象である。

初版（`7befa66`）への独立レビュー（2026-11-26、判定は要修正、高2件・中5件・低4件）を受けて全文を直した。初版は書き込みの途中で本文が切れ、§4以降が欠けていた（レビューの H1）。この版は全文を入れ直し、各指摘への対応を §7 の表にまとめた。

原典は Pekko v2.0.0（`9fbd2283`）の `actor-tests/src/test/scala/org/apache/pekko/util/` の3つの Spec と、それが使う本体（`PrettyDuration` は `actor/src/main/scala/org/apache/pekko/util/PrettyDuration.scala`、`Glob` は同 `Glob.scala`、`Switch` は同 `LockUtil.scala` の中の `class Switch`、`Glob` の照合相手である `Helpers.makePattern` は同 `Helpers.scala`）である。移植の規約は `.takt/facets/knowledge/fraktor-porting-rules.md` と `.takt/workflows/rules/fraktor-coding-rules.md`、形式の前例は [PR #205](https://github.com/j5ik2o/fraktor-ng-rs/pull/205) の [issue-177-frequency-sketch.md](issue-177-frequency-sketch.md) と `docs/goals/issue-177-frequency-sketch.md` である。

## 0. 件数の数え直し（目安との差）

目安と実数が違う。PrettyDurationSpec の目安は issue 本文では4件、台帳 `ledger/nodes.yaml` の `test_cases_approx` では3件だが、実数は **13件** である。理由は、原典の `cases.foreach` が10件の表（期間と期待文字列の組）を1件ずつテストとして展開するためである。

| ノード | 目安（issue／台帳） | 実数 |
|:---|:---|:---|
| `actor-tests/src/test/scala/org/apache/pekko/util/PrettyDurationSpec.scala` | 4／3 | **13**（表10件＋特殊値3件） |
| `actor-tests/src/test/scala/org/apache/pekko/util/GlobSpec.scala` | 6／6 | **6** |
| `actor-tests/src/test/scala/org/apache/pekko/util/SwitchSpec.scala` | 6／6 | **6** |
| 合計 | 16 | **25** |

台帳へは第2段で `cases.total` にこの実数（13／6／6）を記録する。`test_cases_approx` は前例どおり変えない。レビューでも件数と対応表の抜け・重複が無いことを確認済みである。

## 1. 何を移植するか（ケース対応表）

移植先はすべて `crates/fraktor-actor` で、統合テストはノード1件＝ファイル1件で `tests/` に置き、ファイル冒頭に原典のパスをコメントで残す。作らないケースは無い。全25件を移す。

### 1.1 PrettyDurationSpec → `crates/fraktor-actor/tests/pretty_duration_spec.rs`（13件）

原典の表10件は `cases.foreach` で生成される。原典のケース名は `s"print $d nanos as $expectedValue"` という補間文字列であり、展開すると例として `print 9 nanoseconds nanos as 9.000 ns` の形になる。下の表の「原典のケース」はその言い換えである。移植では1件ずつ名前付きのテスト関数にする。

| # | 原典のケース（言い換え） | 移植先のテスト名 | 扱い |
|:---:|:---|:---|:---|
| 1 | 9 ns を `9.000 ns` と出す | `pretty_duration_prints_9_nanos` | 移す |
| 2 | 95 ns を `95.00 ns` と出す | `pretty_duration_prints_95_nanos` | 移す |
| 3 | 999 ns を `999.0 ns` と出す | `pretty_duration_prints_999_nanos` | 移す |
| 4 | 1000 ns を `1.000 μs` と出す | `pretty_duration_prints_1000_nanos_as_micros` | 移す |
| 5 | 9500 ns を `9.500 μs` と出す | `pretty_duration_prints_9500_nanos` | 移す |
| 6 | 9500 μs を `9.500 ms` と出す | `pretty_duration_prints_9500_micros_as_millis` | 移す |
| 7 | 9500 ms を `9.500 s` と出す | `pretty_duration_prints_9500_millis_as_seconds` | 移す |
| 8 | 95 s を `1.583 min` と出す | `pretty_duration_prints_95_seconds_as_minutes` | 移す |
| 9 | 95 min を `1.583 h` と出す | `pretty_duration_prints_95_minutes_as_hours` | 移す |
| 10 | 95 h を `3.958 d` と出す | `pretty_duration_prints_95_hours_as_days` | 移す |
| 11 | infinity（`Duration.Inf.pretty` が `infinity` を含む） | `pretty_duration_works_with_infinity` | 移す（入力は §2.1 の `PrettyDurationInput::Infinite`） |
| 12 | -infinity（`minus infinity` を含む） | `pretty_duration_works_with_minus_infinity` | 移す（同 `MinusInfinite`） |
| 13 | undefined（`undefined` を含む） | `pretty_duration_works_with_undefined` | 移す（同 `Undefined`） |

原典の検査は、表10件が完全一致、特殊値3件が部分一致である。移植でもその強さを変えない。これに加え、§2.1 の `%g` の算法を固定するため、原典の外の固定ケース（レビューの M1 の表の値）を統合テスト `tests/pretty_duration_format_test.rs` に置く。整形の部品は `pub(crate)` なので、公開の `pretty_with` を通して確かめる。

### 1.2 GlobSpec → `crates/fraktor-actor/tests/glob_spec.rs`（6件）

| # | 原典のケース | 移植先のテスト名 | 扱い |
|:---:|:---|:---|:---|
| 1 | match the literal cases（5組） | `glob_matches_literal_cases` | 移す |
| 2 | treat ? as exactly one character（4組） | `glob_treats_question_mark_as_exactly_one_character` | 移す |
| 3 | treat * as any run of characters（7組） | `glob_treats_star_as_any_run_of_characters` | 移す |
| 4 | agree with the regular expression it replaces, exhaustively over short inputs | `glob_agrees_with_reference_matcher_exhaustively_over_short_inputs` | 移す（照合相手の定義は §2.2 と §3 の G1） |
| 5 | match a pattern that makes the regular expression backtrack, promptly（パターン26文字、入力 `a`×36、制限3秒、結果は false） | `glob_matches_backtracking_pattern_promptly` | 移す |
| 6 | stay prompt as the input grows（パターン `*a`×20＋`*b`、入力 `a`×2000、制限5秒、結果は false） | `glob_stays_prompt_as_input_grows` | 移す |

ケース4の数え方：原典はアルファベット `{a, b, *, ?}` の長さ0〜4のパターン全341件（`patterns.size should be > 300` で検査）と、アルファベット `{a, b}` の長さ0〜4の入力全31件の、全組（341×31＝10,571組）を照合する。移植でも同じ生成と、パターン数が300を超えることの検査を残す。

ケース5と6の時間制限：原典は ScalaTest の `failAfter` である。移植では結果（false）の検査を本体とし、経過時間が原典の制限（3秒／5秒）未満であることも `std::time::Instant` で検査する。待ち（sleep）による検証はしない。

### 1.3 SwitchSpec → `crates/fraktor-actor/tests/switch_spec.rs`（6件）

原典の `s.switchOn(())` と `s.switchOff(())` は、Scala の解決では1引数の action つき版（`transcend`）である。原典の全ケースは引数なし版を一度も呼ばない（レビューの L1）。そのため移植のケースでは、状態の切り替えをすべて action つきの `switch_on_with(|| Ok(()))`／`switch_off_with(|| Ok(()))` で写し、引数なしの `switch_on()`／`switch_off()` の検査は原典の外の単体テスト（`switch_value_test.rs`）に置く。

| # | 原典のケース | 移植先のテスト名 | 扱い |
|:---:|:---|:---|:---|
| 1 | on and off | `switch_turns_on_and_off` | 移す（切り替えは action つき版で写す） |
| 2 | revert when exception | `switch_reverts_when_action_fails` | 移す。例外の代わりに `Err` を返すクロージャで検証する（§5 の決定1）。panic のときに状態が戻ることは原典の外のテストで確かめる（§3 の S3） |
| 3 | run action without locking（`ifOffYield`／`ifOnYield`／`ifOff`／`ifOn`） | `switch_runs_action_without_locking` | 移す |
| 4 | run action with locking（`whileOffYield`／`whileOnYield`／`whileOff`／`whileOn`） | `switch_runs_action_with_locking` | 移す |
| 5 | run first or second action depending on state（`fold`） | `switch_folds_by_state` | 移す |
| 6 | do proper locking | `switch_locked_allows_reentry_and_then_other_thread_changes` | 移す。中身は下の段落のとおり読み直した |

ケース6の読み直し（レビューの H2）：原典では、別スレッドは `locked { ... }` が戻ってから `start()` される。保持している間に別スレッドを待たせる場面は無い。`Thread.sleep(500)` は、ほかにスレッドが居ない `locked` の中で走るため結果に効かない。原典が実際に確かめているのは次の2つである。(1) `locked` の中から、ロックを取る `switchOn(())`（transcend）を呼べること、つまり再入ができることと、そのあと `isOn` であること。(2) その後、別スレッドの `switchOff(())` が完了し、`isOff` であること。移植はこの2つを確かめる。`Thread.sleep(500)` は消し、`CountDownLatch.await(5s)` は `JoinHandle::join()` に置き換える（§5 の決定2）。「`locked` の保持中はほかのスレッドの変更が完了しない」という性質（S5）は原典のケース6の内容ではないため、原典の外のテストと Quint で別に確かめる（§3 の S5、§4）。

## 2. 本体の有無と、新しく足す公開 API

3つの本体はどれも fraktor-ng-rs に無い。レビューでも `crates/fraktor-actor/src` を再検索し、重なりが無いことを確認済みである。したがって3つとも新規に足す。公開 API は、このリポジトリの決まり（`.takt/workflows/rules/fraktor-coding-rules.md`：振る舞いは型の `impl` に置くこと、ドメインの値は専用の型にすること）に従い、自由関数と生の基本型で組み立てない（レビューの M3）。置き場所は既存の平らな構成に合わせて `kernel` の直下に3モジュールを作る。既存の公開 API には触れない（追加だけ）。

| ファイル | 中身 |
|:---|:---|
| `crates/fraktor-actor/src/kernel.rs` | `pub mod glob;`、`pub mod pretty_duration;`、`pub mod switch;` の3行 |
| `crates/fraktor-actor/src/kernel/glob.rs` | 宣言と再エクスポート |
| `crates/fraktor-actor/src/kernel/glob/glob_pattern.rs` | `GlobPattern` |
| `crates/fraktor-actor/src/kernel/pretty_duration.rs` | 宣言と再エクスポート |
| `crates/fraktor-actor/src/kernel/pretty_duration/pretty_duration_input.rs` | `PrettyDurationInput` とその `impl` |
| `crates/fraktor-actor/src/kernel/pretty_duration/pretty_duration_format.rs` | `%g` 形式の整形（`pub(crate)` の補助） |
| `crates/fraktor-actor/src/kernel/switch.rs` | 宣言と再エクスポート |
| `crates/fraktor-actor/src/kernel/switch/switch_value.rs` | `Switch` |
| `crates/fraktor-actor/src/kernel/switch/locked_switch.rs` | `LockedSwitch`（ロック保持中の窓口） |
| `crates/fraktor-actor/src/kernel/switch/revert_guard.rs` | 状態を戻す drop ガード（`pub(crate)`） |
| `crates/fraktor-actor/src/kernel/switch/switch_value_test.rs` | 原典の外の単体テスト（引数なし版、panic のときの戻し以外の kernel 内で確かめる分） |
| `crates/fraktor-actor/tests/pretty_duration_spec.rs` | 原典13件 |
| `crates/fraktor-actor/tests/pretty_duration_format_test.rs` | 原典の外（M1 の固定ケース、i64 の範囲の外の1件） |
| `crates/fraktor-actor/tests/pretty_duration_property_test.rs` | PBT（§3、原典外） |
| `crates/fraktor-actor/tests/glob_spec.rs` | 原典6件 |
| `crates/fraktor-actor/tests/glob_property_test.rs` | PBT（§3、原典外） |
| `crates/fraktor-actor/tests/switch_spec.rs` | 原典6件 |
| `crates/fraktor-actor/tests/switch_model_test.rs` | PBT／モデル照合（§3、原典外） |
| `crates/fraktor-actor/tests/switch_extra_test.rs` | 原典の外（S5、panic のときの戻し） |
| `crates/fraktor-actor/specs/switch.qnt` | Quint（§4） |
| `ledger/nodes.yaml` | 対象3ノードを green に（第2段） |
| `docs/goals/issue-186-util-specs.md` | ゴール文書（第2段で作る） |

### 2.1 PrettyDuration

原典の入力は Scala の `Duration` で、有限値のほかに `Inf`、`MinusInf`、`Undefined` を取る。Rust の `core::time::Duration` は有限の非負値しか表せないため、入力の型を足し、振る舞いはその型の `impl` に置く（レビューの M3、推奨3）。

```rust
// kernel::pretty_duration
pub enum PrettyDurationInput {
  Finite(core::time::Duration),
  Infinite,
  MinusInfinite,
  Undefined,
}

impl PrettyDurationInput {
  /// 原典の `pretty`（`include_nanos = false`、精度4）
  pub fn pretty(&self) -> alloc::string::String;
  /// 原典の `pretty(includeNanos, precision)`。精度は0を取り得ない型で受ける
  pub fn pretty_with(&self, include_nanos: bool, precision: core::num::NonZeroUsize)
    -> alloc::string::String;
}
```

- 原典の `require(precision > 0)` は、精度を `NonZeroUsize` で受けることで型で閉じる。誤りの型（初版の `PrettyDurationError`）は置かない（レビューの L2 もこれで解消する）。検査は原典どおり分岐の前に当たるが、型で表すため実行時の分岐自体が要らない。
- 出力は原典どおり：有限値は `%.<precision>g <単位>` の形で、単位は `ns`、`μs`、`ms`、`s`、`min`、`h`、`d`。`include_nanos` が真なら末尾に ` (<nanos> ns)` を足す。特殊値は `∞ (infinity)`、`-∞ (minus infinity)`、`undefined`。
- 単位の選択は原典の `chooseUnit` の写しである：期間を日、時、分、秒、ミリ秒、マイクロ秒へ変換し、最初に0を超えた単位を選ぶ。どれも0なら `ns`。ただし変換は次の M2 の決まりに従い `u128` で行う。
- 飽和させない（レビューの M2）：初版が書いた「`FiniteDuration.toNanos` は飽和する」という前提は誤りだった。Scala の `FiniteDuration` は作る時点で ±(2^63−1) ns に収まることを `require` で確かめ、`toNanos` は飽和しない。一方 `core::time::Duration` は約1.8e28 ns まで表せるため、原典に無い範囲が入力に入る。移植では `as_nanos()` の `u128` のまま単位を選び、表示値の計算で `f64` に直す。i64 の範囲の中では、原典の `Long` から `Double` への変換と同じ値になる。範囲の外は原典に無い拡張として扱い、`Duration::from_secs(1_000_000_000_000)`（約1.157e7日）で表示が黙って誤らないことを原典の外の統合テスト1件で固定する。この入力の期待値 `1.157e+07 d` は原典の範囲の外であり Java では作れないため、算法から出る値である。`include_nanos` の末尾の値も `u128` をそのまま出す。
- `%g` の算法を書き切る（レビューの M1）：Java の `Formatter` の `%g` は、`Double.toString` の最短の10進の桁に HALF_UP の丸めをかける。f64 の正確な2進の値で丸める素直な実装（Rust の `{:.3}` など）とは結果が違う。例として、1,000,500 ns は Pekko では `1.001 ms` だが、正確な値で丸めると `1.000 ms` になる。移植の算法は次の順である。(1) core の精度なし `{:e}` で最短の10進の桁を得る。(2) その桁列に HALF_UP で精度桁数への丸めをかける。(3) 10進形式か指数形式かの選択は、丸めた後の指数で行う（丸めで桁が繰り上がると指数が変わるため）。(4) 指数形式の指数は Java の形（`e+04` のように符号つき2桁以上）にする。固定ケースとして、レビューが JDK 26 の `String.format(Locale.ROOT, ...)` で実測した次の値を原典の外のテストに置く（期待値の出どころは JDK 26 であり、古い JDK で `Formatter` の桁の作り方が違う可能性は試した範囲では見ていない）：1,000,500 ns → `1.001 ms`、1,002,500 ns → `1.003 ms`、1,100,000 ns を精度20で → `1.1000000000000000000 ms`、9,999,500 ns → `10.00 ms`（単位は ms のまま）、59,999,999,999 ns → `60.00 s`、864,000,000,000,000,000 ns → `1.000e+04 d`、95 ns を精度1で → `1e+02 ns`。
- 負の有限期間：Scala の `FiniteDuration` は負を取れるが、`core::time::Duration` は取れない。原典のケースに負の有限値はないため、これはケースの除外ではなく API の範囲の差であり、入力型が負を受け付けないことで設計上閉じる。記録として、原典では負の有限値は単位の選択がすべて0以下になるため常に ns で出る（例：-5000 ns なら `-5000 ns`）。

no_std：整形は `alloc::string::String` と core の `{:e}` だけで書ける。std は使わない。

### 2.2 Glob

パターンは専用の型にし、振る舞いはその `impl` に置く（レビューの M3）。

```rust
// kernel::glob
pub struct GlobPattern { /* パターンの文字の列 */ }

impl GlobPattern {
  /// 失敗しない。どの文字列もパターンとして受け付ける
  pub fn new(pattern: &str) -> Self;
  pub fn matches(&self, input: &str) -> bool;
}
```

- 原典の `Glob.matches(pattern, input)` の写しである。原典は呼び出しのたびにパターンを走査する。`GlobPattern` に文字の列を持たせると、同じパターンでの繰り返し照合で走査の準備を使い回せるが、照合の意味は変えない。原典の利用箇所（actor selection のパターン）は同じパターンを繰り返し使うため、この形が合う。
- アルゴリズムは原典と同じ貪欲な1本走査＋直近の `*` への巻き戻しである：ポインタ `p`（パターン）と `i`（入力）、直近の `*` の位置と、その `*` が消費した入力の位置を持ち、`?` か一致で両方進め、`*` で巻き戻し点を記録して0文字消費から始め、不一致で `*` の消費を1つ増やして再試行する。末尾の `*` は読み飛ばし、パターンを最後まで消費したら一致とする。正規表現は使わない。照合中の確保はしない。そのために、入力の巻き戻し位置は文字の位置ではなく `str::char_indices` が返すバイト位置で持ち、入力を `Vec<char>` へ集めることはしない（パターン側も、構築時に一度だけバイト位置と文字の組へ変換し、照合では確保しない）。
- ケース4の照合相手の定義を正す（レビューの M4）：参照照合は「glob の意味そのもの」として定義する。すなわち、`?` は任意の1文字、`*` は任意の文字の並び、それ以外はその文字自身に一致し、全体が一致するときに真になる、という意味をテスト側に再帰で直接書いた関数である。`Helpers.makePattern` はこの意味の近似であり、完全に一致するのは、行の終わりの文字（`\n`、`\r`、`\u{85}`、`\u{2028}`、`\u{2029}`）と `\` を含まないパターンと入力の範囲だけである。原典どうしの間でも、makePattern の `.` は行の終わりの文字に一致せず、サロゲートペアには符号点1つとして一致し、パターンに `\E` があると例外になるため、「すべての入力で makePattern と一致する」という性質は成り立たない。原典のケース4が `{a, b}` と `{a, b, *, ?}` の範囲で成り立つのは、その範囲が行の終わりの文字と `\` を含まないためである。
- 文字の単位：原典の `Glob` は `charAt`、すなわち UTF-16 の符号単位で数え、`?` は符号単位1つに一致する。移植は Rust の `char`（Unicode スカラ値）で数える。差が出るのは BMP の外の文字であり、原典では `?` がその半分（サロゲート1つ）に一致しうるのに対し、移植では文字全体に一致する。原典のケースはすべて ASCII であり一致する。この差は §3 の PBT で固定する。

no_std：`&str` の `chars` と `alloc::vec::Vec` だけで書ける。

### 2.3 Switch

原典は `AtomicBoolean` と、そのオブジェクト自身のモニタ（`synchronized`）で作られている。移植は状態を `core::sync::atomic::AtomicBool` の1か所だけに置き、モニタに当たる排他は `spin::Mutex<()>`（ガードだけを持つ）で写す（レビューの推奨2。状態を `Mutex<bool>` に移す案は、ロックを取らない `is_on`／`if_on` のために atomic の写しが要り、状態が2か所になるため採らない）。`spin` は既存の依存であり、`kernel::event::event_stream_bus` がすでに kernel で `spin::Mutex` を使っている。

```rust
// kernel::switch
pub struct Switch {
  on: core::sync::atomic::AtomicBool,
  lock: spin::Mutex<()>,
}

impl Switch {
  pub fn new(start_as_on: bool) -> Self;
  pub fn is_on(&self) -> bool;
  pub fn is_off(&self) -> bool;

  // ロックを取る切り替え（原典の引数なし switchOn / switchOff）
  pub fn switch_on(&self) -> bool;
  pub fn switch_off(&self) -> bool;

  // ロックの下で action を実行する切り替え（原典の switchOn(action) / switchOff(action)）
  pub fn switch_on_with<E>(&self, action: impl FnOnce() -> Result<(), E>) -> Result<bool, E>;
  pub fn switch_off_with<E>(&self, action: impl FnOnce() -> Result<(), E>) -> Result<bool, E>;

  // ロックなしの即時実行（原典の ifOn / ifOff / ifOnYield / ifOffYield）
  pub fn if_on(&self, action: impl FnOnce()) -> bool;
  pub fn if_off(&self, action: impl FnOnce()) -> bool;
  pub fn if_on_yield<T>(&self, action: impl FnOnce() -> T) -> Option<T>;
  pub fn if_off_yield<T>(&self, action: impl FnOnce() -> T) -> Option<T>;

  // ロックを取ってからの実行（原典の whileOn / whileOff / whileOnYield / whileOffYield）
  pub fn while_on(&self, action: impl FnOnce()) -> bool;
  pub fn while_off(&self, action: impl FnOnce()) -> bool;
  pub fn while_on_yield<T>(&self, action: impl FnOnce() -> T) -> Option<T>;
  pub fn while_off_yield<T>(&self, action: impl FnOnce() -> T) -> Option<T>;

  pub fn fold<T>(&self, on: impl FnOnce() -> T, off: impl FnOnce() -> T) -> T;

  // ロックを保持している間の窓口を渡す（原典の locked）
  pub fn locked<T>(&self, code: impl FnOnce(&LockedSwitch<'_>) -> T) -> T;
}

// ロックを取り直さずに状態を読み書きする窓口。locked の中での再入はこれで受ける
pub struct LockedSwitch<'a> { /* Switch への参照 */ }

impl LockedSwitch<'_> {
  pub fn is_on(&self) -> bool;
  pub fn is_off(&self) -> bool;
  pub fn switch_on(&self) -> bool;
  pub fn switch_off(&self) -> bool;
  pub fn switch_on_with<E>(&self, action: impl FnOnce() -> Result<(), E>) -> Result<bool, E>;
  pub fn switch_off_with<E>(&self, action: impl FnOnce() -> Result<(), E>) -> Result<bool, E>;
}
```

- 戻り値の `bool` は原典どおり「実際に状態が移った／action が実行されたか」である。
- 状態を戻す仕組み（レビューの M5）：action つきの切り替えは、状態を移した直後に drop ガード（`revert_guard.rs`）を置く。ガードは、action が成功して切り替えが確定した時点で解除され、それ以外の経路（`Err` の返却、panic による unwind）では drop のときに状態を元の値へ戻す。戻しはロックを持っている間に起きなければならないため、ガードはロックのガードより後に宣言し、ロックのガードより先に drop される寿命にする。戻し方そのものは原典と同じ `compare_exchange(!from, from)` とする（ロックの下では store と同じ結果になるが、原典の形を残す）。drop ガードは core だけで書けるため no_std を保てる。原典は `Throwable` をすべて捕まえて戻すため、panic の経路も覆う必要がある。panic のときに状態が戻ることは、原典の外のテスト（`switch_extra_test.rs`、統合テストは std なので `catch_unwind` が使える）で1件確かめる。
- 再入（レビューの H2 と推奨2）：原典のモニタは再入可能だが `spin::Mutex` は再入可能ではない。`locked` の中から `self` のロックを取る操作（`switch_on` など）を呼ぶと止まる。そこで `locked` は、ロックを取り直さずに使える窓口 `LockedSwitch` をクロージャへ渡し、原典のケース6がしている再入はこの窓口で受ける。窓口経由でない再入が止まることは rustdoc に明記する。no_std では持ち主のスレッドを識別できないため、型で完全には防げないことも rustdoc に書く。
- 原典の action は名前渡しであり、状態が移らないときは評価されない。移植の `FnOnce` も同じであり、移らないときは呼ばない。

no_std：`AtomicBool` は core、`spin::Mutex` は no_std 対応の既存依存、drop ガードは core の `Drop` だけであり、kernel に置ける。ケース6の検証自体はスレッドを使うため統合テスト（std）側で行う。

## 3. 守る性質（不変条件）と壊しうる入力、PBT

### PrettyDuration

| 番号 | 性質 |
|:---|:---|
| D1 | 単位は単調である：期間が増えて単位が大きな単位へ移ることはあっても、小さな単位へ戻らない |
| D2 | 表示は §2.1 の算法（最短の10進の桁への HALF_UP の丸め）で決まる。D2 は誤差の範囲では表さない。丸めの方式の差は誤差の範囲では拾えないため、算法そのものと M1 の固定ケースで固定する |
| D3 | 特殊値の出力は入力だけで決まり、`∞ (infinity)`、`-∞ (minus infinity)`、`undefined` の語を含む |
| D4 | 精度が0であることは型で表せない（`NonZeroUsize`）。実行時の検査と誤りの値は無い |

壊しうる入力：0ナノ秒、単位の境界の前後（999／1000 ns など各換算値の前後）、丸めの境界（1,000,500 ns のような HALF_UP と正確な値の丸めで結果が分かれる値）、桁の繰り上がりで単位や指数形式が変わる値（9,999,500 ns、9999.5 に当たる値）、精度1と大きな精度（差が出る17以上、例として20）、i64 の範囲の外の期間（`Duration::from_secs(1_000_000_000_000)`）、`include_nanos = true` の併用。

PBT：足す。`pretty_duration_property_test.rs` に、ランダムなナノ秒数について D1 を確かめる性質と、表示値を単位で戻した値が丸めの範囲で元の値に近いことを確かめる性質を置く。丸めの方式そのものは PBT では固定せず、M1 の固定ケースで固定する。原典外であり台帳の件数に含めない。乱数と性質テストの枠組みは既存の `frequency_sketch_property_test.rs` の流儀に合わせ、新しい依存は足さない。

### Glob

| 番号 | 性質 |
|:---|:---|
| G1 | 参照照合と一致する。参照照合は §2.2 で定義した glob の意味そのもの（`?` は任意の1文字、`*` は任意の文字の並び、それ以外はリテラル、全体の一致）である。`Helpers.makePattern` と一致するのは、行の終わりの文字と `\` を含まない範囲だけであり、その範囲では両者が一致することもケース4が確かめる |
| G2 | `*` を含まないパターンは、長さが等しく全文字が一致（`?` は任意の1文字）するときにだけ一致する |
| G3 | 計算量はパターンの長さ×入力の長さのオーダー以下であり、正規表現のバックトラックのような指数的な爆発はしない |
| G4 | 空パターンは空入力にだけ一致する。`*` だけのパターンはすべての入力に一致する |

壊しうる入力：空パターンと空入力、`*` の連続、`*` の前後のリテラルが入力に現れない場合、パターンが入力より長い場合、原典のケース5と6の形、行の終わりの文字を含む入力（移植の `?` と `*` は一致する。makePattern は一致しない）、BMP の外の文字を含む入力（`?` の数え方が原典の `Glob` と違う）。

PBT：足す。`glob_property_test.rs` に、短いアルファベット上のランダムなパターンと入力について G1 を参照照合と比べる性質と、G2 と G4 を置く。アルファベットには `a` と `*` と `?` のほかに `\n` と BMP の外の文字（例として 😀）を入れ、§2.2 に書いた文字単位の差と、行の終わりの文字への振る舞いを固定する。原典外であり台帳の件数に含めない。

### Switch

| 番号 | 性質 |
|:---|:---|
| S1 | `is_on()` と `is_off()` は常に対であり、両方真や両方偽にはならない（状態は `AtomicBool` の1か所だけにある） |
| S2 | action つきの切り替えは、状態が移るときだけ真（`Ok(true)`）を返し、移らないときは action を実行せず `Ok(false)` を返す。引数なし版も同じである |
| S3 | action が失敗したら状態は呼び出し前の値に戻る。失敗には `Err` の返却と panic の両方を含む（drop ガードが戻す）。`Err` のときはその `Err` が戻り値になる |
| S4 | `if_on` 系はロックを取らず、呼んだ時点の状態だけで決まる。`while_on` 系と `fold` と `locked` はロックを取り、ロックの保持者の変更と競合しない |
| S5 | `locked` の保持中は、ほかのスレッドからのロックを取る操作は完了しない。これは原典のケース6の内容ではなく、原典の外のテストと §4 の Quint で確かめる性質である |

壊しうる入力・状況：すでに on のものへの切り替えの連続呼び出し、失敗する action、panic する action、`locked` の中からの窓口経由でない再入（止まる。rustdoc に明記する）、複数スレッドからの同時の切り替え。

PBT：足す。`switch_model_test.rs` に、ランダムな操作列（切り替え、失敗する action、panic する action、`if_on` 系、`while_on` 系、`fold`、`locked` の中での窓口経由の切り替え）を単一スレッドで実行し、`bool` のモデルと状態が常に対になることを確かめる性質を置く。panic する action は `catch_unwind` で受ける。原典外であり台帳の件数に含めない。

S5 の確かめ方（原典の外のテスト、`switch_extra_test.rs`）：チャネルで「ロックを保持した」「放す」という合図を送り合い、放す前に相手のスレッドの変更が完了していないことを見る。このテストは正しい実装で誤って失敗することは無いが、壊れた実装を見つけられるかはタイミングに依存し、確率的である。その限界をここに書く。

## 4. Quint（Switch）

書く。理由は、Switch がロックと CAS と失敗のときの戻しという状態遷移を持ち、S1 から S5 までは実行列の性質だからである。PrettyDuration は入力から出力への純粋な変換であり、Glob は状態を持たない照合であるため、この2つには Quint を書かない。

置き場所は `crates/fraktor-actor/specs/switch.qnt` の **1ファイル** とする。CI の検査（`scripts/quint-check-specs.sh`、quint 0.32.0 固定）は `crates/*/specs/*.qnt` の直下のファイルだけを対象にし、ファイルごとに次を求めるため、モデルと `run` を同じファイルに置く（レビューの L4）。

- 主のモジュールは `switch`（またはファイル内の唯一のモジュール）である
- 名前がちょうど `invariant` の不変条件を定義する（検査は `--invariant=invariant` で呼ぶ）
- `run` が1本以上あり、すべての `run` の名前が `Test` で終わる。名前が `Test` で終わる定義は `run` にだけ使う
- `step` が実際に動く（トレースの長さの最大が1より大きい）。動かない仕様は検査が落とす
- 副ディレクトリや `.QNT` は対象外で黙って外れるため、置き場所の字句を守る

このファイルは fraktor-actor で最初の `crates/*/specs/` の仕様になる。既存の actor の仕様（frequency_sketch など）は `docs/design/specs/` にあり CI の対象外だが、それらを移すことはこの issue の範囲外であり、ここでは触れない。

モデルの中身：状態は「off／on」と、ロックの持ち主（なし／スレッドA／スレッドB）と、action の実行中であるかを持つ。遷移は、ロックの取得と解放、CAS による状態の反転、action の成功（確定）と失敗（drop ガードによる戻し）、窓口経由の再入（ロックを取り直さない遷移）である。不変条件 `invariant` は、ロックの持ち主が最大で1つであることと、action の実行中は状態が反転した値であり持ち主がそのスレッドであることの組にする。S5 は状態の述語ではなく遷移の性質であるため、そのままでは不変条件に入らない。そこで履歴の変数を2つ足す：`locked` に入ったときの状態 `onAtLock` と、最後に状態を変えたスレッド `lastChanger` である。そのうえで「`locked` の中に A が居る間は、`on == onAtLock` であるか `lastChanger == A` である」を `invariant` の一部にする（再レビューの L-a の案）。`run` は、切り替えの成功と失敗と戻しを含む名前が `Test` で終わるものを1本以上置く。再入の禁止そのものは Quint では表さず、API の形（`LockedSwitch`）と rustdoc で受ける（レビューの L4 の試し書きでも同じ結論だった）。

## 5. 判断事項への決定（レビューの推奨を採る）

初版が「決めてほしい」と挙げた3点は、独立レビューの推奨が出たため、この版で次のとおり決める。

1. **Switch の「revert when exception」を `Err` を返すクロージャで表す案：採る。** 条件として、戻しを drop ガードにし、unwind のときにも状態を戻す（M5）。ケース2は `switch_on_with(|| Err(..))` がその `Err` を返し、そのあと `is_off()` であることを確かめる。ケースの除外ではない。
2. **ケース6の進め方と再入：sleep は使わない。latch は join に置き換える。再入は `LockedSwitch` で受ける。** 状態は `AtomicBool` の1か所に置き、ロックは `spin::Mutex<()>` のままにする。S5 は原典の外のテストと Quint で確かめ、その検出が確率的であることを §3 に書いた。
3. **PrettyDuration の入力を列挙型にする案：採る。** 条件は、振る舞いを列挙型の `impl` に置くこと（M3）、飽和させず `u128` で扱うこと（M2）、精度を `NonZeroUsize` にすることである。

## 6. 影響

- 公開 API：追加だけである。既存の型と関数には触れず、互換を壊さない。新しい公開名は `kernel::pretty_duration::PrettyDurationInput`、`kernel::glob::GlobPattern`、`kernel::switch::{Switch, LockedSwitch}` である。製品の表面の命名で `pekko` の語は使わない。
- 上流（Pekko）：原典は読むだけで、変更しない。fraktor-ng-rs のフォークや上流への送付はこの issue の範囲に無い。
- 保存済みのデータ：台帳 `ledger/nodes.yaml` の対象3ノードの状態と件数を第2段で更新する。それ以外の保存データ（設定ファイル、永続化された状態）には触れない。Switch と Glob と PrettyDuration は実行時のみの値であり、永続化の形式を持たない。
- 守りの仕組み：第2段では CI と同じ検査をすべて流す（`cargo fmt --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`、台帳の Python テスト、`scripts/quint-check-specs.sh`）。必須のチェックが1つでも落ちたら、ほかの完了を待たずに直す。
- 依存：新しいクレート依存は足さない。`spin` は既存の依存である。

## 7. レビューの指摘への回答

| 指摘 | 対応 | 内容 |
|:---|:---|:---|
| H1 文書が途中で切れている | 直す | 全文を入れ直した。§3 の Switch の続き、§4、§5、§6 が欠けていた。push の後に GitHub 上のファイルで末尾まで入っていることを確かめる手順を、この回答とは別に実施して報告する |
| H2 ケース6の読み違い | 直す | §1.3 を読み直した。再入と、その後の別スレッドの変更が原典の中身であり、保持中に待たせる場面は無い。S5 は原典の外の性質として §3 と §4 へ移した |
| M1 `%g` の丸め方が未決定 | 直す | §2.1 に算法を書き切った（最短の桁、HALF_UP、丸めた後の指数で形式を選ぶ、Java の形の指数）。レビューが実測した7件を固定ケースにした |
| M2 飽和の前提の誤り | 直す | §2.1 を直した。飽和させず `u128` のまま扱い、範囲の外は原典に無い拡張として単体テスト1件を置く |
| M3 自由関数と生の基本型 | 直す | §2 に API を書き直した。`PrettyDurationInput` の `impl`、`GlobPattern`、精度は `NonZeroUsize` |
| M4 G1 の言い過ぎと参照照合の説明 | 直す | §2.2 と §3 の G1 を直した。参照照合は glob の意味そのものとし、makePattern と一致する範囲を限定した。PBT のアルファベットに `\n` と BMP の外の文字を入れる |
| M5 panic のときに状態が戻らない | 直す | §2.3 に drop ガードを入れた。panic のときに戻ることを原典の外のテストで確かめる |
| L1 原典の `switchOn(())` は action つき版 | 直す | §1.3 に書いた。移植のケースは action つき版で写し、引数なし版は原典の外の単体テストにする |
| L2 `PrettyDurationError` のファイルが無い | 直す | M3 の対応で型ごと無くなったため、ファイル表にも無い |
| L3 文書の細かい点 | 直す | §2.3 の中から §2.3 を指していた言い回しを消した。§1.1 に、原典のケース名が補間文字列であり表は言い換えであることを書いた |
| L4 Quint の CI 要件 | 直す | §4 に要件をすべて書き、モデルと `run` を1ファイルに置く形にした |

採らなかった指摘は無い。高2件、中5件、低4件の全11件に対応した。
