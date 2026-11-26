# 設計：#296 と #297 dead letters の直しの残り（再送の入れ子、予備の道、閉じたメールボックスへの system message）

[Issue #296](https://github.com/j5ik2o/fraktor-ng-rs/issues/296) と
[Issue #297](https://github.com/j5ik2o/fraktor-ng-rs/issues/297) の設計である。親は
[Issue #286](https://github.com/j5ik2o/fraktor-ng-rs/issues/286) で、実装はプルリクエスト
[#295](https://github.com/j5ik2o/fraktor-ng-rs/pull/295)（マージ済み、基点は `526e4f2`）まで入っている。
原典は Pekko v2.0.0（`~/muse-work/pekko-v2.0.0` で読んだ）。

状態：設計の案。実装はこの文書ではしない。テストは、テストの担当（Opus）がこの文書の
「テストに要る面」に沿って先に red のテストを書き、実装がそのあとで通す。
意味論に関わる選択は §6 にまとめ、元帥が 2026-11-27 にすべて決めた。§6 は
「元帥の決定（2026-11-27）」として書き直してあり、実装はその決定に従う。

この文書に入れるものは 1 つである。

| ファイル | 中身 |
|:---|:---|
| `docs/design/issue-296-297-dead-letters-followup.md` | この文書 |

## 1. 目的と範囲

プルリクエスト #295 のあとの元帥の点検で残った直しは、次の 3 つである。

1. #296 の中 1：停止済みの宛先へ dead letters の購読者が送り直すと、送り手のスレッドでの
   配送が入れ子になり、際限なく続くとスタックがあふれる（元帥の調べ役の実験で深さ 51、
   送り直し 1000 回でスタックがあふれた）。`tell` が返った時点で購読者が受け取っている
   という今の同期の観察を保ったまま、入れ子を防ぐ形を決める。あわせて
   `crates/fraktor-actor/src/kernel/actor_ref/actor_ref_value.rs` の `tell` の rustdoc
   （基点で 91 行目付近）が事実と違う点を、実装と同時に直す。
2. #296 の中 2 と低：dead letters の経路が入っていないメールボックスの予備の道、
   取り出しの重なり、二重持ちの状態、要らない clone、経緯のコメント、弱い確かめ、
   bounded が満杯の経路のテスト。
3. #297：閉じたメールボックスへの system message を黙って捨てている箇所を、
   原典どおり dead letters へ流す形に直す。原典で確かめた形を §5 に書く。

範囲の外にするものは §5.5 に書く（システム全体の終了時の破棄）。

## 2. 今の形（基点 `526e4f2` で確かめた）

### 2.1 dead letters が送り手のスレッドで流れる道

- `ActorRef::send_message`（`kernel/actor_ref/actor_ref_value.rs`）は、メールボックスが
  封筒を断ると `Mailbox::route_refused` を呼ぶ。経路が入っているメールボックスでは、
  経路（`Executor::dead_letter_route` が作るクロージャ）がその場で
  `DeadLettersShared::route_undeliverable` を呼ぶ。
- `DeadLettersShared::publish`（`kernel/actor/dead_letters_shared.rs`）は
  `EventStreamBusWeak::publish` を呼び、バスは購読者を集めてから、各購読者へ
  `recipient.tell(message, recipient)` する（`kernel/event/event_stream_bus.rs` の
  `publish_through`）。購読者が関数参照なら、宛先は `FunctionBoundary::post` で、
  ハンドラがその場で最後まで走る。
- したがって、購読者のハンドラが停止済みのアクターへ送り直すと、上の道がそのまま
  再帰する。`tell` → 断り → 経路 → 公開 → 購読者のハンドラ → `tell` → … の 1 周ごとに
  スタックが積まれる。再帰を断つ仕組みは今は無い。`route_undeliverable` が
  `DeadLetter` 自体を落とす規則は、宛先が dead letters そのものの場合の循環を断つだけで、
  「購読者が別の停止済みアクターへ送り直す」循環は断たない。

### 2.2 予備の道

- `Mailbox::route_refused`（`kernel/dispatch/mailbox.rs`）は、経路が入っていないと
  封筒を呼び出し側へ返す。`ActorRef::send_message` は返ってきた封筒を
  `mailbox.hand_over(ManualTask::Envelope(refused))` で実行器へ渡し、実行器の
  `Executor::deliver` が dead letters へ流す。
- 本番の道では経路は常に入っている。アクターを生む箇所
  （`kernel/executor/executor_value.rs` の `spawn` 相当、基点で 940 行目）が
  `mailbox.set_refused_route(self.dead_letter_route())` を呼ぶ。経路が入っていない
  メールボックスを作るのは、カーネルの単体テストが `Mailbox::for_actor` を直接使う
  場合だけである（`kernel/dispatch/mailbox_test.rs`、`mailbox_run_test.rs`）。
- `Executor::deliver` にはもう 1 つ、経路を通らない分岐がある。境界から届いた封筒を
  生きている entry のメールボックスへ `enqueue` して断られた場合（基点で 2455 行目
  付近の `Err(refused)` 分岐）、`route_refused` を通さず
  `dead_letters_shared.route_undeliverable` を直接呼ぶ。この分岐は本番でも通る
  （entry が残っているのにメールボックスが閉じた、または bounded が満杯の場合）。

### 2.3 system message を黙って捨てる箇所

`SystemMessage` は `kernel/dispatch/system_message.rs` の列挙で、種類は §5.3 の表の
とおりである。閉じたメールボックスに関わる箇所は 3 つある。

1. `Executor::resume_if_open`（基点で 2100 行目付近）。`is_closed` を先に確かめてから
   `Mailbox::system_send` し、結果を `let _ =` で捨てる。単一スレッドの今の実行器では
   確かめと送信の間に割り込みは無いので `Err` はほぼ起きないが、#234 の複数スレッド化で
   競合が入ると、閉じたあとの `Resume` が黙って消える。先確かめ自体が競合に弱い形である。
2. `Executor::queued_to_dead_letters`（基点で 1680 行目付近）。アクターの停止時に
   `Mailbox::close` が返した system message を `drop(system)` で捨て、コメントに
   「段 3 では多くて `Resume` だけなので意味が無い」と書いてある。原典の
   `Mailbox.cleanUp` は、残った system message を 1 つずつ dead letter mailbox へ
   `systemEnqueue` する（§5.1）。
3. `Executor::stop_all`（基点で 2269 行目付近）。`held.push(entry.mailbox.close())` の
   結果をまとめて `drop(held)` する。これはシステム全体の終了であり、扱いは §5.5 で分ける。

`Mailbox::system_enqueue` 自体は、閉じていれば `Err(message)` で返すだけで、
dead letters への振り替えは呼び出し側の責任になっている。原典は逆で、
`Mailbox.systemEnqueue` が閉じた状態（`NoMessage` が頭）を見ると自分で
dead letter mailbox へ振り替える（§5.1）。振り替えの責任が 1 か所に無いため、
呼び出し側ごとに捨てる・流すが分かれた。

## 3. #296 中 1：入れ子を防ぐ形

### 3.1 候補

候補は 3 つある。どれも「停止済みの宛先への送信が dead letters へ流れ、購読者の
ハンドラが送り手のスレッドで走る」という出発点は変えない。

#### 候補 1（推す）：公開のトランポリン

`DeadLettersShared::publish` に、スレッドごとの待ち行列と「流している最中」の印を置く。

- 入っていない呼び出し（最も外側）が来たら、印を立て、公開しては待ち行列を順に流す
  ループに入る。ループは待ち行列が空になるまで回り、スタックの深さは一定に保たれる。
- 流している最中の呼び出し（入れ子）は、公開の実体を待ち行列の末尾へ積んで、すぐに返る。
  実体は最も外側のループが先入れ先出しで流す。
- 積む単位は「バスとイベントの組」のクロージャである。購読者の集め直し
  （`recipients_of`）は積んだ時点ではなく、流す時点で起きる（§3.3 の表の「変わる観察」）。
- 置き場所は `DeadLettersShared::publish` の 1 か所にする。理由は、dead letters の
  公開はすべてここを通るためである。断られた封筒の経路、実行器の `deliver`、
  メールボックスの後片付け（`publish_dead_letter`）、dead letters のハンドル自身への
  送信（`route_at_dead_letters`）のどれから入っても、入れ子は同じ待ち行列で平らになる。
- スレッドごとの状態は `std::thread_local!` で持つ。先例は
  `kernel/sync/counts.rs`（テスト用）と `adaptor/ask_driver.rs` にある。待ち行列は
  システムを区別しない。同じスレッドでシステム A の購読者がシステム B の停止済み
  アクターへ送る場合も、B の公開は A の最も外側のループが流す。送り手のスレッドは
  同じであり、順序はスレッド全体で先入れ先出しになる。システムごとに待ち行列を
  分ける案は、入れ子の入口ごとに印の照合が要り、壊れ方が増えるので採らない。
- パニックへの備え：購読者のハンドラがパニックで巻き戻ると、印が立ったまま残り、
  そのスレッドの以後の公開がすべて待ち行列に積まれて流れなくなる。ループは
  印と待ち行列を守る RAII の番人を持ち、巻き戻りでは印を下ろし、残った公開は捨てる。
  捨てることは、パニックした配送の続きが保証されないという今の致命の扱い
  （`Executor::lend` の `Lent`）と矛盾しない。
- `no_std` の形：`fraktor-actor` は `std` 機能が既定で、`no_std` でも使える。
  `thread_local!` は `std` が要る。`no_std` では、スレッドという概念自体が下の層の
  担当であり、送り手のスレッドでの同期配送も `std` の形である。設計は、
  トランポリンの状態を `#[cfg(feature = "std")]` の後ろに置き、`no_std` では
  今の直接公開（入れ子あり）へ落ちる形を推す。`no_std` で入れ子を防ぐ必要が
  生じたら、その時点で実行環境ごとの待ち行列の置き場所を別に決める。この点は
  実装の段で `no_std` の検査（既存の検査クレート）を通して確かめる。

#### 候補 2：深さの上限

公開の深さを数え、上限（たとえば 32）を超えた dead letter は落とす、または
上限で送り直しを止める。

- 実装は最も小さい。しかし、原典に上限は無い。上限を超えた letter は失われ、
  購読者が受け取る数が送り方で変わる。いくつを上限にするかにも原典の根拠が無い。
- 落とさずに上限で待ち行列へ逃がす形は、候補 1 のトランポリンに上限の数を足した
  だけであり、候補 1 で足りる。
- 採らない。選ぶ場合は、原典に無い振る舞いを足す理由と上限の数の根拠が要る
  （issue #296 の本文の求め）。元帥は上限を置かないことを決めた（§6 の決定 2）。

#### 候補 3：場所を絞った再帰断ち（断られた経路だけ、または関数参照の配送だけ）

- 断られた経路（`route_refused` の先）だけに印を置く案は、§2.1 の入口のうち
  1 つしか覆わない。購読者が dead letters のハンドルへ直接送る場合と、
  関数参照が閉じたあとの `FunctionBoundary::post` から dead letters へ行く場合は
  覆われず、循環の入口が残る。
- 関数参照の配送を原典どおり非同期の待ち行列に変える案は、原典の形には最も近いが、
  #286 で決めた同期の観察（`tell` が返った時点で購読者が受け取っている）を
  捨てることになる。今回の前提とぶつかるので採らない。

### 3.2 推す形の振る舞い

候補 1 で、送り直し 1000 回の実験は次のようになる。購読者のハンドラは、
受け取った letter の番号を見て、1000 未満なら番号を 1 つ増やして停止済みの
アクターへ送り直す。最も外側の `tell` の中で、公開は 1001 回順に流れ、
スタックの深さは一定のまま、`tell` が返る時点ですべての letter を購読者が
受け取り終えている。際限なく送り直すハンドラは終わらないが、それは原典でも
同じである。原典では購読者アクターのメールボックスに letter が積まれ続け、
ディスパッチャのスレッドが回り続ける。スタックはどちらでもあふれない。
違いは、終わらない処理が止まるかどうかではなく、深さが有界かどうかだけである。

順序は先入れ先出しを保つ。入れ子の letter が、その外側の letter の残りの
購読者より先に流れることはない。公開 1 回分（購読者を集めて全員へ送る）が
終わってから、次の待ち行列の先頭が流れる。これは原典で、購読者アクターが
1 つの letter を処理し終えてから次の letter を受け取る順序と合う。

### 3.3 観察の表：何が保たれ、何が変わるか

原典では、dead letters の購読者はアクターであり、`deadLetters` への送信は
そのアクターのメールボックスへ積まれ、ディスパッチャのスレッドが非同期に届ける。
移植は #286 で、関数参照の購読者については送り手のスレッドでの同期配送を
意図して選んだ。候補 1 は、その選んだ同期を最も外側で保ち、入れ子の内側だけを
原典の非同期に近づける。

| 観察 | 原典（Pekko） | 今の移植（#295 まで） | 候補 1 のあと |
|:---|:---|:---|:---|
| 最も外側の `tell` が返る時点で、購読者が letter を受け取っている | 保たれない（メールボックス越しに非同期） | 保たれる | 保たれる |
| 入れ子になった letter（購読者の送り直しで生じた letter）を、最も外側の `tell` が返る時点で購読者が受け取っている | 保たれない | 保たれる（ただし深さに比例してスタックを使う） | 保たれる |
| スタックの深さ | 有界（メールボックスの処理は反復） | 送り直しの回数に比例して増え、あふれる | 有界（一定） |
| letter の順序 | 購読者アクターのメールボックスの順序 | 再帰の深さ優先の順序（入れ子が外側の残りの購読者より先に届く） | 公開の単位ごとの先入れ先出し（入れ子は外側の公開が終わってから届く） |
| 入れ子の letter の購読者の集合が決まる時点 | ディスパッチャが届ける時点 | 入れ子の公開の時点 | 最も外側のループがその letter を流す時点（公開の時点より後になりうる） |
| 購読者のハンドラが走るスレッド | 購読者アクターのディスパッチャのスレッド | 送り手のスレッド | 送り手のスレッド（最も外側の呼び出しのスレッド） |
| 際限ない送り直し | 終わらない（メールボックスに積まれ続ける） | スタックがあふれて落ちる | 終わらない（待ち行列が回り続ける）。落ちはしない |
| `DeadLetter` 自体を宛先が dead letters の letter が落とされる規則（`route_undeliverable` の先頭） | ある（`DeadLetterActorRef` が `DeadLetter` をそのまま公開し、購読者の再送はメールボックスで受ける） | ある | 変わらない |

変わる観察は 2 つだけである。順序が深さ優先から公開単位の先入れ先出しになることと、
入れ子の letter の購読者の集合が流す時点で決まることである。どちらも、
今の深さ優先の順序に利用者が頼ることはできない。深さ優先は再帰の作りから
偶然生まれた強さであり、`docs/decisions/pekko-semantics-first.md` の分け方では
「作りのずれ」に当たる。公開単位の先入れ先出しは原典のメールボックスの順序に近い。

### 3.4 rustdoc の直し

`ActorRef::tell` の rustdoc は、実装の段で次の趣旨に書き換える。経緯は書かない。

- 停止済みの宛先への送信は、送り手のスレッドで dead letters へ流れ、`tell` が
  返る前に購読者が受け取る。
- 購読者のハンドラは、別のアクターへ送ること（停止済みのアクターを含む）ができる。
  その送信で生じた dead letter は、入れ子にならず順に流れて、最も外側の `tell` が
  返る前に購読者へ届く。
- ハンドラが同じシステムを駆動する呼び出し（生む、止める、システムを回す）を
  してはいけないという今の注意は、そのまま残す。実行器を貸している間の制約は
  トランポリンでは変わらない。

## 4. #296 中 2 と低

### 4.1 中 2：予備の道を消し、本番の道を 1 本にする

推す形は次の 2 つである。

1. 予備の道を消す。`Mailbox::for_actor` の引数に、断られた封筒の経路を必須で足す
   （型は今の `Arc<dyn Fn(Envelope) + Send + Sync>` のまま）。経路を持たない
   メールボックスは作れない形にする。`route_refused` の戻りは `Option<Envelope>`
   をやめ、経路を呼んで終わる形にする。`ActorRef::send_message` の
   `hand_over(ManualTask::Envelope(refused))` の分岐は消える。
   - カーネルの単体テストは、記録用の経路（受け取った封筒を一覧に積むクロージャ）を
     `for_actor` へ渡して作る。テストが確かめる道は本番と同じ
     「断る → 経路がその場で呼ばれる」になり、実行器の `deliver` 経由の
     振る舞いにテストが依存しなくなる。
   - `ManualTask::Envelope` 自体は、境界から届いた封筒の配送に使うので残る。
     消えるのは、断られた封筒を運ぶ使い方だけである。
   - 経路を必須にすると、メールボックス単体のテストが dead letters の規則を
     持ち込む必要はない。経路はただの関数であり、記録用で足りる。依存の向きは
     変わらない（経路を作るのは実行器と、テストの組み立てだけである）。
2. `Executor::deliver` の `enqueue` が断った分岐を、経路経由にそろえる。
   断られた封筒に宛先を付け直してから `mailbox.route_refused(refused)` を呼び、
   直接の `route_undeliverable` 呼び出しを消す。これで、閉じた・満杯で断られた
   封筒の流し方は、入口が `tell` でも境界でも同じ 1 本になる。流れる先と
   抑制の規則は同じであり、変わるのは呼ばれる場所だけである。実行器が
   `deliver` を回すスレッドが、そのまま送り手のスレッドになる。

消す代わりに予備の道を残す案は、残す理由が「経路を入れ忘れた場合の保険」だけであり、
その保険が通るのは単体テストの中だけという今の状態を固定する。必須化すれば
入れ忘れは型で防げるので、保険は要らない。

### 4.2 低の各項目

| 項目（基点の場所） | 直し方 |
|:---|:---|
| 取り出しの 3 か所（`Executor::notify_watchers`、基点で 2937 行目付近。関数参照の監視者、通常の監視者、カスタムの通知文を別々の走査で集めている） | 監視者の表を 1 回の走査で走り、関数参照の監視者と通常の監視者を分けながら、カスタムの通知文を同じ走査で引く。通知の順序（関数参照の監視者が先、重なる監視者は関数参照側で 1 回だけ）と、表から外す時点は変えない |
| `routes_closed` と `terminated` の二重持ち（基点で 301〜302 行目） | 状態は 1 つにする。`routes_closed: Arc<AtomicBool>` を唯一の置き場所とし、`Executor::is_terminated` と各所の `self.terminated` の読みは、その原子変数を読むヘルパに寄せる。`stop_all` は原子変数への書き込みだけにする。経路のクロージャが複製を持つ理由（実行器を借りずに読むため）は、原子変数の複製でそのまま満たされる |
| 要らない clone（`notify_watchers` の関数参照側の通知、基点で 2995 行目付近。`Terminated` を作る複製と、送り手として渡す複製が同じ対象を 2 回複製している） | 通知を作ったあと、対象の複製は送信の 1 回だけにし、通知の中の対象は通知が持つものを送る前に複製しない形へ組み替える。実装の段で、対象の複製の回数を数える既存の数え上げ（所有権モデルの条件 1）に触れないことを確かめる |
| 経緯のコメント（`executor_value.rs` の基点で 541 行目付近と 2993 行目付近、`dead_letters_on_sender_thread_regression_test.rs` の 17 行目付近、設計書を含む） | 「プルリクエスト #295 のレビューで」「#286 の前に」のような経緯の言い方を消し、今の振る舞いだけを書く。理由が要る箇所は、この設計書と #286 の設計を参照する形にする。#279 の検査で止まる対象なので、直しの範囲は上の 3 ファイルに限らず、#295 で入ったコメント全体を検索して洗う |
| 弱い確かめ（`dead_letters_on_sender_thread_regression_test.rs` の基点で 140〜144 行目。2 回目の watch のあとの `assert_eq!` に失敗時の文が無く、件数だけでは「古い記録がもう 1 回流れた」場合と区別が弱い） | 失敗時の文を付け、並び全体（対象つきの通知 2 件だけが、順序どおりに積まれていること）を比べる形にする。さらに `run_until_idle` をもう 1 回回して、3 件目が増えないことを確かめる行を足す。テストの面は §7 のとおり既存の公開面で足りる |
| bounded が満杯の経路のテスト | §7.3 に分けて書く。段 5（#256）と重なる場合は #256 側に置く |

## 5. #297：閉じたメールボックスへの system message

### 5.1 原典で確かめた形

原典は Pekko v2.0.0 の `actor/src/main/scala/org/apache/pekko/dispatch/Mailboxes.scala`
と `Mailbox.scala`、`actor/ActorRef.scala`、`dispatch/sysmsg/SystemMessage.scala` である。

1. dead letter mailbox の定義（`Mailboxes.scala` の基点で読んだ 54〜70 行目付近）。
   `deadLetterMailbox` は、生成直後に `becomeClosed()` で閉じられたメールボックスで、
   中のキューは通常の封筒を `onDeadLetterMailboxEnqueue` へ渡す。system message 用の
   上書きは 1 行である。

   ```scala
   def systemEnqueue(receiver: ActorRef, handle: SystemMessage): Unit =
     deadLetters ! DeadLetter(handle, receiver, receiver)
   ```

   つまり、閉じたメールボックスへの system message は、`DeadLetter` に包まれて
   dead letters のハンドル（`DeadLetterActorRef`）へ送られる。包みの中身は
   system message そのもので、送り手と宛先はどちらも `receiver`、すなわち
   閉じたメールボックスの持ち主である。system message には別に送り手が無いので、
   この形しかない。
2. 閉じたメールボックスへの到達（`Mailbox.scala` の `systemEnqueue`）。
   通常のメールボックスの `systemEnqueue` は、キューの頭が `NoMessage`
   （閉じた印）なら、自分で `deadLetterMailbox.systemEnqueue(receiver, message)` を
   呼ぶ。呼び出し側が捨てる・流すを選ぶ作りではない。後片付けでも同じで、
   `Mailbox.cleanUp` と `processAllSystemMessages` の残りの処理は、残った
   system message を 1 つずつ `dlm.systemEnqueue(actor.self, msg)` する。
   順序は、後片付けでは system message が先で、通常のメッセージの後片付け
   （`messageQueue.cleanUp`）があとである。
3. dead letters のハンドルが `DeadLetter` を受けたとき（`ActorRef.scala` の
   `DeadLetterActorRef.!` と `EmptyLocalActorRef.specialHandle`）。
   ハンドルは、中身で特別扱いを先に試し、当たらなければ `DeadLetter` を
   そのままイベントストリームへ公開する。特別扱いは次のとおりである。
   - 中身が `Watch`：監視者へ `DeathWatchNotification(watchee, existenceConfirmed = false,
     addressTerminated = false)` を system message で返し、公開はしない。
     ただし監視者と監視対象のどちらかが dead letters 自身の場合は返さない。
   - 中身が `Unwatch`：`DeadLetter` として公開せず、黙って捨てる（`ActorRef.scala` の
     `specialHandle`、671 行目。`DeadLetterActorRef.!` は `specialHandle` を先に見る、
     711 行目）。答えも返さない。
   - 中身が `DeadLetterSuppression` を実装するもの：`DeadLetter` としてではなく
     `SuppressedDeadLetter(message, sender, deadLetters)` を公開する。送り手は
     包みの送り手（= 閉じた持ち主）、宛先（処理した参照）は dead letters のハンドルである。
   - それ以外：包みの `DeadLetter` をそのまま公開する。送り手と宛先はどちらも
     閉じた持ち主である。

### 5.2 抑制の対象かどうか

system message の種類のうち、`DeadLetterSuppression` を実装するのは原典で 2 つだけである。

- `Terminate`（`SystemMessage.scala` で `with DeadLetterSuppression`）
- `DeathWatchNotification`（同じく `with DeadLetterSuppression`）

`Terminated`（通常のメッセージ）や `PoisonPill` も抑制の対象だが、system message ではない。
`Create`、`Recreate`、`Suspend`、`Resume`、`Supervise`、`Watch`、`Unwatch`、`Failed` は
抑制の対象ではない。`Watch` と `Unwatch` は、抑制ではなく §5.1 の 3 の特別扱いで
公開そのものが起きない。

移植独自の `ChildTerminated` は原典の system message に対応が無い。元帥は
2026-11-27 に、抑制の対象とすることを決めた（§6 の決定 5）。根拠は、原典で子の終了を
伝える `DeathWatchNotification` が抑制の対象であることに合わせるためである。
したがって移植では、抑制の対象の system message は `Terminate`、
`DeathWatchNotification`、`ChildTerminated` の 3 つになる。

抑制された letter は、dead letter のログには数えられないが、`AllDeadLetters` の
購読者は `SuppressedDeadLetter` として受け取る。移植の `DeadLetterListener` と
`DeadLettersShared::is_suppressed` の分け方は、この原典の分け方と合っている。
ただし移植の抑制の判定は `Message` の積み荷の `TypeId` で行うため、列挙である
`SystemMessage` は種類（バリアント）ごとに判定できない。§5.4 の面は、
バリアントで判定する形にする。

### 5.3 対応表：原典の種類と移植の種類

| 原典の system message | 抑制 | dead letter mailbox へ行ったあとの形 | 移植の `SystemMessage` | 移植での扱い（この設計） |
|:---|:---:|:---|:---|:---|
| `Create` | 対象でない | `DeadLetter(Create, receiver, receiver)` を公開 | `Create` | 同じ |
| `Recreate` | 対象でない | `DeadLetter(Recreate, receiver, receiver)` を公開 | `Recreate { cause }` | 同じ |
| `Suspend` | 対象でない | `DeadLetter(Suspend, receiver, receiver)` を公開 | `Suspend` | 同じ |
| `Resume` | 対象でない | `DeadLetter(Resume, receiver, receiver)` を公開 | `Resume` | 同じ。今は黙って捨てている対象 |
| `Terminate` | 対象 | `SuppressedDeadLetter(Terminate, receiver, deadLetters)` を公開 | `Terminate` | 同じ |
| `Supervise` | 対象でない | `DeadLetter(Supervise, receiver, receiver)` を公開 | `Supervise { child, is_async }` | 同じ |
| `Watch` | 対象でない（特別扱い） | 公開しない。監視者へ `DeathWatchNotification(watchee, false, false)` を返す | `Watch { watchee, watcher }` | §5.4 の 2。移植の監視の通知は通常の `Terminated` で、今はその形で返す（§6 の決定 4） |
| `Unwatch` | 対象でない（特別扱い） | 公開しない。黙って捨てる（`specialHandle`、671 行目） | `Unwatch { watchee, watcher }` | 公開しない。`DeadLetter` としても `SuppressedDeadLetter` としても出さず、黙って捨てる |
| `Failed` | 対象でない | `DeadLetter(Failed, receiver, receiver)` を公開 | `Failed { child, cause }` | 同じ |
| `DeathWatchNotification` | 対象 | `SuppressedDeadLetter(DeathWatchNotification, receiver, deadLetters)` を公開 | `DeathWatchNotification { actor, existence_confirmed, address_terminated }` | 同じ |
| （原典に無い） | ― | ― | `ChildTerminated { child }` | 抑制の対象とする（元帥の決定、2026-11-27。§6 の決定 5）。`SuppressedDeadLetter(ChildTerminated, receiver, deadLetters)` として公開する |

原典の `NoMessage` は閉じた印そのものであり、system message として送られることは
ない。移植の列挙に対応は無いので、扱いは要らない。

### 5.4 直す形と面（シグネチャ）

振り替えの責任を 1 か所に集める。新しい面は 2 つである。どちらも `pub(crate)` で、
公開 API は増やさない。

1. `DeadLettersShared` に、system message 専用の流し方を足す。

   ```rust
   pub(crate) fn route_system_undeliverable(
     &self,
     message: SystemMessage,
     recipient: ActorRef,
     dead_letters: &ActorRef,
   ) -> Option<SystemMessage>
   ```

   - `Watch` と `Unwatch` は、この関数は扱わず、そのまま `Some(message)` で返す。
     答え方に実行器が要るためである（2 を見る）。
   - `Terminate`、`DeathWatchNotification`、`ChildTerminated` は、抑制として流す。
     `ChildTerminated` は原典に対応が無く、元帥の決定で抑制に加わった（§5.2）。
     `SuppressedDeadLetter::from_message(Message::new(message), recipient.clone(), dead_letters.clone())`
     を公開する。送り手は閉じた持ち主、処理した参照は dead letters のハンドルであり、
     原典の `publishSupressedDeadLetter` と移植の既存の抑制の約束
     （`route_undeliverable` の抑制分岐）の両方に合う。
   - それ以外は `DeadLetter::from_message(Message::new(message), recipient.clone(), recipient)`
     を公開する。送り手と宛先はどちらも閉じた持ち主であり、原典の
     `DeadLetter(handle, receiver, receiver)` と同じである。
   - 戻りは `None`（流した）にする。公開は §3 のトランポリンを通るので、
     system message の振り替えが購読者のハンドラから再帰しても入れ子にならない。
   - `SystemMessage` を `Message` に包めることが前提である。列挙は `Clone + Debug` で、
     中身は `ActorRef` と `FailureKind` であり、`Message::new` の
     `Any + Send + Sync` を満たす見込みである。実装の段で、列挙に `Send + Sync` の
     表明が要るかどうかをコンパイラで確かめる。

2. `Executor` に、閉じたメールボックスへの system message の入口を 1 つ作る。

   ```rust
   fn route_closed_system_message(&mut self, message: SystemMessage, recipient: ActorRef)
   ```

   - `Watch { watchee, watcher }` は、原典の特別扱いに当たる。移植では監視の答えは
     通常の `Terminated` の通知であり、既存の `enqueue_termination` の道
     （宛先が止まっていれば dead letters へ流す分岐を含む）で、監視者へ
     `existence_confirmed = false`、`address_terminated = false` の通知を送る。
     dead letters 自身への watch は答えない（原典の条件）。
   - `Unwatch` は、`DeadLetter` として公開せず、黙って捨てる。原典の
     `specialHandle`（671 行目）どおりで、意図した無視である。§5.4 の 1 で
     `Some(message)` として返ってきたものを、ここで捨てて終わる。
   - それ以外は 1 の `route_system_undeliverable` へ渡す。

   呼び出し側の直しは 2 か所である。
   - `resume_if_open`：先確かめ（`is_closed` の読み）を消し、`system_send` の
     `Err(message)` を `route_closed_system_message(message, target.clone())` へ渡す。
     `let _ =` は消える。先確かめを消す理由は、競合で閉じる場合に先確かめは
     何も守らないためであり、振り替えが `Err` の側に集まれば競合でも漏れない。
   - `queued_to_dead_letters`：`drop(system)` を消し、`close` が返した system message を
     順に `route_closed_system_message` へ渡す。順序は原典の後片付けに合わせ、
     system message を先に流し、通常の封筒をあとに流す。今の実装は通常の封筒が先
     なので、この点は順序が変わる。system message が空の場合は何も変わらない。
     段 3 の今は、残る system message は多くて `Resume` であり、その場合は
     `DeadLetter(Resume, target, target)` が 1 件増える。黙って捨てることに頼る
     既存のテストの洗い出しは §7.4 に置く。

   `Mailbox` 側は変えない。`system_enqueue` が `Err` で返す今の形は、メールボックスが
   dead letters の規則を持たないための切り分けであり、振り替えの責任は実行器と
   `DeadLettersShared` に集める。原典と作りは違うが、観察（何が、どの形で、
   どの順序で公開されるか）は §5.3 の表どおりにそろえる。

### 5.5 範囲の外：`stop_all` の破棄

`Executor::stop_all` が `close` の結果をまとめて捨てる点は、この設計では直さない。
システム全体が終了する時点では、dead letters の公開先であるイベントストリーム自体が
終わる途中にあり、プルリクエスト #295 で決めた「終了したシステムは断られた封筒を
捨てる」という振る舞い（回帰テストで固定済み）とぶつかる。アクターが 1 つずつ
止まる通常の停止は `queued_to_dead_letters` の道を通るので、§5.4 で原典にそろう。
`stop_all` の一括の後片付けまで原典の `cleanUp` と同じにすべきかどうかは、
元帥は捨てることを決めた（§6 の決定 3）。

## 6. 元帥の決定（2026-11-27）

意味論に関わる選択は、元帥が 2026-11-27 にすべて決めた。決めた人は元帥である。
実装はこの決定に従い、変える場合は元帥の新しい決定が要る。

1. トランポリンを採る。候補 1（§3.1）のとおり、入れ子の letter は先入れ先出しで
   流し、購読者の集合は流す時点で決まる。順序が深さ優先から公開単位の
   先入れ先出しへ変わることと、購読者の集合の決まる時点が遅れることは、
   作りのずれの解消として認められた。
2. 深さの上限は置かない。原典に上限は無く、上限を超えた letter を失う形は採らない。
3. `stop_all` のあとに残ったものは捨てる。システム全体の終了で閉じた
   メールボックスに残った system message と通常の封筒は、dead letters へ
   流さず、今のとおり捨てる（§5.5）。
4. 閉じたメールボックスへの `Watch` は、今は `Terminated` で返す。答えは
   既存の `enqueue_termination` の道で送る `Terminated`（存在の確認なし、
   住所の終了なし）である。system message の `DeathWatchNotification` で返す形は
   今回は採らない。その形は、issue #255（段 4c）の完了条件に足された。
   段 4c で system message の配送が入ったあとに、そちらへ切り替える。
5. `ChildTerminated` は抑制の対象にする。`DeadLetter` ではなく
   `SuppressedDeadLetter` として公開する。設計の最初の案（抑制の対象でない
   `DeadLetter`）から元帥が変更した。根拠は、原典で子の終了を伝える
   `DeathWatchNotification` が抑制の対象であることに合わせるためである（§5.2）。
6. system message を通常の封筒より先に流す。`queued_to_dead_letters` の順序は、
   原典の後片付けの順序に合わせ、system message が先、通常の封筒があとになる。

この節に無かった扱いとして、閉じたメールボックスへの `Unwatch` も元帥が
確かめた。原典どおり、`DeadLetter` として公開せず黙って捨てる（§5.1 の 3、
§5.3 の表）。この扱いは §7 のテストに要る面に入っている。

## 7. テストに要る面

テストの担当（Opus）は、この節の面だけを使って red のテストを先に書く。
新しい公開 API は作らない。面の追加は §5.4 の 2 つだけで、どちらも `pub(crate)` である。

### 7.1 #296 中 1 のテスト（公開面だけで書ける）

置き場所は `crates/fraktor-actor/tests/dead_letters_on_sender_thread_test.rs` への追加か、
新しい `crates/fraktor-actor/tests/dead_letters_trampoline_test.rs` である。
使う面は既存のものだけである（`ActorSystem`、`function_ref`、イベントストリームへの
購読、`ActorRef::tell`）。

- 購読者のハンドラは、受け取った `DeadLetter` の積み荷の番号を見て、1000 未満なら
  番号を 1 つ増やしたメッセージを、捕まえた停止済みアクターの `ActorRef` へ
  `tell` で送り直す。ハンドラが `ActorRef` を捕まえて送る面は今の `function_ref` の
  クロージャで足りる。新しい面は要らない。
- 確かめることは 4 つである。送り直し 1000 回でスタックがあふれずにテストが終わること。
  1001 件の letter すべてを購読者が受け取ること。番号が 0 から 1000 まで順序どおりに
  届くこと。すべてのハンドラが最も外側の `tell` をしたスレッドで走り、
  `tell` が返った時点で 1001 件がそろっていること（`run_until_idle` を挟まない）。
- 番号の積み荷は、文字列ではなく数（`u32`）にする。文字列の積み荷は抑制の判定で
  値を見られる場合があり、テストの対象がぶれるためである。

### 7.2 #297 のテスト（カーネルの単体テストが主）

`SystemMessage` は `pub(crate)` なので、統合テストからは組み立てられない。
red のテストの置き場所はカーネルの単体テストである。

- `DeadLettersShared::route_system_undeliverable` の単体テスト
  （`kernel/actor/dead_letters_shared.rs` の隣のテストファイル）：種類ごとに、
  公開されるイベントの種類（`DeadLetter` か `SuppressedDeadLetter` か、公開されないか）、
  積み荷のバリアント、送り手と宛先を確かめる。使う面は §5.4 の 1 の関数と、
  既存のバスの組み立て（既存の dead letters の単体テストと同じ組み立て）である。
- `Executor::route_closed_system_message` と `resume_if_open` の単体テスト
  （`kernel/executor/executor_value_test.rs`、既存の `record_dead_letters` の
  組み立てを使う）：閉じたメールボックスへの `Resume` が
  `DeadLetter(Resume, target, target)` として記録されること。停止時に残った
  system message が `queued_to_dead_letters` から流れること。`Watch` が監視者への
  通知になり、公開されないこと。`Unwatch` が、`DeadLetter` としても
  `SuppressedDeadLetter` としても公開されず、dead letters の記録が空のままであること
  （黙って捨てる。原典の `specialHandle`、671 行目）。
- 統合テストで書けるのは、`Resume` の競合の形ではなく、停止したアクターの
  後片付けで system message が残る形が公開面から作れる場合だけである。
  作れない場合は、単体テストで足りる理由をテストファイルの冒頭に書く
  （`SystemMessage` がカーネルの内側の型であるため）。

### 7.3 bounded が満杯の経路のテスト

満杯で断られる道は、閉じて断られる道と同じ `route_refused` を通る。今の
`spawn` は `UnboundedQueue` を作るため、アクターシステムの公開面からは
bounded のメールボックスを持つアクターを作れない。この経路の本番化は
段 5（#256）の範囲である。

- #256 が先に進む場合は、満杯のテストは #256 側に置く。置く場所は、
  送り手のスレッドのテストと同じファイルがよい。確かめることは、容量 1 の
  bounded メールボックスが満杯のときに 2 件目の送信が送り手のスレッドで
  dead letters へ流れ、`tell` が返る時点で購読者が受け取っていることである。
- #256 より先にこの issue で置く場合は、カーネルの単体テストに限る。
  `BoundedQueue` と `Mailbox::for_actor`（§4.1 で経路が必須になる）で組み立て、
  経路が満杯の封筒を受け取ることを確かめる。この場合も、公開面のテストは
  #256 へ残すとテストファイルに書く。

### 7.4 黙って捨てることに頼る既存のテストの洗い出し

実装の段の前に、次の手順で洗う。洗い出し自体はテストの担当の仕事に含める。

1. `crates/fraktor-actor/tests` と `src` の単体テストから、dead letters の件数を
   確かめているテストをすべて挙げる（`DeadLetter` と `SuppressedDeadLetter` の
   購読、dead letter のログの記録の両方）。
2. そのうち、失敗と監督の流れ（suspend と resume を含む）か、アクターの停止の
   後片付けを通るテストを印する。§5.4 のあとでは、`Resume` の dead letter が
   増える場合と、停止時に残った system message の分だけ件数が増える場合がある。
3. 件数が増えるテストは、増える分が §5.3 の表の形と合うかを見て、合えば
   期待値を直し、合わなければ実装の側の漏れとして扱う。期待値を黙って
   合わせる直しはしない。

## 8. 実装の段取り（この設計のあと）

1. テストの担当が §7 の red のテストを書く（#296 のトランポリン、#297 の
   system message、§4.2 の弱い確かめの強化を含む）。
2. §6 の元帥の決定（2026-11-27）は、この文書へ反映済みである。実装は
   その決定どおりに進める。決定 5（`ChildTerminated` の抑制）と決定 4
   （`Watch` への答えは今は `Terminated`、system message の形は #255 の
   段 4c の完了条件）は、テストと実装の両方に効くので、先に確かめる。
3. 実装は、トランポリン、予備の道の削除と `deliver` の統一、system message の
   振り替え、rustdoc と経緯のコメントの直し、二重持ちの解消の順に行う。
   必須 CI が緑であることを完了の条件とする（issue #296 と #297 の完了の条件）。

## 9. 原典の参照

- `dispatch/Mailboxes.scala`：dead letter mailbox の定義（`systemEnqueue` が
  `deadLetters ! DeadLetter(handle, receiver, receiver)`、`enqueue` が
  `onDeadLetterMailboxEnqueue` で、`DeadLetter` の積み荷は落とす）。
- `dispatch/Mailbox.scala`：`systemEnqueue` が閉じた状態で dead letter mailbox へ
  振り替えること、`cleanUp` と `processAllSystemMessages` の残りが
  `dlm.systemEnqueue` で流れること。
- `actor/ActorRef.scala`：`DeadLetter`、`SuppressedDeadLetter`、`DeadLetterSuppression` の
  定義と、`DeadLetterActorRef.!` と `EmptyLocalActorRef.specialHandle` の特別扱い
  （`Watch` への答え、`Unwatch` の無視、抑制の公開）。
- `dispatch/sysmsg/SystemMessage.scala`：system message の種類と、`Terminate` と
  `DeathWatchNotification` だけが `DeadLetterSuppression` を実装すること。
- `event/DeadLetterListener.scala`：抑制された letter がログに数えられないこと。
