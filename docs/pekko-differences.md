# Pekko v2.0.0 との差の一覧

原典 Apache Pekko v2.0.0（基準は `docs/baseline/README.md`）から fraktor がずれている点を、
戻す前提で1か所に集めた一覧である。issue #311・#333・#335・#336・#337・#338・#339・#340・
#345・#347・#348・#350・#351・#352・#353・#356・#357・#361・#362 と、設計
`docs/design/issue-183-metrics-kit.md`（ブランチ `wip/299-design`）の「原典と違う点」の節、
`docs/design/issue-234-multithread-executor.md`（ブランチ `wip/300-stage4b-fix`）で
「原典と違う」と書いてある行を写した。区分・原典の行・戻す段は issue に書いてあることを
写しただけで、この文書では判断していない。原典の行が issue に無い欄は空欄にした。

この一覧の決まりは4つである。

- 一覧に無いずれは入れない。
- 一時的なずれのある台帳のノードは移植済みに数えない。
- 一時的なずれはほかの移植より先に戻す。
- PR で新しいずれを入れるときは、issue を立ててこの一覧に足してから。

（前の3つは元帥の決定、2026-11-28。）

## 一覧

| 区分 | 話題 | 原典の行（Pekko v2.0.0） | fraktor の今の振る舞い | issue | 戻す段・入る PR | 状態 |
|:---|:---|:---|:---|:---|:---|:---|
| 一時的（原典どおりに戻す） | 巻き込まれて再起動する兄弟の `pre_restart` の sender | `ActorCell.scala:615-618`、`dungeon/FaultHandling.scala:101・104`（コメント） | 兄弟の `pre_restart` が見る sender が自身のパスになる（`executor_value.rs` 3975 行付近のフォールバック）。main（ed9fba5）でも同じで、PR #300 が持ち込んだものではない | #311 | ずれを戻す列（`work/design-queue.md` の 6c、元帥の決定 2026-11-28：ほかの移植より先）。#300 のマージの後 | open |
| 一時的（原典どおりに戻す） | all-for-one で子を回す順（原典は名前の順） | `dungeon/ChildrenContainer.scala:120・137-138・173-174・200-201`、`dungeon/Children.scala:228` | 親の表の子の記録を `Vec` の挿入順で回す（`ParentEntry.children`、`children_of`）。上限のある all-for-one で数の尽きる子、兄弟への `Suspend`・`Recreate`・`Terminate` の順、`handleChildTerminated` に渡す並びが変わる。この差は main からあり、#300 で入ったものではない（設計 §11.13.5 の「順の差」） | #333 | ずれを戻す列。段4b の PR では直さない。issue の「PR #300 のマージの前に直す」は設計の文（§11.13.5 の「`forall` の短絡」の順の文）のことで、コードを戻す段ではない | open |
| 一時的（原典どおりに戻す） | ガーディアン自身の失敗と `/user` の停止 | `ActorRefProvider.scala:525-529・534・565-591・580`、`dungeon/FaultHandling.scala:305-325`、`ActorSystemSpec.scala:331-337・353-374・376-392` | ガーディアン自身の失敗では何もしない（セルもインスタンスも無く、再起動する対象が無い）。セルの無いガーディアンの decider の panic は、インスタンスのある子に `Resume { caused_by_failure: true }` を送り、開始に失敗した子はそのまま残す。ガーディアンの下のアクターは止まらずに残り、システムは終わらない（原典は `/user` が止まり、システムが終わる）。`guardian-supervisor-strategy` の設定を持たず、いつも既定の形になる。この差は main からある形のままで、#300 で入ったものではない | #335 | ずれを戻す列。`ActorSystemSpec` の該当 3 件の移植と一緒に。issue の「PR #300 のマージの前に直す」は設計の文（「原典のテストが頼りにしていない」を消すこと）のことで、コードを戻す段ではない | open |
| 一時的（原典どおりに戻す） | `pre_restart` の panic | `dungeon/FaultHandling.scala:299・105-107・109-111・114`、`ActorCell.scala:545-548・283-288`、`SupervisorSpec.scala:421-460` | `pre_restart` の panic で段3の巻き戻りによりシステムを終わらせる（原典は `PreRestartException` を publish して再起動を続ける）。`SupervisorSpec` は未移植（pending） | #336 | 後でよい（main からある差。直すのは #336、設計 §11.13.11） | open |
| 一時的（原典どおりに戻す） | 未停止のタイマーの公開の口 | `KnownOpsInTimespanTimer.scala:42-46`、`PekkoConsoleReporter.scala:128-136` | `elapsed()`・`avg_duration_nanos()`・`ops_per_second()` が `None` を返す。コンソールの報告は原典どおり負の値を出す（#299） | #337 | #299 の後（元帥の決定、2026-11-28。負の値の返し方は Opus が設計で決める） | open |
| 一時的（原典どおりに戻す） | 非 unix のファイルディスクリプタ | `FileDescriptorMetricSet.scala:32-44・48-63`、`MetricsKit.scala:108-115`、`FileDescriptorMetricSetSpec.scala:31-32` | `report_metrics()` が `Err` を捨て、公開の `FileDescriptorGauge::value()` が `NaN` を返す | #338 | #299 の後（元帥の決定、2026-11-28。例外は `Result` で呼び出し側へ返す） | open |
| 一時的（原典どおりに戻す） | `report_and_clear_metrics` の並行の登録 | `MetricsKit.scala:96-99` | 取り出した計測だけを消し、報告中に登録された計測を残す | #339 | #299 の後（元帥の決定、2026-11-28。既存テストの期待は原典の振る舞いに書き換えてよい） | open |
| 環境の違い（JVM 固有。写していないのではない） | 上限が無制限のときのファイルディスクリプタの上限の値 | | プロセスの rlimit を書き換えず `rlim_cur` を u64 から f64 にする。無制限では 9223372036854775807（表示 9223372036854776000）、soft < hard では soft。原典の JVM は HotSpot の `MaxFDLimit` で soft を上げるので、同じ環境でも値が違う | #340 | 直さない（環境の違い） | closed |
| 一時的（原典どおりに戻す） | 名前の重複の文面 | `dungeon/ChildrenContainer.scala:144` | `InvalidActorNameException::name_taken` の文面が "Actor name [{}] is not unique!"（先頭が大文字）。`extension_test.rs:114` がそのずれを固定している | #345 | ずれを戻す列（`work/design-queue.md` の 6c、元帥の決定 2026-11-28：ほかの移植より先）。#300 のマージの後 | open |
| 一時的（原典どおりに戻す） | `mark_dying` を書く呼び手 | `dungeon/Children.scala:132-145` | `Executor::stop` が呼び手を問わず親の表に印を付け、`finish_recreate` の失敗でも子の実行で付ける。親でない呼び手（止まるアクター自身、ほかのアクター、実行の外）の停止でも親の表に印が付く | #347 | ずれを戻す列（`work/design-queue.md` の 6c、元帥の決定 2026-11-28：ほかの移植より先）。#300 のマージの後 | open |
| 一時的（原典どおりに戻す） | 止まる途中で開始に失敗したアクターへの `Recreate` | | 止める（設計 §11.13.8）。原典は `faultCreate` で作り直し、作り直されたアクターは suspend のまま残る。既定の戦略では起きない | #348 | ずれを戻す列（`work/design-queue.md` の 6c、元帥の決定 2026-11-28：ほかの移植より先）。#300 のマージの後。段4c（#255）で届く筋書きが増える | open |
| 原典に定めが無い（fraktor の公開 API の選び） | 生成のときに渡した戦略と親の `supervisor_strategy` の優先 | `actor/FaultHandling.scala`、`dungeon/FaultHandling.scala` の `handleFailure`（行の記載なし） | 生成のときに戦略を渡した子の失敗は、その戦略だけで判断する（トップレベルでない親。decider があればその decider、無ければその policy で決め、all-for-one の範囲もその戦略のもの）。渡さなかった子は親のアクターの `supervisor_strategy` で判断する。判断する親がトップレベルのときは #300 の形（親の戦略か生成のときの戦略が all-for-one なら親の側を先に、親の戦略に独自の decider があればそれを先に、生成のときの戦略の decider は escalation と開始の失敗のときだけ、どれも無ければ生成のときの戦略の policy）。57ea251・d39b871 で規則とコードの食い違いは消えた（「トップレベルでない親では規則1から外れ、main から退行」は 46073e9 までのコードのこと） | #350 | #300 で規則（生成のときに戦略を渡した子はその戦略、渡さなかった子は親の `supervisor_strategy`、トップレベルの親は #300 の形）を入れた。規則を設計と rustdoc（`actor_of_with_strategy`、`spawn_child`、`try_spawn_child`）に書くのが #350 | open |
| 一時的（原典どおりに戻す） | `measure_file_descriptors` の衝突 | `MetricRegistry.java:595-603・164-169`（metrics-core 4.2.40） | 事前の全件検査で全か無か（設計の I8）。1件も登録せず `MetricsError::InvalidArgument` を返し、文面も "A metric named ... already exists" と違う | #351 | #299 の後に1件ずつ | open |
| 一時的（原典どおりに戻す） | `hdr_histogram` の種類の衝突と不正な引数 | `MetricsKit.scala:170-172`、`HdrHistogram.scala:34` | 異種の既存があれば検証も構築もせずに `KindConflict` を返す。返るエラーの列挙子が原典と違う | #352 | #299 の後に1件ずつ | open |
| 一時的（原典どおりに戻す） | `FileDescriptorGauge::value()` の型 | | 公開の `value()` が `f64` を返し、macOS の無制限の 2^63−1 が 2^63 に丸まる。報告は u64 の読みで書くので出力は原典と同じ（#299） | #353 | #299 の後に1件ずつ | open |
| 一時的（原典どおりに戻す） | 状態タイマの印の照合（`state_timer_armed` のガード） | `FSM.scala`（行の記載なし） | `fsm.rs:965` が世代に加えて `state_timer_armed` のガードで照合する。世代の `+= 1` を落とす誤りが正の run でも不変条件でも見えなくなる（fsm_timing.qnt の変異の検査） | #356 | ずれを戻す列（`work/design-queue.md` の 6c、元帥の決定 2026-11-28：ほかの移植より先）。#300 のマージの後 | open |
| 一時的（原典どおりに戻す） | `stopReason` が残っているあいだの `terminate` | `FSM.scala:934・888・940` | `terminate` が `stopReason` の残っている間も働き、原典より強い保証をしている。まだ走らせて確かめていない | #357 | ずれを戻す列（`work/design-queue.md` の 6c、元帥の決定 2026-11-28：ほかの移植より先）。#300 のマージの後 | open |
| 一時的（原典どおりに戻す） | 止めたあとの `is_state_timer_active` | `FSM.scala:636・939` | 止めたあと `is_state_timer_active` を偽にする（本番の `FsmHandle::clear` と Quint の `stopActor`・`stoppedNeverArms`・変異 M13）。原典は `terminate` が `timeoutFuture` を cancel するだけで `None` にせず、真のまま残りうる。原典より強い。寄せるなら本番と Quint を一緒に直す（設計 §6.4 の 6） | #361 | ずれを戻す列（`work/design-queue.md` の 6c）。#357・#356 と一緒に | open |
| 一時的（原典どおりに戻す） | 約248日を超えるタイマの期限 | `checkMaxDelay`（行の記載なし） | あふれない限り期限を受け付け、あふれたら黙って撃たない（`Duration::MAX` でも panic せず、撃たず、`is_timer_active` は真）。原典は約 248 日（Int の最大の tick）を超える期限を `IllegalArgumentException` で拒む | #362 | ずれを戻す列（6c）。#357・#356 と一緒に。#314 の P1 は `Duration::MAX` を入れない | open |
