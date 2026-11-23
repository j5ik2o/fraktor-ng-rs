# ゴール G0issue117：RestartStrategySpec

[Issue #117](https://github.com/j5ik2o/fraktor-ng-rs/issues/117) の対象は Pekko v1.7.1 の `RestartStrategySpec`。原典は `OneForOneStrategy` の `maxNrOfRetries` と `withinTimeRange` の組合せで、子アクターの再起動許可がどう決まるかを5ケースで確かめる。 [Pekko v1.7.1 の原典](https://github.com/apache/pekko/blob/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/actor-tests/src/test/scala/org/apache/pekko/actor/RestartStrategySpec.scala)

## 対象ノード

| id | Rust の統合テスト | 原典のケース数 |
|:---|:---|---:|
| `actor-tests/src/test/scala/org/apache/pekko/actor/RestartStrategySpec.scala` | `crates/fraktor-actor/tests/restart_strategy_spec.rs` | 5 |

原典のケース数は5、除外は0件。台帳の `extends-pekko-spec`、`pekko-spec`、`test-actor`、`default-timeout` は G0b で充足している。

## 原典と実行経路の独立照合

Pekko v1.7.1 commit `9a54d1294c3ac7821bbed3c940769c3d5027d1ff` の `RestartStrategySpec.scala` と `FaultHandling.scala` を照合した。許可判定は `ChildRestartStats.requestRestartPermission` が持つ。

| 原典のケース（実名と原典行） | 戦略 | Rust のテスト名 | 直接観測する契約 |
|:---|:---|:---|:---|
| 1 "ensure that employee stays dead after max restarts within time range"（L43） | `OneForOneStrategy(maxNrOfRetries = 2, withinTimeRange = 1.second)` | `stops_after_max_restarts_within_time_range` | 2回再起動、境界（1000ms）を含む3回目失敗で停止 |
| 2 "ensure that employee is immortal without max restarts and time range"（L91） | `OneForOneStrategy()` | `restarts_one_hundred_times_without_limits` | 100回再起動後に生存（`!isTerminated`） |
| 3 "ensure that employee restarts after number of crashes not within time range"（L115） | `OneForOneStrategy(maxNrOfRetries = 2, withinTimeRange = 500.millis)` | `restarts_after_time_range_expires` | 2回再起動後、700ms経過で窓が再開し3回目も再起動 |
| 4 "ensure that employee is not restarted after max retries"（L173） | `OneForOneStrategy(maxNrOfRetries = 2)` | `stops_after_max_retries_without_time_range` | 時間経過で上限が回復せず3回目失敗で停止 |
| 5 "ensure that employee is not restarted within time range"（L224） | `OneForOneStrategy(withinTimeRange = 1.second)`（`maxNrOfRetries` 省略） | `stops_and_notifies_parent_with_only_time_range` | 上限省略＋有限窓は実効上限1、2回目処理失敗で停止し親が Terminated を受信 |

`requestRestartPermission` はパターンの順序が意味を持つ。`case (Some(retries), _) if retries < 1 => false`、`case (Some(retries), None) => maxNrOfRetriesCount += 1; maxNrOfRetriesCount <= retries`、`case (x, Some(window)) => retriesInWindowOkay(x.getOrElse(1), window)`、最後の `case (None, _) => true` は、上記で捕捉されなかった場合、すなわち上限省略かつ窓なし `(None, None)` にだけ適用される（無条件許可）。したがって上限省略の扱いは窓の有無で異なる。(1) `(None, None)` は無条件許可、(2) `(Some(r), None)` は累計カウント `<= r`、(3) `(x, Some(window))` は `x.getOrElse(1)` を上限として窓判定する（上限省略時は実効1）。窓は最初の許可要求時に開き（`restartTimeWindowStartNanos == 0` の分岐）、`(now - start) <= window` を包含とし、超過時は `count = 1` で窓を開き直す。Rust の [RestartHistory](../../crates/fraktor-actor/src/kernel/supervision/restart_history.rs) は同じアルゴリズムを持つ。`OneForOneStrategy.processFailure` は `restart && stats.requestRestartPermission(...)` のときだけ再起動し、それ以外は `context.stop(child)` する。

## 実際に使った TestKit needs と前提

Rust 版は `FraktorSpec`（`extends-pekko-spec`、`pekko-spec`）、`spec.test_actor()` を sender に使う（`test-actor`）、`ask_with_timeout` と `await_result` に渡す `DefaultTimeout::timeout(spec.system())`（`default-timeout`）を使う。時間窓は `ActorSystem::set_clock` に `Clock` を実装した手動カウンタ（`ControlledClock`）を渡して進め、実時間の待機や `Thread.sleep` を使わない。

原典由来の needs（`event-filter`、`test-latch`）と前提 `thread-sleep` は、Rust 版で使っていないため台帳から外した。`test-latch` は G0b〜G0f でも未提供（`provided_by: null`）であり、原典の待機は本移植では手動時計と有限待ちで代替している。この置換は件数やケースの意味を変えない。

## 観測の違い

Pekko `Actor.scala` の既定 `preRestart` は古いインスタンスの `postStop` を呼ぶ。したがって、原典の stop ラッチは再起動で破棄される旧インスタンスでも開く。Rust 版の `post_stop` フック累計も同じく旧インスタンス破棄を含み、ケース1・4では「2回の再起動＋最後の停止」で3になる。これは累計であり、永久停止の回数ではない。

永久停止と親への通知は別の観測である。ケース5は、同じ `employee` ハンドルが永久停止することと、`WatchingParent` が受信した `Terminated` が対象と同一の `ActorRef` を指すことを別々に確認する。ケース5は後続の `Ping`・`Crash` が停止中の子に届き処理されないことも、処理回数（`crashes = 2`、`pings = 2`）と `restarts = 1` で示す。

## 検証結果と再現コマンド

実装は既存の `OneForOne`、`RestartHistory`、`RestartLimit`、`RestartWindow`、`Clock` を再利用し、`SupervisorStrategy::one_for_one_with_window(RestartLimit, RestartWindow)` を追加した。`RestartLimit::unlimited()` と無期限の組合せは再起動を履歴に数えない。有限の上限には `RestartLimit::new` を使う。有限窓の `RestartHistory` 判定は `RestartLimit::for_window()` で上限省略時を実効1に解決する。executor の受信失敗・初期化失敗・Kill の3入口は、`Restart` 決定後に履歴で許可を判定する共通処理を通し、有限窓では履歴の計数を二重に行わない。

| コマンド | 実測 |
|:---|:---|
| `cargo test -p fraktor-actor --test restart_strategy_spec -- --test-threads=1` | 5 passed、0 failed、0 ignored |
| `cargo test -p fraktor-actor --test restart_limit_zero_no_clock` | 1 passed、0 failed（上限0＋有限窓が時計未読で停止） |
| `cargo test -p fraktor-actor --lib` | 141 passed、0 failed（初期化の窓境界回帰2件を含む） |
| `cargo test --workspace` | 46 バイナリ＋doc-test 結果2件、390 passed、0 failed（doc-test は計0件） |
| `cargo fmt --check` | 差分なし |
| `cargo clippy --workspace --all-targets -- -D warnings` | 警告なし |
| `uv run --with pyyaml python3 ledger/tools/check_needs.py --ledger ledger/nodes.yaml --capabilities ledger/capabilities.yaml --goal G0issue117` | 対象1ノード、needs 閉性 OK |
| `uv run --with pyyaml python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal G0issue117` | 1ノード、green 1、完了 |

分離した一時変異で感度も確認した。窓の包含判定を `<` に変えるとケース1が停止せず失敗し、上限省略の実効値を2に変えるとケース5が2回目で停止せず失敗した。`apply_permission_to` を無条件 `self.now()` に戻すと、時計を読むと panic する control boundary を使う `restart_limit_zero_no_clock` が失敗した。いずれも実験差分は復元済みで、原典5ケースと補助単体テストの件数には加算しない。

初期化失敗では、再起動許可と履歴を子の後片付け前に確定する。補助回帰は同じ子が100msに初期化失敗し、子の停止フックで10ms進むと包含境界で停止、11ms進むと期限後の3回目初期化に成功してメッセージを処理することを確認する。今回の生ログとコマンド・終了値は `.takt/issue-117-phase2-evidence/fix/` に保存した。

対象は1ノードのみ `status: green`、`cases.total/ported/passing: 5`、`excluded: []`、`goal: G0issue117` とした。他ノードと能力定義は変更していない。PR・issue 操作、最終 CI・マージは同担当の対象外とする。

## 確定した製品未変更の初期 red

第2工程の製品変更前（テストのみの確定コミット `606f3291efce242a3469e7473b9fc55a2191170b`、ツリーは統合 `269038df` と同一）の状態では、`SupervisorStrategy::one_for_one_with_window` が未実装のためコンパイルが停止した。実測は exit 101、E0599 が `restart_strategy_spec.rs` の4箇所（宣言5件は実行0件）。この red は実行時に5ケースが失敗したことを意味せず、実行時の成功や全型整合の証拠にならない。第2工程の実装後は上表のとおり5/5成功しており、この red を成功扱いしていない。
