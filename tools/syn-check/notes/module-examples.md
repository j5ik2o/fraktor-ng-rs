# module-examples（旧 module-examples-lint、`module_examples_forbid`）

## 旧 lint から写したもの

- 述語 `is_module_example_file`。拡張子が `rs` で、パスに `target` という要素が無く、
  連続する 4 つの通常の要素が `[modules, <crate>, examples, <file>]` の形になるもの。
  4 番目はディレクトリでもよいので、`examples/nested/main.rs` も対象になる。この並びはパスのどこにあってもよい
- 違反はファイルごとに 1 件。報告位置は 1 行目 1 桁目（旧 `first_line_span`）
- 旧 lint はアイテムを訪れたときに報告した。アイテムが無いファイル（コメントだけ、`macro_rules!` だけ、
  `#![...]` だけ）には出さない。同じ条件を「トップレベルにアイテムがある」として写した
- メッセージの移動先 `showcases/std/<example-name>/main.rs` は、`Layout::Legacy` のメッセージにだけ残した

## 相対パスで同じ結果になること

旧 lint は絶対パスで判定した。新チェッカーは検査時に与えられた相対パスで判定する。述語は要素の並びを見るだけで、
先頭が `modules` でも途中に `modules` があっても同じである。`modules/...` で始まる相対パス、`./` 付き、
絶対パスで結果が同じことを `tests/module_examples.rs` で確かめた（テストは作業ディレクトリをケースのディレクトリに移して実行する）。
旧 UI の補助ファイル `auxiliary/modules/actor-adaptor/examples/classic_logging.rs` は、途中に `modules` があるパスで一致した

## 配置ごとの述語

- `Layout::Legacy`: 旧 lint のまま。`modules/<crate>/examples/`
- `Layout::Ng`（暫定）: `crates/<crate>/examples/`。先頭の `modules` を `crates` に読み替えただけで、ほかは変えていない。
  `Layout::Legacy` は `crates/` を、`Layout::Ng` は `modules/` と `showcases/` を対象にしない

## 写せなかったもの、変えたもの

- 旧 lint が出していた `note`（対象ファイル）、`help`、AI 向けの `note` は、1 件のメッセージに畳んだ。
  対象ファイルは違反のパスとして別に出る
- 旧 lint の対象は、コンパイルに含まれるファイルだけだった。新チェッカーは検査対象のパスの下の `.rs` をすべて見る。
  どこからも取り込まれていない例も違反になる
- `target` 要素の除外は写したが、探索が `target` ディレクトリを飛ばすので、ディレクトリを辿る検査では実際に効く場面が無い
  （ファイルを直接渡したときだけ効く）

## 例外規定と抑制

- 旧 lint の抑制は crate の `#![allow(module_examples_forbid)]` だった。報告位置が 1 行目なので、
  行の `allow` では届かない。ファイルの先頭に `// syn-check: allow-file(module-examples) -- <理由>` を書く

## 既知の限界

- cfg で除かれたアイテムも、アイテムの有無の判定では数える。マクロ呼び出しは、アイテムを生むものとして数える

## 要判断

- **`Layout::Ng` の対象を `crates/<crate>/examples/` と仮定した。** fraktor-ng-rs のクレートは `crates/<crate>/` にあるので、
  `modules/<crate>/` を読み替えた。ng で examples を禁止するのか、禁止するならどのクレートの階層か、は人間が決める必要がある
- **ng には旧 `showcases/` に当たる置き場所がまだ無い。** 旧 lint は移動先を `showcases/std/<example-name>/main.rs` と案内した。
  ng のメッセージは移動先を名指しせず、「置き場所は未定」と書いている。置き場所が決まったらメッセージを直す
- アイテムの有無の判定（コメントだけのファイルには出さない）は旧 lint の副作用である。`examples/` にあるのに
  アイテムが無いファイルを見逃してよいか。暫定では旧 lint どおり出さない
