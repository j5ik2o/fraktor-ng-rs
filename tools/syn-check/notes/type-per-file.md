# type-per-file（旧 type-per-file-lint、`multiple_type_definitions`）

## 旧 lint から写したもの

- 数える対象は `pub` の struct、enum、trait だけ。type alias と union は数えない
  （旧 `check_item` の `ItemKind::Struct | Enum | Trait`）
- 公開の判定は素の `pub` だけ。`pub(crate)`、`pub(super)`、`pub(in ...)` は数えない
  （旧 `cx.tcx.visibility(def_id).is_public()`）
- インラインモジュールの中と、関数本体の中の型も、同じファイルの型として数える
  （旧 lint は HIR の全アイテムを訪問していた）
- 違反は 2 つ目以降の型ごとに 1 件。報告位置は可視性トークンの位置で、属性は含まない
- 除外するパスは旧 `should_ignore` をそのまま写した。`target` 配下、`tests` ディレクトリ配下
  （直後が `ui` の場合を除く）、`tests.rs`、`*_test.rs`、`*_tests.rs`

## 写せなかったもの、変えたもの

- マクロが生成する型は見えない。旧 lint も `item.span.from_expansion()` で除外していたので、結果は同じである
- **例外規定を 1 つ足した。** 同じスコープ（同じインラインモジュールの並び、または同じブロック）に
  同名の型が複数あれば、1 つとして数える。同名の型が同じスコープに並ぶのは、cfg で排他になっている場合だけである。
  旧 lint は cfg 展開後の 1 つだけを見ていたので、数は旧 lint と同じになる
- スパイクが報告した偽陽性（cfg 切り替えの同名 type alias を 6 型と数えた件）は、スパイクが alias を
  数えていたことによる。旧 lint は alias を数えないので、忠実に写した時点で alias は対象外になる。
  上の例外規定は、同じ形が struct で書かれた場合を受ける
- 同梱された旧実装は `lib.rs` だけで、`state.rs`（`TypeRecord`）は無かった。`lib.rs` からの呼び出し方
  （`new`、`name`、`kind`、`span`、`describe`）を見て同じ役割の型を書いた
- スパイクの RESULT.md は「小さい型・親型限定などの許容条件」に触れているが、`lib.rs` にその条件は無い

## 例外規定と抑制

- 旧 `#[allow(multiple_type_definitions)]` は、型の直前の行の
  `// syn-check: allow(type-per-file) -- <理由>` に置き換える
- 抑制した型も型の一覧には残る（旧 lint と同じ）

## 既知の限界

- cfg の条件式は評価しない。同じスコープの同名の型は、条件を見ずに 1 つと数える

## 要判断

- なし
