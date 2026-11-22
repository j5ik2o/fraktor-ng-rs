# ゴール G0Issue96：SupervisorTreeSpec

状態: 定義済み・未発行（2026-11-22 の準備時点）。この文書は、GitHub issue #96（https://github.com/j5ik2o/fraktor-ng-rs/issues/96）の単一ノードのゴール定義である。`goal_complete_gate.py` の完了関門が先頭行から ID を取るため、ゴール ID は専用の `G0Issue96` とする（この準備では `Issue96` を `G0Issue96` に置き換える）。

この準備タスク自体は、ゴール文書の作成と台帳への `goal: G0Issue96` のタグ付けだけを行い、そこでは止まる（Rust テスト作成、本番編集、`status` の `green` 化、CI・workflow 変更、後続タスクの登録・開始、PR 公開、merge はしない）。一方、この文書が定義するゴールそのものの完了は、後続の `fraktor-porting-ng` run が `status` を `unported` から green へ遷移させ、単一の merge-ready な PR として提出するところまでを指す。以下の完了条件と必須ゲートは、そのゴール完了（単一の merge-ready PR）に対する要求である。

kind: single（ノード 1 件の移植。基盤ゴールの G0a〜G0f とは別系統の単発ゴール）
requires: なし（前提とするテスト基盤は、G0a〜G0f が main に入って green 済みであること。2026-11-22 に `rollup.py --goal G0a` から `--goal G0f` を実行し、すべて exit 0 であることを確認した）

## 依拠した原典

- Pekko v1.7.1。ローカルの検証済みチェックアウト `/Users/j5ik2o/Sources/github.com/j5ik2o/pekko` は `git rev-parse HEAD` と `git rev-parse 'v1.7.1^{commit}'` がいずれも `9a54d1294c3ac7821bbed3c940769c3d5027d1ff` であり、`git status --short` の出力は空だった。`tools/pekko-source/prepare_pekko_source.sh` は同じ commit を cwd の `pekko/` に用意する（スクリプト内の `expected` と一致）。
- 原典ファイル: `actor-tests/src/test/scala/org/apache/pekko/actor/SupervisorTreeSpec.scala`（全 51 行。2026-11-22 に全文を読んで確認した）。

## 原典の実ケースと契約（原典 :23–51）

原典は PekkoSpec を継承し、`ImplicitSender` と `DefaultTimeout` を混ぜる（`:23`）。`"In a 3 levels deep supervisor tree (linked in the constructor) we" must { ... }` ブロックの中に、ケースが 1 件だけある（`:25–50`）。

実ケース名は次の 1 件である。

`be able to kill the middle actor and see itself and its child restarted`

契約と時間条件を、原典の行に対応づけて記録する。

1. 全体を `EventFilter[ActorKilledException](occurrences = 1).intercept { ... }` で囲む（`:28`）。ブロック終了時に、`ActorKilledException` のログがちょうど 1 件だったことを検査する。
2. その内側を `within(5.seconds) { ... }` で囲む（`:29`）。ブロック内の処理が 5 秒を超えてはならない。
3. 3 階層を、head・middle への `Props` 要求と ActorRef 応答で構成する（`:30–40`）。各アクターは `receive` で `case p: Props => sender() ! context.actorOf(p)` を行う。`headActor = system.actorOf(p)`、`middleActor = Await.result((headActor ? p).mapTo[ActorRef], timeout.duration)`、`lastActor = Await.result((middleActor ? p).mapTo[ActorRef], timeout.duration)` の順で作る。
4. 全アクターが `OneForOneStrategy(maxNrOfRetries = 3, withinTimeRange = 1.second)(List(classOf[Exception]))` を持つ（`:31–32`）。したがって、`ActorKilledException` を含む `Exception` の再起動判断が 3 回まで許される。
5. `preRestart(cause, msg)` は自身の `self.path` を `testActor` へ送り、既定の `preRestart` を呼ばない（`:36`）。既定実装を呼ばないため、子は停止されない。
6. middle へ `Kill` を送る（`:42`）。
7. `expectMsg(middleActor.path)`（`:43`）、続けて `expectMsg(lastActor.path)`（`:44`）で、通知の順序を確かめる。すなわち、再起動する middle が先に `preRestart` を呼び、その後に、生存した子 last が middle の再生成で restart されて `preRestart` を呼ぶ。
8. `expectNoMessage(2.seconds)`（`:45`）で、その後 2 秒間に追加の通知が無いことを確かめる。head は再起動しないこと、通知が重複しないことを含む。
9. `system.stop(headActor)`（`:46`）で head を止め、3 階層全体が止まる。

子の restart ではなく子の停止・再生成にならない理由は、原典の本番実装が説明している。`actor/src/main/scala/org/apache/pekko/actor/dungeon/FaultHandling.scala:278` では、親を再生成した後、生存している子へ restart を送る。middle の `preRestart` が既定実装（`postStop` と子の停止）を呼ばないため、last は生存したまま restart される。この契約を後続の移植で再現する。

## 原典の前提プロトコール（needs.testkit）

台帳の対象ノードの needs は次の 9 能力である（`ledger/nodes.yaml:4659`。G0a〜G0f で実装済み）。

| 提供元 | 能力 |
|---|---|
| G0b | `extends-pekko-spec`, `pekko-spec`, `test-actor`, `implicit-sender`, `default-timeout`, `expect-msg` |
| G0c | `expect-no-message` |
| G0d | `event-filter` |
| G0f | `within` |

`needs.premise_flags` は空である（`ledger/nodes.yaml:4660`）。能力台帳 `ledger/capabilities.yaml` の変更は要らない。

## 仕事は 1 段

1. **移植**：対象 1 ノードを Pekko v1.7.1 から移植して green にする（ワークフロー `fraktor-port-node`。親は `fraktor-porting-ng`）

## 対象ノード（明示列挙、1 件）

`goal_complete_gate.py` は、この節の最初の表の `id` 列を読み、`goal` の列が `G0Issue96` の台帳ノードの集合と一致することを確かめる。したがって、表はこの位置に置き、`id` 列に完全なノード ID をバッククォートで 1 件だけ記載する。このタグはこの準備タスクで適用済みである。

| id | kind | ケース数（概数） | needs.testkit |
|:---|:---|:---|:---|
| `actor-tests/src/test/scala/org/apache/pekko/actor/SupervisorTreeSpec.scala` | test | 1 | extends-pekko-spec, pekko-spec, test-actor, implicit-sender, default-timeout, expect-msg, expect-no-message, event-filter, within |

ノードの増減は人間だけが行う。ケース数は台帳の概数であり、実数は run が数えて台帳へ記録する（G0d 以降のゴールと同じ進め方）。

## 置き場所

- 本番の actor 処理は `crates/fraktor-actor/src` に置く（gap が実証された場合だけ）
- 統合テストは `crates/fraktor-actor/tests/supervisor_tree_spec.rs` に置く（1 ノード 1 ファイル。actor を検証するテストを actor crate の `tests/` に置く既存の規則に従う。同じ crate には監督の既存例 `crates/fraktor-actor/tests/supervisor_misc_spec.rs` と `crates/fraktor-actor/tests/actor_life_cycle_spec.rs` がある）
- testkit 側の補助が要る場合は `crates/fraktor-testkit/src` に置く。ただし、このゴールに必要な 9 能力は実装済みであり、testkit の追加を前提にしない

## write_scope

- `crates/fraktor-actor/tests/supervisor_tree_spec.rs`（新規、統合テスト）
- `crates/fraktor-actor/src/**`（本番の gap が後述の red で実証された場合だけ）
- `crates/fraktor-testkit/**`（必要な場合だけ。上の 9 能力の既存実装で足りないと red で判明した場合に限る）
- ワークスペース直下の `Cargo.toml` と `Cargo.lock`
- `ledger/nodes.yaml`。対象ノードの `cases` と `status`、`rust.crate`、`rust.test_file` の欄だけ

範囲外の変更が要るときは、止めて報告する。

## 本番コードの gap は「実証された red」でのみ埋める

このゴールは、`actor-tests` の actor を検証するテストを 1 件だけ移植する。本番の失敗経路は、既存の G0a〜G0f の基盤に依存する。2026-11-22 の事前確認では、`crates/fraktor-actor/src` の既存テストコード（`crates/fraktor-actor/src/kernel/actor_system/actor_system_value_test.rs:1912` の「既定の `pre_restart` が子を停めてから親の `post_stop` を呼ぶ」や `:1935` の `Kill` による restart の例）から、既定の `pre_restart` が子を停めること、`Kill` が `ActorKilledException` として restart に至ること、`Actor` トレイトの `pre_restart` を上書きできることが実装済みと分かる。ただし、対象ファイルの主たる経路（middle の `preRestart` 上書きで子を残した 3 階層 + 通知順 + 2 秒の無通知 + 停止）が green になるかは、実際の統合テストを書いて red を実証するまで確定しない。

- このゴール完了のための移植 run では、まず `crates/fraktor-actor/tests/supervisor_tree_spec.rs` に原典どおりのケースを移植し、`cargo test -p fraktor-actor --test supervisor_tree_spec` を実行する。
- red（コンパイルエラーまたはアサーション失敗）が出た場合だけ、その失敗が原典の契約に直結する本番の actor 処理を `crates/fraktor-actor/src` に追加する。実証されない本番変更は行わない。
- 最初から成功した場合（`already_green`）は下記「already_green の扱い」に従う。`already_green` と判定できない `unexpected_pass` のときは red を捏造せず停止して報告する（このゴールは green へ遷移させず終えない）

## already_green の扱い（committed origin/main の経路）

原典のこのノードは、単一の名前付きブロックの下にケース 1 件を持つ。`ledger/tools/verify_red.py:94` と `:98` の `count_original_cases` は、Scala のケース書式と名前付きブロックの数え方を突き合わせ、**全件を説明できないと `None` を返す**。対象原典は `"In a 3 levels deep supervisor tree (linked in the constructor) we" must { ... }` という外側の名前付きブロックがあり、ケース書式の一致は 1 件、名前付きブロックの一致は 2 件になる（2026-11-22 に、`verify_red.py` の `SCALA_CASE` と `SCALA_NAMED_PREFIX` と同じ正規表現を原典へ当てて確認した）。よって `count_original_cases` は `None` を返し、初回から成功した場合は `already_green` ではなく `unexpected_pass` に分類される（`verify_red.py` の docstring のとおり。unexpected_pass は `port-test` へ差し戻す）。

このため、このゴールの文書は、committed origin/main の `fraktor-port-node` の経路を基準にする。`git show HEAD:.takt/workflows/fraktor-port-node.yaml` で確認した内容は次のとおりである。

- `verify_red` の分類遷移に `already_green → verify_green` がある（`:186–187`）。`already_green` のときは `verify_green` と `equivalence-review` を通過した後にだけ `record-already-green` が台帳へ green を永続化する（`:252–305`。`record_already_green.py` は `classification == already_green` のときだけ `status`/`cases`/`triage.reason` を書く）。
- 最初から通ってしまい、`unexpected_pass` に分類された場合は、`port-test` へ戻る。真の `already_green` と判定できないので、green を台帳へ書かない。

したがって、このゴールの run は、初回成功をそのまま green の証拠にしない。次のように扱う。

1. `verify_red` が `already_green` を返した場合（含まれる原典ケース数と承認済み除外数の和が通過数に一致し、対象テストが全件成功する場合だけ）、committed origin/main の経路に従い `verify_green` → `equivalence-review` → `record-already-green` で green を永続化する。失敗を加工して red を作らない。
2. `unexpected_pass` に分類された場合（このファイルでは、上記の数え方の制約で `already_green` に上がらない）、失敗の加工をせず、`port-test` からの差し戻しを、原典との等価性、実ケース数、`count_original_cases` が `None` を返したこと、`cargo test` の生の結果を添えて報告する。green を書かない。扱えない場合は、台帳を書き換えずに停止して報告する。

**条件付きのツール制約として報告する事項**: 実装時に、`already_green` 経路とは別に「保守的な must-block ケース数カウンタ」がある場合、それは別のツール側の制約であり、この文書では前提の成果を発明しない。実際に run でその制約に当たったら、`ledger/tools/` を変更せず、制約の内容と発生箇所を報告する。

`.takt/workflows/fraktor-port-node.yaml` のうち作業ツリーに見えている TAKT ランタイムのオーバーレイ（dirty な canonical から複製されたもの）は、この判断の根拠にしない。判断の根拠は committed origin/main の `git show HEAD:` の内容である。

## 完了条件（機械判定できる形）

- AC1: `cargo test -p fraktor-actor --test supervisor_tree_spec` が通る（移植した 1 ケースが green）
- AC2: `cargo test -p fraktor-actor` が通る（同 crate の既存テストに退行がない）
- AC3: `cargo test -p fraktor-testkit` が通る（G0a〜G0f の testkit に退行がない）
- AC4: `cargo clippy --workspace --all-targets -- -D warnings` が通る
- AC5: `cargo fmt --check` が通る
- AC6: `node scripts/aislop-check-bootstrap.mjs check`（`npm run check:aislop` と同じ）が通る
- AC7: `cargo run --quiet --release --locked --manifest-path tools/syn-check/Cargo.toml -- --format json crates/fraktor-actor` が違反 0 で終わる
- AC8: `uv run --with pyyaml python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal G0Issue96` が 0 を返す（対象ノードが green）
- AC9: 同じ rollup を `--goal G0a` から `--goal G0f` で実行し、すべて 0 を返す（G0a〜G0f の退行なし）
- AC10: `uv run --with pyyaml python3 ledger/tools/check_needs.py --ledger ledger/nodes.yaml --capabilities ledger/capabilities.yaml --goal G0Issue96` が 0 を返す
- AC11: 台帳の差分が、対象ノードの `cases`/`status`/`rust.crate`/`rust.test_file` の欄に限られ、`cases.total` が実数で記録されている
- AC12: 成果物は単一の merge-ready な PR（ドラフトではなく、レビュー可能な完成状態）として提出し、次の必須の GitHub コンテキストがすべて green であること
  - `Fraktor CI Gate`（`.github/workflows/main-pr-ci.yml` の `gate` ジョブ。`ci.yml` を `needs` し、CI の成否と check-result を必須にする）
  - `Check unresolved comments`（`.github/workflows/review-thread-resolution.yml` が公開する review-thread の解決状態。`required_context: Check unresolved comments`）
- AC13: このゴールは 1 ノードの移植を 1 つの PR にまとめる。後続タスクの登録・開始、`G1a` への着手、CI と `.takt` workflow の変更は、その PR に含めない

## 必須ゲート（run 後の人間の確認コマンド）

```sh
cargo test -p fraktor-actor --test supervisor_tree_spec
cargo test -p fraktor-actor
cargo test -p fraktor-testkit
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
node scripts/aislop-check-bootstrap.mjs check
cargo run --quiet --release --locked --manifest-path tools/syn-check/Cargo.toml -- --format json crates/fraktor-actor
uv run --with pyyaml python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal G0Issue96
for g in G0a G0b G0c G0d G0e G0f; do uv run --with pyyaml python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal $g; done
uv run --with pyyaml python3 ledger/tools/check_needs.py --ledger ledger/nodes.yaml --capabilities ledger/capabilities.yaml --goal G0Issue96
```

`verify_green.py` の機械ゲート（`cargo test -p <crate>`、`clippy`、`fmt`、syn-checker、aislop）と、上の AC は同じ対象を判定する（`ledger/tools/verify_green.py:238` 以降）。

## 完了関門（committed origin/main の `goal_complete_gate.py`）との整合

committed origin/main の親ワークフロー `fraktor-porting-ng` は、`ledger/tools/goal_complete_gate.py` でゴール完了を機械判定する。判定条件は次の 3 つであり、この文書はそれに合わせた形にしてある。

- ゴール文書の**先頭行だけ**が `^# ゴール (G0[a-zA-Z0-9]+)：` に一致すること（`goal_complete_gate.py:34`、`:84`）。先頭行は `# ゴール G0Issue96：SupervisorTreeSpec` である。
- `## 対象ノード` 節の**最初の表**の `id` 列から、バッククォート区切りの完全なノード ID を読むこと（`:95`、`:106`、`:135`）。表はこの節の先頭に置き、`id` 列にバッククォートで 1 件だけ記載する。
- その ID 集合が、台帳で `goal == G0Issue96` のノード集合と一致すること（`:146`、`:197`）。その後に `rollup.py --goal G0Issue96` の終了コードを返す（`:202`）。

専用 ID の `G0Issue96` を台帳の `goal` に付けることで、この関門と既存の `rollup.py`／`check_needs.py` を変更せずに通せる。2026-11-22 に、`goal_complete_gate.py` の関数（`goal_id_from_document`、`table_ids_from_document`、`ledger_goal_ids`）をこの文書と台帳へ直接適用し、ID が `G0Issue96`、文書側と台帳側の ID 集合がともに `actor-tests/src/test/scala/org/apache/pekko/actor/SupervisorTreeSpec.scala` 1 件で一致することを確認した。合成した context で関門スクリプトを端から端まで実行すると、`{"exit_code": 1, "goal": "G0Issue96", ...}`（rollup の未完了終了コード）を返した。green 化後は同じ経路で `exit_code` が 0 になる見込みである。

## G0a〜G0f の回帰

このゴールは基盤ゴールではないが、対象テストは G0a〜G0f の基盤（testkit の `within`、`expect-no-message`、`event-filter`、`implicit-sender`、`default-timeout`、`expect-msg` と、actor の監督・`Kill`・`pre_restart`）に依存する。したがって、AC9 として `rollup.py --goal G0a` から `--goal G0f` がすべて 0 を返すことを確認する。2026-11-22 の事前確認では、6 ゴールとも exit 0 だった。加えて、`cargo test -p fraktor-testkit` と `cargo test -p fraktor-actor` で testkit と actor の退行を確認する。

## やらないこと（このゴールの PR に含めない）

- 台帳の他ノードの変更、ノードの追加・削除、`kind` の変更、`do_not_port` への変更、ケースの除外
- `ledger/tools/` の変更、`already_green` やケース数カウンタのツール修正
- CI と `.takt` workflow の変更、`G1a` への着手
- 後続タスクの登録・開始

## この準備タスクの境界（完了を止める位置）

この準備タスクは、上の「対象ノード」と台帳の `goal: G0Issue96` のタグだけを行って止まる。Rust テスト作成、本番コードの編集、`status` の `green` 化、CI・workflow 変更、後続タスクの登録・開始、PR 公開、merge、git add・commit・push は、この準備タスクでは行わない（git のステージング・コミット・プッシュは TAKT のタスク後処理に委ねる）。これらの作業は、この文書が定義するゴール完了（上記「完了条件」と「必須ゲート」を満たす単一の merge-ready PR）を担う後続 run が行う。

## 前提と残るリスク

- 対象は 1 ケースの小さなテストだが、100% の原典等価性（通知順、2 秒の無通知、`EventFilter` の発生数 1）を満たす必要がある。テストの動作は単線（テストスレッドで 1 シャードを進める）であり、`expect_no_message(2.seconds)` は原典と同様、実時間の待ちを含む。
- 3 階層の真ん中を `Kill` して、自身と子を restart し、head を再起動しない挙動は、既定の `pre_restart` が子を停める実装（`actor_system_value_test.rs:1912` が固定している）と、middle が `pre_restart` を上書きして既定実装を呼ばないことの組み合わせで成立する。この組み合わせが統合テストで通るかは red で確認する。
- 原典のケース数の数え方（外側の名前付きブロックがあるため `count_original_cases` が `None` を返す）は、初回成功時に `already_green` でなく `unexpected_pass` になる制約を生む。この制約は上記「already_green の扱い」で明記した。ツールは変更しない。
- 作業ツリー内の `.takt/` は TAKT ランタイムのオーバーレイであり、committed origin/main の workflow と一致しないことがある。この文書の判断は committed origin/main（`git show HEAD:`）の内容に基づく。

## 台帳のタグ付け（この準備タスクで適用済み）

1. 対象ノードの `goal: null` を `goal: G0Issue96` に書き換える（台帳の差分は 1 行だけ。`ledger/nodes.yaml:4672`）。この準備タスクで適用済みである。
2. `status: unported` は変更しない（`:4654`）。
3. 2026-11-22 の事前確認では、台帳のコピーで前処理を行うと `rollup.py --goal G0Issue96` は「対象 1 件、unported 1」で exit 1 になり（未完了なので期待どおり）、`rollup.py --goal G0a`〜`G0f` はすべて exit 0、`check_needs.py --goal G0Issue96` は exit 0 になる見込みである。前処理を実際に適用した後に、上のコマンドで確定させる
