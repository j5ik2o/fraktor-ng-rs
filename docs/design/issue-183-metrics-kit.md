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

**ケース1（ファイルディスクリプタ）。** `measureFileDescriptors(KitKey / "file-desc")` を呼ぶと、3つのゲージが登録される。登録キーは原典の生成規則どおりに決まる。接頭辞は引数のキーに `"file-descriptors"` を繋いだもの（`metrics-kit.file-desc.file-descriptors`）であり、`FileDescriptorMetricSet` が返す名前 `file-descriptors.open`・`file-descriptors.max`・`file-descriptors.ratio` をその接頭辞に繋ぎ、部品の整形（`.` を `-` にする）を通すため、実際に登録されるキーは `metrics-kit.file-desc.file-descriptors.file-descriptors-open`、`metrics-kit.file-desc.file-descriptors.file-descriptors-max`、`metrics-kit.file-desc.file-descriptors.file-descriptors-ratio` の3件である。検査は `registeredMetrics` のキーに `"file-descriptor"` を含むものが1件より多いこと（`> 0`）だけであり、ゲージの値そのものは検査しない。原典の値は JVM の `UnixOperatingSystemMXBean` から取る。Rust でもファイルディスクリプタは OS の概念として同じ振る舞いを作れるため、伺わずに移す（§2 のファイルディスクリプタの項）。登録されるキーが検査の対象なので、値の取得方法のプラットフォーム差はケースの成否に影響しないが、意味論を優先し、値も実際に読む設計にする。

**ケース2（既知の操作回数の時間計測）。** `timedWithKnownOps(KitKey, ops = 10) { 2 + 2 }` である。明示の検査は無い。暗黙の検査は、(a) ブロックの戻り値がそのまま返ること、(b) 例外なく終わること、(c) キー `metrics-kit` に `KnownOpsInTimespanTimer` が登録され、`getCount` が `expectedOps`（10）を返すこと、(d) ブロックが例外で終わっても `finally` で `stop()` が呼ばれること、である。Rust のテストでは (a)〜(c) を明示の検査にし、経過時間の下限のようなタイミング依存の検査は足さない（原典もしていない。足すと不安定になる）。`stop()` は最初の1回だけが効き、2回目は `false` を返す一回限りのタイマーである。

**ケース3（平均ゲージ）。** `averageGauge(KitKey / "avg-size")` に `add(List(1, 2, 3))` と `add(4)` を行い、`getValue` が `2.5` に等しいことを検査する。算術平均そのものであり、浮動小数点の近似は要らない（`(1+2+3+4) / 4 = 2.5` は `f64` で正確に表せる）。

**ケース4（ヒストグラムへの記録と報告）。** `hdrHistogram(KitKey / "hist", highestTrackableValue = 100.millis.toNanos, 4, "ns")` に、`1 to 11` の外側ループと `0L to 1579331` の内側ループで `update(i)` を呼び、最後にもう1回 `update(1579331)` を呼んでから `reportMetrics()` を呼ぶ。回数を数え直すと `11 × 1_579_332 + 1 = 17_372_653` 回の記録である。明示の検査は無く、暗黙の検査は、(a) すべての値が追跡可能な範囲内（最大 `1_579_331 < 100_000_000`）で溢れのエラーにならないこと、(b) 記録した件数が `17_372_653` になること、(c) `reportMetrics()` が例外なく終わること、である。回数を減らして移すことは移植を弱めるためしない。第2段では、このケースの実行時間を測って報告する。

**ケース5（ヒストグラムの溢れ）。** ケース4と同じ設定のヒストグラムに `update(10.second.toNanos)`（`10_000_000_000`、追跡可能な最大値 `100_000_000` を超える）を呼び、`IllegalArgumentException` が投げられ、そのメッセージが `"can not be stored in this histogram"` を含むことを検査する。この値は、記録できる上限であるバケット上界（§4）も超えるため、原典でも Rust でも範囲外のエラーになる。なお、追跡可能な最大値に1を足した値（`100_000_001`）はバケット上界の内側であり、原典でもエラーにならない（§4）。原典のメッセージは `Given value $value can not be stored in this histogram (min: ${hist.getLowestDiscernibleValue}, max: ${hist.getHighestTrackableValue}})` であり、末尾の括弧が `}` になっているのは原典の字句の誤りである。検査される部分文字列は再現し、括弧の誤りは再現しない（§2 のエラーの項）。

## 2. 対応する本体がすでにあるか

### 既存の実装の調査結果

- `crates/fraktor-testkit` には計測に関わるモジュールは無い。既存のモジュールは `coroner`、`default_timeout`、`event_filter`、`fraktorspec`、`implicit_sender`、`manual_scheduler`、`test_actors`、`test_kit`、`test_kit_extension` であり、レジストリ、ゲージ、ヒストグラム、タイマーのいずれも持たない。
- `crates/fraktor-actor` の中を `metric`、`histogram`、`gauge` で検索しても、計測に関わる実装は無い（一致するのは `kernel/version` の文字列だけである）。
- ワークスペースの依存にも Codahale Metrics や HdrHistogram に相当するものは無い。`crates/fraktor-testkit/Cargo.toml` の依存は `fraktor-actor` だけである。

したがって、5件すべてについて本体の新規追加が要る。原典で対応するファイルは、testkit の本体（`src/main`）ではなく、テスト側（`src/test`）の `org.apache.pekko.testkit.metrics` パッケージに置かれた `private[pekko]` の内部ユーティリティ一式である（`MetricsKit.scala`、`MetricsKitOps.scala`、`MetricKeyDSL.scala`、`AveragingGauge.scala`、`HdrHistogram.scala`、`KnownOpsInTimespanTimer.scala`、`FileDescriptorMetricSet.scala`、`MemoryUsageSnapshotting.scala`、`reporter/PekkoConsoleReporter.scala`）。外部ライブラリへの依存は、Codahale／Dropwizard Metrics（レジストリ、カウンタ、ゲージ、レポーターの枠組み）と HdrHistogram（Java 版、値の分布の記録）である。

### 新しく足すもの（ファイルの配置）

配置は `crates/fraktor-testkit` の既存の慣例（モジュール1件＝フォルダ、型1件＝ファイル1件）に従う。CI の `syn-check` の `type-per-file` 決まり（1ファイルに置く `pub` の struct / enum / trait は1つまで）にも従い、設定の3型は3ファイルに分ける。`metrics_kit.rs` は `mod` 宣言と、直属の親としての `pub use` による再エクスポートだけを行う（`module-wiring` の決まり）。新しい `.rs` ファイルには、自作のものも含めて、既存ファイルと同じ Apache-2.0 のヘッダー（SPDX 行と Pekko 由来の表記）を付ける。

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
| `.../metrics_kit/file_descriptor_metric_set.rs` | `FileDescriptorMetricSet`（公開型はこの1つだけ。ファイルディスクリプタの3つのゲージの値の取得） |
| `.../metrics_kit/metrics_kit_settings.rs` | `MetricsKitSettings`（型付きの設定。公開型はこの1つだけ） |
| `.../metrics_kit/metrics_reporter_kind.rs` | `MetricsReporterKind`（公開型はこの1つだけ） |
| `.../metrics_kit/console_reporter_settings.rs` | `ConsoleReporterSettings`（公開型はこの1つだけ） |
| `.../metrics_kit/console_reporter.rs` | `ConsoleReporter`（報告の整形と出力） |
| `crates/fraktor-testkit/specs/console_reporter.qnt` | 定期報告スレッドのライフサイクルの Quint 仕様（§6） |
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
impl Default for AveragingGauge;                 // new と同じ値

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
  pub fn min(&self) -> u64;                       // 原典 getData.getMinValue。空では 0
  pub fn max(&self) -> u64;                       // 原典 getData.getMaxValue。空では 0
}

// known_ops_in_timespan_timer.rs — 原典 KnownOpsInTimespanTimer
#[derive(Clone)]
pub struct KnownOpsInTimespanTimer { /* Arc<開始時刻 Instant, 停止時刻 Mutex<Option<Instant>>> */ }
impl KnownOpsInTimespanTimer {
  pub fn new(expected_ops: i64) -> Self;         // 構築時に計測を始める
  pub fn stop(&self) -> bool;                    // 最初の1回だけ true
  pub fn count(&self) -> i64;                    // 常に expected_ops
  pub fn elapsed(&self) -> Option<core::time::Duration>; // 未停止では None（§3 の I5）
  pub fn avg_duration_nanos(&self) -> Option<i64>;  // 原典 avgDuration。未停止では None
  pub fn ops_per_second(&self) -> Option<f64>;      // 原典 opsPerSecond。未停止では None
}

// metric.rs — 登録される計測。値はレジストリと状態を共有するハンドル
#[derive(Clone)]
pub enum Metric {
  AveragingGauge(AveragingGauge),
  HdrHistogram(HdrHistogram),
  KnownOpsInTimespanTimer(KnownOpsInTimespanTimer),
  // ファイルディスクリプタのゲージの列挙子は実装の担当が決める
}

// metrics_kit_value.rs — 原典 MetricsKit と MetricsKitOps のうち、5件が使う範囲
// レジストリは定期報告スレッドと共有するため Arc<Mutex<...>> で持ち、公開メソッドは &self で受ける。
// このファイルの公開型は MetricsKit の1つだけであり、停止ガード等の補助型は公開しない。
pub struct MetricsKit { /* shared: Arc<MetricsKitShared>（registry: Mutex<BTreeMap<String, Metric>>、
                           停止フラグと Condvar、レポーター） */ }
impl MetricsKit {
  pub fn new(settings: MetricsKitSettings) -> Self;   // 原典 metricsConfig の型付きの置き換え。出力先は標準出力
  pub fn with_output(settings: MetricsKitSettings, output: Box<dyn Write + Send>) -> Self;
                                                      // テストから報告の出力先を差し替えるための構築子
  pub fn registered_metrics(&self) -> BTreeMap<String, Metric>;  // 原典 registeredMetrics。
                                                      // 借用は返さず、ロックを取って複製したスナップショットを返す
  pub fn average_gauge(&self, key: &MetricKey) -> Result<AveragingGauge, MetricsError>;
  pub fn timed_with_known_ops<T>(&self, key: &MetricKey, ops: i64, run: impl FnOnce() -> T)
    -> Result<T, MetricsError>;
  pub fn hdr_histogram(&self, key: &MetricKey, highest_trackable_value: i64,
    number_of_significant_value_digits: u8, unit: &str) -> Result<HdrHistogram, MetricsError>;
  pub fn measure_file_descriptors(&self, key: &MetricKey) -> Result<(), MetricsError>;
  pub fn report_metrics(&self);                  // 原典 reportMetrics
  pub fn report_and_clear_metrics(&self);        // 原典 reportAndClearMetrics
  pub fn clear_metrics(&self);                   // 原典 clearMetrics(MetricFilter.ALL) の範囲
  pub fn shutdown_metrics(&self);                // 原典 shutdownMetrics。冪等であり、定期報告スレッドを即座に止める
}
impl Drop for MetricsKit;                        // shutdown_metrics 未呼出でも定期報告スレッドを止めて接合する
```

`Metric` の3つの列挙子、`HdrHistogram::min` と `HdrHistogram::max`、`impl Default for AveragingGauge` は、テストの担当が足した面である（§12 の末尾）。テストは `registered_metrics()` の値を `let Metric::KnownOpsInTimespanTimer(timer) = ... else { ... }` の形で取り出し、列挙子を網羅する `match` は書かない。そのため、ファイルディスクリプタの列挙子を足してもテストは変わらない。

設計上の判断を6点書く。

1. **例外は `Result` で表す。** 原典は種類の衝突とヒストグラムの溢れを `IllegalArgumentException` で表す。対応表（`Throwable` の扱い）と既存の移植の慣例に従い、公開 API は `Result<_, MetricsError>` を返す。ケース5の検査は、返ったエラーの `Display` が `"can not be stored in this histogram"` を含むことに写る。メッセージの文面は `Given value {value} can not be stored in this histogram (min: {lowest_discernible_value}, max: {highest_trackable_value})` とし、原典の末尾の括弧の誤り（`}`）は再現しない。検査される部分文字列は変わらない。
2. **`timed_with_known_ops` の `finally` は、ドロップするガードで表す。** Rust では、クロージャがパニックするとスタックの巻き戻しでその後のコードは実行されないため、明示の呼び出し順序だけでは `finally` を保証できない。そこで、`metrics_kit_value.rs` の中だけに置く非公開の RAII ガード（タイマーへの参照を持ち、`Drop` で `stop()` を呼ぶ）を使い、ガードを `run()` の呼び出しの前から関数の終わりまでのスコープに置く。通常の戻りでもパニックの伝播中でも、スコープを抜けるときに必ず `stop()` が呼ばれる。ガードは `Drop` に頼るため、クロージャに `UnwindSafe` の制約は課さない。パニック時に止まることは単体テストで確かめる対象として §10 に挙げる。
3. **キーの `/` は名前付きのメソッド `child` にする。** Scala の演算子 DSL を Rust の `Div` 実装で真似ることはしない。キーの整形規則そのものは原典どおりに写す（§4）。
4. **登録キーの生成規則はメソッドごとに原典どおりにする。** `hdr_histogram` は、引数のキーに `"hdr-histogram"` を繋いだキー（例：`metrics-kit.hist.hdr-histogram`）で登録する（原典 `MetricsKitOps.scala` の `(key / "hdr-histogram")`）。そのため、同じキーで `average_gauge` と `hdr_histogram` を両方呼んでも衝突しない。`measure_file_descriptors` は、引数のキーに `"file-descriptors"` を繋いだ接頭辞の下に、`file-descriptors.open`・`file-descriptors.max`・`file-descriptors.ratio` を繋いで整形した3キーで登録する（ケース1の項の実例を参照）。登録は原典の `registerAll` と同じく一括であり、同じキーでの再呼出は、同種でも再利用せずエラーにする。3キーのいずれかが既存のキーと衝突する場合は、登録の前に全件を検査し、1件も登録せずにエラーを返す（原子性。I2 と I8）。
5. **`hdr_histogram` の再取得は、引数の検証より先にレジストリを引く。** 原典の `getOrRegister` は名前渡し（遅延評価）であり、既存のキーがある場合は新しいヒストグラムを構築しない。Rust でも、導出したキーで先にレジストリを照会し、同種の既存ハンドルがあれば、引数（最大値・有効数字・単位）を検証せずにそのハンドルを返す。既存が無く、または種類が違う場合にだけ、検証と構築と登録を行う。
6. **`getOrRegister` の種類の衝突は型付きのエラーにする。** 原典のメッセージは `Key: [%s] is already for different kind of metric! Was [%s], expected [%s]` であるが、`Was` に渡されているのは新しく作ろうとした計測のクラスであり、名前が示す「既存の種類」ではないという原典側の取り違えがある。Rust では `MetricsError::KindConflict { key, existing, requested }` とし、`Display` にはキーと両方の種類名を含める。文面の完全一致は原典の検査対象ではないため、取り違えは写さない。

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

なお、このコード片は3型の形をまとめて示したものであり、実際の配置はファイル表のとおり3ファイルに分ける（`type-per-file`）。

`reporters` の意味は原典の `settings.Reporters.contains("console")` どおりにする。`Console` が重複して入っていてもレポーターは最大で1台だけ作り、空の場合は0台であり、その場合の `report_metrics()` は何もしない。`verbose` が `true` のときは、報告に百分位の分布（原典 `PekkoConsoleReporter` の `outputPercentileDistribution` 相当）を含め、`false` のときは含めない。タイマーの報告の時間の整形には、`crates/fraktor-actor` に実装済みの `PrettyDurationInput`（`fraktor_actor::kernel::PrettyDurationInput`）を再利用する。

この置き換えで表せなくなるのは、HOCON の文字列としての不正（型の違う値、存在しないキーへの文字列指定）であり、それらは移植では作れない入力として §4 に書く。なお、原典の設定には graphite レポーターの項目もあるが、`MetricsKit.initMetricReporters` が作るのは console だけであり、graphite は設定だけの死んだ表面であるため作らない。これはケースの除外ではなく、原典でも動かない表面を作らない判断である。

### ヒストグラムの実装方法（新しい依存）

値の分布の記録（バケットとサブバケットによる対数階級の集計）は、Java の HdrHistogram ライブラリの振る舞いである。Rust で同じ振る舞いを作る方法を2つ比べ、**方法 A を推奨する**。

- **方法 A（推奨）: `hdrhistogram` クレートを `fraktor-testkit` の依存に足す。** このクレートは Gil Tene の HdrHistogram の Rust 移植であり、記録・件数・最小・最大・百分位の意味論が原典の基になった Java 版と対応する。既定の機能（直列化など）は外し（`default-features = false`）、同期版も使わず、こちらで `Mutex` で包む。エラーの型が違うため、範囲外の記録が返したエラーは §2 の判断1の `MetricsError` に翻訳し、Pekko の文面を作る。依存を足すのは std 前提の testkit だけであり、`fraktor-actor` の kernel の no_std には触らない。
- **方法 B: バケット集計を自前で実装する。** 依存は増えないが、Java 版 HdrHistogram の添字計算（バケット幅、サブバケットの半分ずらし、百分位の丸め）をすべて写すことになり、分量と誤りの余地がこの issue の大きさ（size 1）を超える。方法 A がレビューで採られない場合の代替として残す。

百分位や平均の端数の丸めが Java 版と完全に一致するかは、方法 A でも保証の対象が原典のケースには無い（ケース4は百分位を検査しない）。第2段では、原典の入力列に対する件数・最小・最大を単体テストで固定し、百分位は参考値として報告に留める。

### ファイルディスクリプタ

原典は `UnixOperatingSystemMXBean` のメソッドハンドルで開いている数と上限と比率を取る。Rust では unix 系に限り、次のように同じ値を取る設計にする。

- 開いている数: Linux では `/proc/self/fd` の要素数を優先し、読めない場合は `/dev/fd` の要素数に落とす（最小のコンテナ環境でも安定させるため）。読み取り自体が使うディスクリプタもその時点では開いているため、数に含めることは原典の「その時点の開数」と矛盾しない。
- 上限: `getrlimit(RLIMIT_NOFILE)` が要るため、`fraktor-testkit` に unix 限定で `libc` 依存を足す。これも testkit だけの依存であり、kernel の no_std には触らない。無制限の設定では `RLIM_INFINITY`（`u64::MAX`）が返りうるため、値は `u64` のまま扱い、`i64` へ素朴に変換して負の値にしない。
- 比率: 開いている数を上限で割った `f64`（原典 `FileDescriptorRatioGauge` と同じ）。上限が `u64::MAX` のときは比率は0に近い値になる。

unix でないプラットフォーム（CI の Windows を含む）でも、3つのゲージの登録は行い、ケース1はスキップしない。原典のケース1も Windows でスキップせず通る（登録だけを検査し、ゲージの値は呼ばないためであり、値を読む側を止めるのは別ファイルの `FileDescriptorMetricSetSpec` だけである）。非 unix では値の取得は未取得を表す形（開数と上限は `Option<u64>` の `None`、比率は `f64::NAN`）に確定させる。CI は Linux と Windows を動かしており（macOS は無い）、`fraktor-actor` の dev-dependencies 経由で Windows でも `fraktor-testkit` がビルドと lint の対象になるため、非 unix の分岐も警告ゼロで通る形にする。

### レポーターと報告の形

#### 行番号の書き方

本項では、原典の行は「ファイル名 N行」と書き、Pekko v2.0.0（`9fbd2283b1cf701c90a1223bf621942c191db61f`）の行番号である。ファイル名を省かない。リポジトリの行は「`パス` N行（ed9fba5）」と書き、`origin/main` の `ed9fba5d5bb071c6ac32373e2d50b871d087bb3d` の行番号である（この改訂の HEAD `74cff4c` は ed9fba5 に設計の文書だけを足したもので、コードの行は同じ）。原典のファイルは次のとおりである。

| 略さない名前 | 置き場所 |
|:---|:---|
| `PekkoConsoleReporter.scala` | `testkit/src/test/scala/org/apache/pekko/testkit/metrics/reporter/`（Pekko v2.0.0） |
| `KnownOpsInTimespanTimer.scala`・`FileDescriptorMetricSet.scala`・`FileDescriptorMetricSetSpec.scala`・`MetricsKit.scala`・`MetricsKitOps.scala`・`HdrHistogram.scala` | `testkit/src/test/scala/org/apache/pekko/testkit/metrics/`（Pekko v2.0.0） |
| `PrettyDuration.scala` | `actor/src/main/scala/org/apache/pekko/util/`（Pekko v2.0.0） |
| `Dependencies.scala` | `project/`（Pekko v2.0.0）。metrics-core 4.2.40、HdrHistogram 2.2.2、Scala 2.13.18 と 3.3.9（45〜48行。既定は先頭の 2.13.18、58行） |
| `AbstractHistogram.java`・`PercentileIterator.java`・`AbstractHistogramIterator.java` | HdrHistogram 2.2.2 の sources jar の `org/HdrHistogram/` |
| `MetricRegistry.java`・`RatioGauge.java`・`ScheduledReporter.java` | metrics-core 4.2.40 の sources jar の `com/codahale/metrics/` |
| `FileDescriptorRatioGauge.java` | metrics-jvm 4.2.40 の sources jar の `com/codahale/metrics/jvm/` |

#### 出力の例の導き方

本項の出力の例は、すべて原典のコードを動かして得たものである。手順はスクリプト `docs/design/issue-299/derive-console-reporter-examples.sh` に置いた。スクリプトは、Pekko v2.0.0 の `PekkoConsoleReporter.scala`・`AveragingGauge.scala`・`HdrHistogram.scala`・`KnownOpsInTimespanTimer.scala`・`FileDescriptorMetricSet.scala`・`PrettyDuration.scala` と、`MetricsKit.scala` 203〜217行の `PekkoMetricRegistry` を `git show v2.0.0:<path>` で取り出し、Maven Central から取った jar（Scala 2.13.18 のコンパイラ、metrics-core と metrics-jvm 4.2.40、HdrHistogram 2.2.2、slf4j-api 1.7.36。`.sha1` で照合する）で、駆動のプログラム `docs/design/issue-299/ConsoleReporterExamples.scala` と一緒にコンパイルして動かす。出力は `docs/design/issue-299/console-reporter-examples.expected` と比べ、一致すれば `the output matches console-reporter-examples.expected` と出して 0 で終わる。`--print` を付けると比べずに出力だけを出す。`MetricsKit.scala` の残りは scalatest と設定のライブラリを要し、報告の形には関わらないため取り込まない。駆動のプログラムは、原典のコンストラクタが受ける `OperatingSystemMXBean`（`FileDescriptorMetricSet.scala` 31行）に代わりの Bean を渡してファイルディスクリプタの値を決め、`KnownOpsInTimespanTimer` の `elapsedTime`（`KnownOpsInTimespanTimer.scala` 42行、`def`）を上書きして経過時間を決める。`avgDuration` と `opsPerSecond` は原典の定義（44・46行）のまま `elapsedTime` から計算される。

`PekkoConsoleReporter.scala` 152行の `data.outputPercentileDistribution(output, 1)` は、`AbstractHistogram.java` 1737〜1740行の `(PrintStream, Double outputValueUnitScalingRatio)` 版に解決される（`1` は `Double` の比 1.0。`int` を取る版は 1752〜1756行の3引数だけである）。この版は刻みを既定の 5（半距離あたり5本）にして 1769行の版を呼ぶ。

この改訂では Temurin 26.0.2 で動かし、出力は `.expected` と一致した（2026-11-28）。

#### 原典の構造と Rust の対応

原典の `report`（`PekkoConsoleReporter.scala` 38〜60行）は、8つの種類を固定の順に印字する。標準の5種類（ゲージ、カウンタ、ヒストグラム、メーター、タイマー、46〜51行）と、独自の3種類（`KnownOpsInTimespanTimer`、`HdrHistogram`、`AveragingGauge`、53〜56行）である。1つの種類について `printMetrics`（`PekkoConsoleReporter.scala` 62〜71行）は、空でなければ（63行）見出しを1行出し（64行）、項目ごとに `"  " + key` の行（66行）と値の行を出し、種類の終わりに空行を1つ出す（69行）。空の種類は見出しも空行も出さない（63行の `if (metrics.nonEmpty)`）。報告の最後にもう1つ空行を出して flush する（58〜59行）。したがって、空のレジストリの報告は改行1つだけになり、空でない報告の末尾には空行が2つ続く。

見出しは `printWithBanner`（`PekkoConsoleReporter.scala` 159〜168行）が作る。`s = "-- " + simpleName(...)` を出し、空白を1つ出し、`-` を `ConsoleWidth - s.length - 1` 個（`ConsoleWidth = 80`、36行）出して改行する。名前の長さを `n` とすると `-` は `76 - n` 個で、改行を除いた行の長さは80になる（`n` が76を超えると `-` は0個で、行は80を超える）。`simpleName`（171〜175行）は実行時クラスの完全名の最後の `.` の後ろを取る。名前は、種類ではなく、その種類の先頭の項目の実行時クラスから取る（64行の `metrics.head._2.getClass`）。

Rust の `Metric`（`crates/fraktor-testkit/src/metrics_kit/metric.rs` 23〜32行（ed9fba5））は4つの列挙子だけを持つ。原典の8つの種類のうち、Rust が印字するのは次の4つであり、この順に出す。

| 順 | 原典の種類 | 入る Rust の列挙子 | 原典の行 |
|:---|:---|:---|:---|
| 1 | ゲージ（標準） | `AveragingGauge` と `FileDescriptorGauge` | `PekkoConsoleReporter.scala` 47行、`printGauge` 85〜87行 |
| 2 | `KnownOpsInTimespanTimer` | `KnownOpsInTimespanTimer` | 54行、128〜136行 |
| 3 | `HdrHistogram` | `HdrHistogram` | 55行、138〜153行 |
| 4 | `AveragingGauge` | `AveragingGauge` | 56行、155〜157行 |

- **`AveragingGauge` は2回出る。** 原典の `AveragingGauge` は `Gauge[Double]` でもある（`AveragingGauge.scala` の `extends Gauge[Double]`）ため、ゲージの種類（`value = ...`）と `AveragingGauge` の種類（`avg = ...`）の両方に現れる。
- **ファイルディスクリプタの3つはゲージの種類にだけ出る。** 原典では3つとも `Gauge` であり（`FileDescriptorMetricSet.scala` 35〜40行）、独自の種類には入らない。
- **カウンタ、Codahale のヒストグラム、メーター、タイマーは出さない。** Rust のレジストリにこれらの種類が無いため、原典で空の種類を出さない規則（63行）と同じ結果になる。

#### 見出しの名前（問い1の決定）

見出しの名前は原典の規則（`PekkoConsoleReporter.scala` 64行、171〜175行）どおり、その種類の先頭の項目の実行時クラスの名前にする。原典の実行時クラスは、Pekko の既定の Scala 2.13.18 でコンパイルした `FileDescriptorMetricSet.scala` から確かめた（`.expected` の `=== class names`）。

| Rust の項目 | 原典の実行時クラス | 見出しの名前 |
|:---|:---|:---|
| `Metric::AveragingGauge` | `org.apache.pekko.testkit.metrics.AveragingGauge` | `AveragingGauge` |
| 開数の `FileDescriptorGauge` | `FileDescriptorMetricSet.scala` 35行の無名クラス `...FileDescriptorMetricSet$$anon$1` | `FileDescriptorMetricSet$$anon$1` |
| 上限の `FileDescriptorGauge` | 38行の無名クラス `...FileDescriptorMetricSet$$anon$2` | `FileDescriptorMetricSet$$anon$2` |
| 比率の `FileDescriptorGauge` | 40行の `com.codahale.metrics.jvm.FileDescriptorRatioGauge` | `FileDescriptorRatioGauge` |
| `Metric::KnownOpsInTimespanTimer` | `...KnownOpsInTimespanTimer` | `KnownOpsInTimespanTimer` |
| `Metric::HdrHistogram` | `...HdrHistogram` | `HdrHistogram` |

ゲージの種類はキー順であるため、ファイルディスクリプタの3つだけなら先頭は `...file-descriptors-max` で、見出しは `-- FileDescriptorMetricSet$$anon$2 ` に `-` 45個が続く。前の版の問い1の案A（`-- FileDescriptorGauge`）は原典に無い名前なので採らない。Rust で同じ名前を作れるため、問い1はこれで閉じる。

#### 種類ごとの行

値の行は、原典の書式文字列をそのまま写す。空白の数は原典の文字列のとおりである。

**ゲージ**（`PekkoConsoleReporter.scala` 85〜87行）: `             value = {値}`（`value` の前に空白13）。`{値}` は `getValue` を `%s` で書いたものである。

| 項目 | 原典の値 | `%s` の結果 | 原典の行 |
|:---|:---|:---|:---|
| `AveragingGauge` | `Double`（和 / 個数） | `Double.toString`（`2.5`、`2.0`、`1.0E-4`、`NaN`） | `AveragingGauge.scala` の `getValue` |
| 開数・上限（unix） | `Long` | 10進の整数（`42`、`1024`） | `FileDescriptorMetricSet.scala` 35〜39行、55〜63行 |
| 開数・上限（非 unix） | 値は返らず、`getValue` が `ClassCastException` を投げる | 値の行は出ない（下の「非 unix のファイルディスクリプタ」） | `FileDescriptorMetricSet.scala` 48〜53行、61〜63行 |
| 比率（unix） | `Double`（開数 / 上限。上限が0なら `NaN`） | `Double.toString`（`0.041015625`） | `FileDescriptorRatioGauge.java` 43〜45行、`RatioGauge.java` 41〜47行 |
| 比率（非 unix） | `Double.NaN` | `NaN` | `FileDescriptorRatioGauge.java` 46〜47行、`RatioGauge.java` 43〜44行 |

**`KnownOpsInTimespanTimer`**（`PekkoConsoleReporter.scala` 128〜136行）: 4行。

```
               ops = {getCount を %d}
              time = {elapsedTime ナノ秒の PrettyDuration}
             ops/s = {opsPerSecond を %2.2f}
               avg = {avgDuration ナノ秒の PrettyDuration}
```

値の定義は `KnownOpsInTimespanTimer.scala` 29〜46行である。`elapsedTime = stopTime.get - startTime`（42行）で、未停止では `stopTime` が0なので `0 - startTime`（`startTime = System.nanoTime`、29行）になる。`avgDuration = (elapsedTime.toDouble / expectedOps).toLong`（44行）、`opsPerSecond = expectedOps.toDouble / (elapsedTime.toDouble / 1_000_000_000)`（46行）である。`PrettyDuration` は `PrettyDuration.scala` 42〜61行で、`chooseUnit`（63〜73行）が日・時・分・秒・ミリ・マイクロのうち変換した値が0より大きい最初の単位を選び、どれも0以下ならナノ秒にする。値は `%.4g` を `Locale.ROOT` で書き（51〜55行）、単位は `ns`・`μs`・`ms`・`s`・`min`・`h`・`d`（75〜83行）である。したがって負のナノ秒は常に `ns` で、`%.4g` の指数表記になりうる。

原典で確かめた値（`.expected` の `=== timers`）:

| 操作回数 | 経過（ns） | `time` | `ops/s` | `avg` |
|---:|---:|:---|:---|:---|
| 10 | 1,234,567 | `1.235 ms` | `8100.01` | `123.5 μs` |
| 10 | 0 | `0.000 ns` | `Infinity` | `0.000 ns` |
| 10 | −123,456,789,012,345（未停止） | `-1.235e+14 ns` | `-0.00` | `-1.235e+13 ns` |
| 0 | 1,000 | `1.000 μs` | `0.00` | `1.068e+05 d`（`avgDuration` が `Long.MaxValue`） |
| 0 | 0 | `0.000 ns` | `NaN` | `0.000 ns` |
| 0 | −123,456,789,012,345 | `-1.235e+14 ns` | `-0.00` | 行は出ない。`avgDuration` が `Long.MinValue` になり、`.nanos` が `IllegalArgumentException` を投げる（`scala.concurrent.duration.FiniteDuration` は ±(2^63−1) ns まで） |

**`HdrHistogram`**（`PekkoConsoleReporter.scala` 138〜153行）: 9行。`count` と `median` は出さない（それがあるのは Codahale のヒストグラム用の `printHistogram`、89〜102行）。

```
               min = {getMinValue を %d} {単位}
               max = {getMaxValue を %d} {単位}
              mean = {getMean を %2.2f} {単位}
            stddev = {getStdDeviation を %2.2f}
              75% <= {getValueAtPercentile(75.0) を %d} {単位}
              95% <= {getValueAtPercentile(95.0) を %d} {単位}
              98% <= {getValueAtPercentile(98.0) を %d} {単位}
              99% <= {getValueAtPercentile(99.0) を %d} {単位}
            99.9% <= {getValueAtPercentile(99.9) を %d} {単位}
```

`stddev` だけ単位を付けない（144行）。`verbose` が `true` のときは、この9行の後ろに百分位の分布を続ける（151〜152行）。

**`AveragingGauge`**（`PekkoConsoleReporter.scala` 155〜157行）: `                avg = {getValue を %2.2f}`（`avg` の前に空白16）。

#### 改行

原典の行の終わりは、`%n`（`PekkoConsoleReporter.scala` 74〜157行の各行）と `println`（58・66・69・167行）であり、どちらも JVM の `line.separator`（Windows では CRLF、他では LF）である。百分位の分布だけは `AbstractHistogram.java` 1777・1789・1790・1825〜1831行の書式が `\n` を直に書くため、どのプラットフォームでも LF である。スクリプトは `-Dline.separator=$'\r\n'` で同じ報告を動かし、この混ざり方を確かめた（`.expected` の `=== crlf:` の節。CR を `<CR>` と書いている）。

Rust も同じにする。報告の行の区切りは `cfg(windows)` で `"\r\n"`、それ以外で `"\n"` とし、分布の表の行（見出し・空行・各行・`#[` の3行）は常に `"\n"` とする。CI の `windows-latest` で動くテストは、期待の文字列をこの区切りで組み立てる。

#### 出力の例

`.expected` から2つを抜き出す。どちらも報告の末尾には空行が2つ続く（ここでは省く）。

ゲージの種類と `AveragingGauge` の種類（`metrics-kit.avg-size` に 1, 2, 3, 4 を足した平均ゲージと、開数42・上限1024のファイルディスクリプタの3つ。`=== gauges: averaging gauge and file descriptors 42/1024`）。ゲージの見出しが `-- AveragingGauge` なのは、キー順の先頭が平均ゲージだからである。

```
-- AveragingGauge --------------------------------------------------------------
  metrics-kit.avg-size
             value = 2.5
  metrics-kit.file-desc.file-descriptors.file-descriptors-max
             value = 1024
  metrics-kit.file-desc.file-descriptors.file-descriptors-open
             value = 42
  metrics-kit.file-desc.file-descriptors.file-descriptors-ratio
             value = 0.041015625

-- AveragingGauge --------------------------------------------------------------
  metrics-kit.avg-size
                avg = 2.50
```

`HdrHistogram`（0〜4 を1回ずつ、最大 100,000,000、4桁、単位 `ns`、`verbose = true`。`=== HdrHistogram: 0..4, verbose`）。

```
-- HdrHistogram ----------------------------------------------------------------
  metrics-kit.hist.hdr-histogram
               min = 0 ns
               max = 4 ns
              mean = 2.00 ns
            stddev = 1.41
              75% <= 3 ns
              95% <= 4 ns
              98% <= 4 ns
              99% <= 4 ns
            99.9% <= 4 ns
       Value     Percentile TotalCount 1/(1-Percentile)

      0.0000 0.000000000000          1           1.00
      0.0000 0.100000000000          1           1.11
      0.0000 0.200000000000          1           1.25
      1.0000 0.300000000000          2           1.43
      1.0000 0.400000000000          2           1.67
      2.0000 0.500000000000          3           2.00
      2.0000 0.550000000000          3           2.22
      2.0000 0.600000000000          3           2.50
      3.0000 0.650000000000          4           2.86
      3.0000 0.700000000000          4           3.33
      3.0000 0.750000000000          4           4.00
      3.0000 0.775000000000          4           4.44
      3.0000 0.800000000000          4           5.00
      4.0000 0.825000000000          5           5.71
      4.0000 1.000000000000          5
#[Mean    =       2.0000, StdDeviation   =       1.4142]
#[Max     =       4.0000, Total count    =            5]
#[Buckets =           13, SubBuckets     =        32768]
```

#### キーの順序

- **標準のゲージはキー順である。** `ScheduledReporter.report()` が渡す `getGauges(filter)` は、`MetricRegistry.java` 533〜542行で `TreeMap` に入れて返す。順は `String.compareTo`、すなわち UTF-16 の符号単位の辞書順である。
- **独自の3種類はキー順ではない。** `PekkoMetricRegistry.filterFor`（`MetricsKit.scala` 212〜216行）は `MetricRegistry.getMetrics`（`MetricRegistry.java` 606〜608行、`ConcurrentHashMap` を包んだ `Map`）をそのまま辿るため、ハッシュ表の並びになる。`.expected` の `=== averaging gauges` の `avg` の節は `b-tiny`・`a-empty`・`c-quarter`・`e-eighth`・`d-whole` の順で、キー順ではないことを確かめた。原典のテストも文書もこの順を約束していない。

Rust では4つの種類のすべてをキー順に出す。元帥の指示（項目の並びはキー順）に従い、原典が約束していない順をキー順に確定させる選択であり、`pekko-semantics-first.md` の「許している範囲の中で選べる振る舞いを狭めることはずれにしない」に当たる。比較は UTF-16 の符号単位で行う。Rust の `BTreeMap` は UTF-8 のバイト順なので、U+10000 以上の文字と U+E000〜U+FFFF の文字が混ざると順が変わる（Java では U+1F600 が U+E000 より先、UTF-8 では逆。`.expected` の `=== key order` で確かめた）。そのため、種類に分けた後に UTF-16 の符号単位で並べ直す。

#### 書式の規則

| 書式 | 原典の結果 | Rust の作り方 |
|:---|:---|:---|
| `%d` | 10進、桁区切りなし | `i64`・`u64` の `Display` |
| `%s` の `Double` | `Double.toString`：`1e-3 <= abs(v) < 1e7` は小数、外は `1.0E-4` のような指数表記、整数値にも `.0`、`NaN`・`Infinity`・`-Infinity` | 同じ規則の関数を testkit に置く。最短の桁は Rust のもの（下の違いの表の2行目） |
| `%2.2f`・`%12.4f`・`%2.12f`・`%14.2f` | `Double.toString` の最短桁を HALF_UP で丸める（`0.125` は `0.13`、`1.125` は `1.13`）。負は `-` を付け、`-0.0` と負の小さい値は `-0.00`。`NaN`・`Infinity`・`-Infinity` はその綴り。幅は右寄せ | 最短桁を HALF_UP で丸める関数を1つ置き、幅は右寄せで埋める。Rust の `{:.2}` は2進の正確な値を偶数丸めする（`0.125` が `0.12`）ので使わない |
| `%.4g`（`PrettyDuration`） | 最短桁を HALF_UP で4桁に丸め、丸めた後の指数が −4 以上 4 未満なら小数、外なら `1.235e+14` の形。負は `-` を付ける | 正のナノ秒は `fraktor_actor::kernel::PrettyDurationInput::Finite(..).pretty()` を使う（同じ規則）。0以下は `fraktor-actor` の `pretty_duration_format::format_g`（`pub(crate)` で非負だけを受ける）と同じ作りの関数を testkit に置き、負の値は `-` と絶対値の書式で書く。単位は常に `ns` |
| `Locale` | `PekkoConsoleReporter.scala` の `"...".format` は JVM の既定のロケール、分布は `Locale.US`（`AbstractHistogram.java` 1796・1802・1824〜1831行）、`PrettyDuration` は `Locale.ROOT` | 常に `.`（下の違いの表の3行目） |

#### ヒストグラムの値の求め方

- **百分位の9行。** `getValueAtPercentile`（`AbstractHistogram.java` 1432〜1454行）は、要求百分位を1ulp下げ（1435〜1436行）、`ceil(p × 全件数 / 100)`（1439〜1440行、1未満は1、1442行）を数え、累積件数がそれ**以上**になった最初の添字の値を返す（1446行の `>=`）。`p` が0なら階級の下端（`lowestEquivalentValue`）、それ以外は上端（`highestEquivalentValue`）である（1448〜1450行）。0〜4 の5件で 75% は、`ceil(3.75)` = 4 件目の値 3 になる。
- **`mean` と `stddev`。** `getMean`（1385〜1397行）は各階級の中点（`medianEquivalentValue`）× 件数の和を全件数で割る。`getStdDeviation`（1404〜1418行）は同じ中点からの偏差の二乗 × 件数の和を全件数で割った平方根である。どちらも全件数0では0。
- **分布の表。** 見出しは `%12s %14s %10s %14s\n\n`（1777行）、行は `%12.{桁}f %2.12f %10d %14.2f\n`、100%の行は `%12.{桁}f %2.12f %10d\n`（1789〜1790行）、末尾の3行は 1824〜1832行である。`{桁}` は有効数字の桁数。各行の値は `valueIteratedTo / 1.0`（1797行）、百分位は `percentileLevelIteratedTo / 100`（1798行）、累積件数（1799行）、`1/(1 − 百分位/100)`（1800行）である。
- **分布の刻み。** `PercentileIterator.java` の刻みは、`percentileLevelToIterateTo` に `100 / (5 × 2^(floor(log2(100 / (100 − 現在の百分位))) + 1))` を足して進む（78〜82行）。各値の添字で、`100 × 累積件数 / 全件数 >= 次の百分位` であれば（`PercentileIterator.java` 86〜91行。件数0の添字は飛ばす）その添字の値の上端（`AbstractHistogramIterator.java` 133〜135行）で行を出し、同じ添字のまま次の百分位を試す（`AbstractHistogramIterator.java` 75〜98行）。全件数を数え終えた後に、同じ値で百分位100の行をもう1つ出して止まる（`PercentileIterator.java` 50〜58行）。

Rust の `hdrhistogram` 7.6.0 の `iter_quantiles(5)` は、この刻みと値が原典と一致しない。0〜4 の5件で実測すると、クレートは14行、原典は15行であり、百分位 0.6 の行（原典は値2・累積3、クレートは値3・累積4）、0.8 の行（原典は値3・累積4、クレートは値4・累積5）が違い、原典の 0.825 の行がクレートには無い。そのため分布はクレートの `iter_quantiles` を使わず、`iter_recorded()` の（値、件数）の列から上の規則で組み立てる。`value_at_percentile` も使わず同じ累積から求め、`mean` と `stddev` も上の式で求める。

原典は `getData` でヒストグラムを複製してから読む（`HdrHistogram.scala` 58行）。Rust も同じにし、`Mutex` の中では `hdrhistogram::Histogram` を複製するだけにして、読み取りと出力は複製の上で行う。そのため `HdrHistogram` に非公開の `with_snapshot`（複製を渡して閉包を呼ぶ）を置き、いまの `report_values` と `PERCENTILES`（`crates/fraktor-testkit/src/metrics_kit/hdr_histogram.rs` 27行、165〜173行（ed9fba5））は置き換える。`min`・`max`・`unit`・`count` の公開メソッドはそのまま残す。

#### 未停止のタイマー（問い2の決定）

報告は原典どおり、未停止のタイマーで負の `time` と `avg` を出す（`KnownOpsInTimespanTimer.scala` 42〜46行、`PekkoConsoleReporter.scala` 132〜135行）。前の版の問い2の案A（`time = not stopped`）は原典に無い出力なので採らない。

- **開始時刻の読み。** 原典の `startTime` は `System.nanoTime` の値であり、その起点は Java の契約で「固定だが任意」である。Rust の `Instant` は数値を取り出せないため、`KnownOpsInTimespanTimer` に構築時の `SystemTime::now()` の `UNIX_EPOCH` からのナノ秒（`i64`）を非公開で持たせ、未停止の報告の `elapsedTime` を `0 - その値` とする。値は約 −1.8×10^18 になり、`time = -1.8xxe+18 ns`、`ops/s = -0.00`、操作回数が10なら `avg = -1.8xxe+17 ns` になる。数字は原典でも機械の時計で変わるので、テストは形だけを比べる。
- **停止後の経過。** これまでどおり `Instant` の差（`stop - start`）のナノ秒を `i64` に飽和させたものを使う。
- **`avg` が `i64::MIN` になる場合**（未停止で操作回数0）。原典は `IllegalArgumentException` で報告が止まる（上の表の最後の行）。Rust は `ops/s` の行まで書いてから `ConsoleReporter::report` が `Err`（`io::ErrorKind::InvalidInput`）を返す。`as i64` の変換は Java の `toLong` と同じく飽和し、`NaN` は0になる。
- **公開 API は変えない。** `elapsed()`・`avg_duration_nanos()`・`ops_per_second()` は未停止で `None` を返したままにし（I5）、報告だけが非公開の読みを使う。原典の `elapsedTime`・`avgDuration`・`opsPerSecond` は未停止でも値を返す（`KnownOpsInTimespanTimer.scala` 42〜46行）ので、公開の口は原典と違う。直すには公開 API の型を変える判断が要るため、この改訂では書かずに報告に上げる（直す issue は未起票）。

#### 非 unix のファイルディスクリプタ

原典の開数と上限のゲージは、`UnixOperatingSystemMXBean` のメソッドハンドルを `(Object)Object` の形に変えて呼ぶ（`FileDescriptorMetricSet.scala` 48〜53行、61〜63行）。`com.sun.management.UnixOperatingSystemMXBean` は JDK の共通のソース（OpenJDK の `src/jdk.management/share/classes/`）にあるため、Windows でもクラスはあり、`findVirtual` は成功する。Windows の OS Bean はこのインタフェースを実装しないため、`invokeExact` の引数の型変換で `ClassCastException` が投げられる。スクリプトで、`UnixOperatingSystemMXBean` を実装しない Bean を渡して確かめた（`.expected` の `=== gauges: file descriptors on a non-unix bean`）。報告は見出しと先頭の項目のキーの行まで出て、`getValue` の例外が `report` から外へ出る。比率のゲージは `FileDescriptorRatioGauge.java` 43〜47行で `NaN` を返すので、比率だけなら `value = NaN` が出る（`=== gauges: the ratio alone on a non-unix bean`）。原典の `FileDescriptorMetricSetSpec.scala` 31〜32行も、Bean が `UnixOperatingSystemMXBean` でなければ値の検査を `cancel` する。

Rust は報告をこれに合わせる。開数と上限の読みが無い（非 unix）とき、そのゲージの値の行は書かず、`ConsoleReporter::report` は `Err`（`io::ErrorKind::Unsupported`）を返す。それまでに書いた見出しとキーの行はそのまま残る。比率は `NaN` と書く。

違いが2つ残る。どちらも公開 API の判断が要るため、この改訂では書かずに報告に上げる（直す issue は未起票）。

- 原典の `reportMetrics`（`MetricsKit.scala` 108〜111行）は例外を呼び出し側へ出す。Rust の `report_metrics()` は `()` を返し、`Err` を捨てる。
- 原典の開数と上限の `getValue` は非 unix で例外を投げる。Rust の公開の `FileDescriptorGauge::value()` は `NaN` を返す（`crates/fraktor-testkit/src/metrics_kit/file_descriptor_gauge.rs` 52〜61行（ed9fba5））。

#### 報告の作りと、確かめる口

報告を「読む」と「書く」の2段に分ける。

1. **読む。** `ConsoleReporter::report` は、受け取った写像の各項目から、印字に要る値だけを持つ非公開の読み（次の形）を作る。ロック・OS・時計に触れるのはこの段だけである。ヒストグラムは `with_snapshot` の複製を持つ。
2. **書く。** 読みの列と `verbose` と行の区切りだけを受ける非公開の関数が、種類に分け、UTF-16 の順に並べ、見出しと行を書く。この関数は状態を持たず、同じ読みと同じ `verbose` と同じ区切りに対して同じバイト列を書く。

```rust
// 非公開（pub(crate) 以下）。名前と置き場所は実装の担当が syn-check の決まりに従って決める。
enum MetricReading {
  AveragingGauge { value: f64 },                           // 見出し AveragingGauge
  FileDescriptor(FileDescriptorReading),
  KnownOpsInTimespanTimer { ops: i64, elapsed_nanos: i64 }, // 未停止は 0 - 開始時刻の読み
  HdrHistogram { snapshot: hdrhistogram::Histogram<u64>, unit: String },
}
enum FileDescriptorReading {
  Open(Option<u64>),   // 見出し FileDescriptorMetricSet$$anon$1。None は非 unix
  Max(Option<u64>),    // 見出し FileDescriptorMetricSet$$anon$2
  Ratio(f64),          // 見出し FileDescriptorRatioGauge
}
fn render(readings: &[(String, MetricReading)], verbose: bool, line_separator: &str,
  output: &mut dyn Write) -> io::Result<()>;
```

これに伴い、`FileDescriptorGauge` に種類と値を返す非公開の口（例: `pub(crate) fn reading(&self) -> FileDescriptorReading`）を足し、`KnownOpsInTimespanTimer` に開始時刻の読みと、報告用の符号つきの経過のナノ秒を返す非公開の口を足す。`FileDescriptorKind`（`file_descriptor_gauge.rs` 21〜25行（ed9fba5））と `Metric::FileDescriptorGauge` の公開の形は変えない。

確かめる口は2つになる。

- **単体テスト**（`console_reporter.rs` に `#[cfg(test)] #[path = "console_reporter_test.rs"] mod tests;` を置く、このクレートの慣例の形）は、読みを手で作って `render` を呼ぶ。ファイルディスクリプタの値（42・1024・0.041015625、非 unix の `None`）とタイマーの経過時間（0、1,234,567、負の値）を注入でき、期待の文字列を完全一致で比べられる。区切りも `"\n"` と `"\r\n"` の両方を渡せる。
- **結合テスト**（`crates/fraktor-testkit/tests/` の新しいファイル）は、公開の `ConsoleReporter::new(verbose)` と `report(&self, metrics: &BTreeMap<String, Metric>, output: &mut dyn Write) -> io::Result<()>`（`console_reporter.rs` 38・49行（ed9fba5））と `Vec<u8>` で動かす。値が入力で決まる `AveragingGauge` と `HdrHistogram` は完全一致で、OS と時計を読む `FileDescriptorGauge` と `KnownOpsInTimespanTimer` は形だけで比べる。

`with_output` は `MetricsKit` 側に既にあり（`metrics_kit_value.rs` 168行（ed9fba5））、`ConsoleReporter` には足さない。

#### 原典と違う点

Rust で同じにできないもの、または直すのに公開 API の判断が要るもの。

| 違い | 理由 | 原典の行 |
|:---|:---|:---|
| `Double.toString` の最短の桁 | Rust 側で同じ規則を実装するが、最短の桁は Rust のものを使う。JDK 19 以降は同じ最短表現で、JDK 18 以前は同じ値でも桁が増えることがある。JDK の版による差であり、Rust で一つに決める | `AveragingGauge.scala` の `getValue`、`PekkoConsoleReporter.scala` 86行 |
| `Locale` | 原典の `"...".format` は JVM の既定のロケールを使うため、小数点が `,` のロケールでは Pekko も `,` を出す。Rust は常に `.` である。CI のロケールでは差は出ない | `PekkoConsoleReporter.scala` 74〜157行 |
| 未停止のタイマーの公開の口 | Rust の `elapsed()` 等は `None`、原典は負の値。報告は原典どおり。直すには公開 API の型の変更が要る（直す issue は未起票） | `KnownOpsInTimespanTimer.scala` 42〜46行 |
| 非 unix のファイルディスクリプタ | 報告は原典どおり止まるが、`report_metrics()` は `Err` を捨て、公開の `value()` は `NaN` を返す。直すには公開 API の変更が要る（直す issue は未起票） | `MetricsKit.scala` 108〜111行、`FileDescriptorMetricSet.scala` 48〜63行 |
| 上限が無制限のときの値 | Rust は `RLIM_INFINITY` を `u64` のまま書く（`18446744073709551615`）。原典の値は JDK の `getMaxFileDescriptorCount` が返す `Long` であり、この改訂では JDK の実装を確かめていない | `FileDescriptorMetricSet.scala` 58〜59行 |
| `report_and_clear_metrics` の並行の登録 | 原典は `reportMetrics(); clearMetrics()` であり、報告の間に登録された計測も消す。Rust は取り出した計測だけを消し、報告の間の登録を残す。出力の失敗で取り出した計測を戻すのは、原典の `PrintStream` が書き込みの失敗を投げないため原典に対応する経路が無い。どちらも既存のテスト（`metrics_kit_review_followup_test.rs`）が固定しているため、この改訂では変えずに報告に上げる（直す issue は未起票） | `MetricsKit.scala` 96〜99行 |

前の版がこの表に置いていた「ファイルディスクリプタの開数と上限の型」と「ファイルディスクリプタの見出しの名前」は、上の非公開の読みと見出しの決定で原典と同じになるため、表から外した。

#### 定期報告スレッド

定期報告（Console レポーターが有効であり、かつ `scheduled_report_interval` が 0 より大きい場合）は標準ライブラリのスレッドで回す。これはアクターの dispatcher ではなく、レポーター自身のスレッドであり、[Issue #234](https://github.com/j5ik2o/fraktor-ng-rs/issues/234) の実行器とは無関係である（§8）。既定値では間隔が 0 のためスレッドは起動しない。スレッドと本体は、レジストリ・停止フラグ・出力先を含む共有状態を `Arc` で共有し、レジストリは `Mutex<BTreeMap<String, Metric>>` とする。スレッドの待ちは `thread::sleep` ではなく、停止フラグと対になった `Condvar::wait_timeout_while`（述語は「フラグが立っていない」）とし、`shutdown_metrics()` が呼ばれたら長い間隔の途中でも即座に目覚めて終わる。スレッドはフラグの確認のために停止ロックを取るが、報告はロックを解放してから行うため、報告の途中でも停止側はフラグを立てられる。`shutdown_metrics()` は冪等であり（2回目以降は何もしない）、呼ばれないまま `MetricsKit` がドロップされた場合も、`Drop` が同じ停止手順（停止ロックの中でフラグを立て、解放して Condvar で起こし、ハンドルの状態を見て、スレッドを接合するか接合の完了を待つ）を行うため、スレッドは残らない。このライフサイクルは状態遷移であるため、Quint で検査する（§6）。テストから出力を差し替えられるよう、出力先は `with_output` 構築子で `Box<dyn Write + Send>` を受け取り、`new` では標準出力にする。

#### 停止の境界

停止の保証の境界は、停止フラグを立てた時点ではなく、`shutdown_metrics()`（または `Drop` の停止手順）が戻った時点である。スレッドは報告の前に停止フラグを読むため、停止要求の直前に始まっていた報告は、停止手順が戻る前に完了することがある。戻った後に新しい報告が始まることはない。並行して `shutdown_metrics()` を呼んだ場合も、全ての呼び出しがスレッドの終了を待ってから戻る（接合中の呼び出しは完了を待つ）。報告は出力のロック（出力の `Mutex` と、標準出力なら `StdoutLock`）を取って書く。呼び出し側が出力のロックを持ったまま停止手順を呼ぶと、スレッドが出力のロックを待ち、停止手順がスレッドの終了を待つため止まったままになる。そのため、出力のロックを解放してから停止する契約とし、公開 API の文書に明記した（`metrics_kit_value.rs` 149〜154行（ed9fba5））。この契約と、契約を守る限り待ちの輪ができないことは、`console_reporter.qnt` の `outputLock` と不変条件 `noStopHoldsTheOutput` で検査する（§6）。

#### 報告の失敗の扱い

`ConsoleReporter::report` は書き込みと flush の失敗、非 unix のファイルディスクリプタ、`avg` の `i64::MIN` を `Err` で呼び出し側へ返す。`report_metrics()` は失敗を呼び出し側へ伝えず、計測は登録されたまま残る（原典との違いは上の表）。`report_and_clear_metrics()` は、報告対象を取り出してから出力し、出力が成功した場合だけ取り出した計測を消去済みとする。失敗した場合は取り出した計測を戻す（その間に同じキーが再登録されていた場合は新しい登録を残す）。出力の途中で失敗した場合、既に出力された行が再報告で重複することがある。報告中に新しく登録された計測は取り出しの対象に含まれないため、次の報告に残る。この取り出し・出力・消去か戻しの手順は Quint にはせず、テストで確かめる（§6、§15）。

### no_std の判断

`fraktor-testkit` は no_std のクレートではない（`#![no_std]` を持たず、スレッドと時刻と標準入出力を使うテスト支援専用のクレートである）。原典の metrics も、ファイルディスクリプタ、実時間（`System.nanoTime`）、標準出力への報告という std と OS の機能に依存する内部ユーティリティであり、actor の kernel に属さない。したがって新しいモジュールは `fraktor-testkit` の中で std を使い、`fraktor-actor` の kernel（`core` と `alloc` だけ）の no_std は変更によって影響を受けない。時刻は port 越しにせず `std::time::Instant` を直接使う。これは kernel の規約（Clock や Scheduler の port）ではなく、testkit の既存の実装（実時間を直接読む `TestLatch` など）と同じ扱いである。

## 3. 守る性質（不変条件）

- **I1（キーの整形の一貫性）。** `MetricKey` のパスは、生成（`from_string`）と連結（`child`）のどちらでも、原典の `sanitizeMetricKeyPart` を通した部品だけから組み立てる。整形の順序は原典どおりで、先に `"..."` を `"…"`（U+2026）に置き、次に `"."` を `"-"` に、`]`・`[`・`(`・`)`・`<`・`>` を `"|"` に、空白を `"-"` に、`"/"` を `"-"` に置く。ここでいう空白は原典の `.replaceAll(" ", "-")` どおり ASCII の半角スペース（U+0020）だけであり、タブ・改行・全角スペース（U+3000）は置換しない（`char::is_whitespace()` で広く解釈しない）。
- **I2（レジストリの一意性）。** 1つのキーに登録できる計測は1つだけである。同じキーで同じ種類を再び求めると、既存のハンドルが返り、状態は共有される。同じキーで違う種類を求めると `MetricsError::KindConflict` になり、レジストリは変わらない。
- **I3（平均の定義）。** `AveragingGauge::value` は、加えられたすべての値の合計を個数で割ったものに等しい。一括の加算（`add_slice`）は、要素ごとの加算と同じ結果になる。個数が 0 のときは原典どおり `NaN` である（`0.0 / 0.0`。原典の `LongAdder` の和と個数でも同じになる）。
- **I4（ヒストグラムの件数と記録できる範囲）。** 成功した `update` の回数と `count` は常に等しい。範囲外の値で失敗した `update` は件数を変えない。記録できる値の下限は `0` である。上限は、構築時に決めた追跡可能な最大値そのものではなく、対数バケットの上界（その最大値を含むバケットの等価階級の上限）である。これは原典の Java 版 HdrHistogram と、方法 A の `hdrhistogram` クレートの両方に共通する振る舞いであり、たとえば最大値 `100_000_000`・有効数字4桁では、記録できる上限は `134_217_727`（`2^27 - 1`）であり、`100_000_001` も記録でき、`134_217_728` から範囲外のエラーになる。追跡可能な最大値で厳密に弾く狭めはしない（原典準拠）。
- **I5（タイマーの一回限り）。** `KnownOpsInTimespanTimer::stop` は、最初に呼ばれたときだけ `true` を返して停止時刻を固定し、以後は `false` を返して停止時刻を変えない。`count` は停止の前後によらず常に `expected_ops` である。原典の `elapsedTime` は未停止では負の値になるが、`Duration` は負を表せないため、未停止の間は公開の `elapsed()`・`avg_duration_nanos()`・`ops_per_second()` のいずれも `None` を返すことに確定させる（開始からの経過で代用しない）。これは公開の口の性質であり、報告の出力には及ばない。報告は非公開の開始時刻の読みから原典どおり負の `time` と `avg` を出す（§2「未停止のタイマー」）。公開の口が原典（`KnownOpsInTimespanTimer.scala` 42〜46行）と違う点は §2「原典と違う点」に書いた。
- **I6（消去の完全性）。** `clear_metrics` の後はレジストリが空になり、同じキーで新しい計測を作り直せる（原典の `after { clearMetrics() }` がケース間の干渉を断つ前提である）。
- **I8（ファイルディスクリプタ登録の原子性）。** `measure_file_descriptors` の3キーの登録は全か無かである。いずれかのキーが既存と衝突する場合は、事前の全件検査で失敗し、レジストリは呼び出しの前と変わらない。
- **I7（報告の無害性）。** `report_metrics` はレジストリを変えない。消去まで含めるのは `report_and_clear_metrics` だけである。報告が有効かどうか（`reportMetricsEnabled`）は原典では常時 `true` のフックであり、Rust では設定の表面にせず、メソッドを呼ぶかどうかで表す。

## 4. 壊しうる入力

- **空文字のキー。** `MetricKey::from_string("")` は空のパスのキーを作る（原典も例外にしない）。空の部品を `child("")` で繋ぐと、パスは末尾に `"."` が付いた形になる。例外にせず原典どおりに写し、PBT の入力に含める。
- **ドットとワイルドカード状の連続。** `".."` は `"--"` に、`"...."` は `"…-"` になる（先に `"..."` が置換されるため）。キー自体にワイルドカードの意味は無い。原典で正規表現を使うのは報告の絞り込み（`.*\.mem\..*` など）だけであり、本設計の範囲（全体の消去と全体の報告）では絞り込みを作らないため、連続するワイルドカードを解釈する入力は存在しない。この点は調べた結果として書く。
- **極端な長さのキー。** 長さの上限は設けない（原典にも無い）。メモリの確保はキーの長さに比例するだけで、添字計算には使わない。
- **ヒストグラムの境界値。** 記録できる上限はバケット上界である（I4）。最大値 `100_000_000`・有効数字4桁の構成では、`134_217_727` は記録でき、`134_217_728` は範囲外のエラーになり、追跡可能な最大値に1を足した `100_000_001` は記録できる。負の値は範囲外のエラーになる（原典では Java 版 HdrHistogram の `ArrayIndexOutOfBoundsException` を包んで同じ文面のエラーになる経路である）。有効数字の桁数が 0 から 5 の範囲を出る場合と、最大値が最小の識別可能な値の2倍未満の場合は、構築時に型付きのエラーにする（原典の Java 版の構築子の検査に対応する）。
- **ヒストグラムの極端な最大値。** バケットは対数階級であるため、`i64::MAX` 級の最大値で構築しても内部の配列は小さく（64ビット環境では数 MB 程度）、構築は成功する。生成エラーとして翻訳するのは、有効数字の範囲外など、クレートが型付きで返す構築の失敗だけであり、メモリ確保の失敗（abort）はエラーの対象にしない。
- **`update_with_count` の個数。** 個数が負の場合は型付きのエラーにし、件数は変えない。個数が 0 の場合は何も記録しない成功（no-op）とする。個数が正の場合、記録の前に値の範囲と、現在の件数との加算が溢れないことを検査し、失敗する場合は1件も記録せずにエラーを返す。
- **タイマーの境界の時間の値。** 操作回数 `ops` が 0 のとき、停止後の `avg_duration_nanos` は原典どおり浮動小数点の除算（無限大）を経て `i64` へ飽和する変換になり、整数の除算によるパニックは起こさない（未停止では I5 どおり `None` である）。`ops` が負の場合も原典どおり計算する（検査の対象にはしないが、パニックしないことだけを単体テストで確かめる）。経過時間が 0 のときの `ops_per_second` は原典どおり無限大になりうる。
- **平均ゲージの溢れ。** 和は `AtomicI64` の `fetch_add` で加え、溢れは原典の `LongAdder` と同じく回り込む（wrapping）意味論にする。デバッグ構築でもパニックさせない。一括の加算（`add_slice`）の内部の合計も、`sum()` ではなく `fold(0i64, i64::wrapping_add)` で求める（`i64::MAX` を含む列でもパニックしない）。
- **設定で表せない入力。** HOCON の文字列としての不正（数値の場所に文字列を書くなど）は、型付きの設定にした時点で作れない。これは移植の忠実さの判断として、#154 の D1 の前例と同じ扱いである。

## 5. PBT を足すか

**足す。** 対象は `crates/fraktor-testkit/tests/metrics_kit_property_test.rs` とし、原典外のテストとして台帳の件数には含めない。ワークスペースには proptest の依存が無いため、[issue-177-frequency-sketch.md](issue-177-frequency-sketch.md) の前例どおり、固定の種の SplitMix64 で入力を作る手製の性質テストにし、失敗時に種を出す。

- **P1（キーの整形）。** 任意の文字列について、`from_string` で作った単一部品のキーのパスは `"."`、半角スペース（U+0020）、`"/"`、`[`、`]`、`(`、`)`、`<`、`>` を含まない。`child` で連結したパスには部品の区切りとして `"."` が入るため、この性質は単一部品にだけ主張し、連結したパスについては「各部品が整形済みである」ことを主張する。整形は冪等である（整形した文字列をもう一度整形しても変わらない）。
- **P2（平均）。** 任意の値の列について、一括の加算と要素ごとの加算で `value` が一致し、参照実装（列の合計を `i64` の回り込む加算で求めて個数で割る値）と一致する。空の列では `NaN` である。
- **P3（ヒストグラム）。** バケット上界以下の値だけを記録したとき `count` は記録回数に等しく、最小と最大は記録した値の最小と最大の階級の中にある。バケット上界を超える値を混ぜたとき、失敗した回数だけ `count` が記録の試行回数より少なくなる。追跡可能な最大値を超えてバケット上界以下の値は、失敗ではなく記録される。
- **P4（レジストリ）。** 同じキーへの同じ種類の再取得は状態を共有する（片方への加算がもう片方から見える）。消去の後は再取得で新しい状態になる。違う種類の再取得は常にエラーになり、既存の登録は壊れない。
- **P5（タイマー）。** `stop` を何回呼んでも `true` は最大で1回だけであり、`count` は常に入力した操作回数に等しい。
- **P6（ファイルディスクリプタのキー）。** 空のレジストリ（または対象のキー配下）に対して `measure_file_descriptors` を呼んだ後は、そのキーから導かれる3キーがちょうど登録される。プラットフォームは限定しない（非 unix でも登録はされる）。

## 6. Quint を書くかどうか

**定期報告スレッドのライフサイクルについて書く。** 対象は `crates/fraktor-testkit/specs/console_reporter.qnt` である。モデルは1つの状態列挙ではなく、次の3つの状態と、停止フラグ・停止ロック・出力のロックで表す（出力のロックは 2026-11-28 に足した。下の「`console_reporter.qnt` の見直し」）。

- スレッドの状態 `Thread`：`NotStarted`（未起動）・`Waiting`（待機中）・`Reporting`（報告中）・`Finished`（終了）。スレッドが起動するのは、Console レポーターが有効であること（`console`）と、間隔が正であること（`positiveInterval`）の両方が成り立つときだけであり、この2つの条件は分けて持つ。
- ハンドルの状態 `Handle`：`Running`・`Joining`・`Joined`。実装の `ReporterThreadState`（`Running`・`Joining`・`Finished`）に対応し、実装の `Finished` をモデルでは `Joined` と呼ぶ。
- 停止の呼び出し側の状態 `Call`：`Idle`・`Flagging`・`Flagged`・`Joiner`・`WaitingForTheJoin`・`Returned`。`shutdown_metrics()` の呼び出しと `Drop` のどちらも、この同じ遷移を通る。

停止のロックは、停止フラグ（`stopFlag`）の読み書きの間だけ保持する。報告スレッドはロックを解放してから報告するため、報告の途中でも停止側はフラグを立てて接合へ進める。停止手順は、ロックを取り、フラグを立て、ロックを解放して起こし、ハンドルの状態を見る、という順序でそろえる。最初に `Running` を見つけた呼び出しが `Joiner` として接合し、`Joining` を見つけた呼び出しは `WaitingForTheJoin` として完了を待ち、`Joined` を見つけた呼び出しはそのまま `Returned` になる。停止の保証の境界は、フラグを立てた時点ではなく、呼び出しが戻った時点であり、不変条件 `noReportOnceACallReturned`（呼び出しが戻った後は、報告は走っておらず新しく始まることもない）で表す。ほかの不変条件は、`threadOnlyWithConsoleAndInterval`（Console と間隔の条件が無ければスレッドは起動せず報告も無い）、`flagOnlyAfterAStopBegins`、`leavesOnlyOnTheFlag`（スレッドはフラグが立ったときにだけ終わる）、`stopLockOnlyAroundTheFlag`（ロックはフラグの読み書きの間だけ）、`oneJoinerAtATime`、`joinedAtMostOnce`、`nothingRunsAfterTheDrop` である。あわせて、`shutdown_metrics()` の冪等性、ドロップ時の停止、長い待機の途中での停止要求による即時の終了をモデル化する。なお、以前の版が挙げた「停止要求済み」という状態はモデルには無い。停止要求は `stopFlag` と `Call` の遷移で表す。定期報告スレッドは、待ちと時間とロックと停止要求が絡む並行の状態遷移であり、`fraktor-testkit` の既存の Quint 仕様 `watch_handle.qnt`（監視スレッド）と同型であるため、検査の対象にする。

書かない部分も明記する。タイマーの「計測中→停止済み」の2状態は、停止が一回限りである性質を I5 と P5 で直接に確かめられるため Quint にはしない。レジストリの登録・取得・消去は写像の意味論であり、順序の interleaving をモデル化しても得られる検査が増えないため Quint にはしない。

出力の形（§2）も Quint にはしない。§2 の「報告の作りと、確かめる口」のとおり、報告は「読む」と「書く」に分かれる。状態を持たず、同じ入力に同じバイト列を返すと言えるのは「書く」の段（非公開の `render`。読みの列と `verbose` と行の区切りが同じなら同じバイト列）だけである。「読む」の段は、ヒストグラムの `Mutex` の中の複製、OS のファイルディスクリプタ、時計を読むため、呼ぶたびに値が変わりうる。どちらの段も遷移を持たないため Quint の対象にならない。並び順と書式は §15 の単体テストが `render` で直接に確かめる。

`report_and_clear_metrics()` の手順（取り出す、出力する、成功なら消去済み・失敗なら戻す、並行の再登録は残す。§2「報告の失敗の扱い」）は状態遷移であるが、Quint にはせずテストで確かめる。1回の呼び出しの中の手順であり、並行の関与は「出力の途中で別の登録が入る」ことだけである。この割り込みは、書き込みの中で同じ `MetricsKit` に登録する `Write` を出力先にすれば、テストで決定的に起こせる（§15 の R12）。写像の意味論（取り出し・`or_insert` による戻し）は Quint で interleaving を探しても検査が増えない。

#### `console_reporter.qnt` の見直し（2026-11-28）

quint-modeling の見直しの手順で読み、次の4つを確かめた。quint は 0.32.0（`package-lock.json` の版）である。

**直したこと。** 前の版のモデルは停止ロックだけを持ち、§2「停止の境界」の出力のロックの契約が無かった。`outputLock`（`Free`・`HeldBy(呼び出し側)`・`HeldBy("T")`（報告スレッド））と、呼び出し側が出力のロックを取った回数 `outputHolds`（2回まで）を足し、action `startPrinting`（報告スレッドが出力のロックを取る）・`holdTheOutput`・`releaseTheOutput` を足した。`finishReport` は出力のロックを返す。`beginShutdown` と `beginDrop` には「その呼び出し側が出力のロックを持っていない」というガードを足した（公開 API の文書の契約）。不変条件は `noStopHoldsTheOutput`（停止の途中の呼び出し側は出力のロックを持たない）と `theThreadHoldsTheOutputOnlyToPrint` を足して10個になった。`run` は5つ足して22個（すべて名前が `Test` で終わる）。`outputHolds` に上限を置いたのは、取って返すだけの繰り返しで行き詰まった状態が隠れないようにするためである（上限が無いと全ての実行が20歩まで伸びた。上限を置いた後は最短9歩で終わる実行がある）。`quint typecheck`、`quint test`（22 passing）、`quint run --invariant=invariant --max-steps=20 --max-samples=10000`（違反なし）を通した。

**(a) わざと壊した版。** 1か所だけを変えた版を作り、その不変条件だけを `quint run --invariant=<名前> --max-steps=20 --max-samples=10000 --seed=0x2a` で調べた。11版すべてが違反を返した。

| 不変条件 | 変えた箇所 | 結果 |
|:---|:---|:---|
| `threadOnlyWithConsoleAndInterval` | `construct` で `if (console and positiveInterval)` を `if (console)` にする | 違反 |
| `flagOnlyAfterAStopBegins` | `lookAtTheThread` で `Joined => Returned` を `Joined => Idle` にする | 違反 |
| `leavesOnlyOnTheFlag` | `wakeToStop` のガード `stopFlag` を外す | 違反 |
| `stopLockOnlyAroundTheFlag` | `setTheFlag` で停止ロックを返さない（`stopLock' = stopLock`） | 違反 |
| `noReportOnceACallReturned` | `joinReturn` のガード `thread == Finished` を外す | 違反 |
| `oneJoinerAtATime` | `lookAtTheThread` でハンドルを `Running` のままにする | 違反 |
| `joinedAtMostOnce` | `joinReturn` でハンドルを `Joined` ではなく `Running` に戻す | 違反 |
| `nothingRunsAfterTheDrop` | `beginDrop` のガード「停止中の `shutdown_metrics` が無い」を外す | 違反 |
| `noStopHoldsTheOutput` | `beginShutdown` の出力のロックのガードを外す | 違反 |
| `noStopHoldsTheOutput` | `beginDrop` の出力のロックのガードを外す | 違反 |
| `theThreadHoldsTheOutputOnlyToPrint` | `finishReport` で出力のロックを返さない | 違反 |

`noStopHoldsTheOutput` の違反は、出力のロックを持ったまま停止を始めた呼び出し側の状態であり、そこから報告中のスレッドは `startPrinting` できず、接合する側は `joinReturn` できない。§2 の止まったままの状態がこれに当たる。

**(b) 各 action が起きること。** `quint run --mbt --max-steps=20 --max-samples=1000 --n-traces=1000 --seed=0x1 --out-itf=...` の1000本の実行で、`mbt::actionTaken` を数えた。13の action すべてが1回以上起きた（括弧の中は起きた実行の本数）: `construct` 1000、`wakeToReport` 88、`startPrinting` 88、`finishReport` 88、`wakeToStop` 223、`beginShutdown` 534、`beginDrop` 961、`setTheFlag` 1000、`lookAtTheThread` 1000、`joinReturn` 223、`waitReturn` 59、`holdTheOutput` 1000、`releaseTheOutput` 1000。

**(c) 状態と action と本番の対応。** 行は `crates/fraktor-testkit/src/metrics_kit/metrics_kit_value.rs`（ed9fba5）である。

| モデル | 本番 |
|:---|:---|
| `console`・`positiveInterval` | `with_output` の `has_console` と `scheduled_report_interval > Duration::ZERO`（169・180行） |
| `thread` | 報告スレッドの閉包（182〜193行）。`Waiting` は `wait_timeout_while` の中、`Reporting` は `report_once`（192行）の中、`Finished` は `break`（189行）の後 |
| `handle` | `ReporterThreadState`（31〜38行）。モデルの `Joined` は本番の `Finished` |
| `stopFlag`・`stopLock` | `stop_requested: Mutex<bool>`（46行）とそのロック |
| `outputLock` | `output: Mutex<Box<dyn Write + Send>>`（43行）のロックと、出力先が標準出力なら `StdoutLock`。`HeldBy("T")` は `report_snapshot` の `self.output.lock()`（68行）から戻るまで |
| `calls` | `stop_reporter_thread`（93〜118行）の中の位置。`Flagging` は95〜96行、`Flagged` は98行の後、`Joiner` は107〜110行、`WaitingForTheJoin` は104行、`Returned` は102・114行 |
| `construct` | `with_output`（168〜197行） |
| `wakeToReport`・`wakeToStop` | 183〜191行（フラグを読んで報告へ進むか抜ける） |
| `startPrinting`・`finishReport` | `report_snapshot`（64〜70行）の出力のロックの取得と、`report_once` からの戻り |
| `beginShutdown`・`beginDrop` | `shutdown_metrics`（334〜336行）と `Drop::drop`（339〜343行）から `stop_reporter_thread` に入るところ |
| `setTheFlag`・`lookAtTheThread`・`joinReturn`・`waitReturn` | 95〜98行、99〜108行、110〜114行、104行の待ちからの戻り |
| `holdTheOutput`・`releaseTheOutput` | 呼び出し側が `std::io::stdout().lock()` を取って返すこと（本番のコードには無く、公開 API の文書の契約 149〜154行が対象） |
| `beginShutdown`・`beginDrop` の出力のロックのガード | 公開 API の文書の契約（149〜154行）。本番のコードは検査しない |

**(d) 検査の大きさと、見られないもの。** 呼び出し側は `shutdown_metrics` の2本（A、B）とドロップの1本、報告は3回まで、`shutdown_metrics` の呼び出しは合わせて4回まで、呼び出し側が出力のロックを取るのは2回まで、1本の実行は20歩まで、10,000本の無作為の実行である（`scripts/quint-check-specs.sh` と同じ）。全探索ではなく、違反が無かったことは「調べた実行の中に反例が無かった」という意味である。モデルが見られないものは次のとおりである。報告の中身とレジストリ（§2 の `render` と §15 のテストの対象）、`report_and_clear_metrics` の取り出しと戻し（テストの対象）、報告スレッドの中のパニックと、ロックの poisoning（本番は `expect` で落ちる）、`Condvar` の見かけの目覚め（`wait_timeout_while` の述語が吸収するため、モデルでは目覚めを1つの action にまとめた）、間隔の長さ（時間はモデルに無い）、`thread::spawn` の失敗、出力のロックを持ったまま `report_metrics` を同じスレッドで呼ぶこと（`StdoutLock` は再入できる）、生存性（「いずれ戻る」は不変条件では表せず、行き詰まりは `quint run` では検出されない。行き詰まりの状態に至らないことを `noStopHoldsTheOutput` で間接に見ている）。

**設計とテストの項目との読み直し。** `.qnt` を直した後に §2「停止の境界」と §15 を読み直した。§2 の契約の文は `noStopHoldsTheOutput` と同じことを言い、§15 の項目は出力のロックの契約をテストに求めていない（止まったままの状態をテストで起こすと、テストが止まるため）。食い違いは無い。

## 7. 台帳の見込みと、区分の提案

第1段では台帳（`ledger/nodes.yaml`）を触らない（第1段の前例どおり）。第2段の見込みは次のとおりである（実装はマージ済みであり、いまの台帳の状態は下の「この issue（#299）の台帳への影響」に書く）。

- 対象ノード `testkit/src/test/scala/org/apache/pekko/testkit/metrics/MetricsKitSpec.scala` を、`cases.total: 5`、`ported: 5`、`passing: 5`、`rust.crate: fraktor-testkit`、`rust.test_file: crates/fraktor-testkit/tests/metrics_kit_spec.rs` として green にする見込みである。
- ノードの `goal` は、第1段の設計を書いた時点（`b591452`）では null だった。周囲の testkit のノードは `G0b` に属しているため、第2段では台帳の決まりに従って goal の帰属を確認してから書くとしていた。第2段の後の `ed9fba5` の `ledger/nodes.yaml` では、対象ノード（33460〜33486行）の33485行が `goal: G0issue183` である。
- **ケース単位の除外（`cases.excluded`）と作らない区分（`cases.not_built`）の提案は無い。** 5件すべてを移す。区分は人間だけが決めるものであり、本設計は提案すら要らないという判断である。
- 5件が使わない原典の表面は、この issue の範囲の外として作らない。対象は、`counter`、通常の `histogram`（Codahale の蓄積型）、`measureMemory`、`measureGc`、`gc`（`System.gc()` の委譲）、メモリと GC とファイルディスクリプタに絞った報告である。これらは JVM の管理 Bean とガベージコレクションに固有のものを含むが、ケースの除外ではないため、人間の承認を要する提案としては上げない。将来に別の Spec が必要としたときに、その issue で扱う。

### この issue（#299）の台帳への影響

行番号は `origin/main` の `ed9fba5d5bb071c6ac32373e2d50b871d087bb3d` の `ledger/nodes.yaml` である。

- **`cases` は動かない。** 対象ノード（33460〜33486行）は `total: 5`、`ported: 5`、`passing: 5` のままである。出力の形を確かめるテストは原典の5ケースの検査ではなく、原典外の単体テストだからである。
- **`verified_against: v2.0.0` は付いたままである。** 対象ノードの33486行に既にあり、付け直しも書き換えもしない。
- したがって第2段で `ledger/nodes.yaml` を触る必要は無い。

## 8. 複数スレッドの実行器・dispatcher との関係

5件を1件ずつ確かめた結果、**[Issue #234](https://github.com/j5ik2o/fraktor-ng-rs/issues/234)（1つの ActorSystem を複数のスレッドで回す実行器。段の issue は #250〜#260）の前提になるケースは無い。** 根拠は、原典の `MetricsKit` が ActorSystem を起動しない設計であること（原典のコメントにも、計測対象が ActorSystem を必要としない場合があるため拡張にしなかったと書かれている）、5件すべてが単一スレッドの同期の呼び出しで完結すること、である。レポーターの定期報告だけがスレッドを使うが、これはレポーター自身の標準ライブラリのスレッドであり、アクターの dispatcher ではない。そのライフサイクルは §6 の Quint で検査するが、実行器の段を前提にはしない。したがって「#234 の段X の後」に分けるケースは無く、5件すべてを先に進められる。

## 9. 影響

- 変更は `crates/fraktor-testkit` の追加だけであり、既存の公開 API は変えない。`fraktor-actor` は変更せず、kernel の no_std は維持される。
- 新しい依存は2件の提案を含む（`hdrhistogram` と、unix 限定の `libc`）。どちらも `fraktor-testkit` のみに足す。第2段では、依存を足した後の CI と同じ検査を通す。検査の列挙は、`cargo fmt --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`、台帳の Python テストに加え、新規 `.rs` ファイルのライセンスヘッダーの検査（`license_headers.py --check --manifest docs/design/issue-206-file-origin.tsv`）、kernel の no_std の検査（`cargo check -p fraktor-actor --no-default-features --target thumbv7em-none-eabihf`）、`syn-check`（`type-per-file` を含む）、`npm run check:aislop`、および §6 の Quint の検査である。
- ケース4は約1,737万回の記録を行うため、テスト全体の実行時間が延びる。第2段では所要時間を測り、CI の制限に触れる場合は回数を減らさず、テストの分割ではなく報告として上げる。

## 10. 第2段の進め方（設計レビューが通ってから）

1. Claude Opus の担当が、本設計の対応表どおりに `metrics_kit_spec.rs` の5件と PBT を書き、red を確認してブランチに置く（進め方の変更、2026-11-26）。
2. 実装は red のテストを通す範囲だけを書き、テストは書き換えない。テストが誤りだと思った場合は、実装でごまかさず、判断の依頼として中将へ上げる。
3. あわせて、原典外の単体テストとして、タイマーのパニック時の停止（`catch_unwind` の後にタイマーが停止済みであり、2回目の `stop()` が `false` を返すこと）、種類の衝突のエラー、ヒストグラムの境界値（バケット上界ちょうど、バケット上界+1、負の値、および追跡可能な最大値+1 が記録できること）、`update_with_count` の負の個数と 0 の個数、未停止のタイマーの `elapsed()` 等が `None` であること、ファイルディスクリプタ登録の衝突時にレジストリが変わらないことを固定する。
4. 台帳の対象ノードを green に更新し、`ledger/tools` の検査を通す。Quint は §6 の `console_reporter.qnt` を書き、Quint の検査を通す。
5. CI と同じ検査をすべて流し、PR をドラフトから外す。マージはしない。

## 11. 第1段で動かしたコマンドと結果

- Mac mini の専用フォルダ `~/muse-work/fraktor-ng-rs-183` に `git@github.com:j5ik2o/fraktor-ng-rs.git` を clone し、main（`b591452`）から作業ブランチを切った。
- 原典は、同じ Mac mini にある Pekko v2.0.0 の取得済みコピー（`~/muse-work/pekko-v2.0.0`）で `MetricsKitSpec.scala` と metrics パッケージの全ファイルを読んだ。
- Rust のビルドとテストは、第1段（設計のみ）では実行していない。

## 12. 設計レビューへの回答表

独立レビュー（判定は要修正、高5件・中4件・低3件）への回答である。各指摘を一次情報（原典のソース、`hdrhistogram` クレートについてレビュー担当が行った実測の記述、リポジトリの決まりと CI の構成）で裏取りしたうえで、直す・別件・採らないのいずれかにした。採らなかった指摘は無い。

| 指摘 | 対応 | 理由と直した場所 |
|:---|:---|:---|
| 高-1 ヒストグラムの上限・溢れ仕様がクレートと原典と矛盾する | 直す | レビュー担当の実測（`record(100_000_001)` が成功し、範囲外になるのはバケット上界 `134_217_728` から）を受け入れ、原典の Java 版も同じ仕組みであることを確かめた。記録できる上限はバケット上界であることを I4・§1 のケース5・§4・P3・§10 に明記し、境界テストをバケット上界ちょうど／バケット上界+1 と原典の `10_000_000_000` に改めた。追跡可能な最大値で厳密に弾く狭めは、原典準拠を優先して採らない。`update_with_count` の負の個数（エラー）・0 の個数（no-op）・件数の加算の溢れ（事前検査して記録しない）を §4 に定義し、`i64::MAX` 級の最大値でも構築が成功する事実に合わせて §4 の説明を直した。 |
| 高-2 `measure_file_descriptors` と `hdr_histogram` の登録キーの誤り | 直す | 原典 `MetricsKitOps.scala` で、接頭辞が引数のキーに `"file-descriptors"` を繋いだものであること、`hdrHistogram` が `(key / "hdr-histogram")` で登録することを確かめた。ケース1の実際に登録される3キーを全文で書き、登録キーの生成規則を §2 の判断4・§5 の P6 に明記した。 |
| 高-3 `timed_with_known_ops` のパニック時の停止が Rust と矛盾する | 直す | Rust ではパニックの巻き戻し中に後続のコードは実行されず、明示の呼び出し順序では保証できない指摘が正しい。非公開の RAII ガードの `Drop` で `stop()` を呼ぶ形に §2 の判断2を書き換え、`UnwindSafe` の制約を課さないことと、パニック時の停止を単体テストで固定することを §10 に書いた。 |
| 高-4 定期報告スレッドと公開 API・Quint 不採用判断の矛盾 | 直す | レジストリを `Arc<Mutex<BTreeMap<String, Metric>>>` の共有にし、`registered_metrics` は借用ではなく複製したスナップショットを返す形に改めた。`with_output` 構築子、`Condvar::wait_timeout` による即時の停止、`shutdown_metrics` の冪等性、`Drop` での停止手順を §2 に明記した。定期報告スレッドのライフサイクルは並行の状態遷移であり既存の `watch_handle.qnt` と同型であるため、§6 を「`console_reporter.qnt` を書く」に改めた。 |
| 高-5 ファイル配置が `syn-check`（`type-per-file`）に違反する | 直す | リポジトリの決まり（1ファイルに `pub` の型は1つまで）と CI の `syn-check` の実行を確かめた。設定の3型を `metrics_kit_settings.rs`・`metrics_reporter_kind.rs`・`console_reporter_settings.rs` の3ファイルに分け、`metrics_kit.rs` から再エクスポートする形に §2 のファイル表を直し、`file_descriptor_metric_set.rs` の公開型が `FileDescriptorMetricSet` の1つだけであることも明記した。 |
| 中-1 ファイルディスクリプタの重複登録・原子性、`RLIM_INFINITY`、Windows CI と P6 の不整合 | 直す | 原典の `registerAll` の意味（再呼出は同種でもエラー）を確かめ、3キーの事前全件検査による原子性を判断4と I8 に書いた。上限は `u64` のまま扱い `RLIM_INFINITY` を負の値にしないこと、Linux では `/proc/self/fd` を優先することを §2 に書いた。CI は Linux と Windows であり macOS は無いこと、Windows でもケース1をスキップしないこと、非 unix の値の表現（`Option<u64>` の `None`、比率は `NaN`）を §2 に確定させ、P6 の unix 限定を外して空のレジストリ前提に直した。 |
| 中-2 `elapsed()` の未停止時の仕様の欠落 | 直す | 原典の `elapsedTime` は未停止では負の値になり、`Duration` では表せないことを確かめた。未停止では `elapsed()`・`avg_duration_nanos()`・`ops_per_second()` のいずれも `None` を返す形に公開 API と I5 を改めた。 |
| 中-3 `MetricKey` 整形の「空白」の範囲と P1 の対象 | 直す | 原典の置換は半角スペース（U+0020）だけであることを確かめ、I1 に明記した。P1 は `from_string` の単一部品にだけ主張し、連結したパスは部品ごとに整形済みである主張に直した。 |
| 中-4 `add_slice` の加算溢れと P2 の参照実装 | 直す | `add_slice` の内部の合計と P2 の参照実装のどちらも `i64` の回り込む加算（`fold(0i64, i64::wrapping_add)`）で定義することを §4 と P2 に明記した。 |
| 低-1 `reporters` の重複・空、`verbose` の百分位分布、`PrettyDuration` の再利用 | 直す | 原典の `contains("console")` の意味（重複してもレポーターは最大1台、空なら0台で報告は何もしない）、`verbose` が `true` のとき百分位の分布を含めること、タイマーの報告の整形に実装済みの `PrettyDurationInput` を再利用することを §2 の設定の項に書いた。 |
| 低-2 第2段の CI 検証チェックリストの補完 | 直す | §9 の検査の列挙に、ライセンスヘッダーの検査、kernel の no_std の検査、`syn-check`、`npm run check:aislop`、Quint の検査を足した。 |
| 低-3 `hdr_histogram` の既存キー再取得と引数検証の順序 | 直す | 原典の `getOrRegister` の遅延評価の意味を確かめ、導出したキーで先にレジストリを照会し、同種の既存ハンドルがあれば引数を検証せずに返すことを §2 の判断5に明記した。 |

内訳は、直す12件、別件0件、採らない0件である。新しい依存2件（`hdrhistogram` を `default-features = false` で、`libc` を unix 限定で）の採用は妥当であるというレビューの結論に従い、§2 の依存の設計は維持した。

テストの担当が足した面：`Metric` の列挙子 `AveragingGauge`・`HdrHistogram`・`KnownOpsInTimespanTimer`（`#[derive(Clone)]`。ファイルディスクリプタの列挙子は実装の担当が決める）、`HdrHistogram::min` と `HdrHistogram::max`（原典 `getMinValue`・`getMaxValue`、空では 0）、`impl Default for AveragingGauge`（`new` と同じ値）。理由：ケース2の暗黙の検査（登録されたタイマーの件数と停止）と P3 の最小・最大を書くため。また、引数の無い `new` には `Default` が無いと clippy の `new_without_default` が `-D warnings` で落ちるため（中将の判断、2026-11-27）。

追記（テストの担当による `console_reporter.qnt` のモデルの作り直しへの追従）：上記の回答表は当時の対応の記録であり、書き換えない。作り直し後のモデルでは、状態は `Thread`・`Handle`・`Call` の3つに分かれ、「停止要求済み」という状態は無く、停止の保証の境界は不変条件 `noReportOnceACallReturned` で表される。§6 の記述は、作り直し後の状態と遷移に合わせて直した。高-4 の理由に書いた `Condvar::wait_timeout` は当時の設計文面であり、実装は述語つきの `wait_timeout_while` であるため、§2 の記述は実態にそろえた。

## 13. 改訂（2026-11-28）

issue #299 の元帥の点検（2026-11-27）の重い指摘「コンソールの出力の形が原典と違う」に合わせ、§2 の「レポーターと報告の形」を原典 `PekkoConsoleReporter` の出力の形に書き直した。コンソールの出力は利用者の目に触れる振る舞いであり、`pekko-semantics-first.md` により原典の意味論を優先するためである。行番号は `origin/main` の `ed9fba5d5bb071c6ac32373e2d50b871d087bb3d` と Pekko v2.0.0（`9fbd2283b1cf701c90a1223bf621942c191db61f`）のものである。

変えた点は次のとおりである。

- 種類ごとの見出し（80桁のバナー）、種類の順、1項目1行の書式、キーの順序、空行の位置を、原典の行（`PekkoConsoleReporter.scala` 38〜175行）に対応づけて書いた。
- 原典のコードと、原典が使う Dropwizard 4.2.40 と HdrHistogram 2.2.2 のコードから出力の例を作って載せた。`HdrHistogram` の9行と百分位の分布は、原典が使う HdrHistogram 2.2.2 を JVM（Temurin 26）で動かして確かめた。Rust 側は `hdrhistogram` 7.6.0 で同じ入力を記録し、百分位の表の値と件数が一致することを確かめた。違ったのは12桁の百分位の2行だけで、これは Java の HALF_UP と Rust の偶数丸めの違いであり、同じ関数を置いて同じにする（§2）。
  - 追記（§14 の改訂）：ここで「一致」と書いたのは、§2 の独自の再構成（`iter_recorded()` の列から原典の刻みの規則で組み立てたもの）の値と件数である。クレートの `iter_quantiles(5)` そのものは一致しない。0〜4 の5件で、クレートは14行、原典は15行であり、百分位 0.6 と 0.8 の行の値と件数が違い、0.825 の行がクレートに無い（§2「ヒストグラムの値の求め方」）。原典の刻みは `PercentileIterator.java` 78〜91行、表の書式は `AbstractHistogram.java` 1789〜1790行である。
- JVM の書式のうち Rust で同じにできるもの（`%d`、HALF_UP の固定小数、`NaN`・`Infinity`、UTF-16 順）は同じにし、できないものだけを違いとして1行ずつ挙げた。
- red のテストの担当のための面（`report` で足りるか、確かめる項目）を書いた。
- 出力の形は Quint にしない理由を §6 に1段落で足した。
- §7 に、台帳の `cases` と `verified_against` が動かないことを書いた。
- 出力の形を確かめるテストは原典外であるため、台帳の件数と `verified_against` を変えない。

issue #299 の「あわせて」の2点は、PR #306（`68a48a7`、コミット `0992b1e`）でマージ済みである。設計し直さない。

- 中：`metrics_kit_value.rs` の複製してから retain する形を `mem::take` にする — 済み。`0992b1e`
- 中：hdrhistogram の推移の依存（byteorder（Unlicense OR MIT）、num-traits、autocfg）を `docs/design/issue-206-license-policy.md` §4.3 に記録する — 済み。`0992b1e`

§12 は前のレビューの記録であり、書き換えない。

## 14. 改訂（2026-11-28、2回目）：§2 のレビューへの対応

§13 の改訂に対する別のレビューの一覧を、1件ずつ原典と実測で確かめてから直した。直した場所は §2「レポーターと報告の形」（書き直し）、§3 の I5、§6（出力の形の段落と `console_reporter.qnt` の見直し）、§7、§13 の追記、`crates/fraktor-testkit/specs/console_reporter.qnt`、新しい `docs/design/issue-299/`（出力の例を原典から導くスクリプト・駆動のプログラム・期待の出力）である。方針は `pekko-semantics-first.md` と元帥の決定（2026-11-28）による。Rust で同じ振る舞いを作れるものは原典どおりにし、直すのに公開 API の判断が要るものは §2「原典と違う点」に書いて報告に上げた。

| 指摘 | 対応 | 確かめたことと直した場所 |
|:---|:---|:---|
| 1-1 §2 の「以上」と「超える」の矛盾 | 採る | `AbstractHistogram.java` 1446行は `>=`。0〜4 の5件の 75% は3（`.expected` でも3）。§2「ヒストグラムの値の求め方」を「以上」にそろえた |
| 1-2 タイマーの4行と I5 と問い2 が両立しない | 採る | `KnownOpsInTimespanTimer.scala` 42〜46行と `PekkoConsoleReporter.scala` 128〜136行で、原典は未停止でも4行を出す。問い2を原典どおり（負の値を出す）に決め、I5 は公開の口の性質に限った（§2「未停止のタイマー」、§3） |
| 1-3 §13 の「一致」と §2 の「1刻みずれる」 | 採る | クレートの `iter_quantiles(5)` を実測し、14行対15行、0.6 と 0.8 の行の値と件数が違い 0.825 の行が無いことを確かめた。§2 を実測どおりに書き、§13 に追記した |
| 1-4 非 unix の値は NaN ではない | 採る（例外の種類は訂正） | 原典は非 unix で値を返さず例外を投げる。ただし例外は `NoSuchMethodException` ではなく `ClassCastException` である。`UnixOperatingSystemMXBean` は OpenJDK の共通のソースにあり `findVirtual` は成功し、`invokeExact` の引数の型変換で落ちる。スクリプトで確かめた。関わる行は `FileDescriptorMetricSet.scala` 35〜40・48〜53・61〜63行、`FileDescriptorMetricSetSpec.scala` 31〜32行（§2「非 unix のファイルディスクリプタ」） |
| 1-5 §7 の「goal は null」 | 採る | ed9fba5 の `ledger/nodes.yaml` 33485行は `goal: G0issue183`。§7 を時点つきの記述に直した |
| 1-6 行番号の宣言が原典とリポジトリを区別しない | 採る | §2 の冒頭に書き方を定め、原典はファイル名つき、リポジトリは「（ed9fba5）」つきに書き分けた |
| 2-1 出力の例の導き方が無い | 採る | `docs/design/issue-299/derive-console-reporter-examples.sh` を置いた（§2「出力の例の導き方」）。152行の呼び出しが2引数の `Double` 版（1737〜1740行）で刻みが5であることも書いた |
| 2-2 ファイルディスクリプタの3つを報告側が区別する口が無い | 採る | 非公開の読み `FileDescriptorReading` と `FileDescriptorGauge` の非公開の口を設計した。公開の形は変えない（§2「報告の作りと、確かめる口」） |
| 2-3 改行 | 採る | 原典は `%n`・`println` が `line.separator`、分布の表だけ `\n`。`-Dline.separator=$'\r\n'` で確かめた。Rust は `cfg(windows)` で CRLF、表は常に LF（§2「改行」） |
| 2-4 未停止のタイマーの期待の文字列 | 採る | §2「未停止のタイマー」と §15 の R10・R11・R13 |
| 3-1 ファイルディスクリプタの値を注入する口が無い | 採る | 読みを手で作って `render` を呼ぶ単体テストの口にした（§2、§15 の R3〜R7） |
| 3-2 item 4 の入力と、空の種類の見出しを出さない項目が無い | 採る | §15 の R20（入力つき）と R22 |
| 3-3 問い1が未決のまま例を固定している | 採る | 原典の規則（`PekkoConsoleReporter.scala` 64・171〜175行）で決め、原典の実行時クラスの名前を Scala 2.13.18 のコンパイル結果で確かめた（§2「見出しの名前」） |
| 3-4 item 13 の `ops/s` と `time`・`avg` の書式 | 採る | 経過0で `Infinity`、`%.4g` の指数表記を実測した。§15 の R10 は完全一致、R14 は1つの述語にした |
| 3-5 未停止のタイマーを確かめる項目が無い | 採る | §15 の R10・R11・R13 |
| 4-1 §6 の「同じ入力なら同じ文字列」が §2 と食い違う | 採る | 報告を「読む」と「書く」に分け、決定的なのは「書く」だけと §6 に書いた |
| 4-2 `report_and_clear_metrics` を Quint とテストのどちらで見るか | 採る | テストで見る（§6、§15 の R12）。あわせて、原典は報告の後に全部を消す（`MetricsKit.scala` 96〜99行）ので並行の登録の扱いが違うことを §2「原典と違う点」に書いた |
| 4-3 出力のロックと停止の順序が Quint に無い | 採る | `outputLock` と不変条件 `noStopHoldsTheOutput` を足した（§6） |

採らなかった指摘は無い。1-4 だけは、指摘の結論（非 unix では値が返らない）を採り、例外の種類と行番号を一次情報で直した。

## 15. テストの担当への申し送り

置き場所は2つである。単体テストは `crates/fraktor-testkit/src/metrics_kit/console_reporter_test.rs`（`console_reporter.rs` に `#[cfg(test)] #[path = "console_reporter_test.rs"] mod tests;`）で、非公開の読みを手で作って `render` を呼ぶ。結合テストは `crates/fraktor-testkit/tests/console_reporter_output_test.rs` で、公開の `ConsoleReporter::new(verbose).report(&metrics, &mut Vec<u8>)` を呼ぶ。どちらも原典外のテストであり、台帳の5ケースに数えない（§7）。

共通の約束を先に書く。

- `NL` は結合テストでは `if cfg!(windows) { "\r\n" } else { "\n" }`、単体テストでは `render` に渡した区切りである。百分位の分布の表の行（見出し・その後の空行・各行・`#[` の3行）は常に `"\n"` である。
- 期待の文字列の元は `docs/design/issue-299/console-reporter-examples.expected` の節（`=== ` の行の名前で引く）である。そこから次の3点だけを差し替える。(1) 独自の3種類（タイマー、ヒストグラム、平均ゲージ）の項目の順をキー順にする（原典はハッシュ順、Rust はキー順。§2「キーの順序」）。(2) タイマーの見出しを `-- KnownOpsInTimespanTimer ` と `-` 53個にする（`.expected` には駆動のプログラムの無名の子クラスの名前が出ている）。(3) 行の区切りを `NL` にする。
- 見出しの `-` の数は `76 - 名前の長さ` である。`AveragingGauge` は62個、`FileDescriptorMetricSet$$anon$1` と `$$anon$2` は45個、`FileDescriptorRatioGauge` は52個、`KnownOpsInTimespanTimer` は53個、`HdrHistogram` は64個。どれも改行を除いて80文字である。
- 表の「red」は、いまの実装（`console_reporter.rs` 49〜77行（ed9fba5）の `key: gauge value=...` の形）で落ちる項目である。「緑のまま」はいまの性質を固定する項目である。

| # | 口 | 入力 | 期待 | いま |
|:---|:---|:---|:---|:---|
| R1 | 結合 | 空の `BTreeMap` | 出力は `NL` ちょうど | red |
| R2 | 単体 | 上の6つの名前それぞれを先頭に持つ読み | 1行目は `"-- " + 名前 + " " + "-" × (76 - 長さ)`、改行を除いて80文字 | red |
| R3 | 単体 | `metrics-kit.avg-size` に `AveragingGauge { value: 2.5 }`、`metrics-kit.file-desc.file-descriptors.file-descriptors-{max,open,ratio}` に `Max(Some(1024))`・`Open(Some(42))`・`Ratio(0.041015625)` | `.expected` の `=== gauges: averaging gauge and file descriptors 42/1024` の節（15行。末尾は空行2つ） | red |
| R4 | 単体 | R3 からファイルディスクリプタの3つだけ | `=== gauges: file descriptors only` の節。見出しは `-- FileDescriptorMetricSet$$anon$2 ` と `-` 45個 | red |
| R5 | 単体 | `Max(Some(0))`・`Open(Some(42))`・`Ratio(f64::NAN)` | `=== gauges: file descriptors with limit 0` の節（`value = 0`、`value = NaN`） | red |
| R6 | 単体 | `Max(None)`・`Open(None)`・`Ratio(f64::NAN)`（非 unix） | 書かれたのは見出しの1行と `  metrics-kit.file-desc.file-descriptors.file-descriptors-max` + `NL` だけで、`render` は `Err`（`io::ErrorKind::Unsupported`）を返す | red |
| R6w | 結合 | `MetricsKit::measure_file_descriptors(&MetricKey::from_string("metrics-kit").child("file-desc"))` の後の `registered_metrics()` を `report` | `cfg(windows)`: R6 と同じ出力と `Err`。`cfg(unix)`: 見出しは `-- FileDescriptorMetricSet$$anon$2 `、開数と上限の値の行は `             value = ` の後が10進の数字だけ、比率の値の行は `Double.toString` の形（数字と `.` を含み、`NaN` ではない） | red |
| R7 | 単体 | `...-ratio` に `Ratio(f64::NAN)` だけ | `=== gauges: the ratio alone on a non-unix bean` の節。見出しは `-- FileDescriptorRatioGauge ` と `-` 52個 | red |
| R8 | 結合 | `metrics-kit.a-empty`（何も足さない）、`b-tiny`（0 を9999個と 1）、`c-quarter`（0 を3個と 25）、`d-whole`（0 と 4）、`e-eighth`（0 を7個と 1）の `AveragingGauge` | ゲージの節は `.expected` の `=== averaging gauges` と同じ（`NaN`、`1.0E-4`、`6.25`、`2.0`、`0.125`）。平均ゲージの節はキー順で `avg = NaN`、`avg = 0.00`、`avg = 6.25`、`avg = 2.00`、`avg = 0.13` | red |
| R9 | 結合 | `metrics-kit.` + U+E000 に 1、`metrics-kit.` + U+1F600 に 2 の `AveragingGauge` | 2つの節のどちらも U+1F600 のキーが先（ゲージの節は `.expected` の `=== key order` と同じ。平均ゲージの節もキー順で U+1F600 が先） | red |
| R10 | 単体 | タイマーの読み `(ops, elapsed_nanos)` を `metrics-kit.t1-ms` (10, 1234567)、`t2-zero` (10, 0)、`t3-unstopped` (10, −123456789012345)、`t4-no-ops` (0, 1000)、`t5-no-ops-zero` (0, 0) | キー順に、`=== timers` の節の各4行（例: `              time = 1.235 ms`、`             ops/s = 8100.01`、`               avg = 123.5 μs`、`             ops/s = Infinity`、`              time = -1.235e+14 ns`、`             ops/s = -0.00`、`               avg = -1.235e+13 ns`、`               avg = 1.068e+05 d`、`             ops/s = NaN`）。見出しは `-- KnownOpsInTimespanTimer ` と `-` 53個 | red |
| R11 | 単体 | `metrics-kit.t6-unstopped-no-ops` に (0, −123456789012345) | 見出し、キー、`               ops = 0`、`              time = -1.235e+14 ns`、`             ops/s = -0.00` まで書き、`render` は `Err`（`io::ErrorKind::InvalidInput`）を返す（`=== a timer whose avg is Long.MinValue`） | red |
| R12 | 結合 | `Arc<MetricsKit>` を `with_output` で作り、出力先の `Write` が最初の `write` で同じ kit に `metrics-kit.late` の `AveragingGauge` を登録する。先に `metrics-kit.avg-size` を登録してから `report_and_clear_metrics()` | 後の `registered_metrics()` のキーは `["metrics-kit.late"]` だけ（報告の間の登録は残る。原典との違いは §2「原典と違う点」）。kit への参照は `Weak` で持たせ、循環させない | 緑のまま |
| R13 | 結合 | `KnownOpsInTimespanTimer::new(10)` を止めずに `Metric::KnownOpsInTimespanTimer` で `report` | 4行が `               ops = 10`、`time` の行が `^              time = -[1-9]\.[0-9]{3}e\+18 ns$`、`             ops/s = -0.00`、`avg` の行が `^               avg = -[1-9]\.[0-9]{3}e\+17 ns$` に合う（開始時刻の読みは UNIX 時刻のナノ秒。2286年まで指数は18） | red |
| R14 | 結合 | `KnownOpsInTimespanTimer::new(10)` を `stop()` してから `report` | `ops = 10`。`time` と `avg` の行は、`= ` の後が「数字4つと `.` 1つからなる値（`1.235`、`12.35`、`123.5`、`0.000` の形）、空白、`ns`・`μs`・`ms`・`s`・`min`・`h` のいずれか」で、`ops/s` の行は `= ` の後が `[0-9]+\.[0-9]{2}` か `Infinity`（経過0のとき）。この3つを1つの述語にして1つの `assert!` で確かめる | red |
| R15 | 結合 | `HdrHistogram::new(100_000_000, 4, "ns")` に 0〜4 を1回ずつ、`verbose = true` | `=== HdrHistogram: 0..4, verbose` の節。9行と、分布の見出し・空行・15行・`#[` の3行。分布の部分の区切りは常に `"\n"`、それ以外は `NL` | red |
| R16 | 結合 | R15 と同じで `verbose = false` | 9行だけで、`       Value     Percentile TotalCount 1/(1-Percentile)` の行が無い。`count` と `median` の行も無く、`stddev` の行に単位が無い | red |
| R17 | 結合 | 空の `HdrHistogram`、`verbose = true` | `=== HdrHistogram: empty, verbose` の節（`min = 0 ns` … `99.9% <= 0 ns`、分布は見出し・空行・`#[` の3行だけ） | red |
| R18 | 結合 | 1 を7件と 2 を1件 | `=== HdrHistogram: seven 1s and one 2` の節（`mean = 1.13 ns`、`stddev = 0.33`、`75% <= 1 ns`、`95% <= 2 ns`） | red |
| R19 | 結合 | ケース4の入力（0〜1,579,331 を11回と 1,579,331）、`verbose = false` | `=== HdrHistogram: case 4 input, not verbose` の節（`max = 1579391 ns`、`mean = 789666.04 ns`、`stddev = 455913.92`、`75% <= 1184511 ns` …）。約1,737万回の記録のため、このファイルで1本だけにする | red |
| R20 | 単体 | `metrics-kit.avg-size` に `AveragingGauge { value: 2.5 }`、`metrics-kit.hist.hdr-histogram` に 5 を1件のヒストグラム、`metrics-kit.timer` に (10, 1234567) | 種類の順がゲージ、タイマー、ヒストグラム、平均ゲージ。`=== kinds in order` の節と同じ（タイマーの見出しだけ差し替え） | red |
| R21 | 単体 | R15 の読みを区切り `"\r\n"` で `render` | `=== crlf: HdrHistogram 0..4, verbose` の節の `<CR>` を `"\r"` にしたもの | red |
| R22 | 結合 | `HdrHistogram` だけを登録 | `-- ` で始まる行がちょうど1つ（`-- HdrHistogram`）で、`-- AveragingGauge`・`-- KnownOpsInTimespanTimer`・`-- FileDescriptor` で始まる行が無い。出力の末尾は `NL` + `NL` | red |
| R23 | 結合 | いつも失敗する `Write` | `report` は `Err` を返す | 緑のまま |

書式の関数（`Double.toString`、HALF_UP の固定小数、負を受ける `%.4g`）は、上の項目の期待の文字列で決まる。別に関数だけを確かめる項目を足すなら、`0.125` の2桁が `0.13`、`-0.0` の2桁が `-0.00`、`f64::INFINITY` の2桁が `Infinity`、`1.0e-4` の `Double.toString` が `1.0E-4`、`2.0` が `2.0`、−1.23456789012345e14 の4桁の `%g` が `-1.235e+14` を入れる。

既存のテストとの関係。`metrics_kit_edge_case_test.rs` の `an_unstopped_timer_has_no_elapsed_time`（公開の口が `None`）と、`metrics_kit_review_followup_test.rs` の `report_and_clear_keeps_the_metrics_when_the_output_fails` は、この設計でも変わらない。報告の文字列の中身を確かめている既存のテストは、`output.text().matches("metrics-kit.avg-size")` のようにキーの文字列だけを見ているので、形が変わっても落ちない見込みである（第2段で確かめる）。
