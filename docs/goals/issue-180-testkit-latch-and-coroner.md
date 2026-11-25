# ゴール G0issue180：testkit の CoronerSpec・AbstractSpec と、TestLatch・receiveWhile

状態：**green**（2 ノード）。CoronerSpec は 4 件のうち 2 件を移植し、2 件はオーナーの承認で除外した。AbstractSpec は fixture として green にした。PekkoSpecSpec は [#210](https://github.com/j5ik2o/fraktor-ng-rs/issues/210) に分けたので、この PR には入っていない。

[Issue #180](https://github.com/j5ik2o/fraktor-ng-rs/issues/180)（親 [#179](https://github.com/j5ik2o/fraktor-ng-rs/issues/179)）。原典は Pekko v2.0.0（commit `9fbd2283b1cf701c90a1223bf621942c191db61f`）。TAKT は使っていない。

この文書は、実装がほぼ終わった時点の設計を書いたものである（2026-11-25 の決まりの変更で、設計の独立レビューは実装の後に受けることになった）。設計の独立レビュー（`95719005` に対して）の指摘 12 件と、Devin の指摘 5 件を受けて直した。直した点と、指摘ごとの扱いは、末尾の「レビューの指摘の扱い（修正 1）」にある。

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

- `test_kit::TestLatch`（新規）：数え下げのラッチ。`new(count, settings)`、`count_down`、`is_open`、`open`、`reset`、`ready(&ActorSystem, max)`、`DEFAULT_TIMEOUT`。複製は、数と「数が 0 になった時刻」を共有する（同じロックの下に置く）。`Send + Sync` なので、`Props::new` の工場に渡せる。
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

足しただけで、壊したものはない。既存のテスト（`cargo test --workspace`）はすべて通る。後続の 7 件（#197〜#204）と #210 は、**テストのスレッドから待つ場合は**、次の形で使える。

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

G0issue180 のノード（CoronerSpec、AbstractSpec）は、この 2 つの能力を使っていない。このため、`check_needs.py --goal G0issue180` は 2 つの能力を検査しない。能力があるという根拠は、次の単体テストである。

- `test-latch`：`crates/fraktor-testkit/src/test_kit/test_latch_test.rs` の全件（性質 P1、アクターやほかのスレッドが開く場合、期限、time factor、期限の後に開く場合、終了したシステム）
- `receive-while`：`crates/fraktor-testkit/src/test_kit/test_kit_value_test.rs` の `receive_while_*`（性質 P2 を含む）、`test_probe_test.rs` の `receive_while_takes_the_messages_of_the_probe`、`fraktor_spec_test.rs` の `receive_while_reads_the_test_actor_of_the_spec`

能力の範囲には制約が 2 つある。どちらも、後続の issue で扱う（起動元が #197・#200・#201 に送る）。

- `ready` は、テストのスレッドから呼ぶ。アクターの中や監督の判断の中では待てない。システムはテストのスレッドが動かしていて、待つ側がそのシステムの中にいるからである（原典では SupervisorHierarchySpec L839、ActorSystemSpec L110、BalancingSpec L46 がこの使い方をする）。
- `ready` の時間切れは panic で知らせる。時間切れを確かめてテストを続けるには、`catch_unwind` で受ける（`ready_fails_with_the_max_and_the_time_factor_when_the_latch_stays_closed` がその形）。原典の ReceiveTimeoutSpec L139・L176 の `intercept[TimeoutException]` がこれに当たる。`Result` を返す版は足していない。

## 守る不変条件

TestLatch：

- 数は 0 以上、`count` 以下に収まる。`count_down` は 0 で止まり、`open` は 0 にし、`reset` は `count` に戻す。
- `is_open` は、数が 0 のときだけ真である。
- 数が 0 である ⇔ 「開いた時刻」が記録されている。`reset` は時刻を消す。0 のまま `count_down` や `open` をしても、時刻は最初に開いたときのまま変わらない。
- `ready` が戻るのは、ラッチが期限（伸ばした `max`）までに開いたときだけである。期限までに開かなかったときと、期限の後に開いたとき（配送の処理が期限をまたいで、その最後に開けた場合）は、`Timeout of <max> with time factor of <factor>` で失敗する。
- `ready` は最初にラッチを見るので、開いていればすぐ戻る。終了したシステムでも同じである。

receive_while：

- メッセージは失われず、順序も変わらない。拾ったもの、続けてキューに残ったものを並べると、届いた順になる。
- 拾うのは `pick` が受け入れたものだけで、数は `messages` を超えない。
- 拒んだメッセージは、キューの先頭に戻る。
- 終わった後の `last_message` は、最後に拾ったメッセージで、拾っていなければ空である。`messages(0)` のときは、待つ前に終わるので、呼ぶ前の値が残る（Pekko と同じ）。
- 終わった後は、no-message の待ちとして扱う（Pekko の `lastWasNoMsg = true`）。

Coroner の WatchHandle：

- 結果は、cancel と期限切れのうち先に起きた方で決まり、その後は変わらない。
- 報告を書くのは、期限切れになったときだけである。cancel された watch は何も書かない。
- watch のスレッドが終わった時点では、期限切れなら報告の書き込みを試し終えている。出力が途中で失敗したときは、残りを書かずに終わる（試し終えたことは変わらない）。
- `cancel` は何度でも、どのスレッドからでも呼べる。結果を決めた後、スレッドが終わるまで待つ。start-and-stop の時間が先に過ぎたら、終わる前に戻る。終わったのを見て戻ったときは、スレッドは必ず終わっている。

## 状態遷移と Quint

TestLatch、receive_while、WatchHandle は、どれも状態遷移を持つ。WatchHandle には、2 本のスレッドの競合もある。そこで、それぞれを Quint で書き、上の不変条件を検査した。仕様は `crates/fraktor-testkit/specs/` にある。

| 仕様 | モデルにしたもの | 不変条件（`invariant` にまとめた） |
|:---|:---|:---|
| `test_latch.qnt` | 数（`count` は 0〜3）、開いた時刻、`countDown`・`openLatch`・`reset`（待っている間にも起きる）、待つ側（Idle・Waiting・Returned・TimedOut）、期限、時刻（配送が期限をまたぐ場合を含む） | `counterInRange`、`countDownSaturates`、`openMeansZero`、`resetRestoresCount`、`openedAtMatchesCounter`、`returnedOnlyWhenOpenedInTime`、`timedOutOnlyWhenNotOpenedInTime` |
| `receive_while.qnt` | キュー、到着、上限（なし・0〜3）、呼ぶ前の `last`、受け入れたものを拾う、上限に達する、拒んだものを戻す、時間切れ | `nothingLostOrReordered`、`pickedOnlyAccepted`、`withinLimit`、`lastIsLastPicked` |
| `watch_handle.qnt` | 結果（Pending・Cancelled・Expired）、start、cancel の要求と戻り（終わったのを見て戻る・時間切れで戻る、最大 3 回）、expire、報告（試していない・書けた・失敗した）、finish | `outcomeNeverChanges`、`reportOnlyWhenExpired`、`cancelledWritesNothing`、`finishedConsistent`、`cancelDecides`、`cancelWaitsForFinish` |

`test_latch.qnt` の待つ側は、実装のループと同じく「開いた時刻を読み、期限と比べて決める」。不変条件は、決めた結果と、読んだ時刻・期限・現在の時刻の関係を、決め方とは別の式で書いている。

検査の結果（quint 0.32.0）：

| コマンド | 結果 |
|:---|:---|
| `quint test test_latch.qnt` | 5 件 pass |
| `quint test receive_while.qnt` | 4 件 pass |
| `quint test watch_handle.qnt` | 6 件 pass |
| `quint run <各仕様> --invariant=invariant --max-steps=30 --max-samples=20000` | 3 本とも `[ok] No violation found` |

不変条件が実際に違反を捕まえることも確かめた。わざと壊した写しを 11 本作り、同じ `quint run` を流した。11 本とも `[violation]` になった。

| 仕様 | 壊し方 |
|:---|:---|
| test_latch | `reset` でラッチを開ける（設計レビューの M3）。`countDown` を 0 で止めない。開いた時刻を見ずに数だけで戻る。`reset` で開いた時刻を消さない |
| receive_while | 拒んだメッセージを戻さずに捨てる。上限 0 で `last` を消す。上限を見ない |
| watch_handle | cancel が期限切れの結果を上書きする。cancel が終わりを待たずに「終わった」として戻る。報告を試さずに終わる。expire が Pending かを見ない |

Quint の検査は CI に入れていない（CI に quint がない）。仕様を変えたら、上のコマンドを手で流す。

## PBT の性質

性質は、範囲を区切った入力をすべて試す形で確かめる（乱数で抜き出すのではなく、全件を流す）。新しい依存は足していない。除外はない。

| 性質 | 入力の全域 | テスト |
|:---|:---|:---|
| P1：`is_open` は、数のモデル（0 で止まる数え下げ、`open` で 0、`reset` で `count`）が 0 のときに限り真 | `count` は 0〜4、操作の列は長さ 0〜5（3 種類の操作のすべての並び） | `test_latch_test.rs` の `is_open_follows_the_counter_model_for_every_bounded_sequence` |
| P2：`receive_while` は、受け入れたものが続く先頭の部分を上限で切って返し、残りは順に残り、`last_message` は最後に拾ったもの（上限 0 なら呼ぶ前の値） | キューは長さ 0〜4、値は 0〜3（偶数を受け入れる）、上限はなし・0〜4 | `test_kit_value_test.rs` の `receive_while_takes_the_accepted_prefix_and_keeps_the_rest_for_every_bounded_queue` |
| P3：cancel と期限切れのどちらが先でも、結果が `true` なら出力は空、`false` なら報告が最後まである（通知の行で始まり、題名とスタックの見出しを含み、Coroner のスレッドが最後の flush を済ませている） | watch の時間 0〜6 ms と、cancel までの遅れ 0〜6 ms のすべての組 | `coroner_test.rs` の `the_outcome_and_the_output_agree_whichever_of_cancel_and_expiry_comes_first` |

P3 は時刻の競合なので、どちらが先になるかは実行ごとに変わりうる。そこで、両方の側を数え、全組を流した後に、どちらの側も 1 回以上通ったことを assert する。時間 6 ms・遅れ 0 ms の組は cancel が先に、時間 0 ms・遅れ 6 ms の組は期限切れが先になるので、ふつうはどちらの側も通る。

## 壊しうる入力の一覧

| 入力 | 扱い | 確かめたテスト |
|:---|:---|:---|
| `TestLatch::new(0, ..)` | はじめから開いている | P1（count 0） |
| 開いたラッチへの `count_down` | 0 のまま | `counting_down_an_open_latch_keeps_it_open` |
| `ready` の `max` が 0 | 開いていれば戻り、閉じていれば失敗する | P1 の範囲外。最初にラッチを見る作りで保証している |
| 時間の倍率（time factor）が 3 | 待ちが 3 倍になる | `ready_waits_for_the_dilated_max`、`receive_while_dilates_max` |
| 期限をまたぐ配送の最後にラッチが開く | 時間切れで失敗する | `ready_fails_when_the_latch_opens_only_after_the_deadline` |
| アクターの中・監督の判断の中からの `ready` | 扱えない（テストのスレッドから呼ぶ）。後続の issue で扱う | なし（公開 API への影響の節） |
| 時間切れを確かめてテストを続ける | `catch_unwind` で受ける | `ready_fails_with_the_max_and_the_time_factor_when_the_latch_stays_closed` |
| 待っている間の `reset` | 数を共有するので、待つ側にも効く（原典との差異） | `test_latch.qnt` の `resetDuringWaitClosesAgainTest` |
| ほかのスレッドからの `count_down` | `ready` が 10 ms 以内に気づく | `ready_returns_when_another_thread_opens_the_latch` |
| アクターの中での `count_down` | `ready` がシステムを動かすので開く | `ready_runs_the_system_so_an_actor_can_open_the_latch` |
| 終了したシステムでの `ready` | 開いていればすぐ戻る | `ready_returns_on_a_terminated_system_when_the_latch_is_open` |
| `messages(0)` | 何も取らずに戻り、`last_message` は呼ぶ前のまま | P2、`receive_while_with_a_limit_of_zero_keeps_the_previous_last_message` |
| `pick` が遅く、期限を過ぎる | キューにあるものは取り続ける（Pekko と同じ） | なし（Pekko の `receiveOne` と同じ作り） |
| 空のキュー、最初から拒むメッセージ | 空を返し、メッセージは残る | P2、`receive_while_with_no_match_returns_nothing_and_leaves_the_message_queued` |
| `idle` が `max` より短い | 間が空いたところで終わる | `receive_while_stops_when_idle_passes_between_messages` |
| `max` なしで within の中 | within の残り時間を使う | `receive_while_without_max_uses_the_remaining_time_of_within` |
| 期限 0 ms の watch、すぐの cancel | P3 の通り | P3 |
| 期限切れの後の cancel | 結果は `false` のまま | `cancelling_an_expired_watch_keeps_it_expired` |
| 何度も・複数のスレッドからの cancel | 最初に決まった結果のまま | `watch_handle.qnt` の `repeatedCancelKeepsTheOutcomeTest` |
| 書き込みに失敗する出力 | `print_report` は最初のエラーを返す | `print_report_returns_the_error_of_a_failing_output` |
| `max` が短い `result` | 決まっていなければ `false` | `result_of_a_running_watch_is_false_when_max_passes_first` |

## 原典との差異

- `TestLatch` は、Pekko では暗黙のシステムから time factor を読む。port は `TestKitSettings` を受け取る。time factor を持つのは、システムではなく spec の設定だからである。待つときにシステムを動かす必要があるので、`ready` はシステムを引数に取る（Pekko では `Await.ready(latch, max)` で、アクターはディスパッチャーの上で勝手に動く）。
- `ready` は、テストのスレッドから呼ぶ。Pekko では、アクターや監督者の中からも `Await.ready` で待てる。
- `TestLatch::reset` は、数を共有したまま `count` に戻す。Pekko は中の `CountDownLatch` を新しいものに差し替えるので、`reset` の前から待っている側は古いラッチを待ち続ける。このため、次の 2 点が違う。(a) ほかのスレッドが一度 0 にした直後に `reset` すると、port の待つ側は、それを見逃して時間切れになることがある（Pekko は戻る）。(b) `reset` の後の `count_down` で 0 になると、port の待つ側は戻る（Pekko は戻らない）。
- `TestLatch::ready` は、期限の後に開いたラッチを時間切れとして扱う。Pekko の `CountDownLatch.await` と同じ結果になるように、開いた時刻を記録して比べる。
- `receive_while` の中の `receive_one` は、within の期限で待ちを短くする。Pekko の `receiveOne` は短くしない。このため、within より長い `max` を渡したときは、Pekko より早く、within の終わりで戻る。期限を過ぎてもキューにあるメッセージを取り続ける点は、Pekko と同じである（`receiveOne` は残り時間が 0 以下でも `pollFirst` でキューから取る。TestKit.scala L788-796、L844-852）。
- `receive_while` の `messages(0)` は、Pekko と同じく、呼ぶ前の `last_message` を残す（最初は消していたので、合わせた）。
- Coroner の cancel と期限切れが競ったとき、port は報告を書かない。Pekko は、期限切れを確かめてから `expired()`（`trySuccess(false)`。失敗は無視する）を呼ぶまでの間に cancel が勝つと、結果は `true` なのに報告を書く。port は、結果を決めることと報告を書くかどうかの判断を、1 つのロックの中で行う。このため、P3 の「`true` なら出力は空」は port だけの性質である。
- Coroner の報告を書いている途中で出力が失敗したとき、Pekko は `NonFatal` を受けて「Error displaying Coroner's Report」とスタックを書こうとする。port は、`print_report` の失敗なら `Error displaying Coroner's Report: <error>` を書こうとし、それも失敗したら残りを書かずに終わる。どちらでも、スレッドは flush を試してから終わる。
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
| `cargo test -p fraktor-testkit --lib` | 84 passed（修正 1 の後） |
| `cargo test --workspace` | 77 スイート、941 passed / 0 failed（修正 1 の後） |
| `cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --check` | 成功 |
| syn-check（`crates/fraktor-testkit`） | 違反 0、抑制 0 |
| `npm run check:aislop` | 既知の指摘と一致 |
| `verify_green.py --crate fraktor-testkit` | `all_pass: true` |
| `pekko_baseline.py check-ledger`、`check-refs`、`relocate_nodes.py check` | 成功 |
| `report_ledger_1to1.py check` | 成功（未記載の一覧に 2 ノードを `no-report` で足した） |
| `check_needs.py --goal G0issue180`、`rollup.py --goal G0issue180` | needs 閉性 OK、`{'green': 2}`、完了 |

## レビューの指摘の扱い（修正 1）

設計の独立レビュー（実装に関わっていない claude-opus-5-5、`95719005` を対象。全文は起動元の `/Users/j5ik2o/orca/evidence/fraktor-ng-rs-pr211-95719005/review.md`）と、Devin のレビュー（`95719005`、5 スレッド）を受けた。直す・別件・採らないを、1 件ずつ決めた。

### 設計の独立レビュー（12 件）

| 指摘 | 扱い | 根拠と、したこと |
|:---|:---|:---|
| 1. アクターの中・監督の判断の中では `ready` を呼べない | 別件（起動元が #197・#201 に送る）。この文書は直した | 公開 API への影響の節を「テストのスレッドから待つ場合は」に絞り、制約として書いた。原典との差異と、壊しうる入力にも足した。`TestLatch` の rustdoc にも書いた |
| 2. `ready` の時間切れは panic しかない | 別件（起動元が #200 に送る）。この文書は直した | `catch_unwind` で受ける形を、能力の範囲として書いた。その形のテストはある（`ready_fails_with_the_max_and_the_time_factor_when_the_latch_stays_closed`）。`Result` を返す版は、後続で要るとわかってから足す |
| 3. `test_latch.qnt` に待ちと時間切れがなく、`readyOnlyWhenOpen` が定義から成り立つ | 直す | 待つ側（Idle・Waiting・Returned・TimedOut）、期限、時刻、開いた時刻を足した。`count` は 0〜3 で選ぶ。`reset`・`countDown` は待っている間にも起きる。不変条件に `resetRestoresCount`、`countDownSaturates`、`openMeansZero` を足した。M3（`reset` が開ける）を捕まえる。(a)(b) は原典との差異に書いた |
| 4. `watch_handle.qnt` の cancel が finish を待たない | 直す | cancel を要求と戻り（終わったのを見て戻る・時間切れで戻る）に分け、最大 3 回まで繰り返せるようにした。不変条件 `cancelWaitsForFinish` を足した。待たずに戻る写しを捕まえる |
| 5. cancel と期限切れの競合の扱いが原典より厳しい | 直す（差異として書いた） | 原典との差異の節 |
| 6. 出力が失敗すると「報告をすべて書き終えている」が成り立たない | 直す | 不変条件を「書き込みを試し終えている」に直した。仕様に、書けた・失敗したの 2 つの経路を足した（`failedOutputStillFinishesTest`）。差異の節にも書いた |
| 7. `messages(0)` の `last_message` が原典と違う | 直す（Pekko に合わせた） | 上限 0 のときは `last_message` を変えない。先にテストを足して red を確かめた（`receive_while_with_a_limit_of_zero_keeps_the_previous_last_message`、left `None` / right `Some(5)`）。P2 と `receive_while.qnt`（呼ぶ前の `last`、上限なし・0〜3）も合わせた |
| 8. P3 が競合の両方の側を通ったか確かめていない | 直す | 両方の側を数え、どちらも 1 回以上あることを assert する |
| 9. `provided_by` の能力を、ゴールのどのノードも使っていない | 直す（文書） | 能力を裏づけるテストの名前と、能力の範囲（指摘 1・2）を、公開 API への影響の節に書いた。`provided_by` は `G0issue180` のままにする。能力を作って確かめたのはこのゴールで、後続の issue は red を確かめる段階でこの能力が足りるかを見る |
| 10. 承認の一次情報 | 確認済み | 出所は「元帥の項目365（e5608）」「元帥の項目366（e5619）」と書いてある（判断の記録の節、台帳の `reason`）。起動元の指示どおりで、足すものはない |
| 11. 状態遷移なのに Quint が無いところ | 直す（3 と 4 で）。`await_cond` は採らない | TestLatch の待ちと時間切れ、cancel の待ちは、3 と 4 で仕様に入れた。`await_cond` は、前からある `TestKitBase::await_cond` に委ねるだけで、新しい遷移を足していないので、仕様は作らない。`receive_one` が within で待ちを短くすることは、`timeOut` がいつでも起きる形で含んでいる |
| 12. 設計が実装の後に書かれた | 採らない（記録だけ） | 経緯は冒頭に書いてある。2026-11-25 の決まりの変更のときに、実装はほぼ終わっていた |

### Devin のレビュー（5 件）

| スレッド | 扱い | 根拠と、したこと |
|:---|:---|:---|
| `test_kit_base.rs:237` 期限を過ぎても `receive_while` が取り続ける | 採らない | Pekko も同じである。`receiveWhile` は `receiveOne((stop - now) min idle)` を呼び（TestKit.scala L795）、`receiveOne` は残り時間が 0 以下でも `queue.pollFirst` でキューにあるメッセージを取る（L844-852）。期限の後に取ったメッセージも結果に入る。rustdoc に、Pekko と同じであることを書いた |
| `test_latch.rs:98` 期限の後に開いたラッチで `ready` が成功する | 直す | ラッチが、数と同じロックの下で、数が 0 になった時刻を記録する。`ready` は、その時刻が期限以前のときだけ戻る。`reset` は時刻を消す。先にテストを足して red を確かめた（`ready_fails_when_the_latch_opens_only_after_the_deadline`）。`test_latch.qnt` にも同じ判断を入れた |
| `test_latch.rs:74` `reset` の後の `count_down` が待っている側にも効く | 直す（契約として書いた） | `TestLatch` の rustdoc と、原典との差異の節に、(a)(b) として書いた |
| `test_kit_base.rs:251` 上限 0 でも直前のメッセージを忘れる | 直す | 設計の独立レビューの指摘 7 と同じ |
| `coroner_test.rs:154` P3 が報告の完了を見ていない | 直す | テストの出力先が flush を記録する。Coroner のスレッドは、書き終えてから flush する。P3 は、期限切れの側で flush が済んでいることと、通知の行、題名、スタックの見出しがあることを確かめる |
