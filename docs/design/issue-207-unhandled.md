# 設計：#207 actor kernel の unhandled 通知（Props::empty・ActorContext::unhandled・UnhandledMessage）

[Issue #207](https://github.com/j5ik2o/fraktor-ng-rs/issues/207) の設計の記録である。原典は Pekko v2.0.0
（commit `9fbd2283b1cf701c90a1223bf621942c191db61f`）。この issue は
[Issue #180](https://github.com/j5ik2o/fraktor-ng-rs/issues/180) から分割した [1a] で、
[Issue #210](https://github.com/j5ik2o/fraktor-ng-rs/issues/210) が `PekkoSpecSpec` を green にするための
kernel の土台である。

状態：実装前の設計（2026-11-26）。設計の独立レビュー：未実施（Opus 復帰後に実施）。

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
- 範囲の外：原典 `Actor.unhandled` の `Terminated` 分岐（`DeathPactException`）。§2 に理由を書く。

## 2. 原典との対応

| 原典（v2.0.0） | 移植 |
|:---|:---|
| `Props.empty = Props[EmptyActor]()`、`EmptyActor.receive = Actor.emptyBehavior`（`Props.scala` L51、L60-62） | `Props::empty()` が `EmptyActor` を作るレシピを返す。`EmptyActor::receive` は毎回 `context.unhandled(message)` を呼ぶ |
| `Actor.unhandled(message)`（`Actor.scala` L658-663）の `case _ => eventStream.publish(UnhandledMessage(message, sender(), self))` | `ActorContext::unhandled` が `UnhandledMessage(message, sender(), self_ref())` を event stream に publish する |
| `UnhandledMessage(message, sender, recipient)`（`Actor.scala` L297-303） | 既存の `UnhandledMessage` をそのまま使う。フィールドは message・sender・recipient |
| `TestEventListener` の `case UnhandledMessage(msg, sender, rcp) => Warning(rcp.path.toString, rcp.getClass, s"unhandled message from $sender: $msg")`（`TestEventListener.scala` L582-584） | event sink（port の TestEventListener 相当。§7）へ、同じ文面の `Warning` を出す |

原典の `Actor.unhandled` はもう 1 つ分岐を持つ。`Terminated` を受けたら `DeathPactException` を投げる。
port の失敗モデルは panic の文字列を監督の判断材料にするもので、`DeathPactException` に当たる型も、
それを Stop にする既定の判断もまだ無い。この分岐は監督の状態遷移を作るので、この issue では扱わない。
`PekkoSpecSpec` の「warn about unhandled messages」は `42` を送るだけで、この分岐を通らない。
残りとして PR 本文に書く。

## 3. 守る不変条件

`U(m, s, r)` を、メッセージ `m` を送り主 `s` から受け取ったアクター `r` について stream に出る
`UnhandledMessage` とする。

| 番号 | 不変条件 |
|:---|:---|
| I1 | `Props::empty()` のアクターは、届いたメッセージ 1 件につき `U` をちょうど 1 件出す（落とさない、重ねない） |
| I2 | `U.message` は届いたメッセージと同じ値・同じ型。`U.sender` はその配信の sender。`U.recipient` は受け取ったアクター |
| I3 | `unhandled` はアクターの状態を変えない。呼んだ後も behavior stack、現在のメッセージ、stash、FSM の状態は同じで、次のメッセージは同じように届く |
| I4 | event sink があるとき、`U` 1 件につき `Warning` を 1 件出す。文面は `unhandled message from <sender の path>: <message の Debug>`。sink が無ければ何も出さない（panic しない） |
| I5 | event stream に購読者がいなくても、sink が無くても、`unhandled` は panic しない |

## 4. PBT の性質

性質は、入力を範囲で区切って全件流す（乱数で抜き出さない）。新しい依存は足さない。除外はない。

| 番号 | 性質 | 入力の全域 | テスト |
|:---|:---|:---|:---|
| P1 | `Props::empty()` は、型の違うメッセージを含むどのメッセージにも `U` をちょうど 1 件出し、message・sender・recipient が一致する | メッセージは `i32` の -2〜2、`&'static str` の 3 種、独自型 1 種。sender は dead letters・test actor・別アクターの 3 種 | `props_empty_reports_every_message_once_for_every_bounded_input` |
| P2 | `ActorContext::unhandled` を明示的に呼ぶアクターでも、P1 と同じ 1 対 1 が成り立つ | 同上 | `actor_context_unhandled_reports_the_message_and_sender` |
| P3 | event sink があるとき、`U` 1 件につき `Warning` が 1 件出て、文面が sender の path と message を含む | `i32` の -2〜2 と 3 種の文字列 | `unhandled_is_reported_as_a_warning_to_the_event_sink` |
| P4 | `unhandled` を繰り返しても、I3 のとおりアクターは生き続け、届いた順に 1 件ずつ報告する | 3 件の列（`0,1,2`） | P1 のテストで同じアクターに連続で送る |

P1 と P2 は「1 件につき 1 件」を、同じアクターに複数件を送って確かめる。`Message` の Debug は
`&'static str` を引用符つきで出すので、P3 の文面の比較は `i32` だけで行い、文字列は「含む」で見る
（既存の `start_unhandled_message_forwarder` と同じ表記である）。

## 5. 壊しうる入力

| 入力 | 扱い | 確かめたテスト |
|:---|:---|:---|
| `i32` の負・0・正、`&'static str`、独自型 | どれも 1 件の `U` になる | P1 |
| `Message` をそのまま `unhandled` に渡す | `Message::new` が中身を返すので、二重に包まない | P1（`Props::empty` は `Message` を渡す） |
| sender が dead letters | `U.sender` は dead letters | P1 |
| sender が別アクター | `U.sender` はそのアクター | P1 |
| 同じメッセージを続けて送る | 2 件の `U`（重ねない） | P4 |
| 購読者のいない stream、sink の無いシステム | 何も起きず panic しない | P1 の一部（sink 無し） |
| `unhandled` をメッセージの外（`pre_start` など）で呼ぶ | sender はその時点の sender（通常は dead letters）。panic しない | なし（契約は `sender()` のとおり。呼び出し経路を作らない） |
| `Terminated` を渡す | `U` を出す（原典は `DeathPactException`）。§2 の範囲外 | なし |

## 6. 状態遷移と Quint

状態遷移に関わらない。`unhandled` は event stream と event sink に 1 件 publish するだけで、アクターの
behavior stack、stash、FSM の状態、監督の判断、終了フック、dead letters のどれも変えない。
`Props::empty` はレシピを作るだけで、アクターを作る経路は既存のままである。Quint の仕様は置かない。

## 7. event sink に `Warning` を出す理由

原典では、`UnhandledMessage` を `Warning` に変えるのは `TestEventListener`（テスト用の logger）である。
port には logger を actor で動かす層が無く、テスト側の観測点は `ActorSystem::set_event_sink` に渡す
`EventSink` である（`EventFilter` はこの sink を通して数える）。そこで、原典と同じく
`UnhandledMessage` を event stream へ出し、加えて同じ文面の `Warning` を event sink へ出す。
sink はテストが付けたときだけ存在する（`FraktorSpec` が付ける）ので、本番の経路（sink 無し）は
原典どおり `UnhandledMessage` だけになる。sink を付けたときに `Warning` が見えるのは、原典が
`TestEventListener` を logger に据えたときと同じ観測結果であり、`EventFilter.warning()` がこの形で
使える。kernel は既に、監督の失敗でも `EventStream` と event sink の両方へ出している
（`publish_child_failure`）。

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

`cargo fmt --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`、
CI の台帳検査（`pekko_baseline.py check-ledger`、`check-refs`、`relocate_nodes.py check`、
`report_ledger_1to1.py check`）と、ツールの Python テストを流す。結果は PR 本文に書く。
