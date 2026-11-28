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

本項の行番号は、断らない限り `origin/main` の `ed9fba5d5bb071c6ac32373e2d50b871d087bb3d` の行番号であり、原典の行番号は Pekko v2.0.0（`9fbd2283b1cf701c90a1223bf621942c191db61f`）の `testkit/src/test/scala/org/apache/pekko/testkit/metrics/reporter/PekkoConsoleReporter.scala` の行番号である。`PekkoConsoleReporter.scala` の本文は、手元の Pekko の写し（`9a54d1294c`）と字句の近代化（`Gauge[?]` など）以外で同じであり、行番号も同じであることを確かめた。原典が継承する Dropwizard の版は v2.0.0 の `project/Dependencies.scala` の metrics-core 4.2.40 であり、HdrHistogram は 2.2.2 である。

#### 原典の構造と Rust の対応

原典の `report`（38〜60行）は、8つの種類を固定の順に印字する。標準の5種類（ゲージ、カウンタ、ヒストグラム、メーター、タイマー、46〜51行）と、独自の3種類（`KnownOpsInTimespanTimer`、`HdrHistogram`、`AveragingGauge`、53〜56行）である。1つの種類について `printMetrics`（62〜71行）が、空でなければ見出しを1行出し（64行）、項目ごとに `"  " + key` の行（66行）と値の行を出し、種類の終わりに空行を1つ出す（69行）。報告の最後にもう1つ空行を出して flush する（58〜59行）。したがって、1つの種類だけを持つ報告の末尾には空行が2つ続き、空のレジストリの報告は空行1つだけになる。

見出しは `printWithBanner`（159〜168行）が作る。`"-- " + simpleName(先頭の項目の実行時クラス)` の後ろに空白を1つ置き、`-` を並べて行の長さをちょうど `ConsoleWidth = 80`（36行）にする。`simpleName`（171〜175行）はクラス名の最後の `.` の後ろを取る。名前は種類ではなく「その種類の先頭の項目の実行時クラス」から取る（64行）ため、同じ種類でも先頭の項目のクラスが変われば名前が変わる。

Rust の `Metric`（`crates/fraktor-testkit/src/metrics_kit/metric.rs` の23〜31行）は4種類だけを持つ。原典の8つのうち、Rust が印字するのは次の4つであり、この順に出す。

| 順 | 原典の種類 | Rust の列挙子 | 原典の行 |
|:---|:---|:---|:---|
| 1 | ゲージ（標準） | `AveragingGauge` と `FileDescriptorGauge` の両方 | 47行、`printGauge` 85〜87行 |
| 2 | `KnownOpsInTimespanTimer` | `KnownOpsInTimespanTimer` | 54行、128〜136行 |
| 3 | `HdrHistogram` | `HdrHistogram` | 55行、138〜153行 |
| 4 | `AveragingGauge` | `AveragingGauge` | 56行、155〜157行 |

- **`AveragingGauge` は2回出る。** 原典の `AveragingGauge` は `Gauge[Double]` でもあるため、ゲージの種類（`value = ...`）と `AveragingGauge` の種類（`avg = ...`）の両方に現れる。同じキーの行が2回出るのは原典どおりであり、Rust も同じにする。
- **`FileDescriptorGauge` はゲージの種類にだけ出る。** 原典では3つの `Gauge`（開数・上限・比率）であり、独自の種類には入らない。
- **カウンタ、Codahale のヒストグラム、メーター、タイマーは出さない。** Rust のレジストリにこれらの種類が無いためであり、原典で空の種類を出さない規則（62〜63行）と同じ結果になる。

#### 種類ごとの行

値の行は、原典の書式文字列をそのまま写す。空白の数は原典の文字列のとおりである。

**ゲージ**（85〜87行）: `             value = {値}`（`value` の前に空白13）。ファイルディスクリプタの開数と上限は整数として（`f64` を `u64` に直して）出し、比率と平均ゲージは Java の `Double.toString` の規則で出す（下の「JVM の書式との違い」）。値が無い非 unix の開数と上限は `NaN` と出す。

**`KnownOpsInTimespanTimer`**（128〜136行）: 4行。

```
               ops = {件数}
              time = {整形した時間}
             ops/s = {2桁の小数}
               avg = {整形した時間}
```

`time` は `elapsedTime.nanos.pretty`（133行）、`avg` は `avgDuration.nanos.pretty`（135行）であり、どちらも `PrettyDuration` の4桁の有効数字と単位（`ns`・`μs`・`ms`・`s`・`min`・`h`・`d`）で書く。`ops/s` は1秒あたりの回数を2桁の小数で書く（134行）。

**`HdrHistogram`**（138〜153行）: 9行。`count` と `median` は出さない。`count` と `median` があるのは Codahale のヒストグラム用の `printHistogram`（89〜102行）であり、Rust のレジストリにその種類は無い。

```
               min = {整数} {単位}
               max = {整数} {単位}
              mean = {2桁の小数} {単位}
            stddev = {2桁の小数}
              75% <= {整数} {単位}
              95% <= {整数} {単位}
              98% <= {整数} {単位}
              99% <= {整数} {単位}
            99.9% <= {整数} {単位}
```

`stddev` だけ単位を付けない（144行）。`verbose` が `true` のときは、この9行の後ろに百分位の分布を続ける（151〜152行）。

**`AveragingGauge`**（155〜157行）: `                avg = {2桁の小数}`（`avg` の前に空白16）。

#### キーの順序

原典の順序は、種類によって2つに分かれる。

- **標準のゲージはキー順である。** `ScheduledReporter.report()` が渡す `getGauges(filter)` は、`MetricRegistry` が `ConcurrentHashMap` から `TreeMap` を作って返す（metrics-core 4.2.40 の `MetricRegistry.java` の531〜542行）。順序は `String` の自然順、すなわち UTF-16 の符号単位の辞書順である（`java.lang.String.compareTo`）。
- **独自の3種類はキー順ではない。** `PekkoMetricRegistry.filterFor`（`MetricsKit.scala` の212〜216行）は `registry.getMetrics` をそのまま辿る。`MetricRegistry.getMetrics` は `ConcurrentHashMap` を包んだ `Map` を返す（`MetricRegistry.java` の605〜608行）ため、`KnownOpsInTimespanTimer`・`HdrHistogram`・`AveragingGauge` の3つの種類の項目の順序は、ハッシュ表の並びであり、キー順でも登録順でもない。原典のテストも文書もこの順序を約束していない。

Rust では4つの種類のすべてをキー順に出す。これは元帥の指示（項目の並びはキー順）に従うものであり、原典が約束していない順序をキー順に確定させる選択である（`pekko-semantics-first.md` の「許している範囲の中で選べる振る舞いを狭めることはずれにしない」に当たる）。キーの比較は UTF-16 の符号単位で行う。Rust の `BTreeMap` は UTF-8 のバイト順なので、U+10000 以上の文字と U+E000〜U+FFFF の文字が同じレジストリに混ざると `BTreeMap` の順は Java と変わる（U+1F600 と U+E000 は、Java では U+1F600 が先、UTF-8 では U+E000 が先）。そのため、種類に分けた後にキーを UTF-16 の符号単位で並べ直す。

#### 原典の行との対応

| 出力 | 原典 |
|:---|:---|
| 種類の順と空行 | `PekkoConsoleReporter.scala` 46〜51、53〜56、58〜59行 |
| 見出しの条件と形（80桁） | 62〜64行、159〜168行、36行 |
| 見出しの名前 | 171〜175行 |
| キーの行（空白2つ） | 66行 |
| 種類の終わりの空行 | 69行 |
| ゲージの値 | 85〜87行 |
| `KnownOpsInTimespanTimer` の4行 | 128〜136行 |
| `HdrHistogram` の9行 | 138〜149行 |
| `HdrHistogram` の分布（`verbose`） | 151〜152行 |
| `AveragingGauge` の1行 | 155〜157行 |
| rate と duration の単位 | `ScheduledReporter.java`（metrics-core 4.2.40）143〜146、304〜318、328〜331行 |
| 百分位の刻みと表の書式 | `AbstractHistogram.java`（HdrHistogram 2.2.2）1737〜1740、1769〜1777、1789〜1790、1822〜1831行、`PercentileIterator.java` 50〜58、86〜91行 |
| 値の分布の元（コピー） | `Histogram.java` 108〜112行、`HdrHistogram.scala` の58行（`getData`） |

rate と duration の単位は、原典が `ScheduledReporter` に rate を `TimeUnit.SECONDS`、duration を `TimeUnit.NANOSECONDS` で渡す（33〜34行）ことから決まる。`ScheduledReporter` は rate の係数を `SECONDS.toSeconds(1) = 1`、名前を `"seconds"` の末尾を落とした `"second"` とし（143〜144行、328〜331行）、duration の係数を `NANOSECONDS.toNanos(1) = 1`、名前を `"nanoseconds"` とする（145〜146行）。変換はどちらも1倍・1除算なので値は変わらない（312〜318行）。この2つの文字列（`events/second`、`calls/second`、`nanoseconds`）はメーターと Codahale のタイマーの行にだけ出る。Rust のレジストリにその2種類は無いため、Rust の出力には現れない。`KnownOpsInTimespanTimer` の単位は `PrettyDuration` のもの、`HdrHistogram` の単位は利用者が渡す `unit`（例: `ns`）である。

#### 出力の例

ゲージの種類と `AveragingGauge` の種類（登録は `metrics-kit.avg-size` の平均ゲージと、`metrics-kit.file-desc.file-descriptors.file-descriptors-{max,open,ratio}` の3つ。値は開数42、上限1024、比率0.041015625、平均2.5）。

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

ゲージの見出しが `-- AveragingGauge` なのは、先頭のキー `metrics-kit.avg-size` の項目が `AveragingGauge` だからである（64行）。ファイルディスクリプタの3つは同じ見出しの下に入る。この報告の末尾には空行が2つ続く。

`HdrHistogram`（ケース4の入力。`highest = 100_000_000`、4桁、単位 `ns`、0〜1,579,331 を11回と 1,579,331 をもう1回、合計 17,372,653 件。`verbose = false`）。

```
-- HdrHistogram ----------------------------------------------------------------
  metrics-kit.hist.hdr-histogram
               min = 0 ns
               max = 1579391 ns
              mean = 789666.04 ns
            stddev = 455913.92
              75% <= 1184511 ns
              95% <= 1500415 ns
              98% <= 1547775 ns
              99% <= 1563583 ns
            99.9% <= 1577791 ns
```

`HdrHistogram`（0〜4 の5件、`highest = 100_000_000`、4桁、単位 `ns`、`verbose = true`）。9行の後ろに分布が続く。

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

分布の書式は、表の見出しが `%12s %14s %10s %14s\n\n`（`AbstractHistogram.java` の1777行）、行が `%12.{桁}f %2.12f %10d %14.2f\n`（1789行）、100%の行が `%12.{桁}f %2.12f %10d\n`（1790行）、末尾の3行が1822〜1831行である。`{桁}` は有効数字の桁数（この例では4）。刻みは、原典が `outputPercentileDistribution(output, 1)` と呼ぶ（152行）ため、引数の `1` はスケーリング比（`Double` の1.0）であり、刻みは既定の「半距離あたり5本」になる（1737〜1740行）。表の行は、記録した値の昇順に累積件数を取り、各刻みの百分位 `p` について、累積件数が `p/100 × 全件数` 以上になる最初の値の階級の上端（`highestEquivalentValue`）を出す（`PercentileIterator.java` 86〜91行、`AbstractHistogramIterator.java` 133〜135行）。全件数を使い切った行を出したら、その同じ値で100%の行を出して止める（`PercentileIterator.java` 50〜58行）。

9行の百分位（75・95・98・99・99.9）は、同じ累積から、要求百分位を1ulp下げて `ceil(p/100 × 全件数)`（1未満は1）を初めて超える値の上端として求める（`AbstractHistogram.java` 1432〜1460行）。

Rust の `hdrhistogram` 7.6.0 の `iter_quantiles(5)` は、この刻みの進み方が原典と一致しない（0〜4 の5件で、原典は 0.6 を値2で出すが、クレートは値3で出し、以降が1刻みずれる）。そのため分布はクレートの `iter_quantiles` を使わず、`iter_recorded()` の（値、件数）の列から上の規則で組み立てる。`value_at_percentile` も使わず、同じ累積から求める（原典は要求百分位を1ulp下げてから数えるため）。9行の `mean` と `stddev` も、クレートの `mean()`・`stdev()` ではなく、原典の式（各階級の中点 × 件数の和を全件数で割る、標準偏差は同じ中点からの偏差の二乗和）で求める（`AbstractHistogram.java` 1385〜1420行）。

原典は `getData` でヒストグラムを複製してから読む（`HdrHistogram.scala` の58行）。Rust も同じにし、`Mutex` の中では `hdrhistogram::Histogram` を複製するだけにして、読み取りと出力は複製の上で行う（出力の間、ロックを保持しない）。そのため `HdrHistogram` に非公開の `with_snapshot`（複製を渡して閉包を呼ぶ）を置き、いまの `report_values` と `PERCENTILES`（`hdr_histogram.rs` の27行、165〜173行）は置き換える。`min`・`max`・`unit`・`count` の公開メソッドはそのまま残す。

#### JVM の書式との違い

次のものは同じ出力にする。

- `%d`（10進、桁区切りなし）は Rust の `u64`・`i64` の `Display` と同じ。
- `%2.2f` は2桁の固定小数である。Java の `Formatter` は `Double.toString` の最短桁を HALF_UP で丸める（`0.125` は `0.13`、`2.675` は `2.68`、`1.005` は `1.01`）。Rust の `{:.2}` は値の2進の正確な値で偶数丸めするため `0.12`・`2.67`・`1.00` になり、違う。報告では、最短桁を取って HALF_UP で丸める関数を1つ置き、`%2.2f` と `%2.12f` の両方に使う（`fraktor-actor` の `pretty_duration_format::format_g` と同じ作り。あれは非公開なので testkit に置く）。
- `NaN`・`Infinity` は Java の綴り（`NaN`、`Infinity`、`-Infinity`）で出す（Rust の `inf` は使わない）。
- キーの順は UTF-16 の符号単位で並べ直す（上のとおり）。

Rust で同じにできない違いは次の4つである。

| 違い | 理由 |
|:---|:---|
| ファイルディスクリプタの開数と上限の型 | 原典は `Long` を `%s` で出す。Rust の `FileDescriptorGauge::value` は3つとも `f64`（`file_descriptor_gauge.rs` の51〜61行）なので、2^53 を超える値（`RLIM_INFINITY` など）は丸まる。また、値が無い非 unix では `NaN` と出る（原典は JVM の `Long` を返す）。通常の値は整数として出すので同じになる。 |
| ゲージの値の `Double.toString` | 原典の `%s` は Java の `Double.toString` であり、`1e-3 <= abs(v) < 1e7` は小数、外は `1.0E-4` のような指数表記、整数値にも `.0` を付ける（`2.0`）。Rust 側で同じ規則を実装するが、最短の桁は Rust のものを使う。JDK 19 以降は同じ最短表現であり、JDK 18 以前は同じ値でも桁が増えることがある。この差は JDK の版による。 |
| `Locale` | 原典の `"...".format(...)` は JVM の既定ロケール（`Locale.getDefault(Locale.Category.FORMAT)`）を使うため、小数点が `,` のロケールでは Pekko も `,` を出す。Rust は常に `.` である。CI のロケールでは差は出ない。原典の百分位分布（`Locale.US`）と `PrettyDuration`（`Locale.ROOT`）はロケールを固定している。 |
| ファイルディスクリプタの見出しの名前 | 原典は先頭のゲージの実行時クラス名を使う（64行、171〜175行）ため、Scala の無名クラス（`FileDescriptorMetricSet$$anon$1`）になる。Rust に無名クラスは無い。`-- FileDescriptorGauge` とする（問い1）。 |

#### 確かめる口と、red のテストの項目

出力を確かめる口は、いまの `report(&self, metrics: &BTreeMap<String, Metric>, output: &mut dyn Write) -> io::Result<()>`（`console_reporter.rs` の49行）で足りる。`ConsoleReporter::new(verbose)`（38行）と `Vec<u8>` だけで、時計もスレッドも出力先の差し替えも要らない。`with_output` は `MetricsKit` 側に既にあり（`metrics_kit_value.rs` の168行）、`ConsoleReporter` には足さない。

材料は2つに分かれる。値が入力で決まるもの（`AveragingGauge` と `HdrHistogram`）は行をそのまま比べられる。OS と時計を読むもの（`FileDescriptorGauge` と `KnownOpsInTimespanTimer`）は値が実行のたびに変わるため、行の形だけを比べる。`FileDescriptorGauge` の3つは `FileDescriptorMetricSet::new().gauges()` で作れる。

テストの担当がそのまま書ける項目を挙げる。置き場所は `crates/fraktor-testkit/tests/` の新しいファイル（例: `console_reporter_output_test.rs`）とし、原典外のテストとして台帳の5ケースには数えない（§7）。

1. 空のレジストリ: 出力は `"\n"` ちょうど（1バイト）。
2. 見出し: 空でない種類の1行目は `"-- " + 名前 + " " + '-' × (76 - 名前の長さ)` で、全体が80バイト。名前は `AveragingGauge`・`FileDescriptorGauge`・`KnownOpsInTimespanTimer`・`HdrHistogram` のいずれか。
3. キーの行: `"  " + key` であり、前に空白が2つある。
4. 種類の順: ゲージ、`KnownOpsInTimespanTimer`、`HdrHistogram`、`AveragingGauge`。
5. `AveragingGauge` は2回出る: ゲージの種類に `value = 2.5`、`AveragingGauge` の種類に `avg = 2.50`。
6. ゲージの見出しは先頭のキーの項目で決まる: `metrics-kit.avg-size` が先頭なら `-- AveragingGauge`、ファイルディスクリプタのキーだけなら `-- FileDescriptorGauge`。
7. キーの順: 同じ種類の中で昇順。`metrics-kit.` + U+1F600 と `metrics-kit.` + U+E000 のキーを入れると、Java の順（U+1F600 が先）で出る。
8. ゲージの値の表記: `2.5`、空のゲージの `NaN`、和1・個数10,000 の `1.0E-4`、和25・個数4 の `6.25`、和4・個数2 の `2.0`（`2` ではない）。
9. `%2.2f` の HALF_UP: 和1・個数8 のゲージは `value = 0.125`、`avg = 0.13`（`0.12` ではない）。
10. `HdrHistogram` の9行: 0〜4 の5件で、上の例の9行と一致する。`count` と `median` の行が無いこと、`stddev` に単位が無いことも確かめる。
11. `mean` の HALF_UP: 1 を7件と 2 を1件で `mean = 1.13`。
12. 百分位: `75% <=` などの値は整数で、小数が付かない。
13. タイマーの4行: 前置きと `ops = 10` が一致する。`time` と `avg` は「4桁の有効数字 + 空白 + `ns`・`μs`・`ms`・`s`・`min`・`h`・`d` のいずれか」の形であり、`ops/s` は2桁の小数である。値は実行ごとに変わるので比べない。
14. `verbose` の差: `false` では表の見出し `       Value     Percentile TotalCount 1/(1-Percentile)` が無い。`true` では9行の後にこの見出しと空行が続き、0〜4 の5件で上の表（15行）と3つの `#[` 行が一致する。
15. 空の `HdrHistogram` の `verbose`: `min = 0`、`max = 0`、`mean = 0.00`、`stddev = 0.00`、百分位はすべて0で、表は見出し・空行・3つの `#[` 行だけで行が無い。
16. 空行: 空でない種類の後に空行が1つ、報告の後にさらに1つ。1種類だけの報告は空行2つで終わる。
17. 書き込みの失敗は `Err` で返る（いまの性質を保つ）。

#### 上司に上げる問い

**問い1（ファイルディスクリプタの見出しの名前）。** 原典はゲージの種類の見出しに先頭の項目の実行時クラス名を使う（64行、171〜175行）。ファイルディスクリプタの3つのゲージは Scala の無名クラスなので、JVM と同じ名前（`FileDescriptorMetricSet$$anon$1`）は Rust で作れない。案A（推し）: 先頭が `FileDescriptorGauge` のときは `-- FileDescriptorGauge` と出し、「先頭の項目で決まる」という原典の規則を保つ（先頭が平均ゲージなら `-- AveragingGauge` になる）。案B: ゲージの種類の見出しを常に `-- Gauges` にする（Dropwizard の `ConsoleReporter` の形。原典の規則からは離れる）。

**問い2（未停止のタイマーの `time` と `avg`）。** 原典の `elapsedTime` は `stopTime - startTime` であり（`KnownOpsInTimespanTimer.scala` の42行）、未停止では `stopTime` が0なので負になる。`PekkoConsoleReporter.scala` の133・135行により、原典は `time = -1.234e+09 ns` のような負の値を出す。Rust の `elapsed()` は未停止で `None` を返し（I5）、報告はいま `time = not stopped` としている。案A（推し）: `not stopped` のままにする。原典の値は意味が無く、5ケースはどれも停止してから報告するためである。案B: 原典どおり負のナノ秒を出す（開始時刻を出す非公開の口と、負の値を受ける `%g` を足す）。

#### 定期報告スレッド

定期報告（Console レポーターが有効であり、かつ `scheduled_report_interval` が 0 より大きい場合）は標準ライブラリのスレッドで回す。これはアクターの dispatcher ではなく、レポーター自身のスレッドであり、[Issue #234](https://github.com/j5ik2o/fraktor-ng-rs/issues/234) の実行器とは無関係である（§8）。既定値では間隔が 0 のためスレッドは起動しない。スレッドと本体は、レジストリ・停止フラグ・出力先を含む共有状態を `Arc` で共有し、レジストリは `Mutex<BTreeMap<String, Metric>>` とする。スレッドの待ちは `thread::sleep` ではなく、停止フラグと対になった `Condvar::wait_timeout_while`（述語は「フラグが立っていない」）とし、`shutdown_metrics()` が呼ばれたら長い間隔の途中でも即座に目覚めて終わる。スレッドはフラグの確認のために停止ロックを取るが、報告はロックを解放してから行うため、報告の途中でも停止側はフラグを立てられる。`shutdown_metrics()` は冪等であり（2回目以降は何もしない）、呼ばれないまま `MetricsKit` がドロップされた場合も、`Drop` が同じ停止手順（停止ロックの中でフラグを立て、解放して Condvar で起こし、ハンドルの状態を見て、スレッドを接合するか接合の完了を待つ）を行うため、スレッドは残らない。このライフサイクルは状態遷移であるため、Quint で検査する（§6）。テストから出力を差し替えられるよう、出力先は `with_output` 構築子で `Box<dyn Write + Send>` を受け取り、`new` では標準出力にする。

#### 停止の境界

停止の保証の境界は、停止フラグを立てた時点ではなく、`shutdown_metrics()`（または `Drop` の停止手順）が戻った時点である。スレッドは報告の前に停止フラグを読むため、停止要求の直前に始まっていた報告は、停止手順が戻る前に完了することがある。戻った後に新しい報告が始まることはない。並行して `shutdown_metrics()` を呼んだ場合も、全ての呼び出しがスレッドの終了を待ってから戻る（接合中の呼び出しは完了を待つ）。出力先が標準出力の場合、呼び出し側が `StdoutLock` を保持したまま停止手順を呼ぶと、スレッドが出力を待ち、停止手順がスレッドの終了を待つため止まったままになる。そのため、出力ロックを解放してから停止する契約とし、公開 API の文書にも明記する。

#### 報告の失敗の扱い

報告の失敗の扱いも定める。`ConsoleReporter::report` は書き込みと flush の失敗を呼び出し側へ返す。`report_metrics()` は原典どおり失敗を呼び出し側へ伝えないが、計測は登録されたまま残る。`report_and_clear_metrics()` は、報告対象を取り出してから出力し、出力が成功した場合だけ取り出した計測を消去済みとする。失敗した場合は取り出した計測を戻す（その間に同じキーが再登録されていた場合は新しい登録を残す）。出力の途中で失敗した場合、既に出力された行が再報告で重複することがある。報告中に新しく登録された計測は取り出しの対象に含まれないため、次の報告に残る。

### no_std の判断

`fraktor-testkit` は no_std のクレートではない（`#![no_std]` を持たず、スレッドと時刻と標準入出力を使うテスト支援専用のクレートである）。原典の metrics も、ファイルディスクリプタ、実時間（`System.nanoTime`）、標準出力への報告という std と OS の機能に依存する内部ユーティリティであり、actor の kernel に属さない。したがって新しいモジュールは `fraktor-testkit` の中で std を使い、`fraktor-actor` の kernel（`core` と `alloc` だけ）の no_std は変更によって影響を受けない。時刻は port 越しにせず `std::time::Instant` を直接使う。これは kernel の規約（Clock や Scheduler の port）ではなく、testkit の既存の実装（実時間を直接読む `TestLatch` など）と同じ扱いである。

## 3. 守る性質（不変条件）

- **I1（キーの整形の一貫性）。** `MetricKey` のパスは、生成（`from_string`）と連結（`child`）のどちらでも、原典の `sanitizeMetricKeyPart` を通した部品だけから組み立てる。整形の順序は原典どおりで、先に `"..."` を `"…"`（U+2026）に置き、次に `"."` を `"-"` に、`]`・`[`・`(`・`)`・`<`・`>` を `"|"` に、空白を `"-"` に、`"/"` を `"-"` に置く。ここでいう空白は原典の `.replaceAll(" ", "-")` どおり ASCII の半角スペース（U+0020）だけであり、タブ・改行・全角スペース（U+3000）は置換しない（`char::is_whitespace()` で広く解釈しない）。
- **I2（レジストリの一意性）。** 1つのキーに登録できる計測は1つだけである。同じキーで同じ種類を再び求めると、既存のハンドルが返り、状態は共有される。同じキーで違う種類を求めると `MetricsError::KindConflict` になり、レジストリは変わらない。
- **I3（平均の定義）。** `AveragingGauge::value` は、加えられたすべての値の合計を個数で割ったものに等しい。一括の加算（`add_slice`）は、要素ごとの加算と同じ結果になる。個数が 0 のときは原典どおり `NaN` である（`0.0 / 0.0`。原典の `LongAdder` の和と個数でも同じになる）。
- **I4（ヒストグラムの件数と記録できる範囲）。** 成功した `update` の回数と `count` は常に等しい。範囲外の値で失敗した `update` は件数を変えない。記録できる値の下限は `0` である。上限は、構築時に決めた追跡可能な最大値そのものではなく、対数バケットの上界（その最大値を含むバケットの等価階級の上限）である。これは原典の Java 版 HdrHistogram と、方法 A の `hdrhistogram` クレートの両方に共通する振る舞いであり、たとえば最大値 `100_000_000`・有効数字4桁では、記録できる上限は `134_217_727`（`2^27 - 1`）であり、`100_000_001` も記録でき、`134_217_728` から範囲外のエラーになる。追跡可能な最大値で厳密に弾く狭めはしない（原典準拠）。
- **I5（タイマーの一回限り）。** `KnownOpsInTimespanTimer::stop` は、最初に呼ばれたときだけ `true` を返して停止時刻を固定し、以後は `false` を返して停止時刻を変えない。`count` は停止の前後によらず常に `expected_ops` である。原典の `elapsedTime` は未停止では負の値になるが、`Duration` は負を表せないため、未停止の間は `elapsed()`・`avg_duration_nanos()`・`ops_per_second()` のいずれも `None` を返すことに確定させる（開始からの経過で代用しない）。
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

**定期報告スレッドのライフサイクルについて書く。** 対象は `crates/fraktor-testkit/specs/console_reporter.qnt` である。モデルは1つの状態列挙ではなく、次の3つの状態と、停止フラグ・停止ロックで表す。

- スレッドの状態 `Thread`：`NotStarted`（未起動）・`Waiting`（待機中）・`Reporting`（報告中）・`Finished`（終了）。スレッドが起動するのは、Console レポーターが有効であること（`console`）と、間隔が正であること（`positiveInterval`）の両方が成り立つときだけであり、この2つの条件は分けて持つ。
- ハンドルの状態 `Handle`：`Running`・`Joining`・`Joined`。実装の `ReporterThreadState`（`Running`・`Joining`・`Finished`）に対応し、実装の `Finished` をモデルでは `Joined` と呼ぶ。
- 停止の呼び出し側の状態 `Call`：`Idle`・`Flagging`・`Flagged`・`Joiner`・`WaitingForTheJoin`・`Returned`。`shutdown_metrics()` の呼び出しと `Drop` のどちらも、この同じ遷移を通る。

停止のロックは、停止フラグ（`stopFlag`）の読み書きの間だけ保持する。報告スレッドはロックを解放してから報告するため、報告の途中でも停止側はフラグを立てて接合へ進める。停止手順は、ロックを取り、フラグを立て、ロックを解放して起こし、ハンドルの状態を見る、という順序でそろえる。最初に `Running` を見つけた呼び出しが `Joiner` として接合し、`Joining` を見つけた呼び出しは `WaitingForTheJoin` として完了を待ち、`Joined` を見つけた呼び出しはそのまま `Returned` になる。停止の保証の境界は、フラグを立てた時点ではなく、呼び出しが戻った時点であり、不変条件 `noReportOnceACallReturned`（呼び出しが戻った後は、報告は走っておらず新しく始まることもない）で表す。ほかの不変条件は、`threadOnlyWithConsoleAndInterval`（Console と間隔の条件が無ければスレッドは起動せず報告も無い）、`flagOnlyAfterAStopBegins`、`leavesOnlyOnTheFlag`（スレッドはフラグが立ったときにだけ終わる）、`stopLockOnlyAroundTheFlag`（ロックはフラグの読み書きの間だけ）、`oneJoinerAtATime`、`joinedAtMostOnce`、`nothingRunsAfterTheDrop` である。あわせて、`shutdown_metrics()` の冪等性、ドロップ時の停止、長い待機の途中での停止要求による即時の終了をモデル化する。なお、以前の版が挙げた「停止要求済み」という状態はモデルには無い。停止要求は `stopFlag` と `Call` の遷移で表す。定期報告スレッドは、待ちと時間とロックと停止要求が絡む並行の状態遷移であり、`fraktor-testkit` の既存の Quint 仕様 `watch_handle.qnt`（監視スレッド）と同型であるため、検査の対象にする。

書かない部分も明記する。タイマーの「計測中→停止済み」の2状態は、停止が一回限りである性質を I5 と P5 で直接に確かめられるため Quint にはしない。レジストリの登録・取得・消去は写像の意味論であり、順序の interleaving をモデル化しても得られる検査が増えないため Quint にはしない。

出力の形（§2）も Quint にはしない。出力は、レジストリの写像をキーで並べ替え、種類ごとの書式で文字列にするだけの関数であり、状態も遷移も持たない（同じ入力と同じ `verbose` に対して常に同じ文字列を返す）。並び順と書式は、§2 の red のテストの項目にある golden テストで直接に確かめられる。定期報告スレッドのライフサイクルは、これまでどおり `crates/fraktor-testkit/specs/console_reporter.qnt` が検査する（本改訂では変えない）。両者は別の関心事であり、出力の形のために Quint の状態を増やさない。

## 7. 台帳の見込みと、区分の提案

第1段では台帳（`ledger/nodes.yaml`）を触らない（第1段の前例どおり）。第2段の見込みは次のとおりである（実装はマージ済みであり、いまの台帳の状態は下の「この issue（#299）の台帳への影響」に書く）。

- 対象ノード `testkit/src/test/scala/org/apache/pekko/testkit/metrics/MetricsKitSpec.scala` を、`cases.total: 5`、`ported: 5`、`passing: 5`、`rust.crate: fraktor-testkit`、`rust.test_file: crates/fraktor-testkit/tests/metrics_kit_spec.rs` として green にする見込みである。
- ノードの `goal` は現在 null である。周囲の testkit のノードは `G0b` に属しているため、第2段では台帳の決まりに従って goal の帰属を確認してから書く（本設計では決めない）。
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
- JVM の書式のうち Rust で同じにできるもの（`%d`、HALF_UP の固定小数、`NaN`・`Infinity`、UTF-16 順）は同じにし、できないものだけを違いとして1行ずつ挙げた。
- red のテストの担当のための面（`report` で足りるか、確かめる項目）を書いた。
- 出力の形は Quint にしない理由を §6 に1段落で足した。
- §7 に、台帳の `cases` と `verified_against` が動かないことを書いた。
- 出力の形を確かめるテストは原典外であるため、台帳の件数と `verified_against` を変えない。

issue #299 の「あわせて」の2点は、PR #306（`68a48a7`、コミット `0992b1e`）でマージ済みである。設計し直さない。

- 中：`metrics_kit_value.rs` の複製してから retain する形を `mem::take` にする — 済み。`0992b1e`
- 中：hdrhistogram の推移の依存（byteorder（Unlicense OR MIT）、num-traits、autocfg）を `docs/design/issue-206-license-policy.md` §4.3 に記録する — 済み。`0992b1e`

§12 は前のレビューの記録であり、書き換えない。
