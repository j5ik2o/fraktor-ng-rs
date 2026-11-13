# ゴール G0d：EventFilter と最初の利用側（actor のライフサイクル）

状態: 定義済み・未発行（2026-11-13）。着手はオーナーの判断を待つ。
根拠は承認済みグラフモデル提案（`docs/decisions/fable-graph-model-proposal.md`）の 4 章と論点 12。
G0c の実施記録（PR #36、マージコミット 8c3c63b）を前提とする。

kind: foundation（基盤ゴール。同時に 1 本だけ走らせる）
requires: [G0c]（PR #36 が main に入り、`rollup.py --goal G0a`〜`G0c` がすべて exit 0 になること。
2026-11-13 に origin/main 21346a3 相当の台帳で実測済み）

## 原典プロトコール依存分析（2026-11-13、オーナー指示により集計と別に原典から導出）

対象スペック群（案A/B/C と後続）を原典のプロトコール定義から読み直した。順序の大半は依存で
決まる。依存では決まらないのは 3 ライン（イベント+フック収束 / ask・pipeTo 完成 / mailbox）の
入口の順だけである。

- 移植済みの土台: tell/sender、watch + Terminated（executor_value.rs:183-189）、監督の再起動、
  system.stop、ask の最小形（ask_future.rs:99 await_result、タイムアウトは panic。
  AskTimeoutException の値化と PromiseActorRef の露出はなし）、pipe_to の成功形（同 :125）、
  testkit の探針一式。pre_start/post_stop/pre_restart、become、PoisonPill、LogEvent、EventBus/
  EventStream、EventFilter、Status.Failure、ActorSelection、Kill、mailbox 上限、within ブロックは
  全 crates に不在（検索済み）
- イベントラインの定義順: LogEvent（Logging.scala:724、Error :811、NoCause :840、Warning :864）→
  EventFilter 述語（TestEventListener.scala:88、matches :97、object :176 の apply[ClassTag] :193/
  error :208/warning :234）。この層に TestEventListenerSpec が載る（EventStream 不要、述語を
  直接検証）。汎用 EventBus（EventBus.scala:33、ActorEventBus :68）→ LoggingBus（Logging.scala:44）
  → EventStream（EventStream.scala:34、Unsubscriber は watch ベース :60/:86/:94、起動は
  ActorSystem.scala:1070）→ TestEventListener（TestEventListener.scala:557）と
  intercept/filterEvents/filterException（同 :125-:140、testkit/package.scala:28/:50/:53）
- intercept は Mute/UnMute を eventStream に publish して成立し、ロガー登録は Error/Warning/
  Info/Debug の各クラスへの完全一致購読（Logging.scala:206 addLogger、購読 :229）。よって案A の
  最小ストリームは SubchannelClassification を要らない。SubchannelClassification
  （EventBus.scala:136）は EventStreamSpec のクラス階層購読が初めて必要にする
- フックライン（イベントと独立）: preStart/postStop/preRestart/postRestart（Actor.scala:599/:611/
  :625/:643、既定の委譲含む）、become/unbecome（ActorCell.scala:113/:129/:137）、PoisonPill
  （Actor.scala:52）、Kill/ActorKilledException（同 :67/:177）
- ActorLifeCycleSpec は両ラインの収束点（filterException[ActorKilledException] :50/:85、
  EventFilter.intercept :137/:158、become(discardOld=false) :145、PoisonPill :138、Kill による
  再起動、Supervisor.scala:20 のテストヘルパー）。TestEventListenerSpec → ActorLifeCycleSpec の
  順は依存で決まる。HotSwapSpec（7 ケース、intercept 1 ケース :129）と SupervisorMiscSpec
  （6 ケース、フック + Kill + filterEvents/intercept ×2）は案A の下流として決まる
- AskSpec（19）と PipeToSpec（10）は ask/pipeTo 完成のスペックで、3 ラインと独立。AskSpec の
  うち 5 ケース（:131/:153/:169/:190/:211）は actorSelection + DeadLetter 観察のために
  EventStream.subscribe に接するのみ。PipeToSpec の失敗配信は Status.Failure（Actor.scala:325）
- ActorMailboxSpec（33）は mailbox 家族（Mailbox.scala:362/:653/:705/:1046、Mailboxes.scala の
  設定解決）のスペックで全ラインから独立。着手条件として上限・あふれの意味論の決定を待つ

## 仕事は 1 段

1. **移植**：対象 2 ノードを Pekko v1.7.1 から移植して green にする（ワークフロー `fraktor-porting-ng`）

（G0c と違い、改名の段はない。）

## 対象ノード（明示列挙、2 件）

| 処理順 | id | kind | ケース数（実数） | needs.testkit |
|---|---|---|---|---|
| 1 | `testkit/src/test/scala/org/apache/pekko/testkit/TestEventListenerSpec.scala` | test | 9 | extends-pekko-spec, pekko-spec, implicit-sender, event-filter（前提フラグなし） |
| 2 | `actor-tests/src/test/scala/org/apache/pekko/actor/ActorLifeCycleSpec.scala` | test | 7 | extends-pekko-spec, pekko-spec, test-actor, implicit-sender, default-timeout, expect-msg, expect-no-message, expect-terminated, event-filter（前提フラグ: thread-sleep） |

ノードの増減は人間だけが行う。

## 依存の閉じ方

- 2026-11-13 に、G0c 取り込み後の台帳（origin/main 21346a3 相当）の一時コピーで 2 ノードに
  `goal: G0d` を付け、`capabilities.yaml` で event-filter の `provided_by` を G0d にすると、
  `check_needs.py --goal G0d` は exit 0（needs 閉性 OK）になる（2026-11-13 に確認）。
  G0a〜G0c が提供する能力は、G0c が main で完了していることを前提とする。
- 作らせるもの（提案書 4 章の形）:
  - ログイベントの模型: Logging の Error/Warning（原因・発信元・メッセージの区別）。
    TestEventListenerSpec がイベントを直接構築して EventFilter の述語に通す形で検証する
  - EventFilter: classic な `error()`/`warning()` と、型指定 `EventFilter[T]`。cause の一致、
    NoCause、メッセージ一致、`occurrences` を含む `intercept`、`filterException`
  - TestEventListener と最小のイベントストリーム: `intercept` が成立する最低限の購読・配信。
    汎用 EventBusSpec・EventStreamSpec の移植は後続ゴールに回し、このゴールが作るのは
    intercept に要る最小の範囲
  - actor のライフサイクル: preStart/postStop、既定の preRestart/postRestart
    （postStop/preStart への委譲）、再起動されなかった場合の呼び出しの有無。
    原典の preRestart は reason と message を取る（Actor.scala:625）が、移植の post_restart は
    失敗を値として渡さない（actor_trait.rs の注記）。reason の伝え方は run の設計判断
  - `context.become`（`discardOld = false` のスタッキングを含む）と、再起動時の振る舞いスタックの初期化
  - testkit 側の Supervisor テストヘルパー（原典では actor-tests のテスト用補助。子の生成を ask で受ける）
  - PoisonPill と `system.stop`、Kill と ActorKilledException（Actor.scala:52/:67/:177）。
    ActorLifeCycleSpec の再起動は Kill で起こし、filterException[ActorKilledException] で静かに進める
- ActorLifeCycleSpec の "clear the behavior stack upon restart" は、become のスタッキングと
  再起動時の初期化の両方を要する。最小の実装で満たすか別の仕組みが要るかは、
  run の設計段階で決めて根拠とともに報告する。
- 実行基盤は論点 14 の暫定方針（自前 executor、シャード内はローカルキュー、境界だけチャネル、
  境界は差し替え可能）を引き続き守る。

## 置き場所

- EventFilter・TestEventListener とログイベントの模型は `crates/fraktor-testkit/src` に置く
  （TestKit の能力のため）
- 最小のイベントストリーム、ライフサイクル、become の実装は `crates/fraktor-actor/src` に置く
- TestEventListenerSpec のテストは `crates/fraktor-testkit/tests/` に置く（1 ノード 1 ファイル）
- ActorLifeCycleSpec のテストは `crates/fraktor-actor/tests/` に置く（案C 規則 3。actor-tests は
  主たる検証対象の actor のテストに置き、fraktor-testkit は dev-dependency で使う）

## write_scope

- `crates/fraktor-testkit/**`
- `crates/fraktor-actor/**`
- ワークスペース直下の `Cargo.toml` と `Cargo.lock`
- `ledger/nodes.yaml`。対象 2 ノードの欄だけ

範囲外の変更が要るときは、止めて報告する。

## 完了条件（機械判定できる形）

- AC1: `cargo test -p fraktor-testkit` が通る
- AC2: `cargo test -p fraktor-actor` が通る（ActorLifeCycleSpec と、G0a〜G0c の退行がない）
- AC3: `uv run --with pyyaml python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal G0d` が 0 を返す
- AC4: 同じ rollup を `--goal G0a`、`--goal G0b`、`--goal G0c` で実行し、すべて 0 を返す（退行なし）
- AC5: `uv run --with pyyaml python3 ledger/tools/check_needs.py --ledger ledger/nodes.yaml --capabilities ledger/capabilities.yaml --goal G0d` が 0 を返す
- AC6: 発行前処理のコミットより後の台帳の差分が、対象 2 ノードの欄に限られる
- AC7: 2 ノードの `cases.total` が実数で記録され、`rust.crate` と `rust.test_file` も記録されている
- AC8: `cargo clippy --workspace --all-targets -- -D warnings` と `cargo fmt --all --check` が通る
- AC9: 成果はドラフト PR と 6 項目の完了報告で提出し、そこで止まる。次のゴールには着手しない

## やらないこと

- 層 2 の残り（TestLatch、awaitCond、receiveWhile/receiveN/receiveOne、within、TestActorRef、
  LogCapturing など）。対象 2 ノードに必要な分は除く
- 汎用 EventBusSpec・EventStreamSpec の移植（最小のイベントストリームの固めは後続ゴール）
- typed、設定体系、本格的なログ出力（ファイル・コンソールへの出力）。ログイベントの模型と
  intercept に要る範囲だけ
- 台帳スキーマの変更。ノードを do_not_port にすること、ケースの除外、kind の変更
- `ledger/tools/` の変更
- mailbox の上限の既定値とあふれ時の意味論には触れない（未決定の設計判断。
  ActorMailboxSpec の移植は、その決定が済んでから別のゴールで行う）
- 旧 fraktor-rs からのコードの持ち込み

## provider

基盤ゴールなのでクラウド（論点 19）。実効並行は 1 本。assignment は発行時に窓口が選ぶ
（2026-11-13 現在の既定は interim-ollama）。

## 発行時の台帳前処理（manager がゴール文書の PR とあわせて行う）

G0c（PR #36）が main に入った後に行う（2026-11-13 時点で済んでいる）。

1. 対象 2 ノードの `goal: null` を `goal: G0d` に書き換える（台帳の差分は 2 行だけ）
2. `ledger/capabilities.yaml` の event-filter の `provided_by: null` を `provided_by: G0d` に書き換える
3. 次のコマンドを実行して確かめる
   - `rollup.py --goal G0d` が「ノード 2 件、status 内訳 {'unported': 2}」で exit 1 になる
   - `check_needs.py --goal G0d` が exit 0 になる

## 前提と残るリスク

- run の作業ツリーへの Pekko 原典の配置は恒久策が入った（PR #28。workflow_config.runtime.prepare が
  `tools/pekko-source/prepare_pekko_source.sh` を init と各ステップの前に実行し、作業ツリー直下の
  `pekko/` に v1.7.1〔commit 9a54d12〕を用意する）。run が原典を読めずに止まった場合は、
  manager が同じ仕組みで再開させる
- 対象は 2 ノード 16 ケースで、G0c（2 ノード 18 ケース）と同規模。`max_steps: 80` は
  G0c で完走の実績がある
- EventFilter の classic な `error()`/`warning()` と型指定 `EventFilter[T]` の違いは、
  TestEventListenerSpec の 9 ケースが直接検証する。`intercept` の意味論
  （occurrences の数え方、不一致時の扱い）は、ActorLifeCycleSpec の利用側ケースが最初に動かす
- ログイベントの模型（Error/Warning の表現、発信元の表現）に既存の設計決定はない。
  run の設計判断に任せ、根拠とともに報告させる
- ActorLifeCycleSpec の 7 番目のケース（終了後の context）は CountDownLatch と after/pipeTo を
  使う。テスト側の補助として run が用意する
- mailbox の上限・あふれの意味論はこのゴールの範囲外（未決定の設計判断）

## run の後に人間が確認するコマンド

```sh
cargo test -p fraktor-testkit
cargo test -p fraktor-actor
for g in G0a G0b G0c G0d; do uv run --with pyyaml python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal $g; done
uv run --with pyyaml python3 ledger/tools/check_needs.py --ledger ledger/nodes.yaml --capabilities ledger/capabilities.yaml --goal G0d
git diff --stat ledger/nodes.yaml
```
