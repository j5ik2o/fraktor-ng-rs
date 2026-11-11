# Pekko 移植コーディングルール（チェッカー連動）

以下は、verify_green の syn チェッカー（tools/syn-check）で機械判定される
ルールである。ゲートで落とされてから直すのではなく、書く時点で守ること。
ルールの正はチェッカーの定義と `tools/syn-check/README.md` であり、この
ポリシーはその事前提示用の要約である。食い違いがあればチェッカー側に
合わせてこちらを直す。

## ファイルと型の配置

- **type-per-file**：1ファイルに置く公開型（pub struct / enum / trait /
  type / union）は1つだけ。関連する小さな型も、公開するなら別ファイルに
  分ける
- **mod-file**：`mod.rs` は作らない。モジュールは `foo.rs` と `foo/`
  ディレクトリの階層で表す
- **module-wiring**：モジュール宣言とファイル配置を一致させる。
  宣言のないファイル、実体のない宣言を作らない
- **module-examples**：モジュールの使用例は所定の置き場に置く

## 記述の配置

- **use-placement**：`use` はファイル冒頭の所定の位置にまとめる。
  関数本体やアイテムの途中に出し入れしない
- **tests-location**：`src` 内の単体テストは、対象ファイルと同階層の
  `*_test.rs` に置く。本体ファイルの中に `#[cfg(test)]` モジュールを
  埋め込まない（移植テストは対象クレートの `tests/` に置く）

## 境界（port & adapter）

- **port-adaptor-boundary**：各クレートの kernel はプラットフォーム
  依存を持ち込まない。時刻、スレッド、IO などの具象は port（trait）越しに
  使い、実装は adaptor 側に置く
- **cfg-std-forbid**：kernel 側で `cfg` による std 有無の切り替えを
  書かない。差異は境界で吸収する

## 命名と文書

- **ambiguous-suffix**：意味の曖昧な接尾辞の付いた型名・モジュール名を
  作らない（禁止対象の正はチェッカー定義）
- **rustdoc**：公開アイテムには rustdoc コメントを付け、契約
  （前提、失敗条件）を書く
