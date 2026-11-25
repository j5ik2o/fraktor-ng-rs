```markdown
# 台帳更新レポート

## 更新したノード
{id} — status: green、cases: total {n} / ported {n} / passing {n}、
rust: {crate} / {test_file}、verified_against: {基準タグ}

## 照合済みタグの検査
- コマンド: uv run --with pyyaml python3 ledger/tools/pekko_baseline.py check-ledger --ledger ledger/nodes.yaml
- 終了コード: {0 ／ 1}

## ゴール完了判定
- コマンド: python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal {ゴールID}
- 終了コード: {0 ／ 1}
- 残ノード: {完了の場合は「なし」、未完の場合は id の一覧}
```
