# ゴール G0b：TestKit の種と actor 最小核

状態: 定義済み・未発行（2026-11-12）。根拠は承認済みグラフモデル提案
（`docs/decisions/fable-graph-model-proposal.md`）の 3 章と 4 章、論点 12・14・19。
判断 A〜E は 2026-11-12 に窓口（オーナー）が推奨案どおり承認した（下記「承認済みの判断」）。

kind: foundation（基盤ゴール。同時に 1 本だけ走らせる）
requires: [G0a]（完了済み。PR #10、3d6ae5b。`rollup.py --goal G0a` は exit 0）

## 対象ノード（明示列挙、4 件）

| 処理順 | id | kind | ケース数（近似） | needs.testkit |
|---|---|---|---|---|
| 1 | `testkit/src/test/scala/org/apache/pekko/testkit/DefaultTimeoutSpec.scala` | test | 1 | test-kit, default-timeout |
| 2 | `testkit/src/test/scala/org/apache/pekko/testkit/ImplicitSenderSpec.scala` | test | 1 | test-kit, test-actor, implicit-sender |
| 3 | `testkit/src/test/scala/org/apache/pekko/testkit/TestActorsSpec.scala` | test | 2 | extends-pekko-spec, pekko-spec, test-actor, implicit-sender, expect-msg |
| (3 と同時) | `testkit/src/test/scala/org/apache/pekko/testkit/PekkoSpec.scala` | fixture（発行前処理で人間が変更） | 0 | pekko-spec, test-kit |

ノードの増減は人間だけが行う。

## 依存の閉じ方

- 4 ノードの needs はすべて `capabilities.yaml` で `provided_by: G0b` の能力である。
  `check_needs.py --goal G0b` は、タグ付けした台帳の一時コピーで exit 0（needs 閉性 OK）を確認済み。
- 層 2 以上の能力（EventFilter、TestProbe、expectNoMessage、within、awaitCond など）は使わない。
- 作らせるもの（actor 最小核と TestKit の種）:
  - ActorSystem の生成と停止（terminate）
  - Props、actorOf、ActorRef への送信（tell）と sender
  - TestKit／TestKitBase、testActor、ImplicitSender、DefaultTimeout、expectMsg
  - TestKitSettings、TestActors（echoActorProps、forwardActorProps）
- 実行基盤は論点 14 の暫定方針に従う。executor は自前で作り、シャード内はローカルキュー、
  境界だけチャネルとする。境界の実装は trait で差し替えられる形にし、実装は 1 つでよい。
  expectMsg の待ち方（テストスレッドが境界チャネルをタイムアウトつきで受信する等）の詳細は、
  run の設計段階で決めて報告する。最終決定は縦切りの実測後に行うので、G0b では決めない。

## 承認済みの判断（2026-11-12）

- **A. fixture の扱い**: PekkoSpec の `kind` を `fixture` に変えるのは、発行前処理で人間が行う。
  run は PekkoSpec を、TestActorsSpec の前提基盤（triage の bootstrap 条件）として作る。
  TestActorsSpec が green になったら、同じ台帳更新で PekkoSpec も green として記録する
  （fixture は「移植済み」をもって green と同じに扱う）。PekkoSpec を単独のノードとして
  verify_red に流さない。
- **B. PekkoSpec の移植範囲**: G0b に要る部分だけを移植する。下の「PekkoSpec で移植しない部分」は
  後のゴールに回す。この範囲を移植したことをもって「移植済み」とみなす。
- **C. 置き場所**: PekkoSpec は `fraktor-testkit` の src に公開モジュールとして置く
  （`actor-tests` の 114 クラスが dev-dependency 越しに使うため）。
- **D. no_std**: 未決定のままとし、G0b は要求しない前提で進める。`fraktor-testkit` は std 専用でよい。
- **E. 設定**: TestKitSettings は Pekko の `testkit/src/main/resources/reference.conf` の既定値を
  定数で持つ（timefactor 1.0、single-expect-default 3s、expect-no-message-default 100ms、
  default-timeout 5s、filter-leeway 3s）。HOCON などの設定体系は作らない。
  設定体系の再現範囲は後のゴールで決める。

### PekkoSpec で移植する部分

- TestKit（TestKitBase）の継承に当たる構造と、ActorSystem の生成と停止
- beforeAll／afterAll の流れと、フック atStartup、beforeTermination、afterTermination
- testKitSettings の参照

### PekkoSpec で移植しない部分（後のゴールに回す）

- `WatchedByCoroner`、`startCoroner`／`stopCoroner`、`expectedTestDuration`
- `muteDeadLetters`（EventFilter／Mute に依存。層 2）
- `spawn`（dispatchers の lookup と Future に依存）
- `log`（LoggingAdapter）
- Config、String、Map を受けるコンストラクタ群と `PekkoSpec.testConf`、`mapToConfig`
- `resourcePath`、`normalizedPath`
- ScalaFutures の `patience`、TypeCheckedTripleEquals 用の `classEqualityConstraint`／`setEqualityConstraint`

## 置き場所

- 新設するクレートは `crates/fraktor-testkit` の 1 つ（案C 規則 1）。
- テスト 3 件は `crates/fraktor-testkit/tests/` に、1 ノード 1 ファイルで置く（案C 規則 3）。
- PekkoSpec は `crates/fraktor-testkit/src/` の公開モジュールとして置く。
- actor 最小核と executor は `crates/fraktor-actor` の中に置く（kernel／adaptor の段階1の形に従う）。

## write_scope

- `crates/fraktor-testkit/**`
- `crates/fraktor-actor/**`（既存の ActorPath と G0a のテストを壊さないこと）
- ワークスペース直下の `Cargo.toml`（members の追加）と `Cargo.lock`
- `ledger/nodes.yaml`（対象 4 ノードの欄のみ）

## 完了条件（機械判定できる形）

- AC1: `cargo test -p fraktor-testkit` が通る
- AC2: `cargo test -p fraktor-actor` が通る（G0a の退行がない）
- AC3: `uv run --with pyyaml python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal G0b` が 0 を返す
- AC4: `uv run --with pyyaml python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal G0a` が 0 を返す
- AC5: `uv run --with pyyaml python3 ledger/tools/check_needs.py --ledger ledger/nodes.yaml --capabilities ledger/capabilities.yaml --goal G0b` が 0 を返す
- AC6: 発行前処理のコミットより後の台帳の差分が、対象 4 ノードに限られる
- AC7: 4 ノードの `cases.total` が実数で記録されている（原典の見立ては 1 / 1 / 2 / 0）。
  `rust.crate` と `rust.test_file` も記録されている（PekkoSpec の test_file は fixture の src のパス）
- AC8: executor が暫定方針の形になっている。根拠は、境界の差し替え用 trait と実装 1 つがコードにあることと、run の報告
- AC9: ワークフロー内の機械検証と、等価性レビュー、`.takt/workflows/rules/pekko-coding-rules.md`
  （貧血モデル禁止を含む）の検査を通っている
- AC10: 成果はドラフト PR と 6 項目の完了報告で提出し、そこで止まる。G0c には着手しない

## やらないこと

- TestProbe、expectNoMessage、ask、watch、子アクターの再起動、AutoPilot（G0c の仕事）
- EventFilter、Coroner、ログ、HOCON の設定体系、typed
- `fraktor-testkit` 以外のクレートの新設、台帳スキーマの変更、`.takt/` と `ledger/tools/` の変更
- ノードを do_not_port にすること、ケースの除外、ゴール外ノードの状態変更
- 旧 fraktor-rs からのコードの持ち込み
- write_scope の外の変更が必要になったら、止めて報告する

## provider

基盤ゴールなのでクラウド（論点19）。実効並行は 1 本。codex の使用量上限（2026-11-17 回復見込み）中は
`.takt/runtime.yaml` の assignment `interim-claude` を使う。

## 発行時の台帳前処理（人間が行う）

1. PekkoSpec の `kind: test` を `kind: fixture` に変える（判断 A。人間だけが行える操作）
2. 対象 4 ノードの `goal: null` を `goal: G0b` に書き換える
3. 台帳の差分は、上の 5 行（kind 1 行と goal 4 行）だけにする

検証として、次のコマンドが「ノード 4 件、status 内訳 {'unported': 4}」で exit 1 になること
（発行前なので unported のみで正しい）。needs 閉性は exit 0 になること。

```sh
uv run --with pyyaml python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal G0b
uv run --with pyyaml python3 ledger/tools/check_needs.py --ledger ledger/nodes.yaml --capabilities ledger/capabilities.yaml --goal G0b
```

タグの除去や付け替え（中止時）は人間だけが決める。

## 前提と残るリスク

- run から Pekko v1.7.1 の原典を参照できること（`pekko/` または後方互換の `../pekko`）
- `pekko-porting-ng` の `max_steps: 80` で 4 ノード（実処理は 3 回の node 呼び出し）が収まるかは、
  未確認である。G0a の消費ステップ数を見て、足りなければ人間が判断する

## run の後に人間が確認するコマンド

```sh
cargo test -p fraktor-testkit
cargo test -p fraktor-actor
uv run --with pyyaml python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal G0b
uv run --with pyyaml python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal G0a
uv run --with pyyaml python3 ledger/tools/check_needs.py --ledger ledger/nodes.yaml --capabilities ledger/capabilities.yaml --goal G0b
git diff --stat ledger/nodes.yaml
```
