# ゴール G0issue180：testkit の CoronerSpec・AbstractSpec と、TestLatch・receiveWhile

状態：**green**（2 ノード）。CoronerSpec は 4 件のうち 2 件を移植し、2 件はオーナーの承認で除外した。AbstractSpec は fixture として green にした。PekkoSpecSpec は [#210](https://github.com/j5ik2o/fraktor-ng-rs/issues/210) に分けたので、この PR には入っていない。

[Issue #180](https://github.com/j5ik2o/fraktor-ng-rs/issues/180)（親 [#179](https://github.com/j5ik2o/fraktor-ng-rs/issues/179)）。原典は Pekko v2.0.0（commit `9fbd2283b1cf701c90a1223bf621942c191db61f`）。TAKT は使っていない。

この文書は、実装がほぼ終わった時点の設計を書いたものである（2026-11-25 の決まりの変更で、設計の独立レビューは実装の後に受けることになった）。

## 対象ノード

| id | Rust | 原典のケース数 | 状態 |
|:---|:---|---:|:---|
| `testkit/src/test/scala/org/apache/pekko/testkit/CoronerSpec.scala` | `crates/fraktor-testkit/tests/coroner_spec.rs` | 4 | green（4/2/2、除外 2） |
| `testkit/src/test/scala/org/apache/pekko/testkit/AbstractSpec.scala` | なし | 0 | green（fixture、0/0/0） |

ケースの数は v2.0.0 の原典で数えた。台帳の近似値は使っていない。

- CoronerSpec（154 行）：`"A Coroner" must {` の下に `in` が 4 つある（L37、L47、L56、L68）。
- AbstractSpec（21 行）：本体は `abstract class AbstractSpec extends AnyWordSpecLike with Matchers with BeforeAndAfterEach` の 1 行だけで、ケースは 0 件。コメントは「we could migrate PekkoSpec to extend this」。v2.0.0 の中で継承しているのは `actor-tests/.../actor/ProviderSelectionSpec.scala` だけで、その spec は `beforeEach` も `afterEach` も使っていない（`git grep -n AbstractSpec v2.0.0` の結果は、この 2 ファイルだけ）。ほかのテストが継承するだけの土台である。
- PekkoSpecSpec（115 行、4 件）は数えたが、この PR の対象ではない。

## 判断の記録

- **範囲**：PekkoSpecSpec の 4 件のうち 3 件は、kernel にない機能（unhandled の通知、root guardian とシステムの終了フック、DeadLetter イベント）を使う。起動元（run_8737572861cb）が範囲を分けると決め、#207・#208・#209・#210 を起票した。#180 に残したのは、CoronerSpec、AbstractSpec、TestKit の能力（TestLatch、receiveWhile、「terminate all actors」が使う awaitCond）である。
- **CoronerSpec の除外 2 件**：JVM にしかない仕組みに頼る 2 件（スレッド数の報告、デッドロックの報告）は、オーナー本人（j5ik2o）が除外として承認した。出所は元帥の項目365（e5608）の回答「除外として承認し、代わりのテストを足す」で、台帳の `cases.excluded` の `approved_by: j5ik2o`、`approved_at: 2026-11-25` はその転記である。OS ごとにスレッドを数える方法や、所有者を追跡するロックで作り直す案は、起動元が採らないと決めた。
- **AbstractSpec の fixture 化**：オーナー本人（j5ik2o）が方針として承認した。出所は元帥の項目366（e5619）の方針（回答「i：fixture にして green（分母から外す）」）で、承認日は 2026-11-25。ノードの書き方は、前例の `testkit/src/test/scala/org/apache/pekko/testkit/PekkoSpec.scala`（kind: fixture、green、0/0/0）と同じにした。台帳のスキーマには fixture の承認を書く欄がないので、承認はこの文書に記録した（PekkoSpec の前例も、承認は `docs/goals/g0b-testkit-seed.md` にある）。AbstractSpec の役目（WordSpec、Matchers、BeforeAndAfterEach）は Rust の `#[test]` と `assert!` が果たすので、`rust.crate` と `rust.test_file` は null である。

## 何を変えるか、何を変えないか

変えるもの（すべて `crates/fraktor-testkit`。std を使う test 用クレートで、kernel ではない）：

- `test_kit::TestLatch`（新規）：数え下げのラッチ。`new(count, settings)`、`count_down`、`is_open`、`open`、`reset`、`ready(&ActorSystem, max)`、`DEFAULT_TIMEOUT`。複製は数を共有する（`Arc<Mutex<usize>>`）。`Send + Sync` なので、`Props::new` の工場に渡せる。
- `test_kit::ReceiveWhile`（新規）：`receive_while` の上限（`max`、`idle`、`messages`）。既定値は Pekko と同じ（`max` なし＝within の残り時間か、伸ばした single-expect の既定値。`idle` なし＝無制限。`messages` なし＝無制限）。
- `receive_while(limits, pick)` を `TestKit`、`TestProbe`、`FraktorSpec` に追加した。`pick` は `FnMut(&Message) -> Option<T>` で、Pekko の部分関数を表す（`None` が「定義されていない」）。
- `await_cond(condition, max)` を `TestKit` と `FraktorSpec` に追加した（これまでは `TestProbe` だけにあった）。
- `coroner` モジュール（新規）：`watch(duration, title, out)`、`print_report(title, out)`、`WatchHandle`（`cancel`、`ready`、`result`）、`DEFAULT_START_AND_STOP_DURATION`。
- 内部：`TestKitBase::receive_while` と `remaining_or_default`、`TestActor::put_back`（拒んだメッセージをキューの先頭に戻す）。

変えないもの：

- `fraktor-actor`（kernel と adaptor）には手を入れていない。kernel の no_std はそのままである。
- 既存の公開 API のシグネチャと振る舞い。足したのは新しい型とメソッドだけである。
- PekkoSpecSpec のテストファイル（#210 で置く）。

範囲の外：

- `watch` の `displayThreadCounts` と `startAndStopDuration` の引数（スレッド数は除外したケースの機能で、時間の引数を使うテストはない）。
- Pekko の報告にある「All threads:」と「Deadlocks found for ...」の節、VM の起動時刻、ヒープの使用量。標準ライブラリでは得られない。

## 公開 API への影響

足しただけで、壊したものはない。既存のテスト（`cargo test --workspace`）はすべて通る。後続の 7 件（#197〜#204）と #210 は、次の形で使える。

```rust
let latch = TestLatch::new(1, spec.test_kit_settings());
let opener = latch.clone(); // アクターやスレッドに渡す
latch.ready(spec.system(), Duration::from_secs(2));

let got = probe.receive_while(ReceiveWhile::new().max(Duration::from_secs(3)).messages(3), |m| {
  m.downcast_ref::<String>().cloned()
});

let system = spec.system().downgrade();
spec.terminate();
spec.await_cond(|| refs.iter().all(|r| system.is_actor_terminated(r)), Duration::from_secs(2));
```

`ledger/capabilities.yaml` の `test-latch` と `receive-while` の `provided_by` を `G0issue180` にした。G0issue180 は green なので、`check_needs.py` はこの 2 つを満たすものとして扱う。`await-cond` は前から G0issue121 が持っている。

## 守る不変条件

TestLatch：

- 数は 0 以上、`count` 以下に収まる。`count_down` は 0 で止まり、`open` は 0 にし、`reset` は `count` に戻す。
- `is_open` は、数が 0 のときだけ真である。
- `ready` が戻るのは、ラッチが開いているときだけである。期限（伸ばした `max`）に閉じたままなら、`Timeout of <max> with time factor of <factor>` で失敗する。
- `ready` は最初にラッチを見るので、開いていればすぐ戻る。終了したシステムでも同じである。

receive_while：

- メッセージは失われず、順序も変わらない。拾ったもの、続けてキューに残ったものを並べると、届いた順になる。
- 拾うのは `pick` が受け入れたものだけで、数は `messages` を超えない。
- 拒んだメッセージは、キューの先頭に戻る。
- 終わった後の `last_message` は、最後に拾ったメッセージで、拾っていなければ空である。
- 終わった後は、no-message の待ちとして扱う（Pekko の `lastWasNoMsg = true`）。

Coroner の WatchHandle：

- 結果は、cancel と期限切れのうち先に起きた方で決まり、その後は変わらない。
- 報告を書くのは、期限切れになったときだけである。cancel された watch は何も書かない。
- watch のスレッドが終わった時点では、期限切れなら報告をすべて書き終えている。
- `cancel` は、スレッドが終わるまで（最長で start-and-stop の時間）待つ。

## 状態遷移と Quint

TestLatch、receive_while、WatchHandle は、どれも状態遷移を持つ。WatchHandle には、2 本のスレッドの競合もある。そこで、それぞれを Quint で書き、上の不変条件を検査した。仕様は `crates/fraktor-testkit/specs/` にある。

| 仕様 | モデルにしたもの | 不変条件（`invariant` にまとめた） |
|:---|:---|:---|
| `test_latch.qnt` | 数、`countDown`、`openLatch`、`reset`、`readyReturns` | `counterInRange`、`readyOnlyWhenOpen` |
| `receive_while.qnt` | キュー、到着、受け入れたものを拾う、拒んだものを戻す、時間切れ、上限 | `nothingLostOrReordered`、`pickedOnlyAccepted`、`withinLimit`、`lastIsLastPicked` |
| `watch_handle.qnt` | 結果（Pending・Cancelled・Expired）、start、cancel、expire、writeReport、finish | `outcomeNeverChanges`、`reportOnlyWhenExpired`、`cancelledWritesNothing`、`finishedConsistent`、`resultAfterCancelIsDecided` |

検査の結果（quint 0.32.0）：

| コマンド | 結果 |
|:---|:---|
| `quint test test_latch.qnt` | 2 件 pass |
| `quint test receive_while.qnt` | 3 件 pass |
| `quint test watch_handle.qnt` | 3 件 pass |
| `quint run <各仕様> --invariant=invariant --max-steps=30 --max-samples=20000` | 3 本とも `[ok] No violation found` |

不変条件が実際に違反を捕まえることも確かめた。わざと壊した写しを 3 本作り、同じ `quint run` を流した。壊し方は、`countDown` を 0 で止めない、拒んだメッセージを戻さずに捨てる、cancel が期限切れの結果を上書きする、の 3 つである。3 本とも `[violation]` になった。

Quint の検査は CI に入れていない（CI に quint がない）。仕様を変えたら、上のコマンドを手で流す。

## PBT の性質

性質は、範囲を区切った入力をすべて試す形で確かめる（乱数で抜き出すのではなく、全件を流す）。新しい依存は足していない。除外はない。

| 性質 | 入力の全域 | テスト |
|:---|:---|:---|
| P1：`is_open` は、数のモデル（0 で止まる数え下げ、`open` で 0、`reset` で `count`）が 0 のときに限り真 | `count` は 0〜4、操作の列は長さ 0〜5（3 種類の操作のすべての並び） | `test_latch_test.rs` の `is_open_follows_the_counter_model_for_every_bounded_sequence` |
| P2：`receive_while` は、受け入れたものが続く先頭の部分を上限で切って返し、残りは順に残り、`last_message` は最後に拾ったもの | キューは長さ 0〜4、値は 0〜3（偶数を受け入れる）、上限はなし・0〜4 | `test_kit_value_test.rs` の `receive_while_takes_the_accepted_prefix_and_keeps_the_rest_for_every_bounded_queue` |
| P3：cancel と期限切れのどちらが先でも、結果が `true` なら出力は空、`false` なら報告が全部ある | watch の時間 0〜6 ms と、cancel までの遅れ 0〜6 ms のすべての組 | `coroner_test.rs` の `the_outcome_and_the_output_agree_whichever_of_cancel_and_expiry_comes_first` |

P3 は時刻の競合なので、全組を流しても、どちらが先になるかは実行ごとに変わりうる。ただし、どちらになっても性質は成り立たなければならないので、確かめ方として欠けてはいない。

## 壊しうる入力の一覧

| 入力 | 扱い | 確かめたテスト |
|:---|:---|:---|
| `TestLatch::new(0, ..)` | はじめから開いている | P1（count 0） |
| 開いたラッチへの `count_down` | 0 のまま | `counting_down_an_open_latch_keeps_it_open` |
| `ready` の `max` が 0 | 開いていれば戻り、閉じていれば失敗する | P1 の範囲外。最初にラッチを見る作りで保証している |
| 時間の倍率（time factor）が 3 | 待ちが 3 倍になる | `ready_waits_for_the_dilated_max`、`receive_while_dilates_max` |
| ほかのスレッドからの `count_down` | `ready` が 10 ms 以内に気づく | `ready_returns_when_another_thread_opens_the_latch` |
| アクターの中での `count_down` | `ready` がシステムを動かすので開く | `ready_runs_the_system_so_an_actor_can_open_the_latch` |
| 終了したシステムでの `ready` | 開いていればすぐ戻る | `ready_returns_on_a_terminated_system_when_the_latch_is_open` |
| `messages(0)` | 何も取らずに戻る | P2 |
| 空のキュー、最初から拒むメッセージ | 空を返し、メッセージは残る | P2、`receive_while_with_no_match_returns_nothing_and_leaves_the_message_queued` |
| `idle` が `max` より短い | 間が空いたところで終わる | `receive_while_stops_when_idle_passes_between_messages` |
| `max` なしで within の中 | within の残り時間を使う | `receive_while_without_max_uses_the_remaining_time_of_within` |
| 期限 0 ms の watch、すぐの cancel | P3 の通り | P3 |
| 期限切れの後の cancel | 結果は `false` のまま | `cancelling_an_expired_watch_keeps_it_expired` |
| 書き込みに失敗する出力 | `print_report` は最初のエラーを返す | `print_report_returns_the_error_of_a_failing_output` |
| `max` が短い `result` | 決まっていなければ `false` | `result_of_a_running_watch_is_false_when_max_passes_first` |

## 原典との差異

- `TestLatch` は、Pekko では暗黙のシステムから time factor を読む。port は `TestKitSettings` を受け取る。time factor を持つのは、システムではなく spec の設定だからである。待つときにシステムを動かす必要があるので、`ready` はシステムを引数に取る（Pekko では `Await.ready(latch, max)` で、アクターはディスパッチャーの上で勝手に動く）。
- `receive_while` の中の `receive_one` は、within の期限で待ちを短くする。Pekko の `receiveOne` は短くしない。このため、within より長い `max` を渡したときは、Pekko より早く、within の終わりで戻る。
- Coroner の報告にあるのは、題名、アーキテクチャ、OS、使える並列度、報告したスレッドの名前とスタック（`Backtrace::force_capture`）である。Pekko の報告にある JVM の情報、全スレッドのダンプ、デッドロックの節はない。

## 先行テストと red の証拠

テストは製品より先に置いた（`62e41bd`、製品は未変更）。

| コマンド | 終了値 | 結果 |
|:---|---:|:---|
| `cargo test -p fraktor-testkit --test coroner_spec` | 101 | E0432（`fraktor_testkit::coroner` が無い） |
| `cargo test -p fraktor-testkit --lib` | 101 | E0432（`ReceiveWhile`）と E0599（`receive_while`、`await_cond`）。コンパイルエラー 16 件 |
| `verify_red.py`（CoronerSpec。原典は v2.0.0 の clean な clone） | 0 | `expected_compile_error`。台帳の status を red にした |

`verify_red.py` の `original_cases` は null だった。WordSpec の `must {` は、この道具が全件を数えきれる書式ではないためである。件数は上のとおり手で数えた。

## 検証（ローカル）

| コマンド | 結果 |
|:---|:---|
| `cargo test -p fraktor-testkit --test coroner_spec` | 2 passed |
| `cargo test -p fraktor-testkit --lib` | 82 passed |
| `cargo test --workspace` | すべて成功（下の PR 本文に件数） |
| `cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --check` | 成功 |
| syn-check（`crates/fraktor-testkit`） | 違反 0、抑制 0 |
| `npm run check:aislop` | 既知の指摘と一致 |
| `verify_green.py --crate fraktor-testkit` | `all_pass: true` |
| `pekko_baseline.py check-ledger`、`check-refs`、`relocate_nodes.py check` | 成功 |
| `report_ledger_1to1.py check` | 成功（未記載の一覧に 2 ノードを `no-report` で足した） |
| `check_needs.py --goal G0issue180`、`rollup.py --goal G0issue180` | needs 閉性 OK、`{'green': 2}`、完了 |
