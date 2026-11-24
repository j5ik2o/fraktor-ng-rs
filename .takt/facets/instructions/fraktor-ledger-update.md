対象ノードの green を台帳に記録し、ゴールの完了判定を行って
ください。このステップで書いてよいのは台帳の対象ノードの欄だけです
（`goal` 欄を除く。`goal` は発行時に人間が記録した値を確かめるだけで、書かない）。

## 手順

1. `ledger/nodes.yaml` の対象ノードに記録する
   - `status`: `green`
   - `cases`: triage で数えた実数（`total`）、移植した数（`ported`）、
     通過した数（`passing`）。承認済みの除外がある場合のみ `excluded`
   - `rust`: `crate` と `test_file`（port-test が記録済みのものを確認し、
     実際のパスとずれていれば直す）
   - `verified_against`: 照合に使った原典の基準タグ。
     `python3 ledger/tools/pekko_baseline.py source-tag` が出す値を書く（原典の場所は
     verify_red と同じ規則で決まる。`--pekko` で別の場所を指定しない）
     （ノードの最上位、`goal` の次の行。record-already-green が書いた値があれば確かめる）。
     終了コードが 0 でなければ、原典が変更のない現行の基準ではないので green を記録せず、
     「台帳の更新や判定を実行できない」として報告して止まる
   - `goal`: 処理中のゴール ID が発行時に既に書かれていることを確かめる。書かない
     （この欄は人間が発行時に記録する。書かれていない、または処理中のゴール ID と
     違う場合は、記録せずに報告して止まる）
2. 台帳の照合済みタグを検査する
   `uv run --with pyyaml python3 ledger/tools/pekko_baseline.py check-ledger --ledger ledger/nodes.yaml`
   （終了コード 0 以外は、記録を直すまで次へ進まない）
3. 完了判定を実行する
   `python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal <ゴールID>`
4. 判定結果を報告する
   - 終了コード 0 → ゴール完了
   - 未完 → 残っているノードの id を報告する（次は triage に戻る）
   - 完了の最終判定は親ワークフローの関門（`rollup.py --goal` の終了コード）が行う。
     ここでの実行は報告用で、合否の正は関門の結果。

## 禁止

- `goal` 欄を書くこと（発行時に人間が記録した値を確かめるだけで、変更しない）
- 対象ノード以外の欄・ノードを変更すること
- 過去に green になったノードの `verified_against` を書き換えること（過去の照合の記録）
- 検証を省略して status を変えること（green は verify-green と
  equivalence-review を通過したノードだけ）
