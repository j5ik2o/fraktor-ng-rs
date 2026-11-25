# ゴール G0issue176：classic DoubleLinkedListSpec

状態：**green**。原典 12 件を移植し、12 件とも pass。除外 0 件、作らない 0 件。

[Issue #176](https://github.com/j5ik2o/fraktor-ng-rs/issues/176)（Orca 項目359）の対象は、Pekko v2.0.0（commit `9fbd2283b1cf701c90a1223bf621942c191db61f`）の classic `DoubleLinkedListSpec`。原典は「DoubleLinkedList must」の下に 12 ケースを持ち、空のリスト、前後への追加、削除、前後への移動、先頭・末尾・隣の値を確かめてから無ければ挿入する操作、条件に合う間だけ進んでから挿入する操作を確かめる。 [原典のテスト](https://github.com/apache/pekko/blob/9fbd2283b1cf701c90a1223bf621942c191db61f/actor-tests/src/test/scala/org/apache/pekko/util/DoubleLinkedListSpec.scala) [原典の実装](https://github.com/apache/pekko/blob/9fbd2283b1cf701c90a1223bf621942c191db61f/actor/src/main/scala/org/apache/pekko/util/DoubleLinkedList.scala)

同じ系統の [#124 RecencyList](issue-124-recency-list.md)、[#126 SegmentedRecencyList](issue-126-segmented-recency-list.md)、[#128 FrequencyList](issue-128-frequency-list.md) の続き。当初は [#177 FrequencySketchSpec](https://github.com/j5ik2o/fraktor-ng-rs/issues/177) と同じ PR の予定だったが、2026-11-25 に分担を変え、#177 は別の担当が単独の PR で扱う。

設計（不変条件、PBT の性質、Quint の仕様と検査の結果、壊しうる入力、影響）は [docs/design/issue-176-double-linked-list.md](../design/issue-176-double-linked-list.md)、Quint の仕様は `docs/design/specs/double_linked_list.qnt` にある。

## 対象ノード

| id | Rust の統合テスト | 原典のケース数 | 状態 |
|:---|:---|---:|:---|
| `actor-tests/src/test/scala/org/apache/pekko/util/DoubleLinkedListSpec.scala` | `crates/fraktor-actor/tests/double_linked_list_spec.rs` | 12 | green（12/12/12） |

台帳は原典 12 件、移植 12 件、成功 12 件、除外 0 件、作らない 0 件、`verified_against: v2.0.0`。`needs` は空のままで、TestKit の機能は使わない。

件数は原典のファイルで数え直した。`"..." in {` の行が 12 本で、文字列で始まる行はこの 12 本と `"DoubleLinkedList" must {` の 13 本。`verify_red.py` の `count_original_cases` は `must {` を含むファイルを数えない作りなので None を返す（RecencyList などと同じ）。v1.7.1 から v2.0.0 までの原典の変更は、テストが `870d4499cc` #2235（import の並び）と `9dfca01566` #3348（import の書き方）、実装が `4749eda97f` #3072（`private[this]` の置き換え）だけで、ケースと契約は変わらない。

## 既存の公開面との照合

起点は `origin/main` の `3dc4bcb`。`crates/` に汎用の双方向連結リストは無い。`FrequencyList` は内部に `FrequencyBuckets`（バケット id で引く双方向連結リスト）を持つが、非公開で、値の型もバケットに固定している。このため新しい公開面 `kernel::double_linked_list` を足した。既存の API は変えていない。kernel の追加は `alloc` と `core` だけを使う。

## 公開面の決定（2026-11-25、run 経由で承認）

原典の `DoubleLinkedList[Node]` は侵入型である。利用者のノード型が自分の前後のリンクを持ち、リストはそのリンクを読み書きする 4 つの関数で作る。Rust にはこれと一対一の形が無い。候補を 2 つ挙げ、案 A で承認を得た（追加だけで既存の公開契約を変えず、RecencyList と FrequencyList の id で引く作りと揃うため、照会なしの通常の移植として扱う）。

- **案 A（採用）**：`DoubleLinkedList<A>` が値を所有し、リンクを自分で持つ。挿入は値のノードの `NodeId` を返し、原典でノードを受け取る操作は `NodeId` を受け取る。
- 案 B（不採用）：`Rc<RefCell<Node>>` とリンクのアクセサ関数を渡す、原典どおりの侵入型。参照の循環と借用の実行時検査が要り、kernel の他の型の作りと揃わない。

承認の条件は、(a) 侵入型をハンドル型に置き換えたことと「同じノード」をハンドルの等価で確かめることを、PR 本文とこの文書に原典との差として書くこと、(b) 件数を v2.0.0 で記録し、除外と作らないにはケースごとの根拠を残すこと（今回はどちらも 0 件）。

### 原典との対応

| 原典 | 移植 |
|:---|:---|
| `new DoubleLinkedList[Node](getPrevious, getNext, setPrevious, setNext)` | `DoubleLinkedList::<A>::new()`（リンクはリストが持つ） |
| `isEmpty` | `is_empty()` |
| `getFirst`、`getLast`（`OptionVal[Node]`） | `get_first()`、`get_last()`（`Option<NodeId>`） |
| ノードの値を読む | `list[id]`、`get(id)`、`get_mut(id)` |
| コンストラクタに渡す `getPrevious`、`getNext` | `get_previous(id)`、`get_next(id)` |
| `prepend(node)`、`append(node)` | `prepend(value)`、`append(value)`。値のノードの `NodeId` を返す |
| `remove(node)` | `remove(id)`。リストから外して値を返す |
| `moveToFront(node)`、`moveToBack(node)` | `move_to_front(id)`、`move_to_back(id)` |
| `getFirstOrElsePrepend(check, newNode)` ほか、`OrElse` の 6 つ | 同名の snake_case。`check` と `isBefore`／`isAfter` は値を受け取るクロージャ、名前渡しの `newNode: => Node` は値を作る `FnOnce() -> A` |
| `forwardIterator`、`backwardIterator` | `forward_iter()`、`backward_iter()`（`NodeId` を順に返す） |

### 原典と違う点

| 項目 | Pekko | 移植 |
|:---|:---|:---|
| ノードの持ち方 | 侵入型。利用者のノードがリンクを持つ | リストが値とリンクを持ち、`NodeId` で引く |
| 「同じノードが返った」の確かめ方 | ノードの値の等価（テストの `shouldBe b`） | `NodeId` の等価。テストは既存のノードの id と返った id を比べる |
| まだリストに無いノードを新しいノードとして渡す | 作っておいたノード（`a` など）を渡し、返ったノードと比べる | 値を作るクロージャを渡し、返った id の値とリストの並びを確かめる |
| `remove` の後 | 外したノードは残り、利用者がまた挿入できる | 値を返す。また挿入すると新しい id になる。外した id は再利用しない |
| リストに無いノードを渡す | 古いリンクをたどり、リストが壊れうる | panic（`the node is in this list`）。`NodeId` はリストごとの番号を持つので、別のリストの id もここに当たる |
| 計算量 | ノードを直接たどるので定数時間 | ノードを順序付きの map で引くので対数時間 |

## 先行テストと red の証拠

テストは承認した案 A の API で書き、製品より先に置いた（commit `62523b2`、製品は未変更）。原典の 12 ケースを 1 ケース 1 テストにし、名前はケース名の snake_case、原典の名前をコメントに残した。操作と `check` の位置は原典どおり。原典の `Node(value)` は値そのもの（`&'static str`）にした。

| コマンド | 終了値 | 結果 |
|:---|---:|:---|
| `cargo test -p fraktor-actor --test double_linked_list_spec` | 101 | compile-red。`E0432 unresolved import fraktor_actor::kernel::double_linked_list`（`tests/double_linked_list_spec.rs:29`）の 1 件。実行 0 件 |

これはコンパイル失敗であり、already_green ではない。台帳はこの commit で `status: red`、`rust` の 2 欄、`goal: G0issue176` にした。

## 実装

| ファイル | 内容 |
|:---|:---|
| `crates/fraktor-actor/src/kernel/double_linked_list.rs` | モジュールの宣言と再エクスポート |
| `crates/fraktor-actor/src/kernel/double_linked_list/double_linked_list_value.rs` | `DoubleLinkedList<A>`。原典の `insertBefore`、`insertAfter`、`link`、`unlink`、`shiftWhile` を同じ手順で持つ |
| `crates/fraktor-actor/src/kernel/double_linked_list/node_id.rs` | `NodeId`。`Copy`、`Eq`、`Ord`、`Hash` |

## 回帰のテスト（原典外、台帳の件数に含めない）

`crates/fraktor-actor/src/kernel/double_linked_list/double_linked_list_value_test.rs` の 9 件。

| テスト | 確かめること |
|:---|:---|
| `remove_returns_the_value_and_a_removed_node_is_no_longer_in_the_list` | `remove` が値を返し、外した id は引けず、隣のリンクも外れる |
| `a_removed_id_is_not_reused_by_a_later_insert` | 外した id を後の挿入が再利用しない |
| `moving_a_removed_node_panics` | リストに無い id を渡すと panic |
| `previous_and_next_follow_the_links` | `get_previous` と `get_next` が移動の後もリンクをたどる |
| `the_new_value_is_made_only_when_a_node_is_inserted` | 既存のノードを返す 4 つの `OrElse` は、新しい値のクロージャを呼ばない（名前渡しの意味） |
| `find_stops_at_the_end_of_the_list_and_inserts_there` | `find_*` がリストの端で止まり、そこに挿入する |
| `index_mut_changes_the_value_in_place` | `list[id] = ..` と `get_mut` が値を書き換える |
| `an_id_from_another_list_does_not_name_a_node_here` | 別のリストの id は、番号が同じでもこのリストのノードを指さない（`get`、`get_mut` が `None`） |
| `removing_an_id_from_another_list_panics` | 別のリストの id で `remove` すると panic する（panic の後もリストが変わらないことは、`tests/double_linked_list_model_test.rs` の同名のテストと PBT が `catch_unwind` で確かめる） |

## 最終の検証

| コマンド | 終了値 | 結果 |
|:---|---:|:---|
| `cargo test -p fraktor-actor --test double_linked_list_spec` | 0 | 12 passed、0 failed（原典 12 件） |
| `cargo test -p fraktor-actor --lib double_linked_list` | 0 | 9 passed、0 failed（レビュー対応後。対応前は 7 件） |
| `cargo test --workspace` | 0 | 77 結果、922 passed、0 failed、0 ignored |
| `cargo clippy --workspace --all-targets -- -D warnings` | 0 | 警告なし |
| `cargo fmt --check` | 0 | 差分なし |
| syn-check（CI と同じく `crates/*` ごと、Release/locked） | 0 | fraktor-actor 270 ファイル、fraktor-testkit 46 ファイル、violations 0、parse failures 0 |
| `npm run check:aislop` | 0 | accepted と一致 |
| `cargo check -p fraktor-actor --lib` と `git diff --check` | 0 | 成功、指摘なし |
| CI の python テスト 12 本と Pekko baseline の検査 4 本（`report_ledger_1to1.py check` を含む） | 0 | すべて成功 |

`report_ledger_1to1.py check` の検査 5 は、`verified_against: v2.0.0` を付けて報告書に載っていないノードを `docs/baseline/v2.0.0-undocumented-nodes.tsv` に求める。`list-undocumented` で一覧を作り直し、このノードの 1 行（`no-report`）が増えた。

負の対照（1 つずつ一時的に入れ、毎回元に戻した。最終の差分には含めない）：

| 変異 | 結果 |
|:---|:---|
| `shift_while` が進まない | `find_next_element_or_else_insert_new_element`、`find_previous_element_or_else_insert_new_element` が失敗 |
| `insert_before` が前のノードを無視する | `get_previous_element_or_else_insert_new_element`、`find_previous_element_or_else_insert_new_element` が失敗 |
| `move_to_front` が動かさない | `move_elements_to_front` が失敗 |
| `unlink` が末尾を更新しない | `remove_elements`、`move_elements_to_front` が失敗 |
| `backward_iter` が 1 件で止まる | 11 件が失敗 |

## G0 と対象ゴールの確認

| コマンド | 結果 |
|:---|:---|
| `rollup.py --goal G0issue176` | 1 件で `{'green': 1}`、「完了: 全ノードが green か do_not_port か not_built」 |
| `check_needs.py --goal G0issue176` | needs 閉性 OK |
| `rollup.py --goal G0a`〜`G0f` | 各ゴール「完了: 全ノードが green か do_not_port か not_built」 |
| `check_needs.py --goal G0a`〜`G0f` | 各ゴール「needs 閉性 OK」 |

## PR #178 のレビュー対応

基準 HEAD は `1ffe0b73d1a6fb627ed537c590009947b4c7a58a`。

| 指摘 | 分類 | 対応 |
|:---|:---|:---|
| 別のリストの id で無関係な要素を誤削除する（Devin、`double_linked_list_value.rs:279`） | 直す | 各リストが 0 から採番し、`NodeId` の等価が番号だけで決まっていたため、別のリストの id が一致して通っていた。リストに全体で一意の番号（`static` の `AtomicU64`、executor の採番と同じ形）を持たせ、`NodeId` にリストの番号を含めた。別のリストの id は map で引けず、`get` は `None`、他の操作は panic する。先に足した回帰 2 件は修正前に失敗した（`assert_ne!` が成り立たず、`remove` が panic しない）。`DoubleLinkedList::new` は atomic を使うため `const fn` でなくなった（この PR で足した API で、利用者はまだいない） |
| Devin のレビュー本文（上の 1 件の要約） | 直す | 上と同じ |
| CodeRabbit の利用上限の通知、Codex のレビュー失敗と利用上限の通知 | 採らない | 指摘を含まない通知で、対応する変更は無い |

## 設計の独立レビューへの対応（2026-11-26）

設計は [docs/design/issue-176-double-linked-list.md](../design/issue-176-double-linked-list.md)。`81d2e00` に対する設計レビューの指摘 F1〜F8 の処理は、その §8 にある。PBT（`tests/double_linked_list_model_test.rs`、P1〜P5、原典外）を足し、`next_id` と `NEXT_LIST` のあふれを panic にそろえ、Quint の仕様のゴーストに値と独立の判定を持たせた。別件は [#212](https://github.com/j5ik2o/fraktor-ng-rs/issues/212)（proptest）と [#213](https://github.com/j5ik2o/fraktor-ng-rs/issues/213)（Quint を CI で流す）。
