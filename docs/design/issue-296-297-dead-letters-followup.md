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

改訂（2026-11-27）：独立レビュー（`pr298-design-review.md`、高 4・中 5・低 4）の指摘と、
元帥の決め直し（同日）を反映した。origin/main（`020786b`）を取り込んだあとの形を基点に
直してある。変えた点と、採らなかった指摘の理由は §10 にまとめた。

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
   bounded が満杯の経路のテスト（取り出し、二重持ち、clone、経緯のコメントは
   origin/main の #305 で済んでいる。§4.2）。
3. #297：閉じたメールボックスへの system message を黙って捨てている箇所を、
   原典どおり dead letters へ流す形に直す。原典で確かめた形を §5 に書く。

範囲の外にするものは §5.5 に書く（システム全体の終了時の破棄）。

## 2. 今の形（`526e4f2` で確かめ、origin/main `020786b` の取り込みで行番号と形を直した）

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
  （`kernel/executor/executor_value.rs` の `spawn` 相当）が
  `mailbox.set_refused_route(self.dead_letter_route())` を呼ぶ（`set_refused_route` の
  呼びは 1292 行目）。経路が入っていないメールボックスを作るのは単体テストである。
  `mailbox_test.rs` は `Mailbox::for_actor` ではなく、テスト用の `Mailbox::new` と
  `Mailbox::sharing` を使う。どちらも `with_queue` を `Weak::new()` で呼び、
  `refused_route: None` のままにする（`kernel/dispatch/mailbox.rs` の 98〜114 行目、
  `mailbox_test.rs` の 357〜379 行目）。`ActorRef::for_mailbox` を呼ぶのは `spawn`
  （1295 行目）だけなので、予備の道（`route_refused` が `None` を返す分岐から
  `hand_over`）を通るテストは今は 1 本も無い。経路を必須にするときは、
  `Mailbox::for_actor` だけでなく `with_queue`、`Mailbox::new`、`Mailbox::sharing` と、
  それらを呼ぶ単体テストも変える対象である（§4.1）。
- `Executor::deliver` にはもう 1 つ、経路を通らない分岐がある。境界から届いた封筒を
  生きている entry のメールボックスへ `enqueue` して断られた場合（`executor_value.rs` の
  2835 行目付近の `Err(refused)` 分岐）、`route_refused` を通さず
  `dead_letters_shared.route_undeliverable` を直接呼ぶ。この分岐は本番でも通る
  （entry が残っているのにメールボックスが閉じた、または bounded が満杯の場合）。

### 2.3 system message を黙って捨てる箇所

`SystemMessage` は `kernel/dispatch/system_message.rs` の列挙で、origin/main を
取り込んだ基点では `Create`、`Resume`、`Terminate`、`ChildTerminated` の 4 つだけである
（`pub(crate)`）。原典の他の種類は、それを送る段（段 4c 以降、#255 ほか）で足す。
§5.3 の表は原典の全種類との対応を示し、移植の列挙にまだ無いものはそう書いてある。
閉じたメールボックスに関わる箇所は 3 つある。

1. `Executor::send_system`（`executor_value.rs` の 572〜576 行目）。`Mailbox::system_send`
   の結果を `.ok()` で捨てる。失敗のあとの再開（`fail_supervisor` の 3170 行目）と、
   再起動した親が子を再開する箇所（3336 行目）が `Resume` をこの道で送る。単一スレッドの
   今の実行器では閉じるのと送るのが同じスレッドなので `Err` はほぼ起きないが、#234 の
   複数スレッド化で競合が入ると、閉じたあとの `Resume` が黙って消える。今はどの system
   message も、閉じたメールボックスではここで消える。
2. `Executor::queued_to_dead_letters`（`executor_value.rs` の 1979〜2001 行目）。アクターの
   停止時に `Mailbox::close` が返した system message を `drop(system)` で捨て、コメントに
   「system message は閉じたメールボックスには意味が無い」と書いてある。原典の
   `Mailbox.cleanUp` は、残った system message を 1 つずつ dead letter mailbox へ
   `systemEnqueue` する（§5.1）。
3. `Executor::stop_all`（`executor_value.rs` の 2622 行目付近）。`held.push(entry.mailbox.close())`
   の結果をまとめて `drop(held)` する。これはシステム全体の終了であり、扱いは §5.5 で分ける。

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

`DeadLettersShared::publish` は、公開のジョブをトランポリンのポートへ渡す。印と待ち行列は
そのポートが持つ（置き場所は下のとおり）。

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
- **印と待ち行列は kernel のポートが持ち、実装は adaptor が置く。** kernel に
  `#[cfg(feature = "std")]` や `std::thread_local!` は置かない。syn-check の
  `cfg-std-forbid` が kernel の `#[cfg(feature = "std")]` と `use std::` を禁じており、
  これは必須の CI である。抑制コメントも足さない。`kernel/actor/dead_letter_trampoline.rs`
  に次のポートを置く。

  ```rust
  pub type DeadLetterJob = Box<dyn FnOnce() + Send + 'static>;

  /// Runs dead letter publications one at a time on the calling path.
  pub trait DeadLetterTrampoline: Send + Sync {
    /// Runs `job` now, or queues it behind the run already under way and
    /// returns.
    fn run(&self, job: DeadLetterJob);
  }
  ```

  先例は `LockFactory` と `RawLock`、`FailureCatcher` と `UnwindCatcher` である。
  `DeadLettersShared` は `Arc<dyn DeadLetterTrampoline>` を持ち、`publish` は
  `bus.clone()` とイベントを包んだジョブを `run` へ渡す。`Executor::new` は kernel の
  既定 `SharedTrampoline` を入れ、`Executor::with_dead_letter_trampoline` と
  `ActorSystem::with_dead_letter_trampoline`（`with_failure_catcher` と同じ形）が
  渡された実装を入れる。`DeadLettersShared::new` はトランポリンを引数に取る。
- **std の実装（adaptor）。** `adaptor/sender_thread_trampoline.rs` の
  `SenderThreadTrampoline` が `std::thread_local!` で「流している最中の印
  （`Cell<bool>`）」と「待ち行列（`RefCell<VecDeque<DeadLetterJob>>`）」を持つ。最も外側の
  呼び出しは印を立ててジョブを実行し、終わったら待ち行列が空になるまで先入れ先出しで
  流す。入れ子は積んで返る。ジョブを流す間は待ち行列を借りたままにしない。
- **kernel の既定（スレッドの無い環境）。** `SharedTrampoline` は 1 つの印と 1 つの
  `VecDeque` を `spin::Mutex` の後ろに持つ。最も外側はロックの下で印を立て、ジョブを
  1 つ取り出してロックを離して実行し、また取り出す。入れ子は積んで返る。ロックを持つのは
  取り出しと積み込みの間だけなので、ジョブの中の公開が同じロックを待って止まることはない。
  これは実行の主体が 1 つの環境向けである。スレッドのある環境でこの既定を使うと、別の
  スレッドが積んだジョブを流している側のスレッドが実行しうるので、#286 の「購読者の
  ハンドラが送り手のスレッドで走る」は保てない。std のシステムを組むときは adaptor の
  `SenderThreadTrampoline` を渡す。今の実行器は 1 スレッドなので既定でも同じ観察になり、
  #234 の複数スレッドの段で adaptor の実装を既定にすることを、その段の完了条件に足す。
- `no_std` でも #296 のスタックあふれは残らない。既定もジョブを積んで反復で流すので、
  再帰の深さは送り直しの回数に比例しない。`cfg(feature = "std")` で直接公開へ落とす形は
  採らない。
- 待ち行列は（std の実装では）スレッドごとで、システムを区別しない。同じスレッドで
  システム A の購読者がシステム B の停止済みアクターへ送る場合も、B の公開は A の
  最も外側のループが流す。送り手のスレッドは同じであり、順序はスレッド全体で
  先入れ先出しになる。システムごとに待ち行列を分ける案は、入れ子の入口ごとに印の照合が
  要り、壊れ方が増えるので採らない。代償は §3.3 の表に書く。
- パニックへの備え：購読者のハンドラがパニックで巻き戻ると、印が立ったまま残り、
  そのスレッドの以後の公開がすべて待ち行列に積まれて流れなくなる。ループは
  印と待ち行列を守る RAII の番人を持ち、巻き戻りでは印を下ろし、残った公開は捨てる。
  捨てることは、パニックした配送の続きが保証されないという今の致命の扱い
  （`Executor::lend` の `Lent`）と矛盾しない。
- スレッドの終わりに TLS を壊している途中は `LocalKey::with` が panic する。
  `SenderThreadTrampoline` は `try_with` を使い、`Err` のときはジョブをその場で実行する
  （TLS の破棄中だけ入れ子に戻る）。

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
| 購読者のハンドラが別のシステムを駆動したとき | そのシステムのディスパッチャのスレッドが流す（ハンドラは待たない） | 入れ子の公開がその場で流れる | ハンドラが返るまで流れない（待ち行列はスレッド単位で、システムを区別しない） |
| 際限ない送り直し | 終わらない（メールボックスに積まれ続ける） | スタックがあふれて落ちる | 終わらない（待ち行列が回り続ける）。落ちはしない |
| `DeadLetter` 自体を宛先が dead letters の letter が落とされる規則（`route_undeliverable` の先頭） | ある（`DeadLetterActorRef` が `DeadLetter` をそのまま公開し、購読者の再送はメールボックスで受ける） | ある | 変わらない |

変わる観察は 3 つだけである。順序が深さ優先から公開単位の先入れ先出しになること、
入れ子の letter の購読者の集合が流す時点で決まること、そして購読者のハンドラが別の
システムの公開を待つと、その公開がハンドラの返るまで流れないことである。どれも、
今の深さ優先の順序に利用者が頼ることはできない。深さ優先は再帰の作りから
偶然生まれた強さであり、`docs/decisions/pekko-semantics-first.md` の分け方では
「作りのずれ」に当たる。公開単位の先入れ先出しは原典のメールボックスの順序に近い。
3 つ目は、システムごとの待ち行列を採らなかったことの代償であり、利用者には
rustdoc の注意（§3.4）で「ハンドラからはどのシステムも駆動しない」と伝える。

### 3.4 rustdoc の直し

`ActorRef::tell` の rustdoc は、実装の段で次の趣旨に書き換える。経緯は書かない。

- 停止済みの宛先への送信は dead letters へ流れ、購読者のハンドラは送り手のスレッドで
  走る。
- 購読者のハンドラは、別のアクターへ送ること（停止済みのアクターを含む）ができる。
  その送信で生じた dead letter も、入れ子にならずに流れる。
- 購読者のハンドラからは、どのシステムも駆動しない。`run_until_idle`、`ask` の答えを
  待つこと、システムやアクターを生む・止めることを含む。ハンドラの中で待つと、
  待たれたシステムの dead letters はハンドラが返るまで流れない（§3.3）。

先入れ先出しの順序と「最も外側の `tell` が返る前に購読者が受け取る」は、
`docs/decisions/pekko-semantics-first.md` が「狭めた振る舞いを利用者への約束として
公開 API の文書に書かない」としているものに当たる。rustdoc には事実だけを書き、
順序や同期を約束として書かない。約束として書くなら、元帥の新しい決定が要る。

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

| 項目（issue #296 の低） | いまの状態 / 直し方 |
|:---|:---|
| 取り出しの 3 か所（issue の「2986〜2997 の取り出しの 3 か所目」。dead letters のハンドルを `let dead_letters = self.dead_letters.clone();` で取り出して `route_undeliverable` へ渡していた箇所で、旧 2462、2483、2995 行目付近） | #305（`9db2732`、origin/main）が `Executor::route_undeliverable` にまとめ、clone ごと消した。済んでいる。設計の前の版が「`notify_watchers` の 3 回の走査」と書いたのは取り違えである（レビュー中 1）。`notify_watchers` を 1 走査にまとめるのは別の掃除であり、issue の対象ではないので、この設計では必須にしない |
| `routes_closed` と `terminated` の二重持ち | #305 が `terminated` を消し、`routes_closed: Arc<AtomicBool>` を唯一の置き場所にした。済んでいる |
| 要らない clone | #305 が `self.route_undeliverable(message, sender, recipient)` へ寄せ、`dead_letters.clone()` を消した。済んでいる |
| 経緯のコメント | #305 と #279（`661af21`）が消した。`let _ =` とレビューの経緯の言い方は #279 の CI が禁じる。済んでいる |
| 弱い確かめ（`dead_letters_on_sender_thread_regression_test.rs`） | `9a29285` が通知の並び全体の比較を入れた。2 回目の watch のあとの `assert_eq!` にはまだ失敗時の文が無い。残りはその文と、`run_until_idle` をもう 1 回回して 3 件目が増えないことを確かめる行である。小さな追加として残す |
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
2. 閉じたメールボックスへの到達（`Mailbox.scala` の `systemEnqueue`、500〜510 行目）。
   通常のメールボックスの `systemEnqueue` は、キューの頭が `NoMessage`
   （閉じた印）なら、自分で `deadLetterMailbox.systemEnqueue(receiver, message)` を
   呼ぶ。呼び出し側が捨てる・流すを選ぶ作りではない。後片付けでも同じで、
   `Mailbox.cleanUp`（354〜370 行目）と `processAllSystemMessages`（303〜345 行目）の
   残りは、残った system message を 1 つずつ `dlm.systemEnqueue(actor.self, msg)` する。
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

#### 停止の後片付けの順序（原典で確かめた）

| 順 | 何が dead letters へ流れるか | 原典 | 移植（この設計） |
|:---:|:---|:---|:---|
| 1 | `postStop` の `unstashAll` があふれさせた分（bounded のときだけ） | `UnrestrictedStash.postStop`（99〜101 行目）→ `unstashAll`（226、244〜252 行目）→ `enqueueFirst`（270〜276 行目）。bounded であふれた分は `postStop` の中で断られ、`cleanUp` より先に dead letters へ行く（`Mailbox.enqueueFirst` 620 行目） | `discard_stash_overflow` の victims を `stash_to_dead_letters` の先頭で流す（今のまま）。段 3 のメールボックスは unbounded なので 0 件 |
| 2 | メールボックスの system queue に残った system message | `Mailbox.cleanUp`（354〜368 行目）: `systemDrain` → `dlm.systemEnqueue`。`Terminate` を処理した回に新しく届いたものもここ | `queued_to_dead_letters` が `close()` の `system` を `route_closed_system_message` へ渡す。`Resume` は捨てる（§5.4） |
| 3 | stash（`postStop` がキューの先頭へ戻した分） | `messageQueue.cleanUp`（367〜369 行目）。`unstashAll` は `enqueueFirst` を後ろから繰り返すので、stash はキューの先頭にあり、キューの封筒より先 | `stash_to_dead_letters` を system のあと、封筒の前に呼ぶ |
| 4 | キューの通常の封筒 | 同じ `messageQueue.cleanUp` | `close()` が返した `envelopes` |
| 5 | `Terminate` と同じ `systemDrain` で取り出した残りの system message（段 4） | `processAllSystemMessages`（303〜345 行目）。`Terminate` の処理で閉じたあと、手元に残った `messageList` を 318〜345 行目で `dlm.systemEnqueue` するので、stash と封筒より後 | 移植は 1 通ずつ取り出す（`mailbox.rs` 405〜413 行目）ので「同じ回に取り出した残り」は無く、閉じたときにキューに残った分として 2 で先に流れる。段 3 では `Resume` だけで、捨てるので差は出ない。段 4 で差が出るなら、`process_all_system_messages` を原典と同じバッチの形にして、閉じたあとの残りをキューの後片付けのあとに流す（#255 の段 4c の完了条件に足す） |
| 6 | `stop_all`（システム全体の終了） | ― | 捨てる（決定 3、§5.5） |

確かめたこと：stash は `UnrestrictedStash.postStop` の `unstashAll` でキューの先頭へ
戻り、`Mailbox.cleanUp` は system を先に、そのあと `messageQueue.cleanUp` で stash と
封筒を流す。順序は「system message → stash → キューの通常の封筒」でよい。
`postStop` が `detach` より先であることは `FaultHandling.finishTerminate`（247〜275 行目、
254 行目の `aroundPostStop`、261 行目の `dispatcher.detach`）で確かめた。`detach` の
`unregister` が `mailbox.cleanUp()` を呼ぶ（`AbstractDispatcher.scala` 276〜284 行目）。

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
`DeathWatchNotification`、`ChildTerminated` の 3 つになる。origin/main を取り込んだ
基点の列挙にあるのは `Create`、`Resume`、`Terminate`、`ChildTerminated` の 4 つだけであり、
`DeathWatchNotification` は段 4c（#255）で足す。決定 5 はそのときも変わらない。

抑制された letter は、dead letter のログには数えられないが、`AllDeadLetters` の
購読者は `SuppressedDeadLetter` として受け取る。移植の `DeadLetterListener` と
`DeadLettersShared::is_suppressed` の分け方は、この原典の分け方と合っている。
ただし移植の抑制の判定は `Message` の積み荷の `TypeId` で行うため、列挙である
`SystemMessage` は種類（バリアント）ごとに判定できない。§5.4 の面は、
バリアントで判定する形にする。

### 5.3 対応表：原典の種類と移植の種類

| 原典の system message | 抑制 | dead letter mailbox へ行ったあとの形 | 移植の `SystemMessage` | 移植での扱い（この設計） | リスナーが書き出す文面 |
|:---|:---:|:---|:---|:---|:---|
| `Create` | 対象でない | `DeadLetter(Create, receiver, receiver)` を公開 | `Create`（今ある） | 同じ | `[fraktor_actor::kernel::dispatch::system_message::Create]` |
| `Recreate` | 対象でない | `DeadLetter(Recreate, receiver, receiver)` を公開 | `Recreate { cause }`（まだ無い。それを送る段で足す） | 同じ | `[fraktor_actor::kernel::dispatch::system_message::Recreate]` |
| `Suspend` | 対象でない | `DeadLetter(Suspend, receiver, receiver)` を公開 | `Suspend`（まだ無い） | 同じ | `[fraktor_actor::kernel::dispatch::system_message::Suspend]` |
| `Resume` | 対象でない | 原典にメッセージとしての対応なし。`finishRecreate` の `resumeNonRecursive()`（`FaultHandling.scala` 278〜284 行目、283 行目）がメッセージではない直接の呼び出しで再開する | `Resume`（今ある） | 閉じたメールボックスでは黙って捨てる。原典の直接の再開に当たり、dead letter にするメッセージが無い（§5.4） | 出さない（捨てる） |
| `Terminate` | 対象 | `SuppressedDeadLetter(Terminate, receiver, deadLetters)` を公開 | `Terminate`（今ある） | 同じ | 出さない（`SuppressedDeadLetter` は数えない） |
| `Supervise` | 対象でない | `DeadLetter(Supervise, receiver, receiver)` を公開 | `Supervise { child, is_async }`（まだ無い） | 同じ | `[fraktor_actor::kernel::dispatch::system_message::Supervise]` |
| `Watch` | 対象でない（特別扱い） | 公開しない。監視者へ `DeathWatchNotification(watchee, false, false)` を返す | `Watch { watchee, watcher }`（まだ無い。段 4c） | §5.4 の 2。段 4c で `Watch` が届くようになったら、まずは通常の `Terminated` で返す（§6 の決定 4） | 出さない |
| `Unwatch` | 対象でない（特別扱い） | 公開しない。黙って捨てる（`specialHandle`、671 行目） | `Unwatch { watchee, watcher }`（まだ無い。段 4c） | 公開しない。`DeadLetter` としても `SuppressedDeadLetter` としても出さず、黙って捨てる | 出さない |
| `Failed` | 対象でない | `DeadLetter(Failed, receiver, receiver)` を公開 | `Failed { child, cause }`（まだ無い） | 同じ | `[fraktor_actor::kernel::dispatch::system_message::Failed]` |
| `DeathWatchNotification` | 対象 | `SuppressedDeadLetter(DeathWatchNotification, receiver, deadLetters)` を公開 | `DeathWatchNotification { actor, existence_confirmed, address_terminated }`（まだ無い。段 4c） | 段 4c で同じにする。今は、閉じた監視者への `watch_with` のカスタム通知が `enqueue_termination` の閉じた分岐で `DeadLetter(custom, watcher, watcher)` として公開され、ログに 1 行出る（原典では出ない）。#297 で、閉じた監視者への通知はカスタムの有無にかかわらず `Message::new(notice)`（`Terminated`）を `route_undeliverable` へ渡す形に直す。`Terminated` は抑制の対象なので `SuppressedDeadLetter` になり、ログは出ない。カスタム文は捨てる（原典では DWN が届かないのでカスタム文は作られない）。段 4c で DWN を送るようになったら、原典どおり `DWN` を抑制として流す | 出さない |
| （原典に無い） | ― | ― | `ChildTerminated { child }`（今ある） | 抑制の対象とする（元帥の決定、2026-11-27。§6 の決定 5）。`SuppressedDeadLetter(ChildTerminated, receiver, deadLetters)` として公開する | 出さない |

リスナーの文面の列は、`DeadLetterListener` の `Message [<名前>] …` の `<名前>` の部分である
（`DeadLetter` として公開された system message だけ）。原典は `unwrapped.getClass.getName`
で種類ごとのクラス名を出す（`DeadLetterListener.scala` 139〜140 行目）。移植は今
`Message::payload_type_name()`（`core::any::type_name::<T>()`）を出すので、列挙をそのまま
包むとどの種類も `SystemMessage` と出る（レビュー高 3）。`SystemMessage` に種類ごとの
`class_name` を持たせ、リスナーがそれを読む形にする（§5.4）。

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
   )
   ```

   - `Terminate` と `ChildTerminated` は、抑制として流す。`ChildTerminated` は原典に
     対応が無く、元帥の決定で抑制に加わった（§5.2）。段 4c で
     `DeathWatchNotification` が入ったら同じ腕に足す。
     `SuppressedDeadLetter::from_message(Message::new(message), recipient.clone(), dead_letters.clone())`
     を公開する。送り手は閉じた持ち主、処理した参照は dead letters のハンドルであり、
     原典の `DeadLetterActorRef.!` の `SuppressedDeadLetter` の公開（`ActorRef.scala` 695 行目）
     と、移植の既存の抑制の約束（`route_undeliverable` の抑制分岐）の両方に合う。
   - それ以外は `DeadLetter::from_message(Message::new(message), recipient.clone(), recipient)`
     を公開する。送り手と宛先はどちらも閉じた持ち主であり、原典の
     `DeadLetter(handle, receiver, receiver)` と同じである。
   - `Watch`、`Unwatch`、`Resume` はこの関数へ来ない（2 が先に分ける）。
   - 公開は §3 のトランポリンを通るので、system message の振り替えが購読者のハンドラから
     再帰しても入れ子にならない。
   - `SystemMessage` を `Message` に包めることが前提である。列挙は `Clone + Debug` で、
     中身は `ActorRef` である（`Recreate` と `Failed` が入ると `FailureKind` も）。
     `Message::new` の `Any + Send + Sync` を満たす見込みである。実装の段で、列挙に
     `Send + Sync` の表明が要るかどうかをコンパイラで確かめる。

   **リスナーの文面（高 3）。** `SystemMessage` に
   `pub(crate) fn class_name(&self) -> &'static str` を足し、種類ごとに
   `"fraktor_actor::kernel::dispatch::system_message::<種類>"` を返す。
   `DeadLetterListener` の `unwrapped` は、積み荷が `SystemMessage` なら
   `payload_type_name()` ではなく `class_name()` を出す。列挙をそのまま包む今の形では、
   どの種類も `...::system_message::SystemMessage` と出て種類が消える（レビュー高 3）。
   種類ごとに別の型を作って包む案は採らない。`DeadLetter` の中身を `SystemMessage` として
   取り出すテストと、`Message` の積み荷をそのまま比べる既存の見方が壊れるためである。
   名前の解決だけを種類に合わせる。

2. `Executor` に、閉じたメールボックスへの system message の入口を 1 つ作る。

   ```rust
   fn route_closed_system_message(&mut self, message: SystemMessage, recipient: ActorRef)
   ```

   - `Resume` は、黙って捨てて終わる。原典の `Resume` の system message は
     `SupervisorStrategy.resumeChild` と、再開した親が子を再開する `faultResume` の
     道だけで、再起動のあとの再開は `FaultHandling.finishRecreate` の
     `resumeNonRecursive()`（`FaultHandling.scala` 278〜284 行目、283 行目）という
     メッセージではない直接の呼び出しである。移植の `Resume` はこの直接の呼び出しに
     当たるので、dead letter にするメッセージが原典に無い。この理由と行をコードの
     コメントに書く（§5.3 の `Resume` の行）。
   - `Terminate`、`ChildTerminated`、それ以外の公開する種類は 1 へ渡す。
   - `Watch`、`Unwatch`、`DeathWatchNotification` は、段 4c（#255）で列挙に足すときに
     この関数へ腕を足す。`Watch` は決定 4 の形（`enqueue_termination` の道で
     `Terminated` を返し、dead letters 自身への watch には答えない）、`Unwatch` は
     原典の `specialHandle`（671 行目）どおり黙って捨てる、`DeathWatchNotification` は
     1 の抑制の腕へ足す。今の列挙にはこれらの型が無いので、段 3 では通らない。

   呼び出し側の直しは 2 か所である。
   - `send_system`（572〜576 行目）：`.ok()` をやめ、`Mailbox::system_send` の
     `Err(message)` を `route_closed_system_message(message, target.clone())` へ渡す。
     引数を取るために `&self` から `&mut self` にする（呼び出し側はすべて `&mut self` の
     中にある。`create_now`、`stop`、`fail_supervisor`、`restart_cell`）。`Resume` を
     捨てる判断は入口の 1 か所に置く。
   - `queued_to_dead_letters`（1979 行目）：`drop(system)` を消し、`close` が返した
     system message を順に `route_closed_system_message` へ渡す。そのあとに
     `stash_to_dead_letters`、そのあとに `close` が返した封筒を流す。今は
     `finish_terminate` が `stash_to_dead_letters` を先に呼び、`queued_to_dead_letters` が
     封筒を流すので、`finish_terminate` の 1743〜1744 行目の 2 呼びを
     `queued_to_dead_letters` 1 つにまとめ、順序を「system message → stash → 封筒」に
     する（§5.1 の表）。`Mailbox::close` は cell も `entry.stash` も触らないので、
     close のあとで stash を読める。
   - 段 3 で残る system message は `Resume`（捨てる）と、停止に追い越された `Create`
     （`DeadLetter(Create)` として公開され、ログに 1 行出る）である。`Terminate` と
     `ChildTerminated` は抑制で、ログには出ない。件数が増えるテストの洗い出しは §7.4 に
     置く。

   `Mailbox` 側は変えない。`system_enqueue` が `Err` で返す今の形は、メールボックスが
   dead letters の規則を持たないための切り分けであり、振り替えの責任は実行器と
   `DeadLettersShared` に集める。原典と作りは違うが、観察（何が、どの形で、
   どの順序で公開されるか）は §5.3 の表どおりにそろえる。

   段 3 では、原典にメッセージとして対応のある「黙って捨てる」は 0 件になる
   （`Resume` は原典ではメッセージではない）。#297 は、それでも §5.3 の対応表と、
   `Resume` を捨てる理由のコメントで閉じる。ケースを数から外す扱いはしない。

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
   作りのずれの解消として認められた。印と待ち行列は kernel のポート
   `DeadLetterTrampoline` が持ち、std の adaptor がスレッドごとの実装を、kernel の
   既定がスレッドの無い環境向けの実装を持つ（§3.1）。
2. 深さの上限は置かない。原典に上限は無く、上限を超えた letter を失う形は採らない。
3. `stop_all` のあとに残ったものは捨てる。システム全体の終了で閉じた
   メールボックスに残った system message と通常の封筒は、dead letters へ
   流さず、今のとおり捨てる（§5.5）。
4. 移植の `SystemMessage` に `Watch` が入る段 4c では、閉じたメールボックスへの
   `Watch` は `Terminated` で返す。答えは既存の `enqueue_termination` の道で送る
   `Terminated`（存在の確認なし、住所の終了なし）である。system message の
   `DeathWatchNotification` で返す形は今回は採らない。その形は、issue #255（段 4c）の
   完了条件に足された。段 4c で system message の配送が入ったあとに、そちらへ
   切り替える。origin/main を取り込んだ基点の列挙に `Watch` はまだ無いので、段 3 では
   この答えを返す道は通らない。
5. `ChildTerminated` は抑制の対象にする。`DeadLetter` ではなく
   `SuppressedDeadLetter` として公開する。設計の最初の案（抑制の対象でない
   `DeadLetter`）から元帥が変更した。根拠は、原典で子の終了を伝える
   `DeathWatchNotification` が抑制の対象であることに合わせるためである（§5.2）。
6. 停止の後片付けは「system message → stash → キューの通常の封筒」の順に流す。
   `queued_to_dead_letters` は、`Mailbox::close` が返した system message を先に、
   そのあとに stash、そのあとに封筒を流す（§5.1 の表）。原典では `postStop` の
   `unstashAll` が stash をキューの先頭へ戻し、`Mailbox.cleanUp` が system を先に、
   `messageQueue.cleanUp` をあとに流すので、この順になる。`Resume` は原典に
   メッセージとしての対応が無いので捨てる（§5.4）。したがって段 3 の後片付けで
   system message の dead letter は増えない（停止に追い越された `Create` があれば、
   それは 1 件増える）。`Terminate` と同じ `systemDrain` で取り出した残りの system
   message をキューの後片付けのあとに流す原典の順序（`Mailbox.scala` 318〜345 行目）は、
   段 4c で `process_all_system_messages` をバッチの形にするときに合わせる（§5.1 の表）。

この節に無かった扱いとして、閉じたメールボックスへの `Unwatch` も元帥が
確かめた。原典どおり、`DeadLetter` として公開せず黙って捨てる（§5.1 の 3、
§5.3 の表）。段 4c で `Unwatch` が列挙に入ったときに、§5.4 の 2 の腕として足す。

## 7. テストに要る面

テストの担当（Opus）は、この節の面だけを使って red のテストを先に書く。
テストのために新しい公開 API は作らない。面の追加は §5.4 の 2 つだけで、どちらも
`pub(crate)` である。実装の側では §3.1 のポート `DeadLetterTrampoline` と
`ActorSystem::with_dead_letter_trampoline` が増えるが、これは
`with_failure_catcher` と同じ組立の面であり、意味論の約束は増やさない。

### 7.1 #296 中 1 のテスト（公開面だけで書ける）

置き場所は新しい `crates/fraktor-actor/tests/dead_letters_trampoline_test.rs` に限る。
既存の `dead_letters_on_sender_thread_test.rs` へ足さない。今のコードでは送り直し
1000 回が `fatal runtime error: stack overflow, aborting`（SIGABRT）でプロセスごと
落ちるので、同じファイルの既存のテストも結果を出せなくなる（レビュー中 5）。
red の間は `cargo test --test dead_letters_trampoline_test` のようにこのファイルだけを
回す。使う面は既存のものだけである（`ActorSystem`、`function_ref`、イベントストリームへの
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
- 深さの境目を環境によらず決めるために、スタックの大きさを指定した
  `std::thread::Builder::new().stack_size(...)` のスレッドで送り直しを回す。
  red ではそのスレッドのスタックがあふれてプロセスが落ちるが、テストのファイルを
  分けてあるので、他のテストのバイナリは巻き込まない。

### 7.2 #297 のテスト（カーネルの単体テストが主）

`SystemMessage` は origin/main を取り込んだ基点では `pub(crate)` である
（`dispatch.rs` の 69 行目）。レビューをした `a2eb669` では `pub` だったが、#279 の
`661af21` が `pub(crate)` に戻した。統合テストからは組み立てられないので、red の
テストはカーネルの単体テストに置く。

- `Executor::route_closed_system_message` と `send_system`/`queued_to_dead_letters` の
  テスト（`kernel/executor/executor_value_test.rs`）：既存の `death_watch_executor` と
  `record_dead_letters` の組み立てを使う。ただし `record_dead_letters` は積み荷を
  `&'static str` として読むので、system message 用に、`DeadLetter` の中身を
  `SystemMessage` として読む記録器を同じファイルに足す。確かめることは、閉じた
  メールボックスへの `Resume` が捨てられて記録が空のままであること、`Create` が
  `DeadLetter(Create, target, target)` として記録されること、`Terminate` と
  `ChildTerminated` が `SuppressedDeadLetter` として記録されること、停止時に残った
  system message が `queued_to_dead_letters` から「system → stash → 封筒」の順で
  流れることである。
- `DeadLettersShared::route_system_undeliverable` のテスト：専用のテストファイルは
  今は無い（`kernel/actor/` に `dead_letters_shared_test.rs` は無い）。新しく
  `#[cfg(test)] #[path = "dead_letters_shared_test.rs"] mod tests;` を
  `dead_letters_shared.rs` に足し、`EventStreamBus::new()` を作って
  `shared.set_bus(bus.downgrade())` し、`EventClass::any()` に記録用の関数参照を
  購読させて、`route_system_undeliverable` が公開するイベントの型と積み荷を見る。
  関数参照は `Executor::function_ref` で作れるので、実行器を 1 つ組み立てて使う。
- リスナーの文面（高 3）のテスト：`kernel/event/` にも `dead_letter_listener` の
  単体テストは無い。`dead_letter_listener.rs` に
  `#[cfg(test)] #[path = "dead_letter_listener_test.rs"] mod tests;` を足し、
  `DeadLetterListener::new` と `on_event` に
  `DeadLetter(Message::new(SystemMessage::Create))` を渡して、返る行が
  `[fraktor_actor::kernel::dispatch::system_message::Create]` を含むことを確かめる。
  `SystemMessage` は `pub(crate)` なので、ここでしか作れない。
- `Watch` と `Unwatch` は段 3 の列挙に無いので、テストも段 4c で足す。決定 4 の答えは
  そのとき統合テストで確かめる。
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
   後片付けを通るテストを印する。§5.4 のあとでは、`Resume` は捨てるので増えない。
   件数が動きうるのは、停止に追い越された `Create` の `DeadLetter` が増える場合と、
   `watch_with` の閉じた監視者への通知が `DeadLetter(custom)` から
   `SuppressedDeadLetter` に変わる場合である。
3. 件数が増えるテストは、増える分が §5.3 の表の形と合うかを見て、合えば
   期待値を直し、合わなければ実装の側の漏れとして扱う。期待値を黙って
   合わせる直しはしない。

## 8. 実装の段取り（この設計のあと）

1. テストの担当が §7 の red のテストを書く（#296 のトランポリン、#297 の
   system message、§4.2 の弱い確かめの強化を含む）。
2. §6 の元帥の決定（2026-11-27）は、この文書へ反映済みである。実装は
   その決定どおりに進める。決定 5（`ChildTerminated` の抑制）と決定 4
   （`Watch` への答えは段 4c で `Terminated`、system message の形は #255 の
   段 4c の完了条件）は、テストと実装の両方に効くので、先に確かめる。
3. 実装は、トランポリン（kernel のポート、adaptor のスレッドごとの実装、
   `ActorSystem` の入口）、予備の道の削除と `deliver` の統一、system message の
   振り替え（`send_system` と `queued_to_dead_letters`、後片付けの順序）、
   `SystemMessage::class_name` とリスナーの文面、rustdoc と経緯のコメントの直しの
   順に行う。二重持ちの解消、取り出しの集約、要らない clone、経緯のコメントは
   origin/main の #305（`9db2732`）で済んでいる。
   必須 CI が緑であることを完了の条件とする（issue #296 と #297 の完了の条件）。

## 9. 原典の参照

- `dispatch/Mailboxes.scala`：dead letter mailbox の定義（`systemEnqueue` が
  `deadLetters ! DeadLetter(handle, receiver, receiver)`、`enqueue` が
  `onDeadLetterMailboxEnqueue` で、`DeadLetter` の積み荷は落とす）。
- `dispatch/Mailbox.scala`：`systemEnqueue`（500〜510 行目）が閉じた状態で dead letter
  mailbox へ振り替えること、`cleanUp`（354〜370 行目）が system を先に
  `messageQueue.cleanUp` をあとに流すこと、`processAllSystemMessages`（303〜345 行目）の
  残りが `dlm.systemEnqueue` で流れること。
- `actor/Stash.scala`：`UnrestrictedStash.postStop`（99〜101 行目）、`unstashAll`
  （226、244〜252 行目）、`enqueueFirst`（270〜276 行目）が stash をキューの先頭へ
  戻すこと。
- `actor/dungeon/FaultHandling.scala`：`finishRecreate`（278〜284 行目、283 行目の
  `resumeNonRecursive()`）と `finishTerminate`（247〜275 行目、254 行目の
  `aroundPostStop`、261 行目の `dispatcher.detach`）。
- `dispatch/AbstractDispatcher.scala`：`unregister`（276〜284 行目）が
  `mailbox.cleanUp()` を呼ぶこと。
- `actor/ActorRef.scala`：`DeadLetter`、`SuppressedDeadLetter`、`DeadLetterSuppression` の
  定義と、`DeadLetterActorRef.!` と `EmptyLocalActorRef.specialHandle` の特別扱い
  （`Watch` への答え、`Unwatch` の無視、抑制の公開）。
- `dispatch/sysmsg/SystemMessage.scala`：system message の種類と、`Terminate` と
  `DeathWatchNotification` だけが `DeadLetterSuppression` を実装すること。
- `event/DeadLetterListener.scala`：`logDeadLetter` の `unwrapped.getClass.getName`
  （139〜140 行目）と、抑制された letter がログに数えられないこと。

## 10. 変更の記録（2026-11-27 の改訂）

独立レビュー（`pr298-design-review.md`）の指摘と、元帥の決め直しを受けて変えた点を、
指摘ごとに書く。採らなかったものは理由も書く。ここは変更の記録であり、実装の手順では
ない（手順は §8）。

### 元帥の決め直し

- 高 1（`Resume`）：移植の `Resume` は原典の `finishRecreate` の `resumeNonRecursive()`
  （`FaultHandling.scala` 278〜284 行目、283 行目）に当たる、メッセージではない直接の
  再開である。閉じたメールボックスでは今のとおり捨て、理由と原典の行をコードの
  コメントに書く。§5.3 の `Resume` の行は「原典にメッセージとしての対応なし」に直した。
  §5.4 の呼び出し側は `resume_if_open` ではなく、origin/main の `send_system`
  （572〜576 行目）を直す形にした。決定 6 も、`Resume` は捨てるので段 3 では
  system message の dead letter が増えない、と書き直した。
- 高 4（後片付けの順序）：§5.1 に「system message → stash → キューの通常の封筒」の
  表を足し、`Terminate` と同じ `systemDrain` で取り出した残りの system message が
  キューの後片付けより後に流れる原典の順序（`Mailbox.scala` 318〜345 行目）も入れた。
  `StashSupport` は `UnrestrictedStash.postStop`（99〜101 行目）→ `unstashAll`
  （226、244〜252 行目）→ `enqueueFirst`（270〜276 行目）で stash をキューの先頭へ
  戻すことを引き、レビューの読み（stash はキューの封筒より先）が正しいと確かめた。
  `postStop` が `detach` → `cleanUp` より先であることも `FaultHandling.finishTerminate`
  （247〜275 行目）と `AbstractDispatcher.unregister`（276〜284 行目）で確かめた。

### 設計の担当が決めたもの

- 高 2（kernel の std）：`#[cfg(feature = "std")]` と `std::thread_local!` を kernel に
  置く案を捨てた。kernel に `DeadLetterTrampoline` のポートを置き、std の adaptor の
  `SenderThreadTrampoline` が `thread_local!` で印と待ち行列を持つ。kernel の既定は
  スレッドの無い環境向けの `SharedTrampoline`（1 つの印と 1 つの待ち行列）で、これも
  ジョブを積んで反復で流すので、no_std でも #296 のスタックあふれは残らない。
  `kernel/sync/counts.rs` と `adaptor/ask_driver.rs` を先例に挙げた前の版は取り下げた
  （前者は `cfg(test)` の抑制つき、後者は adaptor 側である）。抑制コメントは足さない。
- 高 3（ログの文面）：`SystemMessage::class_name` で種類ごとの名前を返し、リスナーの
  `unwrapped` がそれを読む形にした。§5.3 に「リスナーが書き出す文面」の列を足した。
  種類ごとに別の型で包む案は採らない。`DeadLetter` の中身を `SystemMessage` として
  取り出す見方と、`Message` の積み荷をそのまま比べる既存のテストが壊れるためである。
- 中 1：§4.2 を issue #296 の実際の項目に直した。issue の「取り出しの 3 か所目」は
  dead letters のハンドルを `let dead_letters = self.dead_letters.clone();` で取り出して
  `route_undeliverable` へ渡していた箇所（旧 2462、2483、2995 行目付近）である。
  前の版が書いた `notify_watchers` の 3 走査は取り違えで、issue の項目ではない。
  3 か所は origin/main の #305（`9db2732`）で `Executor::route_undeliverable` に
  集約済みである。
- 中 2：閉じた監視者への `watch_with` のカスタム通知が `DeadLetter(custom)` として
  ログに出るずれを、#297 で直すことにした。閉じた監視者への通知は、カスタムの有無に
  かかわらず `Message::new(notice)`（`Terminated`）を `route_undeliverable` へ渡す。
  `Terminated` は抑制の対象なので `SuppressedDeadLetter` になり、ログは出ない。
  カスタム文は捨てる（原典では DWN が届かないのでカスタム文は作られない）。段 4c で
  DWN を送るようになったら、原典どおり `DWN` を抑制として流す（§5.3 の DWN の行）。
- 中 3：origin/main を取り込んだ基点の `SystemMessage` には `Watch`、`Unwatch`、
  `Suspend` などの型が無い（main で段 4a（#253）が送る 4 つだけになっている）。
  段 3 で届かない分岐を単体テストだけで持つことにはならないので、これらの腕は
  段 4c（#255）で型と一緒に足す。決定 4 の `Watch` の答えは、そのときに統合テストで
  確かめる。
- 中 4：§3.4 の rustdoc から、順序と「`tell` が返る前に購読者が受け取る」という約束を
  消した。`docs/decisions/pekko-semantics-first.md` が禁じる「狭めた振る舞いを公開 API の
  文書に書く」に当たるためである。事実（入れ子にならないこと、ハンドラが走るスレッド）
  だけを書く。
- 中 5：§7.1 の red は新しい `dead_letters_trampoline_test.rs` に限る。今のコードでは
  スタックがあふれてプロセスが落ちるので、既存のファイルへ足すとそのファイルの
  テストも結果を出せない。スタックの大きさを指定したスレッドで回すことも書いた。
- 低 1：§2.2 を直した。`mailbox_test.rs` は `Mailbox::for_actor` ではなく
  `Mailbox::new` と `Mailbox::sharing` を使い、どちらも `with_queue` を `Weak::new()` で
  呼ぶ。`ActorRef::for_mailbox` を呼ぶのは `spawn` だけなので、予備の道を通るテストは
  今は無い。経路を必須にするときは `with_queue`、`Mailbox::new`、`Mailbox::sharing` も
  変える対象にした。
- 低 2：`SystemMessage` はレビューをした `a2eb669` では `pub` だったが、origin/main の
  #279（`661af21`）が `pub(crate)` に戻した。§7.2 はその前提に直し、隣に単体テストが
  無いことを認めて、バスと購読者（`EventStreamBus::new`、`set_bus`、`EventClass::any` の
  記録用の関数参照）とリスナーの組み立てを書いた。
- 低 3：§5.4 の流れが食い違っていた（1 が `Watch`/`Unwatch` を `Some` で返し、2 がまた
  分ける）。1 は公開する種類だけを扱い、2 の入口が `Resume` を先に分ける形に直した。
  `Watch` と `Unwatch` は段 4c で型が入ったときに 2 の腕へ足す。`Option` の戻りは
  無くした。
- 低 4：`try_with` で TLS の破棄中を拾い、直接実行へ落ちることを書いた。待ち行列が
  システムを区別しないため、ハンドラが別のシステムの公開を待つと流れない件は、
  rustdoc の注意を「どのシステムも駆動しない」に広げ、§3.3 の表に行を足して伝える。
  システムごとの待ち行列は、入れ子の入口ごとに印の照合が要るので採らない（§3.1）。

### 範囲の外のまま

- レビューの別件（`Kill` が原典では `DeadLetterSuppression` を実装しないのに、移植の
  `dead_letters_shared.rs` が `"Kill"` を抑制している）は、この設計では触らない。
  別の issue で確かめる。
