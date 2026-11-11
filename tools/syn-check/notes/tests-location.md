# tests-location（旧 tests-location-lint、`separate_tests`）

旧 lint には UI テストが同梱されていなかった。照合できる正解が無いので、旧 `lib.rs` を 1 行ずつ読んで、
違反の種類と例外をすべて合成フィクスチャ（`tests/ui/synthetic/tests-location/`）で覆った。
フィクスチャと期待値は旧 `lib.rs` の読解に基づくもので、旧 lint を実際に走らせた結果ではない。
各テストのコメントに、覆っている旧関数と分岐を書いた。

旧 lint は HIR を使わず、ファイルのテキストを読み直して文字列で探していた。このポートも、syn の構文木を使わずに
同じテキスト走査で書いた（`find_*` 関数は旧実装と同じ処理）。

## 旧 lint から写したもの

- ファイルの振り分け（`is_tests_path`）。ファイル名が `tests.rs`、`*_test.rs`、`*_tests.rs` のどれか、
  またはパスのどこかに `tests` という名前の要素があればテストファイル。それ以外は本番ファイル
- 本番ファイルの検査（`analyze_source_file`）は 3 種類
  - `find_inline_test_modules`: `mod tests` の後に空白（改行を含む）を挟んで `{` があれば違反。
    可視性と属性は見ないので、`pub mod tests {` も入れ子の `mod tests {` も違反。報告位置は `mod`
  - `find_cfg_test_items`: 文字列 `#[cfg(test)]` との完全一致を探す。一致したら、次の 2 つの例外に
    当てはまらない限り違反。報告位置は `#`
    - `is_sibling_test_module_hook`: 直後の属性が、空白を除いて `path="` で始まり `_test.rs"` で終わる。
      その後ろに、可視性を挟んでもよい `mod tests;` が続く
    - `is_tests_module_declaration`: 直後が、可視性を挟んでもよい `mod tests` で、その次の文字が `;`、`{`、
      空白のどれか。`#[path]` なしの `#[cfg(test)] mod tests;` もここで通る。`mod tests {` を通すのは、
      `find_inline_test_modules` との二重報告を避けるため
    - 可視性の剥がし方（`strip_visibility_prefix`）は `pub` と空白、または `pub(` から最初の `)` まで
  - `find_direct_test_attributes`: すべての `#[...]` について、属性名（`(` か空白の前まで）が `test` か、
    `::test` で終われば違反。報告位置は `#`。一致しなかった属性の本体は読み飛ばす。閉じ括弧が見つからなければ、
    そこで走査を打ち切る
- テストファイルの検査（`analyze_tests_file`）は `find_inner_cfg_test_attributes` だけ。`#![...]` の本体が、
  空白を除いて `cfg(test)` と一致すれば違反。テストファイルのインライン `mod tests {}`、`#[cfg(test)]`、
  `#[test]` は検査しない
- 属性の閉じ括弧の探し方（`find_attribute_end`）。`[` と `]` の深さだけを数え、文字列やコメントは区別しない
- 報告位置は、旧 lint が span を作っていた範囲の先頭。列は行頭からの文字数で数える（rustc と同じ）
- パスの述語は検査時に与えられた相対パス（`file.path()`）に対して評価する

## 写せなかったもの、変えたもの

- **ファイルを選ぶ条件が違う。** 旧 lint は `check_item` が最初に呼ばれた時点で、そのファイルを検査していた。
  したがって、rustc がコンパイルしたファイルのうち、cfg 展開後にアイテムが 1 つ以上残るものだけが対象だった。
  たとえば、テストのビルドでないときの `#![cfg(test)]` 付きテストファイルは中身ごと消えるので、旧 lint はこれを見ない。
  新チェッカーは、見つけた `.rs` ファイルをビルド構成に関係なくすべて検査する。判定の中身は変えていない
- 違反メッセージは日本語にした。旧 lint の英語の本文、help、日本語の note の内容を 1 文にまとめた
- 判定はテキストだけで行うので、syn がパースできないファイルも検査する（旧 lint もテキストで判定していた）

## 例外規定と抑制

- 例外は `is_sibling_test_module_hook` と `is_tests_module_declaration` の 2 つで、どちらも旧実装のまま写した
- 抑制は共通のコメント指定を使う。`// syn-check: allow(tests-location) -- <理由>` は次の要素全体にかかるので、
  `#[cfg(test)] mod tests { ... }` の前に置けば、中の `#[cfg(test)]` と `#[test]` の報告もまとめて消える。
  ファイル全体は `allow-file(tests-location)` で消す。`#[allow(separate_tests)]` は読まない
- 旧 lint の `#[allow(separate_tests)]` は、`emit_span_lint` を `check_item` の中から呼んでいたので、
  lint レベルは報告位置ではなく、そのファイルで最初に訪問したアイテム（とその祖先）から決まっていたはずである。
  これはコードから読み取った推測で、旧 lint を走らせて確かめてはいない。新チェッカーでは報告位置ごとに抑制する

## 既知の限界

- テキスト走査の癖をそのまま持つ（次節）。マクロが生成するテストは見えない（旧 lint も見ていなかった）

## 要判断

- **旧実装の癖: テキスト走査のため、コメント、doc コメント、文字列リテラルの中も拾う。** 例として、
  doc コメントのコード例に書いた `#[test]`、`// mod tests { }`、`"#[cfg(test)]"` は違反になる。
  syn で書き直せば、構文上の属性と `mod` 宣言だけを見られるので、旧 lint の意図に合い、偽陽性も消える。
  ただし結果が変わるので、書き直さずに旧実装と同じ判定にした（合成テスト
  `text_scan_matches_inside_comments_and_strings` が今の挙動を固定している）
- **旧実装の癖: 例外の判定が字面に依存する。** 次の形は、意図としては hook と同じでも違反になる。
  `#[cfg(test)]` と `#[path]` の間に別の属性がある形、`mod tests ;`（空白入り）、`#[cfg(test)] #[allow(..)] mod tests {}`
  （cfg(test) と inline module の二重報告）。逆に、`#[cfg( test )]` と複合 cfg（`all(test, ..)`、`any(test, ..)`）は、
  一致しないので素通りする。複合 cfg の hook（`#[cfg(all(test, unix))] #[path = ".._test.rs"] mod tests;`）が通るのは、
  module-wiring の旧 UI `sibling_test_hook_compound_cfg_ok.rs` とも合う。rustfmt は `#[cfg( test )]` を `#[cfg(test)]` に
  直すので、実際のコードで差が出るのは属性の順序と複合 cfg に限られる。どちらも旧実装のまま写した
- **旧実装の癖: `#[path]` なしの `#[cfg(test)] mod tests;` を通す。** sibling `*_test.rs` の規約からは外れる形
  （`tests.rs` か `tests/` を読む）だが、旧実装は `is_tests_module_declaration` で通していた。旧実装のまま写した
- **旧実装の癖: 本番ファイルの `#![cfg(test)]` はどの検出にもかからない。** 旧実装のまま写した
- **ng で sibling `*_test.rs` の規約を使うかどうか。** fraktor-ng-rs の移植テストは `crates/<crate>/tests/` に、
  ノード 1 件をファイル 1 件として置く。`is_tests_path` は `tests` ディレクトリを含むパスをテストファイルとみなすので、
  述語を変えなくてもそのまま当てはまる。合成テスト `ng_crate_layout_needs_no_other_predicate` で確かめたので、
  ルールは layout を見ていない。一方、`crates/<crate>/src/**` の中の単体テストを sibling `*_test.rs` に置くか、
  `tests/` に一本化してインラインのテストも sibling hook も禁じるかは決まっていない。決まった時点で、
  hook の例外（`is_sibling_test_module_hook`）を ng でも残すかを見直す
- **ng で `tests` ディレクトリの判定が広すぎないか。** `crates/<crate>/src/tests/` のような本番ソース内のディレクトリも
  テストファイル扱いになる（旧実装と同じ）
