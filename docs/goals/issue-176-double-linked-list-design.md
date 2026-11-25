# 設計：#176 DoubleLinkedList（PR #178、HEAD `6c267a1` の時点）

[Issue #176](https://github.com/j5ik2o/fraktor-ng-rs/issues/176) と [PR #178](https://github.com/j5ik2o/fraktor-ng-rs/pull/178) の設計の記録である。2026-11-25 のオーナーの決定で、実装の独立レビューを廃止し、設計の独立レビューに置き換えた。この PR は実装が先に済んでいたので、出来上がった時点（HEAD `6c267a185e371e5904cac48765d97d7eb311adc3`）の設計をあとから書いた。実装に関わっていない担当が、この文書を読んで設計をレビューする。経緯と検証の結果は [ゴール文書](issue-176-double-linked-list.md) にある。

## 1. 何をどう変えたか

kernel に、id で引く双方向連結リスト `kernel::double_linked_list::{DoubleLinkedList, NodeId}` を足した。Pekko v2.0.0 の `org.apache.pekko.util.DoubleLinkedList` の移植である。

| ファイル | 中身 |
|:---|:---|
| `crates/fraktor-actor/src/kernel.rs` | `pub mod double_linked_list;` の 1 行 |
| `crates/fraktor-actor/src/kernel/double_linked_list.rs` | モジュールの宣言と再エクスポート |
| `crates/fraktor-actor/src/kernel/double_linked_list/double_linked_list_value.rs` | `DoubleLinkedList<A>` |
| `crates/fraktor-actor/src/kernel/double_linked_list/node_id.rs` | `NodeId` |
| `crates/fraktor-actor/tests/double_linked_list_spec.rs` | 原典 12 ケース |
| `crates/fraktor-actor/src/kernel/double_linked_list/double_linked_list_value_test.rs` | 原典外の回帰 9 件 |

### 形

原典のリストは侵入型で、利用者のノードが前後のリンクを持ち、リストはそのリンクを読み書きする 4 つの関数で作る。移植では、リストが値とリンクを持つ（2026-11-25 承認の案 A）。

- 中身は `nodes: BTreeMap<NodeId, Node<A>>`、`first`、`last`、`list`（リストの番号）、`next_id`（次のノードの番号）。`Node<A>` は `value`、`previous`、`next`。
- `NodeId` は `{ list: u64, node: u64 }`。`list` はリストを作るたびに `static NEXT_LIST: AtomicU64` から 1 つ取る番号で、`node` はそのリストの中で 0 から順に振る番号。等価は 2 つの組で決まる。外から作る手段は無い（`new` は `pub(crate)`）。
- 挿入の操作は、値のノードの `NodeId` を返す。原典でノードを受け取る操作は `NodeId` を受け取る。
- 新しいノードの値は `FnOnce() -> A` で受け取り、挿入するときだけ呼ぶ（原典の名前渡し `newNode: => Node`）。
- 判定の `check`、`isBefore`、`isAfter` は `&A` を受け取るクロージャ。

操作は原典の内部の手順（`insertBefore`、`insertAfter`、`link`、`unlink`、`shiftWhile`）をそのまま写した。対応の表はゴール文書の「原典との対応」にある。

### 変えないこと

- 既存の公開 API と型には触れていない。追加は `kernel::double_linked_list` だけ。
- `FrequencyList` の内部にある `FrequencyBuckets`（バケット id で引く連結リスト）は、この型に置き換えていない。置き換えると FrequencyList の rustdoc と計算量の記述、レビューで固めた dynamic aging の並びの検証（PR #129）に触れるため、別の変更にする。
- `RecencyList`、`SegmentedRecencyList` も変えていない。

### 範囲の外

- 原典の `DoubleLinkedList` は Pekko の内部 API（`@InternalApi private[pekko]`）である。移植は kernel の公開型にしたが、Pekko の利用者向け API に当たるものではない。
- 並行での利用。原典もこの型も「スレッド安全でない」。`DoubleLinkedList<A>` は `&mut self` で更新するので、共有して書くには外側で排他する。リストの番号を振る `AtomicU64` だけは、別々のスレッドで同時にリストを作っても一意になる。
- `#177 FrequencySketchSpec` は別の PR で扱う。

## 2. 守る不変条件

`L` を 1 つのリスト、`order(L)` を `L` の先頭から末尾までのノードの列（利用者から見た並び）とする。

| 番号 | 不変条件 |
|:---|:---|
| I1 | `first` と `last` は、どちらも空か、どちらも値を持つ。空なのはノードが 0 件のときだけ（`is_empty()` は `first.is_none()`） |
| I2 | ノード `n` の `next` が `m` なら、`m` の `previous` は `n`。その逆も成り立つ。`previous` が空なのは `first` のノードだけ、`next` が空なのは `last` のノードだけ |
| I3 | `first` から `next` をたどると、全ノードを 1 回ずつ通って終わる（輪にならない）。`last` から `previous` をたどると、その逆順になる |
| I4 | 長さ：`forward_iter` の件数は `nodes` の件数に等しい |
| I5 | `NodeId` のリストごとの一意性：あるリストが返した id は、その番号（`list`）を持つ。あるリストの `nodes` のキーは、すべてそのリストの番号を持つ。別のリストの id は、番号が同じでも、このリストのノードを指さない |
| I6 | 再利用しない：同じリストで、外した id は二度と返らない（`next_id` は単調に増え、減らない）。`u64` を使い切るまでの話で、使い切ると `next_id += 1` が debug では panic、release では 0 に戻る。現実には届かない |
| I7 | 外した id：`remove` の後、その id は `get` と `get_mut` が `None` を返し、ほかの操作は panic する |
| I8 | 意味：各操作の後の `order(L)` と戻り値は、原典 Pekko の同じ操作のそれと等しい（下の表）。I1〜I4 は、どの操作の後でも成り立つ |

I8 の中身（`g` は操作の前の並び、`h` は渡した id、`v` は新しい値で作ったノード）：

| 操作 | 後の並び | 戻り値 |
|:---|:---|:---|
| `prepend(v)` | `[v] ++ g` | `v` の id |
| `append(v)` | `g ++ [v]` | `v` の id |
| `remove(h)` | `g` から `h` を除く | `h` の値 |
| `move_to_front(h)` | `[h] ++ (g から h を除く)`。`h` が先頭なら変わらない | `h` |
| `move_to_back(h)` | `(g から h を除く) ++ [h]`。`h` が末尾なら変わらない | `h` |
| `get_first_or_else_prepend(check, new)` | 先頭が `check` を満たせば `g`、そうでなければ `[v] ++ g` | 先頭か `v` |
| `get_last_or_else_append(check, new)` | 末尾について同じ | 末尾か `v` |
| `get_next_or_else_insert(h, check, new)` | `h` の次が `check` を満たせば `g`、そうでなければ `h` の直後に `v` | 次か `v` |
| `get_previous_or_else_insert(h, check, new)` | `h` の前について同じ。`v` は `h` の直前 | 前か `v` |
| `find_next_or_else_insert(h, is_before, check, new)` | `h` から次が `is_before` を満たす間だけ進み、止まったノードで `get_next_or_else_insert` | 同じ |
| `find_previous_or_else_insert(h, is_after, check, new)` | 前の向きで同じ | 同じ |

`new` は戻り値が `v` のときだけ 1 回呼ばれ、それ以外では呼ばれない。`check`、`is_before`、`is_after` は値を読むだけで、リストを変えない（`&A` しか受け取れない）。

## 3. PBT の性質

性質は、ランダムな操作の列をリストと「並びのモデル」（`Vec<(NodeId, A)>`、I8 の表をそのまま書いたもの）の両方に流し、毎回の操作の後で比べる形で定義する。操作の列には、上の 11 の操作に加えて、`get`、`get_mut`、`get_previous`、`get_next`、`forward_iter`、`backward_iter`、添字を混ぜる。渡す id は、今ある id、外した id、別のリストの id から選ぶ。

| 番号 | 性質 | 確かめる不変条件 |
|:---|:---|:---|
| P1 | 毎回の操作の後で、`forward_iter` の値の列がモデルの並びに等しく、`backward_iter` がその逆 | I3、I4、I8 |
| P2 | 毎回の操作の後で、`is_empty`、`get_first`、`get_last` がモデルの先頭と末尾に一致し、各ノードの `get_previous`、`get_next` がモデルの隣に一致する | I1、I2 |
| P3 | 各操作の戻り値（id と値）が、モデルの戻り値に等しい。`new` を呼んだ回数が、モデルが新しいノードを作った回数に等しい | I8 |
| P4 | 返った id はすべて互いに異なる（同じリストの中でも、リストをまたいでも） | I5、I6 |
| P5 | 外した id と別のリストの id について、`get` と `get_mut` は `None`。panic する操作に渡すと panic し（`catch_unwind` で確かめる）、panic の前後でリストの並びが変わらない | I5、I7 |

除外は付けない。panic する入力（外した id、別のリストの id）も性質の対象に入れ、P5 で「panic して、並びを壊さない」ことを確かめる。

panic の前後で並びが変わらないことは、コードを読むと成り立っている。どの操作も、渡した id を最初に `node`／`node_mut`／`nodes.remove` で引き、そこで panic するので、リンクを書き換える前に止まる。`find_*` は `shift_while` で引いた後に止まった先を引くが、どれも読むだけである。P5 はこれを実行で確かめる。

**実装の状態（未実装）。** PR #178 にはまだ PBT が無い。今あるのは、原典の固定の 12 ケースと固定の回帰 9 件だけである。workspace には PBT のライブラリ（`proptest` など）が無い。設計レビューの後に、次のどちらかで足す。

- 案 1（推奨）：依存を足さない。テストの中に小さな疑似乱数（固定 seed の SplitMix64 など）を書き、操作の列を作ってモデルと比べる。seed と列の長さを固定するので、CI で再現できる。縮小（shrink）は無い。
- 案 2：dev-dependency に `proptest` を足す。縮小がある。ただ、workspace に dev-dependency の外部 crate を増やすので、判断が要る。

## 4. 状態遷移と Quint

**Quint は要る。** `DoubleLinkedList` は状態を持つデータ構造で、どの操作も複数のリンク（自分、前、次、`first`、`last`）を書き換える遷移である。不変条件 I1〜I3 は遷移の列の全体で成り立つ必要があり、固定のテストでは書き換えの順序の誤りを取りこぼしうる。実際に、PR #178 のレビューで、遷移の列（2 つのリストを作り、片方の id をもう片方に使う）でしか出ない誤り（I5 の違反）が見つかった。並行は無いので、仕様は 1 スレッドの遷移だけを扱う。

仕様は `docs/goals/specs/double_linked_list.qnt`（本体）と `docs/goals/specs/double_linked_list_test.qnt`（インスタンスとテスト）。

- 状態：2 つのリスト（`lists: int -> ListState`）。各リストは、id をキーにしたノードの map と、`first`、`last`、`nextId` を持つ。コードの `BTreeMap<NodeId, Node>` と同じ形である。
- 遷移：`prependValue`、`appendValue`、`removeNode`、`moveToFront`、`moveToBack`、`getEndOrElseInsert`（先頭と末尾の 2 つ）、`findOrElseInsert`（前後の `find_*` 2 つ。`bound = -1` で `get_next_or_else_insert` と `get_previous_or_else_insert`）。リンクの書き換えはコードの `insert_before`、`insert_after`、`link`、`unlink`、`shift_while` を 1 対 1 で写した。
- ゴースト：各遷移は、原典の意味（I8 の表）でも並びを更新する（`ghost: int -> List[Handle]`）。リンクとゴーストの判定が食い違ったら、遷移を止めずに `resultOk = false` を記録する（`findDisagrees`）。止めると誤りが隠れるため。
- 不変条件：`endsMatchEmptiness`（I1）、`linksAgree`（I2）、`walksAreTheGhostOrder`（I3、I4、I8 の並び）、`handlesNameExactlyTheGhostNodes`（I5、I7）、`handlesAreNeverReused`（I6）、`resultsMatchTheGhost`（I8 の戻り値）。まとめて `inv`。
- 定数 `IgnoreList`：`true` にすると、id をノードの番号だけで引く（修正前のコード）。負の対照として、`buggy` インスタンスで `handlesNameExactlyTheGhostNodes` が破れることを確かめる。

モデルにしていないもの：値の型（値は `0..2` の整数で、判定は `v == c` と `v <= bound`）、`get_mut` と添字による値の書き換え（リンクに触れない）、`NEXT_LIST` の並行での採番（リストの番号はモデルでは 0 と 1 に固定）、`u64` のあふれ、panic（リストに無い id を渡す遷移は、無効な遷移として起こさない。panic の前に状態が変わらないことは P5 で確かめる）。長さの上限は 5（原典のケースが最大 6 件を使うので、`findNextElementOrElseInsert` の 5 件まで）。

### 検査の結果（2026-11-25、Quint 0.32.0）

| コマンド | 結果 |
|:---|:---|
| `quint typecheck docs/goals/specs/double_linked_list.qnt`、`..._test.qnt` | 成功 |
| `quint test --main=fixed --match='^(foreignHandleWithTheSameNumberIsNotFound\|removedHandleIsNotFoundAndNotReused\|moveElementsToBack\|findNextElementOrElseInsert)$' docs/goals/specs/double_linked_list_test.qnt` | 4 passing |
| `quint run --main=fixed --invariant=inv --max-samples=10000 --max-steps=40 --seed=0x1 docs/goals/specs/double_linked_list_test.qnt` | `[ok] No violation found`（10,000 トレース、各 41 状態） |
| `quint run --main=buggy --invariant=handlesNameExactlyTheGhostNodes --max-samples=2000 --max-steps=30 --seed=0x1 docs/goals/specs/double_linked_list_test.qnt` | `[violation]`。リスト 0 と 1 に 1 件ずつ追加した 2 手で、リスト 1 の id `{ list: 1, n: 0 }` がリスト 0 のノードを指す（PR #178 の Devin の指摘と同じ） |

`--match` を付けるのは、`quint test` が `import ... .*` で取り込んだ定義も `run` として拾い、状態を持たずに失敗するためである。

モデルの負の対照（仕様の写しに 1 つずつ入れて `quint run --main=fixed --invariant=inv` を流した。リポジトリの仕様は変えていない）：

| 変異 | 結果 |
|:---|:---|
| `unlink` が末尾を更新しない | `[violation]` |
| `shift_while` が進まない | `[violation]` |
| `move_to_front` が動かさない | `[violation]` |

仕様はコードと対で保つ。コードの手順（`link`、`unlink`、`shift_while`、id の形）を変えたら、仕様も直して、上の 3 つのコマンドを流し直す。

## 5. 壊しうる入力

| 入力 | 何が起きうるか | 確かめているもの |
|:---|:---|:---|
| 空のリスト | `first`／`last` が空のままか、片方だけ設定される | 原典 `start_empty`、`remove_elements`（最後の 1 件を外す）、Quint の `endsMatchEmptiness` |
| 1 要素 | 先頭と末尾が同じノード。`move_to_front`／`move_to_back` で自分を外して戻す | 原典 `remove_elements`、Quint（1 要素の状態を通る） |
| 先頭・末尾の境界 | 先頭への `prepend`、末尾への `append`、端での `get_*_or_else_*`、端で止まる `find_*` | 原典の 12 ケース、回帰 `find_stops_at_the_end_of_the_list_and_inserts_there`、Quint |
| 同じノードの移動 | 先頭を先頭へ、末尾を末尾へ（何もしない分岐） | 原典 `move_elements_to_front`、`move_elements_to_back` の最初の移動、Quint |
| 同じ値を持つ別のノード | 値の等価でなく id で区別できるか | Quint（値は 3 通りなので重なる）、P1〜P3 |
| 別のリストのハンドル | 番号が同じだと、無関係なノードを指す（修正前） | 回帰 `an_id_from_another_list_does_not_name_a_node_here`、`removing_an_id_from_another_list_panics_and_keeps_this_list`、Quint の `buggy` 負の対照、P5 |
| 削除済みハンドル | 外したノードの古いリンクをたどる（原典）／別の新しいノードを指す（id を再利用した場合） | 回帰 `remove_returns_the_value_and_a_removed_node_is_no_longer_in_the_list`、`a_removed_id_is_not_reused_by_a_later_insert`、`moving_a_removed_node_panics`、Quint の `handlesNameExactlyTheGhostNodes`、P5 |
| `new` が呼ばれるか | 既存のノードを返すのに値を作る | 回帰 `the_new_value_is_made_only_when_a_node_is_inserted`、P3 |
| 長い列 | 再帰は使わず、`forward_iter` と `shift_while` は反復なので、スタックは使わない。1 回の操作は O(log n)、全走査は O(n log n) | P1（列の長さを数百にする）。固定のテストは最大 6 件 |
| `u64` の採番のあふれ | `next_id` と `NEXT_LIST` が一巡すると id が衝突しうる | 範囲外（I6）。2^64 回の挿入か、リストの作成が要る |
| 並行してリストを作る | `NEXT_LIST` の採番が重なる | `fetch_add` は atomic なので重ならない。テストは無い |

## 6. 影響

- **公開 API**：追加だけ。`kernel::double_linked_list::DoubleLinkedList<A>` と `NodeId`。既存の型、関数、モジュールの公開面は変えていない。
- **`DoubleLinkedList::new` は `const fn` でない。** 最初の版（`724d84b`）は `const fn` だった。レビュー対応（`6c267a1`）で、`new` が `NEXT_LIST.fetch_add` を呼ぶようになり、`const fn` をやめた。この PR で足した API で、まだ利用者はいない。代わりに失ったものは、`static` や `const` の初期化子で空のリストを作れないことである。要るなら、リストの番号を遅れて振る形（最初の挿入で取る）にすれば `const fn` に戻せる。ただし `Option` の分岐が毎回の挿入に増えるので、今は採らない。
- **`NodeId` の形**：`NodeId(u64)` から `NodeId { list: u64, node: u64 }` に変わった（`724d84b` → `6c267a1`、同じ PR の中）。フィールドは非公開で、`Debug` の表示だけが変わる。`Ord` はリストの番号、次にノードの番号の順になる。
- **no_std**：kernel の追加は `alloc::collections::BTreeMap` と `core`（`core::ops`、`core::iter`、`core::sync::atomic::AtomicU64`）だけを使う。`AtomicU64` は executor（`kernel/executor/executor_value.rs` の `NEXT_ID`）がすでに使っているので、kernel が動く対象は増えも減りもしない。64 bit の atomic が無い対象では、どちらもビルドできない。
- **既存の契約**：無し。kernel の既存の型はこの型を使っていない。`FrequencyList` は内部の `FrequencyBuckets` のまま。
- **保存済みのデータ・上流・守りの仕組み**：無し。台帳は `DoubleLinkedListSpec` のノードを green にし、`docs/baseline/v2.0.0-undocumented-nodes.tsv` に 1 行足しただけである。

## 7. レビューで見てほしい点

- I8 の表が原典 `DoubleLinkedList.scala` の各操作と合っているか（特に `find_*` の「止まったノード」の定義と、端での挿入の位置）。
- P5 で「panic の前に状態を変えない」と言い切ってよいか。
- Quint の仕様の写し方（`link`、`unlink`、`shift_while`）がコードと合っているか。モデルにしていないものに、穴が無いか。
- PBT の案 1 と案 2 のどちらにするか。
