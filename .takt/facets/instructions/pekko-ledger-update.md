対象ノードの green を台帳に記録し、ゴールの完了判定を行って
ください。このステップで書いてよいのは台帳の対象ノードの欄だけです。

## 手順

1. `ledger/nodes.yaml` の対象ノードに記録する
   - `status`: `green`
   - `cases`: triage で数えた実数（`total`）、移植した数（`ported`）、
     通過した数（`passing`）。承認済みの除外がある場合のみ `excluded`
   - `rust`: `crate` と `test_file`（port-test が記録済みのものを確認し、
     実際のパスとずれていれば直す）
   - `goal`: 処理中のゴール ID
2. 完了判定を実行する
   `python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal <ゴールID>`
3. 判定結果を報告する
   - 終了コード 0 → ゴール完了
   - 未完 → 残っているノードの id を報告する（次は triage に戻る）

## 禁止

- 対象ノード以外の欄・ノードを変更すること
- 検証を省略して status を変えること（green は verify-green と
  equivalence-review を通過したノードだけ）
