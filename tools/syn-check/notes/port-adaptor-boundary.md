# port-adaptor-boundary（旧 port-adaptor-boundary-lint、`port_adaptor_boundary`）

## 旧 lint から写したもの

- 旧 lint は内部で syn を使ってファイルを読み直していた。判定は旧 `lib.rs` の `collect_violations` 以下をそのまま写した
- 見る対象は、ファイル直下の `pub` の struct のフィールドだけ。インラインモジュールの中の struct、
  `pub(crate)` などの制限つき可視性、enum のフィールドは見ない（旧 `file.items` と `is_public`）
- 「core からの import」は、ファイル直下の `use` だけから集める。glob（`use ...::*`）は数えない。
  別名の import は別名を鍵にし、元の名前で判定する（旧 `collect_core_imports`）
- フィールドの型の中を順に見て、最初に該当した型パスを 1 件だけ報告する。ジェネリクスの引数の中も見る
  （`Arc<ClusterApi>` は違反）。trait object の境界は型パスではないので見ない（`Box<dyn Port>` は可）
- 型パスの分類（旧 `classify_type_path`）
  - core のパスで書かれた型：フィールド名が `inner` で末尾が struct 名と同じなら同名 wrapper、末尾が `Api` で終われば core API
  - 1 セグメントで、core から import した名前：フィールド名が `inner`、名前が `Core<struct 名>`、
    元の名前が struct 名か「struct 名 + `Api`」なら同名 wrapper。元の名前が `Api` で終われば core API
  - それ以外（core でない複数セグメントのパス、import していない名前）は対象外
- 報告位置はフィールドの先頭で、属性（doc コメントを含む）があれば属性の位置（旧 `field.span()`）。
  名前なしフィールドは型の位置になる
- 旧 lint の既定レベルは `Warn` で、適用範囲はパスだけで決まっていた。lint レベルは読まない

### 配置ごとの述語

- `Layout::Legacy`：適用範囲は旧 `is_adaptor_std_source`（パスに `/modules/` と `-adaptor-std/src/` を含む）。
  core のパスは旧 `path_segments_are_external_core`（先頭のセグメントが `fraktor_` で始まり `_core` を含む）
- `Layout::Ng`（暫定形）：適用範囲は `crates/<crate>/src/adaptor.rs` と `crates/<crate>/src/adaptor/**`。
  core のパスは、先頭が `crate` か `fraktor_` で始まる名前で、2 つ目のセグメントが `kernel` のもの
  （`crate::kernel::...`、`fraktor_<name>::kernel::...`）

## 写せなかったもの、変えたもの

- 旧 `is_adaptor_std_source` は絶対パスに対して評価していた。新チェッカーは検査時の相対パスを使うので、
  先頭に `/` を足してから同じ部分文字列を探す。旧 fraktor-rs の直下から `modules/...` を渡しても、旧 lint と同じ結果になる
- 旧 lint は rustc がコンパイルしたファイルだけを見ていた。cfg で外れたモジュールのファイル
  （`#[cfg(test)] mod tests;` の先など）も、新チェッカーは検査する。このルールには cfg(test) の例外規定が無いので、そのまま検査対象にした
- 違反メッセージは旧 lint の主メッセージに、旧 `help` の修正手順を 1 文にして続けた。ng の配置では
  「std adapter」「core」を「adaptor」「kernel」に読み替える
- 名前解決をしないので、`use` で import していない名前の別名（`type Alias = core::FooApi;` を経由した使用など）は見えない。旧 lint も syn で同じ判定をしていたので、結果は同じである

## 例外規定と抑制

- port の trait（`Box<dyn Port>`、`Arc<dyn Port>`）、`Api` で終わらない core の型、core 以外の型は違反にしない（旧 lint のまま）
- 旧 `#[allow(port_adaptor_boundary)]` は、ファイル、struct、フィールドのどのレベルでも、
  `// syn-check: allow(port-adaptor-boundary) -- <理由>` と `allow-file` に置き換える。
  struct の直前のコメントは、その struct のすべてのフィールドを抑制する。ルール側では `#[allow]` 属性を読まない
- 旧 UI の 3 ケースには allow を使うものが無いので、フィクスチャは変更していない

## 既知の限界

- 型情報が無いので、「concrete API か port か」は名前（`Api` の接尾辞、`Core<名前>` の別名、フィールド名 `inner`）だけで決まる。旧 lint と同じである
- `self::` や `super::` から kernel に届く相対パス（`super::super::kernel::Foo`）は、ng の配置でも core のパスと見なさない
- インラインモジュールの中の `use` と struct は見ない（旧 lint と同じ）
- `#[path = "..."]` で読み込まれるファイルは、自分のパスで適用範囲が決まる

## 要判断

- **ng での「core」の読み替え（暫定形）。** 同じクレートの `crate::kernel::...` と、ほかのクレートの
  `fraktor_<name>::kernel::...` を core と見なした。ほかに取りうる形は次のとおり
  - 同じクレートの kernel だけを core とし、ほかのクレートの kernel への依存は別のルール（クレート間の依存方向）で扱う
  - `self::` や `super::` を含む相対パスを、ファイルの位置から解決して kernel に届くかを判定する
  - ng のクレート名の形（`fraktor_<name>`）は `docs/decisions/crate-structure-comparison.md` の案C（`fraktor-actor` など）に合わせた仮定である。クレート名が変われば述語も変わる
- **ng での適用範囲（暫定形）。** `crates/<crate>/src/adaptor.rs` と `adaptor/**` だけを見る。adaptor のモジュール名を
  `adaptor` 以外（`std`、`platform` など）にする場合や、adaptor をクレートの外（別クレート）に置く段階2では、述語を足す必要がある
- **判定の中身はそのまま保った。** 公開 struct のフィールドだけを見て、関数の引数や戻り値、`impl` の中で core の具象 API を使うことは対象にしていない（旧 lint と同じ）。
  ng で port & adapter を全クレートにかけるなら、対象を広げるかは別途の判断になる
