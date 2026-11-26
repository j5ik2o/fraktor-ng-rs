# 設計：#183 testkit/metrics の MetricsKitSpec

[Issue #183](https://github.com/j5ik2o/fraktor-ng-rs/issues/183) の設計の記録である。対象は Pekko v2.0.0（`9fbd2283b1cf701c90a1223bf621942c191db61f`）の testkit にある `MetricsKitSpec` の1ノードで、依存する issue は無い。この文書は第1段（設計だけ）であり、実装・テスト作成・台帳の更新は、設計レビューが通ってから第2段で行う。第2段では、進め方の変更（オーナーの指示、2026-11-26）により、テストは Claude Opus の担当が先に書いて red で置き、本設計に沿う実装だけがそのテストを通すために書かれる。テストの書き換え・弱め・削除はしない。

結論を先に書く。**数え直したケースは5件であり、issue と台帳の目安5件と一致する。5件すべてを移す設計であり、作らない・除外の提案はケース単位では無い。** 対応する本体は fraktor-ng-rs に存在しない（§2）ため、`crates/fraktor-testkit` に `metrics_kit` モジュールを新しく足す。`fraktor-actor` は変更しない。複数スレッドの実行器・dispatcher が前提になるケースは5件の中に無い（§8）ため、[Issue #234](https://github.com/j5ik2o/fraktor-ng-rs/issues/234) の段を待たずに進められる。

進め方の規約は `.takt/facets/knowledge/fraktor-porting-rules.md`、設計文書の前例は [PR #205](https://github.com/j5ik2o/fraktor-ng-rs/pull/205) の [issue-177-frequency-sketch.md](issue-177-frequency-sketch.md) と、直近の第1段の前例である [issue-193-specs.md](issue-193-specs.md) に従う。判断の原則は [pekko-semantics-first.md](../decisions/pekko-semantics-first.md)（速さより Pekko の意味論を優先する）と [scala-java-to-rust-correspondence.md](../decisions/scala-java-to-rust-correspondence.md)（Scala・Java の書き方と Rust の対応）を引く。設定を写す部分は HOCON の文字列ではなく型付きの入力にする（#154 の決定 D1 と、G0b の決定 E の前例）。

## 1. 何を移植するか

### 件数の数え直し

原典 `testkit/src/test/scala/org/apache/pekko/testkit/metrics/MetricsKitSpec.scala`（89行）を1件ずつ数え直した。`AnyWordSpec` の `"MetricsKit" must { ... }` の中にある `in` ブロックが5件あり、目安の5件と一致する。台帳ノードは次の1件である。

- id: `testkit/src/test/scala/org/apache/pekko/testkit/metrics/MetricsKitSpec.scala`
- name: `testkit:MetricsKitSpec`、status: `unported`、`test_cases_approx: 5`、`cases.total` は未記録（null）
- 移植先の見込み: crate は `fraktor-testkit`、テストファイルは `crates/fraktor-testkit/tests/metrics_kit_spec.rs`（ノード1件＝ファイル1件、冒頭に原典のパスを残す）

原典の固定具（fixture）の振る舞いも移植の対象に含める。Spec は `MetricsKit` トレイトを混ぜ、`metricsConfig` として `ConfigFactory.load()` を返し、各ケースの後に `clearMetrics()` を呼び（`after`）、全ケースの後に `shutdownMetrics()` を呼ぶ（`afterAll`）。キーの起点は `MetricKey.fromString("metrics-kit")` である。

### 対応表

| # | 原典のケース | 移す先の Rust のテスト名 | 区分 |
|:---|:---|:---|:---|
| 1 | `allow measuring file descriptor usage` | `metrics_kit_allows_measuring_file_descriptor_usage` | 移す |
| 2 | `allow to measure time, on known number of operations` | `metrics_kit_allows_measuring_time_on_known_number_of_operations` | 移す |
| 3 | `allow to measure average value using Gauge, given multiple values` | `metrics_kit_allows_measuring_average_value_using_gauge_given_multiple_values` | 移す |
| 4 | `measure values in histogram` | `metrics_kit_measures_values_in_histogram` | 移す |
| 5 | `fail with human readable error when the histogram overflowed` | `metrics_kit_fails_with_human_readable_error_when_histogram_overflowed` | 移す |

テスト名は原典の文を snake_case に写したもので、原典の名前は各テストの横にコメントで残す（既存の移植テストの慣例）。

### ケースごとの要点

**ケース1（ファイルディスクリプタ）。** `measureFileDescriptors(KitKey / "file-desc")` を呼ぶと、接頭辞 `metrics-kit.file-descriptors` の下に `open`・`max`・`ratio` の3つのゲージが登録される。検査は `registeredMetrics` のキーに `"file-descriptor"` を含むものが1件より多いこと（`> 0`）だけであり、ゲージの値そのものは検査しない。原典の値は JVM の `UnixOperatingSystemMXBean` から取る。Rust でもファイルディスクリプタは OS の概念として同じ振る舞いを作れるため、伺わずに移す（§2 のファイルディスクリプタの項）。登録されるキーが検査の対象なので、値の取得方法のプラットフォーム差はケースの成否に影響しないが、意味論を優先し、値も実際に読む設計にする。

**ケース2（既知の操作回数の時間計測）。** `timedWithKnownOps(KitKey, ops = 10) { 2 + 2 }` である。明示の検査は無い。暗黙の検査は、(a) ブロックの戻り値がそのまま返ること、(b) 例外なく終わること、(c) キー `metrics-kit` に `KnownOpsInTimespanTimer` が登録され、`getCount` が `expectedOps`（10）を返すこと、(d) ブロックが例外で終わっても `finally` で `stop()` が呼ばれること、である。Rust のテストでは (a)〜(c) を明示の検査にし、経過時間の下限のようなタイミング依存の検査は足さない（原典もしていない。足すと不安定になる）。`stop()` は最初の1回だけが効き、2回目は `false` を返す一回限りのタイマーである。

**ケース3（平均ゲージ）。** `averageGauge(KitKey / "avg-size")` に `add(List(1, 2, 3))` と `add(4)` を行い、`getValue` が `2.5` に等しいことを検査する。算術平均そのものであり、浮動小数点の近似は要らない（`(1+2+3+4) / 4 = 2.5` は `f64` で正確に表せる）。

**ケース4（ヒストグラムへの記録と報告）。** `hdrHistogram(KitKey / "hist", highestTrackableValue = 100.millis.toNanos, 4, "ns")` に、`1 to 11` の外側ループと `0L to 1579331` の内側ループで `update(i)` を呼び、最後にもう1回 `update(1579331)` を呼んでから `reportMetrics()` を呼ぶ。回数を数え直すと `11 × 1_579_332 + 1 = 17_372_653` 回の記録である。明示の検査は無く、暗黙の検査は、(a) すべての値が追跡可能な範囲内（最大 `1_579_331 < 100_000_000`）で溢れのエラーにならないこと、(b) 記録した件数が `17_372_653` になること、(c) `reportMetrics()` が例外なく終わること、である。回数を減らして移すことは移植を弱めるためしない。第2段では、このケースの実行時間を測って報告する。

**ケース5（ヒストグラムの溢れ）。** ケース4と同じ設定のヒストグラムに `update(10.second.toNanos)`（`10_000_000_000`、追跡可能な最大値 `100_000_000` を超える）を呼び、`IllegalArgumentException` が投げられ、そのメッセージが `"can not be stored in this histogram"` を含むことを検査する。原典のメッセージは `Given value $value can not be stored in this histogram (min: ${hist.getLowestDiscernibleValue}, max: ${hist.getHighestTrackableValue}})` であり、末尾の括弧が `}` になっているのは原典の字句の誤りである。検査される部分文字列は再現し、括弧の誤りは再現しない（§2 のエラーの項）。

## 2. 対応する本体がすでにあるか

### 既存の実装の調査結果

- `crates/fraktor-testkit` には計測に関わるモジュールは無い。既存のモジュールは `coroner`、`default_timeout`、`event_filter`、`fraktorspec`、`implicit_sender`、`manual_scheduler`、`test_actors`、`test_kit`、`test_kit_extension` であり、レジストリ、ゲージ、ヒストグラム、タイマーのいずれも持たない。
- `crates/fraktor-actor` の中を `metric`、`histogram`、`gauge` で検索しても、計測に関わる実装は無い（一致するのは `kernel/version` の文字列だけである）。
- ワークスペースの依存にも Codahale Metrics や HdrHistogram に相当するものは無い。`crates/fraktor-testkit/Cargo.toml` の依存は `fraktor-actor` だけである。

したがって、5件すべてについて本体の新規追加が要る。原典で対応するファイルは、testkit の本体（`src/main`）ではなく、テスト側（`src/test`）の `org.apache.pekko.testkit.metrics` パッケージに置かれた `private[pekko]` の内部ユーティリティ一式である（`MetricsKit.scala`、`MetricsKitOps.scala`、`MetricKeyDSL.scala`、`AveragingGauge.scala`、`HdrHistogram.scala`、`KnownOpsInTimespanTimer.scala`、`FileDescriptorMetricSet.scala`、`MemoryUsageSnapshotting.scala`、`reporter/PekkoConsoleReporter.scala`）。外部ライブラリへの依存は、Codahale／Dropwizard Metrics（レジストリ、カウンタ、ゲージ、レポーターの枠組み）と HdrHistogram（Java 版、値の分布の記録）である。

### 新しく足すもの（ファイルの配置）

配置は `crates/fraktor-testkit` の既存の慣例（モジュール1件＝フォルダ、型1件＝ファイル1件）に従う。新しい `.rs` ファイルには、自作のものも含めて、既存ファイルと同じ Apache-2.0 のヘッダー（SPDX 行と Pekko 由来の表記）を付ける。

| ファイル | 中身 |
|:---|:---|
| `crates/fraktor-testkit/src/metrics_kit.rs` | モジュールの宣言と再エクスポート |
| `.../metrics_kit/metrics_kit_value.rs` | `MetricsKit`（レジストリとレポーターを持つ本体） |
| `.../metrics_kit/metric_key.rs` | `MetricKey`（検証ではなく整形だけをするキー） |
| `.../metrics_kit/metric.rs` | `Metric`（登録される計測の列挙） |
| `.../metrics_kit/metrics_error.rs` | `MetricsError`（種類の衝突、ヒストグラムの範囲外、設定の不正） |
| `.../metrics_kit/averaging_gauge.rs` | `AveragingGauge`（算術平均のゲージ） |
| `.../metrics_kit/hdr_histogram.rs` | `HdrHistogram`（値の分布の記録） |
| `.../metrics_kit/known_ops_in_timespan_timer.rs` | `KnownOpsInTimespanTimer`（既知の操作回数の一回限りのタイマー） |
| `.../metrics_kit/file_descriptor_metric_set.rs` | ファイルディスクリプタの3つのゲージの取得 |
| `.../metrics_kit/metrics_kit_settings.rs` | `MetricsKitSettings` と `ConsoleReporterSettings`（型付きの設定） |
| `.../metrics_kit/console_reporter.rs` | `ConsoleReporter`（報告の整形と出力） |
| `crates/fraktor-testkit/tests/metrics_kit_spec.rs` | 原典5件の移植（冒頭に原典のパスを残す） |
| `crates/fraktor-testkit/tests/metrics_kit_property_test.rs` | PBT（§5。原典外で、台帳の件数に含めない） |

### 公開 API の形

原典の `MetricsKit` はトレイトであり、テストクラスに混ぜて使う。Rust では、対応表のとおり `MetricsKit` を所有する構造体とし、テストはこれをフィールドに持つ。登録した計測はレジストリと利用者の両方から触るため、各計測は `Arc` で共有するハンドル型にする（原典でも同じオブジェクトがレジストリと戻り値の両方に現れる）。

```rust
// metric_key.rs — 原典 MetricKeyDSL.MetricKey
pub struct MetricKey { path: String }
impl MetricKey {
  pub fn from_string(root: &str) -> Self;        // 原典 MetricKey.fromString
  pub fn child(&self, part: &str) -> Self;       // 原典の演算子 /
  pub fn as_str(&self) -> &str;
}
impl core::fmt::Display for MetricKey;           // 原典 toString = path

// averaging_gauge.rs — 原典 AveragingGauge（LongAdder の和と個数）
#[derive(Clone)]
pub struct AveragingGauge { /* Arc<AtomicI64 の和, AtomicI64 の個数> */ }
impl AveragingGauge {
  pub fn new() -> Self;
  pub fn add(&self, value: i64);                 // 原典 add(Long)
  pub fn add_slice(&self, values: &[i64]);       // 原典 add(Seq[Long])
  pub fn value(&self) -> f64;                    // 原典 getValue（個数 0 では NaN、§3）
}

// hdr_histogram.rs — 原典 HdrHistogram（単位は表示専用の文字列）
#[derive(Clone)]
pub struct HdrHistogram { /* Arc<Mutex<hdrhistogram::Histogram<u64>>> と unit と範囲 */ }
impl HdrHistogram {
  pub fn new(highest_trackable_value: i64, number_of_significant_value_digits: u8, unit: &str)
    -> Result<Self, MetricsError>;
  pub fn update(&self, value: i64) -> Result<(), MetricsError>;            // 原典 update
  pub fn update_with_count(&self, value: i64, count: i64) -> Result<(), MetricsError>;
  pub fn count(&self) -> u64;
  pub fn unit(&self) -> &str;
}

// known_ops_in_timespan_timer.rs — 原典 KnownOpsInTimespanTimer
#[derive(Clone)]
pub struct KnownOpsInTimespanTimer { /* Arc<開始時刻 Instant, 停止時刻 Mutex<Option<Instant>>> */ }
impl KnownOpsInTimespanTimer {
  pub fn new(expected_ops: i64) -> Self;         // 構築時に計測を始める
  pub fn stop(&self) -> bool;                    // 最初の1回だけ true
  pub fn count(&self) -> i64;                    // 常に expected_ops
  pub fn elapsed(&self) -> core::time::Duration; // 未停止では開始からの経過で代用しない（§3）
  pub fn avg_duration_nanos(&self) -> i64;       // 原典 avgDuration
  pub fn ops_per_second(&self) -> f64;           // 原典 opsPerSecond
}

// metrics_kit_value.rs — 原典 MetricsKit と MetricsKitOps のうち、5件が使う範囲
pub struct MetricsKit { /* registry: BTreeMap<String, Metric>, reporters */ }
impl MetricsKit {
  pub fn new(settings: MetricsKitSettings) -> Self;   // 原典 metricsConfig の型付きの置き換え
  pub fn registered_metrics(&self) -> &BTreeMap<String, Metric>;  // 原典 registeredMetrics
  pub fn average_gauge(&mut self, key: &MetricKey) -> Result<AveragingGauge, MetricsError>;
  pub fn timed_with_known_ops<T>(&mut self, key: &MetricKey, ops: i64, run: impl FnOnce() -> T)
    -> Result<T, MetricsError>;
  pub fn hdr_histogram(&mut self, key: &MetricKey, highest_trackable_value: i64,
    number_of_significant_value_digits: u8, unit: &str) -> Result<HdrHistogram, MetricsError>;
  pub fn measure_file_descriptors(&mut self, key: &MetricKey) -> Result<(), MetricsError>;
  pub fn report_metrics(&self);                  // 原典 reportMetrics
  pub fn report_and_clear_metrics(&mut self);    // 原典 reportAndClearMetrics
  pub fn clear_metrics(&mut self);               // 原典 clearMetrics(MetricFilter.ALL) の範囲
  pub fn shutdown_metrics(&mut self);            // 原典 shutdownMetrics
}
```

設計上の判断を4点書く。

1. **例外は `Result` で表す。** 原典は種類の衝突とヒストグラムの溢れを `IllegalArgumentException` で表す。対応表（`Throwable` の扱い）と既存の移植の慣例に従い、公開 API は `Result<_, MetricsError>` を返す。ケース5の検査は、返ったエラーの `Display` が `"can not be stored in this histogram"` を含むことに写る。メッセージの文面は `Given value {value} can not be stored in this histogram (min: {lowest_discernible_value}, max: {highest_trackable_value})` とし、原典の末尾の括弧の誤り（`}`）は再現しない。検査される部分文字列は変わらない。
2. **`timed_with_known_ops` の `finally` は、クロージャの戻りで表す。** Rust のクロージャは通常の戻りでもパニックでもスコープを抜けるため、タイマーの `stop()` は戻り値を作った直後、関数の終了時に必ず呼ぶ。パニックの伝播中にも止まることは、ドロップではなく明示の呼び出し順序で保証し、単体テストで確かめる対象として §10 に挙げる。
3. **キーの `/` は名前付きのメソッド `child` にする。** Scala の演算子 DSL を Rust の `Div` 実装で真似ることはしない。キーの整形規則そのものは原典どおりに写す（§4）。
4. **`getOrRegister` の種類の衝突は型付きのエラーにする。** 原典のメッセージは `Key: [%s] is already for different kind of metric! Was [%s], expected [%s]` であるが、`Was` に渡されているのは新しく作ろうとした計測のクラスであり、名前が示す「既存の種類」ではないという原典側の取り違えがある。Rust では `MetricsError::KindConflict { key, existing, requested }` とし、`Display` にはキーと両方の種類名を含める。文面の完全一致は原典の検査対象ではないため、取り違えは写さない。

### 設定（HOCON を写す部分）

原典の Spec は `ConfigFactory.load()` で設定を読み、`MetricsKitSettings` が `pekko.test.metrics.reporters`、`pekko.test.metrics.reporter.console.scheduled-report-interval`、`pekko.test.metrics.reporter.console.verbose` を読む。既定値は testkit 本体の `reference.conf` ではなく、テスト側の `src/test/resources/reference.conf` にだけある（reporters は `[console]`、scheduled-report-interval は `0 ms`、verbose は `false`）。HOCON のパーサーは作らず、#154 の決定 D1 と G0b の決定 E の前例どおり型付きの入力にする。

```rust
pub struct MetricsKitSettings {
  pub reporters: Vec<MetricsReporterKind>,       // 既定は [Console]
  pub console: ConsoleReporterSettings,
}
pub enum MetricsReporterKind { Console }
pub struct ConsoleReporterSettings {
  pub scheduled_report_interval: core::time::Duration,  // 既定は 0（定期報告をしない）
  pub verbose: bool,                                    // 既定は false
}
impl Default for MetricsKitSettings;  // 上のテスト側 reference.conf の値を写す
```

この置き換えで表せなくなるのは、HOCON の文字列としての不正（型の違う値、存在しないキーへの文字列指定）であり、それらは移植では作れない入力として §4 に書く。なお、原典の設定には graphite レポーターの項目もあるが、`MetricsKit.initMetricReporters` が作るのは console だけであり、graphite は設定だけの死んだ表面であるため作らない。これはケースの除外ではなく、原典でも動かない表面を作らない判断である。

### ヒストグラムの実装方法（新しい依存）

値の分布の記録（バケットとサブバケットによる対数階級の集計）は、Java の HdrHistogram ライブラリの振る舞いである。Rust で同じ振る舞いを作る方法を2つ比べ、**方法 A を推奨する**。

- **方法 A（推奨）: `hdrhistogram` クレートを `fraktor-testkit` の依存に足す。** このクレートは Gil Tene の HdrHistogram の Rust 移植であり、記録・件数・最小・最大・百分位の意味論が原典の基になった Java 版と対応する。既定の機能（直列化など）は外し（`default-features = false`）、同期版も使わず、こちらで `Mutex` で包む。エラーの型が違うため、範囲外の記録が返したエラーは §2 の判断1の `MetricsError` に翻訳し、Pekko の文面を作る。依存を足すのは std 前提の testkit だけであり、`fraktor-actor` の kernel の no_std には触らない。
- **方法 B: バケット集計を自前で実装する。** 依存は増えないが、Java 版 HdrHistogram の添字計算（バケット幅、サブバケットの半分ずらし、百分位の丸め）をすべて写すことになり、分量と誤りの余地がこの issue の大きさ（size 1）を超える。方法 A がレビューで採られない場合の代替として残す。

百分位や平均の端数の丸めが Java 版と完全に一致するかは、方法 A でも保証の対象が原典のケースには無い（ケース4は百分位を検査しない）。第2段では、原典の入力列に対する件数・最小・最大を単体テストで固定し、百分位は参考値として報告に留める。

### ファイルディスクリプタ

原典は `UnixOperatingSystemMXBean` のメソッドハンドルで開いている数と上限と比率を取る。Rust では unix 系に限り、次のように同じ値を取る設計にする。

- 開いている数: `/dev/fd` の要素数（Linux では `/proc/self/fd` へのリンクであり、macOS でも同じ読み方ができる）。読み取り自体が使うディスクリプタもその時点では開いているため、数に含めることは原典の「その時点の開数」と矛盾しない。
- 上限: `getrlimit(RLIMIT_NOFILE)` が要るため、`fraktor-testkit` に unix 限定で `libc` 依存を足すことを提案する。これも testkit だけの依存であり、kernel の no_std には触らない。
- 比率: 開いている数を上限で割った `f64`（原典 `FileDescriptorRatioGauge` と同じ）。

unix でないプラットフォームでは、3つのゲージの登録は行い、値の取得は「取れない」ことを表す値（開数と上限は負の値ではなく、取得できない旨を報告の整形で示す形）を返す案を、レビューで決める材料として挙げる。CI が動く Linux と macOS はどちらも unix であり、ケース1の検査（登録キーの存在）はプラットフォームに依らない。

### レポーターと報告の形

原典の `PekkoConsoleReporter` は、ゲージ、カウンタ、ヒストグラム、メーター、タイマーの標準の種類に加え、独自の3種類（`KnownOpsInTimespanTimer`、`HdrHistogram`、`AveragingGauge`）を横断バナー付きで標準出力へ印字する。Rust では `ConsoleReporter` が `MetricsKit` のレジストリのスナップショットを受け取り、登録順ではなくキー順（`BTreeMap`）に、種類ごとの見出しと `key` と値を印字する。定期報告（scheduled-report-interval が 0 より大きい場合）は標準ライブラリのスレッドで回す。これはアクターの dispatcher ではなく、レポーター自身のスレッドであり、[Issue #234](https://github.com/j5ik2o/fraktor-ng-rs/issues/234) の実行器とは無関係である（§8）。既定値では間隔が 0 のためスレッドは起動しない。テストから出力を差し替えられるよう、出力先は `Box<dyn Write + Send>` とし、通常は標準出力にする。

### no_std の判断

`fraktor-testkit` は no_std のクレートではない（`#![no_std]` を持たず、スレッドと時刻と標準入出力を使うテスト支援専用のクレートである）。原典の metrics も、ファイルディスクリプタ、実時間（`System.nanoTime`）、標準出力への報告という std と OS の機能に依存する内部ユーティリティであり、actor の kernel に属さない。したがって新しいモジュールは `fraktor-testkit` の中で std を使い、`fraktor-actor` の kernel（`core` と `alloc` だけ）の no_std は変更によって影響を受けない。時刻は port 越しにせず `std::time::Instant` を直接使う。これは kernel の規約（Clock や Scheduler の port）ではなく、testkit の既存の実装（実時間を直接読む `TestLatch` など）と同じ扱いである。

## 3. 守る性質（不変条件）

- **I1（キーの整形の一貫性）。** `MetricKey` のパスは、生成（`from_string`）と連結（`child`）のどちらでも、原典の `sanitizeMetricKeyPart` を通した部品だけから組み立てる。整形の順序は原典どおりで、先に `"..."` を `"…"`（U+2026）に置き、次に `"."` を `"-"` に、`]`・`[`・`(`・`)`・`<`・`>` を `"|"` に、空白を `"-"` に、`"/"` を `"-"` に置く。
- **I2（レジストリの一意性）。** 1つのキーに登録できる計測は1つだけである。同じキーで同じ種類を再び求めると、既存のハンドルが返り、状態は共有される。同じキーで違う種類を求めると `MetricsError::KindConflict` になり、レジストリは変わらない。
- **I3（平均の定義）。** `AveragingGauge::value` は、加えられたすべての値の合計を個数で割ったものに等しい。一括の加算（`add_slice`）は、要素ごとの加算と同じ結果になる。個数が 0 のときは原典どおり `NaN` である（`0.0 / 0.0`。原典の `LongAdder` の和と個数でも同じになる）。
- **I4（ヒストグラムの件数）。** 成功した `update` の回数と `count` は常に等しい。範囲外の値で失敗した `update` は件数を変えない。記録できる値は `0` 以上で、構築時に決めた追跡可能な最大値以下である。
- **I5（タイマーの一回限り）。** `KnownOpsInTimespanTimer::stop` は、最初に呼ばれたときだけ `true` を返して停止時刻を固定し、以後は `false` を返して停止時刻を変えない。`count` は停止の前後によらず常に `expected_ops` である。
- **I6（消去の完全性）。** `clear_metrics` の後はレジストリが空になり、同じキーで新しい計測を作り直せる（原典の `after { clearMetrics() }` がケース間の干渉を断つ前提である）。
- **I7（報告の無害性）。** `report_metrics` はレジストリを変えない。消去まで含めるのは `report_and_clear_metrics` だけである。報告が有効かどうか（`reportMetricsEnabled`）は原典では常時 `true` のフックであり、Rust では設定の表面にせず、メソッドを呼ぶかどうかで表す。

## 4. 壊しうる入力

- **空文字のキー。** `MetricKey::from_string("")` は空のパスのキーを作る（原典も例外にしない）。空の部品を `child("")` で繋ぐと、パスは末尾に `"."` が付いた形になる。例外にせず原典どおりに写し、PBT の入力に含める。
- **ドットとワイルドカード状の連続。** `".."` は `"--"` に、`"...."` は `"…-"` になる（先に `"..."` が置換されるため）。キー自体にワイルドカードの意味は無い。原典で正規表現を使うのは報告の絞り込み（`.*\.mem\..*` など）だけであり、本設計の範囲（全体の消去と全体の報告）では絞り込みを作らないため、連続するワイルドカードを解釈する入力は存在しない。この点は調べた結果として書く。
- **極端な長さのキー。** 長さの上限は設けない（原典にも無い）。メモリの確保はキーの長さに比例するだけで、添字計算には使わない。
- **ヒストグラムの境界値。** 追跡可能な最大値ちょうどの値は記録でき、最大値に 1 を足した値は範囲外のエラーになる。負の値は範囲外のエラーになる（原典では Java 版 HdrHistogram の `ArrayIndexOutOfBoundsException` を包んで同じ文面のエラーになる経路である）。有効数字の桁数が 0 から 5 の範囲を出る場合と、最大値が 2 未満の場合は、構築時に型付きのエラーにする（原典の Java 版の構築子の検査に対応する）。
- **ヒストグラムの極端な最大値。** `i64::MAX` 級の最大値で構築すると、内部のバケット配列の確保が失敗しうる。パニックではなく構築のエラーに写す（方法 A のクレートが返す生成エラーを翻訳する）。
- **タイマーの境界の時間の値。** 操作回数 `ops` が 0 のとき、`avg_duration_nanos` は原典どおり浮動小数点の除算（無限大）を経て `i64` へ飽和する変換になり、整数の除算によるパニックは起こさない。`ops` が負の場合も原典どおり計算する（検査の対象にはしないが、パニックしないことだけを単体テストで確かめる）。経過時間が 0 のときの `ops_per_second` は原典どおり無限大になりうる。
- **平均ゲージの溢れ。** 和は `AtomicI64` の `fetch_add` で加え、溢れは原典の `LongAdder` と同じく回り込む（wrapping）意味論にする。デバッグ構築でもパニックさせない。
- **設定で表せない入力。** HOCON の文字列としての不正（数値の場所に文字列を書くなど）は、型付きの設定にした時点で作れない。これは移植の忠実さの判断として、#154 の D1 の前例と同じ扱いである。

## 5. PBT を足すか

**足す。** 対象は `crates/fraktor-testkit/tests/metrics_kit_property_test.rs` とし、原典外のテストとして台帳の件数には含めない。ワークスペースには proptest の依存が無いため、[issue-177-frequency-sketch.md](issue-177-frequency-sketch.md) の前例どおり、固定の種の SplitMix64 で入力を作る手製の性質テストにし、失敗時に種を出す。

- **P1（キーの整形）。** 任意の文字列について、整形後のパスは `"."`、空白、`"/"`、`[`、`]`、`(`、`)`、`<`、`>` を含まない。整形は冪等である（整形した文字列をもう一度整形しても変わらない）。
- **P2（平均）。** 任意の値の列について、一括の加算と要素ごとの加算で `value` が一致し、参照実装（列の合計を個数で割る値）と一致する。空の列では `NaN` である。
- **P3（ヒストグラム）。** 範囲内の値だけを記録したとき `count` は記録回数に等しく、最小と最大は記録した値の最小と最大の階級の中にある。範囲外の値を混ぜたとき、失敗した回数だけ `count` が記録の試行回数より少なくなる。
- **P4（レジストリ）。** 同じキーへの同じ種類の再取得は状態を共有する（片方への加算がもう片方から見える）。消去の後は再取得で新しい状態になる。違う種類の再取得は常にエラーになり、既存の登録は壊れない。
- **P5（タイマー）。** `stop` を何回呼んでも `true` は最大で1回だけであり、`count` は常に入力した操作回数に等しい。
- **P6（ファイルディスクリプタのキー）。** `measure_file_descriptors` を呼んだ後は、`file-descriptors` を含むキーがちょうど3件登録される（unix の場合）。

## 6. Quint を書くかどうか

**書かない。** 理由は3つある。第一に、このモジュールには複数の主体が非同期に状態を遷移させる通信の手順が無く、状態遷移と呼べるものはタイマーの「計測中→停止済み」の2状態だけであり、停止が一回限りである性質は I5 と P5 で直接に確かめられる。第二に、レジストリの登録・取得・消去は写像の意味論であり、順序の interleaving をモデル化しても得られる検査が増えない。第三に、`fraktor-testkit` の既存の Quint 仕様（`test_latch` など）は、待ちと時間の相互作用がある対象に限られており、実時間を読むだけの本モジュールはその範疇に入らない。レビューで必要と判断された場合は、タイマーの2状態だけの小さい仕様を追加する余地を残す。

## 7. 台帳の見込みと、区分の提案

第1段では台帳（`ledger/nodes.yaml`）を触らない（第1段の前例どおり）。第2段の見込みは次のとおりである。

- 対象ノード `testkit/src/test/scala/org/apache/pekko/testkit/metrics/MetricsKitSpec.scala` を、`cases.total: 5`、`ported: 5`、`passing: 5`、`rust.crate: fraktor-testkit`、`rust.test_file: crates/fraktor-testkit/tests/metrics_kit_spec.rs` として green にする見込みである。
- ノードの `goal` は現在 null である。周囲の testkit のノードは `G0b` に属しているため、第2段では台帳の決まりに従って goal の帰属を確認してから書く（本設計では決めない）。
- **ケース単位の除外（`cases.excluded`）と作らない区分（`cases.not_built`）の提案は無い。** 5件すべてを移す。区分は人間だけが決めるものであり、本設計は提案すら要らないという判断である。
- 5件が使わない原典の表面は、この issue の範囲の外として作らない。対象は、`counter`、通常の `histogram`（Codahale の蓄積型）、`measureMemory`、`measureGc`、`gc`（`System.gc()` の委譲）、メモリと GC とファイルディスクリプタに絞った報告である。これらは JVM の管理 Bean とガベージコレクションに固有のものを含むが、ケースの除外ではないため、人間の承認を要する提案としては上げない。将来に別の Spec が必要としたときに、その issue で扱う。

## 8. 複数スレッドの実行器・dispatcher との関係

5件を1件ずつ確かめた結果、**[Issue #234](https://github.com/j5ik2o/fraktor-ng-rs/issues/234)（1つの ActorSystem を複数のスレッドで回す実行器。段の issue は #250〜#260）の前提になるケースは無い。** 根拠は、原典の `MetricsKit` が ActorSystem を起動しない設計であること（原典のコメントにも、計測対象が ActorSystem を必要としない場合があるため拡張にしなかったと書かれている）、5件すべてが単一スレッドの同期の呼び出しで完結すること、である。レポーターの定期報告だけがスレッドを使うが、これはレポーター自身の標準ライブラリのスレッドであり、アクターの dispatcher ではない。したがって「#234 の段X の後」に分けるケースは無く、5件すべてを先に進められる。

## 9. 影響

- 変更は `crates/fraktor-testkit` の追加だけであり、既存の公開 API は変えない。`fraktor-actor` は変更せず、kernel の no_std は維持される。
- 新しい依存は2件の提案を含む（`hdrhistogram` と、unix 限定の `libc`）。どちらも `fraktor-testkit` のみに足す。第2段では、依存を足した後の CI（`cargo fmt --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`、台帳の Python テスト）を通す。
- ケース4は約1,737万回の記録を行うため、テスト全体の実行時間が延びる。第2段では所要時間を測り、CI の制限に触れる場合は回数を減らさず、テストの分割ではなく報告として上げる。

## 10. 第2段の進め方（設計レビューが通ってから）

1. Claude Opus の担当が、本設計の対応表どおりに `metrics_kit_spec.rs` の5件と PBT を書き、red を確認してブランチに置く（進め方の変更、2026-11-26）。
2. 実装は red のテストを通す範囲だけを書き、テストは書き換えない。テストが誤りだと思った場合は、実装でごまかさず、判断の依頼として中将へ上げる。
3. あわせて、原典外の単体テストとして、タイマーのパニック時の停止、種類の衝突のエラー、ヒストグラムの境界値（最大値ちょうど、最大値+1、負の値）を固定する。
4. 台帳の対象ノードを green に更新し、`ledger/tools` の検査を通す。Quint は書かないため、Quint の検査は対象外である。
5. CI と同じ検査をすべて流し、PR をドラフトから外す。マージはしない。

## 11. 第1段で動かしたコマンドと結果

- Mac mini の専用フォルダ `~/muse-work/fraktor-ng-rs-183` に `git@github.com:j5ik2o/fraktor-ng-rs.git` を clone し、main（`b591452`）から作業ブランチを切った。
- 原典は、同じ Mac mini にある Pekko v2.0.0 の取得済みコピー（`~/muse-work/pekko-v2.0.0`）で `MetricsKitSpec.scala` と metrics パッケージの全ファイルを読んだ。
- Rust のビルドとテストは、第1段（設計のみ）では実行していない。
