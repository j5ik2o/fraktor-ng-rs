# 設計：#206 ライセンス表記とヘッダー運用の再設計（方針 C + (a)）

- 対象: [Issue #206](https://github.com/j5ik2o/fraktor-ng-rs/issues/206)、[PR #215](https://github.com/j5ik2o/fraktor-ng-rs/pull/215)（ブランチ `j5ik2o/issue-206-license-notice`。設計レビュー 2 回目の固定 SHA は `b6a4d69`、3 回目は `365d6da`）
- 状態: **設計レビュー 3 回目（要修正）の指摘と、元帥の決定（2026-11-26 の第 2 回・第 3 回、前提の 7）を入れた版。変わった点の再レビュー待ち。** 3 回目の指摘への答えは §10 にある。
- **段 B（`MIT OR Apache-2.0` を付ける段）は当面行わない**（オーナーの指示、2026-11-26）。候補の 55 件も、当面は `Apache-2.0` の側（`保留`：ヘッダーを付けず、クレートの `license = "Apache-2.0"` で扱う）に置く。広げるのは、あとから決定のファイルで行える（決定 6）。
- 置くのは、この設計、線引きの一覧（[TSV](./issue-206-file-origin.tsv)）、判定スクリプト（[`classify_file_origin.py`](../../tools/license-headers/classify_file_origin.py) と `origin_*.py`）、人の決定のファイル（[`issue-206-origin-decisions.tsv`](./issue-206-origin-decisions.tsv)）、`MIT OR Apache-2.0` の候補の一覧（[`issue-206-mit-candidates.md`](./issue-206-mit-candidates.md)）だけである。ルートのファイルの差し替え、ヘッダーの適用、ヘッダーツールの改修、スキルの更新は、設計の独立レビューを通してから行う。
- 実測の基準: main（`f54008c`）を取り込んだ作業ツリー。`crates/**/*.rs` は 350 件ある。前の版の 324 件に、main に入った #205（FrequencySketch）、#207、#223 の 26 件が加わった。
- **`MIT OR Apache-2.0` は、まだどのファイルにも付けない。** 候補の 55 件は、実装に関わっていない担当が 1 件ずつ原典と照らし、その結果を元帥が読んでから付ける（§8.1 の段 B）。
- 関連 issue: [Issue #218](https://github.com/j5ik2o/fraktor-ng-rs/issues/218)（移植ファイルごとの原典著作権年範囲の保持）

## オーナー・元帥の決定事項（本設計の前提）

1. **表記は (a)**：ルートのライセンス文書と `NOTICE` を置き、各ソースファイルにライセンスヘッダーを付ける。
2. **ライセンスは C**：このリポジトリで独自に書いた部分は `MIT OR Apache-2.0`、Apache Pekko に由来するファイルは `Apache-2.0` のみ。
3. **`NOTICE` は原典の全文**：Apache Pekko v2.0.0（git `9fbd2283b1cf701c90a1223bf621942c191db61f`）の `NOTICE` 原文（全 74 行）を省略せず全文再掲する。
4. **CI で抜けを防ぐ**：`--check` を CI に組み込み、新しいファイルに `SPDX-License-Identifier` が無い PR は落とす。
5. **迷うファイルの扱い**：Pekko 由来か自作か迷うファイルは安全側（Pekko 由来＝`Apache-2.0`）に倒し、印を付けて元帥へ上げる。
6. **取り消せないのは MIT を付けることだけ。迷ったら `Apache-2.0` に倒す**（元帥、2026-11-26）。この考え方から、次のことが決まった。
   1. **順番**：Pekko 由来（`Apache-2.0`）を先に push する。`MIT OR Apache-2.0` を付けるのは、H1 の規則を足したあとも「自分で書いた」に残ったものだけで、さらに、実装に関わっていない担当が 1 件ずつ原典（Pekko 2.0.x のどのファイルにも、型・関数・テストの形の対応が無いこと）と照らして確かめたものに限る。その一覧（件数と各ファイルの根拠）を元帥が読んでから push する（§8.1）。
   2. **要確認**：一括で `Apache-2.0` のままにする。1 件ずつは見ない。あとで自分で書いたと分かったものは、そのときに 1 件ずつ確かめて広げる（§3.3、[決定のファイル](./issue-206-origin-decisions.tsv) の 1 行目）。
   3. **`Cargo.toml`**：`license = "Apache-2.0"` のまま。前の版の「簡易監査ツールに混乱」という一文は、根拠が無いので消す（§4.1）。
   4. **ヘッダー**：Pekko 由来のファイルには、原典のファイル先頭にある ASF の表示を原文どおり残す。原典のファイルにある第三者の著作権表示も、原文どおり残す（義務。DnsSettings 由来の「Copyright 2017 The Netty Project」など）。自作のファイルの著作権者は `Junichi Kato` に固定する（§3.1）。
   5. **`--check` と決定の置き場**：`--check` は、一覧にある全件と、新しく足すファイルのすべてに効かせる（M1）。人の決定は一覧と別のファイルに置き、一覧を作り直しても上書きしない（M2）。原典が別のライセンスを持つものは、その SPDX をそのまま残し、`--check` が許す一覧に入れる（M3。ただし `fast_frequency_sketch.rs` の hash-prospector は、決定 7.1 で SPDX を `Apache-2.0` だけにした）。README に、Rust の二重ライセンスで標準の貢献条項を入れる（M4）。
7. **設計レビュー 3 回目のあとの決定**（元帥、2026-11-26 の第 2 回・第 3 回。オーナーの指示を含む）。
   1. **`fast_frequency_sketch.rs` の SPDX は `Apache-2.0` だけにする。** Unlicense の部分の出どころは hash-prospector（public domain、Unlicense）で、Caffeine は Apache-2.0（Copyright 2015 Ben Manes）である。根拠は原典 `pekko/LICENSE` の 272〜299 行（hash-prospector）と 303〜305 行（Caffeine）、`FrequencySketch.scala` の先頭。hash-prospector の表示は、ファイルの中のコメントと `NOTICE` に残す（§2.1、§2.4）。理由は、原典の Pekko 自身がこの形を取っていること、Unlicense に表示の義務が無いこと、複合の式という決まりを 1 つ増やすより原典に合わせるほうが間違いにくいこと。`Cargo.toml` は `Apache-2.0` のままで、決定 6.3 とぶつからない。決定のファイルにあった `Apache-2.0 AND Unlicense` の提案の行は消した。
   2. **#218 は、適用の PR（原典の表示が実際に写った PR）がマージされてから閉じる。** 閉じる条件は、写した表示を検査する C6（§3.4.4）が入り、原典のファイルが決まらない 85 件を「既知の例外」として理由つきの一覧にすること。その 85 件にも、少なくとも ASF の表示（Apache Pekko の派生物であること）は必ず付ける。既定の表示（Akka 由来の形と `2009-2022`）が本当に当てはまるか分からないものは、一覧に「年と著作権者は未確認」と書く（§1.3、§3.1）。
   3. **`tools/syn-check` の移植元は、オーナー自身のリポジトリ**（公開の `j5ik2o/fraktor-rs` の `lints/`）なので、MIT のまま進める（§8.1 の段 B1）。
   4. **MIT の候補 55 件で「Pekko に対応が無い」と 1 件ずつ決めるのは、muse か Claude の 1 本が行う。** pi は材料を集めるところまで。迷うものは `Apache-2.0` にする。
   5. **段 B は当面行わない**（オーナー、2026-11-26）。ライセンスの作業は範囲を広げない。

---

## 1. 現状の実測と変更範囲

### 1.1 リポジトリの実測値（main `f54008c` を取り込んだ作業ツリー）

対象のファイルは `git ls-files` で数えた（追跡していないファイルは入らない）。

| 項目 | 実測結果 |
|:---|:---|
| `crates/**/*.rs` 総数 | 350 件（`crates/fraktor-actor` 297 件、`crates/fraktor-testkit` 53 件） |
| うち `crates/*/src/**/*.rs`（`*_test.rs` 除く） | 207 件（`Mirrors \`org.apache.pekko...\`` あり 76 件、`Pekko` 言及 86 件） |
| うち既存移植マーカーあり（`Ported from` / `Port of` / `Checked against Pekko`） | 52 件（すべて `crates/*/tests/`：`fraktor-actor` 44 件、`fraktor-testkit` 8 件）。大文字小文字を区別して語の境界を見る厳密な定義でも 52 件 |
| `crates/**/*.rs` の由来（線引き一覧、§3.3） | Pekko 由来 295 件（うち要確認 116 件）、自分で書いた 55 件（`MIT OR Apache-2.0` の候補。照合前） |
| `crates/**/*.rs` の既存 `SPDX-License-Identifier` | 0 件 |
| `crates/**/*.rs` の BOM / CRLF / shebang / 生成ファイル | いずれも 0 件 |
| `crates/**/*.rs` の冒頭 30 行内 `//!`（インナードキュメント） | 263 件 |
| 原典のファイル先頭に第三者の著作権表示を持つ移植 | 39 件（Netty 15、Ben Manes 24、scala-library 5、hash-prospector 1。重複あり。§2.1） |
| 原典が ASF 標準の表示（Akka 由来でない形）を持つ移植 | 11 件（§3.1） |
| `tools/syn-check/src/**/*.rs` | 33 件（Pekko / Akka / Lightbend への言及 0 件、完全自作） |
| `tools/syn-check/tests/**/*.rs` | 254 件（うち `#![...]` を持つ UI テストフィクスチャ 57 件） |
| `crates/*/Cargo.toml` の `license` 宣言 | どちらも未設定（`publish = false`、`version = "0.0.0"`） |
| `package.json` の宣言 | `"private": true`、`"license": "Apache-2.0"` |
| `origin/main` のコミット名義（全 593 件） | `Junichi Kato <j5ik2o@gmail.com>` 561 件、`j5ik2o <j5ik2o@gmail.com>` 1 件、ローカル自動化名義 31 件（`kinako (Muse)` 28 件・`manager` 3 件）。年はすべて `2026` |
| 原典 `pekko/LICENSE`（`9fbd2283b1`） | 478 行（1〜201 行が Apache-2.0 原文 SHA-256 `c71d239d...`、203〜478 行が Pekko 第三者コード付録） |
| 原典 `pekko/NOTICE`（`9fbd2283b1`） | 74 行（SHA-256 `8188a337ad167b8bb3a2b53868a061130aca0bcc8abff8fd717900cdb444e73e`）。scala-library と Netty の節を含む |
| リポジトリの公開範囲 | `PUBLIC`（`gh repo view`）。PR ブランチへ push した時点で、誰でもその版を取得できる |

`publish = false` でも GitHub 公開リポジトリは Source form での配布に当たるため、Apache License 2.0 第 4 条 (a)〜(d) の義務はそのまま適用される。

### 1.2 何をどう変えるか（設計レビュー後に実装する変更の全体像）

段は §8.1 の順番を指す。段 A は `Apache-2.0` だけを公開する段、段 B は `MIT OR Apache-2.0` を公開する段である。

| 対象 | 変更内容 | 段 |
|:---|:---|:---:|
| `LICENSE`（PR #215 で追加済み・`main` 未マージ） | 削除する（`LICENSE-APACHE` へ置き換え） | A |
| `LICENSE-APACHE`（新規） | 純粋な Apache License 2.0 原文（201 行、SHA-256 `c71d239d...`） | A |
| `NOTICE`（更新） | `fraktor-ng-rs` の短い前置き＋原典 `pekko/NOTICE`（`9fbd2283b1`）74 行の一字一句同一の全文＋原典 `pekko/LICENSE` の hash-prospector の節（272〜299 行）と Caffeine の節（303〜305 行）の原文（決定 7.1、§2.4） | A |
| `README.md`（更新） | ライセンス節を追加。段 A は `Apache-2.0` の説明だけ、段 B で二重ライセンスと貢献条項（M4）を足す（§2.5） | A、B |
| `crates/fraktor-actor/Cargo.toml`、`crates/fraktor-testkit/Cargo.toml` | `license = "Apache-2.0"` を追加（決定 6.3） | A |
| `docs/design/issue-206-file-origin.tsv`、`issue-206-origin-decisions.tsv`、`tools/license-headers/classify_file_origin.py` と `origin_*.py`（この設計のコミットで置いた） | 線引き一覧（433 行）、人の決定、それを出す判定スクリプト。ヘッダーツールの入力は、この TSV の `走査範囲`、`付けるSPDX`、`原典ファイル`、`第三者の表示` の列（§3.4.2） | 済 |
| `tools/license-headers/license_headers.py` および `_test.py` | 既存 `apply_ported_headers.py` / `_test.py` を置き換え。種別ごとのヘッダーの書き分け（原典の表示の写し、§3.1）、構造化ブロック認識（F1）、厳密マーカー照合（F2）、マーカー非依存 `--remove`（F4）、一覧の全件と新しいファイルへの `--check`（M1）、外部のヘッダーの許可（M3）、BOM/shebang/非 UTF-8/Unicode 改行、PBT を実装する | A |
| `crates/**/*.rs`（`付けるSPDX` が `Apache-2.0` の 295 件） | 原典の表示を写した `Apache-2.0` のヘッダーを付ける（要確認の 116 件を含む。決定 6.2） | A |
| `.github/workflows/ci.yml` | `jobs.check` に `License headers`（`--check --manifest`）を足し、`python tests` のテスト名を更新する | A |
| `LICENSE-MIT`（新規） | MIT License 原文（`Copyright (c) 2026 Junichi Kato`） | B |
| `crates/**/*.rs`（候補 55 件のうち、照合と元帥の確認を通ったもの） | `MIT OR Apache-2.0` のヘッダーを付ける。通らなかったものは `Apache-2.0` | B |
| `tools/syn-check/Cargo.toml` | `license = "MIT OR Apache-2.0"` を追加（移植元の旧 `*-lint` が自作かを確かめてから。L1） | B |
| `package.json` | `"license": "Apache-2.0"` を維持（§4.2） | − |

### 1.3 変えないこと・範囲の外

- **原典 `pekko/`**：読み取り専用であり一切変更しない。
- **`tools/syn-check/tests/**/*.rs`（254 件）**：構文・行番号・属性配置を検証する UI テスト入力フィクスチャを含むため、ヘッダー強制（`--check`）の対象外とする。
- **`scripts/aislop-check-bootstrap.mjs`**：`j5ik2o/ci` 正本の無改変写し（SHA-256 固定）であり編集しない。
- **線引き一覧の `走査範囲 = other` の 83 行**（docs 62、ledger 14、tools 4、`.github` 1、`.gitignore` 1、`package.json` 1。いずれも判定は Pekko 由来・要確認で、決定 6.2 により `Apache-2.0`）：ヘッダーは付けない。ただし `--check --manifest` は、この 83 行にも「一覧と食い違う SPDX を書いていないこと」を確かめる（M1、§3.4.4 の C5）。転載が見つかった場合の扱い（`NOTICE` で足りるか、個別の表示が要るか）は、元帥の決定に従って別 issue にする。
- **Python / JS の補助スクリプト（`tools/**/*.py`、`ledger/**/*.py`）および `tools/syn-check/src/**/*.rs`（33 件）**：`--check` が SPDX を求めるのは `crates/**/*.rs` だけとする。`tools/syn-check/src/**/*.rs` への SPDX 付与の拡大は、同じツールで対象ディレクトリを増やすだけで行えるため、必要なら別 issue で扱う（`tools/syn-check/Cargo.toml` の `license = "MIT OR Apache-2.0"` は段 B で設定する）。
- **原典の著作権年範囲**：前の版は Lightbend の行を一律の `2009-2022` にしていたが、決定 6.4 により、原典のファイル先頭の表示を原文どおり写す（§3.1）。ファイルごとの年範囲（`2018-2022` など）と、Lightbend の行の無い Pekko だけのファイルは、これで原文どおりになる。[Issue #218](https://github.com/j5ik2o/fraktor-ng-rs/issues/218) は、決定 7.2 のとおり、適用の PR（段 A）がマージされてから閉じる。条件は、写した表示を検査する C6（§3.4.4）が入ることと、原典のファイルが決まらない 85 件の「既知の例外」の一覧（理由つき。§3.1）を置くこと。設計だけでは閉じない。
- **`~/.orca/bots/fraktor-ng-rs/skills/fraktor-migration-intake/SKILL.md`**：この設計段階では変更せず、§6 に追記文面の案だけを残す（設計レビュー通過・実装適用時に反映する）。

---

## 2. ルートのファイルと README

### 2.1 `LICENSE-APACHE` と第三者の表示

- **中身**：Apache License, Version 2.0 の**純粋な原文**（`pekko/LICENSE` の 1〜201 行、SHA-256 `c71d239df91726fc519c6eb72d318ec65820627232b2f796219e87dcf35d0ab4`）。
- **前レビュー F9 への対応**：前回の PR #215（`cff1190`）は `pekko/LICENSE` の 478 行すべてを複製していたため、203〜478 行の Pekko 第三者コード付録が混入していた。方針 C では自作部分（`MIT OR Apache-2.0`）も `LICENSE-APACHE` を参照するため、`LICENSE-APACHE` は標準の Apache-2.0 原文 201 行のみとする。

#### 第三者の著作権表示（設計レビュー 2 回目の H2 と、決定 6.4）

前の版の「付録に当たるのは commons-io の 1 件だけ」は誤りだった。原典 `actor/src/main/scala/org/apache/pekko/io/dns/DnsSettings.scala` の 15〜28 行は Netty の著作権表示とライセンス表示で、197 行は `getDefaultNameServers` が Netty の `DefaultDnsServerAddressStreamProvider.java` に基づくと書いている。fraktor の `system_host_dns_configuration.rs` はその部分を移植している。main を取り込んで、FrequencySketch（Ben Manes の Caffeine、hash-prospector、scala-library の MurmurHash3）も入った。

Apache-2.0 第 4 条 (c) は、写した原典にある著作権・帰属の表示を残すことを求める。そこで次のように扱う。

1. **原典のファイル先頭の表示を原文どおりヘッダーへ写す**（§3.1）。第三者の表示も、原典のファイル先頭にあるものはここで写る。対象を決めるのは判定スクリプトで、TSV の `第三者の表示` の列に出す。決め方は次の 3 つで、どれも写しすぎる側に倒してある（表示を余分に残しても害は無い）。
   - 原典のファイル先頭に、ASF と Lightbend 以外の `Copyright` 行を持つ Pekko のファイルを、原典の全ファイルから機械で探す。v2.0.0 では `DnsSettings.scala`（Netty 2017）、`SWARUtil.scala`（Netty 2024）、`FrequencySketch.scala`（Ben Manes 2015）、`Helpers.scala`（Scala、EPFL と Lightbend）、`ORSet.scala`（Basho）、Aeron の 3 件（Real Logic）が当たる。
   - fraktor のファイルの `原典ファイル` にそのファイルがあるか、本文がその型の名前を前方一致で引いていれば（`DnsSettingsValues` も `DnsSettings` として数える）、その表示を写す。`Helpers.scala` は scala-library 由来が `Requiring` だけなので、その名前で見る。
   - モジュールの根（`frequency_sketch.rs`）が写す表示は、その下のファイル（`frequency_sketch/counter_bits.rs` など、本文に原典の名前を書かないもの）にも写す。
2. **原典のファイル先頭に無い第三者の由来**も拾う。本文が `scala.util.hashing.MurmurHash3` か `MurmurHash3.<method>` を引くファイルには scala-library の表示（`Helpers.scala` 18〜25 行と同じ文面。Pekko の `LICENSE` 付録と `NOTICE` の scala-library の節に当たる）を、`hash-prospector` を引くファイルには、原典 `FrequencySketch.scala` 334〜336 行の表示（本文のコメントとして残す。ヘッダーには入れない。決定 7.1）を対応させる。
3. **`NOTICE`（第 4 条 (d)）**：原典 `pekko/NOTICE` の全文に、scala-library と Netty の節がある。全文を写すので、ここは足りている。

実測（main `f54008c` を取り込んだ作業ツリー、Pekko 由来の 295 件）では、第三者の表示を写すファイルは 39 件である（パスは `crates/fraktor-actor/` からの相対。どれも `fraktor-actor` にある）。

- **Copyright 2015 Ben Manes. All Rights Reserved.（FrequencySketch.scala）** — 24 件：`src/kernel/frequency_sketch.rs`、`src/kernel/frequency_sketch/bits.rs`、`src/kernel/frequency_sketch/bits_test.rs`、`src/kernel/frequency_sketch/counter_bits.rs`、`src/kernel/frequency_sketch/counter_matrix.rs`、`src/kernel/frequency_sketch/fast_frequency_sketch.rs`、`src/kernel/frequency_sketch/fast_frequency_sketch_settings.rs`、`src/kernel/frequency_sketch/fast_frequency_sketch_test.rs`、`src/kernel/frequency_sketch/frequency_sketch_error.rs`、`src/kernel/frequency_sketch/frequency_sketch_hasher.rs`、`src/kernel/frequency_sketch/frequency_sketch_settings.rs`、`src/kernel/frequency_sketch/frequency_sketch_value.rs`、`src/kernel/frequency_sketch/frequency_sketch_value_test.rs`、`src/kernel/frequency_sketch/hasher_seal.rs`、`src/kernel/frequency_sketch/murmur3.rs`、`src/kernel/frequency_sketch/reset_size.rs`、`src/kernel/frequency_sketch/sketch_depth.rs`、`src/kernel/frequency_sketch/sketch_width.rs`、`src/kernel/frequency_sketch/string_hash_code.rs`、`src/kernel/frequency_sketch/string_hasher.rs`、`src/kernel/frequency_sketch/string_hasher_test.rs`、`tests/frequency_sketch_allocation_test.rs`、`tests/frequency_sketch_property_test.rs`、`tests/frequency_sketch_spec.rs`
- **Copyright 2017 The Netty Project（DnsSettings.scala）** — 15 件：`src/adaptor/system_host_dns_configuration.rs`、`src/adaptor/system_host_dns_configuration_test.rs`、`src/kernel/io/dns/dns_settings.rs`、`src/kernel/io/dns/dns_settings_config.rs`、`src/kernel/io/dns/dns_settings_config_test.rs`、`src/kernel/io/dns/dns_settings_error.rs`、`src/kernel/io/dns/dns_settings_test.rs`、`src/kernel/io/dns/dns_settings_values.rs`、`src/kernel/io/dns/dns_settings_values_test.rs`、`src/kernel/io/dns/host_dns_configuration.rs`、`src/kernel/io/dns/nameserver_address_error.rs`、`src/kernel/io/dns/nameservers_setting.rs`、`src/kernel/io/dns/ndots_setting.rs`、`tests/dns_settings_spec.rs`、`tests/nameserver_address_parser_spec.rs`
- **Scala（scala-library）: Copyright EPFL and Lightbend, Inc.** — 5 件：`src/kernel/frequency_sketch/frequency_sketch_value.rs`、`src/kernel/frequency_sketch/murmur3.rs`、`src/kernel/frequency_sketch/string_hasher.rs`、`src/kernel/frequency_sketch/string_hasher_test.rs`、`tests/frequency_sketch_property_test.rs`
- **hash-prospector: public domain（Unlicense）** — 1 件：`src/kernel/frequency_sketch/fast_frequency_sketch.rs`

#### 原典が別のライセンスを持つもの（M3、決定 7.1）

- **`fast_frequency_sketch.rs` は `Apache-2.0` だけにする**（決定 7.1）。Unlicense の部分の出どころは hash-prospector（public domain、Unlicense）で、Caffeine（Copyright 2015 Ben Manes）は Apache-2.0 である。前の版は理由を取り違えて書いていた。
  - **hash-prospector 由来の部分が入っているか（原典と照らした結果）**：入っている。原典 `FrequencySketch.scala` の `FastFrequencySketch.rehash`（337〜342 行。直前の 334〜336 行が hash-prospector の表示）は `x = ((x >>> 15) ^ x) * 0xD168AAAD`、`x = ((x >>> 15) ^ x) * 0xAF723597`、`(x >>> 15) ^ x` の 3 段である。fraktor の `fast_frequency_sketch.rs` の `rehash`（131〜136 行）は、同じ 3 段を、同じシフト幅 15 と同じ定数 `0xD168_AAAD`、`0xAF72_3597` で書いている（符号なしの右シフトを `(x as u32) >> 15` に、`*` を `wrapping_mul` にしただけ）。128〜130 行には「A low-bias hash function found by Hash Function Prospector (https://github.com/skeeto/hash-prospector, public domain)」のコメントもある。ほかの crates のファイルに、この定数や hash-prospector への言及は無い（`git grep`）。`frequency_sketch_value.rs` の `rehash` は scala-library の MurmurHash3 で、hash-prospector ではない。
  - **ヘッダー**：原典のファイル先頭にある 3 つの表示（ASF の表示、`Copyright (C) 2021-2022 Lightbend Inc.`、Ben Manes の著作権と Apache-2.0 の文）を原文どおり写す（§3.1）。SPDX は `Apache-2.0`。
  - **本文のコメント**：段 A の適用のときに、128〜130 行のコメントを原典 334〜336 行の 3 行と同じ文面にする（UNLICENSE の URL を含む）。Pekko 自身も、SPDX を足さず、本文のコメントと `LICENSE` の付録でこの表示を済ませている。
  - **`NOTICE`**：原典 `pekko/LICENSE` の hash-prospector の節（272〜299 行）と Caffeine の節（303〜305 行）を原文どおり写す（§2.4）。このリポジトリの `LICENSE-APACHE` は付録を含まないので、`NOTICE` に置く。
- scala-library、Netty、Caffeine は Apache-2.0 なので、SPDX は `Apache-2.0` のままで、表示だけを写す。
- 今後、`pekko/LICENSE` の 203 行目以降にある第三者のコード（Simplified BSD の Vyukov のキューなど）を移植するときは、その PR で原典の表示を写す。SPDX に第三者のライセンスを足す（`Apache-2.0 AND <id>`。決定の式の文法は §3.3）かどうかと、`Cargo.toml` の `license` を変えるかどうか（決定 6.3 は `Apache-2.0`）は、そのときに元帥が決める（§6）。

### 2.2 `LICENSE-MIT`

- **置く時期**：段 B（§8.1）。段 A では置かない。`LICENSE-MIT` を置くこと自体が、MIT の許諾を公開することになるからである。
- **著作権者**：`Junichi Kato` に固定する（決定 6.4）。
- **著作権者の実測根拠**：`git shortlog -sne origin/main` の結果は `Junichi Kato <j5ik2o@gmail.com>` 561 件、`j5ik2o <j5ik2o@gmail.com>` 1 件、ローカル自動化ボット名義 31 件（`kinako (Muse) <kinako@muse.local>` 28 件、`manager <manager@local>` 3 件）。コミット年はすべて `2026`。個人名義は `Junichi Kato` で統一されている。
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

- **決定**：PR #215 の `cff1190` で置いたルートの単一 `LICENSE` は**削除する**（段 A で `LICENSE-APACHE` に、段 B で `LICENSE-MIT` を足す）。
- **理由**：
  1. 方針 C は単一ライセンスではなく「自作は `MIT OR Apache-2.0`、Pekko 由来は `Apache-2.0`」であるため、単一の `LICENSE` を残すとリポジトリ全体が Apache-2.0 単独なのか `LICENSE-APACHE` との関係はどうなのかが曖昧になる。
  2. Rust エコシステムの標準慣例は、MIT / Apache-2.0 デュアルライセンスを含むリポジトリで単一 `LICENSE` を置かず `LICENSE-APACHE` と `LICENSE-MIT` を並置することである。
  3. 既存 `LICENSE` は `main` 未マージである。ただし PR ブランチへは push 済みなので、その版はすでに公開されている（下）。
- **取り消せなくなる時点（設計レビュー 2 回目の H1、前レビュー F3）**：リポジトリは PUBLIC である。**取り消せなくなるのは main へのマージではなく、公開リポジトリのブランチへ push した時点である。** PR ブランチへ push した版は、マージの前でも誰でも取得できる。その時点のソースに対する許諾は、Apache-2.0 第 2 条（perpetual, irrevocable）と MIT ライセンスの性質上、取り消せない。後日ファイルを削除しても、すでに取得された写しへの許諾は残る。
  - `Apache-2.0` の許諾も取り消せないが、方針 C ではどのファイルも少なくとも `Apache-2.0` で出す。だから、新しく取り消せなくなるのは `MIT OR Apache-2.0` を付けることだけである（決定 6）。段 A を先に push し、段 B は照合と元帥の確認のあとに回すのは、このためである。

### 2.4 `NOTICE`

- 決定どおり、Apache License 2.0 第 4 条 (d) に基づき、冒頭に `fraktor-ng-rs` が Apache Pekko の派生物である旨と原典のバージョン・固定 SHA を示す短い前置きを置き、区切り線の下に原典 `pekko/NOTICE`（v2.0.0、git `9fbd2283b1cf701c90a1223bf621942c191db61f`、全 74 行）を**1 バイトも省かず原文そのまま全文再掲**する。
- 前回 PR #215 にあった「省略した節（MurmurHash 等）」の記述は削除する。
- **原典 `pekko/LICENSE` の付録から 2 つの節を足す**（決定 7.1）：hash-prospector の節（272〜299 行。Unlicense の全文を含む）と Caffeine の節（303〜305 行）。原典の `NOTICE` の全文の後ろに、もう 1 本の区切り線を置いて、原文どおり写す。`fast_frequency_sketch.rs` の SPDX を `Apache-2.0` だけにする代わりに、表示をここと本文のコメントに残す（§2.1）。
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

========================================================================

The following sections are reproduced unmodified from the LICENSE file of
Apache Pekko v2.0.0 (lines 272-299 and 303-305).

<pekko/LICENSE 272〜299 行（hash-prospector）をそのまま貼付>

<pekko/LICENSE 303〜305 行（Caffeine）をそのまま貼付>
```

（テストまたは検査で、1 本目の `========...` の直後の空行から 2 本目の区切り線の前の空行までが原典 `pekko/NOTICE` の SHA-256 `8188a337...` と一致し、2 本目の後ろの 2 つの節が原典 `pekko/LICENSE` の該当の行と一致することを確認できるようにする。）

### 2.5 `README.md` の「ライセンス」節

現在の `README.md`（1 行のみ）に節を足す。段 A では `Apache-2.0` のことだけを書き、二重ライセンスの説明と貢献条項は段 B で足す。段 A の README が「自作は MIT でも使える」と書けば、その時点で MIT の許諾を公開したことになるからである。

#### 段 A（`Apache-2.0` だけを公開する段）

```markdown
## License / ライセンス

Files derived from or ported from Apache Pekko (and Akka) are licensed under
[Apache-2.0](./LICENSE-APACHE). See [NOTICE](./NOTICE) for the required upstream
attribution notices. Each source file states its license in the
`SPDX-License-Identifier` header at the top of the file.

Apache Pekko（および Akka）に由来・移植したファイルは `Apache-2.0` です（[LICENSE-APACHE](./LICENSE-APACHE) と [NOTICE](./NOTICE) を参照してください）。各ファイルのライセンスは、ファイル先頭の `SPDX-License-Identifier` ヘッダーで確認できます。
```

#### 段 B（`MIT OR Apache-2.0` を公開する段）で置き換える節

M4（設計レビュー 2 回目）：Rust の二重ライセンスで標準の貢献条項を入れる。これが無いと、Apache-2.0 第 5 条により、外部からの貢献は `Apache-2.0` だけで入り、自作のファイルに付けた `MIT OR Apache-2.0` が正しくなくなる。

```markdown
## License / ライセンス

This repository uses two licenses depending on the origin of each file:

- **Original code authored in this repository**: dual-licensed under either
  [MIT](./LICENSE-MIT) or [Apache-2.0](./LICENSE-APACHE) (`MIT OR Apache-2.0`),
  at your option.
- **Files derived from or ported from Apache Pekko (and Akka)**: licensed solely
  under [Apache-2.0](./LICENSE-APACHE). See [NOTICE](./NOTICE) for the required
  upstream attribution notices.

You can tell which license applies to any individual source file from the
`SPDX-License-Identifier` header at the top of the file.

Crates that include Apache Pekko-derived code (`fraktor-actor`, `fraktor-testkit`)
declare `Apache-2.0` as their crate-level license in `Cargo.toml`, even though
some files in them are original and carry `MIT OR Apache-2.0`.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
Contributions to files that carry `SPDX-License-Identifier: Apache-2.0` are
licensed under Apache-2.0 only.

---

このリポジトリのライセンスはファイルの由来によって分かれています。

- **このリポジトリで独自に書いたファイル**：`MIT OR Apache-2.0`（[LICENSE-MIT](./LICENSE-MIT) または [LICENSE-APACHE](./LICENSE-APACHE) のいずれかを選択できます）。
- **Apache Pekko（および Akka）に由来・移植したファイル**：`Apache-2.0` のみ（[LICENSE-APACHE](./LICENSE-APACHE) および [NOTICE](./NOTICE) を参照してください）。

各ファイルがどちらに該当するかは、ファイル先頭の `SPDX-License-Identifier` ヘッダーで確認できます。

貢献について：特に断らない限り、このリポジトリへ取り込むために意図して提出された貢献（Apache-2.0 の定義による）は、追加の条件なしに上の二重ライセンスで提供されたものとします。`SPDX-License-Identifier: Apache-2.0` のファイルへの貢献は `Apache-2.0` だけで提供されたものとします。
```

---

## 3. ファイル先頭ヘッダーとツールの改修

### 3.1 各ファイルの先頭ヘッダー

#### 種別 1：Pekko 由来ファイル（`kind = pekko`、`Apache-2.0`）

Apache License 2.0 第 4 条 (b)（変更したことを示す目立つ表示）と第 4 条 (c)（原典の著作権・帰属表示の保持）を満たす。決定 6.4 により、**原典のファイル先頭にある表示を、文面を変えずに写す**。前の版は ASF の表示を言い換え、Lightbend の年を一律 `2009-2022` にしていた（設計レビュー 2 回目の L5）。

ヘッダーの組み立て：

```text
// SPDX-License-Identifier: <付けるSPDX。ふつうは Apache-2.0>
//
// <原典ファイルの、package 文より前のコメント。1 行ずつ「// 」を付けて写す。空行は「//」>
// <2 つ目以降の原典ファイルのコメントのうち、まだ出ていないブロック>
// <原典のファイル先頭に無い第三者の表示（scala-library。§2.1 の 2）>
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.
```

- **写す元**：TSV の `原典ファイル` の列（`;` 区切り、強い順）。原典は Pekko v2.0.0（`9fbd2283b1`）の git から読む。決め方は §3.3 の「原典ファイルの決め方」。
- **写し方**：`/* ... */` の各行から先頭の ` * ` だけを外し、行の残り（字下げを含む）は変えない。ブロックの切れ目は `//` 1 行にする。`//` の行コメント（`Helpers.scala` の `//The 'Requiring' class ...`）はそのまま写す。同じ文面のブロックは 1 つにまとめる。
- **原典の表示の形は 2 つある**。どちらも原文どおり写す。
  - Akka 由来の形（原典 2,706 ファイル、移植の 199 件）：`Licensed to the Apache Software Foundation (ASF) under one or more` / `license agreements; and to You under the Apache License, version 2.0:` … `This file is part of the Apache Pekko project, which was derived from Akka.` と、ファイルごとの `Copyright (C) <年> Lightbend Inc.`（`2009-2022` 106 件、`2018-2022` 32 件、`2021-2022` 18 件など 11 通り）
  - ASF 標準の形（Pekko で新しく書かれたファイル、移植の 11 件）：`... under one or more contributor license agreements. See the NOTICE file distributed with this work ...` の 14 行で、Lightbend の行は無い。前の版の一律のヘッダーは、この 11 件に「derived from Akka」と Lightbend の行を書いてしまうところだった。該当：（パスは `crates/fraktor-actor/` からの相対）`src/adaptor/random_id_generator.rs`（IdGenerator.scala）、`src/kernel/io/dns/id_generator.rs`（IdGenerator.scala）、`src/kernel/io/dns/id_generator_policy.rs`（IdGenerator.scala）、`tests/abstract_fsm_with_stash_actor_test.rs`（AbstractFSMWithStashActorTest.java）、`tests/address_spec.rs`（AddressSpec.scala）、`tests/byte_string_builder_scala213_plus_spec.rs`（ByteStringBuilderScala213PlusSpec.scala）、`tests/byte_string_builder_spec.rs`（ByteStringBuilderSpec.scala）、`tests/byte_string_input_stream_spec.rs`（ByteStringInputStreamSpec.scala）、`tests/domain_name_spec.rs`（DomainNameSpec.scala）、`tests/id_generator_spec.rs`（IdGeneratorSpec.scala）、`tests/unsynchronized_byte_array_input_stream_spec.rs`（UnsynchronizedByteArrayInputStreamSpec.scala）
- **原典ファイルが決まらないもの**（Pekko 由来 295 件のうち 85 件。76 件は要確認）：Akka 由来の形の ASF の表示と、原典 `NOTICE` と同じ `Copyright (C) 2009-2022 Lightbend Inc. <https://www.lightbend.com>` を使う。ツールは件数とパスを出す。あとで原典が分かったら `原典ファイル` を埋めて付け直す（ヘッダーの付け直しは §3.4.3 の認識で冪等にできる）。
  - **#218 の「既知の例外」の一覧**（決定 7.2）：この 85 件を、適用の PR（段 A）で理由つきの一覧にする。列は、パス、`根拠の種類`、原典のファイルが決まらない理由（TSV の `要確認の理由`、要確認でない 9 件は「強い根拠はあるが、原典のファイルに行き着かない」）、`年と著作権者`。85 件とも原典のファイルを確かめていないので、`年と著作権者` は「未確認」と書く。ASF の表示（Apache Pekko の派生物であること）は、85 件にも必ず付ける。
- **行の長さ**：写す行は最長 78 文字（`// ` を含めて 81 文字）で、`rustfmt.toml` の `comment_width = 100` 以内なので、`wrap_comments` は折り返さない。100 文字を超える行が出るときは、ツールは適用を断る（折り返されると原文でなくなり、ブロックの認識も崩れる）。
- **長さ**：ヘッダーは 15〜30 行（中央値 15 行）＋区切りの空行 1 行。
- 前回 PR #215 の `See LICENSE and NOTICE` は `See LICENSE-APACHE and NOTICE` に改める。迷うファイル（要確認）もこの種別を使う。

例 1：原典が `actor/src/main/scala/org/apache/pekko/actor/Actor.scala`（Akka 由来の形）のファイル。

```rust
// SPDX-License-Identifier: Apache-2.0
//
// Licensed to the Apache Software Foundation (ASF) under one or more
// license agreements; and to You under the Apache License, version 2.0:
//
//   https://www.apache.org/licenses/LICENSE-2.0
//
// This file is part of the Apache Pekko project, which was derived from Akka.
//
// Copyright (C) 2009-2022 Lightbend Inc. <https://www.lightbend.com>
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.
```

例 2：原典が `DnsSettings.scala` のファイル（`dns_settings.rs` など。Netty の表示がそのまま入る）。

```rust
// SPDX-License-Identifier: Apache-2.0
//
// Licensed to the Apache Software Foundation (ASF) under one or more
// license agreements; and to You under the Apache License, version 2.0:
//
//   https://www.apache.org/licenses/LICENSE-2.0
//
// This file is part of the Apache Pekko project, which was derived from Akka.
//
// Copyright (C) 2018-2022 Lightbend Inc. <https://www.lightbend.com>
//
// Copyright 2017 The Netty Project
//
// The Netty Project licenses this file to you under the Apache License,
// version 2.0 (the "License"); you may not use this file except in compliance
// with the License. You may obtain a copy of the License at:
//
//   https://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS, WITHOUT
// WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the
// License for the specific language governing permissions and limitations
// under the License.
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.
```

`原典ファイル` に DnsSettings.scala が無いのに本文が `DnsSettings…` を引くファイル（`dns_settings_spec.rs` など）は、自分の原典の表示に、DnsSettings.scala の Netty のブロックを足す（§2.1 の 1）。

#### 種別 2：自作ファイル（`kind = original`、`MIT OR Apache-2.0`。段 B で付ける）

```rust
// SPDX-License-Identifier: MIT OR Apache-2.0
//
// Copyright (c) 2026 Junichi Kato
//
// Licensed under either of Apache License, Version 2.0 or MIT license
// at your option. See LICENSE-APACHE and LICENSE-MIT at the repository root.
```

- 6 行（＋本文との区切り空行 1 行＝非空ファイルで計 7 行増加）。最大行長は 77 文字で `comment_width = 100` 以内。
- **著作権者は `Junichi Kato` に固定する**（決定 6.4）。外部の貢献者がいても、この行は変えない。貢献の許諾は README の貢献条項（M4、§2.5）で受ける。
- **年**は、そのファイルを最初に公開した年とする（設計レビュー 2 回目の L6）。`--apply --kind original` は、適用した年（`date +%Y`）を書く。最初の一括適用はすべて `2026` である。あとで年を変えなくても、ヘッダーの認識（§3.4.3）は年によらない。

### 3.2 判定の入力：線引き一覧とファイル内 SPDX の責務分離

「既存ファイルと新規ファイルの分類を、線引き一覧（マニフェスト）とファイル内の印のどちらで判定するか」について、**初期一括適用は「線引きの一覧」、日常運用・CI `--check` は「ファイル先頭の SPDX ヘッダー自体」＋「マーカー矛盾検知」**とする二段階の責務分離を採用する。

| 案 | 利点 | 欠点 | 採否 |
|:---|:---|:---|:---:|
| (1) 常にマニフェストのみを正とする | 1 ファイルで全件を一覧できる | 新規ファイルを追加する全 PR が同じ 1 つのマニフェストを編集することになり、並行開発で常にマージコンフリクトする | 不採用 |
| (2) 常にファイル内の既存マーカーのみを正とする | マニフェスト不要 | 実装ファイル 207 件には統一マーカーが無く、文字列検索だけでは誤分類（偽陰性・偽陽性）が避けられない | 不採用 |
| (3) **初期適用は線引き一覧、運用と CI はファイル内 SPDX を正としつつマーカー矛盾を検知** | 既存 350 件は根拠つきの線引き一覧と人の決定から一括適用でき、運用後は各 PR が自分の新規ファイルに SPDX を書くだけでコンフリクトしない。しかも「移植マーカーがあるのに `MIT OR Apache-2.0` と書いた誤り」を CI が機械的に止められる | 初期適用と運用で入力経路が 2 つになる（ただし同一ツール・同一検査規則で閉じる） | **採用** |

- **なぜ「マーカーあり＋`MIT OR Apache-2.0`」だけを矛盾として止め、「マーカーなし＋`Apache-2.0`」は許すのか**：
  - 実装ファイルにはテストのような `Ported from Pekko` マーカーを持たない Pekko 由来ファイルが存在し、また迷うファイルも安全側（`Apache-2.0`）に倒すため、「マーカーなし＋`Apache-2.0`」は正当な状態である。
  - 逆に、移植マーカーまたは `Mirrors \`org.apache.pekko.` を持つファイルに `MIT OR Apache-2.0` を付けるのは、Pekko 由来コードへ無権限に MIT 許諾を付与してしまう重大な誤り（かつ公開後は不可逆）であるため、CI で確実に落とす。
- **CI の `--check --manifest` が止められる範囲（設計レビュー 2 回目の M1、決定 6.5）**：前の版は、日常運用の `--check` が強い根拠（C3）しか見ず、材料 5〜9 しか当たらないファイルを「自作」と書き換えても止まらなかった。この版では CI が `--check --manifest` を使い、TSV にある全 433 行の SPDX を `付けるSPDX` と照合する（C5、§3.4.4）。既存のファイルの付け替えには、決定のファイルと TSV の差分が要る。新しいファイルを足す PR が TSV を編集しなくてよいので、案 (1) のマージコンフリクトは起きない。
- **`MIT` を含む式は、決定のファイルの行からしか生まれない**（設計レビュー 3 回目の H1、M1、M2）：新しいファイルは TSV に無いので、付けてよい式は `Apache-2.0` だけである。決定の行は TSV にあるパスしか取れないので、`MIT OR Apache-2.0`（や外部の式）を付けるには、TSV を作り直す差分と、決定のファイルの行が要る。前の版は、新しいファイルの種別を書いた人が選べ、「レビューで防ぐ」としていたが、このリポジトリでは人間による PR のコードレビューは関門ではない。広げるのは、あとから決定のファイルで行う（決定 6）。

### 3.3 線引き一覧（件数・判定の規則・要確認・人の決定）

線引きの一覧は機械判定で作る。判定の根拠は 1 ファイルずつ TSV に残してあり、人が根拠を見て覆せる。人の決定は別のファイルに置き、スクリプトが最後に重ねる（M2）。

- 一覧の本体：[`docs/design/issue-206-file-origin.tsv`](./issue-206-file-origin.tsv)（433 行＋見出し、12 列）
- 人の決定：[`docs/design/issue-206-origin-decisions.tsv`](./issue-206-origin-decisions.tsv)（5 列。下の「人の決定」）
- 判定のスクリプト：[`tools/license-headers/classify_file_origin.py`](../../tools/license-headers/classify_file_origin.py)（入口と出力）、`origin_evidence.py`（本文の材料）、`origin_pekko.py`（原典の索引と表示）、`origin_decisions.py`（人の決定）。テストは `classify_file_origin_test.py`（原典の git は要らない）
- 再実行：`PEKKO_REPO=<Pekko v2.0.0 の git リポジトリ> uv run --with pyyaml python3 tools/license-headers/classify_file_origin.py`（既定の出力先は上の TSV）。この版を作業ツリーで流し直し、TSV とバイト単位で一致することを確かめた。対象の集合は `git ls-files` で決める（L7）。`crates/` の下は、名前で飛ばすディレクトリを持たない。ライセンス作業のファイル（`LICENSE*`、`NOTICE`、この文書、TSV、決定のファイル、候補の一覧、`tools/license-headers/`）は走査から外してあるので、TSV を書き直しても自分自身は増えない。
- 原典は Apache Pekko v2.0.0（`9fbd2283b1cf701c90a1223bf621942c191db61f`）。`--pekko-tree` で展開先を渡すと、固定コミット専用の展開先として使う。展開したときに `.pekko-commit` を書き、それが固定コミットと一致するときだけ再利用する。別のリビジョンの作業ツリーや、印の無い空でないディレクトリは使わずに止める。省略時の一時の展開先は、例外で止まったときも消す。
- `根拠の中身` には、台帳の根拠表（`docs/baseline/v2.0.0-port-targets.tsv`）で移動した旧 id を書かない。`<移動前の id>` に置き換える。旧 id が残ると、CI の `relocate_nodes.py check` が「旧 id の参照が残っている」で落とすからである（根拠表そのものを引く行がこれに当たる）。

TSV の列：

| 列 | 中身 |
|:---|:---|
| `path` | リポジトリのルートからの相対パス |
| `判定` | `Pekko 由来` / `自分で書いた`（機械の判定） |
| `根拠の種類`、`根拠の中身` | 当たった材料と、その行 |
| `要確認`、`要確認の理由` | 材料が弱いもの（下） |
| `走査範囲` | `crates`（全件）/ `other`（材料が当たったものだけ） |
| `付けるSPDX` | 人の決定を重ねたあとの SPDX。`Apache-2.0`、`MIT OR Apache-2.0`、決定のファイルにある外部の式（M3）、または `保留` |
| `決定の出典` | `判定`（機械の判定のまま）か、決定のファイルの行（例 `issue-206-origin-decisions.tsv:2（元帥 2026-11-26）`） |
| `原典ファイル` | ヘッダーへ表示を写す原典のファイル（`;` 区切り、強い順。§3.1） |
| `第三者の表示` | ヘッダーへ写す第三者の表示（§2.1） |
| `表示のハッシュ` | 写した原典・第三者の表示ブロック（SPDX 行・直後の `//`・固定末尾 5 行を除いた行の並び）の SHA-256（C6、§3.4.4） |

#### 件数の内訳（main `f54008c` を取り込んだ作業ツリー）

| 走査範囲 | 件数 | Pekko 由来 | うち要確認 | 自分で書いた |
|:---|--:|--:|--:|--:|
| `crates/**/*.rs`（全件を判定） | 350 | 295 | 116 | 55 |
| `crates/` 以外（docs、ledger、tools、`.github`、トップ。材料が当たったものだけ） | 83 | 83 | 83 | 0 |
| **合計** | **433** | **378** | **199** | **55** |

前の版（`b6a4d69`、324＋80 件）からの変わり方：

| | 件数 |
|:---|--:|
| 前の版の 324 件のうち、H1 の規則で「自分で書いた」から要確認へ移ったもの | 6（設計レビュー 2 回目が挙げた 6 件そのもの。ほかの 318 件は判定も印も変わらない） |
| main を取り込んで増えた `crates/**/*.rs` | 26（Pekko 由来 25、うち要確認 15。自分で書いた 1） |
| main を取り込んで増えた `crates/` 以外の行 | 3（いずれも要確認） |
| 要確認の合計 | 175 → 199（＋6、＋15、＋3） |
| 自分で書いた | 60 → 55（−6、＋1） |

**付けるSPDX**（人の決定を重ねたあと。ヘッダーを付けるのは `crates/**/*.rs` だけ）：

| 付けるSPDX | `crates` | `crates` 以外 | 内訳 |
|:---|--:|--:|:---|
| `Apache-2.0` | 295 | 83 | 要確認でない Pekko 由来 179 ＋ 要確認 116（一括の決定、決定のファイルの 2 行目）。`crates` 以外の 83 件はヘッダーを付けない |
| `保留` | 55 | 0 | 自分で書いた。**照合と元帥の確認が済むまで何も付けない**（[候補の一覧](./issue-206-mit-candidates.md)） |
| `MIT OR Apache-2.0` | 0 | 0 | ファイルごとの決定でしか付かない（下） |
| 合計 | 350 | 83 | |

クレートべつ（`crates/**/*.rs`）：

| クレート | 全件 | Pekko 由来 | うち要確認 | 要確認でない Pekko 由来 | 自分で書いた |
|:---|--:|--:|--:|--:|--:|
| `crates/fraktor-actor` | 297 | 250 | 103 | 147 | 47 |
| `crates/fraktor-testkit` | 53 | 45 | 13 | 32 | 8 |
| 合計 | 350 | 295 | 116 | 179 | 55 |

`crates/` の種別べつ（`tests/` は統合テスト、`*_test.rs` は `src` の下の単体テスト、`src` 実装はそれ以外）：

| 種別 | Pekko 由来（要確認でない） | Pekko 由来（要確認） | 自分で書いた |
|:---|--:|--:|--:|
| `crates/*/tests/*.rs` | 52 | 23 | 4 |
| `crates/*/src/**/*_test.rs` | 1 | 37 | 26 |
| `crates/*/src/**`（実装） | 126 | 56 | 25 |

#### 根拠の種類べつ

| 根拠の種類 | crates | crates 以外 | 合計 | 要確認 |
|:---|--:|--:|--:|--:|
| 移植マーカー | 52 | 0 | 52 | 0 |
| Mirrors | 124 | 1 | 125 | 2 |
| 原典パス | 6 | 52 | 58 | 54 |
| Mirrors（原典の明記なし） | 13 | 0 | 13 | 13 |
| 原典への言及（Pekko の明記なし） | 27 | 3 | 30 | 30 |
| 原典同名 | 28 | 0 | 28 | 28 |
| 原典同名（型名） | 3 | 0 | 3 | 3 |
| Pekko言及 | 33 | 27 | 60 | 60 |
| 原典同名（型名・大文字小文字を区別しない）（H1 の規則 1） | 1 | 0 | 1 | 1 |
| 原典の型とメソッド（`Type.method`）（H1 の規則 2） | 4 | 0 | 4 | 4 |
| 原典への対応・移植の語（H1 の規則 3） | 2 | 0 | 2 | 2 |
| 第三者のコードへの言及（規則 4） | 2 | 0 | 2 | 2 |
| なし（自分で書いた） | 55 | 0 | 55 | 0 |

#### 判定の規則

材料は次の 9 つ。ファイルごとに、上から順に当たった 1 つを根拠として記録する。

1. **移植マーカー**：冒頭 20 行の `Ported from Pekko` / `Port of Pekko` / `Checked against Pekko`。現在のスクリプトは `apply_ported_headers.py` と同じ定義（大文字小文字を区別せず、語の境界を見ない）で、同スクリプトが付けたヘッダーを除いた本文を見る。厳密な定義でも結果は同じ 52 件である。F2 の直しは、ヘッダーツールの改修と同時にスクリプトへ反映する（§7.4 の最終行）。
2. **Ported from / Port of**：出現行が Pekko に触れているものだけを原典の対応とみなす（旧 Rust ツールからの移植を拾わないため）。
3. **Mirrors**：`Mirrors org.apache.pekko.X` などの対応の記述。ファイルのどこかに Pekko の記述があるときだけ採用する。
4. **原典パス**：`org.apache.pekko.X` / `org/apache/pekko/...` の形のパス。
5. **台帳**：`ledger/nodes.yaml` の `rust.test_file`（`retired` も含む、53 件）。53 件すべてが一覧に入っている。
6. **原典同名 / 原典同名（型名）**：ファイル名（拡張子抜き）か、宣言している公開型の名前が、原典のファイル名・型名と一致する。
7. **Pekko言及**：コメント行（`.rs`）または本文に `Pekko` の語があるだけ。
8. **原典への言及（Pekko の明記なし）**：`the original` / `upstream` への言及があるが、それが Pekko だと書かれていない。
9. **H1 の規則と規則 4**（`crates/` だけ。1〜8 のどれにも当たらないファイルに使う）：
   1. 型名の照合で大文字小文字を区別しない（`AddressFromUriString` と原典の `AddressFromURIString`）。長さ 9 文字以上、原典の宣言が 6 件以下、本体（`/main/`）にある名前だけを使う（材料 6 の型名と同じ条件）。
   2. コメントにある Scala 流の `Type.camelMethod` で、`Type` が原典の型名のもの（大文字小文字を区別しない）。`Type.snake_case` は `\b` の位置で外れるので、Rust のメソッドは拾わない。
   3. 「corresponding to」「the ported」「original `X`」。行をまたいだ言い回し（`as in the` の次の行が `// original:`）も拾うため、続いたコメントの行をつないだ文で探し、材料 8 の語もそこで見る。
   4. **規則 4（この版で足した）**：Pekko が使う第三者のコードを引く（`scala.util.hashing.MurmurHash3`、`MurmurHash3.<method>`、`hash-prospector`）。H1 の 3 つでは `murmur3.rs`（scala-library の MurmurHash3 の手順を写す）が「自分で書いた」に残り、MIT の候補に入ってしまうため。

規則 9 の 4 つを前の版の「自分で書いた」60 件に流すと、レビューが挙げた 6 件（`address_from_uri_string.rs`、`executor_value_test.rs`、`restart_limit_zero_no_clock.rs`、`manual_scheduler_test.rs`、`test_actors_props.rs`、`test_time_contract.rs`）がちょうど当たり、ほかは当たらない。main から入ったファイルでは、`murmur3.rs`、`string_hasher_test.rs`（規則 4）と `sketch_depth.rs`（行をまたぐ「the original」）が当たる。`status_ask_future.rs`（`askWithStatus` の API の移植。レビューは「迷う」と見た）は文字列の手がかりが無いので規則では拾えず、候補の一覧で照合する。

判定は 2 値で、「判断できない」は使わない。

- **Pekko 由来**：材料 1〜5 のいずれかが当たったもの。材料 6〜9 しか当たらないものも、迷うものとして Pekko 由来に倒す。
- **自分で書いた**：どの材料も当たらなかったもの。**「原典を一切見ずに書いた」ことの証明ではない**ので、そのままでは `MIT OR Apache-2.0` を付けない（決定 6.1）。

**要確認**は、Pekko 由来のうち、材料 1〜4（強い根拠）だけでは確定しないもの。`crates/` では、材料 5〜9 しか当たらない 113 件と、強い根拠はあるが台帳の `test_file` に無い統合テスト 3 件（計 116 件）である。`crates/` 以外は、転載かどうかを機械で決められないので、材料が当たった 83 件すべてを要確認にしてある。

#### 要確認 199 件の理由べつ

| 要確認の理由 | 件数 | crates | crates 以外 |
|:---|--:|--:|--:|
| 文書/ツール：原典のパスや記述を引く（転載の有無は未確認） | 53 | 0 | 53 |
| Pekko への言及はあるが、原典との対応の明記が無い（設計の参考か写しか未確認） | 33 | 33 | 0 |
| 文書/ツール：Pekko の語が出るだけ（転載の可能性は低い） | 30 | 0 | 30 |
| ファイル内に Pekko の記述が無く、原典に同名のファイルがあるだけ | 28 | 28 | 0 |
| 「original」「upstream」への言及はあるが、原典が Pekko だと明記が無い | 27 | 27 | 0 |
| 「original」への対応は書かれているが、それが Pekko だと明記が無い | 13 | 13 | 0 |
| コメントが原典の型とメソッドを `Type.method` の形で引く（H1 の規則 2） | 4 | 4 | 0 |
| ファイル内に Pekko の記述が無いが、宣言している公開型と同名の型が原典にある | 3 | 3 | 0 |
| 移植の根拠はあるが台帳の `test_file` に無い（統合テストのみ） | 3 | 3 | 0 |
| 「corresponding to」「the ported」「original `X`」、または行をまたいだ「the original」（H1 の規則 3） | 2 | 2 | 0 |
| Pekko が使う第三者のコード（scala-library、hash-prospector）を引く（規則 4） | 2 | 2 | 0 |
| 宣言している公開型が、大文字小文字を区別しなければ原典の型と同名（H1 の規則 1） | 1 | 1 | 0 |
| **合計** | **199** | **116** | **83** |

要確認の 1 件ずつのパスと根拠は TSV の `要確認` 列が空でない行である。

```bash
awk -F'\t' 'NR > 1 && $5 == "要確認" {print $7 "\t" $6 "\t" $1}' docs/design/issue-206-file-origin.tsv | LC_ALL=C sort
```

#### 人の決定（M2）

人の決定は [`issue-206-origin-decisions.tsv`](./issue-206-origin-decisions.tsv) に置く。TSV を作り直しても消えない。スクリプトは機械の判定のあとに重ね、結果を `付けるSPDX` と `決定の出典` に出す。「除外と抑制は人間だけが決める」の原則に合わせ、人の決定が機械の出力に埋もれない形にした。

- 列は `対象`、`決定`、`決めた人`、`日付`、`理由`（5 列すべて必須）。`日付` は `YYYY-MM-DD` の実在する日付に限る。`#` で始まる行は読まない（提案を置く場所）。提案の行は `決めた人` を空けておく（`#` を外すだけで決定にならないように。空でなければ止まる。設計レビュー 3 回目の L1 (c)）。
- `対象` は、リポジトリのルートからの相対パスか、`@要確認`（要確認の行すべて）。
- `決定` は付ける SPDX の式で、**文法を 3 つの形に固定する**（設計レビュー 3 回目の M2）：`Apache-2.0`、`MIT OR Apache-2.0`、`Apache-2.0 AND <id>`。`<id>` は固定の一覧（`BSD-2-Clause`、`BSD-3-Clause`、`CC0-1.0`、`Unlicense`。`origin_decisions.py` の `EXTERNAL_IDS`）から選ぶ。綴り、空白、大文字小文字、括弧、並びが 1 つでも違えば、行番号を付けて止まる（`Apache-2.0 OR MIT`、`MIT`、`mit or apache-2.0`、`(MIT OR Apache-2.0)`、`GPL-3.0-only` など）。一覧を広げるのは設計の変更である。
- **`MIT` を含む式は、綴りや並びによらず、すべて MIT として扱う**（`mentions_mit`）。強い根拠の守り、C3、新しいファイルの規則（§3.2）は、この判定で効く。
- 重ね方：`@要確認` の一括の決定を先に、ファイルごとの決定を後に重ねる（ファイルごとが勝つ）。決定の無い行は、要確認でない Pekko 由来が `Apache-2.0`、それ以外が `保留`。
- スクリプトが止まる（書き込まない）入力：見出しが違う、列が欠ける、一覧に無いパス、同じパスの決定が 2 つ、`@要確認` の決定が 2 つ、`@要確認` に `Apache-2.0` 以外、強い根拠（移植マーカー、Pekko に触れる `Ported from`、`Mirrors`、原典パス）のある行に `MIT` を含む式、未知の `@` の対象、3 つの形に無い式、形の違う日付、`決めた人` の入った提案の行。

今の決定：

| 行 | 対象 | 決定 | 決めた人 | 中身 |
|:---|:---|:---|:---|:---|
| 2 | `@要確認` | `Apache-2.0` | 元帥（2026-11-26） | 決定 6.2。要確認は一括で `Apache-2.0` のままにし、1 件ずつは見ない |

前の版にあった `fast_frequency_sketch.rs` の `Apache-2.0 AND Unlicense` の提案の行（3〜4 行）は、決定 7.1 で消した。

**`MIT OR Apache-2.0` へ広げる手順**（候補の照合のあと、または要確認があとで自作と分かったとき）：照合の根拠を書いてファイルごとの行を足し、TSV を作り直し、段 B の手順で付ける（§8.1）。`Apache-2.0` から `MIT OR Apache-2.0` へ広げることは、権利者が後から行える（許諾を足すだけ）。

**逆向きの直し**：`MIT OR Apache-2.0` から `Apache-2.0` へ狭めても、すでに公開した版への許諾は取り消せない。ただし、**Pekko 由来のファイルに誤って付けた MIT は、もともと有効な許諾ではない**（オーナーが MIT で出せるのは自分の書いた部分だけである）。そのため、Pekko 由来と分かったファイルを `Apache-2.0` へ直すことには意味があり、直す義務もある（設計レビュー 2 回目の H1）。

#### 原典ファイルの決め方（ヘッダーへ写す表示の出どころ）

crates の Pekko 由来の行について、`原典ファイル` を次の順で集める（原典のファイルに実在するものだけ）。

1. 本文に書かれた原典のパス（移植マーカーの次の行の `actor-tests/src/test/scala/.../EventStreamSpec.scala` など）
2. 台帳の id（原典のパス）
3. `Mirrors` の行を先に、本文の `org.apache.pekko.<pkg>.<Type>` が指す型のファイル（パッケージのディレクトリで絞る）
4. `Mirrors` の行の `` `Type` `` / `` `Type.member` ``（パッケージを書いていないもの。原典の宣言が 6 件以下の名前）
5. 判定の根拠（原典同名、型名、`Type.method`）が指すファイル

Pekko 由来 295 件のうち 210 件で決まり、85 件は決まらない（76 件は要確認。§3.1 の既定の表示を使う）。原典ファイルの数は、1 件が 191 件、2〜5 件が 18 件、11 件が 1 件（`executor_value.rs`）。

#### 既知の限界

- 判定はファイル単位で、原典のどの部分を写したかまでは見ていない。Pekko 由来のファイルにも、Rust 向けに書き直した部分が混ざる。どこまでが移植かはファイルごとに人が確かめる必要がある（この確認は別 issue にする）。
- 原典同名（材料 6）は弱い。名称が一致しない移植は拾えない（`status_ask_future.rs` がその例）。だから「自分で書いた」は、照合を通るまで `保留` にする。
- 自分で書いた 55 件は「どの材料にも当たらなかった」ことの結果で、「原典を一切見ずに書いた」ことの証明ではない。

### 3.4 ツール仕様（`tools/license-headers/license_headers.py`）

既存の `tools/license-headers/apply_ported_headers.py`（および `_test.py`）は移植ファイル専用だったため、方針 C の 2 種ヘッダー対応に合わせて **`tools/license-headers/license_headers.py`**（および `license_headers_test.py`）へ改名・改修する（`main` 未マージのため互換性破壊なし）。Python 標準ライブラリのみで実装する。

#### 3.4.1 走査対象

- 対象の集合は **`git ls-files`** で決める（設計レビュー 2 回目の L7）。手元と CI で同じ集合になり、追跡していないファイルは入らない。ファイルシステムを歩かないので、`crates/` の下に `pekko` や `target` という名前のディレクトリがあっても、黙って飛ばさない。
- SPDX を求める（C1〜C4）のは、追跡している `crates/**/*.rs` のすべて。
- 一覧との照合（C5）は、TSV の全 433 行（`crates` 350、`other` 83）。

#### 3.4.2 CLI モード

`--manifest` は §3.3 の TSV（12 列）を読む。ツールが使う列は `path`、`走査範囲`、`付けるSPDX`、`原典ファイル`、`第三者の表示`、`表示のハッシュ`。`判定` と `要確認` は読まない（人の決定を重ねた結果が `付けるSPDX` にある）。原典の表示を写すモードは、原典の git（`--pekko`、既定は `PEKKO_REPO`）を読む。

| モード | 動作 | 終了コード |
|:---|:---|:---|
| 既定（引数なし、dry-run） | 全 `crates/**/*.rs` のヘッダー状態（`apache`, `dual`, `external`, `missing`, `invalid`, `conflict`）を集計表示。書き込みなし | 0 |
| `--apply --manifest <path.tsv>` | `走査範囲 = crates` の行だけを書く。`付けるSPDX` が `Apache-2.0` か外部の式なら種別 1（原典の表示を写す、§3.1）、`MIT OR Apache-2.0` なら種別 2、`保留` なら書かない。まず全件を事前検証（§7.1 I6：`crates` の行の集合と実ファイルの集合が完全一致、重複なし、`付けるSPDX` の値が既知、UTF-8 読取可、種別衝突・未知 SPDX なし、`原典ファイル` が原典の git にあり読める、写す行が 100 文字以内）し、すべて通った場合のみ未適用ファイルへヘッダーを書き込む | 成功 0 / 検証失敗 1（書き込み 0 件） |
| `--apply --kind {pekko\|original} <path...>` | 指定した個別ファイル（新規移植・新規自作ファイル等）に該当ヘッダーを冪等適用。`pekko` は `--source <原典のパス>` を 1 つ以上とり、その表示を写す（無ければ §3.1 の既定の表示）。既に別種別や未知 SPDX がある場合、本文に `SPDX-License-Identifier:` がある場合、`original` なのに C3 のマーカーがある場合、`original` なのに決定のファイルにそのパスの `MIT OR Apache-2.0` の行が無い場合（§3.2。新しいファイルはまず `pekko` で付ける）は、書き換えずにエラー（L3） | 成功 0 / 失敗 1 |
| `--remove` | マニフェストやマーカーを見ず、`crates/**/*.rs` のうちツール管理のライセンスヘッダーブロックを持つ全ファイルからヘッダーを除去する（F4 解消）。決定のファイルで許した外部のヘッダー（M3）は消さずに飛ばし、パスを一覧に出す。それ以外の未知形式の SPDX ブロックは誤削除を防ぐためエラーで止める（事前検証で Fail-first） | 成功 0 / 失敗 1 |
| `--check` | `crates/**/*.rs` の全ファイルに対し C1〜C4 を検証する（一覧を使わない簡易の検査） | 全合格 0 / 違反あり 1 |
| `--list`（どのモードにも付けられる） | ファイルごとの状態を出す。`--apply` と `--remove` では、**書き込んだ後の本文**で状態を出す（旧 `apply_ported_headers.py` は書き込む前の本文で出していた。PR #215 の CodeRabbit の指摘。旧ツールは段 2 で消す） | − |
| `--check --manifest <path.tsv>`（**CI はこれを使う**。M1） | C1〜C4 に加えて C5（一覧との照合）と C6（写した表示）を、TSV の全 433 行と、TSV に無い新しい `crates/**/*.rs` のすべてに効かせる。C5 の原典の要らない部分は、判定スクリプトの `origin_decisions.manifest_errors` を使う（§3.4.4）。`保留` の行は、SPDX が無いときだけ合格（`held` と数える）。違反は種別・パス・修正コマンド例を出す | 全合格 0 / 違反あり 1 |

#### 3.4.3 文言の完全一致に頼らない構造化ヘッダーブロック認識（F1 の解決）

前の実装は `text.startswith(insertion)` というバイト完全一致のみに依存していたため、年号の更新（`2022-2026` → `2022-2027`）、[Issue #218](https://github.com/j5ik2o/fraktor-ng-rs/issues/218) のファイル固有 `Copyright` 行への変更、空行の誤削除（前レビュー E6）のいずれでも `--remove` が動かず、`--apply` が二重に SPDX を付けてしまった。

これを解消するため、ヘッダーの解析を**プレフィックス分離＋構造化ブロック認識**に改める。

1. **改行の厳密分割**：`str.splitlines()` は U+2028 / U+2029 / U+0085 で誤分割するため使わず、`\r\n` と `\n` だけを改行として扱う独自行分割を使う。
2. **プレフィックス分離（BOM と shebang）**：
   - 先頭に UTF-8 BOM（`\ufeff`）があれば `bom` として分離する（BOM は出力時に必ず先頭に保持する）。
   - BOM の直後（1 行目）が `#!` で始まり、その後（空白を除く）が `[` でない場合、その 1 行（末尾改行を含む）を `shebang` として分離する。rustc の判定に合わせた（設計レビュー 2 回目の L3。前の版は `#!/` だけを見ていた。`crates/` には 0 件）。
     - もし `#!/...` の 1 行だけで改行が無い場合（`crates/` に実在しない境界入力）は、改行補完で往復性が崩れるためエラーとして拒否する。
3. **ライセンスヘッダーブロックの認識（プレフィックス直後から走査）**：
   - 先頭行が `^// SPDX-License-Identifier:\s*(.+?)\s*$` に一致しない場合：**ヘッダー無し（`NONE`）**。
   - 一致した場合、その行から下へ「通常の行コメント（`^//($|[^/!].*)`）」が連続する間を走査する。`///`（外側 doc）、`//!`（内側 doc）、空行、非コメント行に達した時点で走査を止める。
   - **終端アンカーによる切り止め（E6 対策）**：走査中に次の既知の終端行（トリム後）に達したら、その行で**直ちにブロックを閉じる**：
     - Pekko 由来アンカー：`// This file has been modified from the Apache Pekko original.`
     - 自作アンカー：`// at your option. See LICENSE-APACHE and LICENSE-MIT at the repository root.`
     - **アンカーの行は固定する**（設計レビュー 2 回目の L2）。1 語でも変えると、そのブロックは `UNRECOGNIZED` になり、I6 の fail-first で `--remove` がツリー全体で止まる。文言を変えるときは、旧アンカーを既知の終端行の一覧に残し、旧アンカーのファイルを認識して付け直すテストを足す。
     - 原典から写した行（§3.1）は `// ` で始まるので、`///` や `//!` で走査が止まることはない。
     - これにより、人がヘッダー直後の空行を誤って消して `// Ported from Pekko...` と接してしまっていても、アンカー行の直後でヘッダーが終わるため、**既存の出典コメントを絶対に削らない**。
     - また、ヘッダー内部の `Copyright` 行の年号変更・ファイル固有著作権行の追記・`See LICENSE and NOTICE`（旧版）↔ `See LICENSE-APACHE and NOTICE`（新版）の差があっても、先頭行と終端アンカーで同一のライセンスヘッダーブロックとして認識する。
   - **区切り空行の消費**：終端アンカー直後の次の行が空行（`\r\n` または `\n` のみ）であれば、ヘッダー付与時に挿入した区切り空行として**ちょうど 1 行だけ**をヘッダー領域に含める（先頭空行が 2 行以上ある元ファイルでも 1 行しか消さないため往復が保たれる）。直後に空行が無く本文が続く場合（空ファイルへの適用結果、または E6 の空行欠落）は 0 行消費とする。
   - **外部のヘッダー（`EXTERNAL`、M3）**：決定のファイルがそのパスに許した式の SPDX 行で始まるブロック。`--apply` は書き換えず、`--remove` は飛ばして一覧に出し、`--check` は許した式と一致すれば通す。
   - **未知・破損 SPDX ブロック（`UNRECOGNIZED`）**：先頭行が `// SPDX-License-Identifier:` で始まるのに既知の終端アンカーに到達せず終了した場合（手書きの独自フォーマットや途中で壊れたヘッダー）。この状態では、`--apply` は二重付与を避けてエラー停止、`--remove` は本文誤削除を避けてエラー停止、`--check` は不正フォーマットとして失敗させる。
4. **挿入時の空行ルール（空ファイル往復の保証）**：
   - プレフィックス除去後の残り本文 `rest` が空（`rest == ""`）なら `prefix + header + newline`（末尾区切り空行なし）。
   - `rest != ""` なら `prefix + header + newline + newline + rest`（ヘッダーと本文の間に空行 1 行）。
   - これにより、空ファイル・空行のみ・先頭空行あり・末尾改行なし・CRLF・BOM・shebang のすべてで `remove(apply(x, k)) == x` がバイト単位で完全に成り立つ。

#### 3.4.4 `--check` の検査規則（C1〜C6）

`crates/**/*.rs` の各ファイルについて C1〜C4 を、`--manifest` を付けたときは C5 と C6 も検査する。

- **C1（SPDX 必須・許可式・重複禁止）**：
  - 構造化ヘッダーブロックが存在し、状態が `UNRECOGNIZED` でないこと。
  - SPDX 式が決定の文法の 3 つの形（§3.3）のどれかと綴りまで一致すること（設計レビュー 3 回目の M2。`Apache-2.0 OR MIT` などは `spdx-grammar` で落ちる）。`Apache-2.0` 以外の式は、決定のファイルがそのパスに許したときだけ通る（C5）。許しは「パスと式の対」で、ほかのパスには効かない。
  - 本文（ヘッダー除去後）に `SPDX-License-Identifier:` が再出現しないこと（二重ヘッダー検知）。
  - 非空本文の前に区切り空行が 1 行あること（E6 の空行欠落も検知）。
- **C2（種別ごとの必須要素）**：
  - `Apache-2.0` と外部の式：ブロック内に `Licensed to the Apache Software Foundation (ASF)`（原典の ASF の表示。2 つの形のどちらも、この句で始まる）、`LICENSE-APACHE`、`NOTICE`、および終端アンカー（変更表示 `This file has been modified from the Apache Pekko original.`）が含まれること。
  - `MIT OR Apache-2.0`：ブロック内に `Copyright`、`LICENSE-APACHE`、`LICENSE-MIT`、および自作終端アンカーが含まれること。
- **C3（移植マーカーとの矛盾検知・F2 対応）**：
  - 本文（ヘッダー除去後）の**冒頭 20 行**に、大文字小文字を区別し語境界を見る正規表現 `\b(Ported from|Port of|Checked against) Pekko\b` が一致するか、**本文のどこか**（行の位置によらない）に rustdoc の ``Mirrors `org\.apache\.pekko\.`` が一致するにもかかわらず、SPDX が `MIT` を含む式（`mentions_mit`。綴りによらない）である場合は**失敗（誤分類）**とする。前の版は Mirrors の窓が読み取れなかった（設計レビュー 2 回目の M1）。冒頭 20 行に限ると、20 行より後にだけ Mirrors を持つ 15 件（前の版の 264 件でも、今の 295 件でも 15 件）を取りこぼすので、Mirrors はファイル全体を見る。これで C3 が守るのは Pekko 由来 295 件のうち 128 件になる。
  - 逆（マーカー無し＋`Apache-2.0`）は正当（実装ファイル・迷うファイルの安全側倒し）として許可する。
  - C3 だけでは、残りの 167 件（要確認の 116 件をすべて含む）を `MIT OR Apache-2.0` へ書き換えても止められない。それを止めるのが C5 である。
- **C4（ファイル健全性）**：
  - UTF-8 として読めること（非 UTF-8 はトレースバックを出さず明示エラーで終了コード 1）。
  - 改行なし shebang 単独行のような破壊的入力でないこと。
- **C5（一覧との照合。M1、設計レビュー 3 回目の H1、M1、M2）**：原典の要らない部分は、判定スクリプトの `origin_decisions.manifest_errors` が持つ（テストは `classify_file_origin_test.py` の `ManifestCheck`）。ヘッダーツールはこれを読み込んで使う。
  - **TSV の手直しを見つける**：TSV の `判定`、`根拠の種類`、`要確認` の列と決定のファイルから、`付けるSPDX` と `決定の出典` を計算し直す（`apply_decisions` は純粋な関数なので、CI に原典は要らない）。食い違えば `manifest-tampered` で落とす。
  - **TSV にある行**：ファイルの SPDX が、計算し直した `付けるSPDX` と一致すること（`manifest-mismatch`）。`保留` の行は SPDX を持たないこと（`held-has-spdx`）。`走査範囲 = other` の行（ヘッダーを付けない）は、SPDX の行を持たないか、持つなら `付けるSPDX` と一致すること。
  - **`MIT` を含む式は、決定のファイルにそのパスの行があるときだけ許す**（`mit-without-decision`）。I9 がファイルにも効く。
  - **TSV に無い新しいファイル**：式は `Apache-2.0` だけ（`new-file-not-apache`。§3.2）。`crates/**/*.rs` は C1〜C4 も通ること。`crates/` の外の新しいファイルは SPDX を求めないが、SPDX の行を持つなら同じ規則に従う。
  - TSV のパスが作業ツリーに無い（消えた・移った）ときは失敗にし（`manifest-missing-file`）、TSV の作り直しを求める。
  - **行を消したとき**：既存のファイルの行を TSV から消すと、そのファイルは「新しいファイル」に見える。上の規則で、付けられる式は `Apache-2.0` だけになるので、消しても MIT は通らない（試作 M-c、M-j）。消したこと自体は、判定スクリプトで TSV を作り直して `cmp` で比べると見つかる（原典が要る。移植の手順では push の前に流す。§6）。CI には原典が無いので、CI が保証するのは「消しても MIT と外部の式は付かない」ところまでである。
- **C6（写した表示。設計レビュー 3 回目の M3、決定 7.2）**：原典の要らない検査で、`--check --manifest` が `Apache-2.0` と外部の式のファイルに効かせる。
  - TSV の `第三者の表示` にあるラベルのうち、原典のファイル先頭から写すもの（`Copyright 2017 The Netty Project（DnsSettings.scala）` など、`（<原典>.scala）` か `（<原典>.java）` で終わるもの）は、`（` より前の行（`Copyright 2017 The Netty Project`）がヘッダーのブロックに含まれること。本文のコメントに残すもの（`hash-prospector: public domain（Unlicense）`）は、`hash-prospector` を含むコメントがファイルにあること。無ければ `notice-missing <path> <ラベル>` で落ちる。
  - Lightbend の行と、写したブロック全体：ツールの改修（段 2）で、判定スクリプトが TSV に列 `表示のハッシュ`（写したブロックの行の並びの SHA-256）を足す。C6 は、ヘッダー先頭の 2 行（`// SPDX-License-Identifier: ...` と直後の `//`）と、固定末尾の 5 行（区切りの `//`、`// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE`、`// at the repository root.`、`//`、`// This file has been modified from the Apache Pekko original.`）を除いた部分のハッシュがこの列と一致することを確かめる。年を変えた、ブロックを消した、行を足したのどれでも落ちる（`notice-mismatch <path> expected=<hash> actual=<hash>`）。
  - テストの案（`license_headers_test.py`）：`dns_settings.rs` に当たる入力で、(1) 適用した直後は合格、(2) Netty のブロックを消すと `notice-missing`、(3) Lightbend の年を `2018-2022` から `2018-2023` に変えると `notice-mismatch`、(4) ヘッダーの外（本文）に Netty の行を書いても合格にならない、(5) 原典のファイルが決まらない行（既定の表示）でも、ASF の表示を消すと落ちる、(6) `fast_frequency_sketch.rs` に当たる入力で、hash-prospector のコメントを消すと落ちる。

### 3.5 前回設計レビュー（`pr215-design-review.md`）全 10 指摘への回答

2 回目のレビューの指摘への答えは §9 にある。

| # | 重さ | 指摘の要約 | 本設計での対応 | 扱い |
|:---|:---|:---|:---|:---|
| F1 | 中 | 文言完全一致に依存し、年更新・#218・空行削除（E6）で削除不能・二重付与になる | §3.4.3 の構造化ヘッダーブロック認識（`// SPDX-License-Identifier:` ＋終端アンカー行）に変更。旧版・年号変更・#218 の固有 Copyright 行・空行欠落でも二重付与せず `--remove` で安全に除去できる | 直す |
| F2 | 中 | `re.IGNORECASE`・語境界なしで `Imported from Pekko` 等を誤検出 | `re.IGNORECASE` を廃止し `\b(Ported from\|Port of\|Checked against) Pekko\b` に変更。否定テスト（`Imported from Pekko`、`Support of Pekko`、`UNSUPPORTED FROM PEKKO`）を追加。さらに分類の正はマニフェスト＋SPDX へ移行 | 直す |
| F3 | 中 | `LICENSE` 削除で許諾は戻らないのに「可逆」と書いた | §2.3・§8.2 に「`main` マージ後の許諾は取り消せない（Apache-2.0 第 2 条・MIT 恒久許諾）」と明記。可逆範囲（ヘッダー・ツール・未マージ段階）と不可逆範囲（公開許諾）を明確に分けた。2 回目のレビューの H1 で、取り消せなくなるのは push の時点だと直した（§2.3、§8.2） | 直す |
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
| `Apache-2.0 AND (MIT OR Apache-2.0)` | 妥当 | 正しい（冗長） | 構文上も論理上も正しく、吸収律により `Apache-2.0` と同値。成分ごとに式を書く crate もある（`unicode-ident` の `(MIT OR Apache-2.0) AND Unicode-3.0` など）。cargo-about や cargo-deny のような SBOM の道具には MIT の情報が残る。採らなかったのは読みやすさを取った選択で、この式が誤りだからではない（設計レビュー 2 回目の L1。前の版の「簡易監査ツールに混乱を生む」は根拠が無いので消した） |
| **`Apache-2.0`**（**採用**） | **妥当** | **厳密に同値かつ最小** | 吸収律 \(A \land (B \lor A) \equiv A\) により `Apache-2.0 AND (MIT OR Apache-2.0)` と数学的・法的に完全同値。クレート全体の利用者が満たすべき条件（Apache-2.0 ＋ `NOTICE`）を最も単純かつ誤解なく示す |

**結論**：Pekko 由来ファイルを含むクレートは **`license = "Apache-2.0"`** を採用する（決定 6.3）。

- **要確認の決め方に左右されない**：`license = "Apache-2.0"` が正しいのは、クレートに `Apache-2.0` だけのファイルが 1 件でもあるときである（吸収律の前提）。要確認の扱いが元帥の決定待ちでも、要確認でない Pekko 由来（移植マーカーか `Mirrors`、原典パスの強い根拠があるもの）が `fraktor-actor` に 147 件、`fraktor-testkit` に 32 件ある（§3.3）。元帥が要確認をすべて「自分で書いた」と決めても前提は崩れず、`Cargo.toml` の式は変わらない。逆に、1 件も無くなる（クレートが全件自作になる）ことはこの一覧では起きない。
- **自作ファイルが混ざっていることの表し方**：`fraktor-actor` に自作の候補 47 件、`fraktor-testkit` に 8 件が混ざる（段 B の照合の前）が、クレート単位の `license` は「配布物全体として守る条件」を示すので `Apache-2.0` のままとする。自作ファイルが `MIT OR Apache-2.0` で使えることは、各ファイルの SPDX が正である（クレート全体を MIT で使えるとは書かない）。
- **混在の可視性の補完**：自作ファイルを単独で切り出して利用する場合は `MIT OR Apache-2.0` で使えることが分かるよう、(1) 各 `Cargo.toml` の `license` 行直上にコメントで明記し、(2) `README.md` のライセンス節と (3) 各ファイルの `SPDX-License-Identifier` ヘッダーで明示する。
- **`license-file` は使わない**：Cargo は標準 SPDX 式で表現できる場合 `license` のみを指定することを推奨しており、`license-file` は併用しない。
- **将来の第三者コード追加時**：将来、非 Apache-2.0 の条件付き第三者コード（例：Simplified BSD 等）をクレートに取り込む場合は、その時点で、ファイルの SPDX（`Apache-2.0 AND BSD-2-Clause` など）と `Cargo.toml` の `license` 式を変えるかどうかを元帥が決める（決定 6.3 は `Apache-2.0`。§2.1）。

### 4.2 各マニフェストの設定一覧

| マニフェスト | 現在の `license` | 新しい `license` | 根拠 |
|:---|:---|:---|:---|
| `Cargo.toml`（ルート仮想ワークスペース） | なし（`[package]` なし） | なしのまま | クレートごとに構成（Pekko 混在か完全自作か）が異なるため、各パッケージの `Cargo.toml` で明示する |
| `crates/fraktor-actor/Cargo.toml` | 未設定 | `"Apache-2.0"` | 線引き一覧（§3.3）の実測で、297 件のうち Pekko 由来 250 件（要確認 103 件を含む）と自作の候補 47 件の混在。吸収律より `Apache-2.0 AND (MIT OR Apache-2.0) == Apache-2.0`。直上コメントで自作ファイルが `MIT OR Apache-2.0` である旨を補足 |
| `crates/fraktor-testkit/Cargo.toml` | 未設定 | `"Apache-2.0"` | 同 53 件のうち Pekko 由来 45 件（要確認 13 件を含む）と自作の候補 8 件の混在。上と同じ理由 |
| `tools/syn-check/Cargo.toml` | 未設定 | `"MIT OR Apache-2.0"` | 全 33 ファイルが完全自作（Pekko 由来 0 件を実測済み）。段 B で設定する（旧 `*-lint` が自作かを確かめてから。L1） |
| `package.json` | `"Apache-2.0"` | `"Apache-2.0"`（維持） | `private: true`（npm 非公開、CI 用 `aislop` 実行のみ）。リポジトリ全体の有効ライセンスは同じく吸収律により `Apache-2.0` であり整合する |

### 4.3 依存 crate のライセンス

この節は、このリポジトリが依存する crate と、そのライセンスを記録する。#234 の段 0（[Issue #248](https://github.com/j5ik2o/fraktor-ng-rs/issues/248)、`fraktor-actor` の kernel を `no_std` で build できるようにする段）で足す依存を、段 0 のプルリクエストより前にここへ記録する。案は #234 の設計（[`issue-234-multithread-executor.md`](./issue-234-multithread-executor.md) の §7.4）にある。

| crate | 版 | 使う段 | ライセンス | 使い方 |
|:---|:---|:---|:---|:---|
| `portable-atomic` | 1.15.0（段 0 で確定する） | 段 0 | Apache-2.0 OR MIT | kernel の原子操作（C1） |
| `portable-atomic-util` | 0.2.8（段 0 で確定する） | 段 0 | Apache-2.0 OR MIT | CAS の無い target での `Arc`（C3） |

- ライセンスは、どちらも crates.io に公開された crate の `Cargo.toml` の `license` の欄で確かめた（`portable-atomic` 1.15.0、`portable-atomic-util` 0.2.8。どちらも `license = "Apache-2.0 OR MIT"`）。
- 版は、#234 の設計の §7.4 に書かれていなかったため、この記録を書いた時点の最新の安定版を書いた。実際に使う版は段 0 で確定する。段 0 のプルリクエストでは、`cargo metadata` でライセンスを確かめ直し、使う版がこの記録と違う場合は、この節も合わせて直す。

---

## 5. CI への組み込み（`--check`）

### 5.1 組み込み先ジョブとステップ位置

- `.github/workflows/main-pr-ci.yml` は `.github/workflows/ci.yml` を `workflow_call` で呼び出し、`jobs.check` の成功（および最終ステップ `syn_checker` の成功）を必須チェック `Fraktor CI Gate` でゲートしている。
- したがって、`.github/workflows/ci.yml` の `jobs.check` に次を反映すれば、すべての PR と `main` push で必須検査になる。
  1. `python tests` ステップのテスト対象を `tools/license-headers/license_headers_test.py` に更新する（ユニットテスト＋seed 固定 PBT を毎回実行）。あわせて、この設計のコミットで足した判定スクリプトのテスト `tools/license-headers/classify_file_origin_test.py` も足す（このコミットでは `ci.yml` を変えないので、今は手で流す）。
  2. `Pekko baseline` ステップの直後（`check:aislop`・`cargo test` より前）に、次のステップを追加する：

```yaml
      - name: License headers
        run: uv run python3 tools/license-headers/license_headers.py --check --manifest docs/design/issue-206-file-origin.tsv
```

- **配置の理由**：Python 標準ライブラリのみで 1 秒未満で完了するため、SPDX ヘッダーの付け忘れを Rust のビルドや `aislop` を待たずに Fail-first で落とせる。

### 5.2 新規ファイルに SPDX が無い PR が落ちる仕組みと失敗時出力

- `--check` は `crates/**/*.rs` の全ファイルを走査するため、新しい `.rs` ファイル（テスト・実装・フィクスチャ問わず）が 1 件でも SPDX ヘッダー無しで追加されると、終了コード 1 で `License headers` ステップが失敗し、`Fraktor CI Gate` も落ちる。
- 失敗時の出力例（開発者・ワーカーがすぐ直せる案内を添える）：

```text
checked: 351  apache: 295  dual: 0  held: 55  missing: 1  errors: 1
ERROR missing-spdx crates/fraktor-actor/src/kernel/new_feature.rs
Hint: run `python3 tools/license-headers/license_headers.py --apply --kind pekko <path>`
```

- 移植マーカーや `Mirrors \`org.apache.pekko.` があるのに `MIT OR Apache-2.0` を付けた場合も `ERROR origin-conflict <path>` で終了コード 1 を返す。
- 一覧にあるファイルの SPDX が `付けるSPDX` と違う場合（決定のファイルに行を足さずに付け替えた場合）は `ERROR manifest-mismatch <path> expected=<付けるSPDX> actual=<SPDX>` で終了コード 1 を返す（C5）。
- TSV の `付けるSPDX` か `決定の出典` が決定のファイルから計算し直した値と違う場合は `ERROR manifest-tampered`、`MIT` を含む式を決定の行なしに付けた場合は `ERROR mit-without-decision`、新しいファイルに `Apache-2.0` 以外を付けた場合は `ERROR new-file-not-apache`、3 つの形に無い式は `ERROR spdx-grammar`、写した表示が欠けたか変わった場合は `ERROR notice-missing` / `ERROR notice-mismatch` で、終了コード 1 を返す（C5、C6）。

---

## 6. 新しく移植するときの手順（`fraktor-migration-intake` スキル追記案）

※設計レビュー通過前のため、`~/.orca/bots/fraktor-ng-rs/skills/fraktor-migration-intake/SKILL.md` 本体はまだ変更しない。以下を「設計の独立レビュー」または「ケース0件の土台」の次の節として追加する案とする。

```markdown
## ライセンスヘッダーと第三者帰属（方針 C + (a)）

- `crates/**/*.rs` に追加するすべての Rust ファイルの先頭に `SPDX-License-Identifier` ヘッダーを付ける。ヘッダーが無い PR は CI（`tools/license-headers/license_headers.py --check --manifest docs/design/issue-206-file-origin.tsv`）で落ちる。
- **新しいファイルは、自分で書いたものも含めて、まず `Apache-2.0` で付ける。** `python3 tools/license-headers/license_headers.py --apply --kind pekko --source <原典のパス> <path>` で付与できる。原典のファイル先頭の表示（ASF、Lightbend、第三者の著作権表示）が原文どおり入る。原典を参照していないファイルは `--source` を省く（既定の表示）。
- **`MIT OR Apache-2.0` は、決定のファイル（`docs/design/issue-206-origin-decisions.tsv`）の行からしか生まれない。** 新しいファイルに `MIT` を含む式を書くと、CI が `mit-without-decision` で落とす。自作の部分を `MIT OR Apache-2.0` へ広げるのは、あとから元帥が決定のファイルで行う（許諾を足すのは後からできる。逆はできない）。自作と思うファイルは、パスと理由を PR 本文と報告に書いて元帥へ上げる。
- **迷うファイルは安全側（Pekko 由来＝`Apache-2.0`）に倒す。** `MIT OR Apache-2.0` は、公開リポジトリのブランチへ push した時点で取り消せなくなる（`main` へのマージを待たない）ので、迷うものを自作の側に倒さない。
- 移植元の原典ファイルが `pekko/LICENSE` の第三者付録（203 行目以降：Unlicense、BSD、Caffeine、commons-io 等）に該当する場合は、該当の著作権表示と第三者ライセンス条文を落とさず保持し、設計レビューと PR 本文に明記する。SPDX に第三者のライセンスを足すか、`Cargo.toml` の `license` を変えるかは、元帥に頼む（決定のファイルの式は `Apache-2.0 AND <id>` の形だけ。許可の一覧に足すのは人だけ）。
- **原典の名前を引いて書いたテストと実装は「迷う」に入れる**（Pekko の型・メソッドを `Type.method` で引く、「corresponding to」「the ported」と書く、など）。
- `--kind pekko` で付けるファイルには、``Mirrors `org.apache.pekko.…` `` か移植マーカーを 1 行入れる。C3 が将来の付け替えも止められるようにするため。
- **push の前に、次の 2 つを流す。** CI は push の後に走るので、CI で落ちても公開はもう起きている。
  1. `python3 tools/license-headers/license_headers.py --check --manifest docs/design/issue-206-file-origin.tsv`（0 で終わること）
  2. 線引き一覧を変えた PR では、`uv run --with pyyaml python3 tools/license-headers/classify_file_origin.py --out-tsv <一時のパス>` と `cmp <一時のパス> docs/design/issue-206-file-origin.tsv`（一致すること。TSV を手で直したり、行を消したりしていないことの確かめ。原典 `pekko/` は takt の `prepare_pekko_source.sh` が用意する）
- 実装ワーカーを起動する作業仕様には、「新規作成した `crates/**/*.rs` に `Apache-2.0` のライセンスヘッダーを付け、push の前に `python3 tools/license-headers/license_headers.py --check --manifest docs/design/issue-206-file-origin.tsv` を通すこと（自作と思うファイルは報告に書く。`MIT` を含む式は書かない）」を含める。
```

---

## 7. 不変条件・PBT・Quint 要否・壊しうる入力・影響範囲

### 7.1 守る不変条件

変換関数 `apply(text, kind)`、`remove(text)`、`check_file(text)`（いずれもファイル内容の純粋関数）および CLI 全体について、以下を守る。

| 番号 | 不変条件 |
|:---|:---|
| I1 | **往復性**：ヘッダーを持たず（`parse_header(x).state == NONE`）、かつ「改行を含まない shebang 単独 1 行」でない任意の UTF-8 文字列 `x` と任意の `k ∈ {pekko, original}` に対し、**`apply(x, k)` が成功するなら** `remove(apply(x, k)) == x`。`apply` が断る入力（I3。本文に SPDX がある、`original` なのに C3 のマーカーがある）は I1 の対象外で、I3 と拒否のテストで確かめる（PR #215 の CodeRabbit の指摘）（バイト単位完全一致、BOM・CRLF・先頭空行・末尾改行有無を保持）。 |
| I2 | **冪等性**：ヘッダーを持たないか、`k` と同じ種別のヘッダーを持つ入力で、`apply(x, k)` が成功するなら `apply(apply(x, k), k) == apply(x, k)`。別の種別のヘッダーを持つ入力は I3 で扱う。 |
| I3 | **種別衝突・未知 SPDX の拒否**：既に異なる種別のヘッダーを持つ、または `UNRECOGNIZED` な SPDX ブロックを持つ入力に対する `apply` は例外／エラーを返し、入力を 1 バイトも変更しない（二重付与も無断上書きもしない）。本文に `SPDX-License-Identifier:` があるとき（C1 の再出現）と、`original` なのに C3 のマーカーがあるときも、`apply` が断る（L3）。 |
| I4 | **マーカー判定の位置不変性**：ヘッダーの適用前後で、本文に対する移植マーカー判定 `has_pekko_marker(strip_body(x))` の結果が変わらない（マーカーが元の 20 行目にあっても適用後に窓の外へ落ちない）。 |
| I5 | **非完全一致ヘッダーの安全な除去と本文非破壊**：先頭行が `// SPDX-License-Identifier:` で既知の終端アンカーを持つヘッダーブロックは、内部の年号・`Copyright` 行・`LICENSE` 参照文言が変わっていても、直後の区切り空行が欠けていても（E6）、`remove` はヘッダーブロックのみを除去し、直後の出典コメントや本文を 1 バイトも削らない。例外が 1 つある：区切りの空行が消え、しかも本文が空行で始まるときは、本文の先頭の空行 1 行を区切りとみなして消す（L4。内容としては害が無い）。 |
| I6 | **Fail-first の原子性**：`--apply --manifest` および `--remove` は、全対象ファイルの読み込みと検証（マニフェストの `走査範囲 = crates` の行と `crates/**/*.rs` の集合が完全一致、`付けるSPDX` が既知の値、UTF-8 妥当性・種別衝突なし・未知 SPDX なし）を終えてから書き込みを開始する。1 件でも検証に失敗したら 1 ファイルも書き換えない。 |
| I7 | **保留の不可侵**：`--apply --manifest` は、`付けるSPDX` が `保留` の行のファイルを 1 バイトも書き換えない。`--check --manifest` は、`保留` の行が SPDX を持てば失敗にする。 |
| I8 | **一覧の件数との一致**：`--apply --manifest` の事前検証が数える件数は §3.3 と一致する（段 A で `crates` の行 350、`Apache-2.0` 295、`保留` 55、`MIT OR Apache-2.0` 0）。ずれたら（ファイルが増減した、TSV が古い）適用せずに止まる。 |
| I9 | **MIT はファイルごとの人の決定からしか生まれない**：`付けるSPDX = MIT OR Apache-2.0` になるのは、決定のファイルにそのパスの行があるときだけである（`@要確認` の一括の決定は `Apache-2.0` しか持てない）。強い根拠のある行に `MIT` を含む決定があれば、判定スクリプトが止まる。**ファイルにも効かせる**（設計レビュー 3 回目の H1）：`MIT` を含む SPDX をファイルに書けるのは、決定のファイルにそのパスの行があるときだけで、新しいファイルは `Apache-2.0` だけ（C5）。TSV の手直しは、決定のファイルから計算し直して見つける。試作 M-b、M-c、M-f、M-j、M-k が落ちることを `classify_file_origin_test.py` の `ManifestCheck` で確かめる。 |

### 7.2 PBT の性質と除外集合

`tools/license-headers/license_headers_test.py` 内に、外部依存を増やさない seed 固定のランダム生成テスト（標準ライブラリ `random.Random(seed)` で最低 5,000 ケース／テスト、任意で `hypothesis` でも再現可能な純粋関数構造）を組み込む。入力生成には ASCII・日本語・BOM（`\ufeff`）・shebang（`#!/...`）・`#![...]`・`//!`・`///`・CRLF・LF・混在改行・空ファイル・先頭空行・末尾改行なし・U+2028/U+2029・移植マーカー（1 行目／20 行目／21 行目／偽陽性語）・ヘッダー断片を混ぜる。

| 性質 | 内容 |
|:---|:---|
| P1（往復） | `parse_header(x).state == NONE` かつ「改行なし shebang 単独 1 行」でない `x` で、`apply(x, k)` が成功するものに対し、`remove(apply(x, k)) == x`。生成器は断られる入力も混ぜ、その割合を出す（断られる入力ばかりで P1 が空振りしないことを確かめる）。 |
| P2（除外集合の安全保証） | 除外集合（改行を含まない `#!/...` のみの 1 行入力）に対しては、`apply` と `check` が必ずエラーを返し、サイレントな破損や不整合な変換を行わない。 |
| P3（冪等） | `apply(x, k)` が成功して `y` を返すとき、`apply(y, k) == y`。 |
| P4（種別衝突拒否） | `apply(x, pekko)` の結果に対し `apply(..., original)` は必ずエラー。逆も同様。 |
| P5（検査との整合） | `apply(x, k)` の結果 `y` は、`k == original` かつ `has_pekko_marker(x)` の矛盾ケースおよび本文に既存 `SPDX-License-Identifier:` を含むケースを除き、`check_file(y)` が常に合格する。また `remove(y)` は常に `check_file` で不合格になる。 |
| P6（改変ヘッダーの非破壊除去） | 任意の本文 `body`（`// Ported from Pekko` 始まりや `//!` 始まりを含む）の前に、年号・Copyright 行・区切り空行有無を変えた既知アンカー付きヘッダーを前置した入力 `y` に対し、`remove(y)` は `body` を 1 バイトも欠損させず、`apply(y, same_kind)` は二重に SPDX を付けない。 |

### 7.3 Quint の要否

- **不要**（前レビュー F5 でも不要と合意済み）。
- 根拠：本ツールは各ファイルに対して独立に動く純粋なテキスト変換であり、並行処理・ロック・キュー・再試行・再開・反復上限・複数アクター間の相互作用を持たない。ファイル単位の状態は `{NONE, APACHE, DUAL, EXTERNAL, UNRECOGNIZED}` の 5 値のみであり（段 A から段 B への移り変わりは、人が決定のファイルに行を足して TSV を作り直す一方向の手順で、ツールの状態遷移ではない）、単体テストと PBT で網羅的に検証できる。

### 7.4 壊しうる入力の一覧

| 入力 | 起きうる問題 | 本設計での対処 |
|:---|:---|:---|
| **CRLF（`\r\n`）および混在改行** | 読み書き時の改行正規化で往復が壊れる | `open(..., encoding="utf-8", newline="")` で変換なし読み書き。ファイルに `\r\n` が含まれるなら挿入ヘッダーも `\r\n`、無ければ `\n`。本文の改行は一切変えない |
| **UTF-8 BOM（`\xef\xbb\xbf`）** | BOM の前にコメントが入ると `rustc` が `unknown start of token: \u{feff}` で失敗（前レビュー E3） | BOM をプレフィックスとして分離し、BOM の**直後**にヘッダーを挿入・認識・除去する（コンパイル可かつバイト往復） |
| **shebang（`#!/...`）** | 1 行目の前にコメントが入るとスクリプトとして実行不能 | 1 行目が `#!` で始まり、その後（空白を除く）が `[` でない場合のみ shebang として分離し（rustc と同じ判定。L3）、2 行目からヘッダーを挿入・認識・除去する。改行なし shebang 単独 1 行はエラーで拒否（P2） |
| **`#![...]` 内側属性・`//!` インナードキュメント** | ヘッダーと結合して属性や rustdoc が壊れる、または `--remove` が `//!` を誤削除する | 通常の `//` コメント＋空行 1 行を前に置くのは Rust 文法・`rustdoc`・`syn-check`・`rustfmt` いずれも合法（`crates/` の `//!` 263 件で検証）。ブロック認識は `///`・`//!` で即終了するため誤削除しない |
| **既存の SPDX（正常・旧版・年号変更・#218 固有 Copyright・空行欠落・未知式・二重 SPDX）** | 完全一致のみだと二重付与・削除不能（F1 / E1 / E6） | 構造化ヘッダーブロック認識＋終端アンカー（§3.4.3）。未知式・終端欠落はエラー停止。本文中の 2 つ目の `SPDX-License-Identifier:` は `--check` で二重ヘッダーとして落とす |
| **原典の表示の写し（§3.1）** | 原典のファイル先頭の書式（`/* */`、` * ` の字下げ、`//` の行、ブロックの間の空行）を写し損なうと原文でなくなる。100 文字を超える行は rustfmt の `wrap_comments` が折り返す | 写し方を §3.1 に固定し、判定スクリプトの `leading_blocks` と同じ関数をツールが使う。適用の後に、写した行が原典の行と一致することをテストで確かめる。100 文字を超える行が出るなら適用を断る |
| **決定のファイルの誤り**（列の欠け、一覧に無いパス、二重の決定、強い根拠への MIT） | 人の決定が黙って捨てられる、または MIT が誤って付く | 判定スクリプトが行番号つきで止まり、TSV を書かない（§3.3 の「人の決定」） |
| **生成ファイル・テストフィクスチャ・正本写し** | 生成物や行番号依存テストやハッシュ固定ファイルを壊す | 走査は `crates/**/*.rs`（`target/` 除外）に限定。現状 `crates/` に生成ファイル・`build.rs` は 0 件。`tools/syn-check/tests/`（254 件）と `scripts/aislop-check-bootstrap.mjs` は対象外。暗黙の `@generated` 自動スキップは抜け道になるため入れない |
| **非 UTF-8 バイト列** | `UnicodeDecodeError` のトレースバックで途中停止（前レビュー E7） | 事前読み込み段階で捕捉し、ファイルパス付きエラーを出して 1 件も書かず終了コード 1 |
| **Unicode 改行（U+2028 / U+2029 / U+0085）** | Python の `str.splitlines()` がこれらで分割し行番号・ブロック境界がずれる | `splitlines()` を使わず `\r\n` / `\n` のみで分割する |
| **空ファイル・空行のみ・先頭空行あり・末尾改行なし** | 区切り空行の過不足で `remove(apply(x)) != x` になる | 空本文なら区切り空行なし、非空本文なら区切り空行ちょうど 1 行を追加・除去する（§3.4.3） |
| **偽陽性マーカー（`Imported from Pekko` 等）・21 行目境界・マーカー改変** | 誤検出（F2）、境界漏れ（F8）、ヘッダー孤立（F4） | 大文字小文字区別＋語境界 `\b`、本文冒頭 20 行厳密窓、`--remove` と `--check` はマーカー有無に依存せず全ファイル走査 |
| **線引きのスクリプトとヘッダーツールのマーカー定義の食い違い** | `classify_file_origin.py` の移植マーカーは、現状 `apply_ported_headers.py` と同じ旧定義（大文字小文字を区別せず、語の境界を見ない）。ツールだけ厳密にすると、一覧の判定とツールの C3 がずれる | ツール改修（§8.1 の 2）で、`classify_file_origin.py` が `license_headers.py` のマーカー判定（`\b(Ported from\|Port of\|Checked against) Pekko\b`）を読み込んで使うようにし、TSV を作り直す。作り直した TSV の件数（移植マーカー 52）と、`--check --manifest` の結果が §3.3 と一致することを確かめる。ずれた行は元帥へ上げる。今の TSV（433 行）は旧定義で作ったが、この版の実測で、旧定義と厳密な定義の結果は `crates/` で同じ 52 件である |

### 7.5 上流・保存済みデータ・公開 API・守りの仕組みへの影響

- **上流（`pekko/`）**：影響なし（変更しない）。
- **公開 API・実行時挙動**：`crates/**/*.rs` への行コメント追加と `Cargo.toml` のメタデータ `license` 追加のみ。Rust の公開型・シグネチャ・実行時動作は変わらない。
- **保存済みデータ・台帳（`ledger/nodes.yaml`）**：台帳はファイルパスのみを保持し行番号を持たないため、`pekko_baseline` / `relocate_nodes` / `report_ledger_1to1` への影響なし。
- **守りの仕組み**：
  - `cargo fmt`（stable / nightly）：ヘッダー行は最大 81 文字（`comment_width = 100` 以内）のため整形差分なし。
  - `syn-check`：`//` 行コメントは AST にも `rustdoc`（`///`/`//!`）ブロックにも影響しない。
  - `aislop`：`crates/` の非テスト実装ファイルの行数は、既に `scripts/aislop-accepted.json` で受容済みの `executor_value.rs`（1,767 行）を除くと最大 622 行（`frequency_list_value.rs`）。Pekko 由来のヘッダーは最大 31 行（区切りの空行を含む）なので、`complexity/file-too-large` 閾値（1,100 行超）に新規到達するファイルは 0 件。
  - `docs/goals/` 内の行番号参照：`crates/.../*.rs:行` の参照は全 15 件（`docs/goals/issue-105-status-reply.md` 13 件、`docs/goals/issue-96-supervisor-tree.md` 1 件、`docs/design/issue-207-unhandled.md` 1 件）が、Pekko 由来のファイルで 16〜31 行、自作のファイルで 7 行ずれる。いずれも過去コミット時点のコンパイラ出力や調査ログの記録であり `check-refs` の検査対象外であるため、過去の記録として書き換えずに残す（F10）。

---

## 8. 適用の順番と元に戻し方

### 8.1 適用の順番

**公開リポジトリのブランチへ push した時点で、その版は公開される**（§2.3）。マージを待たない。だから、段の切れ目は「push してよいか」で決める。段 A は `Apache-2.0` だけを公開し、段 B で初めて `MIT OR Apache-2.0` を公開する（決定 6.1）。

| 順 | 作業 | 完了条件（ゲート） |
|:---:|:---|:---|
| 0 | **設計の独立レビュー**（3 回目は済み。いまは変わった点の再レビュー） | 実装に関わっていない Claude Opus（`claude-opus-5-5`）が本設計をレビューし、全指摘を「直す／別件／採らない」で処理完了（3 回目の処理は §10） |
| 1 | **線引き一覧と決定の確定** | この設計のコミットで、TSV（433 行）、判定スクリプト、決定のファイル（要確認は一括で `Apache-2.0`、決定 6.2）、候補の一覧を置いた。ゲートは、TSV が判定スクリプトと決定のファイルから作り直せて、バイト単位で一致すること |
| 2 | **ツール改修と検証** | `tools/license-headers/license_headers.py` と `license_headers_test.py`（F1〜F8・PBT、§3.1 の原典の表示の写し、C5、C6（`表示のハッシュ` の列を含む）、外部のヘッダー、L3 の断り、I6〜I9 のテストを含む）を実装し、旧 `apply_ported_headers.*` を削除。`classify_file_origin.py` を新しいマーカー定義にそろえて TSV を作り直し、§3.3 の件数と一致を確かめる（§7.4 の最終行）。テスト全件合格 |
| A | **段 A：`Apache-2.0` だけを付けて push する** | 中身：`LICENSE` の削除、`LICENSE-APACHE`（201 行）、`NOTICE`（原典 74 行全文に加え、`pekko/LICENSE` の hash-prospector の節（272〜299 行）と Caffeine の節（303〜305 行）を原文どおり写す）、段 A の `README.md`（§2.5）、各 `Cargo.toml` の `license = "Apache-2.0"`、`fast_frequency_sketch.rs` のコメントを原典 334〜336 行の文面に直す、85 件の「既知の例外」の一覧（[`issue-206-known-exceptions.md`](./issue-206-known-exceptions.md)。年と著作権者は未確認と書く）、`--apply --manifest` で `付けるSPDX = Apache-2.0` の 295 件にヘッダー（保留の 55 件は書き換えない）、CI の `License headers`（`--check --manifest`。保留は SPDX が無い限り `held` で合格）。ゲート：`--check --manifest` が `held 55` で 0、`cargo test` / `clippy` / `fmt` / `syn-check` / `aislop` / 全 Python テスト / 台帳検査がすべて 0。**push の前に `git grep -l 'MIT OR Apache-2.0' -- crates` が 0 件で、`LICENSE-MIT` が無いことを確かめる** |
| B1 | **候補の照合** | [候補の一覧](./issue-206-mit-candidates.md) の 55 件を、実装に関わっていない担当が 1 件ずつ原典（Pekko 2.0.x のどのファイルにも、型・関数・テストの形の対応が無いこと）と照らし、結果と根拠を書く。「Pekko に対応が無い」と 1 件ずつ決めるのは muse か Claude の 1 本で、pi は材料を集めるところまで（決定 7.4）。`tools/syn-check` の移植元は、オーナーの公開リポジトリ `j5ik2o/fraktor-rs` の `lints/` と確かめた（決定 7.3。下の注） |
| B2 | **元帥の確認** | 元帥が、照合の一覧（件数と各ファイルの根拠）を読む。ファイルごとの決定を決定のファイルに足す（自作と確かめたものは `MIT OR Apache-2.0`、ほかは `Apache-2.0`）。TSV を作り直し、`付けるSPDX` に `保留` が残らない |
| B | **段 B：`MIT OR Apache-2.0` を付けて push する** | 中身：`LICENSE-MIT`、段 B の `README.md`（二重ライセンスと貢献条項、§2.5）、`tools/syn-check/Cargo.toml` の `license = "MIT OR Apache-2.0"`、`--apply --manifest` で B2 の決定どおりのヘッダー。ゲート：`--check --manifest` が `held 0` で 0、段 A と同じ全検査が 0。**この push で `MIT OR Apache-2.0` の許諾が公開され、取り消せなくなる** |
| 5 | **PR #215 のマージ** | 必須チェック全緑・未解決スレッド 0 件を確かめてマージする（許諾はすでに段 A と段 B の push で公開されている） |
| 6 | **移植手順スキルの更新** | `~/.orca/bots/fraktor-ng-rs/skills/fraktor-migration-intake/SKILL.md` へ §6 の手順を反映 |

**段 B1〜B は当面行わない**（決定 7.5）。段 A のあと、55 件は `保留` のまま（ヘッダーなし、クレートの `license = "Apache-2.0"`）にする。

`tools/syn-check` の移植元（決定 7.3）：`tools/syn-check/README.md` は「旧 fraktor-rs が dylint で運用していた lint のうち 10 本を、syn の上に移植した」と書き、`j5ik2o/fraktor-rs` の `fea328f` を引く。設計レビュー 3 回目の調べでは、`j5ik2o/fraktor-rs` は公開リポジトリで `LICENSE-MIT` を持ち、`lints/`（12 の `*-lint`）へのコミットは j5ik2o（Junichi Kato）52、renovate[bot] 9（依存の更新）、Copilot のエージェント 1（j5ik2o との共著）である。このリポジトリの `tools/syn-check` のコミットは 15 件すべて Junichi Kato（`git log -- tools/syn-check`）。オーナー自身のリポジトリから来たと確かめられたので、MIT のまま進める。

段 A と段 B は別の push にする。段 B の照合で Pekko 由来と分かったものは `Apache-2.0` の決定を足すだけで、段 A で公開した内容は変わらない。

### 8.2 元に戻し方（可逆範囲と不可逆範囲）

#### 可逆な操作（やり直せる範囲）

- **ヘッダー適用を戻す**：
  ```bash
  python3 tools/license-headers/license_headers.py --remove
  ```
  `crates/**/*.rs` からツール管理のライセンスヘッダーブロック（2 種とも、旧版・改変版含む）をマニフェスト・マーカーに依存せず一括除去する。決定のファイルで許した外部のヘッダーは消さずに一覧に出す。手元の作業ツリーと、push する前のコミットは、これで戻せる。
- **個別ファイルの分類を付け替える**：決定のファイルに行を足すか直し、TSV を作り直して、`--remove` と `--apply --manifest` をやり直す。push の前なら、どちら向きの付け替えも公開に残らない。
- **push の後に分類を付け替える**：`Apache-2.0` から `MIT OR Apache-2.0` へ広げる変更（要確認だったファイルが自作と分かった場合）は、権利者が許諾を足すだけなので後から行える。逆向きは下の不可逆の項。
- **ツール・CI・README・`Cargo.toml` の変更を戻す**：通常の `git revert` で戻せる（すでに公開した許諾は戻らない）。

#### 不可逆な操作（戻せない範囲・F3、H1）

- **公開リポジトリのブランチへ push した版の許諾**：`LICENSE-APACHE`、`LICENSE-MIT`、README のライセンス節、各ファイルの `SPDX-License-Identifier` を、公開リポジトリのどのブランチへでも push した時点で、その版のソースに対する許諾は**取り消せない**（Apache License 2.0 第 2 条の perpetual / irrevocable、MIT ライセンスの恒久許諾）。マージの有無は関係ない。後日ファイルを削除・`git revert` したり、あるファイルを `MIT OR Apache-2.0` から `Apache-2.0` へ狭めたりしても、その版を取得した人への許諾は存続する。
- **ただし、Pekko 由来のファイルに誤って付けた MIT は、もともと有効な許諾ではない。** オーナーが MIT で出せるのは自分の書いた部分だけだからである。Pekko 由来と分かったら `Apache-2.0` へ直す（意味があり、直す義務もある）。直したことは、その PR に理由とともに書く。
- この不可逆性が、「迷うファイルは最初から安全側（`Apache-2.0`）に倒す」ルール、段 A と段 B を分けて `MIT OR Apache-2.0` を照合と元帥の確認のあとに回す順番、「`--check --manifest` で一覧に無い付け替えを CI で止める」ルールの根拠である。

---

## 9. 設計レビュー 2 回目（`pr215-design-review2.md`、固定 `b6a4d69`）の指摘への答え

扱いは「直す」「別件」「採らない」のどれか。元帥の決定（2026-11-26、前提の 6）で決まったものは、その番号を書いた。

| # | 重さ | 指摘の要約 | 扱い | 理由と、直した箇所 |
|:---|:---|:---|:---|:---|
| H1 | 高 | 判定の取りこぼしで Pekko 由来のファイルに `MIT OR Apache-2.0` が付き、push の時点で公開される | 直す | 判定に H1 の規則 3 つ（型名を大文字小文字を区別せずに照合、`Type.method`、「corresponding to」「the ported」「original `X`」）を足した。前の版の 60 件に流すと、挙げられた 6 件がちょうど要確認へ移る（§3.3）。あわせて規則 4（第三者のコードへの言及）と、行をまたぐ「the original」を足した（main から入った `murmur3.rs` が MIT の候補に残るため）。順番は決定 6.1 のとおり、段 A（`Apache-2.0`）を先に push し、`MIT OR Apache-2.0` は 1 件ずつの照合と元帥の確認のあとの段 B に回した（§8.1）。取り消せなくなるのは push の時点、と §2.3 と §8.2 に直した。「狭めても効果がない」に、Pekko 由来に誤って付けた MIT は有効な許諾ではなく、直す義務がある、と注記した（§3.3、§8.2）。候補 55 件は [候補の一覧](./issue-206-mit-candidates.md) に置いた。照合は別の担当が行う |
| H2 | 中〜高 | DnsSettings の Netty の著作権表示がヘッダーから落ちる。「付録に当たるのは commons-io だけ」は誤り | 直す | 決定 6.4 により、原典のファイル先頭の表示を原文どおり写す（§3.1）。第三者の表示を持つ原典を機械で探し、写す先を TSV の `第三者の表示` に出した。Netty 15 件、Ben Manes 24 件、scala-library 5 件、hash-prospector 1 件（§2.1）。§2.1 の誤りを直した。§6 に「原典ファイルの先頭の表示を写す」手順を足した |
| M1 | 中 | CI の C3 は Pekko 由来の一部しか守れない。C3 の Mirrors の窓が不明 | 直す | 決定 6.5 により、`--check --manifest` を CI に入れ、TSV の全 433 行と新しいファイルのすべてに効かせる（C5、§3.4.4）。既存のファイルの付け替えには、決定のファイルと TSV の差分が要る。C3 の Mirrors はファイル全体を見ると明記した（C3 が守るのは 295 件のうち 128 件、残る 167 件は C5 が守る） |
| M2 | 中 | TSV を作り直すと、手で直した元帥の決定が消える | 直す | 決定 6.5 により、人の決定を [`issue-206-origin-decisions.tsv`](./issue-206-origin-decisions.tsv) に分けた。スクリプトが最後に重ね、`付けるSPDX` と `決定の出典` に出す。決定 6.2（要確認は一括で `Apache-2.0`）を 2 行目に記録した（§3.3 の「人の決定」） |
| M3 | 中 | 第三者の SPDX を持つファイルで `--check` が落ち続け、`--remove` が止まる | 直す | 決定 6.5 により、決定のファイルの「パスと式の対」を許可の一覧にした。C1 はその式を許し、`--remove` は外部のヘッダーを飛ばして一覧に出す（§3.4.2、§3.4.4）。`fast_frequency_sketch.rs` の `Apache-2.0 AND Unlicense` は提案として `#` 付きで置き、元帥の決定を待つ（許可の一覧に入れるのは人だけ） |
| M4 | 中 | 貢献条項が無く、外部の貢献が `Apache-2.0` だけで入る | 直す | 決定 6.5 により、README に Rust の二重ライセンスで標準の貢献条項と、Pekko 由来のファイルへの貢献は `Apache-2.0` で入る旨を入れる。二重ライセンスの説明と同じく段 B で入れる（§2.5） |
| L1 | 低 | `Cargo.toml` の「簡易監査ツールに混乱」に根拠が無い。syn-check の移植元が自作かを確かめる | 直す | 決定 6.3 により `license = "Apache-2.0"` のまま、根拠の無い一文を消し、複合の式も正しいと書いた（§4.1）。syn-check の確認は段 B1 に入れた（§8.1、候補の一覧） |
| L2 | 低 | アンカーの文言を 1 語変えると `UNRECOGNIZED` になり、`--remove` が止まる | 直す | アンカーの行は固定とし、変えるときは旧アンカーを既知の一覧に残してテストを足す、と書いた（§3.4.3） |
| L3 | 低 | 先頭以外の手書きの SPDX があると二重に入る。移植マーカーのあるファイルにも `--kind original` が通る。shebang の判定が rustc と違う | 直す | `apply` の中でも C1 の再出現と C3 を確かめて断る（§3.4.2、§7.1 I3）。shebang は rustc に合わせ、`#!` の後（空白を除く）が `[` でない 1 行目を shebang とする（§3.4.3）。実装は段 2 |
| L4 | 低 | 区切りの空行が消え、本文が空行で始まるときに、`remove` が空行を 1 行消す | 直す | I5 にこの 1 行の差を書き足した（§7.1）。内容としては害が無い |
| L5 | 低 | ASF の表示を言い換えている。年の範囲が一律 | 直す | 決定 6.4 により、ASF の表示を原文どおり写す。Lightbend の行も原典のファイルごとの年で写す（§3.1）。ASF 標準の形を持つ原典 11 件も原文になる。#218 の目的はこれで満たす見込みで、閉じるかは元帥が決める（§1.3） |
| L6 | 低 | 自作のヘッダーの年と、貢献者が増えたときの書き方が決まっていない | 直す | 決定 6.4 により著作権者は `Junichi Kato` に固定する。年はそのファイルを最初に公開した年で、`--apply --kind original` は適用した年を書く（§3.1）。貢献は README の貢献条項（M4）で受ける |
| L7 | 低 | 走査がファイルシステムを歩き、`crates/` の下の `pekko` を黙って飛ばす | 直す | 判定スクリプトの対象を `git ls-files` で決めるように直し、`crates/` の下は名前で飛ばさない（§3.3）。ヘッダーツールも同じにする（§3.4.1） |

#### この版で、指摘とは別に変えたこと

| 変えたこと | 理由 |
|:---|:---|
| main（`f54008c`）を取り込み、件数を測り直した（`crates/**/*.rs` 324 → 350） | #205（FrequencySketch）、#207、#223 が main に入った。古い木で作った一覧では、増えた 26 件にヘッダーの行が無く、FrequencySketch の第三者の表示（M3 の例そのもの）も扱えない。CI の merge parents の検査も、main より遅れた PR を落とす |
| 規則 4 と、行をまたぐ「the original」 | H1 の 3 つの規則だけでは `murmur3.rs`（scala-library の MurmurHash3 を写す）と `sketch_depth.rs` が「自分で書いた」に残る。迷ったら `Apache-2.0` に倒す（決定 6） |
| `原典ファイル` と `第三者の表示` の列 | 決定 6.4 の「原文どおり」を、ファイルごとに機械で再現できるようにするため（§3.1） |
| 判定スクリプトを 4 つのファイルに分け、テストを足した。前の版の `--out-md` を消した | `aislop` の `complexity/file-too-large`（400 行）などに当たったため。受け入れ一覧に足すのは抑制なので人が決めることで、ここではコードを直した。`--out-md` の内容（件数の表）は §3.3 にある |

---

#### PR #215 のレビューボット（`2b43004` に付いた CodeRabbit の 6 件）

| 場所 | 指摘 | 扱い | 中身 |
|:---|:---|:---|:---|
| 設計 §7.1 I1・I2、§7.2 P1 | 往復性と冪等性の対象から、`apply` が断る入力を外す | 直す | I1、I2、P1 を「`apply(x, k)` が成功するなら」に直した。断る入力は I3 と拒否のテストで見る |
| `NOTICE` | 省略した節の理由が、この版の移植（Netty、scala-library）と合わない | 段 A で直す | 段 A で原典 `NOTICE` の全文に差し替える（§2.4、§8.1）。ルートのファイルの差し替えは、設計の独立レビューの後と決まっているので、この段では変えない |
| `apply_ported_headers.py` の `--list` | 書き込む前の本文で状態を出す | 新しいツールで直す | 指摘どおりだった（`entries` が書き込む前の本文）。旧ツールは段 2 で消すので、新しいツールの `--list` が書き込んだ後の本文で出すことを §3.4.2 に書いた |
| `classify_file_origin.py` | 例外で止まると一時の展開先が残る | 直す | 展開と索引づくりを `open_pekko` のコンテキストマネージャーにまとめ、例外のときも消す。展開の途中で失敗したときも消す。テストを足した |
| `origin_evidence.py` の `mention_evidence` | `org.apache.pekko` だけの行で根拠が None になり、TSV を書く前に落ちる | 直す | 最後の照合で `org.apache.pekko` も拾う。テストを足した。今の木では TSV は変わらない |
| `origin_pekko.py` の `--pekko-tree` | 別のリビジョンの展開先を、`build.sbt` があるだけで再利用する | 直す | 固定コミット専用の展開先にした（`.pekko-commit` の印。一致しなければ止まり、中身は消さない）。テストを足した |

CI の `relocate_nodes.py check` が、TSV の `根拠の中身` に引いた旧 id（根拠表の `move` の行）で落ちた。`b6a4d69` から潜んでいた（そのときの CI は merge parents の検査で先に止まっていた）。根拠の中身では旧 id を `<移動前の id>` に置き換える（§3.3）。

---

## 10. 設計レビュー 3 回目（`pr215-design-review3.md`、固定 `365d6da`）の指摘への答え

扱いは「直す」「別件」「採らない」のどれか。オーナーの指示（範囲を広げない）により、手間のかかる低の指摘は別件として PR 本文に 1 行ずつ書いた。

| # | 重さ | 指摘の要約 | 扱い | 理由と、直した箇所 |
|:---|:---|:---|:---|:---|
| H1 | 高 | 新しいファイルには、書いた人の判断だけで MIT が付き、push の時点で公開される | 直す | 規則「`MIT` を含む式は、決定のファイルの行からしか生まれない」を §3.2 に書き、C5 に入れた（§3.4.4）。新しいファイルは `Apache-2.0` だけ。§6 を「新しいファイルは、まず `Apache-2.0` で付け、広げるのは決定のファイルで後から行う」に直し、push の前に `--check --manifest` を流す手順を入れた。I9 をファイルにも効かせた（§7.1）。検査は `origin_decisions.manifest_errors`、試作 M-f が落ちることを `ManifestCheck` で確かめた |
| M1 | 中 | TSV を手で直すと、決定のファイルも I9 も通らずに付け替えられる | 直す | C5 で、TSV の `判定`、`根拠の種類`、`要確認` と決定のファイルから `付けるSPDX` と `決定の出典` を計算し直し、食い違えば `manifest-tampered` で落とす。行を消しても H1 の規則で MIT は通らない。消したこと自体は、push の前の作り直しと `cmp` で見つける（§3.4.4、§6）。試作 M-b、M-c、M-j が落ちることを `ManifestCheck` で確かめた。CI での作り直しは、CI に原典が無いので入れない |
| M2 | 中 | 決定の式を検査していないので、強い根拠のファイルにも MIT が通る | 直す | 決定の式の文法を 3 つの形に固定し、ほかは行番号を付けて止める（§3.3、`origin_decisions.valid_expression`）。`MIT` を含む式はすべて MIT として扱い（`mentions_mit`）、強い根拠の守り、C3、新しいファイルの規則を効かせる。レビューが試した 6 通りの綴りがすべて止まり、試作 M-k が落ちることをテストで確かめた |
| M3 | 中 | 写した原典の表示が CI で守られない | 直す（設計） | C6 を §3.4.4 に足した。第三者の表示の行がヘッダーにあること、写したブロックのハッシュが TSV の新しい列 `表示のハッシュ` と一致すること。テストの案も書いた。実装は段 2 |
| L1 | 低 | `Apache-2.0 AND Unlicense` の材料が足りない。提案の行の `#` を外すだけで決定になる | 直す | (a)(b)：元帥が決定 7.1 で `Apache-2.0` だけと決めた。`Cargo.toml` は決定 6.3 のままで、ぶつからない（§2.1）。(c)：提案の行は `決めた人` を空にし、空でなければ止まる。`日付` は `YYYY-MM-DD` の実在する日付に限る（§3.3、テスト `DecisionFile`）。提案の行そのものは決定 7.1 で消した |
| L2 | 低 | #218 を閉じる材料が足りない（85 件は既定の表示） | 直す | 決定 7.2 のとおり、C6 と 85 件の「既知の例外」の一覧（理由つき、年と著作権者は未確認）で閉じる。閉じるのは適用の PR がマージされてから。一覧は段 A の PR で置く（§1.3、§3.1） |
| L3 | 低 | ファイル先頭に Copyright が無い第三者の由来（`LICENSE` の付録にだけあるもの）は機械で見つからない | 別件 | 今は 0 件。将来の移植の取りこぼしなので、PR 本文に別件として書いた |
| L4 | 低 | 照合の基準が Pekko しか見ていない（JDK の `ResolverConfigurationImpl.c` など） | 別件 | 段 B を当面行わない（決定 7.5）ので、段 B1 の基準に足すことを PR 本文に別件として書いた |
| L5 | 低 | §6 の「`main` へ公開すると」と、作業仕様の `--check` | 直す | 「push した時点」に直し、作業仕様の文を `--check --manifest` にした（§6） |
| L6 | 低 | 段 A と段 B の間の外部の貢献 | 別件 | 段 B を当面行わない（決定 7.5）ので、段 B1 の照合でオーナー以外のコミットが無いことを確かめる、と PR 本文に別件として書いた |

元帥の決定（第 2 回・第 3 回）の入れ方：

| 決定 | 入れた箇所 |
|:---|:---|
| 7.1 `fast_frequency_sketch.rs` は `Apache-2.0` だけ。理由の訂正（Unlicense は hash-prospector、Caffeine は Apache-2.0）。hash-prospector 由来の部分の確認 | 前提の 7.1、§1.2、§2.1（原典と照らした結果）、§2.4、§3.3。決定のファイルから提案の行を消した |
| 7.2 #218 は適用の PR のマージのあと、C6 と 85 件の一覧で閉じる。85 件にも ASF の表示 | 前提の 7.2、§1.3、§3.1、§3.4.4 の C6 |
| 7.3 `syn-check` の移植元はオーナーのリポジトリ。MIT のまま | 前提の 7.3、§8.1 の注、[候補の一覧](./issue-206-mit-candidates.md) |
| 7.4 MIT の判定は muse か Claude。pi は材料まで | 前提の 7.4、§8.1 の B1、候補の一覧 |
| 7.5 段 B は当面行わない | 冒頭の状態、前提の 7.5、§8.1 |


---

## 付録：実測コマンド

```bash
# 対象ファイル数と種別（追跡しているものだけ）
git ls-files 'crates/**/*.rs' | awk -F/ '{print $1"/"$2}' | sort | uniq -c

# origin/main のコミット名義と年
git shortlog -sne origin/main
git log origin/main --format='%ad' --date=format:'%Y' | sort -u

# Apache-2.0 原文（1..201 行）と原典 NOTICE 全文（74 行）の SHA-256
git -C ~/Sources/github.com/j5ik2o/pekko show 9fbd2283b1cf701c90a1223bf621942c191db61f:LICENSE | sed -n '1,201p' | shasum -a 256
git -C ~/Sources/github.com/j5ik2o/pekko show 9fbd2283b1cf701c90a1223bf621942c191db61f:NOTICE | shasum -a 256

# 線引き一覧の再生成と、書き込み済みの TSV との一致（TSV と同じ内容が出る）
PEKKO_REPO=~/Sources/github.com/j5ik2o/pekko uv run --with pyyaml python3 tools/license-headers/classify_file_origin.py --out-tsv /tmp/origin-check.tsv
cmp /tmp/origin-check.tsv docs/design/issue-206-file-origin.tsv

# 判定スクリプトのテスト（原典の git は要らない）
python3 tools/license-headers/classify_file_origin_test.py

# 要確認の件数（走査範囲べつ。116 と 83）
awk -F'\t' 'NR > 1 && $5 == "要確認" {print $7}' docs/design/issue-206-file-origin.tsv | sort | uniq -c

# 付けるSPDX の件数（crates：Apache-2.0 295、保留 55）
awk -F'\t' 'NR > 1 && $7 == "crates" {print $8}' docs/design/issue-206-file-origin.tsv | sort | uniq -c

# 第三者の表示を写すファイル（39 件）
awk -F'\t' 'NR > 1 && $11 != "" {print $1 "\t" $11}' docs/design/issue-206-file-origin.tsv

# tools/syn-check/src の Pekko 言及有無
rg -i 'pekko|akka|lightbend' tools/syn-check/src/

# docs/ 内の crates/*.rs:行 参照件数
git grep -cE 'crates/[^ ]+\.rs:[0-9]+' -- docs/
```
