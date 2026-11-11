{extends:loop-monitor-reviewers-fix}

## このプロジェクトでの追加（fraktor-ng-rs）

標準の loop-monitor-reviewers-fix への差分。このループは実装（develop）と
機械ゲート（verify_green）の差し戻しである。レビュー報告は無い。

- 選べる結果は「健全」（develop に戻す）と「非生産的」（run を ABORT して
  人間に上げる）の 2 つだけである。再計画や裁定に回す経路は無い
- 標準の指示でいう「収束中」は健全に、「停滞または発散」は非生産的に
  当たる。停滞または発散と判断したら、develop に戻さず非生産的を選ぶ
- 判断の材料は、下の機械ゲートの結果、渡された develop の報告とその履歴、
  現在のコードである。同じゲートが同じ理由で落ち続けていれば停滞である
- 非生産的と判定した場合は、人間が判断するための要約（落ち続けている
  ゲートと理由、試したこと、考えられる選択肢）を報告に含める

直近の機械ゲートの結果（verify_green の出力）:

{context:verify_green.result.stdout}
