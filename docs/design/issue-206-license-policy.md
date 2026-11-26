# 設計：#206 ライセンス表記とヘッダー運用の再設計（方針 C + (a)）

- 対象: [Issue #206](https://github.com/j5ik2o/fraktor-ng-rs/issues/206)、[PR #215](https://github.com/j5ik2o/fraktor-ng-rs/pull/215)（ブランチ `j5ik2o/issue-206-license-notice`、前回レビュー固定 SHA `a778eed`）
- 状態: **方針 C に合わせて作り直し中・設計の独立レビュー待ち**（設計文書と、線引きの一覧（[TSV](./issue-206-file-origin.tsv)）、その判定スクリプト（[`classify_file_origin.py`](../../tools/license-headers/classify_file_origin.py)）だけを置く。ルートファイルの差し替え・ヘッダー適用・ヘッダーツールの改修・スキルの更新は、設計の独立レビュー通過後に実施する）
- **要確認 175 件（うち crates 95 件）の適用は元帥の決定を待つ。決まるまでは Pekko 由来（`Apache-2.0`）として扱う**（§3.3）
- 関連 issue: [Issue #218](https://github.com/j5ik2o/fraktor-ng-rs/issues/218)（移植ファイルごとの原典著作権年範囲の保持）

## オーナー・元帥の決定事項（本設計の前提）

1. **表記は (a)**：ルートのライセンス文書と `NOTICE` を置き、各ソースファイルにライセンスヘッダーを付ける。
2. **ライセンスは C**：このリポジトリで独自に書いた部分は `MIT OR Apache-2.0`、Apache Pekko に由来するファイルは `Apache-2.0` のみ。
3. **`NOTICE` は原典の全文**：Apache Pekko v2.0.0（git `9fbd2283b1cf701c90a1223bf621942c191db61f`）の `NOTICE` 原文（全 74 行）を省略せず全文再掲する。
4. **CI で抜けを防ぐ**：`--check` を CI に組み込み、新しいファイルに `SPDX-License-Identifier` が無い PR は落とす。
5. **迷うファイルの扱い**：Pekko 由来か自作か迷うファイルは安全側（Pekko 由来＝`Apache-2.0`）に倒し、印を付けて元帥へ上げる。

---

## 1. 現状の実測と変更範囲

### 1.1 リポジトリの実測値（`a778eed` 時点）

| 項目 | 実測結果 |
|:---|:---|
| `crates/**/*.rs` 総数 | 324 件（`crates/fraktor-actor` 271 件、`crates/fraktor-testkit` 53 件） |
| うち `crates/*/src/**/*.rs`（`*_test.rs` 除く） | 190 件（`Mirrors \`org.apache.pekko...\`` あり 76 件、`Pekko` 言及 85 件） |
| うち既存移植マーカーあり（`Ported from` / `Port of` / `Checked against Pekko`） | 51 件（すべて `crates/*/tests/`：`fraktor-actor` 43 件、`fraktor-testkit` 8 件） |
| `crates/**/*.rs` の由来（線引き一覧、§3.3） | Pekko 由来 264 件（うち要確認 95 件）、自分で書いた 60 件。`fraktor-actor` に自作 49 件、`fraktor-testkit` に自作 11 件が混ざる |
| `crates/**/*.rs` の既存 `SPDX-License-Identifier` | 0 件 |
| `crates/**/*.rs` の BOM / CRLF / shebang / `#![...]` / 生成ファイル | いずれも 0 件 |
| `crates/**/*.rs` の冒頭 30 行内 `//!`（インナードキュメント） | 246 件 |
| `tools/syn-check/src/**/*.rs` | 33 件（Pekko / Akka / Lightbend への言及 0 件、完全自作） |
| `tools/syn-check/tests/**/*.rs` | 253 件（うち `#![...]` を持つ UI テストフィクスチャ 57 件） |
| `crates/*/Cargo.toml` の `license` 宣言 | どちらも未設定（`publish = false`、`version = "0.0.0"`） |
| `package.json` の宣言 | `"private": true`、`"license": "Apache-2.0"` |
| `origin/main` のコミット名義（全 561 件） | `Junichi Kato <j5ik2o@gmail.com>` 529 件、`j5ik2o <j5ik2o@gmail.com>` 1 件、ローカル自動化名義 31 件（`kinako (Muse)` 28 件・`manager` 3 件）。年はすべて `2026` |
| 原典 `pekko/LICENSE`（`9fbd2283b1`） | 478 行（1〜201 行が Apache-2.0 原文 SHA-256 `c71d239d...`、203〜478 行が Pekko 第三者コード付録） |
| 原典 `pekko/NOTICE`（`9fbd2283b1`） | 74 行（SHA-256 `8188a337ad167b8bb3a2b53868a061130aca0bcc8abff8fd717900cdb444e73e`） |
| `pekko/LICENSE` 付録該当の既存移植 | `crates/fraktor-actor/tests/unsynchronized_byte_array_input_stream_spec.rs`（Apache commons-io、Apache-2.0）。なお [PR #205](https://github.com/j5ik2o/fraktor-ng-rs/pull/205)（OPEN）の `FrequencySketch` は hash-prospector（Unlicense）+ Caffeine（Apache-2.0） |

`publish = false` でも GitHub 公開リポジトリは Source form での配布に当たるため、Apache License 2.0 第 4 条 (a)〜(d) の義務はそのまま適用される。

### 1.2 何をどう変えるか（設計レビュー後に実装する変更の全体像）

| 対象 | 変更内容 |
|:---|:---|
| `LICENSE`（PR #215 で追加済み・`main` 未マージ） | 削除する（`LICENSE-APACHE` と `LICENSE-MIT` へ置き換え） |
| `LICENSE-APACHE`（新規） | 純粋な Apache License 2.0 原文（201 行、SHA-256 `c71d239d...`） |
| `LICENSE-MIT`（新規） | MIT License 原文（`Copyright (c) 2026 Junichi Kato`） |
| `NOTICE`（更新） | `fraktor-ng-rs` の短い前置き＋原典 `pekko/NOTICE`（`9fbd2283b1`）74 行の一字一句同一の全文 |
| `README.md`（更新） | ライセンス節を追加（自作は `MIT OR Apache-2.0`、Pekko 由来は `Apache-2.0`、各ファイルは SPDX で判別） |
| `crates/fraktor-actor/Cargo.toml`、`crates/fraktor-testkit/Cargo.toml` | `license = "Apache-2.0"` と混在の補足コメントを追加 |
| `tools/syn-check/Cargo.toml` | `license = "MIT OR Apache-2.0"` を追加 |
| `package.json` | `"license": "Apache-2.0"` を維持（§4.2 の根拠どおり） |
| `docs/design/issue-206-file-origin.tsv`、`tools/license-headers/classify_file_origin.py`（この設計のコミットで追加済み） | 線引き一覧（404 行）と、それを出す判定スクリプト。ヘッダーツールのコードではない。`--apply --manifest` の入力は、この TSV の `走査範囲 = crates` の 324 行（§3.4.2） |
| `tools/license-headers/license_headers.py` および `_test.py` | 既存 `apply_ported_headers.py` / `_test.py` を置き換え。2 種ヘッダー書き分け、構造化ブロック認識（F1 解消）、厳密マーカー照合（F2 解消）、マーカー非依存 `--remove`（F4 解消）、全ファイル `--check`、BOM/shebang/非 UTF-8/Unicode 改行対応、PBT を実装。`classify_file_origin.py` のマーカー定義もこの版にそろえる（§7.4 の最終行） |
| `crates/**/*.rs`（全 324 件） | 線引き一覧に従い、`Apache-2.0` ヘッダーを 264 件、`MIT OR Apache-2.0` ヘッダーを 60 件に一括適用。**要確認の 95 件は元帥の決定を待つ**（§3.3、§8.1 の 4a と 4b） |
| `.github/workflows/ci.yml` | `jobs.check` に `License headers`（`--check`）ステップを追加、`python tests` のテスト名を更新 |

### 1.3 変えないこと・範囲の外

- **原典 `pekko/`**：読み取り専用であり一切変更しない。
- **`tools/syn-check/tests/**/*.rs`（253 件）**：構文・行番号・属性配置を検証する UI テスト入力フィクスチャを含むため、ヘッダー強制（`--check`）の対象外とする。
- **`scripts/aislop-check-bootstrap.mjs`**：`j5ik2o/ci` 正本の無改変写し（SHA-256 固定）であり編集しない。
- **線引き一覧の `走査範囲 = other` の 80 行**（docs 59、ledger 14、tools 4、`.github` 1、`.gitignore` 1、`package.json` 1。いずれも判定は Pekko 由来・要確認）：ヘッダーは付けない（`crates/**/*.rs` 以外は `--check` の対象外）。Pekko のパスや語を引用しているだけの文書や台帳で、転載かどうかを機械で決められないため、元帥が見る材料として一覧に残す。転載が見つかった場合の扱い（`NOTICE` で足りるか、個別の表示が要るか）は、元帥の決定に従って別 issue にする。
- **Python / JS の補助スクリプト（`tools/**/*.py`、`ledger/**/*.py`）および `tools/syn-check/src/**/*.rs`（33 件）**：今回の `--check` の一次対象はライブラリ・テスト本体である `crates/**/*.rs` とする。`tools/syn-check/src/**/*.rs` への SPDX 付与拡大は、同じツールで `--root` や対象ディレクトリを増やすだけで行えるため、必要なら別 issue で扱う（`tools/syn-check/Cargo.toml` の `license = "MIT OR Apache-2.0"` は今回設定する）。
- **ファイルごとの原典著作権年範囲の細分化（[Issue #218](https://github.com/j5ik2o/fraktor-ng-rs/issues/218)）**：今回の Pekko 由来ヘッダーは原典 `NOTICE` と同じ共通年範囲（Lightbend 2009-2022 / ASF 2022-2026）とする。ただし、§3.4 の構造化ヘッダー認識により、将来 #218 でファイル固有の `Copyright` 行へ更新しても二重付与・削除不能が起きない土台を今回作る。
- **`~/.orca/bots/fraktor-ng-rs/skills/fraktor-migration-intake/SKILL.md`**：この設計段階では変更せず、§6 に追記文面の案だけを残す（設計レビュー通過・実装適用時に反映する）。

---

## 2. ルートのファイルと README

### 2.1 `LICENSE-APACHE`

- **中身**：Apache License, Version 2.0 の**純粋な原文**（`pekko/LICENSE` の 1〜201 行、SHA-256 `c71d239df91726fc519c6eb72d318ec65820627232b2f796219e87dcf35d0ab4`）。
- **前レビュー F9 への対応**：前回の PR #215（`cff1190`）は `pekko/LICENSE` の 478 行すべてを複製していたため、203〜478 行の Pekko 第三者コード付録（MurmurHash、scala-library、Netty、Vyukov queue、FrequencySketch、commons-io、Aeron 等）が混入していた。方針 C では自作部分（`MIT OR Apache-2.0`）も `LICENSE-APACHE` を参照するため、`LICENSE-APACHE` は標準の Apache-2.0 原文 201 行のみとする。
- **Pekko 第三者コード付録の扱い**：
  - 現時点で `crates/` に存在する付録該当ファイルは `unsynchronized_byte_array_input_stream_spec.rs`（原典は Apache commons-io、ライセンスは Apache-2.0）のみで、Apache-2.0 以外の追加ライセンス条文は不要である。
  - 今後 [PR #205](https://github.com/j5ik2o/fraktor-ng-rs/pull/205) の `FrequencySketch`（hash-prospector：Unlicense、Caffeine：Apache-2.0）や BSD 系コードなど、`pekko/LICENSE` 203 行目以降に記載のある第三者コードを移植・取り込みするときは、その PR で該当の第三者ライセンス条文・著作権表示を `LICENSE-THIRD-PARTY`（または該当ファイルのヘッダーおよび `NOTICE` 付記）に保持し、必要なら `Cargo.toml` の `license` 式も更新する（§6 の手順に明記）。

### 2.2 `LICENSE-MIT`

- **著作権者の実測根拠**：`git shortlog -sne origin/main` の結果は `Junichi Kato <j5ik2o@gmail.com>` 529 件、`j5ik2o <j5ik2o@gmail.com>` 1 件、ローカル自動化ボット名義 31 件（`kinako (Muse) <kinako@muse.local>` 28 件、`manager <manager@local>` 3 件）。コミット年はすべて `2026`。個人名義は `Junichi Kato` で統一されている。
- **文面（標準 MIT License 原文）**：

```text
MIT License

Copyright (c) 2026 Junichi Kato

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

### 2.3 既存の `LICENSE`（削除する）

- **決定**：PR #215 の `cff1190` で置いたルートの単一 `LICENSE` は**削除する**（`LICENSE-APACHE` と `LICENSE-MIT` の 2 ファイルに置き換える）。
- **理由**：
  1. 方針 C は単一ライセンスではなく「自作は `MIT OR Apache-2.0`、Pekko 由来は `Apache-2.0`」であるため、単一の `LICENSE` を残すとリポジトリ全体が Apache-2.0 単独なのか `LICENSE-APACHE` との関係はどうなのかが曖昧になる。
  2. Rust エコシステムの標準慣例は、MIT / Apache-2.0 デュアルライセンスを含むリポジトリで単一 `LICENSE` を置かず `LICENSE-APACHE` と `LICENSE-MIT` を並置することである。
  3. 既存 `LICENSE` は `main` 未マージであり、いま削除しても `main` 履歴には残らない。
- **不可逆性（前レビュー F3 への明記）**：`LICENSE-APACHE` と `LICENSE-MIT` を公開の `main` にマージした時点で、その時点のソースに対する許諾は Apache-2.0 第 2 条（perpetual, irrevocable）および MIT ライセンスの性質上**取り消せない**。後日ファイルを削除しても既に取得された写しへの許諾は残る。

### 2.4 `NOTICE`

- 決定どおり、Apache License 2.0 第 4 条 (d) に基づき、冒頭に `fraktor-ng-rs` が Apache Pekko の派生物である旨と原典のバージョン・固定 SHA を示す短い前置きを置き、区切り線の下に原典 `pekko/NOTICE`（v2.0.0、git `9fbd2283b1cf701c90a1223bf621942c191db61f`、全 74 行）を**1 バイトも省かず原文そのまま全文再掲**する。
- 前回 PR #215 にあった「省略した節（MurmurHash 等）」の記述は削除する。
- 構成イメージ：

```text
fraktor-ng-rs

This product is a Rust port of parts of Apache Pekko
(https://pekko.apache.org/) and contains source code derived from it.

The following is the complete, unmodified NOTICE file from Apache Pekko v2.0.0
(git 9fbd2283b1cf701c90a1223bf621942c191db61f), reproduced pursuant to Section
4(d) of the Apache License, Version 2.0.

========================================================================

<pekko/NOTICE (9fbd2283b1cf701c90a1223bf621942c191db61f) 全 74 行をそのまま貼付>
```

（テストまたは検査で、`========...` の直後の空行以降が原典 `pekko/NOTICE` の SHA-256 `8188a337...` と一致することを確認できるようにする。）

### 2.5 `README.md` の「ライセンス」節

現在の `README.md`（1 行のみ）に、以下の節を追加する。

```markdown
## License / ライセンス

This repository uses two licenses depending on the origin of each file:

- **Original code authored in this repository**: dual-licensed under either
  [MIT](./LICENSE-MIT) or [Apache-2.0](./LICENSE-APACHE) (`MIT OR Apache-2.0`),
  at your option.
- **Files derived from or ported from Apache Pekko (and Akka)**: licensed solely
  under [Apache-2.0](./LICENSE-APACHE). See [NOTICE](./NOTICE) for required
  upstream attribution notices.

You can tell which license applies to any individual source file from the
`SPDX-License-Identifier` header at the top of the file (`MIT OR Apache-2.0` or
`Apache-2.0`).

Crates that include Apache Pekko-derived code (`fraktor-actor`, `fraktor-testkit`)
declare `Apache-2.0` as their crate-level effective license in `Cargo.toml`, even
though some files in them are original and carry `MIT OR Apache-2.0`.

---

このリポジトリのライセンスはファイルの由来によって分かれています。

- **このリポジトリで独自に書いたファイル**：`MIT OR Apache-2.0`（[LICENSE-MIT](./LICENSE-MIT) または [LICENSE-APACHE](./LICENSE-APACHE) のいずれかを選択できます）。
- **Apache Pekko（および Akka）に由来・移植したファイル**：`Apache-2.0` のみ（[LICENSE-APACHE](./LICENSE-APACHE) および [NOTICE](./NOTICE) を参照してください）。

各ファイルがどちらに該当するかは、ファイル先頭の `SPDX-License-Identifier` ヘッダー（`MIT OR Apache-2.0` または `Apache-2.0`）で確認できます。
```

---

## 3. ファイル先頭ヘッダーとツールの改修

### 3.1 各ファイルの先頭ヘッダー（正確な文面 2 種）

#### 種別 1：Pekko 由来ファイル（`kind = pekko`、`Apache-2.0` のみ）

Apache License 2.0 第 4 条 (b)（変更したことを示す目立つ表示）および第 4 条 (c)（原典の著作権・帰属表示の保持）を満たす。

```rust
// SPDX-License-Identifier: Apache-2.0
//
// Derived from Apache Pekko, which was derived from Akka.
// Copyright (C) 2009-2022 Lightbend Inc. <https://www.lightbend.com>
// Copyright 2022-2026 The Apache Software Foundation
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.
```

- 10 行（＋本文との区切り空行 1 行＝非空ファイルで計 11 行増加）。最大行長は 81 文字で `rustfmt.toml` の `comment_width = 100` 以内。
- 前回 PR #215 の `See LICENSE and NOTICE` から `See LICENSE-APACHE and NOTICE` へ改める。
- 迷うファイルもオーナー決定どおりこの種別（`pekko`）を適用する。

#### 種別 2：自作ファイル（`kind = original`、`MIT OR Apache-2.0`）

```rust
// SPDX-License-Identifier: MIT OR Apache-2.0
//
// Copyright (c) 2026 Junichi Kato
//
// Licensed under either of Apache License, Version 2.0 or MIT license
// at your option. See LICENSE-APACHE and LICENSE-MIT at the repository root.
```

- 6 行（＋本文との区切り空行 1 行＝非空ファイルで計 7 行増加）。最大行長は 77 文字で `comment_width = 100` 以内。

### 3.2 判定の入力：線引き一覧とファイル内 SPDX の責務分離

「既存ファイルと新規ファイルの分類を、線引き一覧（マニフェスト）とファイル内の印のどちらで判定するか」について、**初期一括適用は「線引きの一覧」、日常運用・CI `--check` は「ファイル先頭の SPDX ヘッダー自体」＋「マーカー矛盾検知」**とする二段階の責務分離を採用する。

| 案 | 利点 | 欠点 | 採否 |
|:---|:---|:---|:---:|
| (1) 常にマニフェストのみを正とする | 1 ファイルで全件を一覧できる | 新規ファイルを追加する全 PR が同じ 1 つのマニフェストを編集することになり、並行開発で常にマージコンフリクトする | 不採用 |
| (2) 常にファイル内の既存マーカーのみを正とする | マニフェスト不要 | 実装ファイル 190 件には統一マーカーが無く、文字列検索だけでは誤分類（偽陰性・偽陽性）が避けられない | 不採用 |
| (3) **初期適用は線引き一覧、運用と CI はファイル内 SPDX を正としつつマーカー矛盾を検知** | 既存 324 件は人手・調査で精査した線引き一覧から正確に一括適用でき、運用後は各 PR が自分の新規ファイルに SPDX を書くだけでコンフリクトしない。しかも「移植マーカーがあるのに `MIT OR Apache-2.0` と書いた誤り」を CI が機械的に止められる | 初期適用と運用で入力経路が 2 つになる（ただし同一ツール・同一検査規則で閉じる） | **採用** |

- **なぜ「マーカーあり＋`MIT OR Apache-2.0`」だけを矛盾として止め、「マーカーなし＋`Apache-2.0`」は許すのか**：
  - 実装ファイルにはテストのような `Ported from Pekko` マーカーを持たない Pekko 由来ファイルが存在し、また迷うファイルも安全側（`Apache-2.0`）に倒すため、「マーカーなし＋`Apache-2.0`」は正当な状態である。
  - 逆に、移植マーカーまたは `Mirrors \`org.apache.pekko.` を持つファイルに `MIT OR Apache-2.0` を付けるのは、Pekko 由来コードへ無権限に MIT 許諾を付与してしまう重大な誤り（かつ公開後は不可逆）であるため、CI で確実に落とす。
- **日常運用の `--check`（マニフェスト無し）が止められる範囲**：C3 が見るのは強い根拠（移植マーカー、`Mirrors`＋Pekko、原典パス）だけである。線引き一覧（§3.3）の材料 5〜8（台帳、原典同名、Pekko言及、original への言及）しか無いファイルは、日常運用では「自作」と書かれても止まらない。その防御は、(a) 初期適用でこの 324 件を一覧の判定どおりに付けること、(b) `--check --manifest` による照合、(c) 新しい移植の手順（§6）で迷うファイルは `pekko` に倒すこと、で担う。この限界は設計として受け入れる。

### 3.3 線引き一覧（件数・判定の規則・要確認）

線引きの一覧は、別担当（pi）が機械判定で作った。判定の根拠は 1 ファイルずつ TSV に残してあり、人が根拠を見て覆せる。

- 一覧の本体：[`docs/design/issue-206-file-origin.tsv`](./issue-206-file-origin.tsv)（404 行＋見出し。列は `path`、`判定`、`根拠の種類`、`根拠の中身`、`要確認`、`要確認の理由`、`走査範囲`）
- 判定のスクリプト：[`tools/license-headers/classify_file_origin.py`](../../tools/license-headers/classify_file_origin.py)
- 再実行：`PEKKO_REPO=<Pekko v2.0.0 の git リポジトリ> uv run --with pyyaml python3 tools/license-headers/classify_file_origin.py`（既定の出力先は上の TSV）。この版のスクリプトを PR #215 の作業ツリーで流し直し、TSV とバイト単位で一致することを確かめた。ライセンス作業のファイル（`LICENSE*`、`NOTICE`、この文書、TSV、`tools/license-headers/`）は走査から外してあるので、TSV を書き直しても自分自身は増えない。
- 原典は Apache Pekko v2.0.0（`9fbd2283b1cf701c90a1223bf621942c191db61f`）。

#### 件数の内訳

| 走査範囲 | 件数 | Pekko 由来 | 自分で書いた | うち要確認 |
|:---|--:|--:|--:|--:|
| `crates/**/*.rs`（全件を判定） | 324 | 264 | 60 | 95 |
| `crates/` 以外（docs、ledger、tools、`.github`、トップ。材料が当たったものだけ） | 80 | 80 | 0 | 80 |
| **合計** | **404** | **344** | **60** | **175** |

`crates/` 以外の 80 件の内訳は、docs 59、ledger 14、tools 4、`.github/workflows/ci.yml` 1、`.gitignore` 1、`package.json` 1。`tools/syn-check/src/**` は材料が当たらず一覧に入っていない（Pekko の記述が 0 件）。

**SPDX の付け分けの対象は `crates/**/*.rs` の 324 件**（§3.4.1）。`MIT OR Apache-2.0` を付けるのは判定が「自分で書いた」の 60 件、`Apache-2.0` を付けるのは「Pekko 由来」の 264 件。`crates/` 以外の 80 件にはヘッダーを付けない（§1.3）。

| ヘッダー | 件数 | 内訳 |
|:---|--:|:---|
| `Apache-2.0`（Pekko 由来） | 264 | 要確認でないもの 169、**要確認 95（元帥の決定待ち）** |
| `MIT OR Apache-2.0`（自分で書いた） | 60 | すべて要確認でない |
| 合計 | 324 | |

クレートべつ（`crates/**/*.rs`）：

| クレート | 全件 | Pekko 由来 | うち要確認 | 要確認でない Pekko 由来 | 自分で書いた |
|:---|--:|--:|--:|--:|--:|
| `crates/fraktor-actor` | 271 | 222 | 85 | 137 | 49 |
| `crates/fraktor-testkit` | 53 | 42 | 10 | 32 | 11 |
| 合計 | 324 | 264 | 95 | 169 | 60 |

`crates/` の種別べつ（`tests/` は統合テスト、`*_test.rs` は `src` の下の単体テスト、`src` 実装はそれ以外）：

| 種別 | Pekko 由来（要確認でない） | Pekko 由来（要確認） | 自分で書いた |
|:---|--:|--:|--:|
| `crates/*/tests/*.rs` | 51 | 18 | 6 |
| `crates/*/src/**/*_test.rs` | 1 | 31 | 27 |
| `crates/*/src/**`（実装） | 117 | 46 | 27 |

#### 根拠の種類べつ

| 根拠の種類 | crates | crates 以外 | 合計 | 要確認 |
|:---|--:|--:|--:|--:|
| 移植マーカー | 51 | 0 | 51 | 0 |
| Mirrors | 115 | 1 | 116 | 1 |
| 原典パス | 5 | 49 | 54 | 51 |
| Mirrors（原典の明記なし） | 13 | 0 | 13 | 13 |
| 原典への言及（Pekko の明記なし） | 18 | 3 | 21 | 21 |
| 原典同名 | 27 | 0 | 27 | 27 |
| 原典同名（型名） | 3 | 0 | 3 | 3 |
| Pekko言及 | 32 | 27 | 59 | 59 |
| なし（自分で書いた） | 60 | 0 | 60 | 0 |

#### 判定の規則

材料は次の 8 つ。ファイルごとに、上から順に当たった 1 つを根拠として記録する。

1. **移植マーカー**：冒頭 20 行の `Ported from Pekko` / `Port of Pekko` / `Checked against Pekko`。現在のスクリプトは `apply_ported_headers.py` と同じ定義（大文字小文字を区別せず、語の境界を見ない）で、同スクリプトが付けたヘッダーを除いた本文を見る。F2 の直しは、ヘッダーツールの改修と同時にスクリプトへ反映する（§7.4 の最終行）。
2. **Ported from / Port of**：出現行が Pekko に触れているものだけを原典の対応とみなす（旧 Rust ツールからの移植を拾わないため）。
3. **Mirrors**：`Mirrors org.apache.pekko.X` などの対応の記述。ファイルのどこかに Pekko の記述があるときだけ採用する。
4. **原典パス**：`org.apache.pekko.X` / `org/apache/pekko/...` の形のパス。
5. **台帳**：`ledger/nodes.yaml` の `rust.test_file`（`retired` も含む、52 件）。52 件すべてが一覧に入っている。
6. **原典同名 / 原典同名（型名）**：ファイル名（拡張子抜き）か、宣言している公開型の名前が、原典のファイル名・型名と一致する。
7. **Pekko言及**：コメント行（`.rs`）または本文に `Pekko` の語があるだけ。
8. **原典への言及（Pekko の明記なし）**：`the original` / `upstream` への言及があるが、それが Pekko だと書かれていない。

判定は 2 値で、「判断できない」は使わない。

- **Pekko 由来**：材料 1〜5 のいずれかが当たったもの。材料 6〜8 しか当たらないものも、迷うものとして Pekko 由来に倒す。
- **自分で書いた**：どの材料も当たらなかったもの。

**要確認**は、Pekko 由来のうち、材料 1〜4（強い根拠）だけでは確定しないもの。`crates/` では、材料 5〜8 しか当たらない 93 件と、強い根拠はあるが台帳の `test_file` に無い統合テスト 2 件（計 95 件）である。`crates/` 以外は、転載かどうかを機械で決められないので、材料が当たった 80 件すべてを要確認にしてある。

#### 要確認 175 件の理由べつ

| 要確認の理由 | 件数 | crates | crates 以外 |
|:---|--:|--:|--:|
| 文書/ツール：原典のパスや記述を引く（転載の有無は未確認） | 50 | 0 | 50 |
| Pekko への言及はあるが、原典との対応の明記が無い（設計の参考か写しか未確認） | 32 | 32 | 0 |
| 文書/ツール：Pekko の語が出るだけ（転載の可能性は低い） | 30 | 0 | 30 |
| ファイル内に Pekko の記述が無く、原典に同名のファイルがあるだけ | 27 | 27 | 0 |
| 「original」「upstream」への言及はあるが、原典が Pekko だと明記が無い | 18 | 18 | 0 |
| 「original」への対応は書かれているが、それが Pekko だと明記が無い | 13 | 13 | 0 |
| ファイル内に Pekko の記述が無いが、宣言している公開型と同名の型が原典にある | 3 | 3 | 0 |
| 移植の根拠はあるが台帳の `test_file` に無い（統合テストのみ） | 2 | 2 | 0 |
| **合計** | **175** | **95** | **80** |

要確認の 1 件ずつのパスと根拠は TSV の `要確認` 列が空でない行である。次のコマンドで一覧にできる。

```bash
awk -F'\t' 'NR > 1 && $5 == "要確認" {print $7 "\t" $6 "\t" $1}' docs/design/issue-206-file-origin.tsv | sort
```

#### 要確認の扱い（元帥の決定待ち）

- **要確認の適用は元帥の決定を待つ。決まるまでは Pekko 由来（`Apache-2.0`）として扱う。** 「扱う」とは、分類は `Apache-2.0` の側に置き、`MIT OR Apache-2.0` を付けない、という意味である。決定前に `MIT OR Apache-2.0` を付けることはない（公開後は取り消せないため、§8.2）。
- 要確認の `crates/` の 95 件にヘッダーをいつ付けるか（決定前は付けない／暫定の `Apache-2.0` を先に付ける）も、元帥の決定に含める。ツールの適用は 2 段に分ける（§8.1 の 4a と 4b）。
- 元帥が個別に「自分で書いた」と決めた行は、TSV の `判定` を `自分で書いた` に、`要確認` を空にして直す。`Apache-2.0` から `MIT OR Apache-2.0` へ広げることは、権利者が後から行える（許諾を足すだけで、すでに与えた許諾は狭まらない）。逆の `MIT OR Apache-2.0` から `Apache-2.0` へ狭める変更は、すでに与えた許諾を取り消せないので、効果がない（§8.2）。
- 要確認が `crates/` の `Apache-2.0` に残っても、`Cargo.toml` の `license` 式は変わらない（§4.1：どの要確認の決め方でも、要確認でない Pekko 由来が 169 件残る）。

#### 既知の限界

- 判定はファイル単位で、原典のどの部分を写したかまでは見ていない。Pekko 由来のファイルにも、Rust 向けに書き直した部分が混ざる。どこまでが移植かはファイルごとに人が確かめる必要がある（この確認は別 issue にする）。
- 原典同名（材料 6）は弱い。名称が一致しない移植は拾えない。
- 自分で書いた 60 件は「どの材料にも当たらなかった」ことの結果で、「原典を一切見ずに書いた」ことの証明ではない。迷いが残れば元帥へ上げる。

### 3.4 ツール仕様（`tools/license-headers/license_headers.py`）

既存の `tools/license-headers/apply_ported_headers.py`（および `_test.py`）は移植ファイル専用だったため、方針 C の 2 種ヘッダー対応に合わせて **`tools/license-headers/license_headers.py`**（および `license_headers_test.py`）へ改名・改修する（`main` 未マージのため互換性破壊なし）。Python 標準ライブラリのみで実装する。

#### 3.4.1 走査対象

- `<root>/crates` 配下のすべての `*.rs` ファイル（`target` ディレクトリ配下は除外）。

#### 3.4.2 CLI モード

| モード | 動作 | 終了コード |
|:---|:---|:---|
| 既定（引数なし、dry-run） | 全 `crates/**/*.rs` のヘッダー状態（`apache`, `dual`, `missing`, `invalid`, `conflict`）を集計表示。書き込みなし | 0 |
| `--apply --manifest <path.tsv>` | §3.3 の TSV（[`docs/design/issue-206-file-origin.tsv`](./issue-206-file-origin.tsv)、7 列）を読む。**使う行は `走査範囲 = crates` の 324 行だけ**（`other` の 80 行は読み飛ばす）。`判定` が `自分で書いた` なら `original`（`MIT OR Apache-2.0`）、`Pekko 由来` なら `pekko`（`Apache-2.0`）とする。`要確認` が空でない行は、`pekko` として扱い、**既定では書き込まない（保留）**。`--include-flagged` を付けたときだけ、保留の行に `Apache-2.0` を書く（§8.1 の 4b）。まず全件を事前検証（§7.1 I6：`crates` の行の集合と実ファイルの集合が完全一致、重複なし、`判定` の値が既知、UTF-8 読取可、種別衝突・未知 SPDX なし）し、すべて通った場合のみ未適用ファイルへヘッダーを書き込む | 成功 0 / 検証失敗 1（書き込み 0 件） |
| `--apply --kind {pekko\|original} <path...>` | 指定した個別ファイル（新規移植・新規自作ファイル等）に該当ヘッダーを冪等適用。既に別種別や未知 SPDX がある場合は書き換えずエラー | 成功 0 / 失敗 1 |
| `--remove` | マニフェストやマーカーを見ず、`crates/**/*.rs` のうちツール管理のライセンスヘッダーブロックを持つ全ファイルからヘッダーを除去する（F4 解消）。未知形式の SPDX ブロックは誤削除を防ぐためエラーで止める（事前検証で Fail-first） | 成功 0 / 失敗 1 |
| `--check` | CI 用。`crates/**/*.rs` の全ファイルに対し C1〜C4 を検証する。1 件でも違反があれば違反種別・パス・修正コマンド例を出力して 1 を返す | 全合格 0 / 違反あり 1 |
| `--check --manifest <path.tsv>` | 上記 C1〜C4 に加え、各ファイルの実際の SPDX 種別がマニフェストの期待種別（`Pekko 由来` は `Apache-2.0`、`自分で書いた` は `MIT OR Apache-2.0`。`要確認` の行も `Apache-2.0`）と 1 対 1 で一致することも検証する（初期一括適用の検算用）。保留のまま（ヘッダー未適用の）要確認の行は `held` として数え、`--include-flagged` を付けない限り違反にしない | 全合格 0 / 違反あり 1 |

#### 3.4.3 文言の完全一致に頼らない構造化ヘッダーブロック認識（F1 の解決）

前の実装は `text.startswith(insertion)` というバイト完全一致のみに依存していたため、年号の更新（`2022-2026` → `2022-2027`）、[Issue #218](https://github.com/j5ik2o/fraktor-ng-rs/issues/218) のファイル固有 `Copyright` 行への変更、空行の誤削除（前レビュー E6）のいずれでも `--remove` が動かず、`--apply` が二重に SPDX を付けてしまった。

これを解消するため、ヘッダーの解析を**プレフィックス分離＋構造化ブロック認識**に改める。

1. **改行の厳密分割**：`str.splitlines()` は U+2028 / U+2029 / U+0085 で誤分割するため使わず、`\r\n` と `\n` だけを改行として扱う独自行分割を使う。
2. **プレフィックス分離（BOM と shebang）**：
   - 先頭に UTF-8 BOM（`\ufeff`）があれば `bom` として分離する（BOM は出力時に必ず先頭に保持する）。
   - BOM の直後（1 行目）が `#!/` で始まり `#![` でない場合、その 1 行（末尾改行を含む）を `shebang` として分離する。
     - もし `#!/...` の 1 行だけで改行が無い場合（`crates/` に実在しない境界入力）は、改行補完で往復性が崩れるためエラーとして拒否する。
3. **ライセンスヘッダーブロックの認識（プレフィックス直後から走査）**：
   - 先頭行が `^// SPDX-License-Identifier:\s*(.+?)\s*$` に一致しない場合：**ヘッダー無し（`NONE`）**。
   - 一致した場合、その行から下へ「通常の行コメント（`^//($|[^/!].*)`）」が連続する間を走査する。`///`（外側 doc）、`//!`（内側 doc）、空行、非コメント行に達した時点で走査を止める。
   - **終端アンカーによる切り止め（E6 対策）**：走査中に次の既知の終端行（トリム後）に達したら、その行で**直ちにブロックを閉じる**：
     - Pekko 由来アンカー：`// This file has been modified from the Apache Pekko original.`
     - 自作アンカー：`// at your option. See LICENSE-APACHE and LICENSE-MIT at the repository root.`
     - これにより、人がヘッダー直後の空行を誤って消して `// Ported from Pekko...` と接してしまっていても、アンカー行の直後でヘッダーが終わるため、**既存の出典コメントを絶対に削らない**。
     - また、ヘッダー内部の `Copyright` 行の年号変更・ファイル固有著作権行の追記・`See LICENSE and NOTICE`（旧版）↔ `See LICENSE-APACHE and NOTICE`（新版）の差があっても、先頭行と終端アンカーで同一のライセンスヘッダーブロックとして認識する。
   - **区切り空行の消費**：終端アンカー直後の次の行が空行（`\r\n` または `\n` のみ）であれば、ヘッダー付与時に挿入した区切り空行として**ちょうど 1 行だけ**をヘッダー領域に含める（先頭空行が 2 行以上ある元ファイルでも 1 行しか消さないため往復が保たれる）。直後に空行が無く本文が続く場合（空ファイルへの適用結果、または E6 の空行欠落）は 0 行消費とする。
   - **未知・破損 SPDX ブロック（`UNRECOGNIZED`）**：先頭行が `// SPDX-License-Identifier:` で始まるのに既知の終端アンカーに到達せず終了した場合（手書きの独自フォーマットや途中で壊れたヘッダー）。この状態では、`--apply` は二重付与を避けてエラー停止、`--remove` は本文誤削除を避けてエラー停止、`--check` は不正フォーマットとして失敗させる。
4. **挿入時の空行ルール（空ファイル往復の保証）**：
   - プレフィックス除去後の残り本文 `rest` が空（`rest == ""`）なら `prefix + header + newline`（末尾区切り空行なし）。
   - `rest != ""` なら `prefix + header + newline + newline + rest`（ヘッダーと本文の間に空行 1 行）。
   - これにより、空ファイル・空行のみ・先頭空行あり・末尾改行なし・CRLF・BOM・shebang のすべてで `remove(apply(x, k)) == x` がバイト単位で完全に成り立つ。

#### 3.4.4 `--check` の検査規則（C1〜C4）

`crates/**/*.rs` の各ファイルについて以下をすべて検査する。

- **C1（SPDX 必須・許可式・重複禁止）**：
  - 構造化ヘッダーブロックが存在し、状態が `UNRECOGNIZED` でないこと。
  - SPDX 式が `Apache-2.0` または `MIT OR Apache-2.0` のどちらかであること。
  - 本文（ヘッダー除去後）に `SPDX-License-Identifier:` が再出現しないこと（二重ヘッダー検知）。
  - 非空本文の前に区切り空行が 1 行あること（E6 の空行欠落も検知）。
- **C2（種別ごとの必須要素）**：
  - `Apache-2.0`：ブロック内に `Copyright`、`LICENSE-APACHE`、`NOTICE`、および終端アンカー（変更表示 `This file has been modified from the Apache Pekko original.`）が含まれること。
  - `MIT OR Apache-2.0`：ブロック内に `Copyright`、`LICENSE-APACHE`、`LICENSE-MIT`、および自作終端アンカーが含まれること。
- **C3（移植マーカーとの矛盾検知・F2 対応）**：
  - 本文（ヘッダー除去後）の冒頭 20 行に、大文字小文字を区別し語境界を見る正規表現
    - `\b(Ported from|Port of|Checked against) Pekko\b`
    - または rustdoc の ``Mirrors `org\.apache\.pekko\.``
    が一致するにもかかわらず、SPDX が `MIT OR Apache-2.0` である場合は**失敗（誤分類）**とする。
  - 逆（マーカー無し＋`Apache-2.0`）は正当（実装ファイル・迷うファイルの安全側倒し）として許可する。
- **C4（ファイル健全性）**：
  - UTF-8 として読めること（非 UTF-8 はトレースバックを出さず明示エラーで終了コード 1）。
  - 改行なし shebang 単独行のような破壊的入力でないこと。

### 3.5 前回設計レビュー（`pr215-design-review.md`）全 10 指摘への回答

| # | 重さ | 指摘の要約 | 本設計での対応 | 扱い |
|:---|:---|:---|:---|:---|
| F1 | 中 | 文言完全一致に依存し、年更新・#218・空行削除（E6）で削除不能・二重付与になる | §3.4.3 の構造化ヘッダーブロック認識（`// SPDX-License-Identifier:` ＋終端アンカー行）に変更。旧版・年号変更・#218 の固有 Copyright 行・空行欠落でも二重付与せず `--remove` で安全に除去できる | 直す |
| F2 | 中 | `re.IGNORECASE`・語境界なしで `Imported from Pekko` 等を誤検出 | `re.IGNORECASE` を廃止し `\b(Ported from\|Port of\|Checked against) Pekko\b` に変更。否定テスト（`Imported from Pekko`、`Support of Pekko`、`UNSUPPORTED FROM PEKKO`）を追加。さらに分類の正はマニフェスト＋SPDX へ移行 | 直す |
| F3 | 中 | `LICENSE` 削除で許諾は戻らないのに「可逆」と書いた | §2.3・§8.2 に「`main` マージ後の許諾は取り消せない（Apache-2.0 第 2 条・MIT 恒久許諾）」と明記。可逆範囲（ヘッダー・ツール・未マージ段階）と不可逆範囲（公開許諾）を明確に分けた | 直す |
| F4 | 低〜中 | 適用後にマーカーを書き換えると `--remove` で消えず孤立、`--check` も検出しない | `--remove` はマーカーを見ず全 `crates/**/*.rs` のヘッダーブロックを対象にする。`--check` も全 `crates/**/*.rs` を対象にし、SPDX 欠如・不正・二重・マーカー矛盾をすべて落とす | 直す |
| F5 | 低 | 設計に不変条件・PBT・Quint 要否・壊しうる入力・影響の節が無い | §7 に不変条件（I1〜I6）、PBT の性質（P1〜P6 と除外集合の扱い）、Quint 不要の根拠、壊しうる入力一覧、影響範囲をすべて独立節として整備した | 直す |
| F6 | 低 | 手書きヘッダーも `--remove` が消すのに「付けたヘッダーだけ」と書いた | §3.4.2・§7.1 I1・§8.2 で「ツールが認識するライセンスヘッダーブロックは誰が付けたかによらず除去する」と明記し、I1 に前提条件 `not has_header(x)` を付けた | 直す |
| F7 | 低 | BOM 付きでコンパイル不能、非 UTF-8 でクラッシュ | §3.4.3 で BOM を分離して BOM 直後に挿入・復元（`rustc` でコンパイル可能かつ往復）。非 UTF-8 はパス付きエラーで 1 件も書かず終了コード 1 | 直す |
| F8 | 低 | 変異テストで生き残った項目（窓 21 行目境界、空行の厳密検証、類似ヘッダー判定） | テストに (1) 21 行目のマーカーは C3 対象外、(2) 適用結果のバイト完全一致、(3) 未知・破損・旧版・空行欠落ヘッダーのテスト、(4) F1/F2/F4 の回帰テストを追加する | 直す |
| F9 | 低 | `LICENSE` が `pekko/LICENSE` の第三者付録（203〜478 行）を含む説明不足 | §2.1 で `LICENSE-APACHE` を純粋な Apache-2.0 原文 201 行のみに変更し、Pekko 第三者付録（commons-io や PR #205 の `FrequencySketch` 等）の扱い方針を明記した | 直す |
| F10 | 低 | ヘッダー適用で `docs/goals/issue-105-status-reply.md` 等の行番号参照がずれる | 実測 14 件（`issue-105-status-reply.md` 13 件、`issue-96-supervisor-tree.md` 1 件）を確認。過去のコンパイラ出力・完了記録のスナップショットであり `check-refs` 対象外のため、書き換えずそのまま残す（§7.5） | 採らない（記録として残す） |

---

## 4. `Cargo.toml` と `package.json` の SPDX 式

### 4.1 Pekko 由来を含むクレートの `license` 式：`Apache-2.0` と `Apache-2.0 AND (MIT OR Apache-2.0)` の比較

Pekko 由来ファイル（`Apache-2.0` のみ）と自作ファイル（`MIT OR Apache-2.0`）の両方を含むクレート（`fraktor-actor`、`fraktor-testkit`）について、クレート全体の有効ライセンスは両者の論理積：

\[
\text{Apache-2.0} \land (\text{MIT} \lor \text{Apache-2.0})
\]

となる。候補を比較する。

| 候補 | SPDX 2.1 妥当性 | 論理的正確さ | 評価 |
|:---|:---:|:---:|:---|
| `MIT OR Apache-2.0` | 妥当 | **誤り（不可）** | Pekko 由来コードを含むのにクレート全体を MIT 単独で使えると虚偽表示してしまう |
| `MIT AND Apache-2.0` | 妥当 | **不正確（過剰制限）** | 自作部分は `MIT OR Apache-2.0` なので、利用者が `Apache-2.0` を選べば MIT 条文の同梱は不要。両方の履行を強いるのは誤り |
| `Apache-2.0 AND (MIT OR Apache-2.0)` | 妥当 | 正しい（冗長） | 構文上は正しいが、ブール代数の吸収律により `Apache-2.0` と完全に同値。人間の読み手や一部の簡易ライセンス監査ツールに「MIT 同梱が必要か？」という不要な混乱を生みやすい |
| **`Apache-2.0`**（**採用**） | **妥当** | **厳密に同値かつ最小** | 吸収律 \(A \land (B \lor A) \equiv A\) により `Apache-2.0 AND (MIT OR Apache-2.0)` と数学的・法的に完全同値。クレート全体の利用者が満たすべき条件（Apache-2.0 ＋ `NOTICE`）を最も単純かつ誤解なく示す |

**結論**：Pekko 由来ファイルを含むクレートは **`license = "Apache-2.0"`** を採用する。

- **要確認の決め方に左右されない**：`license = "Apache-2.0"` が正しいのは、クレートに `Apache-2.0` だけのファイルが 1 件でもあるときである（吸収律の前提）。要確認の扱いが元帥の決定待ちでも、要確認でない Pekko 由来（移植マーカーか `Mirrors`、原典パスの強い根拠があるもの）が `fraktor-actor` に 137 件、`fraktor-testkit` に 32 件ある（§3.3）。元帥が要確認をすべて「自分で書いた」と決めても前提は崩れず、`Cargo.toml` の式は変わらない。逆に、1 件も無くなる（クレートが全件自作になる）ことはこの一覧では起きない。
- **自作ファイルが混ざっていることの表し方**：`fraktor-actor` に自作 49 件、`fraktor-testkit` に自作 11 件が混ざるが、クレート単位の `license` は「配布物全体として守る条件」を示すので `Apache-2.0` のままとする。自作ファイルが `MIT OR Apache-2.0` で使えることは、各ファイルの SPDX が正である（クレート全体を MIT で使えるとは書かない）。
- **混在の可視性の補完**：自作ファイルを単独で切り出して利用する場合は `MIT OR Apache-2.0` で使えることが分かるよう、(1) 各 `Cargo.toml` の `license` 行直上にコメントで明記し、(2) `README.md` のライセンス節と (3) 各ファイルの `SPDX-License-Identifier` ヘッダーで明示する。
- **`license-file` は使わない**：Cargo は標準 SPDX 式で表現できる場合 `license` のみを指定することを推奨しており、`license-file` は併用しない。
- **将来の第三者コード追加時**：将来、非 Apache-2.0 の条件付き第三者コード（例：Simplified BSD 等）をクレートに取り込む場合は、その時点で `Apache-2.0 AND BSD-2-Clause` のように `Cargo.toml` の `license` 式を更新する。

### 4.2 各マニフェストの設定一覧

| マニフェスト | 現在の `license` | 新しい `license` | 根拠 |
|:---|:---|:---|:---|
| `Cargo.toml`（ルート仮想ワークスペース） | なし（`[package]` なし） | なしのまま | クレートごとに構成（Pekko 混在か完全自作か）が異なるため、各パッケージの `Cargo.toml` で明示する |
| `crates/fraktor-actor/Cargo.toml` | 未設定 | `"Apache-2.0"` | 線引き一覧（§3.3）の実測で、271 件のうち Pekko 由来 222 件（要確認 85 件を含む）と自作 49 件の混在。吸収律より `Apache-2.0 AND (MIT OR Apache-2.0) == Apache-2.0`。直上コメントで自作ファイルが `MIT OR Apache-2.0` である旨を補足 |
| `crates/fraktor-testkit/Cargo.toml` | 未設定 | `"Apache-2.0"` | 同 53 件のうち Pekko 由来 42 件（要確認 10 件を含む）と自作 11 件の混在。上と同じ理由 |
| `tools/syn-check/Cargo.toml` | 未設定 | `"MIT OR Apache-2.0"` | 全 33 ファイルが完全自作（Pekko 由来 0 件を実測済み） |
| `package.json` | `"Apache-2.0"` | `"Apache-2.0"`（維持） | `private: true`（npm 非公開、CI 用 `aislop` 実行のみ）。リポジトリ全体の有効ライセンスは同じく吸収律により `Apache-2.0` であり整合する |

---

## 5. CI への組み込み（`--check`）

### 5.1 組み込み先ジョブとステップ位置

- `.github/workflows/main-pr-ci.yml` は `.github/workflows/ci.yml` を `workflow_call` で呼び出し、`jobs.check` の成功（および最終ステップ `syn_checker` の成功）を必須チェック `Fraktor CI Gate` でゲートしている。
- したがって、`.github/workflows/ci.yml` の `jobs.check` に次を反映すれば、すべての PR と `main` push で必須検査になる。
  1. `python tests` ステップのテスト対象を `tools/license-headers/license_headers_test.py` に更新する（ユニットテスト＋seed 固定 PBT を毎回実行）。
  2. `Pekko baseline` ステップの直後（`check:aislop`・`cargo test` より前）に、次のステップを追加する：

```yaml
      - name: License headers
        run: uv run python3 tools/license-headers/license_headers.py --check
```

- **配置の理由**：Python 標準ライブラリのみで 1 秒未満で完了するため、SPDX ヘッダーの付け忘れを Rust のビルドや `aislop` を待たずに Fail-first で落とせる。

### 5.2 新規ファイルに SPDX が無い PR が落ちる仕組みと失敗時出力

- `--check` は `crates/**/*.rs` の全ファイルを走査するため、新しい `.rs` ファイル（テスト・実装・フィクスチャ問わず）が 1 件でも SPDX ヘッダー無しで追加されると、終了コード 1 で `License headers` ステップが失敗し、`Fraktor CI Gate` も落ちる。
- 失敗時の出力例（開発者・ワーカーがすぐ直せる案内を添える）：

```text
checked: 325  apache: 264  dual: 60  missing: 1  errors: 1
ERROR missing-spdx crates/fraktor-actor/src/kernel/new_feature.rs
Hint: run `python3 tools/license-headers/license_headers.py --apply --kind pekko|original <path>`
```

- 移植マーカーや `Mirrors \`org.apache.pekko.` があるのに `MIT OR Apache-2.0` を付けた場合も `ERROR origin-conflict <path>` で終了コード 1 を返す。

---

## 6. 新しく移植するときの手順（`fraktor-migration-intake` スキル追記案）

※設計レビュー通過前のため、`~/.orca/bots/fraktor-ng-rs/skills/fraktor-migration-intake/SKILL.md` 本体はまだ変更しない。以下を「設計の独立レビュー」または「ケース0件の土台」の次の節として追加する案とする。

```markdown
## ライセンスヘッダーと第三者帰属（方針 C + (a)）

- `crates/**/*.rs` に追加するすべての Rust ファイルの先頭に `SPDX-License-Identifier` ヘッダーを付ける。ヘッダーが無い PR は CI（`tools/license-headers/license_headers.py --check`）で落ちる。
- 2 種類を使い分ける。
  - **Apache Pekko 由来のファイル**（原典のテスト・実装・フィクスチャを移植または参照して書いたファイル）：`Apache-2.0` ヘッダー（出典・著作権・変更表示つき）。`python3 tools/license-headers/license_headers.py --apply --kind pekko <path>` で付与できる。
  - **このリポジトリで独自に書いたファイル**（Pekko に対応物がなく原典を参照せずに書いた補助・アダプタ・独自 PBT 等）：`MIT OR Apache-2.0` ヘッダー。`python3 tools/license-headers/license_headers.py --apply --kind original <path>` で付与できる。
- **迷うファイルは安全側（Pekko 由来＝`Apache-2.0`）に倒す。** 迷ったファイルパスと理由を PR 本文と報告に明記して元帥へ上げる（`MIT OR Apache-2.0` で一度 `main` へ公開すると MIT 許諾を取り消せないため、迷うものを自作側に倒さない）。
- 移植元の原典ファイルが `pekko/LICENSE` の第三者付録（203 行目以降：Unlicense、BSD、Caffeine、commons-io 等）に該当する場合は、該当の著作権表示と第三者ライセンス条文を落とさず保持し（必要に応じて `Cargo.toml` の `license` 式や第三者ライセンス記載を更新し）、設計レビューと PR 本文に明記する。
- 実装ワーカーを起動する作業仕様には、「新規作成した `crates/**/*.rs` に適切なライセンスヘッダーを付け、`python3 tools/license-headers/license_headers.py --check` を通すこと（迷うファイルは `pekko` 側に倒して報告する）」を含める。
```

---

## 7. 不変条件・PBT・Quint 要否・壊しうる入力・影響範囲

### 7.1 守る不変条件

変換関数 `apply(text, kind)`、`remove(text)`、`check_file(text)`（いずれもファイル内容の純粋関数）および CLI 全体について、以下を守る。

| 番号 | 不変条件 |
|:---|:---|
| I1 | **往復性**：ヘッダーを持たず（`parse_header(x).state == NONE`）、かつ「改行を含まない shebang 単独 1 行」でない任意の UTF-8 文字列 `x` と任意の `k ∈ {pekko, original}` に対し、`remove(apply(x, k)) == x`（バイト単位完全一致、BOM・CRLF・先頭空行・末尾改行有無を保持）。 |
| I2 | **冪等性**：有効なヘッダーを既に持つか持たないかに関わらず、同じ種別 `k` での適用は冪等：`apply(apply(x, k), k) == apply(x, k)`。 |
| I3 | **種別衝突・未知 SPDX の拒否**：既に異なる種別のヘッダーを持つ、または `UNRECOGNIZED` な SPDX ブロックを持つ入力に対する `apply` は例外／エラーを返し、入力を 1 バイトも変更しない（二重付与も無断上書きもしない）。 |
| I4 | **マーカー判定の位置不変性**：ヘッダーの適用前後で、本文に対する移植マーカー判定 `has_pekko_marker(strip_body(x))` の結果が変わらない（マーカーが元の 20 行目にあっても適用後に窓の外へ落ちない）。 |
| I5 | **非完全一致ヘッダーの安全な除去と本文非破壊**：先頭行が `// SPDX-License-Identifier:` で既知の終端アンカーを持つヘッダーブロックは、内部の年号・`Copyright` 行・`LICENSE` 参照文言が変わっていても、直後の区切り空行が欠けていても（E6）、`remove` はヘッダーブロックのみを除去し、直後の出典コメントや本文を 1 バイトも削らない。 |
| I6 | **Fail-first の原子性**：`--apply --manifest` および `--remove` は、全対象ファイルの読み込みと検証（マニフェストの `走査範囲 = crates` の行と `crates/**/*.rs` の集合が完全一致、`判定` が既知の 2 値、UTF-8 妥当性・種別衝突なし・未知 SPDX なし）を終えてから書き込みを開始する。1 件でも検証に失敗したら 1 ファイルも書き換えない。 |
| I7 | **要確認の保留**：`--include-flagged` を付けない `--apply --manifest` は、`要確認` が空でない行のファイルを 1 バイトも書き換えない（`MIT OR Apache-2.0` を付けることはどの経路でも無い。保留の行の期待種別は常に `pekko`）。 |
| I8 | **一覧の件数との一致**：`--apply --manifest` の事前検証が数える件数は §3.3 と一致する（`crates` の行 324、`Apache-2.0` 264、`MIT OR Apache-2.0` 60、うち保留 95）。ずれたら（ファイルが増減した、TSV が古い）適用せずに止まる。 |

### 7.2 PBT の性質と除外集合

`tools/license-headers/license_headers_test.py` 内に、外部依存を増やさない seed 固定のランダム生成テスト（標準ライブラリ `random.Random(seed)` で最低 5,000 ケース／テスト、任意で `hypothesis` でも再現可能な純粋関数構造）を組み込む。入力生成には ASCII・日本語・BOM（`\ufeff`）・shebang（`#!/...`）・`#![...]`・`//!`・`///`・CRLF・LF・混在改行・空ファイル・先頭空行・末尾改行なし・U+2028/U+2029・移植マーカー（1 行目／20 行目／21 行目／偽陽性語）・ヘッダー断片を混ぜる。

| 性質 | 内容 |
|:---|:---|
| P1（往復） | `parse_header(x).state == NONE` かつ「改行なし shebang 単独 1 行」でない `x` に対し、`remove(apply(x, k)) == x`。 |
| P2（除外集合の安全保証） | 除外集合（改行を含まない `#!/...` のみの 1 行入力）に対しては、`apply` と `check` が必ずエラーを返し、サイレントな破損や不整合な変換を行わない。 |
| P3（冪等） | `apply(x, k)` が成功して `y` を返すとき、`apply(y, k) == y`。 |
| P4（種別衝突拒否） | `apply(x, pekko)` の結果に対し `apply(..., original)` は必ずエラー。逆も同様。 |
| P5（検査との整合） | `apply(x, k)` の結果 `y` は、`k == original` かつ `has_pekko_marker(x)` の矛盾ケースおよび本文に既存 `SPDX-License-Identifier:` を含むケースを除き、`check_file(y)` が常に合格する。また `remove(y)` は常に `check_file` で不合格になる。 |
| P6（改変ヘッダーの非破壊除去） | 任意の本文 `body`（`// Ported from Pekko` 始まりや `//!` 始まりを含む）の前に、年号・Copyright 行・区切り空行有無を変えた既知アンカー付きヘッダーを前置した入力 `y` に対し、`remove(y)` は `body` を 1 バイトも欠損させず、`apply(y, same_kind)` は二重に SPDX を付けない。 |

### 7.3 Quint の要否

- **不要**（前レビュー F5 でも不要と合意済み）。
- 根拠：本ツールは各ファイルに対して独立に動く純粋なテキスト変換であり、並行処理・ロック・キュー・再試行・再開・反復上限・複数アクター間の相互作用を持たない。ファイル単位の状態は `{NONE, APACHE, DUAL, UNRECOGNIZED}` の 4 値のみであり、単体テストと PBT で網羅的に検証できる。

### 7.4 壊しうる入力の一覧

| 入力 | 起きうる問題 | 本設計での対処 |
|:---|:---|:---|
| **CRLF（`\r\n`）および混在改行** | 読み書き時の改行正規化で往復が壊れる | `open(..., encoding="utf-8", newline="")` で変換なし読み書き。ファイルに `\r\n` が含まれるなら挿入ヘッダーも `\r\n`、無ければ `\n`。本文の改行は一切変えない |
| **UTF-8 BOM（`\xef\xbb\xbf`）** | BOM の前にコメントが入ると `rustc` が `unknown start of token: \u{feff}` で失敗（前レビュー E3） | BOM をプレフィックスとして分離し、BOM の**直後**にヘッダーを挿入・認識・除去する（コンパイル可かつバイト往復） |
| **shebang（`#!/...`）** | 1 行目の前にコメントが入るとスクリプトとして実行不能 | 1 行目が `#!/` かつ `#![` でない場合のみ shebang として分離し、2 行目からヘッダーを挿入・認識・除去する。改行なし shebang 単独 1 行はエラーで拒否（P2） |
| **`#![...]` 内側属性・`//!` インナードキュメント** | ヘッダーと結合して属性や rustdoc が壊れる、または `--remove` が `//!` を誤削除する | 通常の `//` コメント＋空行 1 行を前に置くのは Rust 文法・`rustdoc`・`syn-check`・`rustfmt` いずれも合法（`crates/` の `//!` 246 件で検証）。ブロック認識は `///`・`//!` で即終了するため誤削除しない |
| **既存の SPDX（正常・旧版・年号変更・#218 固有 Copyright・空行欠落・未知式・二重 SPDX）** | 完全一致のみだと二重付与・削除不能（F1 / E1 / E6） | 構造化ヘッダーブロック認識＋終端アンカー（§3.4.3）。未知式・終端欠落はエラー停止。本文中の 2 つ目の `SPDX-License-Identifier:` は `--check` で二重ヘッダーとして落とす |
| **生成ファイル・テストフィクスチャ・正本写し** | 生成物や行番号依存テストやハッシュ固定ファイルを壊す | 走査は `crates/**/*.rs`（`target/` 除外）に限定。現状 `crates/` に生成ファイル・`build.rs` は 0 件。`tools/syn-check/tests/`（253 件）と `scripts/aislop-check-bootstrap.mjs` は対象外。暗黙の `@generated` 自動スキップは抜け道になるため入れない |
| **非 UTF-8 バイト列** | `UnicodeDecodeError` のトレースバックで途中停止（前レビュー E7） | 事前読み込み段階で捕捉し、ファイルパス付きエラーを出して 1 件も書かず終了コード 1 |
| **Unicode 改行（U+2028 / U+2029 / U+0085）** | Python の `str.splitlines()` がこれらで分割し行番号・ブロック境界がずれる | `splitlines()` を使わず `\r\n` / `\n` のみで分割する |
| **空ファイル・空行のみ・先頭空行あり・末尾改行なし** | 区切り空行の過不足で `remove(apply(x)) != x` になる | 空本文なら区切り空行なし、非空本文なら区切り空行ちょうど 1 行を追加・除去する（§3.4.3） |
| **偽陽性マーカー（`Imported from Pekko` 等）・21 行目境界・マーカー改変** | 誤検出（F2）、境界漏れ（F8）、ヘッダー孤立（F4） | 大文字小文字区別＋語境界 `\b`、本文冒頭 20 行厳密窓、`--remove` と `--check` はマーカー有無に依存せず全ファイル走査 |
| **線引きのスクリプトとヘッダーツールのマーカー定義の食い違い** | `classify_file_origin.py` の移植マーカーは、現状 `apply_ported_headers.py` と同じ旧定義（大文字小文字を区別せず、語の境界を見ない）。ツールだけ厳密にすると、一覧の判定とツールの C3 がずれる | ツール改修（§8.1 の 2）で、`classify_file_origin.py` が `license_headers.py` のマーカー判定（`\b(Ported from\|Port of\|Checked against) Pekko\b`）を読み込んで使うようにし、TSV を作り直す。作り直した TSV の件数（移植マーカー 51）と、`--check --manifest` の結果が §3.3 と一致することを確かめる。ずれた行は元帥へ上げる。今の TSV（404 行）は旧定義で作ったが、前回の設計レビューの実測で、旧定義と厳密な定義の結果は `crates/` で同じ 51 件である |

### 7.5 上流・保存済みデータ・公開 API・守りの仕組みへの影響

- **上流（`pekko/`）**：影響なし（変更しない）。
- **公開 API・実行時挙動**：`crates/**/*.rs` への行コメント追加と `Cargo.toml` のメタデータ `license` 追加のみ。Rust の公開型・シグネチャ・実行時動作は変わらない。
- **保存済みデータ・台帳（`ledger/nodes.yaml`）**：台帳はファイルパスのみを保持し行番号を持たないため、`pekko_baseline` / `relocate_nodes` / `report_ledger_1to1` への影響なし。
- **守りの仕組み**：
  - `cargo fmt`（stable / nightly）：ヘッダー行は最大 81 文字（`comment_width = 100` 以内）のため整形差分なし。
  - `syn-check`：`//` 行コメントは AST にも `rustdoc`（`///`/`//!`）ブロックにも影響しない。
  - `aislop`：`crates/` の非テスト実装ファイルの行数は、既に `scripts/aislop-accepted.json` で受容済みの `executor_value.rs`（1,745 行）を除くと最大 622 行（`frequency_list_value.rs`）。`complexity/file-too-large` 閾値（1,100 行超）に新規到達するファイルは 0 件。
  - `docs/goals/` 内の行番号参照：`crates/.../*.rs:行` の参照は全 14 件（`docs/goals/issue-105-status-reply.md` 13 件、`docs/goals/issue-96-supervisor-tree.md` 1 件）が 7〜11 行ずれる。いずれも過去コミット時点のコンパイラ出力や調査ログの記録であり `check-refs` の検査対象外であるため、過去の記録として書き換えずに残す（F10）。

---

## 8. 適用の順番と元に戻し方

### 8.1 適用の順番

| 順 | 作業 | 完了条件（ゲート） |
|:---:|:---|:---|
| 0 | **設計の独立レビュー**（いまここ） | 実装に関わっていない Claude Opus（`claude-opus-5-5`）が本設計をレビューし、全指摘を「直す／別件／採らない」で処理完了 |
| 1 | **線引き一覧の確定** | 一覧（[TSV](./issue-206-file-origin.tsv)）と判定スクリプトは、この設計のコミットで置いた（§3.3 に件数を引用）。**要確認 175 件（`crates/` 95 件）の扱いを元帥が決める**。決まるまでは `Apache-2.0` の側で扱う。ゲートは、元帥の決定が TSV（`判定` と `要確認`）に反映されること |
| 2 | **ツール改修と検証** | `tools/license-headers/license_headers.py` と `license_headers_test.py`（F1〜F8・PBT 含む、`--manifest` の TSV 読み込みと保留の扱い I6〜I8 のテストを含む）を実装し、旧 `apply_ported_headers.*` を削除。`classify_file_origin.py` を新しいマーカー定義にそろえて TSV を作り直し、§3.3 の件数と一致を確かめる（§7.4 の最終行）。テスト全件合格 |
| 3 | **ルートファイル・マニフェスト・README 整備** | `LICENSE` 削除、`LICENSE-APACHE`（201 行）・`LICENSE-MIT`・`NOTICE`（原典 74 行全文）・`README.md`・各 `Cargo.toml` を更新 |
| 4a | **要確認でない 229 ファイルへの適用と全検査** | `python3 tools/license-headers/license_headers.py --apply --manifest docs/design/issue-206-file-origin.tsv` で、`Apache-2.0` を 169 件（Pekko 由来で要確認でないもの）、`MIT OR Apache-2.0` を 60 件（自分で書いたもの）に付ける。要確認の 95 件は保留で、書き換えない。`--check --manifest` は保留 95 件を `held` と数えて 0。`cargo test` / `clippy` / `fmt` / `syn-check` / `aislop` / 全 Python テスト / 台帳検査がすべて 0 |
| 4b | **要確認の 95 ファイルへの適用（元帥の決定後）** | 元帥の決定を TSV に反映する。「自分で書いた」と決めた行は `判定` を直し、残りは `Apache-2.0` として `--apply --manifest ... --include-flagged` で付ける。適用後は `--check` と `--check --manifest`（`--include-flagged` なし）が保留 0 件で 0 になる。**決定前でも、暫定の `Apache-2.0` を付けるかどうかは元帥が決める**（付ければ 4b を先にできるが、付けなければ 5 に進めない） |
| 5 | **CI 組み込みと PR #215 マージ** | 4a と 4b が済んでいること（`--check` は `crates/**/*.rs` の全 324 件に SPDX を求めるので、保留が残るとここで落ちる）。`.github/workflows/ci.yml` に `License headers` ステップを追加して push。必須チェック全緑・未解決スレッド 0 件を確認してマージ（**この時点で許諾が公開発効＝不可逆**） |
| 6 | **移植手順スキルの更新** | `~/.orca/bots/fraktor-ng-rs/skills/fraktor-migration-intake/SKILL.md` へ §6 の手順を反映 |

### 8.2 元に戻し方（可逆範囲と不可逆範囲）

#### 可逆な操作（やり直せる範囲）

- **ヘッダー適用を戻す**：
  ```bash
  python3 tools/license-headers/license_headers.py --remove
  ```
  `crates/**/*.rs` からツール管理のライセンスヘッダーブロック（2 種とも、旧版・改変版含む）をマニフェスト・マーカーに依存せず一括除去する。
- **個別ファイルの分類を付け替える（`main` マージ前）**：
  該当ファイルからヘッダーを除去（または書き換え）し、`--apply --kind pekko|original <path>` を再実行する。TSV を使う場合は、その行の `判定` と `要確認` を直して `--remove` と `--apply --manifest` をやり直す。
- **`main` マージ後に分類を付け替える**：`Apache-2.0` から `MIT OR Apache-2.0` へ広げる変更（要確認だったファイルが自作と分かった場合）は、権利者が許諾を足すだけなので後から行える。逆に狭める変更はできない（下の不可逆の項）。
- **ツール・CI・README・`Cargo.toml` の変更を戻す**：
  通常の `git revert` で戻せる。

#### 不可逆な操作（戻せない範囲・F3 対応）

- **`main` へマージした後のライセンス許諾そのもの**：
  `LICENSE-APACHE`、`LICENSE-MIT`、および各ファイルの `SPDX-License-Identifier` を公開リポジトリの `main` へマージした時点で、そのコミット時点のソースコードに対する許諾は**取り消せない**（Apache License 2.0 第 2 条の perpetual / irrevocable、MIT ライセンスの恒久許諾）。後日ファイルを削除・`git revert` したり、あるファイルを `MIT OR Apache-2.0` から `Apache-2.0` へ狭めたりしても、旧コミット時点で取得された写しに対する許諾は存続する。
- この不可逆性こそが、「迷うファイルは最初から安全側（`Apache-2.0`）に倒し、元帥へ上げる」ルールと「`--check` でマーカー付きファイルへの誤った `MIT OR Apache-2.0` 付与を CI で止める」ルールの根拠である。

---

## 付録：実測コマンド

```bash
# 対象ファイル数と種別
git ls-files '*.rs' | awk -F/ '{print $1"/"$2}' | sort | uniq -c

# origin/main のコミット名義と年
git shortlog -sne origin/main
git log origin/main --format='%ad' --date=format:'%Y' | sort -u

# Apache-2.0 原文（1..201 行）と原典 NOTICE 全文（74 行）の SHA-256
sed -n '1,201p' LICENSE | shasum -a 256
git -C ~/Sources/github.com/j5ik2o/pekko show 9fbd2283b1cf701c90a1223bf621942c191db61f:NOTICE | shasum -a 256

# 線引き一覧の再生成と、書き込み済みの TSV との一致（TSV と同じ内容が出る）
PEKKO_REPO=~/Sources/github.com/j5ik2o/pekko uv run --with pyyaml python3 tools/license-headers/classify_file_origin.py --out-tsv /tmp/origin-check.tsv
cmp /tmp/origin-check.tsv docs/design/issue-206-file-origin.tsv

# 要確認の件数（走査範囲べつ。95 と 80）
awk -F'\t' 'NR > 1 && $5 == "要確認" {print $7}' docs/design/issue-206-file-origin.tsv | sort | uniq -c

# tools/syn-check/src の Pekko 言及有無
rg -i 'pekko|akka|lightbend' tools/syn-check/src/

# docs/ 内の crates/*.rs:行 参照件数
rg 'crates/[^ ]+\.rs:[0-9]+' docs/ | wc -l
```
