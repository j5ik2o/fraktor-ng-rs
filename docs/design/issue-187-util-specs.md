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
| 7 | be able to be accessed in parallel | `be_able_to_be_accessed_in_parallel` | 移す（形を変える）。10,000 手を、固定の本数（2 本以上）のスレッドで並行に流す。全スレッドは `Barrier` で一斉に始める（1 本のスレッドで流れて終わる形を避ける。設計レビュー M4）。乱数はスレッドごとの決定的な SplitMix64 に置き換え、`Await` の完了待ちは `join` に置き換える。原典が読む側で課す条件（キー 5〜9 の値集合は空でない。削除はキー 0〜4 だけが対象なので保たれる）はそのまま残す。原典は最終状態を検査しないので、移植も最終状態の一致は検査せず、panic が無く全手が完了することと、読む側の条件だけを検査する |

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

`crates/` の全体を `Index`、`WildcardIndex`、`ConcurrentMultiMap`、`MessageBuffer`、`MessageBufferMap` で検索した。対応する本体は**どれも存在しない**（引っかかるのは台帳と `docs/baseline/pekko-v1.7.1/ledger.yaml` の記録だけである）。`crates/fraktor-actor/src/kernel/byte_string/index_out_of_bounds_error.rs` の `IndexOutOfBoundsError` は無関係である。なお、原典の `LookupClassification`（イベントバスの分類）は内部で `Index` を使うが、fraktor の `kernel/event/lookup_event_bus.rs` は `Vec<(C, S)>` と `PartialEq` で書いてあり、この `Index` は使っていない。重なりは無い。この `Index` を `LookupEventBus` へ使わせるかどうかは別件とし、この issue では触れない（設計レビュー L4）。したがって 3 つとも新規に足す。既存の公開 API、型、モジュールには触れず、追加は `kernel` の下の 3 モジュールだけである。

配置は前例（`frequency_sketch`、`double_linked_list`）と同じ形にする。

| ファイル | 中身 |
|:---|:---|
| `crates/fraktor-actor/src/kernel.rs` | `pub mod index;`、`pub mod message_buffer;`、`pub mod wildcard_index;` の 3 行 |
| `crates/fraktor-actor/src/kernel/index.rs` ＋ `index/index_value.rs` | `Index<K, V>` |
| `crates/fraktor-actor/src/kernel/wildcard_index.rs` ＋ `wildcard_index/wildcard_index_value.rs`、`wildcard_index_error.rs` | `WildcardIndex<T>`、`WildcardIndexError`（木は非公開） |
| `crates/fraktor-actor/src/kernel/message_buffer.rs` ＋ `message_buffer/message_buffer_value.rs`、`message_buffer_map_value.rs` | `MessageBuffer<M, S>`、`MessageBufferMap<I, M, S>` |
| `crates/fraktor-actor/tests/index_spec.rs`、`wildcard_index_spec.rs`、`message_buffer_spec.rs` | 原典 7 件、8 件、9 件 |
| `crates/fraktor-actor/src/kernel/wildcard_index/wildcard_index_value_test.rs` | 補足の回帰テスト。意図した差 1 と差 2 の振る舞いを固定する。原典外で、台帳の件数に含めない |
| `crates/fraktor-actor/tests/index_model_test.rs`、`wildcard_index_model_test.rs`、`message_buffer_model_test.rs` | PBT（§4）。原典外で、台帳の件数に含めない |

### 2.1 Index

原典 `org.apache.pekko.util.Index[K, V](mapSize, valueComparator)` は `ConcurrentHashMap[K, ConcurrentSkipListSet[V]]` で、キーごとに値の**ソート済み集合**を持つマルチマップである。追加と削除はキーごとに直列化され、読み取りは並行である。原典の `ConcurrentMultiMap` は `Index` の中身のないサブクラスである。

Rust への写し方で決める点は 3 つある。

1. **比較器を保つ。** 値の重複判定と順序は `Comparator` が決める（`compare == 0` が同じ値）。`V: Ord` の自然順序に置き換えると、原典の契約（比較器で同一視される別の値は入らない）が落ちる。したがって比較器を `Box<dyn Fn(&V, &V) -> Ordering + Send + Sync>` として持ち（`LookupEventBus` の前例と同じ形。設計レビュー M2）、キーごとの値集合は比較器でソートした `Vec<V>` で持つ（二分探索で挿入位置と既存の値を探す。挿入と削除は O(n) になるが、原典のケースと実際の値集合の大きさでは問題にならず、正確さを優先する）。`K` は `alloc` の `BTreeMap` で引くため `K: Ord` を要求する（原典のケースの `String` と `Int` はどちらも満たす。ハッシュ表が要る場合は std 側の仕事であり、この型の範囲外とする）。

2. **並行性は 1 本のロックで写す。** 原典はキーごとに直列化するが、観測できる契約は「各操作が原子的であること」である。全体を `spin::Mutex<BTreeMap<K, Vec<V>>>`（`spin` は既存の依存で、`event_stream_bus` が同じ使い方をしている）で守れば、各操作は線形化可能になり、原典より強い保証になる。内側に `Arc` は持たない（設計レビュー M2。移植の規約の所有権モデルは `Arc` を「どうしても共有せざるを得ない場所」に限る）。`Index` は `spin::Mutex` を直に持ち、`Clone` は付けない。共有は呼ぶ側の仕事であり、`std::thread::scope` で `&Index` を渡すか、呼ぶ側が `Arc<Index>` で包む（`K`、`V` が `Send` のとき `Index` は `Sync`）。各公開メソッドはロックを 1 回だけ取り、読みと書きをその中で済ませる（`remove_value` と `clear` をキーごとにロックし直す形にしない）。ロックを持ったまま利用者のコードを呼ばない。反復系（`value_iterator`、`for_each`、`values`、`keys`）と `find_value` は、対象をスナップショットへ複製してロックを外してから利用者のクロージャを呼ぶ（設計レビュー M1。`spin::Mutex` は再入できないので、ロックの中でクロージャから `put` 等を呼ぶと止まったままになる。原典のケース 5 は `foreach` の中で `findValue` を呼ぶので、この形が要る）。例外は比較器であり、比較器はロックの中で呼ぶ。したがって「比較器の中からこの `Index` を呼ばないこと」を API 文書の前提に書く。

3. **`map_size` は上限ではない。** 原典では `ConcurrentHashMap` の初期容量の目安である。移植でも `usize` の目安として受け取り、検証も上限化もしない。0 も受け付ける（原典の負値は `usize` では表現できない）。

公開 API の形（シグネチャは要点だけを示す）:

```rust
pub struct Index<K, V> { /* spin::Mutex<BTreeMap<K, Vec<V>>>, comparator: Box<dyn Fn> */ }

impl<K: Ord, V> Index<K, V> {
  pub fn new(map_size: usize, comparator: impl Fn(&V, &V) -> Ordering + Send + Sync + 'static) -> Self;
  pub fn put(&self, key: K, value: V) -> bool;               // 新規なら true
  pub fn remove(&self, key: &K, value: &V) -> bool;          // 値の削除
  pub fn remove_key(&self, key: &K) -> Option<Vec<V>>;       // 原典の remove(key)
  pub fn remove_value(&self, value: &V);                     // 全キーから削除
  pub fn find_value(&self, key: &K, f: impl FnMut(&V) -> bool) -> Option<V> where V: Clone;  // 値を複製してロックを外してから f を呼ぶ
  pub fn value_iterator(&self, key: &K) -> Vec<V> where V: Clone;  // スナップショット
  pub fn for_each(&self, f: impl FnMut(&K, &V)) where K: Clone, V: Clone;  // スナップショットを取るための境界（設計レビュー L1）
  pub fn values(&self) -> Vec<V> where V: Clone;             // 全キーの和集合（重複なし）
  pub fn keys(&self) -> Vec<K> where K: Clone;
  pub fn is_empty(&self) -> bool;
  pub fn clear(&self);
}
pub type ConcurrentMultiMap<K, V> = Index<K, V>;  // 原典のサブクラスに対応する別名
```

原典と違える点: `valueIterator` は原典では弱く一貫したライブ反復子だが、移植はロックを解放できるスナップショット（`Vec`）を返す。検査の内容（集合としての一致）は変わらない。`remove(key)` は原典では `remove_key` に改名する（Rust では引数だけが違う同名メソッドを作れないため）。反復の順序は、原典の `ConcurrentSkipListSet` と同じく比較器の順である。

`values()` の重複の除き方は比較器で決める（設計レビュー L2）。原典は和集合を Scala の `Set` で作り、`equals` で同一視する。移植の `V` には `Eq` を要求しないので、比較器で等しい値を 1 件にする。比較器が `equals` と違う場合（例：絶対値で比べる比較器で、キー s1 に 1、キー s2 に -1 がある場合）は、原典の `{1, -1}` に対して移植は 1 件になる。原典のケースは `values` を使わない。違える点としてここに明記する。

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

- **不変のままにする。** `insert` は `&self` から新しい木を返す（共有ポインタは使わない。複製と書き換えの進め方は下の M3 の項のとおり）。原典の `find(elems: Iterable[String])` は `&[&str]` で受ける。
- **例外は `Err` にする。** `"**"` で終わるが `"**"` 自体ではない要素（例：`"a**"`、`"***"`）は、原典では挿入の走査がその要素へ着いたときに `IllegalArgumentException` を投げる。移植は `FrequencySketch` の前例（原典の `require` の例外をエラーの値にした）に従い、`Result` で返す。ケース 5 はこの `Err` を検査する形に変わる。
- **要素の判定順は原典のままにする。** `"**"` の接尾辞検査 → `"*"` でも `"**"` でもなく `"*"` で終わる要素は接頭辞（接尾辞 `"*"` を除いた文字列）をキーにする接頭辞子へ → それ以外は通常の子へ。`"**"` は通常の子のキーとして入りうる（末尾でない二重ワイルドカードと、末尾の二重ワイルドカードを持つ木の中段）。
- **接頭辞子まわりには、原典の不具合として直す点が 2 つある（元帥の決定）。** 詳しくは §2.2 の「原典との意図した差」に書く。(1) 接頭辞子を引くキーの取り違え（原典 `WildcardIndex.scala` の 72 行目）。(2) 接頭辞子しか持たない根が「空」と判定されること（原典の 29〜43 行目の根の判定と 45 行目の `WildcardIndex.isEmpty`）。元帥の決定で、直すのは根の判定だけで、節一般の判定（原典の 59 行目）は直さない。どちらも直す。直した振る舞いは補足の回帰テスト（`crates/fraktor-actor/src/kernel/wildcard_index/wildcard_index_value_test.rs`。台帳の件数に含めない）で固定する。
- **照合の手順そのものは原典どおりに写す（元帥の決定）。** 照合が貪欲で後戻りしないこと（§3 の W1）と、要素が尽きたときに `"**"` の代替を捨てること（W3）は、直さずに仕様として書く。
- **接頭辞の複数一致の順序は決定的にする。** 原典の `find` は接頭辞子を `HashMap` の走査順（不定）で最初に一致したものを取る。移植は `BTreeMap` のキー順（決定的）で取る。複数の接頭辞が同時に一致する入力では結果が原典と変わりうる。原典のケースには現れない。決定的であることを優先し、違える点としてここに明記する。
- 木（`WildcardTree`）は原典どおり非公開にする。
- **`insert` の進め方（設計レビュー M3 の (b)）。** まず経路の全要素について `"**"` の接尾辞検査だけを行い（失敗なら元の木を返さず `Err`。W6 の「失敗しても木を変えない」はこの段で保たれる）、その後に木を 1 本だけ複製し、複製の上で要素をたどりながらその場で書き換える。検索は原典の `@tailrec` と同じくループで書く。したがって再帰の深さの問題は残らない。ただし共有ポインタを使わないため、複製は木の全体を深く写す。1 回の `insert` の計算量は木の全体に比例する（§7）。原典の `HashMap` は永続的で変わらない部分を共有できるので、ここは原典と違う。原典のケースは `insert` の前の木を後で使わない（`emptyIndex` は毎回新しく作る）ので、ケースの意味は変わらない。

#### 原典との意図した差（WildcardIndex、元帥の決定）

前例は [PR #153](https://github.com/j5ik2o/fraktor-ng-rs/pull/153) の「意図した差」の記録の形である。原典の振る舞いは、設計レビューの担当が原典の `WildcardIndex.scala` を写して sbt（Scala 3.3.4）で実際に動かして確かめた（PR #229 の設計レビュー、H2 の表）。この節の内容は、第 2 段で作るゴール文書 `docs/goals/issue-187-util-specs.md` にも同じく書く。

- **差 1：接頭辞子を引くキー。** 原典の何行目か：`actor/src/main/scala/org/apache/pekko/util/WildcardIndex.scala` の 71〜72 行目。`wildcardSuffixChildren.updated(e.stripSuffix("*"), wildcardSuffixChildren.getOrElse(e, ...))` とあり、登録は除いた後のキー（例：`"a"`）で行うのに、既存の部分木を引くときは要素そのもの（例：`"a*"`）で引く。どう違うか：同じ接頭辞パターンを別の続きで再挿入すると、既存の部分木が引けず、前の続きが落ちる。sbt で動かした結果：レビューの Q2 で、`None` になったのは先に入れた側の経路（`["a*","b"]`）であり、再挿入した側（`["a*","c"]`）は `Some(2)` だった。移植は、引くときも除いた後のキーで引き、前の続きを保つ。原典の不具合として扱う。
- **差 2：接頭辞子しか持たない根の `isEmpty`。** 原典の何行目か：同じファイルの 59 行目（`WildcardTree.isEmpty` は `data.isEmpty && children.isEmpty` で接頭辞子を数えない）が、根の判定（29〜43 行目の `find` の冒頭と、45 行目の `WildcardIndex.isEmpty`）に効いている。どう違うか：接頭辞子しか持たない根は「空」になり、`find` は単一ワイルドカードの木にも二重ワイルドカードの木にも入らず、常に `None` を返す。sbt で動かした結果：レビューの Q1 で、`insert(["a*"], 1)` の後の `find(["a"])` と `find(["ab"])` がどちらも `None`、`isEmpty` が `true` になった。移植で直す範囲（元帥の決定、再レビューの対案）：根の判定（原典の 30 行目に当たる `find` の冒頭の木の選択と、45 行目に当たる `WildcardIndex::is_empty`）だけで接頭辞子を数える。節一般の判定（59 行目）と、二重の木へ移る条件（38 行目）は原典のままにする。理由は、原典と違える範囲は狭いほどよいことと、59 行目で直すと副作用が出ることである。副作用の例（再レビューの S1）：`["x","a*"]` に 1、`["**"]` に 2 を入れた木で `find(["x"])` が、59 行目で直すと `Some(2)` から `None` に変わる（着いた節 `x` が接頭辞子しか持たないため）。根だけで直す版では原典のまま `Some(2)` であり、原典の 8 件はどちらの版でも通ることを再レビューが sbt で確かめた。原典の不具合として扱う。
- **写すもの（差ではない）：** 照合が貪欲で後戻りしないこと（原典の 77〜96 行目 `findWithSingleWildcard`）と、要素が尽きたときに `"**"` の代替を捨てること（原典の 98〜113 行目 `findWithTerminalDoubleWildcard` が、要素が尽きると着いた節そのものを返す）は、照合の仕様として §3 の W1 と W3 に書く。sbt で動かした結果はレビューの Q3〜Q6 のとおりである。

**条件の確認：直した振る舞いで原典のケースが全部通ること。** 差 1 と差 2 が効くのは、接頭辞パターンを持つ木だけである。WildcardIndexSpec の 8 件のうち接頭辞パターンを使うのはケース 4 だけで、その木は根に通常の子 `"b"` を持ち、接頭辞子しか持たない節は照合の結果節にならないため、差 2 はケース 4 の結果を変えない。差 1 は同じ接頭辞パターンの再挿入でのみ効き、ケース 4 は各パターンを 1 回だけ入れるので効かない。したがって 8 件とも直した振る舞いで通る見込みであり、除外になるケースは無い。第 2 段では、実装後に 8 件が実際に通ることを実行で確かめる。万一、不具合の振る舞いそのものを期待して通らないケースが出たら、そのケースは除外として設計に書いたうえで、こちらへ知らせる（上司へ上げる）。

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

3 つとも `core` と `alloc` だけで書ける。`Index` は `alloc::{collections::BTreeMap, vec::Vec, string::String}` と既存依存の `spin::Mutex`（`default-features = false` で no_std 対応）を使う。`WildcardIndex` は `alloc::{collections::BTreeMap, string::String, vec::Vec}` だけである。`MessageBuffer` は `alloc::collections::{VecDeque, BTreeMap}` だけである。std を必要とするのは並行ケースのテスト（`std::thread`）だけであり、テスト crate は std でよい。前例どおり、クレート全体は `#![no_std]` ではないが、新しいモジュールは `std` を import しない。

## 3. 守る性質（不変条件）

### Index

| 番号 | 不変条件 |
|:---|:---|
| I1 | キーごとに、値は比較器でソートされ、`compare == 0` の重複は無い |
| I2 | キーが表に現れるのは、その値集合が空でないときだけである（最後の値を消す操作はキーを消す） |
| I3 | `put` の戻り値は、その（キー、値）が操作の前に無かったときに限って `true`。`remove(key, value)` と `remove_key` の戻り値も同様に、実際に消えたときに限って成功を返す |
| I4 | `remove_value(v)` の後は、どのキーも `v`（と比較器で等しい値）を含まない。空になったキーは消える |
| I5 | `clear()` の後は `is_empty()` が `true` で、すべてのキーの値集合が空である |
| I6 | 反復系（`value_iterator`、`for_each`、`values`、`keys`）と `find_value` は、呼び出し時点のスナップショットを見る。反復の途中でほかのスレッドが書き換えても内容は変わらず、ロックも保持し続けない。利用者のクロージャはロックを外してから呼ぶので、クロージャの中からこの `Index` のメソッドを呼んでも止まらない（比較器だけはロックの中で呼ぶ。§2.1） |
| I7 | 並行性：すべての操作はロックの下で原子的であり、操作の列は逐次実行のどれかと等価である（線形化可能性）。原典のキーごとの直列化より強い。各公開メソッドはロックを 1 回だけ取り、ロックを持ったまま利用者のコードを呼ばない（比較器を除く） |

### WildcardIndex

| 番号 | 不変条件 |
|:---|:---|
| W1 | 照合は貪欲で、後戻りしない（仕様として写す）。単一ワイルドカードの木では、各段で完全一致の子、`"*"` の子、接頭辞子（キーの順で最初に一致するもの）の順に試し、最初に見つかった枝だけを進む。その先で一致しなくても、ほかの枝は試さない。例：`["a","b","c"]` に 1、`["a","*","d"]` に 2 を入れた木で `find(["a","b","d"])` は `None` である（`"b"` の枝を進むため。レビューの Q4）。`"*"` の枝を選んだ段では接頭辞子へは戻らない（レビューの Q6） |
| W2 | 二重ワイルドカードの木へ移るのは、単一の木の照合で着いた節が構造として空のときだけである。節が構造として空とは、値が無く通常の子が無いことである（接頭辞子は数えない。原典の 59 行目のまま。意図した差 2 で直すのは根の判定だけである。§2.2）。例：`["a","b","c"]` に 1、`["a","**"]` に 2 を入れた木で `find(["a","b"])` は `None` である。着いた節には子 `"c"` があるので空ではなく、二重の木へ移らない（レビューの Q3） |
| W3 | 末尾の `"**"` の照合では、進む途中で見た `"**"` の子を代替として持ち、要素がどの子にも一致しない段で代替の節を返す。ただし要素が尽きたときは、代替を捨てて、着いた節そのものを返す（仕様として写す）。例：`["a","**"]` に 1、`["a","b","c","**"]` に 2 を入れた木で `find(["a","b"])` は `None`（着いた節に値が無い）であり、`find(["a","b","x"])` は `Some(1)` である（レビューの Q5）。二重ワイルドカードの木の照合は接頭辞子を見ない（原典の振る舞いとして写す。元帥の決定）。そのため、接頭辞パターンを含み `"**"` で終わる経路は `find` で一致しない。例：`["a*","**"]` に 1 を入れた木で `find(["ab","x"])` と `find(["a","x"])` はどちらも `None` であり、`["x","a*","**"]` に 1 を入れた木で `find(["x","ab","y"])` も `None` である（再レビューの D1 と D2。補足の回帰テストにも入れる） |
| W4 | 根の判定だけで、接頭辞子を数える（意図した差 2）。接頭辞子しか持たない木は、根の判定では空ではなく、`find` は照合を行い、`is_empty()` は `false` である。`insert(["a*"], 1)` の後の `find(["a"])` と `find(["ab"])` はどちらも `Some(1)` である（原典はどちらも `None` で `is_empty` は `true`。レビューの Q1）。根以外の節の空の判定と、二重の木へ移る条件は原典のままである（W2） |
| W5 | 同じ経路への再挿入は値を置き換える。接頭辞パターンの再挿入でも、前の続きは保たれる（意図した差 1）。ただし違う経路の追加が、ほかの経路の `find` の結果を変えることがある（W1 の貪欲さと W2 の空の条件による。例は W2 のとおり） |
| W6 | 空の経路への `insert` は木を変えない。何も入れていない木の `find` は、空の経路を含め、常に `None` である。`"**"` で終わるが `"**"` でない要素を含む経路の `insert` は `Err` を返し、木を変えない（全要素の検査を書き換えの前に行う。§2.2）。末尾でない `"**"` は通常の要素としてだけ扱われ、任意の要素には一致しない |

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
| Index（並行・決定的） | (a) N 本のスレッドが同じキーへ重なりのある値を `put` する。`true` の回数の合計が異なる値の数に等しく、最後の集合が和集合に等しい。(b) `for_each` と `find_value` のクロージャの中から `put` と `remove` を呼んでも止まらず、スナップショットは変わらない。再入が壊れるとテストが失敗せずに止まったままになるので、このテストは別のスレッドで走らせ、`recv_timeout` で待つ番人を付けて、時間切れを失敗として返す形にする | I6、I7 |
| WildcardIndex | ランダムな経路の挿入列と検索列を、（経路、値）の列に対する宣言的な照合モデルと比べる。モデルは、候補の経路の集合を段ごとに絞る形で W1〜W3 の手順を書く（貪欲な枝の選び方は、ほかの経路にどの枝があるかで決まるので、経路を 1 本ずつ別々に照合する形では書けない）。枝の名前は、通常の子と接頭辞子を区別して持つ（同じ文字列でも別の表に入る）。空の判定は W2 と W4、代替の扱いと二重の木が接頭辞子を見ないことは W3 に従う。接頭辞子の引き方と根の空の判定は、意図した差 1 と差 2 で直した形にする。要素の部品は、通常の文字列、`"*"`、`"**"`、接頭辞パターン、空文字、二重ワイルドカード接尾辞（`Err` の確認）から選ぶ。意図した差 1 と差 2 の振る舞いそのものは、補足の回帰テスト（§2.2）で固定する | W1〜W6 |
| MessageBuffer | ランダムな `append`／`drop_head`／`filter_not` の列を `VecDeque` のモデルと毎手比べる。`size`、`head`、`for_each` の列が一致する | M1〜M4 |
| MessageBufferMap | ランダムな `add`／`append`／`remove` の列をモデルと比べる。`total_size`、`contains`、各 id の内容が一致する | M5 |

## 5. Quint で書くかどうか

**3 つとも Quint は書かない。** 判断と理由は対象ごとに次のとおりである。

- **Index（書かない）。** 逐次意味論の不変条件 I1〜I5 は、§4 のモデル PBT が操作列の全体で毎手確かめるので、Quint の跡の検査と重なる。原典の特徴である並行性については、Quint はスレッドとロックをモデルにしないため、確かめる対象にならない。移植の並行性は「1 本の `spin::Mutex` の下で全操作が原子的」という実装の形そのもので保証し、実行では並行ケース（ケース 7）と、PBT のモデルと並行に流す追加テストではなくケース 7 の決定的な乱数化で確かめる。壊れ方として残るのはロックの取り忘れであり、これは Quint ではなく型（`&self` のメソッドがすべて同じロックを通ることのコードレビュー）と実行テストの担当である。
- **WildcardIndex（書かない）。** 状態遷移を持つ可変の構造ではなく、`insert` が新しい値を返す純粋な関数である。不変条件は「挿入列の後の `find` の結果」であり、§4 のモデル PBT が優先順位を含む照合規則そのものをモデルとして確かめる。遷移の列でしか出ない壊れ方（リンクの繋ぎ間違い等）は、この構造には無い。
- **MessageBuffer（書かない）。** 原典の壊れ方は `filterNot` の手書きの繋ぎ直しに集中するが、移植は `VecDeque` を使うため、その壊れ方自体が存在しない（§2.3）。残る不変条件 M1〜M4 は FIFO の順序と件数だけであり、§4 のモデル PBT が毎手確かめる。`DoubleLinkedList`（#176）で Quint が要った理由は、自前で前後のリンクを書き換える遷移が複数あったことであり、その前提がここには無い。

並行ケース（Index のケース 7）の順序の扱い: 原典はキー 5〜9 の値集合が削除対象でないことだけに依存し、最終状態を検査しない。移植も同じ前提だけを使い、スレッドごとの決定的な乱数列（スレッド番号を seed に混ぜる）で再現可能にする。操作の競合で結果が変わるのは戻り値と中間の内容だけであり、ケースの合否はそのどちらにも依存しない。I6 と I7 はケース 7 だけに任せず、§4 の決定的な並行テスト (a)(b) で確かめる（設計レビュー M4）。

なお、移植の規約は状態遷移とロックに Quint を必須としている。この「書かない」判断は、元帥の判断（2026-11-26）である。状態遷移の無い純粋なデータ構造なので Quint の対象外であり、共通の方針の Quint の節は、状態遷移に関わる変更が対象である。将来、Index をキーごとのロックや原典の spin-put の再試行に寄せる変更が入ったら、その時点で Quint を書く。

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
| WildcardIndex | 同じ接頭辞パターンの別の続きへの再挿入 | 意図した差 1 で直し、前の続きは保たれる（原典では落ちる） | 補足の回帰テストで固定 |
| WildcardIndex | 複数の接頭辞が同時に一致する検索 | 原典は `HashMap` の走査順で不定、移植はキー順で決定的（§2.2） | PBT のモデルはキー順で書く |
| WildcardIndex | 接頭辞子しか持たない木 | 意図した差 2 で根の判定だけを直し、照合が働く（原典では `find` が常に `None`）。二重の木への切り替えの条件は原典のままなので、`["x","a*"]` と `["**"]` を入れた木の `find(["x"])` は原典どおり `Some(2)` のままである | 補足の回帰テストで固定 |
| WildcardIndex | 極端に長い経路（数千要素） | `insert` と検索はループで書くので再帰の深さの問題は無い。`insert` の複製は木全体に比例する（§7） | PBT の部品に長い経路を含める |
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
- **計算量**: `Index` の値の挿入と削除はキーごとの値の数に比例する（ソート済み `Vec`）。`WildcardIndex` の `insert` は木の全体に比例する（共有ポインタを使わない深い複製のため。§2.2）。検索は経路長に比例する。`MessageBuffer` は原典と同じく追加と先頭の削除が償却定数、`filter_not` は件数に比例する。
- **原典が内部 API であること（設計レビュー L3）**: 原典の `WildcardIndex` は `private[pekko]`、`MessageBuffer.filterNot` は `@InternalApi private[pekko]` である。どちらも kernel の公開型にする判断は、#177 の `FrequencySketch` の前例と同じである。

## 8. レビューで見てほしい点

設計レビュー（PR #229）で、次のとおり決まった。回答の詳細は §9 にある。

1. **Index の比較器の形**: 比較器を保ち、`Box<dyn Fn>` で持つ（レビューの推奨どおり）。値集合はソート済み `Vec` のままである。
2. **Index の並行性**: 全体 1 本のロックでよい（レビューの推奨どおり）。条件は、各公開メソッドがロックを 1 回だけ取ることと、ロックを持ったまま利用者のコードを呼ばないことであり、I7 に入れた。
3. **WildcardIndex の接頭辞子まわり**: 元帥が決めた。キーの取り違えと、接頭辞子しか持たない節の `isEmpty` は原典の不具合として直し、意図した差として §2.2 に記録する。貪欲で後戻りしない照合と、要素が尽きたときに `"**"` の代替を捨てることは、仕様として W1 と W3 に書く。
4. **WildcardIndex の `insert` の `Err`**: `Err` でよい（レビューの推奨どおり）。全要素の検査を先に行う形（M3 の (b)）と組み合わせる。
5. **MessageBuffer のジェネリクス化と `head`／`get` の `Option` 化**: 妥当（レビューの確認どおり）。
6. **Quint を 3 つとも書かない判断**: 元帥の判断（2026-11-26）として §5 に記録した。状態遷移の無い純粋なデータ構造なので Quint の対象外であり、共通の方針の Quint の節は、状態遷移に関わる変更が対象である。Index の形が変わったら書き直す条件も §5 に入れた。
7. **作らないもの**: 妥当（レビューの確認どおり）。

## 9. 設計レビュー（PR #229）への回答

対象は HEAD `a8d31d5` への独立レビュー（高 2 件、中 4 件、低 5 件）である。元帥の決定（接頭辞子まわりの 2 件を直す）を含む。

| 指摘 | 対応 |
|:---|:---|
| H1（ファイルが途中で切れ、切り詰めを示す印の文字列が入っている） | 直した。印の文字列そのものを取り除き、前後の文がつながることを確かめた。抜けた中身は無く、§2.1 の比較器の段落が印を挟んで続いていることが全文である。あわせて §5 の壊れた文（意味の通らない並行ケースの文）を書き直した。再発の防止として、push の後に GitHub の raw のファイルを取り、手元の全文と diff で一致することと、切り詰めを示す印の文字列が無いことを確かめてから知らせる手順にした |
| H2（W1〜W6 が原典と食い違う。Q1、Q3〜Q6） | 直した。W1〜W6 を原典の照合の手順で書き直した。貪欲で後戻りしない照合（Q4、Q6）と、二重の木へ移る条件が「着いた節が構造として空」であること（Q3）と、要素が尽きると `"**"` の代替を捨てること（Q5）は、仕様として W1〜W3 に書いた。Q1 は元帥の決定で意図した差 2 として直す側に倒し、W4 に直した振る舞いを書いた。§4 の PBT のモデルも、宣言的な照合の形に書き直した |
| M1（`find_value` がロックを持ったままクロージャを呼ぶと止まる） | 直した。`find_value` も値を複製してロックを外してからクロージャを呼ぶ形にし、I6 に入れた。比較器だけはロックの中で呼ぶので、「比較器の中からこの `Index` を呼ばないこと」を API 文書の前提として §2.1 に書いた |
| M2（内側の `Arc` が二重で、所有権の規約に触れる） | 直した。内側の `Arc` をやめ、`spin::Mutex` を直に持つ形にした。`Clone` は付けず、共有は呼ぶ側の仕事とした。比較器は `Box<dyn Fn>` にした |
| M3（`insert` の計算量の説明が違う。検索は再帰ではない） | 直した。(b) を選び、全要素の検査を先に行ってから、複製した木をその場で書き換える形にした。計算量は §7 で「木の全体に比例する」に直し、検索は原典の `@tailrec` と同じくループで書くこと、再帰の深さの問題は残らないことを §2.2 と §6 に書いた |
| M4（I6 と I7 を確かめる手段が無い） | 直した。§4 に決定的な並行テスト (a)（同じキーへの重なりのある `put` で、`true` の合計と最後の和集合を見る）と (b)（`for_each` と `find_value` のクロージャからの再入）を足した。ケース 7 はスレッドの本数を固定し、`Barrier` で一斉に始める形を §1 の対応表に書いた |
| L1（`for_each` に `K: Clone, V: Clone` が無い） | 直した。API の形に境界を入れた |
| L2（`values()` の重複の決め方が書かれていない） | 直した。比較器で同一視することを §2.1 に書き、原典の `equals` と結果が変わる例（絶対値の比較器で 1 と -1）を違える点として明記した |
| L3（原典が内部 API であることを書いていない） | 直した。§7 に、`WildcardIndex` が `private[pekko]`、`MessageBuffer.filterNot` が `@InternalApi private[pekko]` であり、公開型にする判断は #177 の前例と同じであることを書いた |
| L4（既存の LookupEventBus との関係が書かれていない） | 直した。§2.0 に、fraktor の `LookupEventBus` はこの `Index` を使っておらず重なりは無いこと、この `Index` を使わせるかは別件とすることを書いた |
| L5（雛形の点の確認。変更なし） | 確認した。3 つの Spec に `cases.foreach` の展開は無く、`crates/` に glob の照合や多値の表の既存実装は無い。設計の変更は無い |
| 論点への推奨（比較器は `Box<dyn Fn>`、ロックは 1 本、`Err` と `Option` は妥当、Quint は条件つきで妥当、作らないものは妥当） | すべて受け入れた。§8 に決定として記録した |
| 接頭辞の癖を写すか直すか（上司へ確認中だった件） | 元帥が決めた。キーの取り違え（Q2）と、接頭辞子しか持たない節の `isEmpty`（Q1）を原典の不具合として直し、意図した差として §2.2 に、原典の行番号と sbt で動かした結果を添えて記録した。貪欲な照合（Q3〜Q6）は写して W に書いた。直した振る舞いでも原典のケースが全部通ることの確認を §2.2 に書き、通らないケースが出た場合の扱い（除外として設計に書いて知らせる）も書いた |

## 10. 設計の再レビュー（PR #229）への回答

対象は HEAD `c473a9b` への再レビュー（判定は要修正（軽微）。中 3 件、低 8 件）である。前回の高 2 件、中 4 件、低 5 件はすべて直っていることが確かめられ、WildcardIndexSpec の 8 件は直した版の原典でも sbt で 8 件とも通った。

| 指摘 | 対応 |
|:---|:---|
| 中 1（差 2 を 59 行目で直すと、`**` への切り替えも狭まる。S1 と S5） | 元帥が決めた。対案のとおり、直すのは根の判定（原典の 30 行目と 45 行目に当たる場所）だけにし、59 行目は直さない。意図した差 2 の記述と W2、W4、§6 の行を、この範囲に直した。副作用の例（S1）は §2.2 に理由とともに書いた |
| 中 2（二重ワイルドカードの木が接頭辞子を見ない。D1 と D2） | 元帥が決めた。直さずに写し、W3 に原典の振る舞いとして例（D1 と D2）とともに書いた。補足の回帰テストにも入れる |
| 中 3（Quint を書かない例外に、オーナーが確認した記録が無い） | 直した。「オーナーの確認を経た」という書き方は誤りであり、オーナーは確認していない。§5 と §8 を「元帥の判断（2026-11-26）。状態遷移の無い純粋なデータ構造なので Quint の対象外。共通の方針の Quint の節は、状態遷移に関わる変更が対象」に書き直した。今後、オーナーや上司が実際に言っていない判断を、その人の名前で書かない |
| L1（§2.2 に古い一文が残っている） | 直した。「書き換えるのは経路上の節だけ……経路の長さがそのまま複製の量になる」の括弧を消し、M3 の項への参照にした |
| L2（335 行目に U+FFFD が 2 文字） | 直した。「を足した」に戻した。push 後の確認の手順に、置換文字（U+FFFD）の検査も足す |
| L3（Q2 の記述が逆） | 直した。`None` になったのは先に入れた側（`["a*","b"]`）であり、再挿入した側は `Some(2)` であったと書き直した |
| L4（W の参照番号が古い） | 直した。「W5 はこの段で保たれる」を「W6 の『失敗しても木を変えない』はこの段で保たれる」に直した |
| L5（原典の行番号がずれている） | 直した。`findWithSingleWildcard` は 77〜96 行目、`findWithTerminalDoubleWildcard` は 98〜113 行目、`find` は 29〜43 行目に直した |
| L6（102〜103 行目の文の途中の改行） | 直した。改行をつないだ |
| L7（PBT のモデルの書き方） | 直した。「候補の経路の集合を段ごとに絞る」と書き、枝の名前は通常の子と接頭辞子を区別して持つことを §4 に書いた |
| L8（並行テスト (b) が止まったままになる） | 直した。別のスレッドで走らせ、`recv_timeout` で待つ番人を付けて、時間切れを失敗として返す形を §4 に書いた |
