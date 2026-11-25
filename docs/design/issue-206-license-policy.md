# 設計：#206 移植ファイルのライセンス表記の方針（提案）

[Issue #206](https://github.com/j5ik2o/fraktor-ng-rs/issues/206) の判断材料と、方針決定後に使う可逆な実装である。**方針の最終決定は元帥が行う**。この文書は提案であり、決定ではない。

- 背景：移植した実装・テストには原典（Apache Pekko）の出典と SHA をコメントで書いているが、ライセンス表記が無い。リポジトリに `LICENSE` と `NOTICE` が無い。別担当の [PR #205](https://github.com/j5ik2o/fraktor-ng-rs/pull/205) に対する Devin のレビューで指摘された。
- この PR で置いたもの：`LICENSE`、`NOTICE`、ヘッダー一括スクリプトとそのテスト、この文書。**ヘッダーの適用はしていない**（方針決定後）。
- 設計の独立レビュー：未実施（Opus 復帰後に実施）。

## 1. 事実（実測）

### 1.1 リポジトリのライセンス表示の現状

| 調べたこと | コマンド | 結果 |
|:---|:---|:---|
| ライセンス系ファイルの追跡 | `git ls-files \| grep -iE '(^\|/)(licen[cs]e\|notice\|copying)'` | 0 件（この PR の前） |
| `package.json` の宣言 | `node -p "require('./package.json').license"` | `Apache-2.0` |
| GitHub の認識 | `gh repo view j5ik2o/fraktor-ng-rs --json visibility,licenseInfo` | `visibility: PUBLIC`、`licenseInfo: null` |

`package.json` は Apache-2.0 と言っているが、`LICENSE` が無いため GitHub はライセンスを認識していない。README は 1 行（`# fraktor-ng-rs`）だけで、ライセンスの記述は無い。

### 1.2 移植済みファイルの数と分布

台帳（`ledger/nodes.yaml`）が進捗の正である。2026-11-26 時点の実測：

```
nodes 1271
status {'unported': 1020, 'green': 52, 'do_not_port': 197, 'not_built': 2}
with test_file 51
retired 1
```

- green のノードのうち 51 件が Rust のファイルを持つ（50 件が統合テスト、1 件が `crates/fraktor-testkit/src/fraktorspec/fraktor_spec.rs`）。
- 退役（`retired`）した 1 件も green で、`crates/fraktor-actor/tests/actor_system_scala213_plus_spec.rs` を持つ。v2.0.0 に原典ファイルが無くなった記録として残している。

ファイル冒頭の移植マーカーを数えると、台帳とほぼ一致する。

```
$ grep -rlE 'Ported from Pekko|Port of Pekko|Checked against Pekko' --include='*.rs' crates | wc -l
51
$ grep -rhoE 'Ported from Pekko|Port of Pekko|Checked against Pekko' --include='*.rs' crates | sort | uniq -c
     38 Checked against Pekko
      1 Port of Pekko
     48 Ported from Pekko
$ ... | cut -d/ -f2 | sort | uniq -c
     43 fraktor-actor
      8 fraktor-testkit
$ ... | awk '{if ($0 ~ /\/tests\//) print "integration_test"; else print "other"}' | sort | uniq -c
     51 integration_test
```

- マーカーのある 51 ファイルは**すべて統合テスト**である（`crates/*/tests/`）。
- 台帳の 51 ファイルとの差は 1 件ずつ。台帳だけが `fraktor_spec.rs`（テスト基盤の実装ファイル、マーカー無し）を持ち、マーカーだけが退役ノードの `actor_system_scala213_plus_spec.rs` を持つ。

**実装ファイルはこれより多い。** 台帳のノードはテストクラス単位なので、移植した実装ファイルはノードにならない。

```
$ find crates -path '*/src/*' -name '*.rs' ! -name '*_test.rs' | wc -l       # 実装ファイル総数
190
$ grep -rl 'Pekko' --include='*.rs' crates/*/src | grep -v '_test\.rs$' | wc -l   # Pekko に触れる
85
$ grep -rlE 'Mirrors `org\.apache\.pekko' --include='*.rs' crates/*/src | grep -v '_test\.rs$' | wc -l  # 原典の型に対応すると明記
76
$ find crates -name '*.rs' | wc -l                                            # 全 Rust ファイル
324
```

実装 76 ファイルは rustdoc に「`org.apache.pekko.X` を写した」と書いているが、テストのような統一マーカーは無い。**この PR のスクリプトの対象は、統一マーカーのある 51 ファイルだけ**である（§3）。

### 1.3 原典ヘッダーの有無

- 移植した Rust ファイルには、原典のパスと SHA を書いたコメントがある。例：`recency_list_spec.rs` の冒頭は `Ported from Pekko v1.7.1:` と原典パス、v2.0.0 の SHA `9fbd2283b1cf701c90a1223bf621942c191db61f` との照合結果を持つ。
- 原典の Pekko 側のファイルは、ASF のライセンスヘッダーと、ファイルごとの Lightbend の著作権表示を持つ。例：`actor-tests/.../RecencyListSpec.scala` は `Copyright (C) 2016-2022 Lightbend Inc.`。
- 移植した Rust ファイルに、この著作権表示とライセンスへの参照は無い。SPDX の識別子も無い（`grep -rl 'SPDX-License-Identifier'` は 0 件）。

### 1.4 Apache License 2.0 第4条が求めるもの

原典の `pekko/LICENSE`（Apache License 2.0 の原文、478 行）の第4条は、Work または Derivative Works を配布するとき、次を求める。

- **(a)** 受け取る者にライセンスの写しを渡す。
- **(b)** 変更したファイルには、変更したことを示す目立つ表示を付ける。
- **(c)** 配布する Source form に、原典の著作権・特許・商標・帰属の表示を保持する（自分の Derivative Works に関係しないものを除く）。
- **(d)** 原典に `NOTICE` があれば、関係する帰属表示を読める形で同梱する（`NOTICE` の中、ソースや文書の中、または通常サードパーティ表示が出る画面の中）。

このリポジトリは移植（変更を伴う Derivative Work）を公開リポジトリで配布しているので、第4条の対象になる。

### 1.5 `publish = false` の意味

```
$ grep -rn '^publish' crates/*/Cargo.toml
crates/fraktor-testkit/Cargo.toml:5:publish = false
crates/fraktor-actor/Cargo.toml:5:publish = false
```

`publish = false` は Cargo が crates.io へ公開するのを止める設定である。両クレートは `version = "0.0.0"` で、crates.io には出していない。

ただし、**配布が無いことにはならない**。リポジトリは GitHub で公開されており、誰でも clone してソースを受け取れる。Apache License 2.0 の第4条は「Source or Object form での配布」を対象にするので、`publish = false` は第4条の義務を外さない。この点は方針の前提になる。

## 2. 選択肢

作業量は相対見積もり（1・2・3・5・8）。基準は「[PR #178](https://github.com/j5ik2o/fraktor-ng-rs/pull/178)（DoubleLinkedList の設計・移植・Quint・レビュー対応）＝ 3」。

### (a) `LICENSE` / `NOTICE` を置き、マーカーのある移植ファイルに短いヘッダーを付ける（推奨）

- **利点**：第4条の (a)〜(d) をひととおり満たす。ファイル単位で SPDX と出典・変更表示が見える。`--check` を CI に入れれば、今後の移植で付け忘れを機械的に防げる。ヘッダーはスクリプトで冪等に付き、`--remove` で戻せる。
- **欠点**：51 ファイルの冒頭が変わり、行番号がずれる。既存の出典コメントと内容が一部重複する。実装 76 ファイルは統一マーカーが無いため、このままではヘッダーが付かない（別途、マーカーを足すか対象を広げる判断が要る）。
- **作業量**：2（LICENSE/NOTICE とスクリプトはこの PR で用意済み。決定後の適用は 1 コマンド）。
- **戻し方**：`python3 tools/license-headers/apply_ported_headers.py --remove` でヘッダーを消す。`NOTICE` は削除、`LICENSE` は残すか削除するかを別に決める。

### (b) `LICENSE` / `NOTICE` だけを置き、出典は既存コメントに任せる

- **利点**：既存ファイルを触らない。第4条 (a)(d) は満たし、(c) も `NOTICE` と既存の出典コメントで説明できる。差分が小さい。
- **欠点**：(b) の「変更したファイルへの目立つ表示」が弱い。既存の `Ported from Pekko` は出典であって変更表示ではない、と読める余地がある。SPDX が無く、ファイル単体を持ち出したときにライセンスが分からない。
- **作業量**：1。
- **戻し方**：`NOTICE` を削除し、`LICENSE` を残すか削除する。

### (c) 現状維持し、方針だけを文書化する

- **利点**：何も変えない。ライセンスの選択を急がない。
- **欠点**：公開リポジトリの配布に対して第4条 (a)(d) を満たさないままになる。GitHub のライセンス表示も `null` のまま。Devin の指摘も未解決のまま残る。
- **作業量**：1（この文書だけ）。
- **戻し方**：文書を削除する。

### 比較と推奨

| | (a) ヘッダーまで | (b) LICENSE/NOTICE のみ | (c) 現状維持 |
|:---|:---:|:---:|:---:|
| 第4条 (a) 写し | ○ | ○ | × |
| 第4条 (b) 変更表示 | ○ | △ | × |
| 第4条 (c) 著作権表示の保持 | ○ | △ | × |
| 第4条 (d) NOTICE | ○ | ○ | × |
| 既存ファイルへの変更 | 51 件 | 0 件 | 0 件 |
| 作業量 | 2 | 1 | 1 |
| 戻しやすさ | `--remove` で可 | ファイル削除 | 文書削除 |

**推奨は (a)。** 公開リポジトリで配布している以上、(a)(d) は満たす必要がある。(b) との差は 51 ファイルのヘッダーだけで、スクリプトにより冪等・可逆に適用できる。判断を保留する理由が (c) に無い。

(a) を採る場合の適用範囲は、次の二段階を推奨する。

1. いま統一マーカーのある 51 テストファイルに適用する（この PR のスクリプトで 1 コマンド）。
2. 実装 76 ファイル（`Mirrors \`org.apache.pekko...\`` を持つもの）へ広げる作業を別 issue にする。実装ファイルは「Pekko の型に対応する」と書いていても、どこまでが移植でどこからが Rust 向けの再設計かをファイルごとに確かめる必要がある。マーカーの規約を決めてから、同じスクリプトの対象を広げる。

## 3. この PR で用意したもの（可逆・未適用）

### 3.1 `LICENSE`

原典の `pekko/LICENSE` をそのまま複製した。Apache License 2.0 の原文で、別ライセンスは選んでいない。

```
$ cp pekko/LICENSE LICENSE
$ shasum -a 256 LICENSE pekko/LICENSE
2dca66a35023ac4f692030f7848d41098f622b18ab2f58a15cd48a6de272f30c  LICENSE
2dca66a35023ac4f692030f7848d41098f622b18ab2f58a15cd48a6de272f30c  pekko/LICENSE
```

### 3.2 `NOTICE`

原典 `pekko/NOTICE`（v2.0.0、git `9fbd2283b1`）のうち、このリポジトリが移植しているコードに関係する帰属表示を再掲した。

- 再掲したもの：Apache Pekko の著作権表示、ASF への帰属、Akka/Lightbend 由来の表示。
- 省いたもの：MurmurHash、scala-library、Netty、java-uuid-generator、CountMinSketch の各節。`crates/` に対応するコードが無いことを `grep` で確かめた（Murmur と Netty は台帳の unported ノードにだけ現れる）。省いた理由と、移植したら足すことを `NOTICE` 本文に書いた。

**代替案**：第4条 (d) の「関係しないものを除く」を厳密に読まず、原典の `NOTICE` 全文を再掲する方法もある。帰属表示を多めに載せても違反にはならないと解されることが多く、安全側に倒すならこちら。判断は元帥に委ねる（差分は `NOTICE` 1 ファイル）。

### 3.3 ヘッダー一括スクリプト

`tools/license-headers/apply_ported_headers.py`

- 対象は `crates/**/*.rs` のうち、冒頭 20 行に `Ported from Pekko` / `Port of Pekko` / `Checked against Pekko` を持つファイルだけ。**マーカーの無いファイルには触らない。**
- 付けるヘッダー（Rust のコメント）：

```rust
// SPDX-License-Identifier: Apache-2.0
//
// Derived from Apache Pekko, which was derived from Akka.
// Copyright (C) 2009-2022 Lightbend Inc. <https://www.lightbend.com>
// Copyright 2022-2026 The Apache Software Foundation
//
// Licensed under the Apache License, Version 2.0. See LICENSE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.
```

- 冪等：2 回 `--apply` しても内容は変わらない。`--remove` は付けたヘッダーだけを消して元に戻す。`--check` は未適用があれば終了コード 1（CI 用）。
- dry-run（既定）の実測：

```
$ python3 tools/license-headers/apply_ported_headers.py
ported: 51  headed: 0  to add: 51
pending crates/fraktor-actor/tests/...
```

- テスト：`tools/license-headers/apply_ported_headers_test.py`（4 件、すべて成功）。冪等性、対象の限定、`--remove` での復元、`--check` の終了コードを一時ディレクトリで確かめる。

**適用はまだしていない。** 方針決定後に次を実行する。

```
python3 tools/license-headers/apply_ported_headers.py --apply
python3 tools/license-headers/apply_ported_headers.py --check   # 0 を確認
```

### 3.4 変えていないこと

- 既存の移植ファイルは 1 つも書き換えていない。
- クレートの `Cargo.toml`（`publish = false`、`version = "0.0.0"`）は変えていない。
- CI のワークフローは変えていない（`--check` の組み込みは未実施）。
- 原典 `pekko/` は読み取り専用として扱い、変更していない。

## 4. 残る判断（元帥）

1. **方針**：(a) ヘッダーまで / (b) `LICENSE`・`NOTICE` のみ / (c) 現状維持。推奨は (a)。
2. **`NOTICE` の範囲**：関係する節だけ（現在の実装）か、原典の全文か。
3. **適用の時期と範囲**：いま 51 テストファイルに適用するか。実装 76 ファイルへ広げる別 issue を立てるか。
4. **CI**：`--check` を既存の CI に足すか（足すと、新しい移植ファイルにヘッダーが無い PR が落ちる）。
5. **`LICENSE` の意味**：ルートに Apache-2.0 を置くと、移植部分だけでなくリポジトリ全体が Apache-2.0 であると表示される。`package.json` は既に `Apache-2.0` と言っているが、この機会にリポジトリ全体のライセンスとして確定してよいか。

## 5. 検証

この PR で流した検査は PR 本文に記録する。対象は次のとおり（CI と同じ）。

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `ledger/tools` と `tools/` の Python テスト（CI の `python tests` と同じ一覧）
- `npm run check:aislop`
- `tools/license-headers/apply_ported_headers_test.py`

## 付録：実測コマンドの再掲

```bash
# ライセンス表示
git ls-files | grep -iE '(^|/)(licen[cs]e|notice|copying)'
node -p "require('./package.json').license"
gh repo view j5ik2o/fraktor-ng-rs --json visibility,licenseInfo

# 移植マーカー
grep -rlE 'Ported from Pekko|Port of Pekko|Checked against Pekko' --include='*.rs' crates | wc -l
grep -rhoE 'Ported from Pekko|Port of Pekko|Checked against Pekko' --include='*.rs' crates | sort | uniq -c

# 実装ファイルの対応（マーカー無し）
grep -rlE 'Mirrors `org\.apache\.pekko' --include='*.rs' crates/*/src | grep -v '_test\.rs$' | wc -l

# 台帳
uv run --with pyyaml python3 - <<'PY'
import yaml
from collections import Counter
d = yaml.safe_load(open('ledger/nodes.yaml'))
print('nodes', len(d['nodes']))
print(Counter(n['status'] for n in d['nodes']))
print('with test_file', sum(1 for n in d['nodes'] if n.get('rust', {}).get('test_file')))
print('retired', len(d.get('retired', [])))
PY

# Apache License 2.0 第4条
grep -n '4\. Redistribution' -A 30 pekko/LICENSE
```
