# ゴール G0issue147：classic UnsynchronizedByteArrayInputStreamSpec

[Issue #147](https://github.com/j5ik2o/fraktor-ng-rs/issues/147)（Orca 項目225）の対象は、Pekko v1.7.1 の classic `UnsynchronizedByteArrayInputStreamSpec` である。「UnsynchronizedByteArrayInputStream must」の下に3ケースあり、mark と reset、1バイトの skip、残量を超える skip と EOF を確かめる。

- [原典のテスト](https://github.com/apache/pekko/blob/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/actor-tests/src/test/scala/org/apache/pekko/io/UnsynchronizedByteArrayInputStreamSpec.scala)
- [原典の実装](https://github.com/apache/pekko/blob/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/actor/src/main/java/org/apache/pekko/io/UnsynchronizedByteArrayInputStream.java)
- 原典の利用元: [ByteString.scala](https://github.com/apache/pekko/blob/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/actor/src/main/scala-2.13/org/apache/pekko/util/ByteString.scala) の `asInputStream`（L308、L505–506、L637–638）

## 対象ノード

| id | Rust の統合テスト | 原典 | 移植 | 成功 | 除外 |
|:---|:---|---:|---:|---:|---:|
| `actor-tests/src/test/scala/org/apache/pekko/io/UnsynchronizedByteArrayInputStreamSpec.scala` | `crates/fraktor-actor/tests/unsynchronized_byte_array_input_stream_spec.rs` | 3 | 3 | 3 | 0 |

原典のケース数は、原典ファイルの `" in {"` の数で数えた（3件）。`ignore` と `pending` はない。`needs` は空で、TestKit の機能は使わない。台帳の件数は原典の3ケースだけを数える。後述の補足の単体テスト12件は台帳に数えない。

## 実行経路

TAKT を使わず、担当（Claude Opus 5.5）が直接実装した。TAKT にはゴールもタスクも投入していない。起動仕様が、共有 TAKT の保留の有無にかかわらず直接実装と決めていた。

| 段階 | コミット | 内容 |
|:---|:---|:---|
| タグ付け | `718a713` | ノードの `goal: G0issue147`（台帳1行） |
| テストと台帳 red | `1357099` | 移植テスト1本、status `red` と rust の欄。製品差分0 |
| テストの修正（red のまま） | `d286b23` | `reset()` の戻り値を `Result` にする決定を受けて、テストの1行を `.expect(..)` にした。製品差分0 |
| 実装と台帳 green | 本書のコミット | 既存の `ByteStringInputStream` に4メソッド、`MarkNotSupportedError`、`ByteIterator::skip_bytes`、単体テスト12件、台帳 green、本書 |

### 着手時の調査と、調整担当の決定

G0issue130 で入れた `ByteStringInputStream` の公開 API は `read` と `read_into` だけだった。mark・reset・skip・markSupported がないので、既存の API だけでは原典3ケースを確かめられなかった。公開 API の追加が要るため、着手前に調整担当へ報告した。副元帥の決定は次のとおり（2026-11-25）。

- 新しい型は作らず、既存の `ByteStringInputStream` に `mark_supported`・`mark`・`reset`・`skip` を足す。
- mark への対応は、原典と同じく表現で分ける。判定には配列の数を使う。連結は mark に対応せず、`mark` は何もせず、`reset` はエラーを返す。
- `skip` は `usize` を受け取り、残量で頭打ちにする。EOF の後の skip は0を返す。原典との差は記録し、境界テストで固定する。
- 既存の `ByteStringBuilder` には手を入れない。builder の断片の差は意図した差として記録し、[#148](https://github.com/j5ik2o/fraktor-ng-rs/issues/148) で追跡する。

## 原典との照合

### 原典のテストと実装の関係

原典のテストは `new UnsynchronizedByteArrayInputStream("abc".getBytes(UTF_8))` で入力ストリームを作る。Pekko の `ByteString1C.asInputStream` は、自分のバイト列でまさにこの入力ストリームを作る（ByteString.scala L308）。そこで移植では、`ByteString::from_string("abc").as_input_stream()` で作る。G0issue130 の対応表のとおり、`from_string` は UTF-8 でエンコードし、1つの配列を持つ（原典の `ByteString1C`）。

### API の対応

| 原典 | 移植 |
|:---|:---|
| `new UnsynchronizedByteArrayInputStream(bytes)` | `ByteString::from_string(..).as_input_stream()`（`ByteStringInputStream`） |
| `markSupported()` | `mark_supported() -> bool` |
| `mark(readAheadLimit: Int)` | `mark(read_ahead_limit: usize)`。引数は原典と同じく使わない |
| `reset()`（非対応なら `IOException`） | `reset() -> Result<(), MarkNotSupportedError>` |
| `skip(n: Long): Long` | `skip(count: usize) -> usize` |
| `read()`（EOF は `-1`） | `read() -> Option<u8>`（EOF は `None`） |
| `close()` | 値の破棄（`drop`）。ほかの資源は持たない |

追加はすべて既存の型へのメソッドと新しいエラー型で、既存の公開 API（`read`、`read_into`、`as_input_stream` の戻り値の型）は変えていない。

### ケース対応

| 原典のケース | Rust のテスト | 原典のアサーション（順） | 移植 |
|:---|:---|:---|:---|
| "support mark and reset" | `support_mark_and_reset` | `markSupported`→true、`read`→'a'、`mark(1)`、`read`→'b'、`reset`、`read`→'b'、`close`（L31–38） | `true`、`Some(b'a')`、`mark(1)`、`Some(b'b')`、`reset()` が `Ok`、`Some(b'b')`、`drop` |
| "support skip" | `support_skip` | `skip(1)`→1、`read`→'b'、`close`（L41–44） | `1`、`Some(b'b')`、`drop` |
| "support skip with large value" | `support_skip_with_large_value` | `skip(50)`→3、`read`→-1、`close`（L47–50） | `3`、`None`、`drop` |

### 表現ごとの mark 対応

| 原典の表現 | 原典の入力ストリーム | 原典の mark 対応 | 移植の到達経路 | 移植の配列数 | 移植の mark 対応 |
|:---|:---|:---|:---|:---|:---|
| `ByteString1C` | `UnsynchronizedByteArrayInputStream(bytes)`（L308） | 対応 | `empty`、`from_string`、`from_array`、`from_ints` | 0か1 | 対応 |
| `ByteString1` | `UnsynchronizedByteArrayInputStream(bytes, startIndex, length)`（L505–506） | 対応。mark していない reset は `startIndex` へ戻る | 移植にスライスの操作はない。`from_array` は原典でもコピーして `ByteString1C` を返す（L1234–1243） | 1 | 対応。reset は先頭へ戻る |
| `ByteStrings` | `SequenceInputStream`（L637–638） | 非対応。`mark` は何もせず、`reset` は `IOException` | 非空どうしの `concat` | 2以上 | 非対応。`mark` は何もせず、`reset` は `Err(MarkNotSupportedError)` |

原典の `++` は、片方が空なら相手をそのまま返し、非空どうしなら `ByteStrings` にする（L217–220、L437–448、L597–604）。移植の `concat` も空の配列を持たないので、配列の数は原典の表現と一致する。原典の、隣接するスライスを1つにまとめる分岐（L444–445）は、移植にスライスの操作がないので起こらない。

### 原典との差異（意図した差）

原典3ケースの観測は変わらない。

- **builder の結果の mark 対応**：Pekko の `ByteStringBuilder` は、配列や大きな列を足すたびに独立した断片を作り、断片が2つ以上なら `ByteStrings` を返す（L1355–1366、L1566–1575）。移植の builder は、`add_all` で足したバイトを1つの配列にまとめる。例：`builder ++= Array(1, 2)` の後に `builder ++= Array(3)` を足すと、原典は2断片の `ByteStrings`（mark 非対応）、移植は1配列（mark 対応）になる。差は「原典が非対応のものを、移植が対応と答える」の一方向だけである。builder には手を入れず、[#148](https://github.com/j5ik2o/fraktor-ng-rs/issues/148) で追跡する。単体テスト `a_built_byte_string_supports_marks_by_how_many_arrays_it_holds` で今の振る舞いを固定した。#148 では副元帥の決定（項目227）により意図した差として固定し、補足回帰と台帳の据え置きの根拠を [G0issue148 のゴール文書](issue-148-builder-mark.md) に記録した。
- **skip の負の入力**：原典の `skip` は負の `n` で `IllegalArgumentException` を投げる。移植の `usize` では負の値を表せない。
- **skip の JVM 整数の上限**：原典の `skip` は `Math.toIntExact(n)` と `Math.addExact` を使うので、`n` が `Int.MaxValue` を超えるか、位置との和が溢れると `ArithmeticException` を投げる。移植は `usize::MAX` でも残量で頭打ちにして返す。
- **残量を超える skip の後の位置**：原典は残量で頭打ちにした値を返すが、位置は `n` だけ進める（L172–178）。そのため `"abc"` で `skip(50)` の後は位置50になり、続く `skip(0)` や `skip(1)` は `eod - offset` の負値（-47）を返す。移植は位置を末尾で止め、EOF の後の skip は0を返す。`read` が EOF を返す点と、mark の位置へ reset で戻れる点は同じである。
- **連結への skip**：原典の連結は `SequenceInputStream` で、`skip` は `InputStream` の既定の実装（読み捨て）になる。返す値は `min(n, 残量)` で、移植と同じである。
- **EOF の表し方**：`-1` ではなく `None` で返す（G0issue130 と同じ）。

## 実装

- `ByteStringInputStream`（既存）に `marked: Option<ByteIterator>` を足した。`None` は mark 非対応を表す。`mark` は今の `ByteIterator` を複製して保持し、`reset` はそれで読み位置を戻す。`ByteIterator` は配列を `Arc` で共有するので、複製はコピーしない。
- `ByteStringInputStream::new` は `pub(crate)` のまま、mark 対応かどうかの引数を足した。`ByteString::as_input_stream` が `arrays.len() <= 1` を渡す。
- `MarkNotSupportedError`：`reset` が返すエラー。`Display` は Java の例外のメッセージ `mark/reset not supported` と同じ。`core::error::Error` を実装する。
- `ByteIterator::skip_bytes`（`pub(crate)`）：配列の境界をまたいで最大 `count` バイト進め、進んだ数を返す。
- kernel に std は持ち込んでいない（syn-check の違反0）。

### 補足の単体テスト（台帳の件数に含めない）

原典3ケースの外で Rust の契約を固定するために12件を足した。`ByteStringInputStream` に11件、`ByteIterator` に1件である。

| テスト | 固定する契約 |
|:---|:---|
| `reset_without_a_mark_goes_back_to_the_start` | mark していない reset は先頭へ戻る（原典 `markedOffset` の初期値） |
| `a_mark_survives_reading_past_the_read_ahead_limit_and_the_end` | read-ahead の値に関係なく、EOF まで読んでも mark の位置へ戻れる |
| `a_later_mark_replaces_the_earlier_one` | 後の mark が前の mark を置き換える |
| `reset_goes_back_to_the_mark_after_skipping_past_the_end` | 残量を超える skip の後も mark の位置へ戻れる |
| `a_sliced_byte_string_supports_marks_from_its_own_start` | `from_array` の結果も mark 対応で、先頭はスライスの先頭 |
| `an_empty_stream_skips_nothing_and_supports_marks` | 空のバイト列は mark 対応で、skip は0 |
| `skip_returns_what_is_left_then_zero_after_the_end` | `skip(0)` は0、`usize::MAX` は残量、EOF の後は0、`read`/`read_into` は EOF、空バッファは `Some(0)` |
| `concatenating_with_an_empty_byte_string_keeps_mark_support` | 空との連結は mark 対応のまま（原典の `++` が相手を返すのと同じ） |
| `a_concatenation_does_not_support_marks` | 連結は非対応。`mark` は何もせず、`reset` は `Err` で、読み位置は変わらない |
| `a_concatenation_skips_across_its_parts_and_stops_at_the_end` | 連結への skip は部分をまたぎ、残量で頭打ち |
| `a_built_byte_string_supports_marks_by_how_many_arrays_it_holds` | builder の結果は配列の数で決まる（#148 の差を固定） |
| `skipping_moves_across_arrays_and_stops_at_the_end`（`ByteIterator`） | 空の配列を含む配列の境界をまたいで進み、末尾で止まる |

## 等価性レビュー

### 実装者自身のレビュー

実施者は本 issue の担当（Claude Opus 5.5）。Pekko 固定 SHA の原典テスト、`UnsynchronizedByteArrayInputStream.java`、`ByteString.scala` の `asInputStream`・`++`・builder と、移植の実装・テストを突き合わせた。判定は「等価」。原典3ケースは観測上等価で、除外は0。差異は前節のとおりで、どれも原典3ケースの観測には現れない。

### 独立レビュー（別系統のモデル）

PR 作成後、既出の指摘に対応した HEAD を固定して行う（`docs/decisions/takt-workflow-design.md` §2.6）。結果はここに追記する。

## 製品編集前の実測

`1357099` と `d286b23` は、どちらも `718a713` からの製品差分が0である（`git diff --stat 718a713 d286b23 -- 'crates/*/src'` は空）。テストの blob は `1357099` が `d54e869a`、`d286b23` が `92325cec`。

| コミット | コマンド | 終了値 | 結果 |
|:---|:---|---:|:---|
| `1357099` | `cargo test -p fraktor-actor --test unsynchronized_byte_array_input_stream_spec -- --test-threads=1` | 101 | compile-red。`E0599` が5件（`mark_supported`、`mark`、`reset` が未定義、`skip` は Iterator ではないため未解決）。宣言3件、実行0件、除外0件 |
| `1357099` | `verify_red.py`（原典を `PEKKO_ORIGINAL_ROOT` に置いて実行） | 0 | `expected_compile_error`、`excluded: 0`、`red_recorded: true` |
| `d286b23` | 同じ cargo test | 101 | compile-red。同じ `E0599` の5件 |
| `d286b23` | 同じ `verify_red.py` | 0 | `expected_compile_error`、`excluded: 0` |

already_green ではない。既存の入力ストリームに mark・reset・skip がなく、コンパイルに失敗した。実装のコミットでは統合テストを変えていない。

## 実装後の実測

実装と台帳 green のコミットで、担当が次を実行した。

| コマンド | 終了値 | 結果 |
|:---|---:|:---|
| `cargo test -p fraktor-actor --test unsynchronized_byte_array_input_stream_spec -- --test-threads=1` | 0 | 3 passed / 0 failed / 0 ignored |
| `cargo test -p fraktor-actor --test byte_string_input_stream_spec --test byte_iterator_spec` | 0 | 3 passed、1 passed（既存の回帰） |
| `cargo test -p fraktor-actor --lib byte_` | 0 | 37 passed（既存25、新規12） |
| `cargo test --workspace` | 0 | 67 スイート、683 passed / 0 failed / 0 ignored |
| `cargo fmt --check` | 0 | 差分なし |
| `cargo clippy --workspace --all-targets -- -D warnings` | 0 | 警告なし |
| syn-check（`crates/fraktor-actor`、`crates/fraktor-testkit`） | 0 | 両方 `clean: true`、違反0 |
| `npm ci` → `npm run check:aislop` | 0 | warn 以上の指摘は受理済み一覧と一致 |
| `goal_complete_gate_test.py`、`verify_red_test.py`、`verify_green_test.py`、`verify_green_test_aislop.py`、`verify_green_test_crate.py` | 0 | 成功 |
| `verify_green.py --crate fraktor-actor` | 0 | `all_pass: true` |

台帳は `G0issue147`、`G0issue130`、`G0issue133`、`G0a`〜`G0f` の各々で `check_needs.py` と `rollup.py` を実行し、18コマンドすべて終了値0だった。`G0issue147` は1件で `{'green': 1}`。

## 再現手順

```text
cargo test -p fraktor-actor --test unsynchronized_byte_array_input_stream_spec -- --test-threads=1
cargo test -p fraktor-actor --test byte_string_input_stream_spec --test byte_iterator_spec
cargo test -p fraktor-actor --lib byte_
cargo test --workspace
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
uv run --with pyyaml python3 ledger/tools/check_needs.py --ledger ledger/nodes.yaml --capabilities ledger/capabilities.yaml --goal G0issue147
uv run --with pyyaml python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal G0issue147
```
