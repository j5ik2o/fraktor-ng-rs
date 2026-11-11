# Pekko 移植コーディングルール（チェッカー連動）

以下は、verify_green の syn チェッカー（tools/syn-check）で機械判定される
ルールである。ゲートで落とされてから直すのではなく、書く時点で守ること。
ルールの正はチェッカーの定義と `tools/syn-check/README.md` であり、この
ポリシーはその事前提示用の要約である。食い違いがあればチェッカー側に
合わせてこちらを直す。

「暫定」と書いた箇所は、チェッカー側で人間の判断を待っている点である
（`tools/syn-check/README.md` の「要判断」）。決まれば変わりうる。

## ファイルと型の配置

- **type-per-file**：1ファイルに置く `pub` の struct / enum / trait は
  1つだけ。type alias、union、`pub(crate)` などの限定公開は数えない。
  cfg で切り替える同名の型は1つと数える。テスト側のファイル
  （`tests/` 配下、`*_test.rs`）は対象外
- **mod-file**：`mod.rs` は作らない。モジュールは `foo.rs` と `foo/`
  ディレクトリの階層で表す
- **module-wiring**：再エクスポート（可視性を付けた `use`）は、末端
  モジュール（子モジュールを持たないモジュール）の直属の親だけが、
  `pub use child::Type;` の形で行う。`self` / `super` / `crate` を経由した
  再エクスポート、`as` での付け替え、子モジュールを介さない再エクスポートは
  書かない。直属の子でないもの（ほかのクレート、`core`、`std`、孫モジュール）
  からの再エクスポートも書かない。末端モジュールは可視性なしの `mod foo;` で
  宣言する。
  `#[path]` と `include!` でモジュールを結線しない。例外は
  `#[cfg(test)] #[path = "<module>_test.rs"] mod tests;` だけ。
  `prelude` という名前のモジュールは対象外
- **module-examples**：`crates/<crate>/examples/` に example を置かない
  （暫定。代わりの置き場所は未定）

## 記述の配置

- **use-placement**：`use` と、本体を持たない `mod` 宣言は、ファイルまたは
  モジュールの冒頭にまとめる。関数やブロックの中に `use` を書かない。
  テストファイルにも適用される（暫定）
- **tests-location**：`src` 内の単体テストは、対象ファイルと同階層の
  `*_test.rs` に置き、本体ファイルには
  `#[cfg(test)] #[path = "<module>_test.rs"] mod tests;` だけを書く。
  この宣言も `mod` 宣言なので、ファイル末尾ではなく冒頭の `use` / `mod` の
  まとまりに置く（末尾に置くと use-placement に違反する）。
  本体ファイルに `mod tests { }`、`#[cfg(test)]` のアイテム、`#[test]` を
  書かない。テストファイルに `#![cfg(test)]` を書かない
  （移植テストは対象クレートの `tests/` に置く）

## 境界（port & adapter）

- **port-adaptor-boundary**：adaptor（`src/adaptor.rs` と `src/adaptor/**`）の
  公開 struct のフィールドに、kernel の具象 API（名前が `Api` で終わる型）を
  持たせない。kernel の型を、同名の struct の `inner` フィールドで包むことも
  しない。kernel 側に port（trait）を定義し、adaptor はその trait に依存する
  （適用範囲は暫定）
- **cfg-std-forbid**：kernel（`src/kernel.rs` と `src/kernel/**`）に
  `#[cfg(feature = "std")]` と `use std::...` を書かない。`core` か `alloc` を
  使うか、std に依存するコードを adaptor に移す。`#[cfg(test)]` のコードは
  対象外（適用範囲は暫定。no_std の要否は未決定）

## 命名と文書

- **ambiguous-suffix**：Manager / Util / Facade / Service / Runtime / Engine で
  終わる名前を付けない。型、関数、変数、引数、フィールド、モジュール、
  ファイル名のすべてが対象。テスト側のファイルは対象外
- **rustdoc**：doc コメント（`///` と `//!`）は英語で書く。コードフェンスに
  `ignore` を付けない

## 抑制コメント

チェッカーには抑制の手段がある（`// syn-check: allow(<rule>) -- <理由>` と
`// syn-check: allow-file(<rule>) -- <理由>`）。抑制コメントは人間の許可なく
足さない。ルールを守れない箇所があれば、コードを直すか、理由を添えて
報告する。抑制コメントはチェッカーの出力に一覧として出る。
