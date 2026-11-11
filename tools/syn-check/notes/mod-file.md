# mod-file（旧 mod-file-lint、`no_mod_rs`）

## 旧 lint から写したもの

- `is_mod_rs`。ファイル名が `mod.rs` で、パスに `target` という要素が無いファイルを違反にする
- 報告位置はファイルの 1 行目 1 桁目（旧 `first_line_span`）

## 写せなかったもの、変えたもの

- 旧 lint は、コンパイル対象のクレートが取り込んだ `mod.rs` だけを見た。新チェッカーは、
  検査対象のパスの下にある `mod.rs` をすべて見る。どこからも取り込まれていない `mod.rs` も違反になる
- 旧 lint の抑制は、取り込み側のクレートの `#![allow(no_mod_rs)]` だった（UI テスト `allowed_override`）。
  コメント指定はファイル単位なので、`mod.rs` 自身の先頭に `// syn-check: allow-file(mod-file) -- <理由>` を書く

## 既知の限界

- なし（ファイルシステムの判定だけで決まる）

## 要判断

- `tests/common/mod.rs` のようなテスト補助の扱い。旧コーパスでは 4 件がこの形だった。
  暫定では違反にし、必要なら `allow-file` で個別に許可する。ディレクトリ単位の除外設定は入れていない
