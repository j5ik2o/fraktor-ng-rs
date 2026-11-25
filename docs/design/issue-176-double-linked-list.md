# 設計：#176 DoubleLinkedList（PR #178）

[Issue #176](https://github.com/j5ik2o/fraktor-ng-rs/issues/176) と [PR #178](https://github.com/j5ik2o/fraktor-ng-rs/pull/178) の設計の記録である。2026-11-25 のオーナーの決定で、実装の独立レビューを廃止し、設計の独立レビューに置き換えた。この PR は実装が先に済んでいたので、出来上がった時点（HEAD `6c267a185e371e5904cac48765d97d7eb311adc3`）の設計をあとから書いた。実装に関わっていない担当が、この文書を読んで設計をレビューした（`81d2e00` に対して要修正。指摘の処理は §8）。§8 の修正で変わった点（PBT の実装、`next_id` と `NEXT_LIST` のあふれ、Quint のゴーストと判定）は、もう一度レビューを受ける。経緯と検証の結果は [ゴール文書](../goals/issue-176-double-linked-list.md) にある。

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
| `crates/fraktor-actor/tests/double_linked_list_model_test.rs` | PBT（§3 の P1〜P5）。原典外で、台帳の件数に含めない |

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
| I6 | 再利用しない：同じリストで、外した id は二度と返らない（`next_id` は単調に増え、減らない）。`next_id` と `NEXT_LIST` は `checked_add` で進め、`u64` を使い切る挿入やリストの作成は、debug でも release でも panic する（0 に戻って生きているノードを上書きし、I2 と I3 を壊すことはない）。現実には届かない |
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

`new` は戻り値が `v` のときだけ 1 回呼ばれ、それ以外では呼ばれない。`check`、`is_before`、`is_after` は `&A` しか受け取らないので、リストは変えられない。値そのものを内部可変性（`Cell` など）で書き換えないことは、呼び出し側の前提である。

## 3. PBT の性質

性質は、ランダムな操作の列をリストと「並びのモデル」（`Vec<(NodeId, A)>`、I8 の表をそのまま書いたもの）の両方に流し、毎回の操作の後で比べる形で定義する。操作の列には、上の 11 の操作に加えて、`get`、`get_mut`、`get_previous`、`get_next`、`forward_iter`、`backward_iter`、添字を混ぜる。渡す id は、今ある id、外した id、別のリストの id から選ぶ。

| 番号 | 性質 | 確かめる不変条件 |
|:---|:---|:---|
| P1 | 毎回の操作の後で、`forward_iter` の id と値の列がモデルの並びに等しく、`backward_iter` がその逆 | I3、I4、I8 |
| P2 | 毎回の操作の後で、`is_empty`、`get_first`、`get_last` がモデルの先頭と末尾に一致し、各ノードの `get_previous`、`get_next`、`get` がモデルの隣と値に一致する | I1、I2 |
| P3 | 各操作の戻り値（id と値）が、モデルの戻り値に等しい。`new` を呼んだ回数が、モデルが新しいノードを作った回数に等しい | I8 |
| P4 | 返った id はすべて互いに異なる（同じリストの中でも、リストをまたいでも） | I5、I6 |
| P5 | 外した id と別のリストの id について、`get` と `get_mut` は `None`。panic する操作に渡すと `the node is in this list` で panic し、その後の並び、値、リンクは P1 と P2 のとおり変わらない。利用者のクロージャ（`check`、`is_before`、`is_after`、`new`）が panic しても、リストの構造（並び、リンク、`first` と `last`）は変わらない。値も変わらないのは、クロージャが内部可変性などで値を書き換えない限りである | I5、I7 |

除外は付けない。panic する入力（外した id、別のリストの id、panic するクロージャ）も性質の対象に入れ、P5 で「panic して、並びを壊さない」ことを確かめる。

panic の前後で並びが変わらないことは、コードを読むと成り立っている。どの操作も、渡した id を最初に `node`／`node_mut`／`unlink` の頭で引き、そこで panic するので、リンクを書き換える前に止まる。`find_*` は `shift_while` で引いた後に止まった先を引くが、どれも読むだけである。利用者のクロージャも、`allocate` とリンクの書き換えより前に評価される（`allocate(new_value())` は引数の `new_value()` を先に評価する）。P5 はこれを実行で確かめる。値については、クロージャが `&A` から内部可変性で書き換えない限り変わらない（`&A` は書き換えを禁止しないので、これは呼び出し側の前提である）。`new` が panic したときに次の番号を使ってしまわないかは、公開 API から見えない（番号は `NodeId` の中で非公開）。上の評価の順で使わないうえ、番号は再利用しないので、使ったとしても飛ぶだけで、I5 と I6 は保たれる。

**実装（案 1、依存なし。設計レビューの推奨に従った）。** `crates/fraktor-actor/tests/double_linked_list_model_test.rs`。

- 疑似乱数は固定 seed の SplitMix64。リストは 3 本。値は `(key, serial)` で、`key` は 4 通り（判定が同じ値の別ノードを見る）、`serial` は一意（値の取り違えを見分ける）。
- `random_operations_match_the_model`：64 seed × 400 手。`long_random_operations_match_the_model`：4 seed × 3000 手で、挿入が削除より多いので、各 seed で最長のリストが 300 件以上になることも確かめる（数百件の列）。
- 1 手ごとに P1〜P4 を確かめ、外した id・別のリストの id・panic するクロージャの手は P5 を `catch_unwind` で確かめる。走査は「モデルの長さ + 1」件で打ち切るので、リンクが輪になっても止まらない走査にはならない。
- 失敗したときは、seed、手の番号、直前 12 手の操作を表示する。期待する panic は、それを起こす区間の中でだけ出力を抑える（区間の外の panic は、メッセージが同じでも表示する）。
- `removing_an_id_from_another_list_panics_and_keeps_this_list`：別のリストの id で `remove` して panic した後、両方のリストの並びと値が変わらないことを、固定の手順で確かめる（設計レビュー F3）。
- `proptest` を足すかは [#212](https://github.com/j5ik2o/fraktor-ng-rs/issues/212) で決める。

結果（2026-11-26）：`cargo test -p fraktor-actor --test double_linked_list_model_test` は 3 passed（約 3 秒）。

コードの負の対照（1 つずつ一時的に入れ、毎回元に戻した）：

| 変異 | 結果 |
|:---|:---|
| `shift_while` が進まない | 失敗（P3：`new` を呼ぶ回数） |
| `unlink` が末尾を更新しない | 失敗（後の操作が古い末尾を引いて panic。期待しない区間の panic なので表示される） |
| `move_to_back` が先頭を見る | 失敗（P1） |
| `get_previous_or_else_insert` が後ろに挿入する | 失敗（P1） |
| `get_first_or_else_prepend` の `check` を反転 | 失敗（P3） |
| `remove` が `unlink` しない | 失敗（外したノードを引いて panic） |
| `remove` が id を引く前に `first` と `last` を消す | 失敗（P1。P5 の「panic の前に状態を変えない」の破れ） |
| 挿入の後に `next_id` を戻す（id の再利用） | 失敗（P4） |
| `link` が `next` に前のノードを入れる（輪ができる） | 100 秒で打ち切り。テストの走査は打ち切るが、`find_*` の `shift_while` がコードの中で輪を回り続ける。CI ではジョブの時間切れで落ちるが、速くは落ちない |

## 4. 状態遷移と Quint

**Quint は要る。** `DoubleLinkedList` は状態を持つデータ構造で、どの操作も複数のリンク（自分、前、次、`first`、`last`）を書き換える遷移である。不変条件 I1〜I3 は遷移の列の全体で成り立つ必要があり、固定のテストでは書き換えの順序の誤りを取りこぼしうる。実際に、PR #178 のレビューで、遷移の列（2 つのリストを作り、片方の id をもう片方に使う）でしか出ない誤り（I5 の違反）が見つかった。並行は無いので、仕様は 1 スレッドの遷移だけを扱う。

仕様は `docs/design/specs/double_linked_list.qnt`（本体）と `docs/design/specs/double_linked_list_test.qnt`（インスタンスとテスト）。

- 状態：2 つのリスト（`lists: int -> ListState`）。各リストは、id をキーにしたノードの map と、`first`、`last`、`nextId` を持つ。コードの `BTreeMap<NodeId, Node>` と同じ形である。
- 遷移：`prependValue`、`appendValue`、`removeNode`、`moveToFront`、`moveToBack`、`getEndOrElseInsert`（先頭と末尾の 2 つ）、`findOrElseInsert`（前後の `find_*` 2 つ。`bound = -1` で `get_next_or_else_insert` と `get_previous_or_else_insert`）。リンクの書き換えはコードの `insert_before`、`insert_after`、`link`、`unlink`、`shift_while` を 1 対 1 で写した。
- ゴースト：各遷移は、原典の意味（I8 の表）でも並びを更新する（`ghost: int -> List[{ h: Handle, v: int }]`）。ゴーストは値も持ち、`check` と `is_before` の判定はゴーストの値で行う。新しいノードのゴーストの値は、遷移に渡した値で、リンク側が `allocate` で入れた値を読まない。リンク側とゴースト側はそれぞれ独立に判定し、食い違ったら遷移を止めずに `resultOk = false` を記録する（`disagree`）。止めると誤りが隠れるため。`getEndOrElseInsert` と `findOrElseInsert` の両方がこの形で、挿入する分岐には、長さの上限のほかにガードは無い（設計レビュー F1、F2）。
- `removeNode` は、リンク側が返す値（`remove` の戻り値に当たる）がゴーストの値に等しいことも確かめる。
- 不変条件：`endsMatchEmptiness`（I1）、`linksAgree`（I2）、`walksAreTheGhostOrder`（I3、I4、I8 の並びと値。リンクをたどって読んだ値の列がゴーストの値の列に等しい）、`handlesNameExactlyTheGhostNodes`（I5、I7）、`handlesAreNeverReused`（I6）、`resultsMatchTheGhost`（I8 の戻り値）。まとめて `inv`。
- 定数 `IgnoreList`：`true` にすると、id をノードの番号だけで引く（修正前のコード）。負の対照として、`buggy` インスタンスで `handlesNameExactlyTheGhostNodes` が破れることを確かめる。

モデルにしていないもの：値の型（値は `0..2` の整数で、判定は `v == c` と `v <= bound`）、`get_mut` と添字による値の書き換え（リンクに触れない）、`NEXT_LIST` の並行での採番（リストの番号はモデルでは 0 と 1 に固定）、`u64` のあふれ、panic（リストに無い id を渡す遷移は、無効な遷移として起こさない。panic の前に状態が変わらないことは P5 で確かめる）。長さの上限は 5（原典のケースが最大 6 件を使うので、`findNextElementOrElseInsert` の 5 件まで）。

### 検査の結果（2026-11-26、Quint 0.32.0。F1、F2 の修正の後）

| コマンド | 結果 |
|:---|:---|
| `quint typecheck docs/design/specs/double_linked_list.qnt`、`..._test.qnt` | 成功 |
| `quint test --main=fixed --match='^(foreignHandleWithTheSameNumberIsNotFound\|removedHandleIsNotFoundAndNotReused\|moveElementsToBack\|findNextElementOrElseInsert)$' docs/design/specs/double_linked_list_test.qnt` | 4 passing |
| `quint run --main=fixed --invariant=inv --max-samples=10000 --max-steps=40 --seed=0x1 docs/design/specs/double_linked_list_test.qnt` | `[ok] No violation found`（10,000 トレース、各 41 状態、約 50 秒） |
| `quint run --main=buggy --invariant=handlesNameExactlyTheGhostNodes --max-samples=2000 --max-steps=30 --seed=0x1 docs/design/specs/double_linked_list_test.qnt` | `[violation]`。リスト 0 と 1 に 1 件ずつ追加した 2 手で、リスト 1 の id `{ list: 1, n: 0 }` がリスト 0 のノードを指す（PR #178 の Devin の指摘と同じ） |

**`quint test` には必ず `--match` を付け、`4 passing` を確かめる。** Quint 0.32.0 の `quint test --main=fixed docs/design/specs/double_linked_list_test.qnt` は、`--match` が無いと `fixed` とだけ表示し、テストを 0 件流して exit 0 で終わる（2026-11-26 に実測。設計レビュー F8）。付け忘れると黙って何も検査しない。逆に `--match='.*'` のように広く取ると、`import ... .*` で取り込んだ定義も拾い、状態を持たずに失敗する。CI で流すことは [#213](https://github.com/j5ik2o/fraktor-ng-rs/issues/213) で扱う。

モデルの負の対照（仕様の写しに 1 つずつ入れて `quint run --main=fixed --invariant=inv` を流した。リポジトリの仕様は変えていない）：

| 変異 | 結果 |
|:---|:---|
| M8：`getEndOrElseInsert` のリンク側の `check` を反転 | `[violation]`（F1 の前は検出できなかった） |
| M9：`getEndOrElseInsert` のリンク側の端が常に `check` を満たす | `[violation]`（F1 の前は検出できなかった） |
| M10：`allocate` が値を常に 0 にする | `[violation]`（F2 の前は `inv` で検出できなかった） |
| `removeNode` のリンク側の戻り値を 0 にする | `[violation]`（F2 で足した照合） |
| M4：`get_last_or_else_append` が先頭を読む | `[violation]` |
| `unlink` が末尾を更新しない | `[violation]` |
| `shift_while` が進まない | `[violation]` |
| `move_to_front` が動かさない | `[violation]` |

どれも `quint run --main=fixed --invariant=inv --max-samples=3000 --max-steps=30 --seed=0x1` で、仕様の写しに 1 つずつ入れて流した。

仕様はコードと対で保つ。コードの手順（`link`、`unlink`、`shift_while`、id の形）を変えたら、仕様も直して、上の 4 つのコマンドを流し直す。

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
| 利用者のクロージャが panic する | `check`、`is_before`、`is_after`、`new` の panic で、挿入やリンクの書き換えが途中で止まる | P5（`panicking_closure`。10 通りの操作で、どれも panic して並びが変わらないことを確かめる。`check` が `find_*` の中で panic する場合も含む）。順序はコードの評価の順（§3）で守られる |
| 長い列 | 再帰は使わず、`forward_iter` と `shift_while` は反復なので、スタックは使わない。`find_*` 以外の操作は O(log n)。`find_*` は歩いた件数 k に対して O(k log n)。全走査は O(n log n)。原典はそれぞれ O(1)、O(k)、O(n) で、ノードを `BTreeMap` で引く分だけ log n 倍遅い | P1（`long_random_operations_match_the_model` で最長 300 件以上）。固定のテストは最大 6 件 |
| `u64` の採番のあふれ | `next_id` と `NEXT_LIST` が一巡すると、id が衝突し、`allocate` が生きているノードを上書きしうる | `checked_add` で、debug でも release でも panic にした（I6、設計レビュー F5）。2^64 回は届かないので、テストは無い |
| 並行してリストを作る | `NEXT_LIST` の採番が重なる | `try_update`（compare-and-swap）は atomic なので重ならない。テストは無い |

## 6. 影響

- **公開 API**：追加だけ。`kernel::double_linked_list::DoubleLinkedList<A>` と `NodeId`。既存の型、関数、モジュールの公開面は変えていない。
- **`DoubleLinkedList::new` は `const fn` でない。** 最初の版（`724d84b`）は `const fn` だった。レビュー対応（`6c267a1`）で、`new` が `NEXT_LIST` から番号を取るようになり、`const fn` をやめた（設計レビュー F5 の後は `try_update` と `checked_add`。2^64 個目のリストで panic する）。この PR で足した API で、まだ利用者はいない。代わりに失ったものは、`static` や `const` の初期化子で空のリストを作れないことである。要るなら、リストの番号を遅れて振る形（最初の挿入で取る）にすれば `const fn` に戻せる。ただし `Option` の分岐が毎回の挿入に増えるので、今は採らない。
- **`NodeId` の形**：`NodeId(u64)` から `NodeId { list: u64, node: u64 }` に変わった（`724d84b` → `6c267a1`、同じ PR の中）。フィールドは非公開で、`Debug` の表示だけが変わる。`Ord` はリストの番号、次にノードの番号の順になる。
- **no_std**：kernel の追加は `alloc::collections::BTreeMap` と `core`（`core::ops`、`core::iter`、`core::sync::atomic::AtomicU64`）だけを使う。`AtomicU64` は executor（`kernel/executor/executor_value.rs` の `NEXT_ID`）がすでに使っているので、kernel が動く対象は増えも減りもしない。64 bit の atomic が無い対象では、どちらもビルドできない。
- **`Clone` を足すとき**：今の `DoubleLinkedList` は `Clone` を持たない。`#[derive(Clone)]` を足すと、複製が元と同じリストの番号を持ち、元の id が複製のノードも指す（I5 が破れる）。足すなら、複製に番号を振り直す（id は複製では使えない）か、id が複製でも有効だという意味を決めて I5 を書き直す（設計レビュー F6）。
- **計算量**：§5 の「長い列」のとおり、原典より log n 倍遅い。FrequencyList や RecencyList をこの型の上に作り直すときは、slab（`Vec` と世代番号）でノードを引く形を検討する（設計レビュー F7）。
- **既存の契約**：無し。kernel の既存の型はこの型を使っていない。`FrequencyList` は内部の `FrequencyBuckets` のまま。
- **保存済みのデータ・上流・守りの仕組み**：無し。台帳は `DoubleLinkedListSpec` のノードを green にし、`docs/baseline/v2.0.0-undocumented-nodes.tsv` に 1 行足しただけである。

## 7. レビューで見てほしい点

最初のレビュー（`81d2e00`）で見た点は §8 で処理した。2 回目に見てほしいのは、変わった点だけである。

- PBT（§3）の性質と実装が P1〜P5 を満たしているか。取りこぼしている操作や id の選び方が無いか。
- Quint のゴーストと独立の判定（§4）で、F1 と F2 が塞がったか。
- `next_id` と `NEXT_LIST` の `checked_add`（I6、§6）。

## 8. 設計レビューの指摘の処理（`81d2e00` に対して、2026-11-26）

レビューは実装に関わっていない Opus 5.5（`claude-opus-5-5`）が行った。判定は要修正。

| 指摘 | 判定 | 処理 |
|:---|:---|:---|
| F1：`getEndOrElseInsert` にゴースト側の独立した判定が無く、挿入する分岐のガードで遷移が止まり、`check` の反転（M8、M9）を検出できない | 直す | ゴースト側の判定を独立に計算し、リンク側と両方が満たす、両方が満たさない、食い違う、の 3 分岐にした。食い違いは `disagree` で `resultOk = false` を記録する。挿入する分岐のガードは長さの上限だけ。`findOrElseInsert` も同じ形にし、`findDisagrees` はこれに吸収した。M8、M9 は `[violation]` になる（§4） |
| F2：ゴーストがリンク側の値を読むので、値の取り違え（M10）を検出できない。`remove` の戻り値の値も見ていない | 直す | ゴーストに値を持たせ（`{ h, v }`）、判定はゴーストの値で行う。新しいノードのゴーストの値は遷移に渡した値にする。`walksAreTheGhostOrder` で、リンクをたどって読んだ値の列とゴーストの値の列を比べる。`removeNode` はリンク側の戻り値とゴーストの値を比べる。M10 は `[violation]` になる |
| F3：`removing_an_id_from_another_list_panics_and_keeps_this_list` は `#[should_panic]` で、「keeps this list」を確かめていない | 直す | kernel の回帰は名前を `removing_an_id_from_another_list_panics` に縮めた。「keeps this list」は `tests/double_linked_list_model_test.rs` の同名のテストで、`catch_unwind` の後の並びと値を確かめる。PBT の P5 でも全操作について確かめる |
| F4：利用者のクロージャの panic が壊しうる入力に無く、評価の順を守るものが無い | 直す | P5 に「クロージャが panic してもリストの構造が変わらない」を足し、PBT の `panicking_closure`（10 通り。`check` が `find_*` の中で panic する 2 通りを足した）で確かめる。値の不変は内部可変性を使わない前提つきである。§5 に行を足した。「次に返る id が変わらない」は公開 API から見えないので、性質にはせず、評価の順と再利用しないことで I5 と I6 が保たれる理由を §3 に書いた |
| F5：`next_id` があふれると release で 0 に戻り、生きているノードを黙って上書きする。`NEXT_LIST` も同じ | 直す | `next_id` は `checked_add(1).expect(..)`、`NEXT_LIST` は `try_update(.., checked_add)` にし、debug と release の両方で panic にそろえた。rustdoc の `# Panics` と I6 を直した |
| F6：将来 `Clone` を足すと I5 が破れる | 直す（記録） | §6 に、足すときに決めることを書いた |
| F7：「1 回の操作は O(log n)」は `find_*` で成り立たない | 直す | §5 を「`find_*` は O(k log n)」に直し、原典との差と、置き換えるときの slab の検討を §5 と §6 に書いた |
| F8：`--match` が無いと `quint test` は 0 件で exit 0（文書の記述が実測と違う） | 直す（文書）、CI は別件 | 実測を確かめて §4 の記述を直し、必ず `--match` を付けて `4 passing` を確かめる手順にした。CI で流すことは [#213](https://github.com/j5ik2o/fraktor-ng-rs/issues/213) |
| 推奨：PBT は案 1 を、この PR でマージの前に実装する | 直す | §3 のとおり実装した |
| 推奨：`proptest` を workspace の dev-dependency にするかは別の issue で | 別件 | [#212](https://github.com/j5ik2o/fraktor-ng-rs/issues/212) |
