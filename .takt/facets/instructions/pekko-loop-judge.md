{extends:loop-monitor-reviewers-fix}

## このプロジェクトでの追加（fraktor-ng-rs）

標準の loop-monitor-reviewers-fix への差分。このループは等価性レビューと
実装（develop）の差し戻しであり、非生産的と判定すると run は ABORT して
人間に上がる。

- 非生産的と判定した場合は、人間が判断するための要約（詰まっている論点、
  試したこと、考えられる選択肢）を報告に含める
