# ledger — リポジトリ台帳（進捗の正）

ここが fraktor-ng-rs の進捗の正になる場所である。台帳の本体は `nodes.yaml` である。
2026-11-11 に、baseline から全 1,262 ノードを変換器で生成した。

スキーマの叩き台は `docs/decisions/ledger-schema-draft.md`。
変換器は `tools/convert_baseline.py`、変換サンプルは `sample-actor-tests.yaml`
（actor-tests の先頭5ノード）。`tools/recount_supplement.py` は、グラフモデル提案書が
引用する数字を再計算する。

## 位置づけ

- **正はここ**。グラフ（DOT 等）はここから生成するビューであり、直接編集しない
- 種は `docs/baseline/pekko-v1.7.1/ledger.yaml`（モジュール粒度の事実層スナップショット）。
  あちらは凍結物であり、進捗を書き込む場所ではない。ここへ変換して取り込む
- 粒度はテストクラスファイル（2026-11-11 承認）。ノードの一意キーはファイルパスである。
  ノード状態は 未移植 / red / green / 移植しない
- 「移植しない」の変更は人間の判断でのみ行う。TAKT の manager / run が勝手に
  ノードを「移植しない」へ倒すことは許さない（理由の記録欄は台帳側に持つ）

## TAKT との接続（予定）

- manager のゴールは、台帳のノード部分集合として定義できる形にする
- ゴールの完了条件は、台帳の状態遷移（対象ノードが green になること）で判定できる形にする
- 台帳の更新は takt run の成果として行い、manager が取り込む。人間が手で進捗を
  書き換える運用にはしない（来歴が run と対応しなくなるため）

## ゴール発行時の手順（タグ付け）

- ゴールを発行する前に、対象ノードの `goal:` にゴール ID を書き込む（タグ付け）。
  台帳の差分は対象ノードの `goal:` 行のみに限定する
- タグ付け後に `rollup.py --goal <ID>` で対象ノード数と status 内訳を確認する
- タグの除去・付け替えは人間のみが決める

## needs の抽出と閉性判定

各ノードの `needs.testkit` は、Pekko 原典ファイルが使う TestKit の能力 id のリストである。
能力語彙と提供元の対応は `capabilities.yaml` にある（語彙の正は
`docs/baseline/pekko-v1.7.1/supplement-recount.md` §2）。

- `null` は未抽出、`[]` はその能力を使っていない、配列は一致した能力 id（一致順）
- `needs.premise_flags` は前提フラグ（Java 直列化、`Thread.sleep` など）の一致リストで、
  閉性判定には使わない
- 抽出は存在判定（正規表現が 1 回以上一致するか）であり、継承やヘルパ経由の利用は見えない

```sh
# 抽出（nodes.yaml の needs 配下の 2 行だけを書き換える。再実行は冪等）
uv run --with pyyaml python3 ledger/tools/extract_needs.py \
  --ledger ledger/nodes.yaml --capabilities ledger/capabilities.yaml --pekko pekko

# ゴールの needs 閉性判定（provided_by が自身か green 済みゴールなら充足）
uv run --with pyyaml python3 ledger/tools/check_needs.py \
  --ledger ledger/nodes.yaml --capabilities ledger/capabilities.yaml --goal G0b
```

## ケース除外の承認記録

`cases.excluded` は人間が承認した除外の一覧である。各要素は `name`・`reason` に加えて、
人による承認を記録する `approved_by`（承認した人間。空白以外の文字列）と
`approved_at`（承認日。`YYYY-MM-DD` の暦日）を持つ。

- 承認を記録するのは人間だけである。run は承認者を推測・捏造・代行しない
- `approved_by` と `approved_at` のどちらかが欠けた要素、空欄・null の要素は未承認として扱う。
  `approved_at` は暦日だけを受理し、時刻を含む timestamp は受理しない
- 承認記録のない既存の除外（`name`・`reason` だけの要素）は fail closed になる。記録を足すまで
  有効な除外にならず、`verify_red.py` の `count_excluded` は件数を返さない（`already_green` にしない）
- `count_excluded` は対象ノードの `cases.excluded` だけを検査する。同じノードの `cases` の外や、
  除外要素の下位（`metadata` など）にある承認欄は流用しない
- YAML の alias（`*anchor`）で他ノードの承認済みマッピングを対象の除外要素へ持ち込む記法は
  専用ローダが読み込み前に拒否し、`count_excluded` は件数を返さない（`already_green` にしない）。
  承認は対象ノード・除外要素の直下に明示値で書く
- `count_excluded` は PyYAML を呼ぶときだけ遅延 import する。`record_already_green.py` は
  `uv run python3`（`--with pyyaml` なし）で起動し、`verify_red` を import する。除外の検査を
  行わない限り PyYAML なしで動く（verify_red の呼び出し側は `--with pyyaml` 付き）
- スキーマの詳細は `docs/decisions/ledger-schema-draft.md` の `cases` を参照する

## 未決定

- 台帳スキーマの欄の名前と並び（叩き台のまま。粒度、ID、種別、ケース単位の除外は承認済み）
- 状態遷移を誰（どのステップ）が書き込むかのワークフロー設計
