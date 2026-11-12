# ゴール G0c：TestProbe と最初の利用側、製品表面の改名

状態: 定義済み・未発行（2026-11-13）。着手はオーナーの判断を待つ。
根拠は承認済みグラフモデル提案（`docs/decisions/fable-graph-model-proposal.md`）の 4 章と論点 12。
改名は 2026-11-13 に窓口（オーナー）が指示し、判断 A〜D を承認した（下記「承認済みの判断」）。

kind: foundation（基盤ゴール。同時に 1 本だけ走らせる）
requires: [G0b]（PR #21 が main に入り、`rollup.py --goal G0b` が exit 0 になること）

## 仕事は 2 段

1. **移植**：対象 2 ノードを Pekko v1.7.1 から移植して green にする（ワークフロー `pekko-porting-ng`）
2. **改名**：移植が終わった後の最後のタスクで、製品表面とハーネスの移植元の名前を fraktor に改める
   （ワークフロー `default`）

## 対象ノード（明示列挙、2 件）

| 処理順 | id | kind | ケース数（近似） | needs.testkit |
|---|---|---|---|---|
| 1 | `testkit/src/test/scala/org/apache/pekko/testkit/TestProbeSpec.scala` | test | 16 | extends-pekko-spec, pekko-spec, test-actor, default-timeout, expect-msg, expect-no-message, test-probe, await-assert, fish-for-message, ignore-msg, calling-thread-dispatcher, set-auto-pilot（前提フラグ: eventually） |
| 2 | `actor-tests/src/test/scala/org/apache/pekko/actor/ForwardActorSpec.scala` | test | 2 | extends-pekko-spec, pekko-spec, test-actor, expect-msg |

ノードの増減は人間だけが行う。

## 依存の閉じ方

- G0b の成果（8645638）を含む台帳の一時コピーで 2 ノードに `goal: G0c` を付けると、
  `check_needs.py --goal G0c` は exit 0（needs 閉性 OK）になる（2026-11-13 に確認）。
  G0b が提供する能力は、G0b が main で完了していることを前提とする。
- 作らせるもの（提案書 4 章）:
  - TestProbe、expectNoMessage、ignoreMsg、fishForMessage、awaitAssert、AutoPilot（setAutoPilot）
  - ask と pipeTo
  - 子アクターの生成（childActorOf）と、監督戦略による再起動、watch と Terminated
- TestProbeSpec の最初のケースは、ask の直後に `expectMsg(0.millis, "hello")` が通ることを求める
  （原典のコメントは CallingThreadDispatcher を理由に挙げている）。G0b の expectMsg は、
  テストスレッドが executor を駆動して待つ形である。この形で満たすか、別の仕組みが要るかは、
  run の設計段階で決めて根拠とともに報告する。
- 実行基盤は論点 14 の暫定方針（自前 executor、シャード内はローカルキュー、境界だけチャネル、
  境界は差し替え可能）を引き続き守る。
- Pekko の送信順序の保証（同じ送信者から同じ受信者へのメッセージは送信順に届く）を守る。
  G0b の PR #21 で CodeRabbit が、`ActorContext::tell` と `ActorRef::tell` を混ぜると順序が
  入れ替わりうると指摘した。PR #21 のマージ時に対応されなかった場合は、G0c の中で直す。

## 承認済みの判断（2026-11-13）

- **A. 改名の時点**: 改名は G0c の移植が終わった後の、最後のタスクで行う。manager はタスクを積むとき
  main の作業ツリーにある `.takt/config.yaml` でワークフロー名を照合するので、ゴールの途中で
  ワークフロー名を変えると、新しい名前のタスクを main への反映まで積めないためである。
- **B. 台帳の例外**: PekkoSpec ノード（G0b のノード）の `rust.test_file` 欄だけを、G0c の
  write_scope に例外として加える。改名でファイルのパスが変わるためである。
- **C. 改名の範囲**: `.takt/facets/` の instruction ファイルと knowledge のファイル名も対象に含める。
  ドキュメントコメントの「Mirrors `org.apache.pekko...`」のような原典の対応を示す記述は、事実の
  記述なので対象外とする。
- **D. 次のゴール**: 次のゴールは G0c とし、改名は G0c に含める。

## 改名の範囲（最後のタスク）

### 対象

- **fixture 型と派生名**（`crates/fraktor-testkit`）
  - 型 `PekkoSpec` → `FraktorSpec`、`PekkoSpecHooks` → `FraktorSpecHooks`
  - モジュール `pekkospec` → `fraktorspec`
  - ファイル `pekko_spec.rs`、`pekko_spec_hooks.rs`、`pekko_spec_test.rs` → `fraktor_spec*.rs`
  - これらを参照する src とテスト（`implicit_sender_value.rs`、`lib.rs`、tests/ の各ファイル）。
    G0c の移植で追加したファイルも含む
- **設定キーの接頭辞**: `pekko.` → `fraktor.`。たとえば TestKitSettings のドキュメントにある
  `pekko.test.default-timeout` などである。現時点で設定体系はなく、キーはドキュメントにしか現れない
- **ワークフロー名**: `pekko-porting-ng` → `fraktor-porting-ng`、`pekko-port-node` → `fraktor-port-node`。
  ファイル名、`name`、`call` の参照、`.takt/config.yaml` の `manager.workflows`、
  `.takt/runtime.yaml` のステップキー（`pekko-port-node/classify-red` など）を追随させる
- **規則ファイル**: `.takt/workflows/rules/pekko-coding-rules.md` → `fraktor-coding-rules.md`。
  命名ルール「製品表面（型、モジュール、ファイル、公開 API、設定キー、ワークフローと facet の名前）
  には移植元の名前を使わない」を追記する。原典テストのデータ文字列、台帳のノード ID、原典の対応を示す
  ドキュメントコメントは例外とする
- **facet**: `.takt/facets/instructions/pekko-*.md`（8 件）→ `fraktor-*.md`。
  `.takt/facets/knowledge/pekko-porting-rules.md` → `fraktor-porting-rules.md`。
  ワークフローと output-contracts からの参照も追随させる
- **設計文書の参照**: `docs/decisions/takt-workflow-design.md` の名前の参照

### 対象外

- 原典テストのデータ文字列（`"pekko://..."`、`"PekkoCustomSpec"` など）
- 台帳のノード ID と `ledger/capabilities.yaml` の能力 id・正規表現（原典の Scala を照合するためのもの）
- 原典の対応を示すドキュメントコメント（`Mirrors org.apache.pekko...` など）と、「Pekko 原典」のような原典を指す記述
- 過去のゴール文書（`docs/goals/g0a-pipeline-smoke.md`、`docs/goals/g0b-testkit-seed.md`）。実施の記録として残す
- 作業ツリー直下の `pekko/`（原典の置き場所）

## 置き場所

- TestProbe などの TestKit の能力は `crates/fraktor-testkit/src` に置く
- TestProbeSpec のテストは `crates/fraktor-testkit/tests/` に置く（1 ノード 1 ファイル）
- ForwardActorSpec のテストは `crates/fraktor-actor/tests/` に置く（案C 規則 3。actor-tests は
  主たる検証対象の actor のテストに置き、fraktor-testkit は dev-dependency で使う）
- ask、pipeTo、子アクター、監督、watch の実装は `crates/fraktor-actor` に置く

## write_scope

- 移植の段:
  - `crates/fraktor-testkit/**`
  - `crates/fraktor-actor/**`
  - ワークスペース直下の `Cargo.toml` と `Cargo.lock`
  - `ledger/nodes.yaml`。対象 2 ノードの欄と、PekkoSpec ノードの `rust.test_file` 欄（判断 B の例外）だけ
- 改名の段（上記に加えて）:
  - `.takt/workflows/**`、`.takt/facets/**`、`.takt/config.yaml`、`.takt/runtime.yaml`
  - `docs/decisions/takt-workflow-design.md`

範囲外の変更が要るときは、止めて報告する。

## 完了条件（機械判定できる形）

- AC1: `cargo test -p fraktor-testkit` が通る
- AC2: `cargo test -p fraktor-actor` が通る（ForwardActorSpec と、G0a・G0b の退行がない）
- AC3: `uv run --with pyyaml python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal G0c` が 0 を返す
- AC4: 同じ rollup を `--goal G0a` と `--goal G0b` で実行し、どちらも 0 を返す（退行なし）
- AC5: `uv run --with pyyaml python3 ledger/tools/check_needs.py --ledger ledger/nodes.yaml --capabilities ledger/capabilities.yaml --goal G0c` が 0 を返す
- AC6: 発行前処理のコミットより後の台帳の差分が、対象 2 ノードの欄と PekkoSpec ノードの `rust.test_file` 欄に限られる
- AC7: 2 ノードの `cases.total` が実数で記録され、`rust.crate` と `rust.test_file` も記録されている。
  PekkoSpec ノードの `rust.test_file` は改名後のパスを指す
- AC8: 改名が済んでいる。次の 5 つで確かめる
  - `git ls-files crates .takt | grep -i pekko` が何も返さない
  - `git grep -n -E '\b(struct|trait|enum|mod|type|fn)\s+\w*[Pp]ekko' -- crates` が何も返さない
  - `git grep -n -E '\buse\b.*[Pp]ekko' -- crates` が何も返さない
  - `git grep -n 'pekko\.test\.' -- crates` が何も返さない
  - `git grep -n -E 'pekko-(porting|port-node|coding-rules|classify-red|ledger-update|loop-judge|port-test|review-equivalence|triage|verify-green-loop-judge|verify-red-loop-judge)' -- .takt docs/decisions` が何も返さない
- AC9: `.takt/config.yaml` の `manager.workflows` に `fraktor-porting-ng` があり、ワークフロー
  `fraktor-porting-ng` と、その呼び先の `fraktor-port-node` がどちらも解決できる。
  `fraktor-coding-rules.md` に命名ルールが追記されている
- AC10: ワークフロー内の機械検証と等価性レビュー、`.takt/workflows/rules/` の規則の検査を通っている。
  `cargo clippy --workspace --all-targets -- -D warnings` と `cargo fmt --all --check` が通る
- AC11: 成果はドラフト PR と 6 項目の完了報告で提出し、そこで止まる。次のゴールには着手しない

## やらないこと

- 層 2 以上の基盤（EventFilter、TestLatch、within、awaitCond など）。TestProbeSpec に必要な分は除く
- TestActorRef、TestFSMRef、CallingThreadDispatcher の汎用実装（TestProbeSpec の要求を満たす
  最小限の仕組みは、run の設計判断に任せる）
- typed、HOCON などの設定体系、ログ
- 台帳スキーマの変更。ノードを do_not_port にすること、ケースの除外、kind の変更
- `ledger/tools/` の変更。改名の段より前に `.takt/` を変更すること
- 旧 fraktor-rs からのコードの持ち込み

## provider

基盤ゴールなのでクラウド（論点19）。実効並行は 1 本。codex の使用量上限（2026-11-17 回復見込み）中は
`.takt/runtime.yaml` の assignment `interim-claude` を使う。

## 発行時の台帳前処理（人間が行う）

PR #21 が main に入った後で行う。

1. 対象 2 ノードの `goal: null` を `goal: G0c` に書き換える（台帳の差分は 2 行だけ）
2. 次のコマンドを実行して確かめる
   - `rollup.py --goal G0c` が「ノード 2 件、status 内訳 {'unported': 2}」で exit 1 になる
   - `check_needs.py --goal G0c` が exit 0 になる

## 前提と残るリスク

- run の作業ツリー（`takt-worktrees` 配下）に Pekko の原典を置く恒久策は、まだない。
  恒久策が入るまでは、manager がタスクの作業ツリー直下の `pekko/` に v1.7.1 を用意する
  （G0b と同じ対応）。原典を読めずに triage が止まった場合も、同じ対応で再開する
- TestProbeSpec は 16 ケースあり、G0b の 4 ノード分より重い。`max_steps: 80` に収まるかは未確認
- 改名の段では、ワークフロー自身のファイルを書き換える。そのため、この段は `pekko-porting-ng` では
  なく `default` で走らせる

## run の後に人間が確認するコマンド

```sh
cargo test -p fraktor-testkit
cargo test -p fraktor-actor
for g in G0a G0b G0c; do uv run --with pyyaml python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal $g; done
uv run --with pyyaml python3 ledger/tools/check_needs.py --ledger ledger/nodes.yaml --capabilities ledger/capabilities.yaml --goal G0c
git ls-files crates .takt | grep -i pekko
git diff --stat ledger/nodes.yaml
```
