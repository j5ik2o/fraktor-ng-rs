# ambiguous-suffix（旧 ambiguous-suffix-lint、`ambiguous_suffix`）

## 旧 lint から写したもの

### 禁止サフィックスと代替案

旧 `FORBIDDEN_SUFFIXES` をそのまま写した。上から順に試し、最初に当たったものだけを報告する。

| サフィックス | snake_case | 代替案 |
|---|---|---|
| `Manager` | `manager` | Registry, Coordinator, Dispatcher, Controller |
| `Util` | `util` | 具体的な動詞を含む名前 (例: FormatHelper → DateFormatter) |
| `Facade` | `facade` | Gateway, Adapter, Bridge |
| `Service` | `service` | Executor, Scheduler, Evaluator, Repository, Policy |
| `Runtime` | `runtime` | Executor, Scheduler, EventLoop, Environment |
| `Engine` | `engine` | Executor, Evaluator, Processor, Pipeline |

### 名前の判定

- Camel（型の名前）: `name == suffix || name.ends_with(suffix)`。大文字小文字を区別する。
  `Manager` と `TaskManager` は当たり、`Mismanager`、`TASK_SERVICE` は当たらない
- Delimited（それ以外）: 先頭の `'` を外し、ASCII 小文字にしてから、`name == suffix` か
  「`_` + suffix で終わる」かを見る。`taskmanager` は当たらず、`Task_Manager` と `manager` は当たる
- 名前が `_` のときは見ない（`const _`）
- ファイル名（拡張子を除いた名前）は小文字にせず、snake_case のサフィックスと比べる。`my_runtime.rs` と
  `runtime.rs` は当たり、`HttpRuntime.rs` は当たらない

### 検査する構文要素と種別ラベル

旧 lint の各フックと、新実装の対応。名前の形は旧 lint の `NameStyle`。

| ラベル | 形 | syn で見る場所 |
|---|---|---|
| static / const / function / macro / module | Delimited | `Item::Static` / `Const` / `Fn` / 識別子つきの `Item::Macro`（`macro_rules!`）/ `Mod` |
| type alias / enum / struct / union / trait / trait alias | Camel | `Item::Type` / `Enum` / `Struct` / `Union` / `Trait` / `TraitAlias` |
| associated const / trait method | Delimited | `TraitItem::Const` / `Fn` |
| associated type | Camel | `TraitItem::Type`、`ImplItem::Type` |
| associated const / method | Delimited | `ImplItem::Const` / `Fn` |
| parameter | Delimited | 本体の無いトレイトメソッドの引数のうち `Pat::Ident` |
| field | Delimited | 名前つきフィールド（構造体、共用体、列挙型バリアントの中）。タプルの位置フィールドは対象外 |
| variant | Camel | `Variant` |
| variable | Delimited | 束縛になる `Pat::Ident`（関数引数、`let`、`match`、`if let`、`for`、クロージャ引数を含む） |
| generic parameter / const parameter | Camel / Delimited | `GenericParam::Type` / `Const` |
| lifetime | Delimited | `GenericParam::Lifetime`（`for<'a>` も含む）。名前は `'` つきで表示する |
| function pointer parameter | Delimited | `BareFnArg` の名前 |
| file | 小文字化しない snake | ファイルの最初のアイテムの位置で報告 |

`use`、`extern crate`、`extern` ブロック、`impl` ブロックは名前の検査をしない。外部アイテム
（`extern` ブロックの中の関数・static）には旧 lint のフックが無いので、名前も引数も見ない
（中の関数ポインタ型と、ジェネリクスだけは見る）。

### 報告位置と抑制

- 名前の違反は識別子の位置。ファイル名の違反は、そのファイルの最初のアイテムの可視性トークンの位置
  （`ast::decl_start`。属性と doc コメントは含まない）
- 「最初のアイテム」は、識別子を持たない `Item::Macro`（マクロ呼び出し）を除いた先頭。
  旧 lint はマクロ呼び出しが展開された後のアイテムを検査しなかったので、同じ結果になる
- 旧 `#[allow(ambiguous_suffix::ambiguous_suffix)]` は、直前の
  `// syn-check: allow(ambiguous-suffix) -- <理由>` に置き換える。ファイル名の違反を許可するには、
  ファイル全体にかかる `allow-file` を使う（旧 lint は `#![allow]` だった）
- 除外するパスは旧 `should_ignore` をそのまま写した。`.rs` 以外、`target` 配下、`tests` ディレクトリ配下
  （直後が `ui` の場合を除く）、`tests.rs`、`*_test.rs`、`*_tests.rs`
- 違反メッセージは旧 lint の主メッセージ・help・2 つの note を 1 つにつなげた。AI 向けアドバイスの
  `#[allow(...)]` は、コメント指定の書き方に差し替えた
- 述語は配置（`cx.layout`）で変わらない。`Layout` は見ない

## 写せなかったもの、変えたもの

`from_expansion` で旧 lint が除外していたものと、syn から見えないものの対応は次のとおり。

| 旧 lint | 新実装 |
|---|---|
| `derive` やマクロが生成するアイテム・メソッド・フィールド（`from_expansion`） | syn にも見えないので、同じく対象外 |
| マクロ呼び出しの引数に書かれたアイテムやパターン（利用者のトークンで、展開後の AST では検査された可能性がある） | syn はトークン列のまま持つので見えない。**旧 lint より検出が減る** |
| `macro_rules!` の本体 | 旧 lint も見ない（定義の名前だけ見る）。同じ |
| `#[cfg]` で外れたコード | 旧 lint はコンパイルされるコードだけを見た。新実装は cfg を評価せず、全部を見る |
| `Item::Verbatim`（`macro` 宣言など syn が型を持たないもの） | 名前を取れないので名前は検査しない。ファイルの「最初のアイテム」には数える |

型情報や名前解決が要るものは次のとおり。

- **名前だけの束縛パターン。** `Foo` のような識別子のパターンが、束縛か、定数・単位構造体・単位バリアントかは
  名前解決でしか決まらない。旧 lint は `PatKind::Binding` だけを見ていた。新実装は `mut`、`ref`、`@` が付いて
  いれば束縛とみなし、付いていなければ大文字で始まる名前をパスとみなす（下の「要判断」）
- **`impl Trait` を引数に書いたときの合成ジェネリクス。** 下の「要判断」

## 例外規定と抑制

- 旧 lint に、サフィックスの許容条件（小さい型、親型限定など）は無い。外部 API 由来の名前は
  `#[allow]` で許可する運用だったので、コメント指定に置き換えた
- 抑制した名前も、他の違反の判定に影響しない（旧 lint と同じ）

## 既知の限界

- 上の「写せなかったもの」のとおり、マクロ呼び出しの引数の中は見えない
- cfg を評価しない。`#[cfg(test)] mod tests { ... }` の中も検査する
- 旧 lint は、`async fn` のように脱糖で同じパターンが複数のノードになる場合に、
  重複して報告した可能性がある。新実装は構文上の 1 か所につき 1 件

## 要判断

1. **`impl Trait` 引数の合成ジェネリクス。** 旧 lint の `check_generic_param` は、`fn f(x: impl TaskManager)` の
   脱糖で作られる合成パラメータも受けるはずで、その名前は `impl TaskManager` のような型の文字列になり、
   末尾が禁止サフィックスなら「`impl TaskManager` (generic parameter)」と報告される。
   rustc を動かして確かめられていない推定であり、利用側の型を指摘する効果しかなさそうなので、
   新実装は写していない。写すなら、引数位置の `impl` 型の最後のトークンで Camel 判定をする
2. **大文字で始まる名前だけのパターンをパスとみなす点。** 旧 lint に無い条件だが、名前解決の代わりとして
   必要になった。`use Role::*; match role { Manager => .. }` のような定数・バリアントの利用を、
   変数の違反にしないための近似である。`let Worker_service = ..` のように、大文字で始まる変数を
   `mut` も付けずに宣言したものは見逃す
3. **cfg(test) の扱い。** 旧 lint はテスト用のコードを見なかったが、新実装はファイル内の `#[cfg(test)]` を
   区別しない。fraktor-ng-rs の規約ではテストは別ファイル（`*_test.rs`、除外済み）に置くので、
   実害は小さいと見ている

差異の一覧: 旧 UI のケースは、フィクスチャの `#[allow]` をコメント指定に置き換えた 1 件（`allow_attribute.rs`）
を除き、そのまま全件が一致する。
