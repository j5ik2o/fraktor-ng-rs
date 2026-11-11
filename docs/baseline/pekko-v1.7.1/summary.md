# Pekko 事実層分析サマリ（fraktor-rs 再実装向け）

## 基準タグ

- **v1.7.1**（Apache Pekko、https://github.com/apache/pekko.git を `--depth 1` で clone）
- 選定理由：`git ls-remote --tags` で確認した v1.x.x 系の安定版リリースタグの最大。v1.8.0-M0 はマイルストーン版、1.7.1-RC1 等は RC のため除外した。

## 主要な数字

| 項目 | 値 |
|---|---|
| モジュール数（`pekkoModule` プロジェクト） | 39 |
| モジュール依存の辺（build.sbt の dependsOn 由来） | 152 |
| 主ソース総行数 / テスト総行数 | 474,887 / 294,382 |
| テストクラス総数（src/test の *Spec/*Suite/*Test/*Tests ファイル） | 1,262 |
| テスト件数（近似、後述の数え方） | 約 7,561 |

テストクラスが多いモジュール：stream-tests 254、actor-tests 177、docs 173（docs はドキュメント用コードのテストソースを含むため、移植対象としては他と区別すべき）、actor-typed-tests 117、remote 107（remote は主ソース 68,021 行で最大級）。

## TestKit 使用頻度（テストソース全体、正規表現の出現回数・import 含む）

| 機能 | 出現回数 |
|---|---|
| expectMsg | 2,203 |
| TestProbe | 1,756 |
| expectNoMessage | 997 |
| TestKit（識別子） | 614 |
| expectMsgType | 600 |
| TestLatch | 221 |
| within | 183 |
| expectMsgPF | 146 |
| testActorRef / TestActorRef | 95 |
| expectMsgAnyOf | 8 |
| TestBarrier | 1 |

読み解き：expectMsg 系（expectMsg + expectMsgPF + expectMsgType + expectMsgAnyOf + expectNoMessage ＝ 3,954 回）と TestProbe が圧倒的多数。**TestKit 基盤の実装優先度は TestProbe と expectMsg 系が最優先**で、TestBarrier / expectMsgAnyOf は後回しにできる。TestProbe の利用は actor-typed-tests（338 回）・actor-tests（263 回）・persistence-typed-tests（196 回）に集中している。

## 前提フラグ（移植時の読み替え候補、テストソース内の出現回数）

Thread.sleep 295、Random 111、eventually 108、System.currentTimeMillis 104、System.nanoTime 91。
モジュール別では actor-tests（Thread.sleep 79、nanoTime 32）と stream-tests（Thread.sleep 67）に集中する。

## 分析手法

- すべて機械的抽出（`scripts/analyze.py`、再実行は `scripts/run.sh`）。LLM によるコード読みはしていない。コンパイル・sbt 実行は行っていない（この VM には Java / sbt が無く、`sbt projects` との突き合わせは未実施）。
- モジュールと依存：build.sbt の `lazy val x = pekkoModule("dir")` と、そのブロック内の `.dependsOn(...)` に現れる既知プロジェクト識別子から抽出（文字列・コメント除去後）。スコープ（test / optional 等）は区別していない。
- テスト件数の近似：テストクラスファイル内の `"..." in`（WordSpec 形式）、`test("`（FunSuite / typed 形式）、`@Test`（JUnit）の正規表現マッチ数の合計。ヘルパーが生成するテストやプログラム的に登録されるテストは正確には数えられない。
- テスト→実装の対応：**import ベースの近似のみ**。テストファイルの `org.apache.pekko.*` import（testkit パッケージを除く）から参照型を拾い、主ソース内の class/object/trait/enum 宣言で定義モジュールを推定した。

## 近似の限界（正直なところ）

- テスト→実装対応のカバー率は低い：参照型が1つ以上取れたテストクラスは **292 / 1,262** に留まる。主因は、テストが対象コードと同一パッケージにあって import が不要なケースと、ワイルドカード import（`org.apache.pekko..._` / `.*` を使うファイルがリポジトリ全体で 230 件）のため。
- `src/multi-jvm` のテスト（8 モジュールに存在）はファイル数のみ数え、テストクラスとしては分析していない。
- ファイル名が Spec/Suite/Test/Tests で終わらないテストクラスは拾えていない。
- protobuf-v3（シェード jar 専用）と scala-nightly（集約専用）はソースを持たないため統計はゼロ。
- TestKit の `within` は TestKit 専用メソッドではないため、183 回には他用途も含まれる可能性がある。

## 次に SemanticDB / SCIP で厳密化すべき点

1. **テスト→実装の対応**：同一パッケージ参照とワイルドカード import を解決し、292/1,262 のカバー率を実用水準まで上げる（最優先）。
2. **dependsOn のスコープ分離**：sbt が動く環境で `sbt projects` / 依存レポートと突き合わせ、compile / test / optional を区別する。
3. **TestKit 使用の意味解析**：単なる出現回数ではなく「どの Probe に対してどの expect が呼ばれるか」の対応を取り、Rust 側 TestKit の API 設計に直接使える形にする。
4. **multi-jvm テストの棚卸し**：クラスタ系の移植順序を決める材料として、テストクラス単位まで降ろす。

## 成果物

- `ledger.yaml`：モジュールごとの依存・統計・テストクラス一覧・TestKit 使用・前提フラグの台帳（手法と限界の注記を含む）
- `module-deps.dot`：モジュール依存グラフ（DOT、152 辺）
- `scripts/analyze.py` / `scripts/run.sh`：抽出スクリプト（再実行可能）
