# ゴール G0issue145：classic の部分関数ビルダーの2スイートを作らない区分で記録する

[Issue #145](https://github.com/j5ik2o/fraktor-ng-rs/issues/145) と [Issue #146](https://github.com/j5ik2o/fraktor-ng-rs/issues/146)（Orca 項目222）の対象は、Pekko v1.7.1 の次の2つの Java テストである。`Match` は `PFBuilder` で部分関数を組むので、2つのノードを1つのゴールにまとめた。

- `PFBuilderTest`: [原典のテスト](https://github.com/apache/pekko/blob/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/actor-tests/src/test/java/org/apache/pekko/japi/pf/PFBuilderTest.java)、[原典の実装](https://github.com/apache/pekko/blob/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/actor/src/main/java/org/apache/pekko/japi/pf/PFBuilder.java)。1ケース
- `MatchBuilderTest`: [原典のテスト](https://github.com/apache/pekko/blob/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/actor-tests/src/test/java/org/apache/pekko/japi/MatchBuilderTest.java)、[原典の実装](https://github.com/apache/pekko/blob/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/actor/src/main/java/org/apache/pekko/japi/pf/Match.java)。3ケース

4ケースとも、台帳の「作らない区分」（`status: not_built`）で記録した。移植0件・除外0件・作らない4件である。区分の定義と当てはめる条件は [`ledger-schema-draft.md` の「作らない区分」](../decisions/ledger-schema-draft.md)にある。このゴールが、区分の最初の適用である。

## 対象ノード

| id | 原典 | 移植 | 除外 | 作らない |
|:---|---:|---:|---:|---:|
| `actor-tests/src/test/java/org/apache/pekko/japi/pf/PFBuilderTest.java` | 1 | 0 | 0 | 1 |
| `actor-tests/src/test/java/org/apache/pekko/japi/MatchBuilderTest.java` | 3 | 0 | 0 | 3 |

合計は原典4件・移植0件・除外0件・作らない4件。`needs` はどちらも空である。原典件数は `@Test` の数で、`verify_red.py` の `count_original_cases` も1件と3件と数えた。

## 経緯と決定

1. main `95f19d0` を基準に、原典4ケースを先に移植した（隔離枝 [`j5ik2o/issue-145-146-match-builder`](https://github.com/j5ik2o/fraktor-ng-rs/tree/j5ik2o/issue-145-146-match-builder)、HEAD [`20fffb6`](https://github.com/j5ik2o/fraktor-ng-rs/commit/20fffb68592028789b48735716d100ce54538538)）。製品差分は0で、2本とも `E0432 unresolved import fraktor_actor::kernel::partial_function` の compile-red だった（`cargo test -p fraktor-actor --test pf_builder_test` と `--test match_builder_test` は終了値101。`verify_red.py` は2ノードとも `expected_compile_error`、`excluded: 0`）。already_green ではない。製品に部分関数や `Match` に当たる型がない。
2. 4ケースを移植するには、kernel に新しい公開モジュール（部分関数、そのビルダー、不成立の誤り）を足す必要があった。担当は公開する前に調整担当へ報告した。調べた範囲では、非公開のままでは検証できない。台帳のゲート（`verify_red.py`・`verify_green.py`）は `cargo test -p <crate> --test <stem>` で統合テストだけを走らせ、統合テストからは `pub` しか見えないためである。
3. オーナーの決定（元帥の項目223、2026-11-25、副元帥経由）：「作らない区分を認める（理由を台帳に書いて対象外にする）」。Rust の言語機能で代わりが効く Java 向けの補助は、公開 API として作らない。
4. 本 PR は、main `8c8d455` を起点にした別の枝で、台帳・文書・集計だけを変える。隔離枝から実装やコミットは取り込んでいない。製品の公開 API は足していない。隔離枝は先行テストと red の証拠として残す。

実行経路は直接実装である。TAKT にはゴールもタスクも投入していない。担当は Claude Opus 5.5 の1名だけである。

## ケースごとの評価

どのケースも、原典の言語のための補助そのもの（Java のラムダの型推論、`Class` での型の判定、型消去の回避、述語の型付け）を検証している。観測している振る舞いは、「登録した順に case を試す」「当たった case の値を返す」「どれにも当たらなければ不成立になる」の3つで、いずれも Rust の `match`・`if let`・ガード・`Option` と、既存の `Message::downcast_ref` で得られる。実行時の契約、JVM の代替の設計、Java からの到達性だけの検査、文書のサンプルには当たらない。

| 原典のケース | 原典が検証していること | 作らない理由 | Rust の代替 |
|:---|:---|:---|:---|
| `pfbuilder_matchAny_should_infer_declared_input_type_for_lambda` | `PFBuilder.<String, Integer>create()` で宣言した入力型を、`matchAny` のラムダが推論すること。`matchEquals("hello")` と `matchAny` の両方で、`"hello"` と `"42"` が定義域に入り、`apply("42")` が 42 を返すこと | ラムダの型推論は Java のビルダーの補助である。Rust では関数の引数型が入力型を固定し、`match` の腕は上から順に試される | 代替1 |
| `shouldPassBasicMatchTest` | `Integer.class` の case が先に、`Number.class` の case が後に登録され、整数は先の case、倍精度値は後の case に当たり、文字列は `MatchError` になること | クラスで case を組むビルダーは、Java に値のパターンマッチがないための補助である。Rust では `downcast_ref` を順に試す `if let` の連鎖で同じ優先順位になり、不成立は `None` になる。`Number` のような上位型は、受け付ける数値型を腕に並べる | 代替2 |
| `shouldHandleMatchOnGenericClass` | `matchUnchecked(GenericClass.class, ...)` で `GenericClass<String>` を受け、`val` の `"A"` を取り出すこと | `matchUnchecked` は、Java の型消去で `GenericClass<String>` を `Class` で表せないための補助である。Rust の型引数は実行時にも消えないので、検査つきの `downcast_ref::<GenericClass<String>>()` で取り出せる | 代替3 |
| `shouldHandleMatchWithPredicateOnGenericClass` | `TypedPredicate` で `val` が空でないことを条件にし、空文字列の `GenericClass` が `MatchError` になること | 型の判定に述語を重ねる `TypedPredicate` は、ラムダの型付けの補助である。Rust では `match` のガードか `Option::filter` で書け、述語を満たさなければ `None` になる | 代替4 |

### 代替の例

次のコードは、main `8c8d455` の `fraktor-actor` を path 依存にした一時クレートの統合テストとして実行し、4件とも成功した（`cargo test --test alternatives` で `4 passed; 0 failed`）。各テストは、原典のケースの入力と期待値をそのまま使う。

```rust
use fraktor_actor::kernel::message::Message;

// 代替1（PFBuilderTest）：入力型は引数の型で決まり、腕は上から順に試される。
fn pf(s: &str) -> Option<i32> {
  match s {
    "hello" => Some(1),
    s => s.parse().ok(),
  }
}

#[test]
fn pf_builder_alternative() {
  assert!(pf("hello").is_some()); // isDefinedAt("hello")
  assert!(pf("42").is_some()); // isDefinedAt("42")
  assert_eq!(pf("42"), Some(42)); // apply("42") == 42
}

// 代替2（shouldPassBasicMatchTest）：先に書いた腕が優先され、どれにも当たらなければ None。
fn basic(m: &Message) -> Option<f64> {
  if let Some(i) = m.downcast_ref::<i32>() {
    Some(f64::from(*i) * 10.0)
  } else if let Some(d) = m.downcast_ref::<f64>() {
    Some(*d * -10.0)
  } else {
    None
  }
}

#[test]
fn basic_match_alternative() {
  assert_eq!(basic(&Message::new(4711_i32)), Some(47110.0));
  assert_eq!(basic(&Message::new(4711.0_f64)), Some(-47110.0));
  assert_eq!(basic(&Message::new("4711")), None); // MatchError
}

struct GenericClass<T> {
  val: T,
}

// 代替3（shouldHandleMatchOnGenericClass）：型引数つきの型を検査つきで取り出す。
#[test]
fn generic_class_alternative() {
  let m = Message::new(GenericClass { val: String::from("A") });
  assert_eq!(m.downcast_ref::<GenericClass<String>>().map(|g| g.val.clone()), Some(String::from("A")));
}

// 代替4（shouldHandleMatchWithPredicateOnGenericClass）：述語は Option::filter（または match のガード）。
#[test]
fn generic_class_with_predicate_alternative() {
  let m = Message::new(GenericClass { val: String::new() });
  assert_eq!(m.downcast_ref::<GenericClass<String>>().filter(|g| !g.val.is_empty()).map(|g| g.val.clone()), None);
}
```

原典の `Integer.valueOf` は数字でない文字列で例外を投げるが、原典のケースは `"hello"` を先の case で受けるので、その経路を観測しない。代替1は `parse().ok()` で `None` にしている。

## 台帳と集計の変更

- 区分の定義・当てはめる条件・自動では入れないもの・混在の扱いを、[`ledger-schema-draft.md` の「作らない区分」](../decisions/ledger-schema-draft.md)に書いた。`ledger/README.md` にも要約と参照を足した。
- 2ノードを `status: not_built`、`triage.kind: not_built` にし、`triage.reason` に理由と本書の場所を書いた。`cases` は `total` に原典の件数、`ported` と `passing` に0、`cases.not_built` にケースごとの `name`・`reason`・`alternative`・`approved_by`・`approved_at` を書いた。`approved_by` と `approved_at` は、オーナーの決定（元帥の項目223）を写したもので、担当が承認したものではない。`rust` の欄は null のままである。`goal` は `G0issue145`。
- `rollup.py` は、`not_built` のノード数、作らない率（B / N）、ケースの 移植・除外・作らない の件数を出す。合格率は G / (N − X − B) にした（作らない区分は「理由を台帳に書いて対象外にする」決定による）。`not_built` のノードの記録を検査し、欠けていればゴールの完了に数えない。
- `check_needs.py` の「完了したゴール」は、`rollup.py` の同じ判定（`is_done`）を使う。
- `goal_complete_gate_test.py` に2件を足した。green と記録の揃った not_built の混在は完了、承認の欄が欠けた not_built は未完了。main の `rollup.py` では前者が失敗することを確かめた（`26 passed, 2 failed`。もう1件は本書を足した後の実物のゴール文書の検査）。
- `verify_red.py`・`verify_green.py` の勘定には手を入れていない。混在のノードはまだない。

集計の変化（`rollup.py --ledger ledger/nodes.yaml` の全体の行）：

| | main `8c8d455` | 本 PR |
|:---|:---|:---|
| ノード | N=1261、green 49、do_not_port 173、not_built 0 | N=1261、green 49、do_not_port 173、not_built 2 |
| 合格率 | 49 / 1088 = 4.5% | 49 / 1086 = 4.5% |
| 作らない率 | 0.0% | 0.2% |
| ケース | 移植 260、除外 0、作らない 0 | 移植 260、除外 0、作らない 4 |

## 検証

本書のコミットの HEAD で、担当が次を実行した。

| コマンド | 終了値 | 結果 |
|:---|---:|:---|
| `rollup.py --goal G0issue145` | 0 | 2件、`{'not_built': 2}`、完了 |
| `check_needs.py --goal G0issue145` | 0 | needs 閉性 OK |
| `check_needs.py` と `rollup.py` を `G0a`〜`G0f` の各々で | 0 | 12コマンドすべて終了値0 |
| `goal_complete_gate_test.py` | 0 | 28 passed, 0 failed |
| `verify_red_test.py` | 0 | 48 passed, 0 failed |
| `verify_green_test.py`、`verify_green_test_aislop.py`、`verify_green_test_crate.py` | 0 | 成功 |
| `npm ci` → `npm run check:aislop` | 0 | warn 以上の指摘は受理済み一覧と一致 |
| `cargo test --workspace` | 0 | 75 スイート、848 passed / 0 failed。製品とテストの Rust コードは変えていない。回帰の確認として実行 |
| `cargo fmt --check` | 0 | 差分なし |

## 独立レビュー（別系統のモデル）

PR 作成後、既出の指摘に対応した HEAD を固定して行う（`docs/decisions/takt-workflow-design.md` §2.6）。結果はここに追記する。

## 再現手順

```text
uv run --with pyyaml python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal G0issue145
uv run --with pyyaml python3 ledger/tools/check_needs.py --ledger ledger/nodes.yaml --capabilities ledger/capabilities.yaml --goal G0issue145
uv run --with pyyaml python3 ledger/tools/goal_complete_gate_test.py
uv run --with pyyaml python3 ledger/tools/verify_red_test.py
cargo test --workspace

# 先行テストの red（隔離枝）
git checkout 20fffb68592028789b48735716d100ce54538538
cargo test -p fraktor-actor --test pf_builder_test -- --test-threads=1
cargo test -p fraktor-actor --test match_builder_test -- --test-threads=1
```

代替の例を確かめるには、`fraktor-actor` を path 依存にした一時クレートの `tests/alternatives.rs` に上のコードを置き、`cargo test --test alternatives` を実行する。
