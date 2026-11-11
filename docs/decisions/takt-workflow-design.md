# TAKT ワークフロー設計（移植パイプライン）

状態: **設計決定済み（2026-11-11、判断ポイント a〜d はすべて決定、ファセットは
標準継承の方針）**。YAML 実装済みで、takt 0.75.0 で再確認した
（下記「現行仕様への適合」参照）。
persona と policy は標準ファセットをそのまま使う。
なお差し戻し上限（決定 c）は、TAKT の loop_monitors（cycle＋threshold＋judge）で
表現した。judge が非生産的と判定したら ABORT し、人間に上がる。

### 現行仕様への適合（takt 0.75.0、2026-11-12 に再確認）

0.74.0 で再構成した構成を、takt 0.75.0 で確認し直した。0.74.0 での通過は
根拠にしていない。ワークフローと facet に、0.75.0 のための変更は要らなかった。

**確認した結果（takt 0.75.0）**

- `takt workflow doctor` は、`pekko-porting-ng` と `pekko-port-node` の両方が OK
- `takt workflow inspect` の出力は、0.74.0 と 0.75.0 で一致する（参照先のパスを除く）。
  ステップ、遷移条件、return、loop monitor、facet の解決先に差は無い
- `takt prompt` で組み立てたプロンプトには、プロジェクト facet の全行が本文に載る。
  0.74.0 との差は、builtin の policy（`contract-change`、`testing`）の文言だけである
- 本文に載る上限（2,000 字）、loop monitor の照合、judge の実行は、0.74.0 と同じ実装である

**0.74.0 で再構成した点（0.75.0 でも変更不要）**

- **サブワークフロー化**：root の `pekko-porting-ng` はゴール単位の薄い
  ループ（`workflow_call` でノード処理を繰り返す）とし、ノード1件の処理は
  callable な子 `pekko-port-node`（`subworkflow.returns` で
  node_green／goal_complete／needs_human を返す）に分けた
- **verify は system step 化**：verify_red／verify_green は agent step では
  なく、固定コマンドを実行する system step とした。実体は
  `ledger/tools/verify_red.py`／`verify_green.py` で、分類・合否は JSON で
  返し、`when(...)` のルールで遷移する。直前ステップの対象（ノード、
  クレート、テストファイル）は `TAKT_PREVIOUS_RESPONSE_FILE` 経由で渡る。
  分類できない場合だけ、軽量な agent step（classify-red）に落ちる（決定 d の形）
- **人間の判断は run の外で受ける**：triage の未承認の除外提案は、
  子が needs_human を返して run が終わる。承認は人間が台帳の
  `cases.excluded` に記録し、再 run がそれを見て進む（第3節）。台帳が正である
- **設定の置き場所**：system step のコマンド実行許可
  （`workflow_system_commands.custom_scripts: true`）を `.takt/config.yaml` に
  置く。provider やモデルの割り当ては workflow YAML には書かず、
  runtime.yaml／config 側で持つ
- 標準 persona の改名に追随：等価性レビューの persona は `qa-reviewer`
  （旧）ではなく `testing-reviewer` を使う

**0.75.0 で消えた前提**

- permission judge は廃止された。`runtime.yaml` の `manager-permission-judge` の
  設定と `manager.sensitive_paths` は、書いてあっても無視される。このリポジトリに、
  それに頼る記述や設定は無い（全文を検索して確認した）
- director は巡回しない。`director.patrol_interval` は無視される。全体の見回りは
  manager の巡回が担う。`docs/design-settings-draft.md` の役割の説明を直した。
  凍結した baseline の `cover-memo.md` にある「director（常駐・監督）」は、
  0.75.0 より前の説明としてそのまま残す
- manager の巡回と、ワークフローの loop monitor は別物である

| | manager の巡回 | ワークフローの loop monitor |
|---|---|---|
| 動く場所 | daemon。run の外 | run の中。子ワークフロー `pekko-port-node` |
| きっかけ | 既定 1,800 秒ごと（`manager.patrol_interval`）。活動中のゴールがあるリポジトリだけを見る。人のマージを待つだけのゴールは除く | 差し戻しが連続した回数。review 系は 3 回目、verify 系は 5 回目 |
| 見るもの | 実行中のタスクの報告、レビュー指摘、修正計画が、指示と受け入れ条件の範囲内か。経過時間や繰り返し回数では判定しない | judge が、差し戻しが前進しているかを判定する |
| 結果 | manager が次の手を決める | 健全なら差し戻しを続け、非生産的なら run を ABORT する |

manager の巡回は回数を数えない。差し戻しの回数の上限を持つのは、loop monitor と
`max_steps` だけである。

**builtin の差分のうち、このワークフローに届くもの**

0.74.0 と 0.75.0 の builtin を比べた。`mini-core`、使っている step fragment、
instruction、output-contract（`test-report`、`testing-review`）、workflow rule に
変更は無い。届くのは、次の文言の変更だけである。

- policy `contract-change`、`testing`、`process-deviation`
- output-contract `supervisor-validation`（mini-core の supervise の報告）
- policy に取り込まれる partial `evidence-based-judgment`、`review-common`

「同じ問題が再発したら、前回の修理の前提を再検討する」という変更（新しい policy
`repair-strategy`）は、`review-remediation` と peer-review の fragment に入った。
`mini-core` はそれを参照しないので、develop の委譲の構造には影響しない。
実際の挙動は G0a の実走で見る。

**manager 経由でゴールを発行する場合に要るもの（発行経路は未決定）**

- manager が新しい開発タスクに選べるワークフローは、既定では `default` と `simple`
  だけである。`pekko-porting-ng` を使わせるには、`.takt/config.yaml` の
  `manager.workflows` に登録する。一覧を書くと、既定の候補は置き換わる。
  手動の CLI、既存タスクの再開、子ワークフローの呼び出しは制限されない

  ```yaml
  manager:
    workflows:
      - pekko-porting-ng: 台帳のゴール（ノードの集合）を、Pekko のテストから1件ずつ移植する
  ```

- コードを変えるゴールには Gherkin 仕様が付く。G0a のゴール定義に足す案は次のとおり
  である（未採用。発行経路が決まってから `docs/goals/g0a-pipeline-smoke.md` に入れる）

  ```gherkin
  Feature: G0a 台帳パイプラインの通し

    Scenario: 対象の 2 ノードが green になる
      Given 台帳の ActorPathSpec と RelativeActorPathSpec がゴール G0a に属し、unported である
      When ゴール G0a の run が完了する
      Then cargo test -p fraktor-actor が通る
      And 2 ノードの status が green で、cases.total が実数で記録されている
      And rollup.py --goal G0a が 0 を返す

    Scenario: 台帳の変更が対象ノードに限られる
      When ゴール G0a の run が完了する
      Then ledger/nodes.yaml の差分は、対象の 2 ノードの欄だけである

    Scenario: 1 ノード目だけが green の時点では完了にならない
      Given 1 ノード目が green で、2 ノード目が unported である
      When 完了判定を実行する
      Then rollup.py --goal G0a は 0 以外を返す
  ```

- 設定の `manager.permission`、`assistant.formal_spec`、`manager.main_merge` は、
  オーナーの判断待ちである（下の表）

**G0a の実走で確認する項目**

0.75.0 側の確認では決められず、実走で見る項目である。G0a の範囲とする。

1. mini-core への委譲（develop）が、0.75.0 の builtin で従来どおり回るか
2. 途中で止めた run を、保存された位置から再開できるか（1 回検証する）。手動の
   CLI なら `takt resume`、manager のタスクなら requeue と retry を使う
3. 再開したとき、前段の system step の結果（`context.verify_red`、
   `context.verify_green`）と構造化出力が復元されているか。classify-red と
   verify 系の judge は、この結果を instruction で参照する。子ワークフローの結果の
   復元は、同じ呼び出しに限られる
4. verify_red から再開したとき、`TAKT_PREVIOUS_RESPONSE_FILE` に port-test の応答
   （node、crate、test_file の 3 行）が渡るか。渡らないと verify_red は
   unclassifiable を返す。verify_green は、台帳の red のノードから対象を引ける
5. needs_human や ABORT で終わった run の成果物が残り、次に引き継げるか（下の
   「run の成果を捨てない設計」）
6. 再 run で、green のノードが飛ばされ、止まったノードがどこからやり直されるか
7. CI 待ちを manager の `takt_wait_command` に寄せる。このリポジトリには GitHub
   Actions のワークフローが無いので、待つ対象は PR のチェックになる
   （例：`gh pr checks <番号> --watch`。Actions を足したら `gh run watch --exit-status`）
8. judge が実際に下す判定（review 系 1 本、verify 系 2 本）

**run の成果を捨てない設計（2026-11-12 のオーナー方針）**

方針は「非対話の run が needs_human や ABORT で終わっても、その run の作業成果を
破棄しない」である。次は 0.75.0 の CLI のヘルプ、changelog、manager の knowledge、
実装の読みによる。実走では確かめていない。

| | 手動の CLI（`takt --pipeline --skip-git`） | manager のタスク |
|---|---|---|
| 作業の場所 | 発行したリポジトリの作業ツリーそのもの。作業用のコピーを作らない | ゴールブランチを基点にした、タスクごとの作業コピー |
| 止まったときのテストと実装 | コミットされない変更として作業ツリーに残る | タスクの作業コピーに残る。ゴールブランチには取り込まれない |
| レポート | `.takt/runs/<run>/` に残る。git の管理外である | run の記録として残る |
| 再開 | `takt resume`。直近の失敗または中断した run を、保存位置から続ける | requeue は保存位置から続ける。retry は指示を更新して同じタスクを戻す |
| 成果が消える操作 | 作業ツリーを reset や clean で戻す | タスクを片付ける操作（`takt_remove_goal_task`）。未取り込みの完了成果は、`discardUnmerged` を付けたときだけ破棄される |

ワークフロー側の事実は次のとおりである。

- ワークフローの中のエージェントは commit しない（takt の実行ルール）。
  「成果を run の外に残してから終わる」は、ワークフローの手順としては書けない。
  成果が残るかどうかは、発行経路と takt の後処理で決まる
- 除外提案は triage（読み中心で、やり直しが安い）で needs_human にする。後段で
  run を終える経路は 4 つある。port-test の needs_human（triage が見逃した除外提案）、
  equivalence-review の ABORT（等価性を確認できない）、review 系の judge の ABORT、
  verify 系の judge の ABORT である。どれも移植テストや実装ができた後に終わるので、
  成果は上の表のとおりに残る前提である
- 完了したノードは、台帳が green を保持する。ただし 1 run が 1 ゴールを通しで
  処理するので、2 ノード目で止まると、1 ノード目の成果もコミットや取り込みが
  されないまま、同じ作業コピーに残る
- 新しい run は triage から始まる。triage は「まだ処理していない最初の 1 件」を
  選ぶが、その判定に台帳の status を使うとは書いていない。rollup が走るのは
  台帳更新のときだけである。red まで進んだノードを新しい run が拾うと、port-test
  から入り直す。「やり直しは当該ノードの triage まで」が成り立つのは、保存位置からの
  再開を使った場合だけである
- ゴールへの所属を決める正が 2 つある。triage はゴール定義文書の一覧から選び、
  `rollup.py --goal` は台帳の `goal` 欄で数える。`goal` を green の記録と同時に
  書くと、1 ノード目が green になった時点で所属ノードが 1 件だけになり、完了と
  判定される（`rollup.py` を台帳の一時コピーで走らせて確認した）。
  `ledger-schema-draft.md` は、`goal` を「割り当て時に記入」としている

成果が消える経路と、やり直しが増える経路に対する回避策の案は次のとおりである。
どれも未決定で、ワークフローには入れていない。

1. ゴール用のブランチで run を動かし、止まった時点の作業コピーをそのブランチに
   コミットしてから次に進む。manager 経由なら、タスクの作業コピーが保持される。
   手動の CLI なら、run の後にコミットする手順を発行手順に足す
2. 1 タスクを 1 ノードにして、ノードが green になるたびにゴールブランチへ取り込む
   （承認済みのグラフモデル提案も、1 タスク 1 ノードを勧めている）
3. triage の選び方を、台帳の status で明記する。止まった run は、新しい run ではなく
   保存位置からの再開で続ける。status が red のノードを develop から始める分岐は、
   遷移の追加になる
4. ゴールを発行するときに、対象ノードの `goal` 欄を書く

**オーナーの判断待ち（2026-11-12）**

| 項目 | 推奨 | 理由 |
|---|---|---|
| G0a の発行経路（manager か、手動の CLI か） | manager 経由 | 設計の前提が director、manager、run の構成である。タスクごとの作業コピー、ゴールブランチへの取り込み、保存位置からの再開が使え、成果を捨てない方針に合う。手動の CLI は、差分がコミットされないまま作業ツリーに残る |
| `manager.permission`（既定 auto か、full か） | auto のまま | auto は Claude と Codex で provider 側の確認を使う。不可逆な操作は人間が承認する方針に合う。実走で必要な操作が止まる場合に full を検討する |
| `assistant.formal_spec`（既定はオフ） | オフのまま | このプロジェクトの正は Pekko のテストで、G0a の完了条件は機械判定できる。Quint と Alloy の仕様は、並行性を扱うゴールで検討する |
| `manager.main_merge`（既定 approve か、auto か） | approve のまま | auto は「以後のゴールは確認なしでマージしてよい」という恒常の指示になる。不可逆な操作は人間が承認する方針と合わない |
| G0a の `goal` 欄を誰がいつ書くか | 発行のときに、対象ノードへ書く | `ledger-schema-draft.md` の「割り当て時に記入」と合う。台帳を正に保てる |
| system step の結果を、次のステップと judge に渡す仕組み | verify のツールが出力の全文をファイルに残し、そのパスを JSON に入れる | 今は末尾だけが残り、直近の 1 回分しか渡らない。verify_green が落ちた理由は develop に渡っていない。台帳ツールの変更が要る |
| 成果を捨てないための回避策（上の 1 から 3） | 1 と 2 を採り、3 は triage の明記までにする | 1 と 2 は takt の仕組みで成果を残せる。red から始める分岐は遷移の追加で、G0a の実走を見てから決める |

### builtin 資産の活用（v3、2026-11-11）

実装⇄レビュー⇄修正のループは自前で組み直さず、builtin の **mini-core** に
委譲する形へ再構成した（backend-cqrs が development-core へドメイン
ファセットを注入するのと同じ流儀）。品質の核は takt の資産が担い、
プロジェクト側はドメイン固有の部分だけを持つ。

- 子ワークフローの develop ステップが `workflow_call` で mini-core を呼ぶ。
  plan → implement → 並列レビュー（AI アンチパターン＋supervisor の要件照合）
  → 修正ループ（履歴つき loop monitor）が mini-core の内側で回る
- ドメイン規約は引数で注入する：`plan_knowledge` と `development_knowledge`
  に `pekko-porting-rules` を加える（配列の先頭に置く。理由は v4 の
  「facet の並び順」）。これで implement、supervisor、fix の
  各ステップに規約が届く。プロジェクト専用の implement instruction は
  持たない（mini-core の implement は標準 instruction 固定のため）
- プロジェクト側に残るのは、台帳と原典と機械判定にまつわる部分だけ：
  triage、テスト移植、verify の system step、
  等価性レビュー（原典との忠実さは builtin レビューの守備範囲外なので
  独立ステップとして残す。指摘は develop の再呼び出しで直す）、台帳更新
- チェッカーのルールは policy としても事前提示する（後出しの回避）：
  syn チェッカーの判定項目は、policy `pekko-coding-rules` として
  mini-core の plan / development ポリシーと port-test のポリシーに加え、
  実装する側が書く時点で見えるようにする。チェッカーは事後の裏付けであり、
  ルールの正はチェッカー側の定義に置き、policy はその要約とする
- ファセットは `{extends:...}` で標準を継承する形にした：
  pekko-port-test は write-tests-first を、pekko-review-equivalence は
  testing-review を継承し、差分だけを書く。triage は、継承候補の
  research-analyze が調査反復型で意味が合わないため、独立のままにする
- 大きな基盤ゴール（G0b の TestKit 等）では、testing 工程と companion、
  レビュースイートを持つ development-core への格上げを検討する。
  その場合も引数の注入で切り替える

### builtin 資産の再利用の棚卸し（v4、2026-11-11）

設計原則は「builtin の facet、step fragment、workflow をできるだけ再利用する。
独自 facet はプロジェクト固有の意味論が必要な所だけに置き、新規作成ではなく
`{extends:...}` で差分だけを書く」である（2026-11-11、ユーザー指摘）。
この原則で `.takt/` の全要素を棚卸しし、次の 3 つに仕分けた。
ステップの順序、遷移条件、return、system step のコマンド、差し戻し上限は
変えていない（`takt workflow inspect` の出力で前後を比較して確認した）。

**builtin で置き換えたもの**

| 対象 | 置き換え先（builtin） |
|---|---|
| port-test のステップ定義 | step fragment `development-core-write-tests` |
| port-test のレポート形式（独自の `verify-report`） | output-contract `test-report`。ファイル名は `test-report.md` |
| equivalence-review のステップ定義 | step fragment `development-testing-reviewer` |
| 指摘と用語の書き方の規則（無し） | workflow rule `findings-handling` と `plain-terminology`（`all_steps.rules`。mini-core と同じ） |
| ループ判定の入力（無し） | judge の `input_reports`。等価性レビュー報告と、その前 2 回分の履歴を渡す |

**extends で差分だけを書くもの**

| プロジェクト facet | 親（builtin） | 差分 |
|---|---|---|
| instruction `pekko-port-test` | `write-tests-first` | 原典の移植であること、置き場所、ケースの扱い、test-report への書き方、機械判定が読む 3 行 |
| instruction `pekko-review-equivalence` | `testing-review` | 原典との突き合わせの 3 観点 |
| instruction `pekko-loop-judge` | `loop-monitor-reviewers-fix` | 選べる結果が 2 つであること、標準の語（収束中、停滞または発散）との対応、非生産的と判定したときの人間向け要約 |
| instruction `pekko-verify-red-loop-judge`、`pekko-verify-green-loop-judge` | `loop-monitor-reviewers-fix` | 選べる結果が 2 つであること、標準の語との対応、直近の system step の出力、非生産的と判定したときの人間向け要約 |
| output-contract `equivalence-review` | `testing-review` | 対象、原典側の根拠、深刻度、追加の 3 観点、結果と遷移条件の対応、未確認範囲が残る間は APPROVE にしないこと、指摘と未確認範囲の両方があるときは REJECT を優先すること |

**独自のまま残すもの**

| プロジェクト facet | builtin で表現できない理由 |
|---|---|
| instruction `pekko-triage`、output-contract `triage-report` | 台帳のノード 1 件について、原典のケース実数、needs、前提基盤、除外提案を確かめる工程である。builtin には台帳も、人間だけが承認する除外も無い。近い instruction のうち `research-*` は計画、分解、分析を別ステップで回す調査用で、`plan` は実装方針の立案用（mini-core が内側で行う）である。fragment `development-core-plan` はレポート名が `plan.md` に固定で、mini-core が書く `plan.md` と同名になる。`triage-report` の欄は遷移条件と後段が読む固定欄で、`research-report` には無い |
| instruction `pekko-classify-red`、output-contract `verify-report` | 固有の機械ゲート verify_red が分類できなかった出力を読み、想定どおりの red かを判定して台帳の `status` を書く工程である。builtin の検証系 instruction（`verify-fix`、`apply-fix-verification`、`merge-ci-investigate`）は `fix-plan.md` や CI の収集結果など、このワークフローに無い入力を前提にしている |
| instruction `pekko-ledger-update`、output-contract `ledger-report` | 台帳への green の記録と、`rollup.py` による完了判定である。builtin に対応する工程が無い |
| knowledge `pekko-porting-rules`、policy `pekko-coding-rules` | 決定文書（台帳の規則、クレート構成、所有権モデル）と syn チェッカーのルールの要約である。builtin の `architecture` や `coding` を置き換えるものではなく、並べて注入する |

ワークフロー 2 本（`pekko-porting-ng`、`pekko-port-node`）も独自である。
ゴールとノードの繰り返し、verify の system step、return ラベルは台帳の
進め方そのもので、builtin のワークフローには無い。

**facet の並び順**

takt 0.74.0 は、1 ステップの knowledge を連結した文字列の先頭 2,000 字だけを
プロンプト本文に載せる（policy も同じ）。残りはスナップショットファイルに
書かれ、エージェントはそれを読むよう指示される。builtin の facet は 1 本で
2,000 字を超えるものが多い（`architecture` は約 12,000 字）ので、builtin を
先に並べるとプロジェクトの規約は本文に 1 行も載らない。

- knowledge `pekko-porting-rules`（1,935 字）は、配列の先頭に置く。
  mini-core への引数（`plan_knowledge`、`development_knowledge`）と、
  equivalence-review の knowledge が該当する。equivalence-review は
  fragment の並びを上書きしている
- `pekko-porting-rules` は 2,000 字以内に保つ。超えると末尾の
  「移植の作法」（テストファイルの凍結を含む）から本文に載らなくなる
- policy `pekko-coding-rules`（2,617 字）は、どのステップでも本文に載って
  いない。builtin の fragment が `contract-change` を先頭に固定するためで、
  引数の並びでは変えられない。スナップショットファイル経由では届く。
  本文に載せる手当ては、builtin 再利用とは別件として切り出す
  （2026-11-11、ユーザー決定）。この PR では現状のままにする

**ループ上限の効き方（PR #2 のレビュー指摘、2026-11-11 のユーザー決定）**

main の loop monitor（cycle が equivalence-review、develop の順）は発動しなかった。
実際の経路は develop、verify_green、equivalence-review、develop の順で、takt は
完了したステップの並びをそのまま照合するためである。次のとおりに直した。

- cycle を develop、equivalence-review の順にし、`ignore_steps` で verify_green を
  照合から外す。judge の遷移（健全なら develop、非生産的なら ABORT）は
  変えていない
- review 系の上限 3 回（決定 c）は「連続 3 回目の指摘で judge が判定する」
  という意味である。3 回差し戻した後に判定するのではない。judge が健全と
  判定すると回数は 0 に戻り、次の判定はそこから連続 3 回目の指摘になる

次の 2 点は仕様として受け入れた（後で誤解しないために明記する）。

- 連続が切れる場合がある。verify_green の失敗で develop をやり直すと、
  それより前の指摘は連続に数えられない。例：2 回目のラウンドで verify_green が
  1 度落ちると、judge が呼ばれるのは 3 回目ではなく 4 回目の指摘になる。
  takt の照合は並びの完全一致で、設定では避けられない
- 全体の上限は `max_steps` が持つ。親 `pekko-porting-ng` の `max_steps: 80` が
  run 全体の予算で、子ワークフローと mini-core の中のステップも同じ予算から
  引かれる。エージェントステップ、system step、judge の呼び出しが 1 ずつ
  消費し、`workflow_call` 自体は消費しない。予算に達すると run は止まる
  （Max steps reached）。連続が切れて judge が呼ばれない場合も、ここで止まる

**verify 系の上限（2026-11-11 のユーザー決定で追加）**

決定 c の verify 系 5 回を、loop monitor 2 本で表す。どちらも「連続 5 回目の
差し戻しで judge が判定する」という意味で、judge の結果は健全（差し戻しを
続ける）と非生産的（ABORT）の 2 つである。5 回で無条件に ABORT にはしない。
review 系と同じく judge が判定する（2026-11-11、ユーザー決定）。

| ループ | cycle | 数える差し戻し | 連続が切れる場合 |
|---|---|---|---|
| verify_red | port-test、verify_red（classify-red は照合から外す） | テストが最初から通る、または classify-red が想定外と判断して、port-test へ戻る | 想定どおりの red になって develop へ進む |
| verify_green | develop、verify_green | 機械ゲートが落ちて develop へ戻る | ゲートを通って equivalence-review へ進む |

- judge が呼ばれるのは、差し戻しで cycle の先頭（port-test または develop）へ
  戻るときだけである。5 回目の実行でテストが想定どおりの red になった場合や、
  ゲートを通った場合は、judge は呼ばれずに先へ進む。takt 0.74.0 の検出器は、
  自然な遷移が cycle の先頭へ戻るときだけ発動する
- judge には、直近の system step の出力を instruction の `{context:...}` で渡す。
  system step の結果はレポートではないので、`input_reports` では渡せない。
  まだ実行されていない step の結果を参照すると失敗するので、judge の
  instruction はループごとに分けた
- judge は、失敗の理由が同じかどうかだけでは決めない。同じゲートが落ち
  続けていても、失敗が減っているか、修正が反映されて内容が変わっていれば
  健全とする。同じ失敗が同じ内容のまま繰り返され、修正も前進していなければ
  非生産的とする（builtin の judge の基準と同じ）
- judge に渡る system step の出力は、直近の 1 回分だけである。過去の回の出力は
  残らない。過去の回は、port-test や develop の報告の履歴で補う。classify-red の
  報告は渡せない（takt は judge の入力を cycle の中のステップに限る）。
  渡る出力も全文ではない。`verify_green.py` は各ゲートの出力の末尾 800 字、
  `verify_red.py` は末尾 1,500 字だけを残す。judge には、切れて読めない原因を
  未確認として扱わせる。各回の出力を全文で残すには、台帳ツールかステップ
  構成の変更が要る（要判断）
- judge が呼ばれた回は、次のステップに渡る前段の応答が judge の応答になる。
  verify_red 側の judge は、健全と判定したときに、分類と red にならなかった
  理由を応答に書く。port-test がそれを受け取る
- judge が判定できなかった場合（provider のエラーなど）、takt は差し戻しを
  そのまま続け、回数を 0 に戻す。review 系を含む 3 本とも同じで、ワークフロー
  側では変えられない。その場合に効く上限は `max_steps` だけである。judge の
  provider は、runtime の設定（`internal_agents` の `loop-judge`）で割り当てる
- 回数はノードごとに数える。子ワークフローは呼び出しのたびに新しい状態で始まる
- judge が判定すると、3 本の loop monitor の回数がすべて 0 に戻る。takt の
  検出器が 1 つの履歴を共有するためである

この 2 本を足す前に verify 系の差し戻しを止めていたのは、`max_steps` だけで
ある（takt 0.74.0 のコードで確認した）。

- loop monitor は無かった
- takt 組み込みのループ検出は、同じステップが連続した回数だけを数える
  （既定は 10 回で、警告のみ）。verify 系の差し戻しは 2 つのステップが交互に
  走るので、当たらない
- `max_steps: 80` から計算すると、verify_green の差し戻しは 1 ノード目で
  19 回前後まで回る。1 回に少なくとも 4 ステップ（mini-core の plan、implement、
  reviewers と verify_green）を使うためである。verify_red の差し戻しは
  1 回 2 ステップで、39 回前後になる

G0a の発行コマンドで参照した旧 fraktor-rs の
`pekko-porting` ワークフローをそのまま使う前提は置き直す。ng の決定済み設計から
ワークフローを設計し直すための叩き台であり、決定ではない。
なお旧ワークフローのステップ定義自体は旧リポジトリには無く（persona の割り当てが
`.takt/config-claude.yaml` に残るのみ）、本書は ng の要件からの新規設計である。

## 1. ワークフローが守る不変条件（決定済み設計から）

- 1 run は 1 ゴール（台帳のノード集合）を扱い、ゴール内ノードは 1 件ずつ処理する。
  `write_scope` の外は書かない
- テスト先行。red を確認していない実装は存在しない
- ケース除外と `do_not_port` は人間だけが決める。ワークフローは提案を出して
  人間の確認を待つ。勝手に除外して green にしない
- 台帳の状態遷移は unported → red → green の決まった遷移だけ。ケース数は
  近似ではなく実数を記録する
- 機械検証を通らない成果物はレビューに進まない（安いゲートを先に置く）
- 実装ステップはテストファイルを編集できない（テスト改変による green 化を
  構造で塞ぐ）

## 2. ステップ構成（提案）

### 2.1 triage（編集不可）

- Pekko の原典テストと実装を読み、ケースの実数、needs（TestKit 能力・前提フラグ）、
  infra 辺（前提基盤が green か）を確認する。出力は triage レポート
- 分岐：前提基盤が未 green → ABORT（ゴール選定の誤りとして manager に返す）。
  未承認の除外提案（ケース除外・do_not_port）がある → needs_human で
  終了（判断は run の外、第3節）。問題なし → port-test へ
- provider: クラウド上位モデル（解釈と判断が重い工程）

### 2.2 port-test（編集可、テストファイルのみ）

- Rust テストを書く。置き場所は案Cの規則（検証対象クレートの `tests/`、
  ノード1件＝ファイル1件）。ファイル冒頭に出典（Pekko v1.7.1 のパス）を残す
- 分岐：移植できないケースが見つかり、未承認の除外提案がある → needs_human で
  終了（triage と同じ扱い。PR #2 のレビュー指摘で追加）。移植完了 → verify-red へ
- provider: 量産期は DGX（DeepSeek V4.1 Flash）。基盤期（G0a〜G0c）はクラウド

### 2.3 verify-red（機械検証）

- `cargo test` を実行し、失敗することと、失敗理由が想定どおりであること
  （未実装によるコンパイルエラー、またはアサーション失敗）を確認する
- 最初から通る、または無関係な理由で落ちる場合は port-test へ差し戻す
  （ループ上限つき）
- 通過したら台帳の status を red に更新する（台帳に触れるのは 2.3 と 2.7 のみ）

### 2.4 implement（編集可、write_scope 内の実装のみ）

- テストを通す最小実装を書く。テストファイルは編集不可
- instruction に、決定済みの規約を明記する：所有権モデル（単独所有＋ハンドル型、
  Arc は限定箇所のみ）、port & adapter 段階1（kernel はプラットフォームに触れない、
  Clock / Scheduler は port 越し）、クレート構成の規則
- provider: 量産期は DGX、基盤期はクラウド

### 2.5 verify-green（機械検証）

- `cargo test`、`cargo clippy`、`cargo fmt --check`、syn checker
  （port-adaptor 境界、1ファイル1公開型ほか）、aislop を実行する
- どれか落ちたら implement へ差し戻す（ループ上限つき）

### 2.6 equivalence-review（編集不可、実装と別系統のモデル）

- Pekko 原典と、移植テスト・実装を突き合わせる。見る観点は2つ：
  移植の忠実さ（テストの弱体化、ケースの取りこぼし、近似の混入）と、
  振る舞い互換（API と意味論が Pekko とず
れていないか）
- 旧 fraktor-rs の persona 名（pekko-porting-qa-reviewer、
  pekko-porting-compat-reviewer）を踏襲する
- provider: 実装者と別系統のクラウド上位モデル固定
- 指摘あり → implement へ差し戻し（ループ上限つき）。なし → ledger-update へ。
  指摘はないが、原典または移植側を確認できず等価性を判断できない → ABORT
  （確認しないまま green にしない。指摘と未確認範囲の両方がある場合は、
  指摘を優先して差し戻す。PR #2 のレビュー指摘で追加）

### 2.7 ledger-update（編集可、台帳の対象ノードのみ）

- status を green にし、`cases` の実数と `rust` 欄（crate、test_file）を記録する
- `ledger/tools/rollup.py --goal <ID>` で完了判定する。未完なら次のノードの
  triage へ戻る。全ノード完了で COMPLETE

## 3. 人間の判断の受け方（HOTL 前提）

TAKT は HOTL（Human on the Loop）の道具であり、run の途中に同期的な
人間の判断は差し込まない。判断が要る場面では、run は提案と根拠を
レポートに残して終わり、判断は run の外（manager との会話、台帳への
記録）で行われる。再 run は、台帳に記録された判断を見て進む。

- triage の除外提案（ケース除外、`do_not_port` の提案）と、port-test で
  移植中に見つかった除外提案：run は
  needs_human で終了する。承認の記録先は台帳の `cases.excluded`
  （人間が書く。項目があることが承認済みを意味する）。`do_not_port` は
  人間が status に直接書く
- 差し戻しループの上限到達（verify 系、review 系とも）：judge が非生産的と
  判定したら ABORT して manager に上がる
- `write_scope` の拡張が必要になった場合（前提基盤の追加実装が要る等）。
  スコープ変更は manager と人間の判断であり、run の裁量にしない

（旧案の「ワークフロー内に人間チェックポイントの step を置く」形は、
この前提と合わないため廃止した。2026-11-11、ユーザー指摘による）

## 4. provider 配分の原則

- 判断系（triage、equivalence-review）はクラウド上位モデルに固定する。
  「設計・契約の判断はトップのクラウドモデル」というユーザーの方針と、
  承認済み論点19（基盤ゴールはクラウド、移植ゴールは DGX）の延長
- 量産系（port-test、implement）は DGX に下ろせる形にしておくが、基盤期は
  全ステップをクラウドで回し、手順が固まってから下ろす
- 機械系（verify-red、verify-green、rollup）はモデルを使わない

## 5. 旧 fraktor-rs の pekko-porting との違い

- 検証器が dylint から syn checker に替わる（決定済み）。aislop は継続する
- 進捗の正が gap-analysis 文書から台帳に替わり、状態遷移と完了判定が
  rollup.py の機械判定に接続される
- implement ステップからテストファイルの編集権限を外した
- 除外判断のエスカレーションをステップの分岐として明示した

## 6. 判断ポイント（すべて決定済み）

a. ワークフローは全ゴール共通の1本か、基盤用／量産用に分けるか。
   **決定済み（2026-11-11、ユーザー）：当面1本**。provider の切り替えは
   ゴール種別でステップ設定側が持つ
b. equivalence-review の2観点を、並列2ステップにするか統合1本にするか。
   **決定済み（2026-11-11、ユーザー）：基盤期は統合1本**、量産期に分離を検討する
c. 差し戻しループ上限の初期値。
   **決定済み（2026-11-11、ユーザー）：verify 系は 5 回、review 系は 3 回**。
   上限に達すると judge が判定し、非生産的なら ABORT して manager に上げる。
   review 系の 3 回は「連続 3 回目の指摘で判定」の意味である（2026-11-11、
   ユーザー確認）。verify 系の 5 回も同じ数え方で、「連続 5 回目の差し戻しで
   判定」の意味である。数え方と受け入れた制限は「ループ上限の効き方」に書いた
d. verify-red で「失敗理由が想定どおりか」の判定を機械だけで閉じるか、
   軽量モデルの判定を挟むか。
   **決定済み（2026-11-11、ユーザー）：機械判定を既定とし、分類できない場合のみ
   軽量モデルに回す**

## 7. ファセットの方針（標準ファセットの継承）

ワークフローのステップが使う persona、instruction、knowledge、policy、
output-contract は、ゼロから書かず **TAKT の標準ファセットを継承し、ng 固有の
差分だけをプロジェクトファセットとして置く**（2026-11-11、ユーザー指摘）。
標準側の更新に追随でき、書く量も減る。

ステップごとの割り当て（全 facet の仕分けと理由は「builtin 資産の再利用の棚卸し（v4）」）：

| ステップ | 継承する標準ファセット | ng 固有の差分 |
|---|---|---|
| triage | persona: research-digger（自分で調べて報告する役。research-analyzer は「自分で調査を実行しない」役なので合わない） | instruction と output-contract は独自（台帳の triage） |
| port-test | step fragment development-core-write-tests（persona: coder、policy: contract-change）、output-contract: test-report | instruction は write-tests-first を継承＋差分：出典ヘッダ、1ノード1ファイル、test-report への書き方 |
| implement | mini-core 内蔵（persona: coder、標準 instruction） | プロジェクト instruction は持たず、knowledge 注入で規約を届ける |
| equivalence-review | step fragment development-testing-reviewer（persona: testing-reviewer、policy: testing、contract-change、review）、policy に ai-antipattern | instruction と output-contract は testing-review を継承＋差分：Pekko 原典との突き合わせ観点 |
| ledger-update | persona: coder | instruction と output-contract は独自（台帳の記録と完了判定） |
| ループ判定（review 系 1 本、verify 系 2 本） | persona: supervisor | instruction は loop-monitor-reviewers-fix を継承＋差分：人間向け要約。verify 系は直近の system step の出力も渡す |

ng 固有の規約は各 instruction に散らさず、knowledge ファセット
（例：`pekko-porting-rules`）に集約し、必要なステップから参照する。
