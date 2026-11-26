# 設計：#188 ImmutableIntMapSpec／TokenBucketSpec

[Issue #188](https://github.com/j5ik2o/fraktor-ng-rs/issues/188) の設計の記録である。Pekko v2.0.0 の actor-tests にある util の `ImmutableIntMapSpec` と `TokenBucketSpec` を Rust へ移植し、台帳の対象ノード2件を green にする。本書は第1段（設計）であり、実装は設計レビューが通ってから第2段で行う。

依頼文は「原典の3つの Spec」と書くが、対象の Spec は2つである（`ImmutableIntMapSpec` と `TokenBucketSpec`）。本書は2つで進める。

原典（Pekko v2.0.0）:

- [ImmutableIntMapSpec.scala](https://github.com/apache/pekko/blob/v2.0.0/actor-tests/src/test/scala/org/apache/pekko/util/ImmutableIntMapSpec.scala)／[ImmutableIntMap.scala](https://github.com/apache/pekko/blob/v2.0.0/actor/src/main/scala/org/apache/pekko/util/ImmutableIntMap.scala)
- [TokenBucketSpec.scala](https://github.com/apache/pekko/blob/v2.0.0/actor-tests/src/test/scala/org/apache/pekko/util/TokenBucketSpec.scala)／[TokenBucket.scala](https://github.com/apache/pekko/blob/v2.0.0/actor/src/main/scala/org/apache/pekko/util/TokenBucket.scala)

移植の規約は `.takt/facets/knowledge/fraktor-porting-rules.md` に従う。形の前例は [PR #205](https://github.com/j5ik2o/fraktor-ng-rs/pull/205) の [issue-177-frequency-sketch.md](https://github.com/j5ik2o/fraktor-ng-rs/blob/main/docs/design/issue-177-frequency-sketch.md) と [issue-176-double-linked-list.md](https://github.com/j5ik2o/fraktor-ng-rs/blob/main/docs/design/issue-176-double-linked-list.md) である。

## 1. 何を移植するか

### 件数の数え直し

原典の `"..." in { ... }` の数を数え直した。どちらも台帳の目安（`test_cases_approx`）と一致した。

| Spec | 目安 | 数え直した実数 | 備考 |
|:---|---:|---:|:---|
| ImmutableIntMapSpec | 10 | **10** | 展開で増えるケースは無い |
| TokenBucketSpec | 12 | **12** | 最後の1件がパラメータの組み合わせをループで回す（下記） |

TokenBucketSpec の最後のケース「behave exactly as the ideal (naive) token bucket if offer is called with perfect timing」は、capacity 4通り × period 3通り × arrivalPeriod 3通り × startTime 4通り × maxCost 3通り = 432 通りの組み合わせを、各組み合わせ最大1001ステップ回す。ケースの数としては1件であり、移植も1テスト関数にする（組み合わせをテスト関数に展開しない）。

### ImmutableIntMapSpec の対応表

置き場所は `crates/fraktor-actor/tests/immutable_int_map_spec.rs`（ノード1件＝ファイル1件、冒頭に出典のパスをコメントで残す）。全10件を移す。作らないケースは無い。

| # | 原典のケース | Rust のテスト名 | 移す／作らない |
|:---:|:---|:---|:---|
| 1 | have no entries when empty | `immutable_int_map_has_no_entries_when_empty` | 移す |
| 2 | add and get entries | `immutable_int_map_adds_and_gets_entries` | 移す |
| 3 | replace entries | `immutable_int_map_replaces_entries` | 移す |
| 4 | update if absent | `immutable_int_map_updates_if_absent` | 移す |
| 5 | have toString | `immutable_int_map_formats_with_display` | 移す（`Display` の文字列で確かめる。§2） |
| 6 | have equals and hashCode | `immutable_int_map_has_equality_and_consistent_hash` | 移す（等しいマップ同士のハッシュ一致まで。Java の `Arrays.hashCode` の値そのものは再現しない。§2） |
| 7 | remove entries | `immutable_int_map_removes_entries` | 移す |
| 8 | get None when entry doesn't exist | `immutable_int_map_get_returns_min_value_for_missing_keys` | 移す（原典の `get` は Option ではなく、無いキーに `Int.MinValue` を返す。本体の仕様であり、ケース名の "None" はその意味） |
| 9 | contain keys | `immutable_int_map_contains_keys` | 移す |
| 10 | have correct behavior for random operations | `immutable_int_map_matches_the_model_for_random_operations` | 移す（原典は `System.nanoTime` を seed にする。移植は固定 seed の SplitMix64 にし、失敗時に seed と手の番号を表示する。操作は1000手、キーは 0..100、値は printable な文字のコード。操作の比率は原典の `nextInt(3)` のとおりで、0 と 1 が `updated`、2 が `remove`（更新が3分の2、削除が3分の1）。参照モデルは `BTreeMap<i32, i32>`。原典の参照は `Map[Long, Int]` と書かれているが、キーは `rnd.nextInt(100)` の `Int` であり、実態は Int キーである） |

### TokenBucketSpec の対応表

置き場所は `crates/fraktor-actor/tests/token_bucket_spec.rs`。全12件を移す。作らないケースは無い。原典は `PekkoSpec` を継承するが、ケースの中で TestKit の機能（expectMsg 等）は使っておらず、時刻を手で動かす `TestBucket` だけが要る。台帳の `needs.testkit` には `extends-pekko-spec` 等が入っているが、実使用は無いので、移植は fraktor-testkit に依存しない通常の統合テストにする。

| # | 原典のケース | Rust のテスト名 | 移す／作らない |
|:---:|:---|:---|:---|
| 1 | start full | `token_bucket_starts_full` | 移す |
| 2 | calculate correctly with different rates and capacities | `token_bucket_calculates_with_different_rates_and_capacities` | 移す |
| 3 | allow sending elements larger than capacity | `token_bucket_allows_cost_larger_than_capacity` | 移す |
| 4 | work with zero capacity | `token_bucket_works_with_zero_capacity` | 移す |
| 5 | not delay if rate is higher than production | `token_bucket_does_not_delay_when_rate_exceeds_production` | 移す |
| 6 | maintain maximum capacity | `token_bucket_maintains_maximum_capacity` | 移す |
| 7 | work if currentTime is negative | `token_bucket_works_with_negative_current_time` | 移す |
| 8 | work if currentTime wraps over | `token_bucket_works_when_current_time_wraps_over` | 移す（`i64::MAX - 5` から始め、時刻を +10 して折り返す。§3） |
| 9 | (attempt to) maintain equal time between token renewal intervals | `token_bucket_keeps_equal_time_between_token_renewal_intervals` | 移す（遅延から「最後のトークン到着からの経過時間」を引く計算が効くケース。§4 の Quint が守る性質の中心） |
| 10 | work with cost of zero | `token_bucket_works_with_zero_cost` | 移す |
| 11 | work with very slow rates | `token_bucket_works_with_very_slow_rates` | 移す（period は `i64::MAX >> 10`。`10 * period` が `i64` に収まる範囲であることも原典の前提） |
| 12 | behave exactly as the ideal (naive) token bucket if offer is called with perfect timing | `token_bucket_matches_the_ideal_bucket_with_perfect_timing` | 移す（432 通りの組み合わせ。原典は seed 無しの `scala.util.Random` を使う。移植は固定 seed の SplitMix64 にし、組み合わせと seed を失敗時に表示する。次の到着までの残り手数の更新は、原典の式 `untilNextElement = time + Random.nextInt(arrivalPeriod)` を字面どおりに写す。残り手数に絶対時刻を足す形は相対の値であるべきところに見える癖だが、直すとどの到着を検査するかが変わり、元のテストと別物になる。`(idealBucket >= 0)` の確認も原典どおりに残す。時刻の加算 `startTime + time` はテストのコードでも `wrapping_add` で書く（開始時刻に `i64::MIN` と `i64::MAX` を含むため。F2） |

## 2. 本体の有無と、新しく足すもの

`crates/` 全体を `ImmutableIntMap`、`TokenBucket`、`immutable_int_map`、`token_bucket` で検索し、該当は無かった。どちらも新しく足す。置き場所は、原典が `org.apache.pekko.util`（actor モジュール）であり、先行する util 系の移植（`DoubleLinkedList`、`FrequencySketch`）がすべて `fraktor-actor` の kernel にあることから、`crates/fraktor-actor/src/kernel/` とする。

| ファイル | 中身 |
|:---|:---|
| `crates/fraktor-actor/src/kernel.rs` | `pub mod immutable_int_map;` と `pub mod token_bucket;` の2行 |
| `crates/fraktor-actor/src/kernel/immutable_int_map.rs` | モジュールの宣言と再エクスポート |
| `crates/fraktor-actor/src/kernel/immutable_int_map/immutable_int_map_value.rs` | `ImmutableIntMap` |
| `crates/fraktor-actor/src/kernel/token_bucket.rs` | モジュールの宣言と再エクスポート |
| `crates/fraktor-actor/src/kernel/token_bucket/token_bucket_value.rs` | `TokenBucket<C>` |
| `crates/fraktor-actor/src/kernel/token_bucket/token_bucket_error.rs` | `TokenBucketError` |
| `crates/fraktor-actor/src/kernel/token_bucket/token_bucket_value_test.rs` | 原典外の単体テスト（負のコスト、構築の `Err`、F1 の和のあふれ。本番のファイルとは分ける。syn-check の `tests-location` に従う） |
| `crates/fraktor-actor/tests/immutable_int_map_spec.rs` | 原典10ケース |
| `crates/fraktor-actor/tests/token_bucket_spec.rs` | 原典12ケース |
| `crates/fraktor-actor/tests/immutable_int_map_property_test.rs` | PBT（§3。原典外で、台帳の件数に含めない） |
| `crates/fraktor-actor/tests/token_bucket_property_test.rs` | PBT（§3。原典外で、台帳の件数に含めない） |
| `crates/fraktor-actor/specs/token_bucket.qnt` | Quint のモデル（1モジュール。§4。CI の `scripts/quint-check-specs.sh` の対象 `crates/*/specs/*.qnt` に載せる） |
| `ledger/nodes.yaml` | 第2段で対象ノード2件を green にする（本書の段では触らない） |

### ImmutableIntMap の公開 API

原典は、キーと値を1本の `Array[Int]` に交互に詰めた不変マップである（二分探索で引き、更新はコピーオンライト、構造の共有は無い。対象は1000件未満の小さいマップ）。移植の表現は、キーと値の組の列 `Vec<[i32; 2]>` にする（F8）。交互に並べた `Vec<i32>` を保つ利点は小さく、組の列にすれば core の `binary_search_by_key` がそのまま使え、「長さが偶数」という条件も型で保証されて、偶数・奇数の添字の取り違えが起きない。`Eq` と `Hash` の意味（並びの比較）は変わらない。

```rust
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct ImmutableIntMap {
  entries: Vec<[i32; 2]>, // [キー, 値] の列。キーの昇順
}

impl ImmutableIntMap {
  pub fn empty() -> Self;
  pub fn size(&self) -> usize;
  pub fn is_empty(&self) -> bool;
  pub fn get(&self, key: i32) -> i32;              // 無いキーは i32::MIN（原典どおり）
  pub fn contains(&self, key: i32) -> bool;
  pub fn updated(&self, key: i32, value: i32) -> Self;
  pub fn update_if_absent(&self, key: i32, value: i32) -> Self;
  pub fn remove(&self, key: i32) -> Self;
  pub fn keys(&self) -> impl Iterator<Item = i32> + '_;   // 昇順
  pub fn iter(&self) -> impl Iterator<Item = (i32, i32)> + '_;
}
impl Default for ImmutableIntMap;
impl core::fmt::Display for ImmutableIntMap; // "ImmutableIntMap(10 -> 10, 20 -> 20)"、空は "ImmutableIntMap()"
impl core::fmt::Debug for ImmutableIntMap;   // Display と同じ形にする
```

原典と違える点:

- 原典の `toString` は `Display` で受ける（ケース5）。
- 原典の `hashCode` は `java.util.Arrays.hashCode(kvs)` で、値は JVM 固有である。ケース6が確かめるのは「等しいマップは等しいハッシュを持つ」ことだけなので、`Hash` の derive（`Vec<i32>` のハッシュ）で満たす。JVM のハッシュ値そのものは再現しない。
- 原典の `updateIfAbsent` の値は名前渡し（`=> Int`）で、キーが有るときは評価されない。ケース4はリテラルしか渡さないので遅延評価は観測できない。移植は値渡しとし、遅延が要る利用が出たら `update_if_absent_with(FnOnce)` を足す、と API 文書に書く。
- 原典の二分探索は、半分の大きさに対する添字の工夫（`lohi & ~1` 等）を含む。観測できる振る舞いは「ソート済みのキー列に対する二分探索の結果」だけなので、内部はキー列に対する通常の二分探索でよい。工夫の写しはしない。
- 原典が同じ値での `updated` で `this` を返す（コピーしない）点は、移植では再現しない。`&self -> Self` で `Vec` を持つ限り、戻り値を作る複製は必ず要り、この分岐で省けるのは要素の書き込みだけである（F7）。値の等価としては観測できないので、移植は常に複製して新しいマップを返す。
- `get` が無いキーに `i32::MIN` を返す仕様は原典どおりに残し、API 文書に「値として `i32::MIN` を格納すると、無いキーと区別できない」と明記する（原典の scaladoc の警告と同じ）。

no_std: `Vec` は alloc、それ以外は core のみで書ける。fraktor-actor は `extern crate alloc` を宣言済みであり、新しい依存は足さない。

### TokenBucket の公開 API

原典は抽象クラスで、`currentTime`（単調時刻、ナノ秒、`Long`、折り返しうる）だけが抽象であり、具象の `NanoTimeTokenBucket` が `System.nanoTime` を使う。kernel はプラットフォームの時計に触れない決まり（port & adapter）なので、時刻は port 越しに受ける。port は新しい trait を足さず、既存の `kernel::Clock` を使う（F5）。初版の設計は「`Clock` は `Duration` を返すので、負の時刻と折り返しを表せない」として専用の port を提案したが、その理由は成り立たない。TokenBucket が使うのは時刻の差だけであり、`clock.monotonic().as_nanos() as i64` は `u128` の下位64ビットなので、どの `i64` の時刻も表せ、差は折り返しの算術で保たれる（設計レビューで、負の時刻と `i64::MAX` を跨ぐ往復が試作で確かめられた）。テストの手動時計も、`Duration::from_nanos(t as u64)` で負の時刻と折り返しを作れる。移植の規約（kernel は時刻に Clock の port 越しで触れる）にも合い、公開する trait が増えず、`adaptor::SystemClock`（`System.nanoTime` に当たる）がすでにある利点もある。

```rust
pub struct TokenBucket<C> {
  capacity: i64,
  nanos_between_tokens: i64,
  available_tokens: i64,
  last_update: i64,
  clock: C,
}

impl<C: Clock> TokenBucket<C> {
  pub fn new(capacity: i64, nanos_between_tokens: i64, clock: C) -> Result<Self, TokenBucketError>;
  pub fn init(&mut self);
  pub fn offer(&mut self, cost: i64) -> Result<i64, TokenBucketError>;
  pub fn capacity(&self) -> i64;
  pub fn nanos_between_tokens(&self) -> i64;
  // 現在時刻は clock.monotonic().as_nanos() as i64 で取る（下位64ビット。差だけが意味を持つ）
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenBucketError {
  NegativeCapacity { capacity: i64 },
  NonPositivePeriod { nanos_between_tokens: i64 },
  NegativeCost { cost: i64 },
}
// TokenBucketError には Display と core::error::Error も実装する（FrequencySketchError の前例と同じ。F9）
```

原典と違える点:

- 原典の `require`（容量が負、周期が0以下）と `offer` の負のコストの `IllegalArgumentException` は、`FrequencySketch` の前例（PR #205）と同じく、例外ではなく `TokenBucketError` の値で返し、最初に失敗した検査を返す。構築は `new` の `Result`、`offer` は `Result<i64, TokenBucketError>` である。原典のケースには不正な入力のケースが無いので、この変更はケースの移植に影響しない。
- 原典は `init()` を呼ぶ前の使用を契約にしている（呼ばなければ `availableTokens = 0`、`lastUpdate = 0` で動く）。移植も同じ状態で作り、`init` の呼び忘れを型では防がない。API 文書に「使う前に `init` を呼ぶこと」と書く。
- テストの `TestBucket` は、テストファイルの中で `kernel::Clock` を実装する手動時計にする。`Clock` は `Send + Sync` を求めるので、時計は `Rc<Cell<i64>>` ではなく `AtomicU64` でナノ秒のビット列を持ち、`monotonic()` は `Duration::from_nanos` で返す（前例の FrequencyList と RecencyList のテストの時計と同じ形）。負の時刻と折り返しは、ビット列として `t as u64` を入れることで作る。時刻の set／加算は `wrapping_add` で行う（F2）。
- 算術は原典の `Long` の折り返しを写す。`now - last_update`、`last_update += nanos_between_tokens`、`last_update += tokens_arrived * nanos_between_tokens`、`tokens_arrived * nanos_between_tokens`、`nanos_between_tokens * 2`（1ティックだけの近道の判定）、`remaining_cost * nanos_between_tokens`、`now + delay` は、すべて `wrapping_*` で書く。`last_update` と時刻の加減はすべて wrapping であると言い切る（F2。`last_update +=` を素の加算にすると、ケース8と12が debug で panic することが設計レビューで試作により確かめられた）。割り算 `time_elapsed / nanos_between_tokens` は、周期が正であることが構築時に保証されるので、通常の `/` でよい（`i64::MIN / -1` の形にならない）。
- 残りへの到着分の加算だけは折り返しにしない。`available_tokens = min(available_tokens.saturating_add(tokens_arrived), capacity)` とする（F1）。原典は `math.min(availableTokens + tokensArrived, capacity)` であり、`tokens_arrived` は最大で `i64::MAX` になるため、和があふれる入力（例：容量10、周期1、時刻0で `init`、残り9の状態で時刻を `i64::MAX` にしてコスト0の `offer`）では、素の `+` は debug で panic し、wrapping では残りが負になって T1 が崩れる。saturating にすると、この入力でも残りは容量どおり10、戻り値は0になる（設計レビューの試作で、原典12ケースが12件とも通ることも確かめられた）。原典と違うのは、和が `i64` に収まらない入力のときだけである。この差は「原典と違える点」として記録し、上の入力の単体テストを `token_bucket_value_test.rs` に1件足す。なお `cost - available_tokens` は、F1 により残りが常に対象範囲（0以上）にあるのであふれない。

作らないもの:

- `NanoTimeTokenBucket`（`System.nanoTime` を使う具象クラス）は、別の型としては作らない。`kernel::Clock` を使う形にしたため、`adaptor::SystemClock` を渡した `TokenBucket<SystemClock>` がそのまま相当品になり、新しいコードは要らない。原典のケースは `TestBucket`（手動時計）しか使わないので、この相当品にケースは無い。
- 原典の `TokenBucket` は Pekko の内部 API（`private[pekko]`）である。移植は kernel の公開型にするが、Pekko の利用者向け API に当たるものではない（先行する util 系の移植と同じ扱い）。

no_std: 本体は `i64` の算術だけであり、core のみで書ける。alloc も使わない。

## 3. 守る性質（不変条件）と壊しうる入力

### ImmutableIntMap

| 番号 | 不変条件 |
|:---|:---|
| M1 | 表現：`entries` は `[キー, 値]` の組の列で、長さは `size` に等しい。キーは狭義の昇順（重複が無い）。組の形により、長さの偶奇の条件は型で保証される（F8） |
| M2 | 不変性：`updated`、`update_if_absent`、`remove` は受け手を変更せず、新しいマップを返す |
| M3 | 引き：キーが有ればその値、無ければ `i32::MIN`。`contains` はキーが有るときだけ真 |
| M4 | `updated` は既存キーの値を置き換え、`size` は変わらない。新しいキーは位置どおりに入り、`size` が1増える。同じ値での `updated` は等しいマップを返す |
| M5 | `update_if_absent` は、キーが有るとき受け手と等しいマップを返し、無いときだけ追加する |
| M6 | `remove` は、キーが有ればそれだけを取り除き、無ければ受け手と等しいマップを返す。全部を取り除くと `empty` と等しくなる |
| M7 | `keys` と `iter` はキーの昇順に返す |
| M8 | 等価とハッシュ：中身（キーと値の列）が等しいマップは等しく、ハッシュも等しい。キーか値が1つでも違えば等しくない |

壊しうる入力:

| 入力 | 何が起きうるか | 確かめているもの |
|:---|:---|:---|
| キー／値が `i32::MIN` | キーとしては正常に引ける。値として格納すると、`get` の「無いキー」の戻り値と区別できない（原典の仕様そのもの） | PBT の入力域に `i32::MIN` を含める。ケースには無い |
| キーが `i32::MAX`、負のキー、挿入の順序（昇順／降順／交互） | 二分探索の境界と挿入位置 | PBT。ケース2は 5、10、15、20 の挿入順を確かめる |
| 空のマップへの `get`、`contains`、`remove`、`update_if_absent` | 探索の範囲が空のときの添字 | PBT の初期状態。ケース1と7の一部 |
| 無いキーの `remove`（最小より小さい、間、最大より大きい） | 受け手と等しいマップを返すこと | ケース7、PBT |
| 大きいマップ（原典の想定は1000件未満） | コピーオンライトの計算量は件数に比例する。表現の正しさは件数に依らない | PBT はキー域を広げた設定も流す。性能は対象外 |
| 同じキーへの連続した `updated` | 置き換えが積み重ならないこと（M4） | ケース3、PBT |

PBT は足す。原典のケース10（ランダムな操作1000手と参照マップの照合）を一般化し、`immutable_int_map_property_test.rs` に置く。

- 固定 seed の SplitMix64 で、`updated`、`remove`、`update_if_absent` の操作列を流し、毎手の後で `BTreeMap<i32, i32>` のモデルと比べる（M1〜M8 のうち観測できるもの：`size`、`keys` の列、各プローブキーの `get` と `contains`、モデルとの等価）。
- キー域は2通り（0..100 で衝突が多くなる設定と、`i32::MIN`、`i32::MAX`、負を含む広い設定）。値は `i32::MIN` を含む境界値の組から選ぶ。
- M2（不変性）は、`&self` の API が借用の規則で受け手の変更を保証するので、PBT の対象から外し「型で保証」とする（F7）。そのうえで、操作の後に受け手そのものが操作前のモデルと一致することも PBT で比べる（複製した別の値ではなく、受け手そのものを比べる）。
- 外部の依存は足さない（`proptest` を足すかは [#212](https://github.com/j5ik2o/fraktor-ng-rs/issues/212) で決める。前例と同じ）。

### TokenBucket

`init` の時刻を `t0`、周期を `p`（`nanos_between_tokens`）、容量を `cap` とする。

| 番号 | 不変条件 |
|:---|:---|
| T1 | 残りのトークン：各 `offer` の後で `0 <= available_tokens <= cap`。すべての入力で成り立つ。到着分の加算を saturating にするためであり、原典は和のあふれでこの条件が崩れる（F1） |
| T2 | 到着の格子：`last_update` は常に `t0 + k * p`（k は整数）の形をしている。トークンはこの格子の点でだけ到着する。遅延を返す分岐でも `last_update = now + delay` が格子の点に一致し、その点は元の `last_update` から `remaining_cost` 個先の点である（ケース3では15周期先。ケース9が確かめる性質） |
| T3 | 遅延の符号：戻り値は0以上。コストが「到着分を加えた残り」以下なら0、そうでなければ正の遅延 |
| T4 | 到着の数：経過時間 `e = now - last_update`（折り返しを込む）について、`e < p` なら到着0、`e >= p` なら到着は `e / p` で、`last_update` はその分だけ進む。残りは `cap` で頭打ちになる |
| T5 | `init` は残りを `cap` にし、`last_update` をその時点の `current_time` にする |
| T6 | 時刻の折り返し：すべての時刻の差は `i64` の折り返し算術で取るので、`current_time` が `i64::MAX` を跨いでも、経過時間が正しく出る（ケース8） |
| T7 | 遅延の計算：遅延を返す分岐では `delay = remaining_cost * p - (now - last_update)`。最後の項を引くことが、到着の間隔を均等に保つ（T2）。引かない実装はケース9で落ちる |
| T9 | 遅延の最早性：正の遅延が返るとき、その遅延は「コストをまかなえる最初の格子の点」までの時間であり、`(remaining_cost - 1) * p < delay <= remaining_cost * p` を満たす。1周期だけ長い遅延は、T2 の格子にも T3 の符号にも掛からないため、この条件が要る（F4） |
| T8 | 呼び出し側の契約：正の遅延が返った後、その遅延が経過する前に `offer` を再び呼ぶことは、原典の文書どおり呼び出し側の責任であり、本体は守らない（早い呼び出しへの防御は無い） |

壊しうる入力:

| 入力 | 何が起きうるか | 確かめているもの |
|:---|:---|:---|
| 容量0 | トークンは貯まらず、コストの分だけ常に待つ | ケース4 |
| コスト0 | 残りが空でも遅延0で、何度呼んでも状態が変わらない | ケース10、PBT |
| コストが容量より大きい | 複数周期分の遅延。残りは0になり、`last_update` は将来の格子の点へ進む | ケース3、ケース9 |
| 負のコスト | `TokenBucketError::NegativeCost`（原典は例外）。状態は変えない | 単体テスト（原典外。ケースには無い） |
| 容量が負、周期が0以下 | 構築が `Err`（原典は `require` の例外）。ケースには無い | 単体テスト（原典外） |
| 現在時刻が負 | `t0` が負でも格子と差の計算は同じ | ケース7 |
| 現在時刻が `i64::MAX` の前後（折り返し） | 差が折り返し算術で正しく出るか | ケース8、ケース12（startTime に `i64::MIN` と `i64::MAX` を含む） |
| 非常に遅いレート（周期が `i64::MAX >> 10`） | `remaining_cost * p` と `now + delay` の折り返し。原典の文書どおり、遅延が正の `i64` で表せない場合は未定義であり、移植も折り返した値を返す（定義にしない） | ケース11。表せる範囲を超える入力は範囲の外と明記する |
| 遅延が経過する前の再呼び出し | 契約の外（T8）。本体は検出しない | 範囲の外（原典どおり） |
| `init` を呼ばない使用 | 残り0、`last_update = 0` で動き始める（原典どおり） | 範囲の外。API 文書に契約を書く |

PBT は足す。`token_bucket_property_test.rs` に置く。

- 性質1（理想バケットとの一致）：ケース12を一般化し、容量、周期、開始時刻（`i64::MIN`、負、0、`i64::MAX` の近傍を含む）、到着の間隔、コストの上限の組み合わせで、素朴な理想バケット（ティックごとに1トークン、容量で頭打ち）と発出時刻が一致することを確かめる。
- 性質2（不変条件）：ランダムな時刻の進み方とコストの列（コスト0と容量超えを含む）で、各 `offer` の後に T1 と T3 を確かめ、`last_update` が格子の上にあること（T2）を確かめる。時刻の跳びには、`i64::MAX` の近くまで一気に進める手を含める（F1 の和のあふれを踏むため）。
- 性質1の一般化では、到着の間隔の式はケース12の字面どおりの式と変えてもよいが、その場合は別の式であることをテストに明記する（F6）。ケース12の写し自体は原典の式のままにする。
- 乱数は固定 seed の SplitMix64。失敗時は seed、手の番号、設定を表示する。外部の依存は足さない。

## 4. TokenBucket の Quint の判断

**TokenBucket には Quint を書く。ImmutableIntMap には書かない。**

TokenBucket に書く理由:

- TokenBucket は、時刻の進みで状態（残りのトークンと `last_update`）が変わる状態機械であり、守るべき性質 T2（到着の格子）と T7（遅延から最後の到着後の経過時間を引くこと）は、遷移の列の全体で成り立つ性質である。素朴な遅延計算（引く項を落とす誤り）は、単純なケースを通り、ケース9のような特定の到着の位置でだけ落ちる。FrequencySketch の前例（PR #205）でも、リセットの大きさの取り違えは PBT と原典のケースを通り、Quint だけが落とした。
- モデルは小さくできる。状態は `available` と `lastUpdate` と現在時刻だけであり、遷移は `init`、`offer(cost)`、時刻の前進の3つである。

仕様の置き場所と形（F3）:

- 置き場所は `crates/fraktor-actor/specs/token_bucket.qnt` の1ファイル、1モジュールにする。CI（`scripts/quint-check-specs.sh`）が見るのは `crates/*/specs/*.qnt` だけであり、初版が置いた `docs/design/specs/` は対象の外で、検査が手作業のまま残る。`docs/design/specs/` の前例（FrequencySketch、DoubleLinkedList）の形（本体とテストを分けた複数モジュール）をそのまま `crates/*/specs/` へ移しても、モジュール名とファイル名の決まりで CI の script を通らない。TokenBucket のモデルは小さいので、CI の形に収める。設計レビューの試作では、この置き場所と形で CI の script がそのまま通り、typecheck、`4 passing`、`quint run` の `No violation found` が確かめられている。
- 設定は `pure val`（`Capacity`、`Period`、開始時刻の集合）に置く。`const` を持つ本体は `quint run` が通らないため使わない。
- 状態：`available: int`、`lastUpdate: int`、`now: int`、ゴースト `lastDelay: int` と `lastRemaining: int`（直前の `offer` の遅延と、その時点の不足分。`delayIsEarliest` のため。F4）、設定は上の `pure val`、`t0`（init の時刻）。
- 遷移：`init`、`advanceTime(d)`（d は0以上）、`offer(cost)`（戻り値の遅延も観測する）。`step` は `"none"` だけを使う。負の対照は `offerWith(bug, cost)` の引数で選び、対照は名前が `Test` で終わる `run` にして `.expect(not(...))` で、誤りが不変条件を崩すことを確かめる。全体の不変条件は `val invariant = ...` にまとめる（CI の決まり：`invariant` があること、`Test` で終わる `run` が1本以上あること、`step` が進むこと）。
- 不変条件：`availableWithinCapacity`（T1）、`delayNonNegative`（T3）、`lastUpdateOnGrid`（T2。`(lastUpdate - t0)` が `period` で割り切れる）、`arrivalCountMatchesElapsed`（T4 のうち割り算の形）、`delayIsEarliest`（T9。`lastDelay > 0` なら `(lastRemaining - 1) * Period < lastDelay <= lastRemaining * Period`。F4）。
- T8 の扱い：モデルの `offer` に `now >= lastUpdate` のガードを置く（F4）。置かないと、遅延が経過する前の早い呼び出し（契約の外）で遅延が `remaining * Period` を超え、`delayIsEarliest` が契約の外で崩れる。
- 負の対照：`bugNaiveDelay`（遅延から経過時間を引かない。`lastUpdateOnGrid` で落ちる）、`bugNoCapacityCap`（到着分を容量で頭打ちにしない。`availableWithinCapacity` で落ちる）、`bugExtraPeriod`（遅延を1周期長くする。`delayIsEarliest` で落ちる。格子の条件と符号の条件だけでは、この誤りは通ってしまう。F4）。
- モデルにしないもの：`i64` の折り返し（Quint の `int` は範囲が無い。折り返しのケース7、8、12は Rust のテストと PBT が確かめる）、実時計（時刻は非決定的な前進として抽象化する）。

ImmutableIntMap に書かない理由:

- ImmutableIntMap は時刻も内部の遷移の履歴も持たない不変の値であり、各操作の意味は「操作後のマップの中身」だけで決まる。その性質は、BTreeMap のモデルと毎手比べる PBT が直接確かめる。Quint でマップをモデル化しても、同じ照合を別の記法で書くことになり、新しく落ちる誤りの種類が増えない。DoubleLinkedList や FrequencySketch に Quint が要ったのは、リンクやカウンタの行列という、公開 API から直接は見えない内部状態の整合を守るためであり、ImmutableIntMap にはそれに当たる内部状態が無い（`entries` の並びは `keys` と `iter` でそのまま観測できる）。

## 5. 第2段ですること（本書の範囲の外）

- red（失敗するテスト）→ green の順に実装する。先に2つの spec ファイルを書き、コンパイルの失敗かテストの失敗で red を確認してから本体を足す。
- 台帳の対象ノード2件を更新する。ImmutableIntMapSpec は total／ported／passing を 10／10／10、TokenBucketSpec は 12／12／12 とし、`rust.crate` に `fraktor-actor`、`rust.test_file` に各テストファイル、`status: green`、`verified_against: v2.0.0` を書く。ケースの除外と `not_built` の提案は無いので、人間の判断を待つ項目は無い。
- CI と同じ検査を流す（`cargo fmt --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`、ledger/tools の Python テスト、Quint は CI の `scripts/quint-check-specs.sh` をそのまま流す。`crates/fraktor-actor/specs/token_bucket.qnt` が対象に入っていることを確かめる）。
- 既存の公開 API、型、モジュールには触れない。追加は `kernel::immutable_int_map` と `kernel::token_bucket` だけである。

## 6. 設計レビュー（PR #231、HEAD 96add60）への回答

判定は「要修正」（高1・中4・低4）だった。9件すべてを直し、採らなかった指摘は無い。判断の出どころは、設計レビューの指摘と、その試作による実測である。

| 番号 | 指摘 | 回答 |
|:---|:---|:---|
| F1（高） | `available_tokens + tokens_arrived` の桁あふれの扱いが決まっていない。素の `+` は panic し、wrapping では T1 が崩れる | 直した。到着分の加算は `saturating_add` にして容量で頭打ちにし、T1 を「すべての入力で成り立つ」と書き直した（§2、§3 の T1）。原典との差として記録し、あふれの入力の単体テストを1件足すことと、PBT の時刻の跳びに `i64::MAX` の近くを入れることを書いた |
| F2（中） | wrapping の一覧に `last_update +=` の2か所が抜けている | 直した。一覧に2か所を足し、「`last_update` と時刻の加減はすべて wrapping」と言い切った（§2）。`cost - available_tokens` があふれない理由も書いた。テスト側の時刻の加算（ケース8の `currentTime += 10`、ケース12の `startTime + time`）も `wrapping_add` で書くことを §1 に書いた |
| F3（中） | Quint を `docs/design/specs/` に置くと CI の対象外になる | 直した。置き場所を `crates/fraktor-actor/specs/token_bucket.qnt` の1ファイル、1モジュールに変え、設定は `pure val`、不変条件は `val invariant`、負の対照は `Test` で終わる `run` とする形を §4 に書き直した。§5 の検査手順も CI の script をそのまま流す形にした |
| F4（中） | 不変条件が、遅延が1周期だけ長すぎる誤りを捉えていない。T8 の扱いもモデルに書いていない | 直した。不変条件 T9（`delayIsEarliest`）を新設し、Quint にゴースト `lastRemaining` と負の対照 `bugExtraPeriod` を足した。モデルの `offer` に T8 のガード（`now >= lastUpdate`）を置くことも書いた。T2 の文言は「`remaining_cost` 個先の点」に直した |
| F5（中） | `TokenBucketClock` は `kernel::Clock` と重なっている。重ねる理由が成り立たない | 直した。専用の port をやめ、既存の `kernel::Clock` を使う形に書き直した（§2）。変換 `as_nanos() as i64` の意味（下位64ビット、差だけが意味を持つ）を API の形に書いた。テストの手動時計は `AtomicU64` で持つ形にし、`NanoTimeTokenBucket` は `TokenBucket<SystemClock>` が相当品になることを「作らないもの」に書いた |
| F6（低） | ケース12は、原典の更新式を字面どおりに写すと明記する | 直した。§1 のケース12に、更新式 `untilNextElement = time + Random.nextInt(arrivalPeriod)` を字面どおりに写すことと、`(idealBucket >= 0)` の確認を残すことを書いた。一般化した PBT の式を変える場合は、その旨を明記することも §3 に書いた |
| F7（低） | ImmutableIntMap の「複製を避ける分岐」は実現できない約束であり、M2 の PBT は受け手を確かめていない | 直した。同じ値での `updated` について、省けるのは書き込みだけで複製は必ず要る、と文言を直した（§2）。M2 は「型で保証」として PBT の対象から外し、受け手そのものを操作前のモデルと比べることを §3 に書いた |
| F8（低） | 表現は `Vec<[i32; 2]>` にすると、二分探索を core のものにできる（提案） | 採った。表現を `Vec<[i32; 2]>` に変え、`binary_search_by_key` を使う形にし、M1 を組の列の形に書き直した（§2、§3） |
| F9（低） | ファイルの表と型の細目が足りない | 直した。単体テストのファイル `token_bucket_value_test.rs` を表に足し、`TokenBucketError` の derive（`Debug, Clone, Copy, PartialEq, Eq`）と `Display`、`core::error::Error` の実装を §2 に書いた。ケース10の操作の比率（更新が3分の2、削除が3分の1）を §1 に書いた |
