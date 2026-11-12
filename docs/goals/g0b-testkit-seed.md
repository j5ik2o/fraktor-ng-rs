# ゴール G0b：TestKit の種と actor 最小核

状態: 定義済み・未発行（2026-11-12）。根拠は承認済みグラフモデル提案の 4 章
（最初の縦切りの第 2 ゴール）。目的は、ActorSystem の起動と停止、Props、
actorOf、送信と sender、testActor、expectMsg という actor 最小核と TestKit の
種を、一体で green にすることである。

## 対象ノード（3 件、台帳ケース 4）

| id | ケース数（近似） |
|---|---|
| `testkit/src/test/scala/org/apache/pekko/testkit/ImplicitSenderSpec.scala` | 1 |
| `testkit/src/test/scala/org/apache/pekko/testkit/DefaultTimeoutSpec.scala` | 1 |
| `testkit/src/test/scala/org/apache/pekko/testkit/TestActorsSpec.scala` | 2 |

選定理由：提案 4 章のとおり、TestKit 基盤と actor 最小核はどちらも単独では
green にできないため、同じゴールで一体として作る。3 ノードの needs は実測で
G0b 提供能力（pekko-spec、test-kit、test-actor、implicit-sender、
default-timeout、expect-msg、extends-pekko-spec）に閉じている
（`ledger/capabilities.yaml`、抽出は PR #16）。前提フラグは 3 ノードとも空である。

## fixture（PekkoSpec）の扱い

提案 4 章は G0b を 4 ノード（fixture の `PekkoSpec.scala` を含む）と数えるが、
台帳の goal タグはテスト 3 ノードに限定する。理由: 完了判定
（`rollup.py --goal`）は goal タグ付き全ノードの green を要求し、ワークフローは
test ノード前提（verify_red が cargo test の失敗を要求）で fixture の経路が
無いため、fixture にタグを付けると run が完了できない。PekkoSpec 相当の
TestKit 基底は本ゴールの仕事 2 として develop が実装する。台帳側の
`kind: fixture` への変更と green 記録は、完了報告時に人間が行う
（kind 変更は人間の承認事項）。

## 前提（発行時点で揃っていること）

- G0a 完了（`crates/fraktor-actor` が main に存在し、cargo test が通る）
- 実行基盤の暫定方針: 案 C＋port & adapter 段階 1 のとおり、executor は自前の
  最小実装、プラットフォーム境界（Clock / Scheduler）は port 越し
- needs 抽出済み（全ノード）と閉性判定ツール（`ledger/tools/check_needs.py`）
- 整形設定（.editorconfig / rustfmt.toml）と設計原則 rich-domain-model が
  `pekko-coding-rules` に入っていること
- run から Pekko v1.7.1 の原典を参照できること（作業コピー直下の `pekko/`
  または `../pekko`。triage が参照できなければ ABORT して報告する）

## 仕事の内容

1. `crates/fraktor-actor` に actor 最小核を実装する: ActorSystem の起動と停止、
   Props、actorOf、メッセージ送信と sender、メールボックスとセルの最小、
   暫定方針どおり自前の最小 executor。kernel／adaptor の境界は段階 1 の規則の
   まま（kernel はプラットフォームに触れない）
2. `crates/fraktor-testkit` を新設する（案 C の 1 対 1 写像）: TestKit 基底
   （PekkoSpec 相当の fixture、testActor、ImplicitSender、DefaultTimeout、
   expectMsg 系の最小）
3. 3 件のテストを移植する。置き場所は案 C の規則により
   `crates/fraktor-testkit/tests/` にノード 1 件＝ファイル 1 件。
   移植時にケースの実数を数え、台帳の `cases` に記録する
4. テストが赤くなることを確認し（red）、最小実装で緑にする（green）。
   台帳の status と `rust` 欄を更新する
5. `rollup.py --goal G0b` が 0 を返すことを確認する

## write_scope

- `crates/fraktor-actor/**`
- `crates/fraktor-testkit/**`（新設。これ以外のクレート新設は禁止）
- `ledger/nodes.yaml`（対象 3 ノードの欄のみ）
- ワークスペース直下の `Cargo.toml`（members 追加）等の初期設定ファイル

## provider

基盤ゴール扱い。割り当ては `.takt/runtime.yaml` の assignment で選ぶ
（codex の使用量上限中は interim-claude）。

## 完了条件（機械判定できる形）

- `cargo test -p fraktor-actor -p fraktor-testkit` が通る
- `uv run --with pyyaml python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal G0b` が 0 を返す
- `uv run --with pyyaml python3 ledger/tools/check_needs.py --ledger ledger/nodes.yaml --capabilities ledger/capabilities.yaml --goal G0b` が 0 を返す
- 台帳の差分が対象 3 ノードに限られる
- 3 ノードの `cases.total` が実数で記録されている（近似のままにしない）

## Gherkin 仕様

```gherkin
Feature: G0b TestKit の種と actor 最小核

  Scenario: 対象の 3 ノードが green になる
    Given 台帳の ImplicitSenderSpec、DefaultTimeoutSpec、TestActorsSpec が
      ゴール G0b に属し、unported である
    When ゴール G0b の run が完了する
    Then cargo test -p fraktor-actor -p fraktor-testkit が通る
    And 3 ノードの status が green で、cases.total が実数で記録されている
    And rollup.py --goal G0b が 0 を返す

  Scenario: 依存が閉じている
    When check_needs.py --goal G0b を実行する
    Then 終了コードは 0 である

  Scenario: 台帳の変更が対象ノードに限られる
    When ゴール G0b の run が完了する
    Then ledger/nodes.yaml の差分は、対象の 3 ノードの欄だけである
```

## やらないこと

- TestProbe、expectNoMessage、AutoPilot、watch（expectTerminated）の実装
  （G0c の仕事）
- EventFilter、TestLatch、within 等の層 2 能力（縦切りの次の基盤ゴール）
- typed（actor-typed、actor-testkit-typed）への着手
- 旧 fraktor-rs からのコードの持ち込み
- 対象外クレートの新設、台帳スキーマの変更

## 発行時の台帳前処理

発行の直前に、対象 3 ノードの `goal: null` を `goal: G0b` に書き換える。
台帳の差分はこの 3 行のみとする。タグの除去や付け替え（中止時）は人間のみが
決める。

検証は次の 2 コマンド。rollup が「ノード 3 件、status 内訳 {'unported': 3}」で
exit 1（発行前なので未完で正しい）、check_needs が「needs 閉性 OK」で exit 0
になること。

```sh
uv run --with pyyaml python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal G0b
uv run --with pyyaml python3 ledger/tools/check_needs.py --ledger ledger/nodes.yaml --capabilities ledger/capabilities.yaml --goal G0b
```

## 発行

発行は窓口が takt manager 経由で行う（コード生成は takt 内で行う運用。
2026-11-12 の是正指示）。ワークフローは `pekko-porting-ng`。タスクとして
渡す内容は本文書のパスと、完了条件が上記の機械判定 5 項目であること、
対象ノード以外の台帳変更と write_scope 外のクレート新設の禁止である。

run の後に人間が確認するコマンド：

```sh
cargo test -p fraktor-actor -p fraktor-testkit
uv run --with pyyaml python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal G0b
uv run --with pyyaml python3 ledger/tools/check_needs.py --ledger ledger/nodes.yaml --capabilities ledger/capabilities.yaml --goal G0b
git diff --stat ledger/nodes.yaml
```
