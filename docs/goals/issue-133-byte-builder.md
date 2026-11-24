# ゴール G0issue133：classic ByteStringBuilder の2スイートと Java ByteStringTest

[Issue #133](https://github.com/j5ik2o/fraktor-ng-rs/issues/133) と [Issue #134](https://github.com/j5ik2o/fraktor-ng-rs/issues/134)（Orca 項目180）の対象は、Pekko v1.7.1 の次の3つのテストである。どれも `ByteString` のビルダーと生成を確かめる。3つのノードを1つのゴールにまとめた。

- `ByteStringBuilderSpec`: [原典のテスト](https://github.com/apache/pekko/blob/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/actor-tests/src/test/scala/org/apache/pekko/util/ByteStringBuilderSpec.scala)。LinearSeq、IndexedSeq、ByteString を `++=` で追加する3ケース
- `ByteStringBuilderScala213PlusSpec`: [原典のテスト](https://github.com/apache/pekko/blob/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/actor-tests/src/test/scala-2.13%2B/org/apache/pekko/util/ByteStringBuilderScala213PlusSpec.scala)。同じ3種類を `addAll` で追加する3ケース
- `ByteStringTest`（Java）: [原典のテスト](https://github.com/apache/pekko/blob/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/actor-tests/src/test/java/org/apache/pekko/util/ByteStringTest.java)。`testCreation` と `testBuilderCreation` の2ケース
- 原典の実装: [ByteString.scala](https://github.com/apache/pekko/blob/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/actor/src/main/scala-2.13/org/apache/pekko/util/ByteString.scala)（`fromInts` L125–126、`newBuilder` L158、`createBuilder` L161、`ByteStringBuilder` L1264 以降）

## 対象ノード

| id | Rust の統合テスト | 原典 | 移植 | 成功 | 除外 |
|:---|:---|---:|---:|---:|---:|
| `actor-tests/src/test/scala/org/apache/pekko/util/ByteStringBuilderSpec.scala` | `crates/fraktor-actor/tests/byte_string_builder_spec.rs` | 3 | 3 | 3 | 0 |
| `actor-tests/src/test/scala/org/apache/pekko/util/ByteStringBuilderScala213PlusSpec.scala` | `crates/fraktor-actor/tests/byte_string_builder_scala213_plus_spec.rs` | 3 | 3 | 3 | 0 |
| `actor-tests/src/test/java/org/apache/pekko/util/ByteStringTest.java` | `crates/fraktor-actor/tests/byte_string_test.rs` | 2 | 2 | 2 | 0 |

2026-11-25 追記：Pekko v2.0.0 で `ByteStringBuilderScala213PlusSpec` が `actor-tests/src/test/scala-2.13+/` から `actor-tests/src/test/scala/` に移った（上流 `f2b677dfa0` #1986）。そのため、台帳の id を v2.0.0 のパスに付け替え、上の表の id も合わせた。旧 id は `actor-tests/src/test/scala-2.13+/org/apache/pekko/util/ByteStringBuilderScala213PlusSpec.scala` である。v2.0.0 ではケースが 3 件から 5 件に増えた（上流 `fef71efd85` #3539）。上の表の件数は、このゴールを閉じた時点（v1.7.1）の記録である。照合の記録は `docs/baseline/pekko-v2.0.0-migration/batch-1.md` にある。

合計は原典8件・移植8件・成功8件・除外0件。`needs` はどれも空で、TestKit の機能は使わない。Scala の2ファイルの原典件数は `" in {"` の数で数えた（`verify_red.py` の `count_original_cases` は WordSpec の `"..." should {` を数えきれず `None` を返す）。Java は `@Test` の2件で、`verify_red.py` も2件と数えた。`ignore` と `pending` はない。

## 実行経路

正規の TAKT で2工程を直列に実行した。工程は2つとも同じゴールに属する。

1. 開始点のブランチ `j5ik2o/issue-133-134-byte-builder` で、台帳の3ノードの `goal:` 行だけを `G0issue133` にした（`0f7c2b3`、`ledger/README.md` のゴール発行手順）。
2. 担当の端末で takt-mcp（`~/local/takt-custom/bin/takt-mcp`、serverInfo takt 0.76.1）を stdio で起動した。cwd は本体のチェックアウトである。#133・#134 を対象とするゴールとタスクが既存のもの（オープン・完了済みのゴール全件、タスク全127件）にないことを確かめた。そのうえでゴール `8e9c4741-861d-4133-b2de-49f78f969a66`（ブランチ `takt/20261124T1141-goal-8e9c4741`、統合先 main）を作った。ワークフローは `simple` で、子のワークフローは review-remediation である。管理ランタイムの opencode は `inspectProviderInstallation` で ready だった。
3. 第1工程（workKey `issue-133-134/01-source-red`）はテストの移植と red の実測である。公開 API の名前を投入文で固定した、手順固定の作業なので、`runtimeAssignment` に `spark-low`（DGX、`dgx-flash`）を指定した。2026-11-24T11:42:30Z に開始し、12:04:46Z に完了した。1回目のレビューは、`from_ints` を実装する工程が決まっていないことを理由に REJECT した。担当が `takt_tell_run` で「`from_ints` は第2工程の対象で、各 `i32` の下位8ビットを取る」と伝え、2回目のレビューで通過した。成果 `23e1060` を `takt_merge_goal_task` でゴールブランチへ統合した（`ec2d0d4`）。
4. 第2工程（workKey `issue-133-134/02-implement-green`）は実装、green、回帰の検証である。判断を要する工程なので、既定の割り当て（`opus-standard`、Claude Opus 5.5）を使った。12:06:40Z に開始した。1回目の実装は、統合テスト2本の `LinkedList::from([b'a'])` が `clippy::byte_char_slices` に当たり、テストを変えない指示との間で止まって再計画に戻った。担当が `takt_tell_run` で、その2行を clippy の提案どおり `LinkedList::from(*b"a")` に変えることと、ヘッダーコメントの誤記（`assertEquals` を `shouldEqual` に）を直すことだけを許可した。lint の抑制は許可していない。2回目でレビューを通過し、成果 `0cf9eb8` をゴールブランチへ統合した（`bd18ea0`）。

工程は直列で、並列実行はしていない。TAKT の同時実行上限は変えていない。

| 段階 | コミット | 内容 |
|:---|:---|:---|
| タグ付け | `0f7c2b3` | 3ノードの `goal: G0issue133`（台帳3行） |
| テストと台帳 red（第1工程） | `23e1060` | 移植テスト3本、3ノードの status `red` と rust の欄。製品差分0 |
| 実装と台帳 green（第2工程） | `0cf9eb8` | kernel の `ByteStringBuilder`、`from_ints`、`new_builder`/`create_builder`、`PartialEq`/`Eq`、単体テスト、3ノードの green。テスト2本の lint 対応 |
| ゴール文書 | 本書のコミット | 本書 |

## 原典との照合

### API の対応

| 原典 | 移植 |
|:---|:---|
| `ByteString.newBuilder`（Scala） | `ByteString::new_builder()` |
| `ByteString.createBuilder()`（Java） | `ByteString::create_builder()` |
| `ByteStringBuilder` | `fraktor_actor::kernel::byte_string::ByteStringBuilder` |
| `++=(xs: IterableOnce[Byte])`、`addAll(xs: IterableOnce[Byte])` | `add_all<I: IntoIterator<Item = u8>>(&mut self, bytes) -> &mut Self` |
| `++=(bytes: ByteString)`、`addAll(bytes: ByteString)`、Java の `append(bs)` | `append(&mut self, &ByteString) -> &mut Self` |
| `result()` | `result(&self) -> ByteString` |
| `ByteString("a")`、`ByteString.fromString(s)` | `ByteString::from_string(s)`（#132 で移植済み） |
| `ByteString.fromInts(1, 2, 3)` | `ByteString::from_ints(&[1, 2, 3])` |
| `shouldEqual` / `assertEquals` | `assert_eq!`（`ByteString: PartialEq + Debug`） |
| `List[Byte]('a')`（LinearSeq） | `LinkedList::from(*b"a")` |
| `Vector[Byte]('a')`（IndexedSeq） | `vec![b'a']` |

Scala 2.13 の `Growable` では `++=` は `addAll` の別名である（`final def ++=(xs) = addAll(xs)`）。`ByteStringBuilder` はさらに、ByteString 専用のオーバーロード `++=(bytes: ByteString) = addAll(bytes)`（L1335–1336）を持つ。原典の呼び出しは、引数の静的な型でオーバーロードが決まる。`List` と `Vector` は `IterableOnce[Byte]` 版に、`ByteString("a")` は ByteString 版に解決される。Rust にはオーバーロードがないため、`IterableOnce` 版を `add_all` に、ByteString 版を `append` に対応させた。Java の `append(bs)` も原典では `this ++= bs`（L1390）なので、同じ `append` に対応する。

### 両原典のケース対応（同じ操作の共有）

| 操作 | `ByteStringBuilderSpec` | `ByteStringBuilderScala213PlusSpec` | `ByteStringTest` |
|:---|:---|:---|:---|
| `add_all` に LinkedList | 「handle ++= with LinearSeq」→ `handle_plus_plus_eq_with_linear_seq` | 「handle addAll with LinearSeq」→ `handle_add_all_with_linear_seq` | — |
| `add_all` に Vec | 「handle ++= with IndexedSeq」→ `handle_plus_plus_eq_with_indexed_seq` | 「handle addAll with IndexedSeq」→ `handle_add_all_with_indexed_seq` | — |
| `append` に ByteString | 「handle ++= with ByteString」→ `handle_plus_plus_eq_with_byte_string` | 「handle addAll with ByteString」→ `handle_add_all_with_byte_string` | `testBuilderCreation` → `test_builder_creation`（3回の `append`） |
| `from_string` と `from_ints` | — | — | `testCreation` → `test_creation` |

Scala の2つのスイートは、原典でも同じ操作を別名で呼ぶだけである。そのため移植では、2つのファイルの各ケースが同じ Rust の呼び出しになる。ケースは原典どおり8件とも独立したテストとして残した。各テストの式は原典と同じく1つの連鎖で書いた（`new_builder().add_all(...).result()`）。`test_creation` は原典と同じく2つを作って束縛するだけで、アサーションはない。束縛した値を `let _ = (&empty, &ints);` で使っているのは、未使用変数の警告を避けるためだけである。

## 実装

kernel の `byte_string` モジュールに足した。どれも `core` と `alloc` だけを使う（syn-check の `cfg-std-forbid` は違反0）。#132 の `ByteString` の既存の公開 API（`empty`、`from_string`、`from_array`、`concat`、`iterator`、`as_input_stream`、`Clone`/`Debug`/`Default`）は変えていない。足したのは次の API だけである。

- `ByteStringBuilder`（`byte_string_builder.rs`）：確定した共有配列の列と、`add_all` で溜めた未確定のバイトを持つ。`append` は先に未確定のバイトを1つの配列として確定させてから、渡された `ByteString` の配列を共有したまま末尾に積む（原典が `clearTemp()` のあと `ByteString1` をそのまま積むのと同じ）。空の `ByteString` を渡しても何も変わらない（原典の `if (bytes.nonEmpty)`）。`result` はビルダーを変えずに、それまでに追加したバイトを追加順に並べた `ByteString` を返す。返した値は、その後に追加しても変わらない。
- `ByteString::from_ints(&[i32])`：各値の下位8ビットを並べる。原典の `fromInts(array: Int*)`（L125–126）は `CompactByteString.apply[T](bytes: T*)(implicit num: Integral[T])` を呼び、各値を `num.toInt(x).toByte` で `Byte` にする（L1196–1198）。`Int` の `toByte` は下位8ビットを取り、Rust の `as u8` は同じビット列を取る。
- `ByteString::new_builder()`、`ByteString::create_builder()`：空のビルダーを返す。
- `PartialEq`/`Eq`：バイトの列として比べる。内部の配列の分け方は結果に影響しない。原典の `ByteString` の等価性も、表現（`ByteString1C`、`ByteString1`、`ByteStrings`）ではなく要素の列で決まる。
- クレート内向けの `ByteString::from_arrays` と `arrays()`：ビルダーが配列をコピーせずに受け渡すために使う。

## 等価性レビュー

### 実装者自身のレビュー

実施者は本 issue の担当（Claude Opus 5.5）。Pekko 固定 SHA の原典テスト3本と `ByteString.scala` を、TAKT が生成した移植の実装・テストと突き合わせた。判定は「等価」で、原典8ケースは観測上等価、除外0件である。

| 原典の振る舞い | 原典の根拠 | 移植で確かめているテスト |
|:---|:---|:---|
| `IterableOnce` を追加すると、そのバイトが順に末尾へ加わる（LinearSeq は `super.addAll`、IndexedSeq は一時配列へのコピー。どちらも観測は同じ） | `ByteString.scala` L1354–1381 | 統合テストの LinearSeq・IndexedSeq の4件、単体テスト `add_all_and_append_keep_the_order_they_are_called_in` |
| ByteString を追加すると、未確定のバイトを確定させてから、その配列を順に積む | L1335–1351 | 統合テストの ByteString の2件と `test_builder_creation`、単体テスト `add_all_and_append_keep_the_order_they_are_called_in` |
| 空の ByteString の追加は何も変えない | L1337、L1390 | 単体テスト `appending_an_empty_byte_string_changes_nothing` |
| 何も追加しなければ `result()` は `ByteString.empty` | L1566–1567 | 単体テスト `an_empty_builder_results_in_the_empty_byte_string` |
| `result()` の後もビルダーを使い続けられ、先に返した結果は変わらない | L1566–1575（`clearTemp` は確定させるだけ） | 単体テスト `a_result_can_be_followed_by_more_bytes` |
| `fromInts` は各 `Int` を `num.toInt(x).toByte` で `Byte` にする（下位8ビット） | L125–126、L1196–1198 | `test_creation`、単体テスト `from_ints_keeps_the_low_byte_of_each_value`（`256`→`0`、`-1`→`0xff`） |
| 等価性はバイトの列で決まる | `ByteString` は `IndexedSeq[Byte]` の等価性を使う | 統合テストの8件中7件の `assert_eq!`、単体テスト `byte_strings_with_the_same_bytes_are_equal_however_they_are_split` |

原典との差異は次のとおりで、原典テストが観測する値は変わらない。

- 原典はオーバーロードを引数の静的な型で選ぶ。移植は ByteString 版を `append` という別名にした。前節の「API の対応」のとおりである。
- 原典の `IterableOnce` 版は、実行時の型が `ByteString` なら ByteString 版へ回す（L1356）。移植では `add_all` に `ByteString` の反復子を渡すと、配列を共有せずにバイトをコピーする。結果のバイト列は同じで、違うのは内部の共有の有無だけである。原典テストはこの経路を使わない。
- 原典の `ByteStringBuilder` は `Builder[Byte, ByteString]` で、`putShort` などの数値の書き込み、`sizeHint`、`clear`、`length`、`asOutputStream` などを持つ。どれもこのゴールの3ノードのテストが使わないため、移植していない。issue の受け入れ条件（ByteString 全体は対象外）のとおりである。
- `Int` は `i32` にした。`from_ints` の可変長引数は `&[i32]` で受ける。
- 原典の `ByteString` は `hashCode` も要素の列から計算する。移植は `Hash` を実装していない。このゴールのテストは使わない。

### 独立レビュー（別系統のモデル）

- 実施者: 実装者とは別系統の、読み取り専用のレビュー担当。モデルは Codex（OpenAI GPT-6.2 Sol、`gpt-6.2-sol`）で、調整担当が手配した。実装（第2工程）と自己レビューがどちらも Claude Opus 5.5 なので、`docs/decisions/takt-workflow-design.md` §2.6 に従って別系統のレビューを足した。
- 対象: PR #135 の固定 HEAD `d272ecc8b23aa8f9a3ff425bc74d93bba2559233`。比較元は `8cecd4783b864f7e19f6ff0a1ba6dfd96985658a`。原典は Pekko 固定 SHA `9a54d1294c3ac7821bbed3c940769c3d5027d1ff`。記録は [PR のコメント](https://github.com/j5ik2o/fraktor-ng-rs/pull/135#issuecomment-7132355633)にある。
- 判定: APPROVE。原典8件（Scala 3＋3、Java 2）の入力の種類、追加順、生成結果、等値比較は等価で、移植テストの弱体化も既存公開 API の回帰もない。除外は0件である。固定 HEAD の `cargo test --locked --workspace` は 494 件成功・失敗0・無視0。製品変更前の `23e10601` で、対象3スイートのコンパイル失敗を再現した。
- 文書への補足1件と対応状況: 下位8ビットへの変換の根拠として、原典の実式 `num.toInt(x).toByte`（`ByteString.scala` L1196–1198）を参照できる、という指摘。本書の「実装」節と自己レビューの表に追記した。テストと実装のコードは変えていない。

## 製品編集前の実測

第1工程の成果 `23e10601d531a42f24a9a53e2df6698e77b9ce86`（`0f7c2b3` からの製品差分0。`git diff --stat 0f7c2b3 23e1060 -- 'crates/*/src'` は空）で、担当が自分の作業ツリーで次を再実行した。その時点のテストの blob は、`byte_string_builder_spec.rs` が `5338eea2`、`byte_string_builder_scala213_plus_spec.rs` が `c1dbf1d0`、`byte_string_test.rs` が `e3ddbb8a` である。

| コマンド | 終了値 | 結果 |
|:---|---:|:---|
| `cargo test -p fraktor-actor --test byte_string_builder_spec -- --test-threads=1` | 101 | compile-red。`E0599 no associated function new_builder` が3件。宣言3件、実行0件、除外0件 |
| `cargo test -p fraktor-actor --test byte_string_builder_scala213_plus_spec -- --test-threads=1` | 101 | compile-red。`E0599 new_builder` が3件。宣言3件、実行0件、除外0件 |
| `cargo test -p fraktor-actor --test byte_string_test -- --test-threads=1` | 101 | compile-red。`E0599 from_ints`（`tests/byte_string_test.rs:31`）と `E0599 create_builder`（L38）の2件。宣言2件、実行0件、除外0件 |
| `verify_red.py`（ノードごと。原典を `PEKKO_ORIGINAL_ROOT` に置いて実行） | 0 | 3ノードとも `expected_compile_error`、`excluded: 0`。台帳はすでに red で、書き換えなし |

already_green ではない。製品にビルダーと `from_ints` と等価比較がなく、3本ともコンパイルに失敗した。

第2工程で、テストを2か所だけ変えた。`byte_string_builder_spec.rs` は L6 のコメントと L32、`byte_string_builder_scala213_plus_spec.rs` は L6 のコメントと L36 である。L32 と L36 の `LinkedList::from([b'a'])` を `LinkedList::from(*b"a")` に変えた。どちらも同じ `[u8; 1]` を渡す式で、LinkedList（LinearSeq）を使う点も変わらない。`clippy::byte_char_slices` を抑制せずに満たすための変更である。コメントは `assertEquals` を、原典どおりの `shouldEqual` に直した。変更後の blob は、それぞれ `a8f6164f` と `02542715` になった。`byte_string_test.rs` は変えていない。

## 実装後の実測

ゴールブランチへの統合 `bd18ea05d78abce0c1fd438986393059a9ae748e` で、担当が自分の作業ツリーで次を実行した。

| コマンド | 終了値 | 結果 |
|:---|---:|:---|
| `cargo test -p fraktor-actor --test byte_string_builder_spec -- --test-threads=1` | 0 | 3 passed / 0 failed / 0 ignored |
| `cargo test -p fraktor-actor --test byte_string_builder_scala213_plus_spec -- --test-threads=1` | 0 | 3 passed / 0 failed / 0 ignored |
| `cargo test -p fraktor-actor --test byte_string_test -- --test-threads=1` | 0 | 2 passed / 0 failed / 0 ignored |
| `cargo test -p fraktor-actor --lib byte_` | 0 | 22 passed（うち `ByteStringBuilder` 4件、`ByteString` 7件） |
| `cargo test --workspace` | 0 | 60 スイート、494 passed / 0 failed / 0 ignored |
| `cargo fmt --check` | 0 | 差分なし |
| `cargo clippy --workspace --all-targets -- -D warnings` | 0 | 警告なし |
| `cargo run --quiet --release --locked --manifest-path tools/syn-check/Cargo.toml -- --format json crates/fraktor-actor` | 0 | `clean: true`、166 files、違反0、抑制0 |
| `npm ci` → `npm run check:aislop` | 0 | warn 以上の指摘は受理済み一覧と一致 |
| `verify_green.py --crate fraktor-actor` | 0 | `all_pass: true` |

台帳は `G0issue133` と `G0a`〜`G0f` の各々で `check_needs.py` と `rollup.py` を実行し、14コマンドすべて終了値0だった（needs の閉性は OK で、対象ノードはすべて green。`G0issue133` は3件で `{'green': 3}`）。

## 再現手順

```text
cargo test -p fraktor-actor --test byte_string_builder_spec -- --test-threads=1
cargo test -p fraktor-actor --test byte_string_builder_scala213_plus_spec -- --test-threads=1
cargo test -p fraktor-actor --test byte_string_test -- --test-threads=1
cargo test -p fraktor-actor --lib byte_
cargo test --workspace
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
uv run --with pyyaml python3 ledger/tools/check_needs.py --ledger ledger/nodes.yaml --capabilities ledger/capabilities.yaml --goal G0issue133
uv run --with pyyaml python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal G0issue133
```
