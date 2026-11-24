# ゴール G0issue148：ByteStringBuilder 経由の mark 対応（意図した差の固定）

[Issue #148](https://github.com/j5ik2o/fraktor-ng-rs/issues/148)（Orca 項目227）は、`ByteStringBuilder` の断片の分け方が Pekko v1.7.1 と異なり、[#147](https://github.com/j5ik2o/fraktor-ng-rs/issues/147)（[PR #149](https://github.com/j5ik2o/fraktor-ng-rs/pull/149)）で入れた入力ストリームの mark 対応にその差が出る件である。

副元帥の決定（項目227）は、builder を原典に合わせず、意図した差として補足回帰で固定することである。製品コードの断片化と動作は変えない。

## 原典との差

- 原典の実装：[ByteString.scala](https://github.com/apache/pekko/blob/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/actor/src/main/scala-2.13/org/apache/pekko/util/ByteString.scala)
  - `addAll(xs: IterableOnce[Byte])`（L1354–1378）：`WrappedArray.ofByte` は `putByteArrayUnsafe`（L1380–1385）で、大きな列は `shouldResizeTempFor` の分岐で、それぞれを独立した `ByteString1` の断片にする。
  - `result()`（L1566–1575）：断片が2つ以上なら `ByteStrings` を返す。
  - `ByteStrings.asInputStream`（L637–638）：`SequenceInputStream` なので `markSupported` は false。
- 移植の実装：`ByteStringBuilder::add_all` は、足したバイトをすべて1つの pending にまとめ、`result()` で1つの配列にする（`crates/fraktor-actor/src/kernel/byte_string/byte_string_builder.rs`）。`as_input_stream` は配列が1つ以下なら mark に対応する。

| 入力 | 原典の結果 | 原典の mark 対応 | 移植の結果 | 移植の mark 対応 |
|:---|:---|:---|:---|:---|
| `++= Array(1, 2)` の後に `++= Array(3)` | 2断片の `ByteStrings` | 非対応 | 1配列 | 対応 |

差は「原典が mark 非対応のものを、移植が対応と答える」の一方向だけである。移植の配列数は原典の断片数以下になる。バイト列の内容・順序・等価性は変わらない。

## 補足回帰

`crates/fraktor-actor/src/kernel/byte_string/byte_string_input_stream_test.rs` の単体テスト `separately_added_arrays_build_one_array_whose_stream_resets_to_the_mark`。テスト内のコメントに、原典との意図した差であることと #148 を書いた。

1. `add_all([1, 2])` と `add_all([3])` を別々に足すと、`result()` の配列は1つになる。
2. その入力ストリームの `mark_supported()` は true。
3. 1バイト読んだ後に `mark` し、EOF まで読み進める（2、3、`None`）。
4. `reset()` は `Ok` で、同じバイト列を順番どおり読み戻せる（2、3、`None`）。

#147 の `ByteStringInputStream`（`mark`、`reset`、`mark_supported`）と、`ByteString::arrays`（`pub(super)`）を再利用した。#147 の既存テスト `a_built_byte_string_supports_marks_by_how_many_arrays_it_holds` は mark 対応の真偽だけを見ていた。新しいテストは、1配列になること、mark 後に読み進めた位置から reset で戻ることまでを固定する。

### already_green の根拠

製品コードは変えていない（`git diff 0bbb9e6 -- 'crates/*/src/**/*.rs' ':!*_test.rs'` は空）。テストは既存の実装のまま最初から通過した。既存の契約が要件を満たしていることを示す。

テストが差を検出できることも確かめた。`add_all` の先頭で pending を確定する（足すたびに別の配列にする、原典と同じ断片化）一時的な変更を入れると、`assert_eq!(built.arrays().len(), 1)` が `left: 2, right: 1` で失敗した。変更は戻し、コミットしていない。

## 台帳

台帳 `ledger/nodes.yaml` は変えていない（調整担当の決定。スキーマに備考欄はなく、`triage.reason` は do_not_port と already_green の用途に限る）。差の記録先は本書である。

| ノード | ゴール | status | 原典 | 移植 | 成功 | 除外 |
|:---|:---|:---|---:|---:|---:|---:|
| `actor-tests/src/test/scala/org/apache/pekko/io/UnsynchronizedByteArrayInputStreamSpec.scala` | G0issue147 | green | 3 | 3 | 3 | 0 |
| `actor-tests/src/test/scala/org/apache/pekko/util/ByteStringBuilderSpec.scala` | G0issue133 | green | 3 | 3 | 3 | 0 |

件数と除外を据え置く根拠：

- 補足回帰は原典にないケースである。台帳の `cases` は原典のケースだけを数えるので、原典・移植・成功の件数に足さない。
- 原典 `UnsynchronizedByteArrayInputStreamSpec` の3ケースは `"abc"` の1配列の入力ストリームだけを観測し、builder を使わない。原典 `ByteStringBuilderSpec` の3ケースは入力ストリームを使わない。差はどちらの観測にも現れないので、除外は要らない（0件のまま）。
- 製品コードを変えていないので、両ノードの統合テストの結果は変わらない（下の実測）。

## 実測

HEAD は本書のコミット。base は `0bbb9e6`（main）。

| コマンド | 終了値 | 結果 |
|:---|---:|:---|
| `cargo test -p fraktor-actor --lib byte_string_input_stream` | 0 | 16 passed（既存15、新規1） |
| `cargo test -p fraktor-actor --lib byte_` | 0 | 38 passed |
| `cargo test -p fraktor-actor --test unsynchronized_byte_array_input_stream_spec --test byte_string_builder_spec --test byte_string_builder_scala213_plus_spec --test byte_string_test --test byte_string_input_stream_spec` | 0 | 3、3、3、2、3 passed |
| `cargo test --workspace` | 0 | 69 スイート、697 passed / 0 failed / 0 ignored |
| `cargo fmt --check` | 0 | 差分なし |
| `cargo clippy --workspace --all-targets -- -D warnings` | 0 | 警告なし |
| syn-check（`crates/fraktor-actor`、`crates/fraktor-testkit`） | 0 | 両方 `clean: true`、違反0 |
| `npm ci` → `npm run check:aislop` | 0 | warn 以上の指摘は受理済み一覧と一致 |
| `goal_complete_gate_test.py`、`verify_red_test.py`、`verify_green_test.py`、`verify_green_test_aislop.py`、`verify_green_test_crate.py` | 0 | 成功 |
| `verify_green.py --crate fraktor-actor` | 0 | `all_pass: true` |

台帳は、`goal:` を持つ29ゴール（G0a〜G0f、G0issue96〜G0issue150）の各々で `check_needs.py` と `rollup.py` を実行し、58コマンドすべて終了値0だった。G0issue147 は1件で `{'green': 1}`、G0issue133 は3件で `{'green': 3}`。

## 実行経路

TAKT を使わず、担当（Claude Opus 5.5）が直接実装した。TAKT にはゴールもタスクも投入していない。台帳にゴール ID `G0issue148` のタグは付けていない（対象ノードの追加はない）。

## 再現手順

```text
cargo test -p fraktor-actor --lib separately_added_arrays_build_one_array_whose_stream_resets_to_the_mark
cargo test -p fraktor-actor --test unsynchronized_byte_array_input_stream_spec --test byte_string_builder_spec
uv run --with pyyaml python3 ledger/tools/check_needs.py --ledger ledger/nodes.yaml --capabilities ledger/capabilities.yaml --goal G0issue147
uv run --with pyyaml python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal G0issue147
uv run --with pyyaml python3 ledger/tools/check_needs.py --ledger ledger/nodes.yaml --capabilities ledger/capabilities.yaml --goal G0issue133
uv run --with pyyaml python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal G0issue133
```

## 独立レビュー

PR 作成後、既出の指摘に対応した HEAD を固定して、調整担当が別系統のモデルで1回行う（`docs/decisions/takt-workflow-design.md` §2.6）。
