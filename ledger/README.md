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
  ノード状態は 未移植 / red / green / 移植しない / 作らない
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

## 原典の基準タグと照合済みタグ

移植の原典は `tools/pekko-source/pekko-baseline.env` の基準タグ（2026-11-25 から v2.0.0）である。
決定の記録は `docs/baseline/README.md`。各ノードの `verified_against` は、そのノードを
どのタグの原典と照合したかを持つ（規則は `docs/decisions/ledger-schema-draft.md`）。

```sh
# 照合済みタグの検査と集計（違反があれば終了コード 1）
uv run --with pyyaml python3 ledger/tools/pekko_baseline.py check-ledger --ledger ledger/nodes.yaml

# 現行タグで未照合のノードの一覧と、現行の原典にファイルが無いノードの件数
uv run --with pyyaml python3 ledger/tools/pekko_baseline.py check-ledger \
  --ledger ledger/nodes.yaml --pekko pekko --list-unverified

# 再照合の候補（現行タグで未照合の判定済みノード）の id を出す。これは候補であって、
# 照合済みの一覧ではない
uv run python3 ledger/tools/reverify_against.py plan --ledger ledger/nodes.yaml

# 実際に原典と突き合わせて理由が成り立つと確かめたノードの id だけを書いたファイルを渡す
# （plan の出力をそのまま流さない。確かめていないノードを現行タグにしない）
uv run python3 ledger/tools/reverify_against.py apply \
  --ledger ledger/nodes.yaml --ids /tmp/checked-ids.txt --dry-run
```

`apply` と `plan` の `--tag` を省くと、基準ファイルの現行タグになる。過去のタグを渡すと拒否する
（照合済みのノードが「現行タグで未照合」に戻り、台帳の検査は過去のタグを有効とするため
気づけない）。

再照合の結果は `docs/baseline/v2.0.0-reverification.md` が持つ（1 ノード 1 行の根拠）。
再照合の判定日と根拠を台帳に持つ欄は規定されていないので、台帳には `verified_against` の値だけを書く。

## 原典でパスが変わったノードと、原典で削除されたノード

基準の切り替えで原典のパスが変わったノードは、`id` を現行の原典のパスへ付け替える（同じノードと
して扱い、`status`・件数・`cases`・`goal` は持ち越す）。原典からスイートごと削除されたノードは
`status: do_not_port`、`triage.kind: do_not_port`、`verified_against: <現行タグ>` にし、
`triage.reason` に次を書く。

> 原典 v2.0.0 で削除（削除コミット <SHA>・確認日 <YYYY-MM-DD>）

`retired` は使わない。後継が無いので `retired.successor` と、後継の `cases.carried` が要求する
`rust_test`・`evidence` を埋められず、埋めれば記録の捏造になる（語義からも外れる）。この遷移は
人間の決定（原典で削除という事実の確認）に基づく。run が自分でノードを `do_not_port` へ倒すことは
ない。

置換は手で書かず、1 行 1 件の根拠表（`docs/baseline/v2.0.0-port-targets.tsv`。旧 id・新 id・理由・
削除コミット・確認日）を読む `ledger/tools/relocate_nodes.py` で行う。台帳の行の書き換えは
`ledger_lines.py` を通る。`apply` は PyYAML なしで動き、冪等である。`check` は根拠表と台帳の
整合と、旧 id の文字列参照 0 件（`ledger`・`docs`・`.takt`・`scripts` を探す。根拠表と凍結した
過去の基準は履歴として除く）を見る。CI でも `check` を実行する。

```sh
python3 ledger/tools/relocate_nodes.py apply --dry-run   # 何が変わるか（書かない）
python3 ledger/tools/relocate_nodes.py apply             # 適用（冪等）
uv run --with pyyaml python3 ledger/tools/relocate_nodes.py check
```

記録は `docs/baseline/v2.0.0-missing-unported.md` にある。

## needs の抽出と閉性判定

各ノードの `needs.testkit` は、Pekko 原典ファイルが使う TestKit の能力 id のリストである。
能力語彙と提供元の対応は `capabilities.yaml` にある（語彙の正は
`docs/baseline/pekko-v1.7.1/supplement-recount.md` §2）。

- `null` は未抽出、`[]` はその能力を使っていない、配列は一致した能力 id（一致順）
- `needs.premise_flags` は前提フラグ（Java 直列化、`Thread.sleep` など）の一致リストで、
  閉性判定には使わない
- 抽出は存在判定（正規表現が 1 回以上一致するか）であり、継承やヘルパ経由の利用は見えない
- 台帳の既存の値は 2026-11-25 の基準切り替えより前の原典から抽出した。`--pekko pekko` は現行の基準（v2.0.0）の原典を
  指すので、再抽出すると値が変わりうる。再抽出は台帳の差分として別に確かめる

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

## 作らない区分

`status: not_built` と `cases.not_built` は「作らない区分」の記録である（2026-11-25 オーナー決定）。
原典のテストが原典の言語のための補助そのものを検証していて、観測できる振る舞いを Rust の言語機能・
標準ライブラリ・既存の API で満たせるため、対応する公開面を作らない。判定はケース単位で、1件ずつ
理由（`reason`）と Rust の代替例（`alternative`）と承認（`approved_by`、`approved_at`）を書く。
除外（`cases.excluded`）や移植しない（`do_not_port`）とは別に数える。

- 定義、当てはめる条件、自動では入れないもの：`docs/decisions/ledger-schema-draft.md` の「作らない区分」
- 集計：`rollup.py` が `not_built` のノード数、作らない率、ケースの 移植・除外・作らない の件数を出す。
  合格率の分母からは除く。記録の欠けた `not_built` はゴールの完了に数えない
- 最初の適用：`docs/goals/issue-145-partial-function-builders.md`（PFBuilderTest、MatchBuilderTest）

## 未決定

- 台帳スキーマの欄の名前と並び（叩き台のまま。粒度、ID、種別、ケース単位の除外は承認済み）
- 状態遷移を誰（どのステップ）が書き込むかのワークフロー設計
