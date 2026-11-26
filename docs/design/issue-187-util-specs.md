# 設計：#187 util の Index／WildcardIndex／MessageBuffer

[Issue #187](https://github.com/j5ik2o/fraktor-ng-rs/issues/187) の設計の記録である。Pekko v2.0.0（commit `9fbd2283b1cf701c90a1223bf621942c191db61f`）の actor-tests にある `IndexSpec`、`WildcardIndexSpec`、`MessageBufferSpec` の 3 本を Rust へ移植し、台帳の対象ノード 3 件を green にする。

この文書は第 1 段（設計だけ）である。実装は、別の担当がこの設計をレビューして通ってから、第 2 段で red → green の順に行う。この文書自体がドラフト PR の唯一の変更である。台帳は第 2 段で更新する（この段では `status: unported` のまま触らない）。

進め方の規約は `.takt/facets/knowledge/fraktor-porting-rules.md`、設計文書の前例は [PR #205](https://github.com/j5ik2o/fraktor-ng-rs/pull/205) の [issue-177-frequency-sketch.md](issue-177-frequency-sketch.md) と `docs/goals/issue-177-frequency-sketch.md` に従う。ノード 1 件＝テストファイル 1 件、ファイル冒頭に出典（Pekko v2.0.0 のファイルパス）をコメントで残す。

## 1. 何を移植するか

### 件数の数え直し

原典の `" in {"` の数を数え直した。台帳の目安（`test_cases_approx`）と 3 本とも一致する。`ignore` と `pending` は無い。

| 原典 | 目安 | 数え直し | 内訳 |
|:---|---:|---:|:---|
| `actor-tests/src/test/scala/org/apache/pekko/util/IndexSpec.scala` | 7 | **7** | 「An Index must」の下に 7 件 |
| `actor-tests/src/test/scala/org/apache/pekko/util/WildcardIndexSpec.scala` | 8 | **8** | 「wildcard index must」の下に 8 件 |
| `actor-tests/src/test/scala/org/apache/pekko/util/MessageBufferSpec.scala` | 9 | **9** | 「A MessageBuffer must」の下に 7 件、「A MessageBufferMap must」の下に 2 件 |
| 合計 | 24 | **24** | |

台帳の対象ノードは次の 3 件で、どれも `status: unported`、`triage.kind: pending` である。

- `actor-tests/src/test/scala/org/apache/pekko/util/IndexSpec.scala`
- `actor-tests/src/test/scala/org/apache/pekko/util/WildcardIndexSpec.scala`
- `actor-tests/src/test/scala/org/apache/pekko/util/MessageBufferSpec.scala`

### 対応表：IndexSpec（7 件）

移す先は `crates/fraktor-actor/tests/index_spec.rs`。原典は `PekkoSpec`（ActorSystem と dispatcher）を使うが、ケースが使うのは `Index` 本体と、並行ケースの `Future`／`Await` だけである。移植は ActorSystem を立てず、並行ケースは `std::thread` で書く。台帳の `needs.testkit`（`extends-pekko-spec`、`pekko-spec`、`default-timeout`）と `premise_flags: ['random']` は原典側の都合の記録であり、移植は TestKit を使わず、乱数も決定的なものに置き換える（§5）。この置き換えはケースの意味を変えない。

| # | 原典のケース | Rust のテスト名 | 移す／作らない |
|:---:|:---|:---|:---|
| 1 | take and return a value | `take_and_return_a_value` | 移す |
| 2 | take and return several values | `take_and_return_several_values` | 移す。重複した `put` が `false` を返す検査を含む |
| 3 | remove values | `remove_values` | 移す。値の削除とキーごとの削除（`remove(key) -> Option`）の両方 |
| 4 | remove the specified value | `remove_the_specified_value` | 移す。`remove_value` が全キーから値を消す |
| 5 | apply a function for all key-value pairs and find every value | `apply_a_function_for_all_pairs_and_find_every_value` | 移す。`for_each` の訪問回数 6 と、各ペアの `find_value` |
| 6 | be cleared | `be_cleared` | 移す |
| 7 | be able to be accessed in parallel | `be_able_to_be_accessed_in_parallel` | 移す（形を変える）。10,000 手をスレッドで並行に流す。乱数はスレッドごとの決定的な SplitMix64 に置き換え、`Await` の完了待ちは `join` に置き換える。原典が読む側で課す条件（キー 5〜9 の値集合は空でない。削除はキー 0〜4 だけが対象なので保たれる）はそのまま残す。原典は最終状態を検査しないので、移植も最終状態の一致は検査せず、panic が無く全手が完了することと、読む側の条件だけを検査する |

作らないケースは無い（7 件全件を移す）。

### 対応表：WildcardIndexSpec（8 件）

移す先は `crates/fraktor-actor/tests/wildcard_index_spec.rs`。原典は `AnyWordSpec` だけで、TestKit は使わない。

| # | 原典のケース | Rust のテスト名 | 移す／作らない |
|:---:|:---|:---|:---|
| 1 | allow to insert elements using Arrays of strings | `allow_to_insert_elements_using_arrays_of_strings` | 移す。戻り値の型の検査（`shouldBe a[WildcardIndex[?]]`）は、Rust では戻り値を次の呼び出しに使えることと型注釈で表す。空の配列への挿入を含む |
| 2 | allow to find inserted elements | `allow_to_find_inserted_elements` | 移す |
| 3 | match all elements in the subArray when it contains a wildcard | `match_all_elements_in_the_sub_array_when_it_contains_a_wildcard` | 移す。`"a"` と `"a/*"` の両方を持つ木で、`find(["a"])` が 1 を返す（末尾の要素が余る経路でも先に一致した節の値を返す）ことを含む |
| 4 | match all elements in the subArray when it contains a wildcard suffix | `match_all_elements_in_the_sub_array_when_it_contains_a_wildcard_suffix` | 移す。`"a*"`、`"c*"`、`"b*"` の接頭辞一致 |
| 5 | fail when a double wildcard is used as a suffix | `fail_when_a_double_wildcard_is_used_as_a_suffix` | 移す（形を変える）。原典は `IllegalArgumentException` を期待する。移植は例外ではなく `Err(WildcardIndexError::DoubleWildcardAsSuffix)` を返す設計にした（§2.2）。テストは `Err` を検査する |
| 6 | never find anything when emptyIndex | `never_find_anything_when_empty` | 移す |
| 7 | match all remaining elements when it contains a terminal double wildcard | `match_all_remaining_elements_when_it_contains_a_terminal_double_wildcard` | 移す。末尾の `"**"` が残りの要素すべてに一致する |
| 8 | ignore non-terminal double wildcards | `ignore_non_terminal_double_wildcards` | 移す。途中の `"**"` は通常の要素として扱われ、任意要素には一致しない |

作らないケースは無い（8 件全件を移す）。

### 対応表：MessageBufferSpec（9 件）

移す先は `crates/fraktor-actor/tests/message_buffer_spec.rs`。原典は `AnyWordSpec` だけで、TestKit は使わない。原典の送信者は、文字列から暗黙変換で作る `DummyActorRef`（`toString` が id）である。移植はバッファをメッセージ型と送信者型のジェネリクスにし、テストではどちらも `&str` を使う（§2.3）。

| # | 原典のケース | Rust のテスト名 | 移す／作らない |
|:---:|:---|:---|:---|
| 1 | answer empty correctly | `answer_empty_correctly` | 移す。`nonEmpty` は Rust では `!is_empty()` で表し、独立のメソッドは作らない |
| 2 | append and drop | `append_and_drop` | 移す。空のバッファへの `drop_head` が件数を 0 のままにすることも含む。`head()` は空でないときにだけ呼ぶ（原典どおり） |
| 3 | process elements in the right order | `process_elements_in_the_right_order` | 移す。FIFO の順序を `for_each` で確かめる |
| 4 | filterNot of first | `filter_not_of_first` | 移す |
| 5 | filterNot of last | `filter_not_of_last` | 移す |
| 6 | filterNot of single entry | `filter_not_of_single_entry` | 移す。削除と追加を混ぜた 13 手の列 |
| 7 | filterNot of several entries | `filter_not_of_several_entries` | 移す |
| 8 | support contains, add, append and remove（MessageBufferMap） | `message_buffer_map_supports_contains_add_append_and_remove` | 移す |
| 9 | handle multiple message buffers（MessageBufferMap） | `message_buffer_map_handles_multiple_message_buffers` | 移す |

作らないケースは無い（9 件全件を移す）。

## 2. 本体の有無と、足すものの形

### 2.0 既存実装の有無

`crates/` の全体を `Index`、`WildcardIndex`、`ConcurrentMultiMap`、`MessageBuffer`、`MessageBufferMap` で検索した。対応する本体は**どれも存在しない**（引っかかるのは台帳と `docs/baseline/pekko-v1.7.1/ledger.yaml` の記録だけである）。`crates/fraktor-actor/src/kernel/byte_string/index_out_of_bounds_error.rs` の `IndexOutOfBoundsError` は無関係である。したがって 3 つとも新規に足す。既存の公開 API、型、モジュールには触れず、追加は `kernel` の下の 3 モジュールだけである。

配置は前例（`frequency_sketch`、`double_linked_list`）と同じ形にする。

| ファイル | 中身 |
|:---|:---|
| `crates/fraktor-actor/src/kernel.rs` | `pub mod index;`、`pub mod message_buffer;`、`pub mod wildcard_index;` の 3 行 |
| `crates/fraktor-actor/src/kernel/index.rs` ＋ `index/index_value.rs` | `Index<K, V>` |
| `crates/fraktor-actor/src/kernel/wildcard_index.rs` ＋ `wildcard_index/wildcard_index_value.rs`、`wildcard_index_error.rs` | `WildcardIndex<T>`、`WildcardIndexError`（木は非公開） |
| `crates/fraktor-actor/src/kernel/message_buffer.rs` ＋ `message_buffer/message_buffer_value.rs`、`message_buffer_map_value.rs` | `MessageBuffer<M, S>`、`MessageBufferMap<I, M, S>` |
| `crates/fraktor-actor/tests/index_spec.rs`、`wildcard_index_spec.rs`、`message_buffer_spec.rs` | 原典 7 件、8 件、9 件 |
| `crates/fraktor-actor/tests/index_model_test.rs`、`wildcard_index_model_test.rs`、`message_buffer_model_test.rs` | PBT（§4）。原典外で、台帳の件数に含めない |

### 2.1 Index

原典 `org.apache.pekko.util.Index[K, V](mapSize, valueComparator)` は `ConcurrentHashMap[K, ConcurrentSkipListSet[V]]` で、キーごとに値の**ソート済み集合**を持つマルチマップである。追加と削除はキーごとに直列化され、読み取りは並行である。原典の `ConcurrentMultiMap` は `Index` の中身のないサブクラスである。

Rust への写し方で決める点は 3 つある。

1. **比較器を保つ。** 値の重複判定と順序は `Comparator` が決める（`compare == 0` が同じ値）。`V: Ord` の自然順序に置き換えると、原典の契約（比較器で同一視される別の値は入らない）が落ちる。したがって比較器を `Arc<dyn Fn(&V, &V) -> Ordering + Send + Sync>` として持ち、キーごとの値集合は比較器でソートした `Vec<V>` で持つ（二分探索で挿入位置と既存の値を探す。挿入と削除は O(n) になるが、原典のケースと実際の値集合の大きさでは問題にならず、正確さ
...[truncated 14061 chars]を優先する）。`K` は `alloc` の `BTreeMap` で引くため `K: Ord` を要求する（原典のケースの `String` と `Int` はどちらも満たす。ハッシュ表が要る場合は std 側の仕事であり、この型の範囲外とする）。

2. **並行性は 1 本のロックで写す。** 原典はキーごとに直列化するが、観測できる契約は「各操作が原子的であること」である。全体を `spin::Mutex<BTreeMap<K, Vec<V>>>`（`spin` は既存の依存で、`event_stream_bus` が同じ使い方をしている）で守れば、各操作は線形化可能になり、原典より強い保証になる。メソッドはすべて `&self` で受け、`Index` 自身を `Arc` で共有すればスレッドをまたいで使える（`K`、`V` が `Send` のとき `Sync`）。反復はロックを保持したまま利用者のクロージャを呼ばないよう、対象をスナップショットへ複製してから処理する（`value_iterator`、`for_each`、`values`、`keys`。§3 の I6）。

3. **`map_size` は上限ではない。** 原典では `ConcurrentHashMap` の初期容量の目安である。移植でも `usize` の目安として受け取り、検証も上限化もしない。0 も受け付ける（原典の負値は `usize` では表現できない）。

公開 API の形（シグネチャは要点だけを示す）:

```rust
pub struct Index<K, V> { /* Arc<spin::Mutex<BTreeMap<K, Vec<V>>>>, comparator */ }

impl<K: Ord, V> Index<K, V> {
  pub fn new(map_size: usize, comparator: impl Fn(&V, &V) -> Ordering + Send + Sync + 'static) -> Self;
  pub fn put(&self, key: K, value: V) -> bool;               // 新規なら true
  pub fn remove(&self, key: &K, value: &V) -> bool;          // 値の削除
  pub fn remove_key(&self, key: &K) -> Option<Vec<V>>;       // 原典の remove(key)
  pub fn remove_value(&self, value: &V);                     // 全キーから削除
  pub fn find_value(&self, key: &K, f: impl FnMut(&V) -> bool) -> Option<V> where V: Clone;
  pub fn value_iterator(&self, key: &K) -> Vec<V> where V: Clone;  // スナップショット
  pub fn for_each(&self, f: impl FnMut(&K, &V));
  pub fn values(&self) -> Vec<V> where V: Clone;             // 全キーの和集合（重複なし）
  pub fn keys(&self) -> Vec<K> where K: Clone;
  pub fn is_empty(&self) -> bool;
  pub fn clear(&self);
}
pub type ConcurrentMultiMap<K, V> = Index<K, V>;  // 原典のサブクラスに対応する別名
```

原典と違える点: `valueIterator` は原典では弱く一貫したライブ反復子だが、移植はロックを解放できるスナップショット（`Vec`）を返す。検査の内容（集合としての一致）は変わらない。`remove(key)` は原典では `remove_key` に改名する（Rust では引数だけが違う同名メソッドを作れないため）。反復の順序は、原典の `ConcurrentSkipListSet` と同じく比較器の順である。

### 2.2 WildcardIndex

原典 `private[pekko] final case class WildcardIndex[T]` は不変の木であり、`insert` は新しい `WildcardIndex` を返す。木は 2 本ある。末尾の要素が `"**"` の経路は `doubleWildcardTree` へ、それ以外は `wildcardTree` へ入る。`find` はまず `wildcardTree` を単一ワイルドカードの規則で探し、無ければ `doubleWildcardTree` を末尾の二重ワイルドカードの規則で探す。

```rust
pub struct WildcardIndex<T> { /* wildcard_tree, double_wildcard_tree: WildcardTree<T> */ }

impl<T: Clone> WildcardIndex<T> {
  pub fn new() -> Self;  // Default も実装する
  pub fn insert(&self, elems: &[&str], value: T) -> Result<Self, WildcardIndexError>;
  pub fn find(&self, elems: &[&str]) -> Option<&T>;
  pub fn is_empty(&self) -> bool;
}

pub enum WildcardIndexError { DoubleWildcardAsSuffix /* 問題の要素を保持する */ }
```

形の判断:

- **不変のままにする。** `insert` は `&self` から新しい木を返し、書き換えるのは経路上の節だけである（子は `BTreeMap<String, WildcardTree<T>>` を複製する。前例の `FrequencySketch` 等と違い共有ポインタは使わない。経路の長さがそのまま複製の量になる）。原典の `find(elems: Iterable[String])` は `&[&str]` で受ける。
- **例外は `Err` にする。** `"**"` で終わるが `"**"` 自体ではない要素（例：`"a**"`、`"***"`）は、原典では挿入の走査がその要素へ着いたときに `IllegalArgumentException` を投げる。移植は `FrequencySketch` の前例（原典の `require` の例外をエラーの値にした）に従い、`Result` で返す。ケース 5 はこの `Err` を検査する形に変わる。
- **要素の判定順は原典のままにする。** `"**"` の接尾辞検査 → `"*"` でも `"**"` でもなく `"*"` で終わる要素は接頭辞（接尾辞 `"*"` を除いた文字列）をキーにする接頭辞子へ → それ以外は通常の子へ。`"**"` は通常の子のキーとして入りうる（末尾でない二重ワイルドカードと、末尾の二重ワイルドカードを持つ木の中段）。
- **接頭辞子の既存引きの癖をそのまま写す。** 原典の `WildcardTree.insert` は、接頭辞子を引くときに除いた後のキーではなく要素そのもの（例：`"a*"`）で `getOrElse` する。そのため、同じ接頭辞パターンを別の続きで再挿入すると、前の部分木が落ちる。この癖は原典のケースには現れない。第 2 段では、原典のコードの字面どおりに写すことを提案する（直すと原典との一致が崩れる）。PBT の入力生成ではこの癖を前提にモデル側も同じ引き方をし、癖そのものは単体テストで固定する。レビューで「癖を直す」判断が出れば、モデルと単体テストを直した形に合わせる。
- **接頭辞の複数一致の順序は決定的にする。** 原典の `find` は接頭辞子を `HashMap` の走査順（不定）で最初に一致したものを取る。移植は `BTreeMap` のキー順（決定的）で取る。複数の接頭辞が同時に一致する入力では結果が原典と変わりうる。原典のケースには現れない。決定的であることを優先し、違える点としてここに明記する。
- 木（`WildcardTree`）は原典どおり非公開にする。深さは経路の長さであり、挿入と検索は再帰で書く（原典も再帰）。極端に長い経路ではスタックが伸びる点は §5 に書く。

### 2.3 MessageBuffer と MessageBufferMap

原典 `MessageBuffer` はスレッド安全でない可変の FIFO バッファで、要素は `(Any, ActorRef)` である。原典 `MessageBufferMap[I]` は id ごとのバッファの表である。

```rust
pub struct MessageBuffer<M, S> { /* alloc::collections::VecDeque<(M, S)> */ }

impl<M, S> MessageBuffer<M, S> {
  pub fn empty() -> Self;  // new と Default も用意するかは実装時に統一する
  pub fn is_empty(&self) -> bool;
  pub fn size(&self) -> usize;
  pub fn append(&mut self, message: M, sender: S) -> &mut Self;
  pub fn drop_head(&mut self);
  pub fn head(&self) -> Option<(&M, &S)>;
  pub fn for_each(&self, f: impl FnMut(&M, &S));
  pub fn filter_not(&mut self, p: impl FnMut(&M, &S) -> bool);
}

pub struct MessageBufferMap<I, M, S> { /* BTreeMap<I, MessageBuffer<M, S>> */ }

impl<I: Ord, M, S> MessageBufferMap<I, M, S> {
  pub fn new() -> Self;
  pub fn is_empty(&self) -> bool;
  pub fn size(&self) -> usize;        // id の数
  pub fn total_size(&self) -> usize;  // 全バッファの要素数の合計
  pub fn add(&mut self, id: I);
  pub fn append(&mut self, id: I, message: M, sender: S);
  pub fn remove(&mut self, id: &I);
  pub fn contains(&self, id: &I) -> bool;
  pub fn get(&self, id: &I) -> Option<&MessageBuffer<M, S>>;
  pub fn for_each(&self, f: impl FnMut(&I, &MessageBuffer<M, S>));
}
```

形の判断:

- **ジェネリクスにする。** 原典の `(Any, ActorRef)` をそのまま写すと、テストが `ActorRef` を作る手段を必要とするが、この crate の `ActorRef` の構築は `pub(crate)` で、テスト用のダミー送信者を作れない。原典のテスト自身も本物の送信者ではなく文字列 id を持つダミーで済ませている。したがって要素は `<M, S>` のジェネリクスにし、テストは `M = &str`、`S = &str` で書く。本体が具体的な型を必要としないことも、この形を支持する。
- **中身は `VecDeque` にする。** 原典は単方向リンクのノード列で、`filterNot` は残すノードを集めて繋ぎ直す。`alloc::collections::VecDeque<(M, S)>` で観測できる意味（FIFO の順序、件数、先頭）は同じになり、手書きのリンク繋ぎ直しという壊れ方そのものが消える。
- **`head()` の空の扱いは `Option` にする。** 原典は空のときに `(null, null)` を返す。Rust では `None` を返す。原典のケースは空のバッファで `head()` を呼ばないので、ケースの意味は変わらない。
- **`get_or_empty` は `get -> Option` にする。** 原典は無い id に対して、その場限りの空のバッファを返す（表には登録されない）。Rust で参照を返す形ではこれは書けない。ケース 8 と 9 は `getOrEmpty` の結果に `isEmpty` と `foreach` だけを呼ぶので、`Option` の有無と `None` の場合の空扱いで同じ検査になる。表へ登録されない性質（`get` の後に `contains` が変わらない）は `Option` の形で自明に保たれる。
- **作らないもの。** `MessageBufferMap::drop`（落とした要素ごとに `Dropped` を dead letters へ送る）は、原典のケースが使わず、`ActorRef` への具体的な送信を伴うので、この issue では作らない。Java API（`getHead`、`forEach(Procedure2)`、表の `forEach`）も作らない。`MessageBuffer::getHead` も同様である。これらはケースの「作らない」区分には当たらない（ケースは全件移す）。

### 2.4 no_std

3 つとも `core` と `alloc` だけで書ける。`Index` は `alloc::{collections::BTreeMap, vec::Vec, sync::Arc, string::String}` と既存依存の `spin::Mutex`（`default-features = false` で no_std 対応）を使う。`WildcardIndex` は `alloc::{collections::BTreeMap, string::String, vec::Vec}` だけである。`MessageBuffer` は `alloc::collections::{VecDeque, BTreeMap}` だけである。std を必要とするのは並行ケースのテスト（`std::thread`）だけであり、テスト crate は std でよい。前例どおり、クレート全体は `#![no_std]` ではないが、新しいモジュールは `std` を import しない。

## 3. 守る性質（不変条件）

### Index

| 番号 | 不変条件 |
|:---|:---|
| I1 | キーごとに、値は比較器でソートされ、`compare == 0` の重複は無い |
| I2 | キーが表に現れるのは、その値集合が空でないときだけである（最後の値を消す操作はキーを消す） |
| I3 | `put` の戻り値は、その（キー、値）が操作の前に無かったときに限って `true`。`remove(key, value)` と `remove_key` の戻り値も同様に、実際に消えたときに限って成功を返す |
| I4 | `remove_value(v)` の後は、どのキーも `v`（と比較器で等しい値）を含まない。空になったキーは消える |
| I5 | `clear()` の後は `is_empty()` が `true` で、すべてのキーの値集合が空である |
| I6 | 反復系（`value_iterator`、`for_each`、`values`、`keys`）は、呼び出し時点のスナップショットを見る。反復の途中でほかのスレッドが書き換えても、反復の内容は変わらず、ロックも保持し続けない |
| I7 | 並行性：すべての操作はロックの下で原子的であり、操作の列は逐次実行のどれかと等価である（線形化可能性）。原典のキーごとの直列化より強い |

### WildcardIndex

| 番号 | 不変条件 |
|:---|:---|
| W1 | `find` の優先順位は、単一ワイルドカードの木が先、二重ワイルドカードの木が後である。単一の木の中では、完全一致の子、`"*"` の子、接頭辞子の順に試す |
| W2 | 末尾の `"**"` は、残りの要素が 0 個より多いときに残りすべてへ一致する。`find` の要素が経路の節で尽きた場合は、その節の値（あれば）を返す（ケース 3 の `find(["a"])`） |
| W3 | 空の経路への `insert` は木を変えない。何も入れていない木の `find` は、空の経路を含め、常に `None` である |
| W4 | 同じ経路への再挿入は値を置き換える。違う経路の値は互いに影響しない（§2.2 の接頭辞子の癖を除く） |
| W5 | `"**"` で終わるが `"**"` でない要素を含む経路の `insert` は `Err` を返し、木を変えない（`insert` は `&self` から作るので、失敗時は元の木がそのまま残る） |
| W6 | 末尾でない `"**"` は通常の要素としてだけ扱われ、任意の要素には一致しない |

### MessageBuffer と MessageBufferMap

| 番号 | 不変条件 |
|:---|:---|
| M1 | `size()` は常に要素数に等しい。空のバッファへの `drop_head` は件数を負にしない（何もしない） |
| M2 | 順序は FIFO である。`for_each` は入れた順に訪問し、`head()` は最も古い要素を返す |
| M3 | `filter_not(p)` の後は、`p` を満たした要素だけが消え、残りの相対順序は変わらない。`size()` は残りの件数に等しい |
| M4 | `drop_head` の後は、残りの順序は変わらず、件数は 1 減る（空でない場合） |
| M5 | 表の `total_size()` は全バッファの件数の合計に等しい。`add` は空のバッファを登録し、件数は増やさない。`remove` はバッファごと消す。無い id への `get` は表を変えない |

## 4. PBT を足すか

足す。3 つとも、依存を足さない形（固定 seed の SplitMix64 で操作列を作り、単純なモデルと毎手比べる。前例の `frequency_sketch_property_test.rs` と `double_linked_list_model_test.rs` と同じ方針）にする。`proptest` を使うかは [#212](https://github.com/j5ik2o/fraktor-ng-rs/issues/212) の決定に従い、この issue では足さない。PBT は原典外であり、台帳の件数に含めない。

| 対象 | 性質 | 確かめる不変条件 |
|:---|:---|:---|
| Index | ランダムな `put`／`remove`／`remove_key`／`remove_value`／`clear` の列を、`BTreeMap<K, BTreeSet<V>>` のモデル（比較器が自然順序の場合）と毎手比べる。戻り値、各キーの値集合、`is_empty` が一致する | I1〜I5 |
| Index | 比較器が自然順序と違う列（例：絶対値で比べる）でも、重複判定が比較器に従う | I1、I3 |
| WildcardIndex | ランダムな経路の挿入列と検索列を、（経路、値）の列に対する素朴な照合モデルと比べる。モデルは W1 の優先順位と W2、W6 をそのまま書き、接頭辞子は §2.2 の癖と同じ引き方をする。要素の部品は、通常の文字列、`"*"`、`"**"`、接頭辞パターン、空文字、二重ワイルドカード接尾辞（`Err` の確認）から選ぶ | W1〜W6 |
| MessageBuffer | ランダムな `append`／`drop_head`／`filter_not` の列を `VecDeque` のモデルと毎手比べる。`size`、`head`、`for_each` の列が一致する | M1〜M4 |
| MessageBufferMap | ランダムな `add`／`append`／`remove` の列をモデルと比べる。`total_size`、`contains`、各 id の内容が一致する | M5 |

## 5. Quint で書くかどうか

**3 つとも Quint は書かない。** 判断と理由は対象ごとに次のとおりである。

- **Index（書かない）。** 逐次意味論の不変条件 I1〜I5 は、§4 のモデル PBT が操作列の全体で毎手確かめるので、Quint の跡の検査と重なる。原典の特徴である並行性については、Quint はスレッドとロックをモデルにしないため、確かめる対象にならない。移植の並行性は「1 本の `spin::Mutex` の下で全操作が原子的」という実装の形そのもので保証し、実行では並行ケース（ケース 7）と、PBT のモデルと並行に流す追加テストではなくケース 7 の決定的な乱数化で確かめる。壊れ方として残るのはロックの取り忘れであり、これは Quint ではなく型（`&self` のメソッドがすべて同じロックを通ることのコードレビュー）と実行テストの担当である。
- **WildcardIndex（書かない）。** 状態遷移を持つ可変の構造ではなく、`insert` が新しい値を返す純粋な関数である。不変条件は「挿入列の後の `find` の結果」であり、§4 のモデル PBT が優先順位を含む照合規則そのものをモデルとして確かめる。遷移の列でしか出ない壊れ方（リンクの繋ぎ間違い等）は、この構造には無い。
- **MessageBuffer（書かない）。** 原典の壊れ方は `filterNot` の手書きの繋ぎ直しに集中するが、移植は `VecDeque` を使うため、その壊れ方自体が存在しない（§2.3）。残る不変条件 M1〜M4 は FIFO の順序と件数だけであり、§4 のモデル PBT が毎手確かめる。`DoubleLinkedList`（#176）で Quint が要った理由は、自前で前後のリンクを書き換える遷移が複数あったことであり、その前提がここには無い。

並行ケース（Index のケース 7）の順序の扱い: 原典はキー 5〜9 の値集合が削除対象でないことだけに依存し、最終状態を検査しない。移植も同じ前提だけを使い、スレッドごとの決定的な乱数列（スレッド番号を seed に混ぜる）で再現可能にする。操作の競合で結果が変わるのは戻り値と中間の内容だけであり、ケースの合否はそのどちらにも依存しない。

## 6. 壊しうる入力

| 対象 | 入力 | 何が起きうるか | 確かめるもの |
|:---|:---|:---|:---|
| Index | 空文字のキー、空文字の値 | 通常のキーと値として扱われる | PBT の部品に含める |
| Index | 比較器が全値を等しいと判定する | 最初の 1 件だけが入り、以降の `put` はすべて `false` | §4 の比較器の PBT |
| Index | 比較器が順序の公理を満たさない（非推移など） | 二分探索の結果が不定になる。原典の `ConcurrentSkipListSet` も比較器の契約違反では保証しないので、呼び出し側の前提として API 文書に書く | 範囲の外（前提として明記） |
| Index | `map_size` に 0 や極端に大きい値 | 目安なので何も起きない。確保は行わない | 単体テストで 0 を確認 |
| Index | 並行した `put` と `remove_key` の競合（原典の spin-put が扱う競合） | ロックで直列化されるので、原典のリトライ経路自体が存在しない。結果は逐次実行のどれかと等価 | ケース 7 |
| WildcardIndex | 空文字の要素 `""` | 通常の要素として一致する。接頭辞パターンにはならない | PBT の部品に含める |
| WildcardIndex | ワイルドカードの連続 `"*","*"`、`"**","**"` | `"*","*"` は各段で任意の 1 要素に一致する。`"**","**"` は末尾が `"**"` なので二重の木へ入り、中段の `"**"` は通常の子になる | PBT の部品に含める |
| WildcardIndex | `"a**"`、`"***"`、`"*a"` | `"a**"` と `"***"` は `Err`。`"*a"` は `"*"` で終わらないので通常の要素 | ケース 5 と PBT |
| WildcardIndex | 同じ接頭辞パターンの別の続きへの再挿入 | §2.2 の癖で前の部分木が落ちる（原典どおりに写す提案） | 単体テストで固定 |
| WildcardIndex | 複数の接頭辞が同時に一致する検索 | 原典は `HashMap` の走査順で不定、移植はキー順で決定的（§2.2） | PBT のモデルはキー順で書く |
| WildcardIndex | 極端に長い経路（数千要素） | 再帰の深さが経路長になり、スタックが伸びる。原典も再帰であり、ケースの範囲では問題にならない | 範囲の外（明記する） |
| MessageBuffer | 空のバッファへの `drop_head` の繰り返し | 件数は 0 のまま（M1）。原典のケース 2 が含む | ケース 2 |
| MessageBuffer | 空のバッファへの `filter_not` | 何も起きない | PBT の初期状態に含める |
| MessageBuffer | 大量の追加（数万件）と `filter_not` の繰り返し | `VecDeque` の再確保だけが起きる。件数と順序は保たれる | PBT の長い列 |
| MessageBufferMap | 無い id への `append` | バッファが作られて登録される（原典の `getOrAddBuffer` と同じ。ケース 9 がこの経路を使う） | ケース 9 |
| MessageBufferMap | 無い id への `remove`、`get` | 何も起きない、`None` が返る | PBT の部品に含める |

「境界の時間の値」に当たる入力は、この 3 つには無い（時刻を扱う型ではない）。

## 7. 影響

- **公開 API**: 追加だけである。`fraktor_actor::kernel::index::Index`（と別名 `ConcurrentMultiMap`）、`kernel::wildcard_index::{WildcardIndex, WildcardIndexError}`、`kernel::message_buffer::{MessageBuffer, MessageBufferMap}`。既存の型と関数は変えない。
- **台帳**: 第 2 段で対象ノード 3 件を green（7/7/7、8/8/8、9/9/9、`verified_against: v2.0.0`）にし、`docs/baseline/v2.0.0-undocumented-nodes.tsv` を作り直す。ゴール文書 `docs/goals/issue-187-util-specs.md` は第 2 段で作る。
- **依存**: 足さない。`spin` は既存の依存を使うだけである。
- **計算量**: `Index` の値の挿入と削除はキーごとの値の数に比例する（ソート済み `Vec`）。`WildcardIndex` の `insert` は経路長と、その経路の子の数に比例する。`MessageBuffer` は原典と同じく追加と先頭の削除が償却定数、`filter_not` は件数に比例する。

## 8. レビューで見てほしい点

1. **Index の比較器の形（§2.1）。** `Arc<dyn Fn>` で比較器を保ち、値集合をソート済み `Vec` にする案である。対案は `V: Ord` にして比較器を落とす案であり、その場合は原典の契約（比較器による同一視）が落ちる。どちらを取るかを確認してほしい。
2. **Index の並行性の写し方（§2.1、§5）。** 全体 1 本のロックとスナップショット反復で、原典より強い保証にする案である。キーごとのロックに寄せる必要があるかを確認してほしい。
3. **WildcardIndex の接頭辞子の癖（§2.2）。** 原典の `getOrElse` のキーの取り違えを、そのまま写して単体テストで固定する提案である。癖を直して入れる判断もありうるので、どちらにするかを確認してほしい。
4. **WildcardIndex の `insert` の `Err`（§2.2）。** 例外を `Result` にする案である。対案は `panic` にする案であり、その場合はケース 5 の形が変わる。
5. **MessageBuffer のジェネリクス化と `head`／`get` の `Option` 化（§2.3）。** 原典の `(Any, ActorRef)` と `(null, null)` の返し方を写さない判断の妥当性を確認してほしい。
6. **Quint を 3 つとも書かない判断（§5）。** 特に Index の並行性を Quint の対象外とした理由と、MessageBuffer で `VecDeque` の採用により Quint の前提が消えたとした理由を確認してほしい。
7. **作らないもの（§2.3）。** `MessageBufferMap::drop` と Java API をこの issue の範囲外とした判断を確認してほしい。
