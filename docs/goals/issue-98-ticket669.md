# ゴール G0issue98：Ticket669Spec

GitHub issue #98（https://github.com/j5ik2o/fraktor-ng-rs/issues/98）の単一ノードの移植記録。`goal_complete_gate.py` が先頭行から専用のゴール ID `G0issue98` を読む。

この準備タスク自体は、ゴール文書の作成と台帳への `goal: G0issue98` のタグ付けだけを行い、そこで止まる（Rust テスト作成、本番編集、`status` の `green` 化、CI・workflow 変更、後続タスクの登録・開始、PR 公開、merge はしない）。一方、この文書が定義するゴールそのものの完了は、後続の `fraktor-porting-ng` run が `status` を `unported` から green へ遷移させ、単一の merge-ready な PR として提出するところまでを指す。以下の完了条件と必須ゲートは、そのゴール完了（単一の merge-ready PR）に対する要求である。

kind: single（ノード 1 件の移植。基盤ゴールの G0a〜G0f とは別系統の単発ゴール）
requires: なし（前提とするテスト基盤は、G0a〜G0f が main に入って green 済みであること）

## 依拠した原典

- Pekko v1.7.1。commit は `9a54d1294c3ac7821bbed3c940769c3d5027d1ff`。
- 原典ファイル: `actor-tests/src/test/scala/org/apache/pekko/actor/Ticket669Spec.scala`（全 77 行。2026-11-22 に固定 commit の全文を取得して確認した）。
- 原典 URL: https://github.com/apache/pekko/blob/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/actor-tests/src/test/scala/org/apache/pekko/actor/Ticket669Spec.scala

## 原典の実ケースと契約

原典は `PekkoSpec` を継承し `BeforeAndAfterAll`・`ImplicitSender`・`DefaultTimeout` を混ぜる（`:28`）。外側の `"A supervised actor with lifecycle PERMANENT" should { ... }` ブロック（`:36`）にケースが 2 件ある。補助オブジェクト `Ticket669Spec` に `Supervised` アクターを定義する（`:64–76`）。

`Supervised` は受け取ったメッセージに対して常に `throw new Exception("test")` する（`:65–67`）。`preRestart(reason, msg)` を上書きし、`sender() ! "failure1"` を送る（`:69–71`）。`postStop()` を上書きし、`sender() ! "failure2"` を送る（`:73–75`）。

実ケース名は次の 2 件である。

1. `be able to reply on failure during preRestart`
2. `be able to reply on failure during postStop`

契約と原典の行の対応を記録する。

| 原典ケース | 原典の位置 | ケース数寄与 |
|:---|:---|:---|
| `be able to reply on failure during preRestart` | `:37–47`（`filterEvents` `:38`、`AllForOneStrategy(5, 10.seconds)` `:40`、`ask` による子取得 `:41`、`supervised.!("test")(testActor)` `:43`、`expectMsg("failure1")` `:44`、`system.stop(supervisor)` `:45`） | 1 |
| `be able to reply on failure during postStop` | `:49–59`（`filterEvents` `:50`、`AllForOneStrategy(maxNrOfRetries = 0)` `:52`、`ask` による子取得 `:53`、`supervised.!("test")(testActor)` `:55`、`expectMsg("failure2")` `:56`、`system.stop(supervisor)` `:57`） | 1 |

各ケースが観測する契約は次のとおり。

1. `preRestart` ケース（`:37–47`）: supervisor を `AllForOneStrategy(maxNrOfRetries = 5, withinTimeRange = 10.seconds)(List(classOf[Exception]))` で生成する（`:40`）。supervisor へ `(supervisor ? Props[Supervised]())` で ask し、子の `ActorRef` を得る（`:41`）。`implicit sender`（`testActor`）を sender として、その子へ `test` を送る（`:43`）。子がメッセージ処理で例外を投げ、自分の `preRestart` が `failure1` を返信する。テストは `expectMsg("failure1")` で返信を観測する（`:44`）。最後に `system.stop(supervisor)` で supervisor を止める（`:45`）。
2. `postStop` ケース（`:49–59`）: supervisor を `AllForOneStrategy(maxNrOfRetries = 0)(List(classOf[Exception]))`（上限 0・`withinTimeRange` は既定）で生成する（`:52`）。同じ ask で子を得て（`:53`）、同じ送信方法で子に `test` を送る（`:55`）。上限 0 のため、例外後に子は再起動されず停止し、`postStop` が `failure2` を返信する。テストは `expectMsg("failure2")` で観測する（`:56`）。最後に `system.stop(supervisor)` で止める（`:57`）。

両ケースとも、生成から停止までの全体を `filterEvents(EventFilter[Exception]("test", occurrences = 1)) { ... }` で囲む（`:38`・`:50`）。ブロック終了時に `Exception("test")` のログがちょうど 1 件だったことを検査する。返信の `expectMsg` だけを取り出した弱いテストへ読み替えない。sender は `implicit sender`（`testActor`）から渡し、`supervised.!("test")(testActor)` の明示 sender を維持する。

## 原典の前提プロトコール（needs.testkit）

台帳の対象ノードの needs は次の 7 能力である（`ledger/nodes.yaml:4893`、G0a〜G0f で実装済み）。

| 提供元 | 能力 |
|---|---|
| G0b | `extends-pekko-spec`, `pekko-spec`, `test-actor`, `implicit-sender`, `default-timeout`, `expect-msg` |
| G0d | `event-filter` |

`needs.premise_flags` は空である（`ledger/nodes.yaml:4894`）。能力台帳 `ledger/capabilities.yaml` の変更は要らない。`filterEvents`/`EventFilter` は `event-filter`、`expectMsg` は `expect-msg`、`implicit sender` からの sender 指定は `implicit-sender`、`(supervisor ? ...)` の await は `default-timeout` に対応する。

## 仕事は 1 段

1. **移植**：対象 1 ノードを Pekko v1.7.1 から移植して green にする（ワークフロー `fraktor-port-node`。親は `fraktor-porting-ng`）

## 対象ノード（明示列挙、1 件）

`goal_complete_gate.py` は、この節の最初の表の `id` 列を読み、`goal` の列が `G0issue98` の台帳ノードの集合と一致することを確かめる。したがって、表はこの位置に置き、`id` 列に完全なノード ID をバッククォートで 1 件だけ記載する。原典 2 件・除外 0。

| id | kind | ケース数（概数） | needs.testkit |
|:---|:---|:---|:---|
| `actor-tests/src/test/scala/org/apache/pekko/actor/Ticket669Spec.scala` | test | 2 | extends-pekko-spec, pekko-spec, test-actor, implicit-sender, default-timeout, expect-msg, event-filter |

ノードの増減は人間だけが行う。ケース数は台帳の概数であり、実数は run が数えて台帳へ記録する（G0d 以降のゴールと同じ進め方）。

## 原典ケース数の実測と named グループの制約

`ledger/tools/verify_red.py` の `count_original_cases` は、Scala のケース書式（`SCALA_CASE`、`verify_red.py:65–71`）と名前付きブロックの書式（`SCALA_NAMED_PREFIX`、`verify_red.py:75`）の一致数を突き合わせ、`named != scala_cases` のとき `None` を返す（`verify_red.py:237–242`）。ツール自体は変更しない。

固定 commit の原典へ、現行ツールの同じ正規表現を当てて実測した結果は次のとおり。

| 正規表現 | 一致数 | 一致した行 |
|:---|---:|:---|
| `SCALA_CASE` | 2 | `:37`・`:49`（`"..." in {` の 2 件） |
| `SCALA_NAMED_PREFIX` | 3 | `:36`（外側の `"A supervised actor with lifecycle PERMANENT" should {`）・`:37`・`:49` |

`named`（3）≠ `scala_cases`（2）のため、`count_original_cases` は `None` を返す。外側の `should` 名前付きブロック 1 件が差分の原因である。

実測手順（再現）:

1. 固定 commit の原典を取得する。
   `curl -sSL https://raw.githubusercontent.com/apache/pekko/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/actor-tests/src/test/scala/org/apache/pekko/actor/Ticket669Spec.scala -o /tmp/Ticket669Spec.scala`
2. `ledger/tools` を import path に置き、`verify_red.SCALA_CASE` と `verify_red.SCALA_NAMED_PREFIX` を原典へ当てて一致数を出す（`len(SCALA_CASE.findall(text))`、`len(SCALA_NAMED_PREFIX.findall(text))`）。

この制約の意味は次のとおり。

- 原典のケース数は 2 で、除外は 0。したがって移植件数と台帳の実数は 2/2/2 を目標にする。
- 初回から全件成功した場合、`count_original_cases` が `None` を返すため、`verify_red` は `already_green` に上がらず `unexpected_pass` に分類する（`verify_red.py` の docstring のとおり）。件数の書換えや意図的な失敗の追加で回避しない。

## 置き場所

- 本番の actor 処理は `crates/fraktor-actor/src` に置く（red で gap が実証された場合だけ）
- 統合テストは `crates/fraktor-actor/tests/ticket669_spec.rs` に置く（1 ノード 1 ファイル。actor を検証するテストを actor crate の `tests/` に置く既存の規則に従う）
- testkit 側の補助が要る場合は `crates/fraktor-testkit/src` に置く。ただし、このゴールに必要な 7 能力は実装済みであり、testkit の追加を前提にしない

## 前半タスク（この準備タスク）の境界

この準備タスクは、上の「対象ノード」と台帳の `goal: G0issue98` のタグだけを行って止まる。Rust テスト作成、本番コードの編集、`status` の `green` 化、CI・workflow 変更、後続タスクの登録・開始、PR 公開、merge、git add・commit・push は、この準備タスクでは行わない（git のステージング・コミット・プッシュは TAKT のタスク後処理に委ねる）。製品・Rust テスト・CI・workflow はこのタスクの差分に含めない。

## 後半タスク（fraktor-porting-ng）の完了条件

後続のゴール run は次を満たすこと。

### 初回 red を先に採証する（fail-first）

- まず `crates/fraktor-actor/tests/ticket669_spec.rs` に原典どおりの 2 ケースを移植し、失敗テストと実際の赤い結果を先行させる。`cargo test -p fraktor-actor --test ticket669_spec` を実行し、コンパイル失敗またはアサーション失敗の生の結果を、実行コマンド・終了値・診断とともに記録する。
- red が出た場合だけ、その失敗が原典の契約に直結する本番の actor 処理を `crates/fraktor-actor/src` に追加する。実証されない本番変更は行わない。
- 最初から成功した場合の扱いは上記「原典ケース数の実測と named グループの制約」と「already_green / unexpected_pass の扱い」に従う。`already_green` と判定できない `unexpected_pass` のときは red を捏造せず停止して報告する（このゴールは green へ遷移させず終えない）。

### 等価性レビュー

- 原典 2 ケースそれぞれについて、supervisor の ask による子生成、`implicit sender` からの送信、返信（`failure1`/`failure2`）、生成から停止までの `Exception("test")` ログ正確に 1 件、supervisor の停止を、移植した Rust テストが同じ契約として観測していることを照合する。返信だけを取り出す弱いテストへ弱めない。
- `preRestart` ケースは `AllForOneStrategy` の 5 回/10 秒、`postStop` ケースは上限 0・`withinTimeRange` 既定という監督方針の差を保つ。

### 回帰

- 台帳の対象ノードが `status: green` になり、`cases` の `total` が実数（2）で記録されている。
- actor・testkit・台帳の回帰が通る（下記「必須ゲート」）。

## 完了条件（機械判定できる形）

- AC1: `cargo test -p fraktor-actor --test ticket669_spec` が通る（移植した 2 ケースが green）
- AC2: `cargo test -p fraktor-actor` が通る（同 crate の既存テストに退行がない）
- AC3: `cargo test -p fraktor-testkit` が通る（G0a〜G0f の testkit に退行がない）
- AC4: `cargo clippy --workspace --all-targets -- -D warnings` が通る
- AC5: `cargo fmt --check` が通る
- AC6: `node scripts/aislop-check-bootstrap.mjs check`（`npm run check:aislop` と同じ）が通る
- AC7: `cargo run --quiet --release --locked --manifest-path tools/syn-check/Cargo.toml -- --format json crates/fraktor-actor` が違反 0 で終わる
- AC8: `uv run --with pyyaml python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal G0issue98` が 0 を返す（対象ノードが green）
- AC9: 同じ rollup を `--goal G0a` から `--goal G0f` で実行し、すべて 0 を返す（G0a〜G0f の退行なし）
- AC10: `uv run --with pyyaml python3 ledger/tools/check_needs.py --ledger ledger/nodes.yaml --capabilities ledger/capabilities.yaml --goal G0issue98` が 0 を返す
- AC11: 台帳の差分が、対象ノードの `cases`/`status`/`rust.crate`/`rust.test_file` の欄に限られ、`cases.total` が実数（2）で記録されている
- AC12: 成果物は単一の merge-ready な PR として提出し、次の必須の GitHub コンテキストがすべて green であること
  - `Fraktor CI Gate`（`.github/workflows/main-pr-ci.yml` の `gate` ジョブ。`ci.yml` を `needs` し、CI の成否と check-result を必須にする）
  - `Check unresolved comments`（`.github/workflows/review-thread-resolution.yml` が公開する review-thread の解決状態。`required_context: Check unresolved comments`）
- AC13: このゴールは 1 ノードの移植を 1 つの PR にまとめる。後続タスクの登録・開始、`G1a` への着手、CI と `.takt` workflow の変更は、その PR に含めない

## 必須ゲート（run 後の人間の確認コマンド）

```sh
cargo test -p fraktor-actor --test ticket669_spec
cargo test -p fraktor-actor
cargo test -p fraktor-testkit
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
node scripts/aislop-check-bootstrap.mjs check
cargo run --quiet --release --locked --manifest-path tools/syn-check/Cargo.toml -- --format json crates/fraktor-actor
uv run --with pyyaml python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal G0issue98
for g in G0a G0b G0c G0d G0e G0f; do uv run --with pyyaml python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal $g; done
uv run --with pyyaml python3 ledger/tools/check_needs.py --ledger ledger/nodes.yaml --capabilities ledger/capabilities.yaml --goal G0issue98
```

## already_green / unexpected_pass の扱い

原典のこのノードは、外側の名前付き `should` ブロック 1 件とケース 2 件を持つ。上記のとおり `count_original_cases` が `None` を返すため、初回成功時の扱いは次のとおりとする。

- `verify_red` が `already_green` を返した場合（含まれる原典ケース数と承認済み除外数の和が通過数に一致し、対象テストが全件成功する場合だけ）、`verify_green` → `equivalence-review` → `record-already-green` で green を永続化する。失敗を加工して red を作らない。
- `unexpected_pass` に分類された場合（このノードでは、上記の数え方の制約で `already_green` に上がらない）、失敗の加工をせず、原典との等価性、実ケース数、`count_original_cases` が `None` を返したこと、`cargo test` の生の結果を添えて報告する。green を書かない。扱えない場合は、台帳を書き換えずに停止して報告する。

## 完了関門（`goal_complete_gate.py`）との整合

- ゴール文書の**先頭行だけ**が `^# ゴール (G0[a-zA-Z0-9]+)：` に一致すること（`goal_complete_gate.py:34`、`:84`）。先頭行は `# ゴール G0issue98：Ticket669Spec` である。
- `## 対象ノード` 節の**最初の表**の `id` 列から、バッククォート区切りの完全なノード ID を読むこと（`:95`、`:106`、`:135`）。表はこの節の先頭に置き、`id` 列にバッククォートで 1 件だけ記載する。
- その ID 集合が、台帳で `goal == G0issue98` のノード集合と一致すること（`:146`、`:197`）。その後に `rollup.py --goal G0issue98` の終了コードを返す（`:202`）。

## G0a〜G0f の回帰

このゴールは基盤ゴールではないが、対象テストは G0a〜G0f の基盤（testkit の `event-filter`・`implicit-sender`・`default-timeout`・`expect-msg` と、actor の監督・`preRestart`/`postStop`）に依存する。したがって、AC9 として `rollup.py --goal G0a` から `--goal G0f` がすべて 0 を返すことを確認する。加えて、`cargo test -p fraktor-testkit` と `cargo test -p fraktor-actor` で testkit と actor の退行を確認する。

## やらないこと（このゴールの PR に含めない）

- 台帳の他ノードの変更、ノードの追加・削除、`kind` の変更、`do_not_port` への変更、ケースの除外
- `ledger/tools/` の変更、`already_green` やケース数カウンタのツール修正
- CI と `.takt` workflow の変更、`G1a` への着手
- 後続タスクの登録・開始

## 前提と残るリスク

- 対象は 2 ケースの小さなテストだが、原典の 100% の等価性（`preRestart` で返信、`postStop` で返信、`EventFilter` の発火数 1、supervisor の ask による子生成、`implicit sender` からの送信、停止）を満たす必要がある。
- `preRestart` ケースは 5 回/10 秒の `AllForOneStrategy` で再起動し、`postStop` ケースは上限 0 で停止する。この差は監督方針の設定に直結する。
- 原典のケース数の数え方（外側の名前付きブロックがあるため `count_original_cases` が `None` を返す）は、初回成功時に `already_green` でなく `unexpected_pass` になる制約を生む。ツールは変更しない。
- 作業ツリー内の `.takt/` は TAKT ランタイムのオーバーレイであり、committed origin/main の workflow と一致しないことがある。この文書の判断は committed origin/main（`git show HEAD:`）の内容に基づく。

## 台帳のタグ付け（この準備タスクで適用する）

1. 対象ノードの `goal: null` を `goal: G0issue98` に書き換える（台帳の差分は 1 行だけ。`ledger/nodes.yaml:4906`）。
2. `status: unported` は変更しない（`:4888`）。`cases`・`rust`・`needs`・`depends_on_impl` など他の欄は変更しない。
3. タグ付け後、`check_needs.py --goal G0issue98` は exit 0、`rollup.py --goal G0issue98` は「1 件・unported 1」で exit 1（未完了なので期待どおり）、`rollup.py --goal G0a`〜`G0f` はすべて exit 0 になることを、コミット前後の実際のコマンドで確定させる。

## 移植結果

| Pekko v1.7.1 のケース | Rust のテスト | 結果 |
|:---|:---|:---|
| `be able to reply on failure during preRestart` | `be_able_to_reply_on_failure_during_pre_restart` | 元の送信者へ `failure1` |
| `be able to reply on failure during postStop` | `be_able_to_reply_on_failure_during_post_stop` | 元の送信者へ `failure2` |

両ケースは、監督役への `Props` の ask、テストアクターからの送信、監督役の停止を通る。ログは受信時の panic に対応する `RuntimeException` 型とメッセージ `test` で絞り、各ケースで 1 件を確認した。台帳は原典 2 件、移植 2 件、成功 2 件、除外 0 件。

実装前のゴール先端 `1265984086f192b3169e95e48a7773348726c9b9` に先行テストだけを載せた `cargo test -p fraktor-actor --test ticket669_spec` は exit 101 だった。`Actor::post_stop_with_context` が未定義で E0407 が 1 件、`SupervisorStrategy::all_for_one` が未定義で E0599 が 2 件出た。TAKT の自動ステップは前段応答の入力行を取得できず `unclassifiable` となった。同じ保存済み応答を渡して独立に実行した `verify_red.py` は `expected_compile_error`、`red_recorded=true`、`original_cases=null`、`excluded=0` を返した。

再開実行は対象テストを通過した。TAKT が前段の `plan.md` を読み戻せず等価性レビューに入れなかったため、保存した差分を回収して原典との照合と検証を行った。時間窓内の許可は兄弟ごとに記録し、全員が許可される場合に再起動する。受信失敗時の送信者は失敗した子にだけ渡し、通常の停止フックには dead letters を渡す。既存の `post_stop` は追加フックの既定実装から呼ばれる。

`cargo test -p fraktor-actor -p fraktor-testkit` は356件成功した。対象 needs、対象と G0a〜G0f の rollup、ゴール文書の検証26件、`verify_green.py`、clippy、fmt、aislop、syn-check も成功した。
