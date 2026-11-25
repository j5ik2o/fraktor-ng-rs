# docs/baseline — 移植の原典にする Pekko の基準

## 現行の基準

| 項目 | 値 |
|:---|:---|
| 正のタグ | Apache Pekko **v2.0.0** |
| タグが指す commit | `9fbd2283b1cf701c90a1223bf621942c191db61f` |
| タグ object | `879512455865e4a81433c8c96fd11c2b95c5bd7d`（注釈付きタグ） |
| 決定日 | 2026-11-25 |
| 決定 | オーナー決定（元帥経由）。新しい移植は Apache Pekko v2.0.0 を正とする |
| 機械が読む正 | `tools/pekko-source/pekko-baseline.env` |

タグと commit は、2026-11-25 に `git ls-remote --tags https://github.com/apache/pekko` で
確かめた。公式タグのうち最新の安定版は v2.0.0 である（v2.0.0 より新しいタグは
`v2.1.0-M0` だけで、マイルストーンなので除く）。

原典は `tools/pekko-source/prepare_pekko_source.sh` が作業ツリー直下の `pekko/` に用意する。
基準ファイルのタグで clone し、HEAD が基準 commit と一致しなければ失敗する。過去の基準の
`pekko/` が残った作業ツリーや、基準 commit の上に変更（追跡ファイルの変更、無視されない
未追跡ファイル、assume-unchanged / skip-worktree の印）がある `pekko/` では、消さずに失敗する
（旧原典や手を加えた原典のまま run を再開させない。利用者の変更は自動で消さない）。
git の検査自体が失敗した（壊れた index など）ときも、変更なしとは見なさず、消さずに失敗する。

原典を読むステップ（triage、port-test、verify_red、record-already-green、ledger-update）は、
同じ場所（`PEKKO_ORIGINAL_ROOT`、無ければ `pekko/`）を同じ検査（`pekko_baseline.source_tag`）に
通してから使う。verify_red は判定結果に出所（タグ、commit、原典の場所）を書き、record-already-green は
出所が欠けた・不正な・今の原典と一致しない結果を拒否して verify_red のやり直しを求める。
record-already-green が拒否したら、ワークフローは ledger-update へ進まず止まる。

## 過去の基準（履歴）

| タグ | commit | 期間 | 記録 |
|:---|:---|:---|:---|
| v1.7.1 | `9a54d1294c3ac7821bbed3c940769c3d5027d1ff` | 2026-11-11〜2026-11-25 | `pekko-v1.7.1/`（凍結スナップショット） |

- `pekko-v1.7.1/` は凍結物で、書き換えない。台帳 `ledger/nodes.yaml` の種と、TestKit 能力の
  語彙（`supplement-recount.md` §2）の出典である
- v1.7.1 の期間に green や do_not_port になったノードは、台帳の `verified_against: v1.7.1` で
  見分けられる。移植済みの Rust テストの冒頭にある出典（`Pekko v1.7.1: ...`）、`docs/goals/` の
  ゴール文書、過去の PR の検証結果も、v1.7.1 での照合の記録として残す。v2.0.0 との照合を
  済ませていないものを v2.0.0 と書き換えない
- 各ノードの v2.0.0 との照合と、その結果による `verified_against` の更新は、この切り替えとは
  別の作業で行う

## 現役の経路と履歴の区別

現役の経路は、`.takt/`、`.github/workflows/`、`tools/pekko-source/`、`ledger/tools/`、
`ledger/README.md`、`ledger/capabilities.yaml`、`.gitignore` である。ここに過去の基準の
タグと commit が残っていないことを、次のコマンドで確かめる（CI でも実行する）。

```sh
python3 ledger/tools/pekko_baseline.py check-refs
```

凍結スナップショットのディレクトリ（`docs/baseline/pekko-v1.7.1/`）を指すパスは、履歴への
参照なので許す。凍結した v1.7.1 の baseline を読むための道具（`ledger/tools/convert_baseline.py`、
`ledger/tools/recount_supplement.py`）と基準ファイル自身も対象外である。`docs/goals/`、
`docs/decisions/`、`crates/` の過去の記録は履歴として扱い、検査しない。走査するのは git が扱う
ファイル（追跡済みと、無視されない未追跡）だけで、`.takt/runs/` など git が無視する過去の run の
記録は読まない。

## 照合の状態を見る

```sh
# 現行の原典が基準タグか（基準タグを 1 行で出す。違えば終了コード 1）
python3 ledger/tools/pekko_baseline.py source-tag

# 台帳の照合済みタグの検査と集計。現行タグで未照合のノードの一覧と、
# 現行の原典にファイルが無いノードの件数も出す
uv run --with pyyaml python3 ledger/tools/pekko_baseline.py check-ledger \
  --ledger ledger/nodes.yaml --pekko pekko --list-unverified
```

未照合のノードをどう減らすかは、2 つの記録が持つ。

- `v2.0.0-reverification.md`：判定済み 175 ノードの v2.0.0 再照合（1 ノード 1 行の根拠。台帳の
  `verified_against` を v2.0.0 に揃えた）
- `v2.0.0-missing-unported.md`：v2.0.0 の原典にファイルが無い unported 23 ノード（移動 9・
  原典で削除 14。台帳に記録する形式が規定されていないため、台帳は変えていない）

## 基準を変えるとき

1. `tools/pekko-source/pekko-baseline.env` の現行タグ・commit・タグ object・決定日を書き換え、
   それまでの現行タグを `PEKKO_HISTORICAL_BASELINES` に足す（`<タグ>:<commit>` をカンマで
   区切る。空白で区切ると、シェルが 2 項目目をコマンドとして実行する）
2. この README の表に決定を記録する
3. `pekko_baseline.py check-refs` と `check-ledger` が通ることを確かめる
