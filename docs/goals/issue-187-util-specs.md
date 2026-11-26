# ゴール G0issue187：util の IndexSpec、WildcardIndexSpec、MessageBufferSpec

[Issue #187](https://github.com/j5ik2o/fraktor-ng-rs/issues/187) の対象は、Pekko v2.0.0 の actor-tests にある util の 3 つの Spec、IndexSpec（7 件）、WildcardIndexSpec（8 件）、MessageBufferSpec（9 件、うち MessageBufferMap が 2 件）で、計 24 件である。件数は原典の `" in {"` の数で数え直し、issue の目安と一致した。`ignore` と `pending` はない。設計は [../design/issue-187-util-specs.md](../design/issue-187-util-specs.md) にある。

- [原典 IndexSpec](https://github.com/apache/pekko/blob/9fbd2283b1cf701c90a1223bf621942c191db61f/actor-tests/src/test/scala/org/apache/pekko/util/IndexSpec.scala)、[原典 Index](https://github.com/apache/pekko/blob/9fbd2283b1cf701c90a1223bf621942c191db61f/actor/src/main/scala/org/apache/pekko/util/Index.scala)
- [原典 WildcardIndexSpec](https://github.com/apache/pekko/blob/9fbd2283b1cf701c90a1223bf621942c191db61f/actor-tests/src/test/scala/org/apache/pekko/util/WildcardIndexSpec.scala)、[原典 WildcardIndex](https://github.com/apache/pekko/blob/9fbd2283b1cf701c90a1223bf621942c191db61f/actor/src/main/scala/org/apache/pekko/util/WildcardIndex.scala)
- [原典 MessageBufferSpec](https://github.com/apache/pekko/blob/9fbd2283b1cf701c90a1223bf621942c191db61f/actor-tests/src/test/scala/org/apache/pekko/util/MessageBufferSpec.scala)、[原典 MessageBuffer](https://github.com/apache/pekko/blob/9fbd2283b1cf701c90a1223bf621942c191db61f/actor/src/main/scala/org/apache/pekko/util/MessageBuffer.scala)

## 対象ノード

| id | Rust の統合テスト | 原典のケース数 |
|:---|:---|---:|
| `actor-tests/src/test/scala/org/apache/pekko/util/IndexSpec.scala` | `crates/fraktor-actor/tests/index_spec.rs` | 7 |
| `actor-tests/src/test/scala/org/apache/pekko/util/WildcardIndexSpec.scala` | `crates/fraktor-actor/tests/wildcard_index_spec.rs` | 8 |
| `actor-tests/src/test/scala/org/apache/pekko/util/MessageBufferSpec.scala` | `crates/fraktor-actor/tests/message_buffer_spec.rs` | 9 |

台帳は 3 ノードとも原典と同数（7、8、9）で、移植と成功も同数、除外 0 件、作らない 0 件である。ケースは全件、統合テストに 1 ケース 1 関数で置いた。補足の回帰テストと PBT は原典外で、台帳の件数には含めない。Index の `needs`（TestKit と乱数）は、移植が ActorSystem も TestKit も使わず、乱数は固定 seed の SplitMix64 に置き換えたため、そのまま残してある（前提の置き換えは設計 §1 と §3 に書いた）。

## 実行経路

TAKT は使わず、担当が直接進めた。テストを先に書き、コンパイルエラー（E0432）で red を確認した（`88f3ada`）。実装を足して green にした。設計は独立レビューと再レビューを受け、WildcardIndex の扱いは元帥の決定に従った（設計 §2.2、§9、§10）。

## 公開面

3 つとも `fraktor_actor::kernel` の新しいモジュールに置いた。原典はどれも `private[pekko]` か `@InternalApi` の内部 API で、移植では #177 の前例どおり公開した。

| 原典 | 移植 |
|:---|:---|
| `Index[K, V](mapSize)(comparator)`、`ConcurrentMultiMap` の別名 | `kernel::index::Index<K, V>::new(map_size, comparator)`、`ConcurrentMultiMap` の型別名 |
| `put` / `remove(key, value)` / `remove(key)` / `removeValue` / `clear` / `isEmpty` | `put` / `remove` / `remove_key` / `remove_value` / `clear` / `is_empty` |
| `valueIterator` / `values` / `keys` / `foreach` / `findValue` | `value_iterator`（スナップショットの `Vec`）/ `values` / `keys` / `for_each` / `find_value` |
| `WildcardIndex[T]()`、`insert(elems, value)`、`find(elems)`、`isEmpty` | `kernel::wildcard_index::WildcardIndex<T>::new()`、`insert(&[&str], T) -> Result<Self, WildcardIndexError>`、`find(&[&str]) -> Option<&T>`、`is_empty` |
| `IllegalArgumentException`（`"**"` を接尾辞に使う） | `Err(WildcardIndexError::DoubleWildcardAsSuffix)` |
| `MessageBuffer`、`MessageBuffer.empty` | `kernel::message_buffer::MessageBuffer<M, S>::empty()`（原典は `(Any, ActorRef)`、移植は総称） |
| `append` / `dropHead` / `head` / `foreach` / `filterNot` / `size` / `isEmpty` | `append` / `drop_head` / `head -> Option<(&M, &S)>` / `for_each` / `filter_not` / `size` / `is_empty` |
| `MessageBufferMap[I]` の `add` / `append` / `remove` / `contains` / `getOrEmpty` / `size` / `totalSize` / `foreach` | `kernel::message_buffer::MessageBufferMap<I, M, S>` の `add` / `append` / `remove` / `contains` / `get -> Option<&MessageBuffer>` / `size` / `total_size` / `for_each` |

### 原典と違えたところ

- Index は全体を 1 本の `spin::Mutex` で守り、公開メソッドはロックを最大 1 回だけ取り、利用者コード（反復と探索のクロージャ）はスナップショットに対してロックの外で呼ぶ。比較器だけはロックの中で呼ぶので、比較器から同じ Index を呼んではならないことを API 文書に書いた。原典の spin-put の再試行とキーごとのロックには寄せていない。
- Index の `values` は、原典が `equals` の集合で重複を除くところを、比較器の同値で除く（`Eq` の境界を付けないため）。比較器と `Eq` が一致する使い方では結果は同じである。
- WildcardIndex には、元帥が決めた意図した差が 2 つある。(1) 接頭辞子の探索キーの取り違え（原典 71〜72 行目）は直した。(2) `isEmpty`（原典 59 行目）は、直す範囲を根の判定（原典 30 行目と 45 行目に当たる場所）だけにし、節一般の判定と二重の木への切り替えの条件は原典のままにした。59 行目で直すと副作用が出る（`["x","a*"]` に 1、`["**"]` に 2 を入れた木で `find(["x"])` が `Some(2)` から `None` に変わる）ためである。どちらも補足の回帰テストで固定した。照合が貪欲で後戻りしないこと、要素が尽きると `"**"` の代替を捨てること、二重ワイルドカードの木が接頭辞子を見ないことは、原典の振る舞いのまま写した。
- WildcardIndex の `insert` は、木全体を複製して複製側をその場で書き換える。原典は経路だけを複製するので、計算量は原典より大きい（木の全体に比例する）。共有ポインタで経路だけを複製する形は採らなかった。
- MessageBuffer の `head` は、原典の `(null, null)` の代わりに `Option` を返す。MessageBufferMap の `getOrEmpty` は、共有の空バッファの代わりに `Option` を返す。

### 作らないもの

- MessageBufferMap の `drop`（バッファの中身を dead letters へ送る）。カーネルの型が ActorSystem の dead letters を持たないためである。原典のケースは使っていない。
- Index と MessageBuffer の Java API（`java.util.Comparator` を受ける生成と `MessageBuffers`）。原典のケースは使っていない。

## 原典との照合

移植テストは原典のケースごとに 1 関数で、原典の名前をコメントに残した。WildcardIndexSpec の 8 件は、意図した差 2 つを入れた原典でも全部通ることを、設計の再レビューが sbt で確かめた。差 1 と差 2 が効く入力は原典のケースにはなく、補足の回帰テスト（`crates/fraktor-actor/src/kernel/wildcard_index/wildcard_index_value_test.rs`）が、レビューが sbt で得た原典の結果（Q1、Q2、S1、D1、D2）と移植の結果を並べて固定する。

PBT は 3 つとも、依存を足さないモデル比較で書いた（固定 seed の SplitMix64、400 系列）。Index は `BTreeMap<i32, BTreeSet<i32>>`、WildcardIndex は候補の経路の集合を段ごとに絞る宣言的なモデル、MessageBuffer と MessageBufferMap は `VecDeque` とその `BTreeMap` がモデルである。Index には決定的な並行テスト 2 件（重なる put の総数、スナップショットのクロージャからの再入を番人付きで確かめる）を足した。Quint は書かない。元帥の判断（2026-11-26）で、状態遷移の無い純粋なデータ構造なので Quint の対象外であり、共通の方針の Quint の節は状態遷移に関わる変更が対象である（設計 §5）。

## 検証

| コマンド | 終了値 | 結果 |
|:---|---:|:---|
| `cargo test -p fraktor-actor --test index_spec --test wildcard_index_spec --test message_buffer_spec` | 0 | 7 件、8 件、9 件すべて成功 |
| `cargo test -p fraktor-actor --test index_model_test --test wildcard_index_model_test --test message_buffer_model_test` | 0 | 3 件、1 件、2 件すべて成功（PBT と決定的な並行テスト。原典外） |
| `cargo test -p fraktor-actor --lib` | 0 | 530 件成功（補足の回帰テスト 11 件を含む） |
| `cargo test --workspace` | 0 | 1069 件成功、失敗 0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | 0 | 警告 0 |
| `cargo fmt --check` | 0 | 差分なし |
| syn-check（`crates/` の 2 クレート） | 0 | 違反 0、抑制 0 |
| `npm run check:aislop` | 0 | 受け入れ済みの一覧と一致 |
| `verify_green.py --crate fraktor-actor` | 0 | `all_pass: true` |
| CI の python テスト（`goal_complete_gate_test.py` ほか 10 本） | 0 | すべて成功。`verify_green_test_aislop.py` だけは手元の Mac では stub の環境に cargo と node が無く失敗する（台帳の道具側の単体テストで、本変更とは無関係。CI で確認する） |
| `pekko_baseline.py check-ledger`、`check-refs`、`relocate_nodes.py check` | 0 | OK |
| `report_ledger_1to1.py check` | 0 | OK |
| `rollup.py --goal G0issue187` | 0 | `{'green': 3}`、「完了: 全ノードが green か do_not_port か not_built」 |
| `check_needs.py --goal G0issue187` | 0 | needs 閉性 OK |

`verified_against: v2.0.0` を付けたので、`report_ledger_1to1.py list-undocumented` で `docs/baseline/v2.0.0-undocumented-nodes.tsv` を作り直した（3 行増えて 56 件、`unclassified` 3 件）。
