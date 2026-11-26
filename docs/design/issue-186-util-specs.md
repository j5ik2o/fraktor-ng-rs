# 設計：#186 util の PrettyDurationSpec／GlobSpec／SwitchSpec

[Issue #186](https://github.com/j5ik2o/fraktor-ng-rs/issues/186) の設計の記録である。第1段はこの設計だけであり、実装は設計の独立レビューが通ってから別の担当が行う。この文書がレビュー対象である。

原典は Pekko v2.0.0 の `actor-tests/src/test/scala/org/apache/pekko/util/` の3つの Spec と、それが使う本体（`PrettyDuration` は `actor/src/main/scala/org/apache/pekko/util/PrettyDuration.scala`、`Glob` は同 `Glob.scala`、`Switch` は同 `LockUtil.scala` の中の `class Switch`、`Glob` の照合相手である `Helpers.makePattern` は同 `Helpers.scala`）である。移植の規約は `.takt/facets/knowledge/fraktor-porting-rules.md`、形式の前例は [PR #205](https://github.com/j5ik2o/fraktor-ng-rs/pull/205) の [issue-177-frequency-sketch.md](issue-177-frequency-sketch.md) と `docs/goals/issue-177-frequency-sketch.md` である。

## 0. 件数の数え直し（目安との差）

目安と実数が違う。PrettyDurationSpec の目安は issue 本文と本依頼では4件、台帳 `ledger/nodes.yaml` の `test_cases_approx` では3件だが、実数は **13件** である。理由は、原典の `cases.foreach` が10件の表（期間と期待文字列の組）を1件ずつテストとして展開するためである。ScalaTest では `foreach` の反復ごとに1テストになる。FrequencySketchSpec の前例でも、`" in {"` の数ではなく展開後の件数を数えた。

| ノード | 目安（issue／台帳） | 実数 |
|:---|:---|:---|
| `actor-tests/src/test/scala/org/apache/pekko/util/PrettyDurationSpec.scala` | 4／3 | **13**（表10件＋特殊値3件） |
| `actor-tests/src/test/scala/org/apache/pekko/util/GlobSpec.scala` | 6／6 | **6** |
| `actor-tests/src/test/scala/org/apache/pekko/util/SwitchSpec.scala` | 6／6 | **6** |
| 合計 | 16 | **25** |

台帳へは第2段で `cases.total` にこの実数（13／6／6）を記録する。`test_cases_approx` は前例どおり変えない。

## 1. 何を移植するか（ケース対応表）

移植先はすべて `crates/fraktor-actor` で、統合テストは移植の規約どおりノード1件＝ファイル1件で `tests/` に置き、ファイル冒頭に原典のパスをコメントで残す。作らないケースは無い。全25件を移す。ただし Switch の「revert when exception」と「do proper locking」は、Rust の誤りと並行の仕組みに合わせて検証の手段を変える（§1.3 の注記と §5 の判断事項）。

### 1.1 PrettyDurationSpec → `crates/fraktor-actor/tests/pretty_duration_spec.rs`（13件）

原典の表10件は、原典では `cases.foreach` で生成されるが、移植では1件ずつ名前付きのテスト関数にする（前例の FrequencySketchSpec と同じ扱い）。

| # | 原典のケース | 移植先のテスト名 | 扱い |
|:---:|:---|:---|:---|
| 1 | print 9 ns as `9.000 ns` | `pretty_duration_prints_9_nanos` | 移す |
| 2 | print 95 ns as `95.00 ns` | `pretty_duration_prints_95_nanos` | 移す |
| 3 | print 999 ns as `999.0 ns` | `pretty_duration_prints_999_nanos` | 移す |
| 4 | print 1000 ns as `1.000 μs` | `pretty_duration_prints_1000_nanos_as_micros` | 移す |
| 5 | print 9500 ns as `9.500 μs` | `pretty_duration_prints_9500_nanos` | 移す |
| 6 | print 9500 μs as `9.500 ms` | `pretty_duration_prints_9500_micros_as_millis` | 移す |
| 7 | print 9500 ms as `9.500 s` | `pretty_duration_prints_9500_millis_as_seconds` | 移す |
| 8 | print 95 s as `1.583 min` | `pretty_duration_prints_95_seconds_as_minutes` | 移す |
| 9 | print 95 min as `1.583 h` | `pretty_duration_prints_95_minutes_as_hours` | 移す |
| 10 | print 95 h as `3.958 d` | `pretty_duration_prints_95_hours_as_days` | 移す |
| 11 | work with infinity（`Duration.Inf.pretty` が `infinity` を含む） | `pretty_duration_works_with_infinity` | 移す（入力は §2.1 の `PrettyDurationInput::Infinite`） |
| 12 | work with -infinity（`minus infinity` を含む） | `pretty_duration_works_with_minus_infinity` | 移す（同 `MinusInfinite`） |
| 13 | work with undefined（`undefined` を含む） | `pretty_duration_works_with_undefined` | 移す（同 `Undefined`） |

原典の検査は、表10件が完全一致（`should ===`）、特殊値3件が部分一致（`should include`）である。移植でもその強さを変えない。

### 1.2 GlobSpec → `crates/fraktor-actor/tests/glob_spec.rs`（6件）

| # | 原典のケース | 移植先のテスト名 | 扱い |
|:---:|:---|:---|:---|
| 1 | match the literal cases（5組） | `glob_matches_literal_cases` | 移す |
| 2 | treat ? as exactly one character（4組） | `glob_treats_question_mark_as_exactly_one_character` | 移す |
| 3 | treat * as any run of characters（7組） | `glob_treats_star_as_any_run_of_characters` | 移す |
| 4 | agree with the regular expression it replaces, exhaustively over short inputs | `glob_agrees_with_reference_pattern_exhaustively_over_short_inputs` | 移す（照合相手は §2.2） |
| 5 | match a pattern that makes the regular expression backtrack, promptly（パターン26文字、入力 `a`×36、制限3秒、結果は false） | `glob_matches_backtracking_pattern_promptly` | 移す |
| 6 | stay prompt as the input grows（パターン `*a`×20＋`*b`、入力 `a`×2000、制限5秒、結果は false） | `glob_stays_prompt_as_input_grows` | 移す |

ケース4の数え方：原典はアルファベット `{a, b, *, ?}` の長さ0〜4のパターン全341件（`patterns.size should be > 300` で検査）と、アルファベット `{a, b}` の長さ0〜4の入力全31件の、全組（341×31＝10,571組）を照合する。移植でも同じ生成と、パターン数が300を超えることの検査を残す。

ケース5と6の時間制限：原典は ScalaTest の `failAfter` である。移植では結果（false）の検査を本体とし、経過時間が原典の制限（3秒／5秒）未満であることも `std::time::Instant` で検査する。アルゴリズムが §3 の不変条件 G3 を守れば実測はミリ秒以下のはずであり、制限は退行の検出用である。待ち（sleep）による検証はしない。

### 1.3 SwitchSpec → `crates/fraktor-actor/tests/switch_spec.rs`（6件）

| # | 原典のケース | 移植先のテスト名 | 扱い |
|:---:|:---|:---|:---|
| 1 | on and off | `switch_turns_on_and_off` | 移す |
| 2 | revert when exception | `switch_reverts_when_action_fails` | 移す。ただし例外の代わりに `Err` を返すクロージャで検証する（§5 の判断事項1）。ケースの除外ではない |
| 3 | run action without locking（`ifOffYield`／`ifOnYield`／`ifOff`／`ifOn`） | `switch_runs_action_without_locking` | 移す |
| 4 | run action with locking（`whileOffYield`／`whileOnYield`／`whileOff`／`whileOn`） | `switch_runs_action_with_locking` | 移す |
| 5 | run first or second action depending on state（`fold`） | `switch_folds_by_state` | 移す |
| 6 | do proper locking（`locked` の保持中に別スレッドの `switchOff` が待つ） | `switch_locked_blocks_concurrent_change` | 移す。ただし原典の `Thread.sleep(500)` と `CountDownLatch.await(5s)` は使わず、チャネルで決定的に検証する（§5 の判断事項2）。台帳のこのノードには `premise_flags: ['thread-sleep']` が付いており、issue #186 本文も「実時間に頼らず決定的に検証する（待ちを弱めない）」としている |

## 2. 本体の有無と、新しく足す公開 API

3つの本体はどれも fraktor-ng-rs に **無い**。`crates/fraktor-actor/src` 全体を `PrettyDuration`、`Glob`、`Switch` で検索し、該当はコメント中の単語（`switched off`、`switch` を含む設定の説明）だけだった。したがって3つとも新規に足す。置き場所は、原典のパッケージが `org.apache.pekko.util` であることと、既存の平らな構成（`kernel::frequency_sketch`、`kernel::double_linked_list` など）に合わせて、`kernel` の直下に3モジュールを作る。既存の公開 API には触れない（追加だけ）。

ファイル構成（命名は既存の `*_value.rs` の規約に従う）：

| ファイル | 中身 |
|:---|:---|
| `crates/fraktor-actor/src/kernel.rs` | `pub mod glob;`、`pub mod pretty_duration;`、`pub mod switch;` の3行 |
| `crates/fraktor-actor/src/kernel/glob.rs` | 宣言と再エクスポート |
| `crates/fraktor-actor/src/kernel/glob/glob_matcher.rs` | `glob_matches` |
| `crates/fraktor-actor/src/kernel/pretty_duration.rs` | 宣言と再エクスポート |
| `crates/fraktor-actor/src/kernel/pretty_duration/pretty_duration_input.rs` | `PrettyDurationInput` |
| `crates/fraktor-actor/src/kernel/pretty_duration/pretty_duration_formatter.rs` | `format_pretty_duration` ほかと `%g` 形式の整形 |
| `crates/fraktor-actor/src/kernel/switch.rs` | 宣言と再エクスポート |
| `crates/fraktor-actor/src/kernel/switch/switch_value.rs` | `Switch` |
| `crates/fraktor-actor/tests/pretty_duration_spec.rs` | 原典13件 |
| `crates/fraktor-actor/tests/glob_spec.rs` | 原典6件 |
| `crates/fraktor-actor/tests/switch_spec.rs` | 原典6件 |
| `crates/fraktor-actor/tests/glob_property_test.rs` | PBT（§3、原典外で台帳の件数に含めない） |
| `crates/fraktor-actor/tests/switch_model_test.rs` | PBT／モデル照合（§3、原典外） |
| `crates/fraktor-actor/tests/pretty_duration_property_test.rs` | PBT（§3、原典外） |
| `crates/fraktor-actor/specs/switch.qnt` | Quint（§4） |
| `ledger/nodes.yaml` | 対象3ノードを green に（第2段） |
| `docs/goals/issue-186-util-specs.md` | ゴール文書（第2段で作る。前例の `docs/goals/issue-177-frequency-sketch.md` と同じ位置づけ） |

### 2.1 PrettyDuration

原典の入力は Scala の `Duration` で、有限値のほかに `Inf`、`MinusInf`、`Undefined` を取る。Rust の `core::time::Duration` は有限の非負値しか表せないため、入力の型を足す。

```rust
// kernel::pretty_duration
pub enum PrettyDurationInput {
  Finite(core::time::Duration),
  Infinite,
  MinusInfinite,
  Undefined,
}

pub fn format_pretty_duration(input: PrettyDurationInput) -> alloc::string::String;
pub fn format_pretty_duration_with(input: PrettyDurationInput, include_nanos: bool, precision: usize)
  -> Result<alloc::string::String, PrettyDurationError>;
```

- `format_pretty_duration` は原典の `pretty`（`includeNanos = false`、精度4）に当たる。
- 原典の `require(precision > 0)` は例外ではなく `PrettyDurationError::PrecisionNotPositive` の `Err` にする（FrequencySketch の前例どおり、原典が投げる例外は値の誤りに写す）。
- 出力は原典どおり：有限値は `%.<precision>g <単位>` の形で、単位は `ns`、`μs`、`ms`、`s`、`min`、`h`、`d`。`include_nanos` が真なら末尾に ` (<nanos> ns)` を足す。特殊値は `∞ (infinity)`、`-∞ (minus infinity)`、`undefined`。
- 単位の選択は原典の `chooseUnit` の写しである：ナノ秒数を日、時、分、秒、ミリ秒、マイクロ秒へ変換し、最初に0を超えた単位を選ぶ。どれも0なら `ns`。
- 最大の実装上の注意は `%g` である。Java／Scala の `%.4g` は有効数字4桁で、表10件の期待値（`9.000`、`95.00`、`999.0`、`1.000`、`1.583`、`3.958`）はすべてこの形式である。Rust の標準の整形（`{:.3}` など）は固定小数であり一致しない。第2段では、有効数字への丸めと末尾の0の保持と指数形式への切り替え（`%g` は指数が -4 未満または精度以上のとき指数形式）を行う小さい整形関数を `pretty_duration_formatter.rs` の中に書き、表10件と境界値（§3 の P3）で固定する。新しいクレート依存は足さない。
- ナノ秒数への変換：原典の `FiniteDuration.toNanos` は `Long` へ飽和する。`core::time::Duration::as_nanos` は `u128` を返すため、移植では `i64::MAX` への飽和として写すことを設計に含める（原典の飽和と同じ値になるかは、第2段で大きな期間の単体テストを原典外に1件足して確かめる。原典のケースには無い）。
- 負の有限期間：Scala の `FiniteDuration` は負を取れるが、`core::time::Duration` は取れない。原典のケースに負の有限値はないため、これはケースの除外ではなく API の範囲の差であり、入力型が負を受け付けないことで設計上閉じる。

no_std：整形は `alloc::string::String` と `core::fmt` だけで書ける。浮動小数点の丸めは `f64` の core 演算で足りる。std は使わない。

### 2.2 Glob

```rust
// kernel::glob
pub fn glob_matches(pattern: &str, input: &str) -> bool;
```

- 原典の `Glob.matches(pattern, input)` の写しである。引数の順も原典どおり（パターン、入力）。
- アルゴリズムは原典と同じ貪欲な1本走査＋直近の `*` への巻き戻しである：ポインタ `p`（パターン）と `i`（入力）、直近の `*` の位置 `star_p` と、その `*` が消費した入力の位置 `star_i` を持ち、`?` か一致で両方進め、`*` で巻き戻し点を記録して0文字消費から始め、不一致で `star_i` を1つ増やして再試行する。末尾の `*` は読み飛ばし、パターンを最後まで消費したら一致とする。正規表現は使わない。確保（allocation）はしない。
- ケース4の照合相手 `Helpers.makePattern` は、本体には入れない。テスト側に、意味を直接書いた参照照合を置く：`makePattern` は `^\\Q … \\E$` で全体を囲み、`?` を `.`（任意の1文字）、`*` を `.*`（任意の文字列）に置き換え、それ以外をリテラルとする正規表現である。参照照合はこの置き換えを再帰で直接評価する小さい関数としてテストファイルに書き、正規表現クレートへの新規依存は足さない。
- 文字の単位：原典は `charAt`、すなわち UTF-16 の符号単位で数え、`?` は符号単位1つに一致する。移植は Rust の `char`（Unicode スカラ値）で数える。原典のケースはすべて ASCII であり、両者は一致する。差が出るのはサロゲートペア（BMP 外の文字）だけであり、`?` が原典ではその半分に一致しうるのに対し、移植では文字全体に一致する。これは意図した差としてここに記録する。バイトで数える案は、UTF-8 の途中のバイトに `?` が一致してしまい原典よりさらに離れるため採らない。

no_std：`&str` の `chars` だけを使う core の処理であり、問題ない。

### 2.3 Switch

原典は `AtomicBoolean` と、そのオブジェクト自身のモニタ（`synchronized`）で作られている。移植はこの2つを分けて写す：即時の読みと CAS のための `core::sync::atomic::AtomicBool` と、原典の `synchronized` 区間のための `spin::Mutex<()>`（`spin` は既存の依存であり、`kernel::event::event_stream_bus` がすでに kernel で `spin::Mutex` を使っている）。

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

  // ロックを取る切り替え（原典の switchOn / switchOff、引数なし）
  pub fn switch_on(&self) -> bool;
  pub fn switch_off(&self) -> bool;

  // ロックの下で action を実行する切り替え（原典の switchOn(action) / switchOff(action)）
  // 状態が移るときだけ action を実行し、action が Err を返したら状態を元へ戻して Err を返す
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
  pub fn locked<T>(&self, code: impl FnOnce() -> T) -> T;
}
```

- 戻り値の `bool` は原典どおり「実際に状態が移った／action が実行されたか」である。
- 原典の引数なし `switchOn()` と action 付き `switchOn(action)` は Rust ではオーバーロードできないため、action 付きを `_with` の名前に分けた。
- 原典の action は名前渡し（`=> Unit`）であり、状態が移らないときは評価されない。移植の `FnOnce` も同じであり、移らないときは呼ばない。
- `locked` は原典どおり、この Switch のロックを保持したまま `code` を実行し、その値を返す。`locked` の中から同じ Switch のロックを取る操作（`switch_on` など）を呼ぶと、原典のモニタは再入可能だが `spin::Mutex` は再入可能ではないためデッドロックする。これは原典との差である。原典のケース6は `locked` の中で `switchOn(())`（action 付き、すなわち `transcend` であり `synchronized` する）を呼んでいるため、この差はケース6に直接効く。対策は §5 の判断事項2に書く。

no_std：`AtomicBool` は core、`spin::Mutex` は no_std 対応の既存依存であり、kernel に置ける。ただし §2.3 のとおり、ケース6の検証自体はスレッドを使うため統合テスト（std）側で行う。本体の置き場所を adaptor にする案は採らない：状態遷移の意味はプラットフォーム非依存であり、原典もプラットフォーム固有の要素を持たない。

## 3. 守る性質（不変条件）と壊しうる入力、PBT

### PrettyDuration

| 番号 | 性質 |
|:---|:---|
| D1 | 単位は単調である：ナノ秒数が増えて単位が大きな単位へ移ることはあっても、小さな単位へ戻らない |
| D2 | 表示値は選ばれた単位での値であり、`%g` の精度（既定4桁）へ丸めたものと一致する。表示値を単位でナノ秒へ戻すと、元の値との相対誤差は丸めの範囲（有効数字の最終桁の半分）以内である |
| D3 | 特殊値の出力は入力だけで決まり、`include_nanos` と精度に影響されない部分（`infinity`／`minus infinity`／`undefined` の語）を含む |
| D4 | 精度の検査は分岐の前に行う：原典の `require(precision > 0)` は `pretty` の先頭にあり、特殊値の入力でも精度が0以下なら失敗する。移植でも `format_pretty_duration_with` は入力の種類によらず最初に精度を検査する |

壊しうる入力：0ナノ秒（単位は `ns`、値は `0.000` になるはず）、単位の境界の前後（999／1000 ns、999_999／1_000_000 ns、各単位の換算値−1と換算値そのもの）、`i64::MAX` ナノ秒に近い巨大な期間（飽和と `f64` への変換の精度）、精度1と大きな精度（例：15）、`include_nanos = true` の併用。

PBT：足す。`pretty_duration_property_test.rs` に、ランダムなナノ秒数について D1 と D2 を確かめる性質と、境界の前後の単位を確かめる固定ケースを置く。原典外であり台帳の件数に含めない。使用する乱数と性質テストの枠組みは、既存の `frequency_sketch_property_test.rs` の流儀に合わせる（新しい依存を足さない）。

### Glob

| 番号 | 性質 |
|:---|:---|
| G1 | 参照照合（`Helpers.makePattern` の意味）と、すべてのパターンと入力について一致する |
| G2 | `*` を含まないパターンは、長さが等しく全文字が一致（`?` は任意の1文字）するときにだけ一致する |
| G3 | 計算量はパターンの長さ×入力の長さのオーダー以下であり、正規表現のバックトラックのような指数的な爆発はしない。巻き戻しは直近の `*` に対してだけ起き、同じ組を再試行しない |
| G4 | 空パターンは空入力にだけ一致する。`*` だけのパターンはすべての入力に一致する |

壊しうる入力：空パターンと空入力、`*` の連続（`**`、`***`）、`*` の前後のリテラルが入力に現れない場合、パターンが入力より長い場合、`?` が入力の長さを超える場合、原典のケース5と6の形（`*a` の繰り返し＋末尾の `*b` と、満たせない入力）、非 ASCII 文字（§2.2 の文字単位の差）。

PBT：足す。`glob_property_test.rs` に、短いアルファベット上のランダムなパターンと入力について G1 を参照照合と比べる性質と、G2 と G4 を置く。ケース4の全数照合がすでに強い検査だが、PBT は長さ5以上の入力にも広げるために置く。原典外であり台帳の件数に含めない。

### Switch

| 番号 | 性質 |
|:---|:---|
| S1 | `is_on()` と `is_off()` は常に対であり、両方真や両方偽にはならない |
| S2 | `switch_on` は off のときだけ状態を on にし、そのときだけ真を返す。on のときに呼んでも状態は変わらず偽を返す。`switch_off` はその逆である |
| S3 | action 付きの切り替えは、状態が移るときだけ action を実行する。action が失敗（`Err`）したら状態は呼び出し前の値に戻り、戻り値はその `Err` である |
| S4 | `if_on` 系はロックを取らず、呼んだ時点の状態だけで決まる。`while_on` 系と `fold` と `locked` はロックを取り、ロックの保持者の変更と競合しない |
| S5 | `locked` の実行中は、ほかのスレッドからの状態の変更（ロックを取る操作）は完了しない |

壊しうる入力・状況：すでに on のものへの `switch_on` の連続呼び出し、失敗する action、action の中からの同じ Switch の再入（§2.3 のデッドロックの注意）、複数スレッドからの同時の切り替え、`locked` の保持中
...[truncated 3073 chars]