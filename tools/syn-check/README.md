# syn-check

fraktor-ng-rs の構造ルールを検査する静的チェッカーである。旧 fraktor-rs が dylint で運用していた lint のうち 10 本を、syn の上に移植した。stable の toolchain で動く。検査対象のコードはコンパイルしない。

状態は proposal である（2026-11-11、ブランチ `proposal/syn-checker`）。「要判断」の節に挙げた点は暫定形で実装してあり、人間の判断を待っている。

## 使い方

```sh
cd tools/syn-check
cargo build --release --locked
./target/release/syn-check <検査するパス>...
```

パスはファイルでもディレクトリでもよい。ディレクトリの下の `.rs` ファイルをすべて検査する。`target` と、名前が `.` で始まるディレクトリは飛ばす。

パスは相対パスで渡す。パスで適用範囲を決めるルールは、渡された形のパスを見る。絶対パスで渡すと、親ディレクトリの名前（`tests` や `target`）が判定に混ざる。

| オプション | 意味 |
|---|---|
| `--layout <ng\|legacy>` | パスで適用範囲を決めるルールが仮定する配置。既定は `ng` |
| `--rules <id,id,...>` | 実行するルールを限定する。既定は全ルール |
| `--format <json\|text>` | 出力の形式。既定は `json` |
| `--list-rules` | ルールの id を出して終わる |

終了コードは 3 通りである。

| 終了コード | 意味 |
|---|---|
| 0 | 違反なし |
| 1 | 違反がある。または、パースできないファイルがある |
| 2 | 使い方の誤り、または入出力のエラー |

JSON の出力は次の形である。

```json
{
  "tool": "syn-check", "version": "0.1.0", "layout": "ng", "clean": false,
  "rules": ["mod-file", "module-examples", "..."],
  "summary": {
    "files_checked": 1, "violations": 1, "violations_by_rule": {"type-per-file": 1},
    "suppressions": 0, "parse_failures": 0, "elapsed_ms": 1
  },
  "violations": [
    {"rule": "type-per-file", "path": "crates/x/src/a.rs", "line": 3, "column": 1, "message": "..."}
  ],
  "suppressions": [
    {"path": "crates/x/src/b.rs", "line": 5, "scope": "next", "rules": ["type-per-file"], "reason": "...", "used": true}
  ],
  "parse_failures": [{"path": "crates/x/src/c.rs", "message": "..."}]
}
```

## ルール

| id | 旧 lint | 何を違反にするか | 配置で変わるか | 移植メモ |
|---|---|---|---|---|
| `mod-file` | mod-file-lint | `mod.rs` という名前のファイル | 変わらない | [notes/mod-file.md](notes/mod-file.md) |
| `module-examples` | module-examples-lint | ライブラリのクレートの `examples/` に置いた example | 変わる | [notes/module-examples.md](notes/module-examples.md) |
| `type-per-file` | type-per-file-lint | 1 ファイルに 2 つ以上ある `pub` の struct、enum、trait | 変わらない | [notes/type-per-file.md](notes/type-per-file.md) |
| `tests-location` | tests-location-lint | 本番ファイルの中のテスト（`mod tests { }`、`#[cfg(test)]` のアイテム、`#[test]`） | 変わらない | [notes/tests-location.md](notes/tests-location.md) |
| `use-placement` | use-placement-lint | ファイルやモジュールの冒頭に無い `use` と `mod`、関数内の `use` | 変わらない | [notes/use-placement.md](notes/use-placement.md) |
| `ambiguous-suffix` | ambiguous-suffix-lint | Manager、Util、Facade、Service、Runtime、Engine で終わる名前 | 変わらない | [notes/ambiguous-suffix.md](notes/ambiguous-suffix.md) |
| `rustdoc` | rustdoc-lint | doc コメントの中の日本語と、`ignore` つきのコードフェンス | 変わらない | [notes/rustdoc.md](notes/rustdoc.md) |
| `module-wiring` | module-wiring-lint | 末端モジュールの直属親以外での再エクスポート、末端モジュールの `pub mod`、`#[path]`、`include!` | 変わらない | [notes/module-wiring.md](notes/module-wiring.md) |
| `port-adaptor-boundary` | port-adaptor-boundary-lint | adaptor の公開 struct が kernel の具象 API をフィールドに持つこと | 変わる | [notes/port-adaptor-boundary.md](notes/port-adaptor-boundary.md) |
| `cfg-std-forbid` | cfg-std-forbid-lint | kernel の `#[cfg(feature = "std")]` と `use std::...` | 変わる | [notes/cfg-std-forbid.md](notes/cfg-std-forbid.md) |

移植メモは、旧 lint のどの関数をどう写したか、写せなかったもの、例外規定、既知の限界、要判断を、ルールごとに書いている。

旧 lint のうち 2 本は入れていない。redundant-fqcn は、`use` のスコープと名前衝突のモデルが要るので見送った。let-underscore-forbid は型情報が要るので syn では作れない。clippy の `let_underscore_must_use` で代替する方針が決まっている。

## 抑制コメント

旧 lint は `#[allow(<lint>)]` 属性で許可していた。stable の rustc は、`#[allow(tool::lint)]` の形をエラー（E0710）にし、素の名前を `unknown_lints` の警告にする。clippy のゲートは `-D warnings` なので、どちらも通らない。そこでコメントに置き換えた。

```rust
// syn-check: allow(type-per-file) -- 外部仕様の型を 1 か所にまとめるため
pub enum Second {}

// syn-check: allow-file(mod-file) -- cargo が tests/common/mod.rs の形を要求するため
```

- `allow` は、次の行（コメント行は飛ばす）から始まる構文要素の全体にかかる。アイテムの上に書けば、その中身にもかかる。旧 `#[allow]` と同じ範囲である。空行をはさむと、どこにもかからない。
- `allow-file` はファイル全体にかかる。
- ルールは `,` で区切って複数書ける。
- 理由（`--` の後ろ）は必須である。理由が無い、ルール名が違う、形が違うコメントは、ルール `suppression` の違反として報告する。その違反は抑制できない。
- doc コメント（`///`、`//!`）の中の指定は読まない。

抑制は監査できる。JSON の `suppressions` に、すべての抑制コメントの場所、対象ルール、理由、その実行で実際に違反を抑えたかどうか（`used`）が入る。`--format text` でも 1 行ずつ出る。

## 配置（layout）

3 つのルールは、ファイルの置き場所で適用範囲が決まる。旧 fraktor-rs と fraktor-ng-rs では配置が違うので、述語を 2 組持つ。

| | `ng`（既定） | `legacy` |
|---|---|---|
| クレートの置き場所 | `crates/<crate>/` | `modules/<crate>/` |
| プラットフォーム非依存のコード | `crates/<crate>/src/kernel.rs` と `src/kernel/**` | lint を有効にしたクレート（ルートの `#![deny(cfg_std_forbid)]`） |
| std 実装のコード | `crates/<crate>/src/adaptor.rs` と `src/adaptor/**` | `modules/<name>-adaptor-std/src/**` |
| kernel（core）からの import | `crate::kernel::...`、`fraktor_<name>::kernel::...` | `fraktor_*_core*::...` |

`legacy` は、旧 UI テストと旧コーパスを判定するために残している。`ng` の述語はすべて暫定形である。

## 検証の結果

### 旧 UI テストとの対応

旧 lint の UI テスト（`tests/ui/*.rs` と `.stderr`）を `tests/ui/legacy/` にそのまま取り込み、新チェッカーの結果と突き合わせた。期待は `.stderr` の診断の位置から機械的に読む。

- 9 本の旧 lint の 65 ケースが、すべて一致した。差異は 0 件、不一致は 0 件である。
- フィクスチャを変更したのは 5 ファイルである。どれも、旧 `#[allow]` 属性か旧形式の許可コメントで通していたケースに、抑制コメントを足したものである。4 ファイルでは空行か旧コメントの行を置き換えたので、行番号は動いていない。残る 1 ファイル（`mod-file/auxiliary/suppressed/mod.rs`）は先頭に 1 行足したが、このファイルに期待出力は無い。`.stderr` は 1 つも変更していない。
- ルールごとの対応表は `tests/ui/legacy/<旧 lint>/RESULTS.md` に、まとめは [tests/ui/legacy/SUMMARY.md](tests/ui/legacy/SUMMARY.md) にある。どちらもテストが生成し、古くなるとテストが落ちる。
- tests-location は旧 UI テストが同梱されていなかった。旧 `lib.rs` の分岐を合成フィクスチャ 16 件で覆った。期待値は旧 `lib.rs` の読解から書いたもので、旧 lint を走らせた結果ではない。

取り込みの経緯は git で追える。最初のコミットが旧 UI テストの verbatim なコピーで、フィクスチャの変更はその後のコミットの差分として見える。

### 旧コーパスでの件数

旧 fraktor-rs（`j5ik2o/fraktor-rs` の `fea328f`）の `modules/` に走らせた。対象は 2,689 ファイルで、パースの失敗は 0 件である。同梱のスパイク（`spike/main.rs`）を同じコーパスでビルドして走らせると、スパイクの報告どおり 4 件、14 件、3 件が再現した。

スパイクの 3 ルールとの比較は次のとおりである。

| ルール | スパイク | 新チェッカー | 差異の理由 |
|---|---|---|---|
| mod-file | 4 件 | 4 件 | 同じ 4 ファイル（`tests/common/mod.rs` が 3 件、`tests/support/mod.rs` が 1 件） |
| type-per-file | 14 ファイル | 4 ファイル | 下の表 |
| use-placement | 3 件 | 248 件 | 3 件はスパイクと同じ箇所。245 件は、スパイクが写していなかった「関数内の `use`」の検出である。248 件すべてがテストかベンチのファイルにある |

type-per-file の 14 ファイルの内訳は次のとおりである。

| ファイル数 | 新チェッカー | 理由 |
|---|---|---|
| 9 | 報告しない | スパイクは `pub type` を数えていた。旧 lint は struct、enum、trait だけを数える。`utils-core/src/sync.rs`（cfg で切り替える同名 alias、スパイクは 6 型と報告）もここに入る |
| 1 | 報告しない | `utils-core/src/sync/shared/shared_bound.rs`。`pub trait SharedBound` が `#[cfg(target_has_atomic = "ptr")]` とその否定の下に 1 つずつある。例外規定が 1 つとして数えた |
| 4 | 報告する | 公開型が 2 つある。旧コードは 4 ファイルとも `#![allow(multiple_type_definitions)]` で許可していた |

全 10 ルールの結果は次のとおりである。「テスト側のパス」は、`tests.rs`、`*_test.rs`、`*_tests.rs` と、`tests`、`benches`、`examples` ディレクトリの下を指す。

| ルール | 件数 | テスト側のパス | それ以外 | それ以外の内訳 |
|---|---|---|---|---|
| `mod-file` | 4 | 4 | 0 | |
| `module-examples` | 0 | 0 | 0 | |
| `type-per-file` | 4 | 0 | 4 | 旧コードが `#![allow(multiple_type_definitions)]` で許可していた 4 ファイル |
| `tests-location` | 0 | 0 | 0 | |
| `use-placement` | 248 | 248 | 0 | |
| `ambiguous-suffix` | 4 | 4 | 0 | |
| `rustdoc` | 64 | 61 | 3 | `cfg(feature = "compression")` で宣言されたファイル 1 つ |
| `module-wiring` | 49 | 44 | 5 | 2 件は旧コードが `#![allow(no_parent_reexport)]` で許可。3 件は `cfg(feature = "compression")` の下 |
| `port-adaptor-boundary` | 0 | 0 | 0 | |
| `cfg-std-forbid` | 7 | 1 | 6 | 6 件とも旧コードが `#[allow(cfg_std_forbid)]` で許可 |
| 合計 | 380 | 362 | 18 | |

読み方は次のとおりである。

- 旧 CI は lib ターゲットだけを lint の対象にしていた。旧 `scripts/ci-check.sh` の dylint の箇所に「既定の `cargo dylint` は cargo check の既定ターゲット（lib のみ）で走るため、`#[cfg(test)]` モジュール（兄弟 `tests.rs` を含む）はどの lint からも visit されない」とある。テスト側のパスの 362 件は、旧 lint が見ていなかったコードにある。
- それ以外の 18 件のうち 12 件は、旧コードが属性で明示的に許可していた箇所である。新チェッカーは `#[allow]` 属性を読まないので報告する。
- 残る 6 件は `compression` フィーチャの下にある。このフィーチャは既定で無効なので、既定のビルドではコンパイルされない。旧 CI がこのフィーチャを有効にして lint を走らせたかどうかは確かめていない。
- 旧 CI が検査していたコードで、上のどれにも当てはまらない違反は 0 件である。

再現の手順は次のとおりである。

```sh
git clone https://github.com/j5ik2o/fraktor-rs.git && cd fraktor-rs && git checkout fea328f
<repo>/tools/syn-check/target/release/syn-check --layout legacy modules \
  | python3 <repo>/tools/syn-check/scripts/corpus_breakdown.py
```

「それ以外」の 18 件が旧コードの `#[allow]` とフィーチャのどちらに当たるかは、該当行を読んで分類した。

### verify_green との連動

`ledger/tools/verify_green.py` の `syn_checker` ゲートが、対象クレートのディレクトリに対してこのチェッカーを実行する。`python3 ledger/tools/verify_green_test.py` が、スタブのワークスペースで 3 通りを確かめる。

| 場合 | `syn_checker` | `all_pass` |
|---|---|---|
| 違反の無いクレート | pass | true |
| 公開型が 2 つあるクレート（test、clippy、fmt は pass） | fail | false |
| `tools/syn-check` が無い配置 | not_installed | true |

ゲートは `cargo run --release --locked` でチェッカーを起動する。最初の 1 回はチェッカー自体のビルドが走る。

### 実行時間

旧コーパスの 2,689 ファイルに全 10 ルールを走らせて、1.00 秒から 1.12 秒だった（release ビルド、5 回計測、Apple M4 Pro、wall clock）。同じコーパスでスパイク（パースと 3 ルール）は 0.76 秒から 0.78 秒なので、時間の大半は syn のパースである。処理は 1 スレッドで、並列化はしていない。

チェッカー自体のビルドは、何も無い状態から 12.5 秒だった（release、依存のダウンロードを除く）。

## 限界

syn は構文だけを見る。次のものは見えない。

- 型情報。型に依存する判定は作れない。
- マクロ展開後の姿。マクロが生成するアイテムは検査されない。マクロ呼び出しの引数の中も見ない。
- 名前解決。別名の再エクスポートを経由した利用は追えない。cfg-std-forbid は、`pub use std::sync::Arc as Shared;` を別のモジュールから import する形を検出できない。`use` 以外の位置の `std::` パスも見ない（旧 lint も同じ）。
- cfg の評価。cfg で外れるコードも検査する。旧 lint は、コンパイルされたコードだけを見ていた。旧コーパスでの件数の差は、ほぼすべてここから来る。

ルールごとの限界は、各移植メモの「既知の限界」にある。

## 要判断

暫定形で実装し、人間の判断を待っている点である。

### 全体

1. **CLI の形。** 位置引数でパスを取り、JSON を標準出力に出し、違反があれば 1 で終わる。パースできないファイルも 1 にした。検証できないファイルを合格にしないためである。
2. **抑制の形。** コメント指定（`allow` と `allow-file`）にした。理由を必須にした。使われなかった抑制は `used: false` と出すだけで、違反にはしていない。ディレクトリ単位やクレート単位の除外設定は入れていない。
3. **旧 `#[allow]` 属性を読まないこと。** 旧コードを新チェッカーに通すと、属性で許可されていた箇所が違反になる（旧コーパスで 12 件）。fraktor-ng-rs は旧コードを持ち込まないので、影響は無い見込みである。
4. **cfg で外れるコードも検査すること。** テスト、ベンチ、無効なフィーチャの下のコードも検査する。旧 CI より範囲が広い。ルールごとの扱いは次のとおりである。
   - type-per-file と ambiguous-suffix は、旧実装がテスト側のパスを除外しているので、そのまま除外する。
   - cfg-std-forbid は、旧実装が `cfg(test)` を例外にしているので、`#[cfg(test)]` で宣言されたファイルも例外にした。
   - module-wiring は、`cfg(test)` だけの子モジュールを末端判定で数えない。
   - use-placement、rustdoc、module-wiring の宣言の判定は、旧実装に除外が無いので、テストコードにも適用する。
5. **ng の配置。** kernel を `src/kernel.rs` と `src/kernel/**`、adaptor を `src/adaptor.rs` と `src/adaptor/**` と仮定した。G0a でワークスペースの骨格ができた時点で、実際の配置と合わせる必要がある。
6. **verify_green の起動方法。** `cargo run --release --locked` にした。対象は `cargo metadata` で引いたクレートのディレクトリである。ビルド済みのバイナリを置く形もありうる。

### ルール別

詳細は各移植メモの「要判断」にある。

| ルール | 要判断 |
|---|---|
| `mod-file` | `tests/common/mod.rs` のようなテスト補助を違反にするか |
| `module-examples` | ng で `crates/<crate>/examples/` を禁止するか。旧 `showcases/` に当たる置き場所が ng に無い |
| `tests-location` | sibling `*_test.rs` の規約を ng でも使うか。テキスト走査の癖（コメントや文字列の中も拾う）を残すか |
| `use-placement` | テストコードの関数内 `use` を違反にするか |
| `ambiguous-suffix` | 大文字で始まる名前だけのパターンをパスとみなす近似。引数位置の `impl Trait` を検査するか |
| `rustdoc` | テキスト走査の癖（文字列の中の行頭 `///` も拾う）を残すか |
| `module-wiring` | クレートのルートを判定する近似。`#[cfg_attr(..., path = "...")]` を `#[path]` とみなすこと |
| `port-adaptor-boundary` | ng での「kernel からの import」の読み替え。ほかのクレートの kernel への依存をこのルールで扱うか |
| `cfg-std-forbid` | ng での適用範囲。no_std が未決定なので、暫定では kernel 全体で std を禁止した。`std` フィーチャを ng に置くかどうかも未決定である |

## 開発

```sh
cd tools/syn-check
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

- ルールは `src/rules/<ルール>.rs` に 1 つずつある。`Rule` トレイト（`src/rule.rs`）を実装し、`src/rules.rs` の一覧に足す。
- 旧 UI テストとの照合は `tests/legacy_ui/` にある。フィクスチャを変えたら、`SYN_CHECK_BLESS=1 cargo test --test legacy_ui` で `RESULTS.md` と `SUMMARY.md` を作り直す。
- 旧 UI が覆わない分岐のテストは `tests/<ルール>.rs` と `tests/ui/synthetic/<ルール>/` にある。
- このクレートは独立したワークスペースである。ライブラリのワークスペースには入れない。
