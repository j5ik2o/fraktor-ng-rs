# ゴール G0e：become/unbecome の完成と監督の収束（HotSwapSpec と SupervisorMiscSpec）

状態: 定義済み・未発行（2026-11-14）。着手は、オーナーが 2026-11-14 に「本来の計画を進めて」と指示したことで決まった。
根拠は承認済みグラフモデル提案（`docs/decisions/fable-graph-model-proposal.md`）の 4 章と、G0d のゴール文書（`docs/goals/g0d-eventfilter-and-lifecycle.md`）の原典プロトコール依存分析である。G0d のゴール文書は、HotSwapSpec と SupervisorMiscSpec を「案A の下流として決まる」と記録している。
G0d の実施記録（プルリクエスト #50、マージコミット 3afdfb5）を前提とする。

kind: foundation（基盤ゴール。同時に 1 本だけ走らせる）
requires: [G0d]（プルリクエスト #50 が main に入り、`rollup.py --goal G0a` から `--goal G0d` がすべて exit 0 になること。2026-11-14 に origin/main（6e1569f）相当の台帳で実測済み）

## 原典プロトコール依存分析（2026-11-14、窓口が原典 Pekko v1.7.1 のスペック本体を読んで導出）

- HotSwapSpec（7 ケース）は、振る舞いのスタックの意味論を検証するスペックである。内訳は、コンストラクタでの become（置き換え）、コンストラクタでの複数回の become、スタッキングした become（discardOld = false）と unbecome、複数回のスタッキングと unbecome の連鎖、メッセージ処理中の become によるホットスワップ、unbecome による巻き戻し、再起動時に初期の振る舞いへ戻ることである。最後のケースだけが EventFilter の intercept を使う（HotSwapSpec.scala:129）。become と unbecome の原典は ActorCell.scala の :113、:129、:137 である。
- G0d のライフサイクルのテストは、テスト内のアクター状態にスタックを持たせて become の意味論を再現する方式だったため、本番コードへの context.become の実装は G0d では行われていない（2026-11-14 に作業ツリーで grep して確認。crates/fraktor-actor/src に become の実装は 0 件）。G0e で作るのは、本番コードへの become/unbecome の実装そのものである。内訳は、振る舞いスタックの仕組み（ActorCell と executor の側に置く）、unbecome、コンストラクタで指定した become が最初のメッセージより前に効くこと、再起動時の振る舞いスタックの初期化、スタックの意味論全体の検証である。再開後の最初の段で、振る舞いスタックの設計案を示して規模を見極めてから実装へ進む。
- SupervisorMiscSpec（6 ケース）は、監督と子の管理の収束点である。内訳は次のとおりである。
  1. Supervisor と OneForOneStrategy（maxNrOfRetries = 3、withinTimeRange = 5 秒）配下で、クラッシュした actor とそのディスパッチャを再起動すること。子の生成は ask で受け、Kill で 4 つの子を落とし、postRestart のラッチで再起動を数え、status への応答を確認する。子には withDispatcher で pinned-dispatcher と test-dispatcher を指定する。ディスパッチャの定義はスペックの設定テキストにある。
  2. コンストラクタで名前つきの子を作れること。例外で再起動し、preStart が 2 回呼ばれることを EventFilter の intercept と expectMsg で確認する。
  3. 終了した子と同じ名前の子を再び作れること。watch で Terminated を受け、context.stop で子を止め、同じパスで新しい子を作る。
  4. 生きている子と同じ名前の子は作れないこと。二重の生成が InvalidActorNameException になることを確認する。
  5. 障害処理の戦略の中で子を作り直せること。supervisorStrategy の handleChildTerminated を上書きし、その中で終了した子と同じ名前の子を作る。
  6. 監督戦略の decider が、失敗した子への参照を sender として受け取れること。Kill が ActorKilledException になることを EventFilter で確認する。
- 移植済みの土台は、watch と Terminated、監督による再起動、system.stop、ask の最小形（ask_future.rs の await_result）、testkit の Supervisor テストヘルパー（G0d）、preStart/postStop と既定の preRestart/postRestart（G0d）、Kill と ActorKilledException（G0d）、EventFilter と filterEvents（G0d）である。
- G0e で新しく作るものは、本番コードへの become/unbecome の実装と、コンストラクタ時点の become の適用、子の名前の登録の意味論（二重登録では InvalidActorNameException、終了後は同じ名前を再利用できる）、handleChildTerminated のフック点、decider が失敗した子を sender として受け取ることの保証、Props の withDispatcher に相当するディスパッチャ名の指定である。
- ディスパッチャについては、設定体系（HOCON 相当）の再現範囲が未決定の設計判断であるため、G0e では設定テキストの解釈までは作らない。ディスパッチャ名はプログラム側の指定で受け、暫定の実行基盤のうえでは名前を記録するに留める。PinnedDispatcher のスレッドプールの意味論は再現しない。ケース 1 が確認するのは再起動と応答であり、ディスパッチャのスレッドの違いは確認していない。この扱いは移植の忠実さの判断として、プルリクエスト本文に記録する。
- 処理順は HotSwapSpec を先、SupervisorMiscSpec を後とする。HotSwapSpec は前提がすべてそろっており、SupervisorMiscSpec は新しく作る面が多いため、依存の形で順序が決まる。

## 仕事は 1 段

1. **移植**：対象 2 ノードを Pekko v1.7.1 から移植して green にする（ワークフロー `fraktor-porting-ng`）

## 対象ノード（明示列挙、2 件）

| 処理順 | id | kind | ケース数（概数） | needs.testkit |
|---|---|---|---|---|
| 1 | `actor-tests/src/test/scala/org/apache/pekko/actor/HotSwapSpec.scala` | test | 7 | extends-pekko-spec, pekko-spec, implicit-sender, expect-msg, event-filter（前提フラグなし） |
| 2 | `actor-tests/src/test/scala/org/apache/pekko/actor/SupervisorMiscSpec.scala` | test | 6 | extends-pekko-spec, pekko-spec, test-actor, default-timeout, expect-msg, event-filter（前提フラグなし） |

ノードの増減は人間だけが行う。ケース数は台帳の概数であり、実数は run が数えて台帳へ記録する（G0d の進め方と同じ）。

## 置き場所

- become/unbecome、子の名前の登録、監督戦略のフックの実装は `crates/fraktor-actor/src` に置く
- テストは 2 ノードとも `crates/fraktor-actor/tests/` に置く（1 ノード 1 ファイル。actor-tests は主たる検証対象の actor のテストに置くという G0d の規則に従う）
- testkit 側の補助が要る場合は `crates/fraktor-testkit/src` に置く

## write_scope

- `crates/fraktor-actor/**`
- `crates/fraktor-testkit/**`
- ワークスペース直下の `Cargo.toml` と `Cargo.lock`
- `ledger/nodes.yaml`。対象 2 ノードの欄だけ

範囲外の変更が要るときは、止めて報告する。

## 完了条件（機械判定できる形）

- AC1: `cargo test -p fraktor-actor` が通る
- AC2: `cargo test -p fraktor-testkit` が通る（G0a から G0d の退行がない）
- AC3: `uv run --with pyyaml python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal G0e` が 0 を返す
- AC4: 同じ rollup を `--goal G0a`、`--goal G0b`、`--goal G0c`、`--goal G0d` で実行し、すべて 0 を返す（退行なし）
- AC5: `uv run --with pyyaml python3 ledger/tools/check_needs.py --ledger ledger/nodes.yaml --capabilities ledger/capabilities.yaml --goal G0e` が 0 を返す
- AC6: 発行前処理のコミットより後の台帳の差分が、対象 2 ノードの欄に限られる
- AC7: 2 ノードの `cases.total` が実数で記録され、`rust.crate` と `rust.test_file` も記録されている
- AC8: `cargo clippy --workspace --all-targets -- -D warnings` と `cargo fmt --all --check` が通る
- AC9: 成果はドラフト PR と 6 項目の完了報告で提出し、そこで止まる。次のゴールには着手しない

## やらないこと

- 設定体系（HOCON 相当）の再現と、設定テキストからのディスパッチャ定義の読み取り
- ディスパッチャのスレッドプールの意味論（PinnedDispatcher の専用スレッドなど）の再現
- 汎用 EventBusSpec と EventStreamSpec の移植（後続のゴール G0f の見通し）
- mailbox の上限の既定値とあふれ時の意味論（未決定の設計判断。ActorMailboxSpec はその決定後に別のゴールで行う）
- typed、台帳スキーマの変更。ノードを do_not_port にすること、ケースの除外、kind の変更
- `ledger/tools/` の変更
- ドメインプリミティブ対応（プルリクエスト #56 はクローズ済み。対応は保留であり、このゴールでは扱わない）
- 旧 fraktor-rs からのコードの持ち込み

## provider

基盤ゴールなので強いモデルを使う（論点 19）。実効並行は 1 本。assignment は発行時の既定に従う。main 上の既定は interim-ollama である。2026-11-14 に Ollama Cloud の月間利用上限へ達したため、主チェックアウトの作業ツリーでは interim-dgx（全役割を DGX Spark の DeepSeek V4.1 Flash へ寄せる割り当て）へ切り替えて未コミットで運用しており、利用上限が 2026-11-30 に回復したら既定の扱いを見直す。

## 発行時の台帳前処理（窓口がゴール文書のプルリクエストとあわせて行う）

1. 対象 2 ノードの `goal: null` を `goal: G0e` に書き換える（台帳の差分は 2 行だけ）
2. `ledger/capabilities.yaml` の変更は要らない。対象 2 ノードの needs は、G0b が提供する能力と G0d が提供する event-filter だけで閉じている
3. 2026-11-14 に origin/main（6e1569f）相当の台帳の一時コピーで確認済みである。`rollup.py --goal G0e` は「ノード 2 件、status 内訳 {'unported': 2}」で exit 1 になり、`check_needs.py --goal G0e` は exit 0 になる。あわせて `--goal G0a` から `--goal G0d` の rollup はすべて exit 0 である

## 前提と残るリスク

- run の作業ツリーへの Pekko 原典の配置は恒久策が入っている（プルリクエスト #28。`tools/pekko-source/prepare_pekko_source.sh` が作業ツリー直下の `pekko/` に v1.7.1〔commit 9a54d12〕を用意する）
- 対象は 2 ノード 13 ケース（概数）で、G0d（2 ノード 16 ケース）より小さい
- SupervisorMiscSpec のケース 1 は CountDownLatch を使う。テスト側の補助として run が用意する（G0d の 7 番目のケースと同じ扱い）
- become/unbecome は本番コードに存在しない（2026-11-14 に確認）。G0e では本番コードへの実装を最初から行う

## run の後に人間が確認するコマンド

```sh
cargo test -p fraktor-actor
cargo test -p fraktor-testkit
for g in G0a G0b G0c G0d G0e; do uv run --with pyyaml python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal $g; done
uv run --with pyyaml python3 ledger/tools/check_needs.py --ledger ledger/nodes.yaml --capabilities ledger/capabilities.yaml --goal G0e
```
