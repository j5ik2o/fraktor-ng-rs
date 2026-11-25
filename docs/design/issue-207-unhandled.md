# 設計：#207 actor kernel の unhandled 通知（Props::empty・ActorContext::unhandled・UnhandledMessage）

[Issue #207](https://github.com/j5ik2o/fraktor-ng-rs/issues/207) の設計の記録である。原典は Pekko v2.0.0
（commit `9fbd2283b1cf701c90a1223bf621942c191db61f`）。この issue は
[Issue #180](https://github.com/j5ik2o/fraktor-ng-rs/issues/180) から分割した [1a] で、
[Issue #210](https://github.com/j5ik2o/fraktor-ng-rs/issues/210) が `PekkoSpecSpec` を green にするための
kernel の土台である。

状態：実装済み（[PR #217](https://github.com/j5ik2o/fraktor-ng-rs/pull/217)）。設計の独立レビュー：実施済み
（2026-11-26、判定は要修正）。指摘 F1〜F5 の処理と、それで変わった箇所は §11 にある。

## 1. 何を変えるか、何を変えないか

変えるもの（すべて `crates/fraktor-actor`。kernel のままで no_std を保つ）：

| ファイル | 中身 |
|:---|:---|
| `src/kernel/actor/props.rs` | `Props::empty()` と、その工場が作る非公開の `EmptyActor` |
| `src/kernel/actor/actor_context.rs` | `ActorContext::unhandled<M: Any + Send + Sync>(&mut self, message: M)` |
| `src/kernel/executor/executor_value.rs` | `publish_unhandled`（event stream への `UnhandledMessage` と、event sink への `Warning`） |
| `tests/unhandled_notification_spec.rs` | 追加した公開 API のテスト（新規） |
| `src/kernel/actor/props_test.rs` | `Props::empty` の単体テスト |

`UnhandledMessage` は既に `kernel::actor` にあり、`EventStream` の
`start_unhandled_message_forwarder` も既にある。この issue は、アクターが「扱わなかった」と
申告する口と、その申告を `UnhandledMessage` として stream に流す経路を足す。

変えないこと：

- 既存の公開 API のシグネチャと振る舞い。足すのは新しいメソッドと、それを実現する非公開の型だけ。
- `Actor::receive`、ライフサイクルフック、behavior stack、stash、FSM、監督の判断、終了フック、
  dead letters の経路。`unhandled` はどれも変えない。
- `PekkoSpecSpec` の Rust テストファイルと、台帳の `PekkoSpecSpec` ノード。後者は
  [Issue #210](https://github.com/j5ik2o/fraktor-ng-rs/issues/210) の完了条件が green にすることで、
  この issue は 4 ケースのうち 0 ケースを移植する土台である（§8）。
- 範囲の外：原典 `Actor.unhandled` の `Terminated` 分岐（`DeathPactException`）。§2 に理由を書き、
  [Issue #219](https://github.com/j5ik2o/fraktor-ng-rs/issues/219) で追跡する。

## 2. 原典との対応

| 原典（v2.0.0） | 移植 |
|:---|:---|
| `Props.empty = Props[EmptyActor]()`、`EmptyActor.receive = Actor.emptyBehavior`（`Props.scala` L51、L60-62） | `Props::empty()` が `EmptyActor` を作るレシピを返す。`EmptyActor::receive` は毎回 `context.unhandled(message)` を呼ぶ |
| `Actor.unhandled(message)`（`Actor.scala` L658-663）の `case _ => eventStream.publish(UnhandledMessage(message, sender(), self))` | `ActorContext::unhandled` が `UnhandledMessage(message, sender(), self_ref())` を event stream に publish する |
| `UnhandledMessage(message, sender, recipient)`（`Actor.scala` L297-303） | 既存の `UnhandledMessage` をそのまま使う。フィールドは message・sender・recipient |
| `TestEventListener` の `case UnhandledMessage(msg, sender, rcp) => Warning(rcp.path.toString, rcp.getClass, s"unhandled message from $sender: $msg")`（`TestEventListener.scala` L582-584） | event sink（port の TestEventListener 相当。§7）へ `Warning` を出す。前置き（`unhandled message from `）は原典と同じ。違いは下の 3 点 |

原典の `Actor.unhandled` はもう 1 つ分岐を持つ。`Terminated` を受けたら `DeathPactException` を投げる。
port の失敗モデルは panic の文字列を監督の判断材料にするもので、`DeathPactException` に当たる型も、
それを Stop にする既定の判断もまだ無い。この分岐は監督の状態遷移を作るので、この issue では扱わない。
`PekkoSpecSpec` の「warn about unhandled messages」は `42` を送るだけで、この分岐を通らない。
この分岐は [Issue #219](https://github.com/j5ik2o/fraktor-ng-rs/issues/219) で移植する。それまでは
`unhandled` に渡した `Terminated` も他のメッセージと同じく報告し、アクターは生き続ける。最初に
これが必要になるのは `DeathWatchSpec`（unported）の「fail a monitor which does not handle Terminated()」で、
台帳の `DeathWatchSpec` ノードに #219 への注記を置いた（移植の担当が待ちの失敗をケースの除外で
済ませないように）。

`Warning` の文面は、原典 `TestEventListener` と次の 3 点で違う。どれも既存の
`start_unhandled_message_forwarder` と同じ表記で、#210 が使う `start = "unhandled message"` の
前置きは一致する。

| 項目 | 原典 | port |
|:---|:---|:---|
| sender の表記 | `ActorRef.toString`（`Actor[pekko://sys/deadLetters]`、uid があれば `#uid` が付く） | `ActorRef::path()` の表記だけ |
| message の表記 | `toString` | `Message` の `Debug`。`i32` は数字、`&'static str` は引用符つき、それ以外の型は `<message of an unknown type>` |
| log class | `rcp.getClass`（受け取ったアクターのクラス） | `ACTOR_LOG_CLASS` |

## 3. 守る不変条件

`U(m, s, r)` を、メッセージ `m` を送り主 `s` から受け取ったアクター `r` について stream に出る
`UnhandledMessage` とする。

| 番号 | 不変条件 |
|:---|:---|
| I1 | `Props::empty()` のアクターは、届いたメッセージ 1 件につき `U` をちょうど 1 件出す（落とさない、重ねない） |
| I2 | `U.message` は届いたメッセージと同じ値・同じ型。`U.sender` はその配信の sender。`U.recipient` は受け取ったアクター |
| I3 | `unhandled` はアクターの状態を変えない。呼んだ後も behavior stack、現在のメッセージ、stash、FSM の状態は同じで、次のメッセージは同じように届く。behavior stack と stash はテストで確かめる（P5、P6）。FSM の状態は `Fsm` の値が持ち、`ActorContext` からは届かないので、`unhandled` が変えられないことは型で決まる |
| I4 | event sink があるとき、`U` 1 件につき `Warning` を 1 件出す。文面は `unhandled message from <sender の path>: <message の Debug>`。sink が無ければ何も出さない（panic しない） |
| I5 | event stream に購読者がいなくても、sink が無くても、`unhandled` は panic しない |

## 4. PBT の性質

性質は、入力を範囲で区切って全件流す（乱数で抜き出さない）。新しい依存は足さない。除外はない。

入力の全域（P1〜P3 で共通。テストの `Input::all()` と `senders()`）：

- メッセージ 9 種：`i32` の -2〜2、`&'static str` の 3 種（`"alpha"`、空文字列、`"with spaces"`）、独自型 `Custom(7)` 1 種
- sender 3 種：dead letters、test actor、別アクター（`TestProbe`）
- 組：メッセージ 9 種 × sender 3 種 = 27 組を、すべて同じアクターへ順に送る

| 番号 | 性質 | 入力 | テスト |
|:---|:---|:---|:---|
| P1 | `Props::empty()` は、型の違うメッセージを含むどのメッセージにも `U` をちょうど 1 件出し、message（値と型）・sender・recipient が一致する | 27 組 | `props_empty_reports_every_message_once_for_every_bounded_input` |
| P2 | `ActorContext::unhandled` を明示的に呼ぶアクター（`Rejector`）でも、P1 と同じ 1 対 1 が成り立つ | 27 組 | `actor_context_unhandled_reports_the_message_and_sender` |
| P3 | event sink があるとき、`U` 1 件につき `Warning` が 1 件、送った順に出る。source は受け取ったアクターの path、文面は `unhandled message from <sender の path>: <message の Debug>` と完全に一致する | 27 組 | `unhandled_is_reported_as_a_warning_to_the_event_sink` |
| P4 | `unhandled` を繰り返しても、I3 のとおりアクターは生き続け、届いた順に 1 件ずつ報告する。同じ sender から同じメッセージを続けて 2 件送ると、`U` は 2 件（重ねない、まとめない） | 27 組の後に、dead letters から `0` を 2 件 | P1、P2 のテスト（`assert_every_pair_is_reported_once`） |
| P5 | `push_behavior` で積んだ behaviour の中で `unhandled` を呼んでも、その behaviour が次のメッセージも受け取る（I3 の behavior stack） | `0`（積む）、`1`、`2` | `unhandled_keeps_the_pushed_behavior` |
| P6 | `stash` の後に `unhandled` を呼んでも、`unstash_all` で stash した全件が同じ順で戻る（I3 の stash） | `1`、`2`、`3` を stash し、`"release"` で戻す | `unhandled_keeps_the_stash` |

`Message` の Debug は `&'static str` を引用符つきで出し、知らない型には値を出さない（§2 の表）。
P3 はこの表記まで含めて文面を完全に比べる。I5（sink も購読者も無いとき panic しない）は
`unhandled_without_subscribers_or_a_sink_does_not_panic` で確かめる。`FraktorSpec` は sink を付けるので、
P1〜P3 では sink の無い経路を通らない。

「1 件につき 1 件」は、報告を 1 件取った直後に長さ 0 の `expect_no_message` を置いて確かめる。
同じ配信で作られた 2 件目は、1 件目を受け取る前に probe の待ち行列に入っているので、待たずに見える。
まとめて送るテスト（P4 の 2 件、P5、P6）は、全件を取ってから 1 回だけこの確かめを置く。

変異で確かめた（2026-11-26、`unhandled_notification_spec` を流した）：

| 変異 | 結果 |
|:---|:---|
| `unhandled` の最後に `pop_behavior` を足す（独立レビューの M8。以前は 5 件すべて通った） | P5 が `["base:0", "pushed:1", "base:2"]` で落ちる |
| `unhandled` の中で `unstash_all` を呼ぶ | P6 が終わらない（戻したメッセージが再び stash と報告に回り続ける）。CI では時間切れで落ちる |
| stream へ 2 回 publish する（M1） | P1、P2、P5、P6 が落ちる |
| `UnhandledMessage` の sender と recipient を取り違える（M2） | P1、P2、P5、P6 が落ちる |

## 5. 壊しうる入力

| 入力 | 扱い | 確かめたテスト |
|:---|:---|:---|
| `i32` の負・0・正、`&'static str`（空文字列を含む）、独自型 | どれも 1 件の `U` になり、型が残る | P1、P2 |
| `Message` をそのまま `unhandled` に渡す | `Message::new` が中身を返すので、二重に包まない | P1（`Props::empty` と `Rejector` は `Message` を渡す。`downcast_ref::<T>()` が通る） |
| sender が dead letters・test actor・別アクター | `U.sender` はそのアクター。`Warning` の文面もその path | P1、P2、P3（27 組） |
| 同じメッセージを続けて送る | 2 件の `U`（重ねない） | P4 |
| 積んだ behaviour の中から呼ぶ | behaviour は残り、次のメッセージも受け取る | P5 |
| `stash` の後に呼ぶ | stash は残り、`unstash_all` で全件戻る | P6 |
| 購読者のいない stream、sink の無いシステム | 何も起きず panic しない | `unhandled_without_subscribers_or_a_sink_does_not_panic` |
| `unhandled` をメッセージの外（`pre_start` など）で呼ぶ | sender はその時点の sender（通常は dead letters）。panic しない | なし（契約は `sender()` のとおり。呼び出し経路を作らない） |
| `Terminated` を渡す | `U` を出し、アクターは生き続ける（原典は `DeathPactException` で落ちる）。§2 の範囲外 | なし（[Issue #219](https://github.com/j5ik2o/fraktor-ng-rs/issues/219)） |
| `Props::empty` のアクターを `UnhandledMessage` または全イベントの購読先にする | 通知が購読先に届き、空アクターがそれをまた未処理として報告するので止まらない。原典も `aroundReceive` が `emptyBehavior` に一致しないメッセージを `unhandled` にするため同じ循環になる。port は単一スレッドなので、`run_until_idle` が戻らない形で表に出る（独立レビューで実測）。利用側の誤用であり、購読先には未処理通知を扱うか無視するアクターを使う（`Props::empty` の doc コメントにも書く） | なし（原典と同じ。直さない） |

## 6. 状態遷移と Quint

状態遷移に関わらない。`unhandled` は event stream と event sink に 1 件 publish するだけで、アクターの
behavior stack、stash、FSM の状態、監督の判断、終了フック、dead letters のどれも変えない。
`Props::empty` はレシピを作るだけで、アクターを作る経路は既存のままである。Quint の仕様は置かない。

## 7. event sink に `Warning` を出す理由

原典では、`UnhandledMessage` を `Warning` に変えるのは `TestEventListener`（テスト用の logger）である。
port には logger を actor で動かす層が無く、テスト側の観測点は `ActorSystem::set_event_sink` に渡す
`EventSink` である（`EventFilter` はこの sink を通して数える）。そこで、原典と同じく
`UnhandledMessage` を event stream へ出し、加えて `Warning` を event sink へ出す（文面の前置きは
原典と同じ。違いは §2 の表）。
sink はテストが付けたときだけ存在する（`FraktorSpec` が付ける）ので、本番の経路（sink 無し）は
原典どおり `UnhandledMessage` だけになる。sink を付けたときに `Warning` が見えるのは、原典が
`TestEventListener` を logger に据えたときと同じ観測結果であり、`EventFilter.warning()` がこの形で
使える。kernel は既に、監督の失敗でも `EventStream` と event sink の両方へ出している
（`publish_child_failure`）。

`UnhandledMessage` を `Warning` に変えるのは kernel の `publish_unhandled` だけである。testkit は
2 つ目の変換を足さない。#210 以降で `TestEventListener` を原典どおりに移植し、stream の
`UnhandledMessage` を購読して `Warning` に変える形にすると、未処理 1 件に `Warning` が 2 件出て、
ちょうど 1 件を求める `EventFilter::intercept` が「found 2」で落ちる。その形に移すときは、
`publish_unhandled` の sink への出力を消すのと同じ変更で行う。

`set_event_sink` は公開 API なので、テスト以外の sink を付けると、未処理 1 件ごとに `Warning` が出る
（原典の既定の logger では `DeadLetterListener` が info で、件数の上限つきで出すだけである）。
今の呼び出し元は testkit だけである。sink が未処理の `Warning` を受け取ることは
`ActorSystem::set_event_sink` の doc コメントに書いた。

## 8. 台帳

`PekkoSpecSpec` ノード（`testkit/src/test/scala/org/apache/pekko/testkit/PekkoSpecSpec.scala`）は
変更しない。理由：

- 原典のケースは 4 件で、この issue はそのうち 0 件を移植する（土台だけ）。green の条件は
  `passing == ported` かつ `ported + 除外 == total` なので、0/4 では green にできない。
- [Issue #210](https://github.com/j5ik2o/fraktor-ng-rs/issues/210) の完了条件が「PekkoSpecSpec を
  移植し、ノードを green にする」であり、ノードの持ち主は #210 である。
- 3 件の除外・作らないは人間の承認が要る。この issue では承認を得ていないので、件数を green に
  合わせる形では書かない（見込みや捏造を書かない）。

台帳のほかのノードも変更しない。`EventStreamSpec` の「be able to log unhandled messages」は既に
green で、この issue はその経路をアクターから使えるようにするだけである。

## 9. red の証拠

テストを製品より先に置き、実装前のツリーで流した（red のコミットは PR 本文に書く）。

```text
$ cargo test -p fraktor-actor --test unhandled_notification_spec
error[E0599]: no method named `unhandled` found for mutable reference `&mut ActorContext<'_>`
  --> crates/fraktor-actor/tests/unhandled_notification_spec.rs:44:13
error[E0599]: no associated function or constant named `empty` found for struct `Props`
  --> crates/fraktor-actor/tests/unhandled_notification_spec.rs (3 か所)
error: could not compile `fraktor-actor` (test "unhandled_notification_spec") due to 4 previous errors
$ cargo test -p fraktor-actor --lib
error[E0599]: no associated function or constant named `empty` found for struct `props::Props`
  --> crates/fraktor-actor/src/kernel/actor/props_test.rs (2 か所)
error: could not compile `fraktor-actor` (lib test) due to 2 previous errors
```

## 10. 検証（ローカル）

| コマンド | 結果 |
|:---|:---|
| `cargo test -p fraktor-actor --test unhandled_notification_spec` | 7 passed |
| `cargo test --workspace` | すべて pass |
| `cargo clippy --workspace --all-targets -- -D warnings` | 成功 |
| `cargo fmt --check` | 成功 |
| CI の Python テスト 12 本（`goal_complete_gate_test` ほか） | 成功 |
| `pekko_baseline.py check-ledger`、`check-refs`、`relocate_nodes.py check`、`report_ledger_1to1.py check` | 成功 |

`an_unhandled_message_is_one_warning_for_event_filter` は、`PekkoSpecSpec` の
「warn about unhandled messages」が使う `EventFilter.warning` の経路をそのまま通す。原典の
`start = "unhandled message"` の絞り込みは、port の `EventFilter::Warning` がまだ持たないので、
この issue では件数（1 件）だけを確かめる。絞り込みは #210 で必要になったら足す。

## 11. 設計の独立レビューの処理

レビューは 2026-11-26、実装に関わっていない担当が `b622798` で行った（判定：要修正。コードは正しい）。

| 指摘 | 決定 | 変わった箇所 |
|:---|:---|:---|
| F1 `Terminated` → `DeathPactException` の後回しが追跡されていない | 別件にして追跡を入れた | [Issue #219](https://github.com/j5ik2o/fraktor-ng-rs/issues/219) を作った。§1、§2、§5 の `Terminated` の行、`ActorContext::unhandled` の doc コメントから #219 へリンクした。台帳の `DeathWatchSpec` ノードに #219 への注記を置いた |
| F2 §4・§5 の入力がテストと合わない | テストを設計に合わせた | P1〜P3 は 27 組を全件流す（§4）。P3 は文字列と独自型まで文面を完全に比べる。sink の無い経路の確かめ先を `unhandled_without_subscribers_or_a_sink_does_not_panic` に直した（§4、§5）。状態の行を直した |
| F3 I3 のテストが無い | 直した | P5、P6 を足した（§3 の I3、§4）。M8 が P5 で落ちることを確かめた |
| F4 「同じ文面」が正確でない | 直した | §2 に、前置きが同じことと、違い 3 点の表を書いた。§7 も合わせた |
| F5 sink への変換が 1 か所だという前提が書かれていない | 直した | §7 に、変換は `publish_unhandled` だけで testkit は足さないこと、`set_event_sink` の公開 API としての影響を書いた。`set_event_sink` の doc コメントを足した |
