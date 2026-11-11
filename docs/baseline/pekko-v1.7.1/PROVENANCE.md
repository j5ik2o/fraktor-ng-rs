# PROVENANCE — Pekko v1.7.1 事実層スナップショット

このディレクトリは凍結スナップショットである。原則として変更しない。
内容の更新が必要になった場合は、ここを書き換えるのではなく、新しい基準タグ /
再分析として別スナップショットを作り、差し替えを明示的な判断として記録する。

## 由来

- 元になった配布物: `fable-input-fraktor-restart.zip`
  （Fable への判断層設計の投入用に 2026-11-11 に作成されたパッケージ）
- zip の SHA-256: `985b361710ccd00ba3dd2ec386b49710067b9cb6c9b0983623fd4b77c4e5efba`
- zip 原物の保管場所: Muse ワークスペース（`~/workspace/your_files/`）。git には入れない
- fraktor-ng-rs への取り込み決定: 2026-11-11（凍結＋PROVENANCE 方式をユーザーが採用）
- 分析対象: Apache Pekko `https://github.com/apache/pekko.git`、基準タグ **v1.7.1**
  （安定版 v1.x 系の最大。v1.8.0-M0 等のマイルストーンと RC は除外）
- 分析の生成物一式（再実行可能な抽出スクリプト含む）はワークスペースの
  `~/workspace/fraktor-restart/pekko-analysis/` にあり、`scripts/analyze.py` が抽出本体

## ファイル対応表と SHA-256

| このディレクトリの名前 | zip 内の元の名前 | SHA-256 |
|---|---|---|
| `ledger.yaml` | `ledger.yaml` | `f0ab69cb1c7093eeb369cb6a299719f63f09cdf560bf918f8a5a8afad0a401bd` |
| `summary.md` | `summary.md` | `96a552684dd5e209f3bc529fda14dd4755a0f0fccc5d21715fb8250942f57f11` |
| `module-deps.dot` | `module-deps.dot` | `2c7926a4d6403a5f0ee3a877508436f9d5a71d4437efae7696f3cec170e1440d` |
| `brief.md` | `pekko-analysis-brief.md` | `abd680ec742022e1686403e234a94a35f957c9e83d6d90e269bdcb3a32d04808` |
| `cover-memo.md` | `00-cover-memo.md` | `c07b3f076a55a84a7b787175d3e12d5b84e8e1c561aa36dc07119b6170d6833a` |

注: `brief.md` と `cover-memo.md` は取り込み時に改名しただけで、内容は無変更。
`cover-memo.md` は Fable 宛ての依頼文であり、このリポジトリの設計文書ではない。
依頼範囲の5項目（ノード粒度／辺の定義／ゴールの切り方／最初の縦切り／ロールアップ）は
設計課題として別途管理し、Fable の回答は `docs/decisions/` に提案として受ける。

## 手法（要約）

- すべて機械的抽出（正規表現とソース走査）。LLM によるコード読みはしていない
- コンパイル・sbt 実行は行っていない（分析環境に Java / sbt が無く、`sbt projects` との
  突き合わせは未実施）
- モジュールと依存は `build.sbt` の `pekkoModule` 定義と `.dependsOn(...)` から抽出
  （スコープ compile / test / optional は区別していない）
- テスト件数は `"..." in` / `test("` / `@Test` の正規表現マッチによる近似
- テスト→実装の対応は import ベースの近似のみ

## 主要な数字

- 39 モジュール、モジュール依存 152 辺
- 主ソース 474,887 行 / テスト 294,382 行
- テストクラス 1,262 件、テスト件数 約 7,561（近似）
- TestKit 頻度: expectMsg 2,203、TestProbe 1,756、expectNoMessage 997、expectMsgType 600

## 既知の限界（このスナップショットを使う前に必ず踏まえること）

- テスト→実装対応のカバー率は 292 / 1,262 クラスに留まる。主因は同一パッケージ参照
  （import 不要）とワイルドカード import（230 ファイル）。SemanticDB / SCIP による
  厳密化は未実施で、次段の作業として残っている
- テスト件数は近似であり、ヘルパー生成・プログラム的登録のテストは正確に数えられない
- `docs` モジュールのテストクラス 173 件はドキュメント用コードを含むため、
  移植対象として他と区別が必要
- `src/multi-jvm` のテストはファイル数のみ数え、テストクラスとしては分析していない
- ファイル名が Spec / Suite / Test / Tests で終わらないテストクラスは拾えていない

## このスナップショットの位置づけ

- ここにあるのは**事実層の種**であり、進捗の正ではない
- 進捗の正は、テストクラス粒度で別途持つリポジトリ台帳（`ledger/`）である
- このスナップショットを直接編集して進捗を記録してはならない
