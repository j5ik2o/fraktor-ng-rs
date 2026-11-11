# cfg-std-forbid（旧 cfg-std-forbid-lint、`cfg_std_forbid`）

## 旧 lint から写したもの

- 旧 lint は内部で syn を使ってファイルを読み直していた。判定は旧 `lib.rs` の `collect_forbidden_spans` 以下をそのまま写した
- 禁止するのは 2 つ
  - `cfg` 属性のうち、条件に `feature = "std"` を含むもの（`any(...)` や `all(...)` の中も見る。`not(...)` の中は見ない）。
    ファイル、アイテム、フィールド、文、式など、syn が訪問するすべての属性が対象（旧 `visit_attribute`）
  - `use` のツリーのどこかに `std` というセグメントがあるもの（`use std;`、`use ::std::...`、`use std as s;`、
    `use crate::a::std::B;` も違反）。関数本体の中の `use` も対象
- 例外は cfg(test)。`#[cfg(test)]` か、`test` を含み `not` と `any` を含まない `all(...)` が付いたアイテムの中は、
  ファイルの残りの部分と切り離して丸ごと許す。ファイルの `#![cfg(test)]` はファイル全体を許す
  （旧 `attr_is_cfg_test`、`meta_requires_test`、`meta_allows_without_test`）。
  `cfg(any(test, feature = "std"))` は例外にならず、`cfg(all(test, not(...)))` も例外にならない
- 報告位置は、属性はその `#` の位置、`use` はアイテムの先頭（属性を含む）（旧 `attr.span()`、`item_use.span()`）

### 適用範囲

- 旧 lint は既定レベルが `Allow` で、パスの判定を持たなかった。適用範囲は lint レベルで決まり、
  旧 fraktor-rs では core クレートのルートの `#![deny(cfg_std_forbid)]` で有効にしていた
  （`docs/decisions/fraktor-rs-dev-rules-survey.md`）。旧 UI テストも、各ケースの先頭の `#![deny]` で有効にしている
- `Layout::Legacy`：1 ファイルからは決まらないので、次のどちらかが成り立つファイルを適用範囲とする近似にした
  - そのファイル自身の内側属性に `#![deny(cfg_std_forbid)]`、`#![warn(...)]`、`#![forbid(...)]` がある
  - そのファイルを含む最も近い `src` ディレクトリの `lib.rs` か `main.rs` の内側属性に、同じものがある
- `Layout::Ng`（暫定形）：適用範囲は kernel のソース（`crates/<crate>/src/kernel.rs` と `crates/<crate>/src/kernel/**`）。lint レベルは読まない

## 写せなかったもの、変えたもの

- **ファイルをまたぐ cfg(test) の例外を足した。** 親モジュール（またはその祖先）が `#[cfg(test)] mod <name>;` で宣言しているファイルは、
  丸ごと許す。旧 lint は rustc がコンパイルしたファイルだけを見ていたので、テストをビルドしない実行ではこれらのファイルを見ていなかった。
  またインラインの `#[cfg(test)] mod tests { ... }` は旧 lint でも許されていた。この例外が無いと、
  `<module>/tests.rs` に置くテスト（tests-location の規約）がすべて違反になる。親は `<dir>.rs`、`<dir>/mod.rs`、
  `src` 直下なら `lib.rs` か `main.rs` を探す。ファイル直下の `mod` 宣言だけを見る
- 同じ例外を、sibling test hook にも適用した。`<module>_test.rs` は、隣の `<module>.rs` にある
  `#[cfg(test)] #[path = "<module>_test.rs"] mod tests;` で宣言されていれば丸ごと許す。tests-location が求める形が
  これなので、テストファイルはふつう自分の名前では宣言されない。`cfg(test)` の無い `#[path]` 宣言は例外にしない
- 旧 lint が `#[allow(cfg_std_forbid)]` を見たとき、その属性以降を囲むアイテムの終わりまで許していた挙動は、コメント指定に置き換えた（下記）
- lint レベルの近似で拾えないもの：`Cargo.toml` の `[lints]`、`RUSTFLAGS` や dylint の設定、`#![cfg_attr(..., deny(cfg_std_forbid))]`、
  親モジュールの `#[deny(cfg_std_forbid)] mod x;`、アイテムの外側属性の `#[deny]`。どれも 1 ファイルの構文からは決まらない
- 違反メッセージは旧 lint の主メッセージに、移し先を 1 文で続けた。旧 `help` にあった `#![allow(cfg_std_forbid)]` の案内は、コメント指定に置き換わったので書かない

## 例外規定と抑制

- 旧 `#[allow(cfg_std_forbid)]` と `#![allow(cfg_std_forbid)]` は、`// syn-check: allow(cfg-std-forbid) -- <理由>` と
  `// syn-check: allow-file(cfg-std-forbid) -- <理由>` に置き換える。ルール側では `#[allow]` 属性を読まない
- 旧 UI の `core_allow.rs` は、`#[allow(cfg_std_forbid)]` の直前の空行をコメント指定に置き換えて通した（行番号は動かない）
- 旧 UI の `non_core_ok.rs` はファイルの `#![allow]` だけを持ち、`#![deny]` を持たない。Legacy の適用範囲に入らないので、フィクスチャは変えずにクリーンになる

## 既知の限界

- **別名の再エクスポートを経由した std の利用は検出できない。** 別のモジュールが `pub use std::sync::Arc as Shared;` していて、
  それを `use crate::util::Shared;` で import する場合、import する側には `std` が現れない。syn は名前解決をしないためである
  （再エクスポートする側が適用範囲にあれば、そちらは違反になる）
- **`use` 以外の位置の `std::` パスは見ない。** 旧 lint も `Item::Use` だけを見ていた。式や型の中のフルパス
  （`std::time::Instant::now()`、`let _: std::sync::Arc<u8>`）、`extern crate std;`、マクロの中の `std::` は検出しない
- cfg の条件式は評価しない。`feature = "std"` を含むかどうかを構文で見るだけである
- `#[cfg(test)]` が付いた impl のメソッドやトレイトのメソッドは syn のアイテムではないので、例外にならない。
  その本体の `use std::...` は違反になる（旧 lint と同じ）
- インラインモジュールの中の `#[cfg(test)] mod x;` が宣言するファイル（`mod a { #[cfg(test)] mod tests; }` の先）と、上の sibling test hook 以外の `#[path]` で指すファイルは、ファイルをまたぐ例外の対象にならない

## 要判断

- **ng での適用範囲（暫定形）。** kernel のソース全体で std を禁止した。ng では no_std の要否が未決定であり
  （`docs/design-settings-draft.md` 3.4）、この形はそれを先取りしている。ほかに取りうる形は次のとおり
  - kernel では、時刻、スレッド、IO に触れる std のモジュール（`std::time`、`std::thread`、`std::io`、`std::net`、`std::fs`、`std::process` など）だけを禁止する。
    port & adapter 段階1の目的（テストの決定性、実行基盤の差し替え）には、これで足りる。`std::sync::Arc` や `std::collections` は許すことになる
  - no_std を宣言したクレート（ルートに `#![no_std]` か `#![cfg_attr(not(test), no_std)]`）だけに適用する。no_std を要求するクレートが決まるまで、どこにも効かない
  - 旧 lint と同じく、クレートのルートの `#![deny(cfg_std_forbid)]` のような明示で有効にする
- **ng の kernel のパス（暫定形）。** `crates/<crate>/src/kernel.rs` と `kernel/**` を kernel とした。モジュール名や配置が変われば述語も変わる
- **ファイルをまたぐ cfg(test) の例外**（上記「変えたもの」）を残すかどうか。外すと、kernel の `tests.rs` の `use std::...` がすべて違反になる
- **`#[cfg(feature = "std")]` の禁止を ng で残すかどうか。** ng に `std` フィーチャを置くかは決まっていない。
  置かないなら、この判定は常に空振りする。置くなら、kernel の中で std を cfg で切り替えることを禁止する意味になる
