# ゴール G0issue177：classic FrequencySketchSpec

[Issue #177](https://github.com/j5ik2o/fraktor-ng-rs/issues/177)（Orca 項目359）の対象は、Pekko v2.0.0 の classic `FrequencySketchSpec`。原典は「FrequencySketch must」の下に14件、「FastFrequencySketch must」の下に8件、計22件を持つ。件数は原典の `" in {"` の数で数えた。`ignore` と `pending` はない。[原典のテスト](https://github.com/apache/pekko/blob/9fbd2283b1cf701c90a1223bf621942c191db61f/actor-tests/src/test/scala/org/apache/pekko/util/FrequencySketchSpec.scala) [原典の実装](https://github.com/apache/pekko/blob/9fbd2283b1cf701c90a1223bf621942c191db61f/actor/src/main/scala/org/apache/pekko/util/FrequencySketch.scala)

## 対象ノード

| id | Rust の統合テスト | 原典のケース数 |
|:---|:---|---:|
| `actor-tests/src/test/scala/org/apache/pekko/util/FrequencySketchSpec.scala` | `crates/fraktor-actor/tests/frequency_sketch_spec.rs` | 22 |

台帳は原典22件、移植22件、成功22件、除外0件、作らない0件。`needs` は空のままで、TestKit の機能は使わない。台帳の `test_cases_approx: 16` は変換時の概数で、変えていない。引き継ぎの時点では 23件（15＋8）とされていたが、原典を数え直すと「FrequencySketch must」は14件だった。

設計は [issue-177-frequency-sketch-design.md](issue-177-frequency-sketch-design.md) に記録した（設計の独立レビュー用）。PBT と Quint は原典外で、台帳の件数には含めない。

## 実行経路

TAKT は使わず、担当が直接進めた。前の担当がテストを先に書き、台帳を red にした（`dce451c`、`cargo test -p fraktor-actor --test frequency_sketch_spec` は E0432 で失敗）。実装を足して green にした。

## 公開面

`fraktor_actor::kernel::frequency_sketch` に置いた。設定は構造体で渡し、不正な設定は `Result` で返し、ハッシュは原典と同じ方式にする（決定済み）。

| 原典 | 移植 |
|:---|:---|
| `FrequencySketch[A](capacity, widthMultiplier, resetMultiplier, depth, counterBits)(implicit hasher)` | `FrequencySketch::<A, H>::new(capacity, FrequencySketchSettings)`、ハッシュを渡すときは `with_hasher` |
| `FastFrequencySketch[A](capacity, widthMultiplier, resetMultiplier)` | `FastFrequencySketch::<A, H>::new(capacity, FastFrequencySketchSettings)`、`with_hasher` |
| 引数の既定値 | 各設定の `Default`（4、10.0、4、4 と 4、10.0） |
| `require` の `IllegalArgumentException` | `Err(FrequencySketchError::..)` |
| `size` / `frequency` / `increment` | `size() -> i32` / `frequency(&A) -> i32` / `increment(&A)` |
| `Hasher[A]`、`StringHasher`（`MurmurHash3.stringHash`、種 135283237） | `FrequencySketchHasher<A>`、`StringHasher`（`FrequencySketch` の既定） |
| `FastFrequencySketch` が使う `value.hashCode` | `StringHashCode`（`String.hashCode` と同じ多項式。`FastFrequencySketch` の既定） |
| `FrequencySketch.Bits` | `Bits`（`ceiling_power_of_two`、`is_power_of_two`、`power_of_two_exponent`） |

数と計数は原典の `Int` と同じく `i32` で持ち、折り返す。カウンタは 64 bit の語に詰め、シフトは論理シフト、`Long` のシフト量は 64 の剰余とした。リセットの大きさは `Double` から `Long` への飽和と、`toInt` の下位 32 bit を写した。

### 原典と違えたところ

- 原典が投げる例外は、どれも `FrequencySketchError` の値にした。順序は原典の検査の順で、幅のあふれ（`WidthOverflow`）、2 の冪でない幅（`WidthNotPowerOfTwo`）、カウンタの大きさ（`UnsupportedCounterBits`）、負の深さ（`NegativeDepth`。原典では `NegativeArraySizeException`）。
- 幅が 0 以下（幅の倍率 0 や、容量 `i32::MIN`）のときは `WidthNotPositive` を返す。原典は作成を通し、最初の更新で配列の範囲外になる。2 の冪の検査は 0 と `i32::MIN` を通すので、その後ろに置いた。
- 容量が `2^30` を超えると、`ceiling_power_of_two` は原典どおり `i32::MIN` になる。64 bit の積は負になって原典のあふれの検査を通り、既定の倍率では下位 32 bit が 0 になる。そのため `WidthOverflow` ではなく `WidthNotPositive { width: 0 }` を返す（PR #205 の Devin の指摘で単体テストに足した）。
- 深さ 0 は原典どおり受け付け、`frequency` は `i32::MAX`、`size` は 0 になる。

### 作らないもの

`toDebugString` と `FrequencySketchUtil.debugString`（デバッグ用の表の文字列）は作らない。原典のテストは使っておらず、ケースの作らない区分にも当たらない（ケースは全件移植した）。

## 原典との照合

移植テストは原典のケースごとに1関数で、原典の名前をコメントに残した。操作と検査は原典の順に並べた。補助の `ZipfianGenerator` と、それが使う `java.util.Random`（48 bit の線形合同法と `nextDouble`）もテストの中に移した。原典は zipfian のケースで、実頻度の同数を `mutable.Map` の順に並べる。この順は検査の対象ではないので、移植では同数を id の順に並べた。

JVM の実物と同じ値になることを単体テストで確かめた。期待値は、Pekko 2.0.0 の jar を jshell から直接動かして読んだ値である。

| 単体テスト | 確かめること |
|:---|:---|
| `string_hasher_test.rs` | `MurmurHash3.stringHash` の値（foo、bar、baz、空文字、a） |
| `string_hash_code_test.rs` | `String.hashCode` の値 |
| `bits_test.rs` | `Bits` の3関数の値 |
| `frequency_sketch_value_test.rs` | 5000 回の更新の 1000 回ごとの `size`（3通りの設定）、例外に当たる設定の誤り |
| `fast_frequency_sketch_test.rs` | 同じく2通りの設定の `size`、設定の誤り |

`size` の追跡値は、ハッシュ、カウンタの位置、リセットの算術がどれか1つずれても変わる。5通りとも JVM と一致した。

## 検証

| コマンド | 終了値 | 結果 |
|:---|---:|:---|
| `cargo test -p fraktor-actor --test frequency_sketch_spec` | 0 | 22件成功 |
| `cargo test -p fraktor-actor --lib frequency_sketch` | 0 | 18件成功 |
| `cargo test --workspace` | 0 | 1009件成功、失敗0（main の取り込み後） |
| `cargo clippy --workspace --all-targets -- -D warnings` | 0 | 警告0 |
| `cargo fmt --check` | 0 | 差分なし |
| syn-check（`crates/` の2クレート） | 0 | 違反0、抑制0 |
| `npm run check:aislop` | 0 | 受け入れ済みの一覧と一致 |
| `verify_green.py --crate fraktor-actor` | 0 | `all_pass: true` |
| CI の python テスト 11 本（`goal_complete_gate_test.py` ほか） | 0 | すべて成功 |
| `pekko_baseline.py check-ledger`、`check-refs`、`relocate_nodes.py check` | 0 | OK |
| `rollup.py --goal G0issue177` | 0 | `{'green': 1}`、「完了: 全ノードが green か do_not_port か not_built」 |
| `check_needs.py --goal G0issue177` | 0 | needs 閉性 OK |
| `cargo test -p fraktor-actor --release --test frequency_sketch_property_test` | 0 | 4件成功（PBT の P1〜P7。原典外） |
| `quint typecheck docs/goals/specs/frequency_sketch.qnt`、`..._test.qnt` | 0 | 成功 |
| `quint test --main=narrow --match='^(incrementCountersToMaxValue\|resetHalvesAndAdjustsForOddCounters)$' ..._test.qnt` | 0 | 2 passing（各 10,000 test） |
| `quint run --main=narrow/fast/wide/noReset/noRows --invariant=inv --max-samples=10000 --max-steps=40 --seed=0x1 ..._test.qnt` | 0 | すべて `[ok] No violation found` |
| `quint run --main=bugNoOddAdjust/bugNoSaturation/bugStaleSize --invariant=inv ..._test.qnt` | 1 | すべて `[violation]`（負の対照） |

`verified_against: v2.0.0` を付けたので、`report_ledger_1to1.py list-undocumented` で `docs/baseline/v2.0.0-undocumented-nodes.tsv` を作り直した（1行増えて50件、`no-report`）。作り直す前は `report_ledger_1to1_test.py` が検査5で落ちた。
