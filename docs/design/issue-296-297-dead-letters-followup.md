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

改訂（2026-11-27）：独立レビュー（`pr298-design-review.md`、高 4・中 5・低 4）と
再レビュー（`pr298-design-rereview.md`、重い 6 件・中低・行番号）の指摘、元帥の
決め直し（同日。とくに、閉じたメールボックスに残った `Resume` は `DeadLetter` として
公開する、という読み替え）を反映した。origin/main（`020786b`）を取り込んだあとの形を
基点に直してある。変えた点と、採らなかった指摘の理由は §10 にまとめた。

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
   （HEAD の 84〜92 行目）が事実と違う点を、実装と同時に直す。
2. #296 の中 2 と低：dead letters の経路が入っていないメールボックスの予備の道、
   取り出しの重なり、二重持ちの状態、要らない clone、経緯のコメント、弱い確かめ、
   bounded が満杯の経路のテスト（取り出し、二重持ち、clone、経緯のコメントは
   origin/main の #305 で済んでいる。§4.2）。
3. #297：閉じたメールボックスへの system message を黙って消している箇所を、
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
  積み荷の `DeadLetter` を宛先を見ずに落とす規則（`kernel/actor/dead_letters_shared.rs`
  の 111〜113 行目）は、dead letters 自身への送信の循環を断つだけで、
  「購読者が別の停止済みアクターへ送り直す」循環は断たない。

### 2.2 予備の道

- `Mailbox::route_refused`（`kernel/dispatch/mailbox.rs`）は、経路が入っていないと
  封筒を呼び出し側へ返す。`ActorRef::send_message` は返ってきた封筒を
  `mailbox.hand_over(ManualTask::Envelope(refused))` で実行器へ渡し、実行器の
  `Executor::deliver` が dead letters へ流す。
- 本番の道では経路は常に入っている。アクターを生む箇所
  （`kernel/executor/executor_value.rs` の `spawn_at`）が
  `mailbox.set_refused_route(self.dead_letter_route())` を呼ぶ（`set_refused_route` の
  呼びは 1292 行目）。経路が入っていないメールボックスを作るのは単体テストである。
  `mailbox_test.rs` は `Mailbox::for_actor` ではなく、テスト用の `Mailbox::new` と
  `Mailbox::sharing` を使う。どちらも `with_queue` を `Weak::new()` で呼び、
  `refused_route: None` のままにする（`kernel/dispatch/mailbox.rs` の 98〜114 行目、
  `mailbox_test.rs` の 361〜379 行目）。`Mailbox::for_actor` を呼ぶのは `spawn_at`
  （1291 行目）と `mailbox_run_test.rs` の 418 行目だけであり、`ActorRef::for_mailbox`
  を呼ぶのは `spawn_at`（1295 行目）だけである。予備の道（`route_refused` が `None` を
  返す分岐から `hand_over`）を通るテストは今は 1 本も無い。経路を必須にするときは、
  `Mailbox::for_actor` だけでなく `with_queue`、`Mailbox::new`、`Mailbox::sharing` と、
  それらを呼ぶ単体テスト（`mailbox_test.rs` と `mailbox_run_test.rs` の 418 行目）も
  変える対象である（§4.1）。
- `Executor::deliver` にはもう 1 つ、経路を通らない分岐がある。境界から届いた封筒を
  生きている entry のメールボックスへ `enqueue` して断られた場合（`executor_value.rs` の
  2839〜2843 行目の `Err(refused)` 分岐）、`route_refused` を通さず
  `dead_letters_shared.route_undeliverable` を直接呼ぶ。この分岐は本番でも通る
  （entry が残っているのにメールボックスが閉じた、または bounded が満杯の場合）。

### 2.3 system message を黙って破棄する箇所

`SystemMessage` は `kernel/dispatch/system_message.rs` の列挙で、origin/main を
取り込んだ基点では `Create`、`Resume`、`Terminate`、`ChildTerminated` の 4 つだけである
（`pub(crate)`）。原典の他の種類は、それを送る段（段 4c 以降、#255 ほか）で足す。
§5.3 の表は原典の全種類との対応を示し、移植の列挙にまだ無いものはそう書いてある。
閉じたメールボックスに関わる箇所は 6 つある。

1. `Executor::send_system`（`executor_value.rs` の 572〜576 行目）。`Mailbox::system_send`
   の結果を `.ok()` で破棄する。この関数が送るのは `Create`（`restart_child` の 2988 行目、
   `apply_all_for_one` の 3096 行目、`finish_recreate` の 3327 行目）と `Resume`
   （`fail_supervisor` の 3170 行目、`finish_recreate` の 3336 行目）だけであり、
   `Terminate` と `ChildTerminated` はここを通らない（3 と 4）。
2. `Executor::stop`（1698 行目）。`Mailbox::system_send(&mailbox, SystemMessage::Terminate).ok()`
   を直接呼ぶ。閉じたメールボックスへのもう一度の停止は、ここで黙って消える。
3. `Executor::finish_terminate`（1748 行目）。
   `Mailbox::system_send(&entry.mailbox, SystemMessage::ChildTerminated { .. }).ok()` を
   直接呼ぶ。親が先に閉じている場合（停止の追い越し）は、ここで黙って消える。
4. `Executor::spawn_at`（1313 行目）。`Mailbox::system_send(&mailbox, SystemMessage::Create).ok()`
   を直接呼ぶ。こちらは生まれたばかりの開いたメールボックスなので、閉じた経路ではない。
   数え落とさないために挙げるが、§5.4 で直すのは 2 と 3 だけである。
5. `Executor::queued_to_dead_letters`（`executor_value.rs` の 1979〜2000 行目、`drop(system)`
   は 1990 行目）。アクターの停止時に `Mailbox::close` が返した system message を
   `drop(system)` で落とし、コメントに「system message は閉じたメールボックスには意味が無い」
   と書いてある（1984〜1987 行目）。原典の `Mailbox.cleanUp` は、残った system message を
   1 つずつ dead letter mailbox へ `systemEnqueue` する（§5.1）。
6. `Executor::stop_all`（`executor_value.rs` の 2622 行目付近）。`held.push(entry.mailbox.close())`
   の結果をまとめて `drop(held)` する。これはシステム全体の終了であり、扱いは §5.5 で分ける。

したがって「どの system message も閉じたメールボックスでは 1 か所で消える」は今のコードと
合わない。入口は `send_system`、`stop`、`finish_terminate` の 3 つに分かれ、種類によって通る
道が違う。この 3 つを同じ振り替えへ寄せるのが §5.4 である。

`SystemMessage` の列挙の doc（`kernel/dispatch/system_message.rs` の 27〜28 行目）と
`send_system` の doc（`executor_value.rs` の 570〜571 行目）は、どちらも「閉じたメールボックス
では破棄する」と書いたままである。§5.4 でこの 2 つの doc も、閉じたメールボックスでは原典の
dead letter の規則（抑制される種類は `SuppressedDeadLetter`、それ以外は `DeadLetter`）へ
流す、と直す。`system_enqueue` の doc が「呼び出し側の責任」と書いている点は変えない。

`Mailbox::system_enqueue` 自体は、閉じていれば `Err(message)` で返すだけで、
dead letters への振り替えは呼び出し側の責任になっている。原典は逆で、
`Mailbox.systemEnqueue` が閉じた状態（`NoMessage` が頭）を見ると自分で
dead letter mailbox へ振り替える（§5.1）。振り替えの責任が 1 か所に無いため、
呼び出し側ごとに破棄する・流すが分かれた。

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
  - **可視性。** `DeadLetterTrampoline` は `pub` にする。`FailureCatcher` と同じポートで
    あり、std の `SenderThreadTrampoline` を `fraktor-actor` の外（`fraktor-testkit` の
    `FraktorSpec`）から `ActorSystem` へ渡すためである。`SharedTrampoline` は kernel の
    既定なので `pub(crate)` のままにする。`SenderThreadTrampoline` は adaptor の `pub`
    である。`ActorSystem` には `with_failure_catcher` と同じ形のコンストラクタ
    `pub fn with_dead_letter_trampoline(name, boundary, trampoline: Arc<dyn
    DeadLetterTrampoline>)` と、`set_event_sink` と同じ形の差し替え
    `pub fn set_dead_letter_trampoline(&self, trampoline: Arc<dyn DeadLetterTrampoline>)`
    を置く。差し替えが要るのは、`FraktorSpec` が `with_failure_catcher` で組んだ
    システムにも後から入れられるようにするためである。`Executor` 側の
    `set_dead_letter_trampoline` は `pub(crate)` である。新しい公開の面は
    この 4 つ（ポート、std の実装、コンストラクタ、差し替え）であり、§7 の数え方も
    そう直す。
  - **持ち方。** `DeadLettersShared` は
    `trampoline: spin::Mutex<Option<Arc<dyn DeadLetterTrampoline>>>` を持つ。
    `bus` と同じ `spin::Once` にはしない。`Executor::new`（`ActorSystem::new` が呼ぶ）が
     kernel の既定 `SharedTrampoline` を入れたあと、`with_dead_letter_trampoline` が
     後から `SenderThreadTrampoline` に差し替える道が要るためである。
    `DeadLettersShared::new` は引数を取らず、空のまま作る。`Executor::new` がその場で
    `set_dead_letter_trampoline(Arc::new(SharedTrampoline::new()))` を呼び、
    `with_dead_letter_trampoline` はアクターを生む前に差し替える。差し替えは
    `Executor::lend` の中で行う。ジョブは実行器のロックの外で流れる（公開は実行器を
    借りない）。
    `publish` はロックの下で `Arc` を 1 つ複製し、ロックを離してから `run` へ渡す。
    ジョブは `Box<dyn FnOnce() + Send>` なので、`EventStreamBusWeak` の clone と
    イベントの型が `Send` であることを実装の段でコンパイラに確かめる。`SystemMessage` に
    `Send + Sync` の表明が要るかも同じ段で確かめる。
    差し替え前（構築の途中）に公開が起きた場合は、その場で実行する（今の入れ子と同じ）。
  - **std の実装を誰が入れるか。** 今の `ActorSystem::new`/`with_setup` は kernel の
    コンストラクタであり、`Executor::new` を通じて既定の `SharedTrampoline` を入れる。
    std 専用の `ActorSystem` コンストラクタはまだ無い。std 側の組み立ては
    `fraktor-testkit` の `FraktorSpec::with_hooks_and_settings`（`attach_wait_driver` を
    呼んでいる場所）が担っており、そこが `ActorSystem::with_failure_catcher` で組んだ
    システムに `set_dead_letter_trampoline(Arc::new(SenderThreadTrampoline::new()))` を
    呼ぶ。ポートしたテストで `ActorSystem::new` を直接使うものは、§7.1 のトランポリンの
    テストだけが明示的に `with_dead_letter_trampoline` を使う。#234 の複数スレッドの段では、
    std のディスパッチャ付き `ActorSystem` のコンストラクタが既定で
    `SenderThreadTrampoline` を入れることを、その段の完了条件に足す。それまでは
    `ActorSystem::new` の既定が `SharedTrampoline` であることを、§3.3 と §3.4 に明記する。
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
  ハンドラが送り手のスレッドで走る」は保てない。**`SharedTrampoline` は `Executor` ごと、
  すなわちシステムごとの実体である。** したがって「待ち行列はスレッド単位で、システムを
  区別しない」は既定では成り立たない（§3.3 の表を実装ごとに分ける）。
- `no_std` でも #296 のスタックあふれは残らない。既定もジョブを積んで反復で流すので、
  再帰の深さは送り直しの回数に比例しない。`cfg(feature = "std")` で直接公開へ落とす形は
  採らない。割り込みのある環境では、割り込みハンドラが `SharedTrampoline` の `spin::Mutex`
  を取らない（公開しない）ことを前提にする。取るなら、割り込みは公開をメインのループへ
  渡すだけで、ロックを保持したまま `run` を呼ばない。
- 待ち行列がシステムを区別しないのは、std の `SenderThreadTrampoline` を全システムが
  使う場合である。同じスレッドでシステム A の購読者がシステム B の停止済みアクターへ
  送る場合も、B の公開は A の最も外側のループが流す。送り手のスレッドは同じであり、
  順序はスレッド全体で先入れ先出しになる。システムごとに待ち行列を分ける案は、入れ子の
  入口ごとに印の照合が要り、壊れ方が増えるので採らない。kernel 既定の
  `SharedTrampoline` では、A のハンドラが B を駆動すると B は自分の
  `SharedTrampoline` の最外殻になり、その場で流れる（今の入れ子に戻る）。代償は
  §3.3 の表に実装ごとに書く。
- パニックへの備え：購読者のハンドラがパニックで巻き戻ると、印が立ったまま残り、
  そのスレッドの以後の公開がすべて待ち行列に積まれて流れなくなる。ループは
  印と待ち行列を守る RAII の番人を持ち、巻き戻りでは印を下ろし、残った公開は破棄する。
  破棄することは、パニックした配送の続きが保証されないという今の致命の扱い
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
  破棄することになる。今回の前提とぶつかるので採らない。

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
| 購読者のハンドラが別のシステムを駆動したとき | そのシステムのディスパッチャのスレッドが流す（ハンドラは待たない） | 入れ子の公開がその場で流れる | std の `SenderThreadTrampoline`: ハンドラが返るまで流れない（スレッド単位で、システムを区別しない）。kernel 既定の `SharedTrampoline`: その場で流れる（システムごとの実体なので、駆動された側が自分の最外殻になる） |
| 際限ない送り直し | 終わらない（メールボックスに積まれ続ける） | スタックがあふれて落ちる | 終わらない（待ち行列が回り続ける）。落ちはしない |
| 積み荷が `DeadLetter` の letter が落とされる規則（`route_undeliverable` の先頭、宛先を見ない） | 経路で違う（`onDeadLetterMailboxEnqueue` は積み荷の `DeadLetter` を落とすが、`DeadLetterActorRef.!` は中身の `specialHandle` を先に試す） | ある（`route_undeliverable` が宛先を見ずに落とす） | 変わらない |

変わる観察は次のとおりである。順序が深さ優先から公開単位の先入れ先出しになること、
入れ子の letter の購読者の集合が流す時点で決まることは、どちらの実装でも変わる。
3 つ目（購読者のハンドラが別のシステムの公開を待つと、その公開がハンドラの返るまで
流れないこと）は、std の `SenderThreadTrampoline` を入れたシステムだけに当たり、
kernel 既定の `SharedTrampoline` ではその場で流れる。どれも、今の深さ優先の順序に
利用者が頼ることはできない。深さ優先は再帰の作りから偶然生まれた強さであり、
`docs/decisions/pekko-semantics-first.md` の分け方では「作りのずれ」に当たる。
公開単位の先入れ先出しは原典のメールボックスの順序に近い。3 つ目は、システムごとの
待ち行列を採らなかったことの代償であり、std の実装を使うシステムでは利用者に
rustdoc の注意（§3.4）で伝える。

Quint：トランポリンが変えるのは同じ公開を流す深さと順序だけであり、
`actor_termination.qnt` の `sendSys`（193〜195 行目）が数える `sysDead` の値も、
`invariant`（558〜562 行目）が読む不変条件も変えない。この設計は
`actor_termination.qnt` と `dead_letter_listener.qnt` の仕様を変えない。

### 3.4 rustdoc の直し

`ActorRef::tell` の rustdoc は、実装の段で次の趣旨に書き換える。経緯は書かない。

- 停止済みの宛先への送信は dead letters へ流れ、購読者のハンドラは送り手のスレッドで
  走る。
- 購読者のハンドラは、別のアクターへ送ること（停止済みのアクターを含む）ができる。
  その送信で生じた dead letter も、入れ子にならずに流れる。
- 購読者のハンドラから、同じシステムの駆動（`run_until_idle`、`ask` の答えを待つこと、
  アクターを生む・止めること）を呼ばない。実行器は run の間貸し出されており、
  それらの呼びは待ってしまう。
- std の `SenderThreadTrampoline` を入れたシステムでは、購読者のハンドラから別の
  システムを駆動しない（`ask` の答えを待つことを含む）。待たれたシステムの dead letters は
  ハンドラが返るまで流れない（§3.3）。kernel 既定の `SharedTrampoline`（今の
  `ActorSystem::new`）では、その場で流れるのでこの待ちは起きない。

HEAD の `tell` の rustdoc（`kernel/actor_ref/actor_ref_value.rs` の 84〜92 行目）には、
すでに「If the actor has stopped, the message goes to dead letters on this (the sender's)
thread, before `tell` returns (issue #286)」と、同期とスレッドの約束・経緯が書かれている。
実装の段でこの文を書き換え、「`tell` が返る前に購読者が受け取る」という約束と issue の
経緯を消す。スレッドの事実（送り手のスレッドで走る）と、同じシステムの駆動を呼ばない
注意は残す。

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
   抑制の規則は同じであり、変わるのは呼ばれる場所だけである。購読者は `deliver` を
   回すスレッドで呼ばれる。そのスレッドが送り手のスレッドとは限らない（マルチスレッド
   段では、送り手が先に返っていることもある）。

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

1. dead letter mailbox の定義（`Mailboxes.scala` の 54〜66 行目）。
   `deadLetterMailbox` は、生成直後に `becomeClosed()` で閉じられたメールボックスで、
   中のキューは通常の封筒を `onDeadLetterMailboxEnqueue`（69〜73 行目）へ渡す。
   system message 用の上書きは 2 行である（`systemEnqueue`、62〜63 行目）。

   ```scala
   def systemEnqueue(receiver: ActorRef, handle: SystemMessage): Unit =
     deadLetters ! DeadLetter(handle, receiver, receiver)
   ```

   つまり、閉じたメールボックスへの system message は、`DeadLetter` に包まれて
   dead letters のハンドル（`DeadLetterActorRef`）へ送られる。包みの中身は
   system message そのもので、送り手と宛先はどちらも `receiver`、すなわち
   閉じたメールボックスの持ち主である。system message には別に送り手が無いので、
   この形しかない。
2. 閉じたメールボックスへの到達（`Mailbox.scala` の `systemEnqueue`、500〜512 行目。
   閉じた印の判定は 504 行目、転送は 505 行目）。
   通常のメールボックスの `systemEnqueue` は、キューの頭が `NoMessage`
   （閉じた印）なら、自分で `deadLetterMailbox.systemEnqueue(receiver, message)` を
   呼ぶ。呼び出し側が破棄する・流すを選ぶ作りではない。後片付けでも同じで、
   `Mailbox.cleanUp`（354〜369 行目）と `processAllSystemMessages`（303〜347 行目）の
   残りは、残った system message を 1 つずつ `dlm.systemEnqueue(actor.self, msg)` する。
3. dead letters のハンドルが `DeadLetter` を受けたとき（`ActorRef.scala` の
   `DeadLetterActorRef.!` と `EmptyLocalActorRef.specialHandle`）。
   ハンドルは、中身で特別扱いを先に試し、当たらなければ `DeadLetter` を
   そのままイベントストリームへ公開する。特別扱いは次のとおりである。
   - 中身が `Watch`：監視者へ `DeathWatchNotification(watchee, existenceConfirmed = false,
     addressTerminated = false)` を system message で返し、公開はしない。
     ただし監視者と監視対象のどちらかが dead letters 自身の場合は返さない。
   - 中身が `Unwatch`：`DeadLetter` として公開せず、黙って破棄する（`ActorRef.scala` の
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
| 1 | `postStop` の `unstashAll` があふれさせた分（bounded のときだけ） | `UnrestrictedStash.postStop`（99〜101 行目）→ `unstashAll`（226、244〜251 行目）→ `enqueueFirst`（270〜276 行目）。bounded であふれた分は `postStop` の中で断られ、`cleanUp` より先に dead letters へ行く（unbounded の `enqueueFirst` は 620 行目、bounded のあふれは 642〜650 行目） | 停止の入口で、`close()` の前に `entry.mailbox.number_of_messages()`（`mailbox.rs` 343〜345 行目）を読んで backlog を控える。`entry.stash` を取り、`discard_stash_overflow`（`executor_value.rs` 1934〜1947 行目）へ控えた backlog を渡して victims を流す（今は 1935〜1939 行目が自分で `number_of_messages()` を読む）。victims は原典どおり system message より先に流し、残りは `close()` のあとまで持つ |
| 2 | メールボックスの system queue に残った system message | `Mailbox.cleanUp`（354〜369 行目）: `systemDrain`（356〜365 行目）→ `dlm.systemEnqueue`（363 行目）。`Terminate` を処理した回に新しく届いたものもここ | `queued_to_dead_letters` が `close()` の `system` を順に `route_closed_system_message` へ渡す。`Terminate` は抑制、`Resume` と `Create` は `DeadLetter`（§5.4） |
| 3 | stash（`postStop` がキューの先頭へ戻した分と、1 で残した分） | `messageQueue.cleanUp`（367〜368 行目）。`unstashAll` は `enqueueFirst` を後ろから繰り返すので、stash はキューの先頭にあり、キューの封筒より先 | 1 で残した stash を、system のあと封筒の前に流す。段 3 の queue は `UnboundedQueue`（`spawn_at` の 1289 行目）だが、`DispatchSettings` の mailbox 容量は `entry.capacities` に入り、`discard_stash_overflow` はそれを使う（1283、1935〜1939 行目）。段 3 でも bounded の指定があれば victims は 0 件ではない |
| 4 | キューの通常の封筒 | 同じ `messageQueue.cleanUp` | `close()` が返した `envelopes` |
| 5 | `Terminate` と同じ `systemDrain` で取り出した残りの system message（段 4） | `processAllSystemMessages`（303〜347 行目）。`Terminate` の処理で閉じたあと、手元に残った `messageList` を 322〜341 行目で `dlm.systemEnqueue` するので、stash と封筒より後 | 移植は 1 通ずつ取り出す（`mailbox.rs` 405〜413 行目）ので「同じ回に取り出した残り」は無く、閉じたときにキューに残った分として 2 で先に流れる。段 3 で残るのは `Terminate`・`Resume`・`Create`（`#234` 設計 1976 行目と同じ集合）。`Terminate` は抑制、`Resume` と `Create` は `DeadLetter` でログに 1 行出る。原典は同じバッチの残りを stash と封筒の後ろに流すので、そこだけ順序が違う。段 4 で差が出るなら、`process_all_system_messages` を原典と同じバッチの形にして、閉じたあとの残りをキューの後片付けのあとに流す（#255 の段 4c の完了条件に足す） |
| 6 | `stop_all`（システム全体の終了） | ― | 破棄する（決定 3、§5.5） |

手順（`queued_to_dead_letters` にまとめる）は次のとおりである。

1. `close()` の前に `entry.mailbox.number_of_messages()` を読む（backlog）。
2. `entry.stash` を取り、backlog を `discard_stash_overflow` へ渡して victims を流す（表の 1）。
3. `entry.mailbox.close()`（`mailbox.rs` 310〜330 行目、drain は 318〜321 行目）。
4. system message を順に `route_closed_system_message` へ渡す（2）。
5. 残りの stash を流す（3）。
6. `close()` が返した封筒を流す（4）。

`close()` を先に呼ぶと `queue.drain()`（320 行目）で `number_of_messages()` が 0 になり、
`discard_stash_overflow` の部屋が `capacity - 0` に変わる。bounded の既存テスト
`bounded_stash_stop_counts_the_queued_backlog_against_the_mailbox_capacity`
（`actor_with_bounded_stash_spec.rs` 277〜296 行目）は、停止時に `late1`・`late2` が queue に
残っている前提で `room = 3 - 2 = 1` とし、`hello2, hello1, hello3, late1, late2` を
期待する。backlog を close の前に読めば `room = 3 - 2 = 1` のまま、victims は
`hello2, hello1`、残りは `hello3`、封筒は `late1, late2` で期待どおりになる。close の
あとで読むと `room = 3 - 0 = 3` になり、`hello1, hello2, hello3, late1, late2` になって落ちる。

`postStop` の `unstashAll` の道では、停止フックが `ActorContext::unstash_all` を呼ぶと、
`Executor::unstash_all`（2010〜2023 行目）の中の `discard_stash_overflow`（2021 行目）が
その場で victims を流し、残りを `send_first` でキューの先頭へ戻す。この場合は
`entry.stash` が空なので、手順 2 と 5 は何もしない。stash は `close()` の封筒に含まれ、
system message のあとに流れる（原典と同じ）。`stash_to_dead_letters`（1957〜1968 行目）は、
停止フックが `unstash_all` を呼ばなかったときに `entry.stash` を流す別の道であり、
手順 2 と 5 がこれに当たる。どちらの道も `discard_stash_overflow` を使う。

確かめたこと：stash は `UnrestrictedStash.postStop` の `unstashAll` でキューの先頭へ
戻り、`Mailbox.cleanUp` は system を先に、そのあと `messageQueue.cleanUp` で stash と
封筒を流す。順序は「あふれた分 → system message → stash → キューの通常の封筒」でよい。
`postStop` が `detach` より先であることは `FaultHandling.finishTerminate`（247〜276 行目、
254 行目の `aroundPostStop`、261 行目の `dispatcher.detach`）で確かめた。`detach` の
`unregister` が `mailbox.cleanUp()` を呼ぶ（`AbstractDispatcher.scala` 279〜285 行目、
284 行目）。

Quint：後片付けの順序・抑制・文面は `actor_termination.qnt` に無い。`finishTerminate`
（227〜241 行目）は閉じたときに残った `sys` を `sysDead` に足すだけであり（236 行目）、
この節の順序はモデルで検査できない。`actor_termination.qnt` の仕様は変えない。

### 5.2 抑制の対象かどうか

system message の種類のうち、`DeadLetterSuppression` を実装するのは原典で 2 つだけである。

- `Terminate`（`SystemMessage.scala` で `with DeadLetterSuppression`）
- `DeathWatchNotification`（同じく `with DeadLetterSuppression`）

`Terminated`（通常のメッセージ）や `PoisonPill` も抑制の対象だが、system message ではない。
`Create`、`Recreate`、`Suspend`、`Resume`、`Supervise`、`Watch`、`Unwatch`、`Failed` は
抑制の対象ではない。`Resume` は原典でも `DeadLetterSuppression` を実装しないので、
閉じたメールボックスでは抑制せず `DeadLetter(Resume)` として公開し、リスナーが 1 行書く
（§5.3、§5.4）。`Watch` と `Unwatch` は、抑制ではなく §5.1 の 3 の特別扱いで
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

Quint：`dead_letter_listener.qnt` の `receiveSuppressed`（129〜139 行目）は抑制を
種類非依存に扱う（`SuppressedKind` を受けて数えず、ログにも出さないだけである）。
どの種類を抑制するか、`ChildTerminated` を抑制にすることは、この仕様では検査できない。
`dead_letter_listener.qnt` の仕様は変えない。

### 5.3 対応表：原典の種類と移植の種類

| 原典の system message | 抑制 | dead letter mailbox へ行ったあとの形 | 移植の `SystemMessage` | 移植での扱い（この設計） | リスナーが書き出す文面 |
|:---|:---:|:---|:---|:---|:---|
| `Create` | 対象でない | `DeadLetter(Create, receiver, receiver)` を公開 | `Create`（今ある） | 同じ | `[fraktor_actor::kernel::dispatch::system_message::Create]` |
| `Recreate` | 対象でない | `DeadLetter(Recreate, receiver, receiver)` を公開 | `Recreate { cause }`（まだ無い。それを送る段で足す） | 同じ | `[fraktor_actor::kernel::dispatch::system_message::Recreate]` |
| `Suspend` | 対象でない | `DeadLetter(Suspend, receiver, receiver)` を公開 | `Suspend`（まだ無い） | 同じ | `[fraktor_actor::kernel::dispatch::system_message::Suspend]` |
| `Resume` | 対象でない | `DeadLetter(Resume, receiver, receiver)` を公開し、リスナーが 1 行書く（`Resume` は `DeadLetterSuppression` ではない） | `Resume`（今ある） | 原典の `Resume` は子への再開メッセージ（`resumeChildren`、`Children.scala` 216〜219 行目 → `InternalActorRef.resume`、`Dispatch.scala` 156〜158 行目）。移植の `SystemMessage::Resume` がそれである。閉じたメールボックスでは `DeadLetter(Resume)` として公開し、抑制せず、リスナーが `Message [...Resume]` を 1 行書く。自分の再開は `resume_mailbox` の直接呼び出し（`executor_value.rs` の 580〜585 行目、`system_invoke` は 1549〜1551 行目）で、メッセージではない（§5.4） | `[fraktor_actor::kernel::dispatch::system_message::Resume]` |
| `Terminate` | 対象 | `SuppressedDeadLetter(Terminate, receiver, deadLetters)` を公開 | `Terminate`（今ある） | 同じ | 出さない（`SuppressedDeadLetter` は数えない） |
| `Supervise` | 対象でない | `DeadLetter(Supervise, receiver, receiver)` を公開 | `Supervise { child, is_async }`（まだ無い） | 同じ | `[fraktor_actor::kernel::dispatch::system_message::Supervise]` |
| `Watch` | 対象でない（特別扱い） | 公開しない。監視者へ `DeathWatchNotification(watchee, false, false)` を返す | `Watch { watchee, watcher }`（まだ無い。段 4c） | §5.4 の 2。段 4c で `Watch` が届くようになったら、まずは通常の `Terminated` で返す（§6 の決定 4） | 出さない |
| `Unwatch` | 対象でない（特別扱い） | 公開しない。黙って破棄する（`specialHandle`、671 行目） | `Unwatch { watchee, watcher }`（まだ無い。段 4c） | 公開しない。`DeadLetter` としても `SuppressedDeadLetter` としても出さず、黙って破棄する | 出さない |
| `Failed` | 対象でない | `DeadLetter(Failed, receiver, receiver)` を公開 | `Failed { child, cause }`（まだ無い） | 同じ | `[fraktor_actor::kernel::dispatch::system_message::Failed]` |
| `DeathWatchNotification` | 対象 | `SuppressedDeadLetter(DeathWatchNotification, receiver, deadLetters)` を公開 | `DeathWatchNotification { actor, existence_confirmed, address_terminated }`（まだ無い。段 4c） | 段 4c で同じにする。今は、閉じた監視者への `watch_with` のカスタム通知が `enqueue_termination` の閉じた分岐で `DeadLetter(custom, watcher, watcher)` として公開され、ログに 1 行出る（原典では出ない）。#297 で、閉じた監視者への通知はカスタムの有無にかかわらず `Message::new(notice)`（`Terminated`）を `route_undeliverable` へ渡す形に直す。`Terminated` は抑制の対象なので `SuppressedDeadLetter` になり、ログは出ない。カスタム文は破棄する（原典では DWN が届かないのでカスタム文は作られない）。段 4c で DWN を送るようになったら、原典どおり `DWN` を抑制として流す | 出さない |
| （原典に無い） | ― | ― | `ChildTerminated { child }`（今ある） | 抑制の対象とする（元帥の決定、2026-11-27。§6 の決定 5）。`SuppressedDeadLetter(ChildTerminated, receiver, deadLetters)` として公開する | 出さない |

リスナーの文面の列は、`DeadLetterListener` の `Message [<名前>] …` の `<名前>` の部分である
（`DeadLetter` として公開された system message だけ）。原典は `unwrapped.getClass.getName`
で種類ごとのクラス名を出す（`DeadLetterListener.scala` 139〜140 行目）。移植は今
`Message::payload_type_name()`（`core::any::type_name::<T>()`）を出すので、列挙をそのまま
包むとどの種類も `SystemMessage` と出る（レビュー高 3）。`SystemMessage` に種類ごとの
`class_name` を持たせ、リスナーがそれを読む形にする（§5.4）。移植の名前空間は原典と
違う（`org.apache.pekko.dispatch.sysmsg.Create` ではなく
`fraktor_actor::kernel::dispatch::system_message::Create`）ので、ログの文字列そのものは
原典と一致しない。種類が消えないことが主眼であり、原典と同じ文字列にすることは
この設計の条件にしない。種類を足す段で `class_name` と実際の `type_name` がずれないかを
確かめる。

原典の `NoMessage` は閉じた印そのものであり、system message として送られることは
ない。移植の列挙に対応は無いので、扱いは要らない。

### 5.4 直す形と面（シグネチャ）

振り替えの責任を 1 か所に集める。この節で足す面は 2 つで、どちらも `pub(crate)` である。
公開 API が増えるのは §3.1 の 4 つ（`DeadLetterTrampoline`、`SenderThreadTrampoline`、
`ActorSystem::with_dead_letter_trampoline`、`ActorSystem::set_dead_letter_trampoline`）
だけで、§7 の数え方もそう直す。

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
     `DeadLetter(handle, receiver, receiver)` と同じである。`Create`、`Resume`、
     `Recreate`、`Suspend`、`Supervise`、`Failed` がこの腕に来る。`Resume` は原典の
     子への再開メッセージ（`resumeChildren`）に当たり、抑制の対象ではないので、
     `DeadLetter(Resume)` として公開し、リスナーが 1 行書く。
   - `Watch`、`Unwatch` はこの関数へ来ない（2 が先に分ける）。
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

   - `Terminate`、`ChildTerminated`、それ以外の公開する種類は 1 へ渡す。
   - `Resume` も 1 へ渡す。原典の `Resume` は子への再開メッセージ（`resumeChildren`、
     `Children.scala` 216〜219 行目 → `InternalActorRef.resume`、`Dispatch.scala`
     156〜158 行目）であり、移植の `SystemMessage::Resume` がそれである。自分の再開は
     `resume_mailbox` の直接呼び出し（`executor_value.rs` の 580〜585 行目）で、
     メッセージではない。`finishRecreate` の `resumeNonRecursive()`（`FaultHandling.scala`
     283 行目）は後者に当たる。`Resume` を落とす腕は作らない。
   - `Watch`、`Unwatch`、`DeathWatchNotification` は、段 4c（#255）で列挙に足すときに
     この関数へ腕を足す。`Watch` は決定 4 の形（`enqueue_termination` の道で
     `Terminated` を返し、dead letters 自身への watch には答えない）、`Unwatch` は
     原典の `specialHandle`（671 行目）どおり黙って破棄する、`DeathWatchNotification` は
     1 の抑制の腕へ足す。今の列挙にはこれらの型が無いので、段 3 では通らない。

   呼び出し側の直しは 4 か所である。
   - `send_system`（572〜576 行目）：`.ok()` をやめ、`Mailbox::system_send` の
     `Err(message)` を `route_closed_system_message(message, target.clone())` へ渡す。
     引数を取るために `&self` から `&mut self` にする。呼び出し側はすべて `&mut self` の
     中にある（`restart_child` の 2988 行目、`apply_all_for_one` の 3096 行目、
     `fail_supervisor` の 3170 行目、`finish_recreate` の 3327・3336 行目）。
     `create_now` と `stop` は `send_system` を呼ばない。
   - `stop`（1698 行目）：`Mailbox::system_send(&mailbox, SystemMessage::Terminate).ok()`
     の `.ok()` をやめ、`Err(message)` を `route_closed_system_message` へ渡す。
   - `finish_terminate`（1748 行目）：`ChildTerminated` の直接の `system_send` も同じ形に
     する。親が先に閉じている場合は、抑制として記録される。
   - `queued_to_dead_letters`（1979〜2000 行目）：`drop(system)`（1990 行目）を消し、
     `close` が返した system message を順に `route_closed_system_message` へ渡す。
     後片付けは §5.1 の手順にまとめる。`close()` の前に `number_of_messages()` で
     backlog を読み、stash の victims を先に流し、`close()` のあとに system message、
     残りの stash、封筒の順に流す。今は `finish_terminate` が `stash_to_dead_letters` を
     先に呼び（1743 行目）、`queued_to_dead_letters` が封筒を流す（1744 行目）ので、
     この 2 呼びを `queued_to_dead_letters` 1 つにまとめる。`discard_stash_overflow` は
     backlog を引数に取る形に変える（§5.1）。`Mailbox::close` は cell も `entry.stash` も
     触らないが、queue は drain する（`mailbox.rs` 318〜321 行目）ので、部屋の計算に使う
     backlog は close の前に読む。
   - `spawn_at` の `Create`（1313 行目）は、生まれたばかりの開いたメールボックスへ送るので
     直さない。数え落とさないために §2.3 に挙げてある。
   - `enqueue_termination` の閉じた監視者の分岐（3449〜3450 行目）は、§5.3 の DWN の行の
     とおり、カスタムの通知文ではなく `Message::new(notice)`（`Terminated`）を
     `route_undeliverable` へ渡す形に直す。これは system message の入口ではないが、
     同じ「閉じたメールボックスへの通知を抑制する」直しであり、変更の一覧に入れる。

   doc の直しも変更の一覧に入れる。
   - `SystemMessage` の列挙の doc（`kernel/dispatch/system_message.rs` の 27〜28 行目）は
     「閉じたメールボックスでは drop する」と書いてある。閉じたメールボックスでは原典の
     dead letter の規則（抑制される種類は `SuppressedDeadLetter`、それ以外は `DeadLetter`）
     へ流す、と直す。
   - `send_system` の doc（`executor_value.rs` の 570〜571 行目）も同じく「drops it」を
     やめ、`Err` を `route_closed_system_message` へ渡すと直す。
   - `ActorRef::tell` の rustdoc（`kernel/actor_ref/actor_ref_value.rs` の 84〜92 行目）は
     §3.4 のとおりに直す。

   段 3 で残る system message は `Terminate`・`Resume`・`Create` である（`#234` 設計の
   1976 行目と同じ集合）。`Terminate` は抑制で、ログには出ない。`Resume` と `Create` は
   `DeadLetter` として公開され、ログに 1 行ずつ出る。件数が増えるテストの洗い出しは
   §7.4 に置く。

   `Mailbox` 側は変えない。`system_enqueue` が `Err` で返す今の形は、メールボックスが
   dead letters の規則を持たないための切り分けであり、振り替えの責任は実行器と
   `DeadLettersShared` に集める。原典と作りは違うが、観察（何が、どの形で、
   どの順序で公開されるか）は §5.3 の表どおりにそろえる。

   段 3 では、原典にメッセージとして対応のある「黙って破棄する」は 0 件になる。閉じた
   メールボックスへの `Resume` も `DeadLetter(Resume)` として流し、リスナーが 1 行書く。
   黙って破棄するのは、原典の `Unwatch`（段 4c で型が入る）と、システム全体の終了
   （`stop_all`、決定 3、§5.5）だけである。#297 は §5.3 の対応表で閉じる。
   ケースを数から外す扱いはしない。

### 5.5 範囲の外：`stop_all` の破棄

`Executor::stop_all` が `close` の結果をまとめて破棄する点は、この設計では直さない。
システム全体が終了する時点では、dead letters の公開先であるイベントストリーム自体が
終わる途中にあり、プルリクエスト #295 で決めた「終了したシステムは断られた封筒を
破棄する」という振る舞い（回帰テストで固定済み）とぶつかる。アクターが 1 つずつ
止まる通常の停止は `queued_to_dead_letters` の道を通るので、§5.4 で原典にそろう。
`stop_all` の一括の後片付けまで原典の `cleanUp` と同じにすべきかどうかは、
元帥は破棄することを決めた（§6 の決定 3）。

## 6. 元帥の決定（2026-11-27）

意味論に関わる選択は、元帥が 2026-11-27 にすべて決めた。決めた人は元帥である。
実装はこの決定に従い、変える場合は元帥の新しい決定が要る。決定 6 は、再レビューのあと
（2026-11-27 14:4x）に `Resume` の扱いを読み替えた。閉じたメールボックスに残った
`Resume` は `DeadLetter(Resume)` として公開し、抑制しない。

1. トランポリンを採る。候補 1（§3.1）のとおり、入れ子の letter は先入れ先出しで
   流し、購読者の集合は流す時点で決まる。順序が深さ優先から公開単位の
   先入れ先出しへ変わることと、購読者の集合の決まる時点が遅れることは、
   作りのずれの解消として認められた。印と待ち行列は kernel のポート
   `DeadLetterTrampoline` が持ち、std の adaptor がスレッドごとの実装を、kernel の
   既定がスレッドの無い環境向けの実装を持つ（§3.1）。
2. 深さの上限は置かない。原典に上限は無く、上限を超えた letter を失う形は採らない。
3. `stop_all` のあとに残ったものは破棄する。システム全体の終了で閉じた
   メールボックスに残った system message と通常の封筒は、dead letters へ
   流さず、今のとおり破棄する（§5.5）。
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
6. 停止の後片付けは「あふれた分 → system message → stash → キューの通常の封筒」の順に
   流す。`queued_to_dead_letters` は、`close()` の前に queue の backlog を読み、stash の
   victims を先に流し、`close()` のあとに system message、残りの stash、封筒を流す
   （§5.1 の手順）。原典では `postStop` の `unstashAll` があふれさせた分を先に流して
   stash をキューの先頭へ戻し、`Mailbox.cleanUp` が system を先に、`messageQueue.cleanUp`
   をあとに流すので、この順になる。`Resume` は原典の子への再開メッセージ
   （`resumeChildren`）であり、`DeadLetter(Resume)` として公開し、抑制しない（§5.3、
   §5.4）。したがって段 3 の後片付けで system message の dead letter は増えうる。
   残るのは `Terminate`・`Resume`・`Create` で、`Terminate` は抑制、`Resume` と
   `Create` は 1 件ずつログに出る。`Terminate` と同じ `systemDrain` で取り出した残りの
   system message をキューの後片付けのあとに流す原典の順序（`Mailbox.scala` 322〜341 行目）
   は、段 4c で `process_all_system_messages` をバッチの形にするときに合わせる
   （§5.1 の表）。

この節に無かった扱いとして、閉じたメールボックスへの `Unwatch` も元帥が
確かめた。原典どおり、`DeadLetter` として公開せず黙って破棄する（§5.1 の 3、
§5.3 の表）。段 4c で `Unwatch` が列挙に入ったときに、§5.4 の 2 の腕として足す。

## 7. テストに要る面

テストの担当（Opus）は、この節の面だけを使って red のテストを先に書く。
テストのために新しい API は作らない。§5.4 で足す面は 2 つで、どちらも `pub(crate)` で
ある。§3.1 が足す公開の面は `DeadLetterTrampoline`、adaptor の
`SenderThreadTrampoline`、`ActorSystem::with_dead_letter_trampoline`、
`ActorSystem::set_dead_letter_trampoline` の 4 つで、`with_failure_catcher` と同じ
組立の面であり、意味論の約束は増やさない。§7.1 は `SenderThreadTrampoline` を
明示的に入れるために `with_dead_letter_trampoline` を使う（既定の `SharedTrampoline`
だけでは std の実装が観測されないため）。

### 7.1 #296 中 1 のテスト（公開面だけで書ける）

置き場所は新しい `crates/fraktor-actor/tests/dead_letters_trampoline_test.rs` に限る。
既存の `dead_letters_on_sender_thread_test.rs` へ足さない。今のコードでは送り直し
1000 回が `fatal runtime error: stack overflow, aborting`（SIGABRT）でプロセスごと
落ちるので、同じファイルの既存のテストも結果を出せなくなる（レビュー中 5）。
red の間は `cargo test --test dead_letters_trampoline_test` のようにこのファイルだけを
回す。使う面は既存のもの（`ActorSystem`、`function_ref`、イベントストリームへの購読、
`ActorRef::tell`）と、§3.1 の `ActorSystem::with_dead_letter_trampoline` および adaptor の
`SenderThreadTrampoline` である。システムは
`ActorSystem::with_dead_letter_trampoline(name, boundary, Arc::new(SenderThreadTrampoline::new()))`
で作る。既定の `SharedTrampoline` でも今の 1 スレッドでは同じ観察になるが、std の
実装（TLS の印・待ち行列・`try_with`）をこのテストで観測するために、明示的に入れる。

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

- `Executor::route_closed_system_message` と `send_system`/`stop`/`finish_terminate`/
  `queued_to_dead_letters` のテスト（`kernel/executor/executor_value_test.rs`）：既存の
  `death_watch_executor` と `record_dead_letters` の組み立てを使う。ただし
  `record_dead_letters` は積み荷を `&'static str` として読むので、system message 用に、
  `DeadLetter` の中身を `SystemMessage` として読む記録器を同じファイルに足す。
  確かめることは、閉じたメールボックスへの `Resume` が `DeadLetter(Resume, target, target)`
  として記録され、リスナーが `Message [...Resume]` を 1 行書くこと、`Create` が
  `DeadLetter(Create, target, target)` として記録されること、`Terminate` と
  `ChildTerminated` が `SuppressedDeadLetter` として記録されること、停止時に残った
  system message が `queued_to_dead_letters` から「あふれた分 → system → stash → 封筒」の
  順で流れることである。`Terminate` と `ChildTerminated` は、`route_closed_system_message`
  を直接呼ぶだけでなく、`stop` の 1698 行目と `finish_terminate` の 1748 行目の
  直接の `system_send` を通る形でも確かめる（直接呼びだけでは入口の直しを見ていない）。
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
  `DeadLetterListener::new` と `on_event` に `AnyDeadLetter::DeadLetter(&dead)` を渡す
  （`on_event` は `&AnyDeadLetter` を取るので、`DeadLetter` をそのまま渡さず、
  `AnyDeadLetter::DeadLetter` を組み立てる）。`dead` は
  `DeadLetter::from_message(Message::new(SystemMessage::Create), target.clone(), target)`
  で作り、返る行が `[fraktor_actor::kernel::dispatch::system_message::Create]` を含むことを
  確かめる。`SystemMessage` は `pub(crate)` なので統合テストからは作れないが、
  `executor_value_test.rs` や `mailbox_run_test.rs` などクレート内の他の単体テストからは
  作れる。
- `Watch` と `Unwatch` は段 3 の列挙に無いので、テストも段 4c で足す。決定 4 の答えは
  そのとき統合テストで確かめる。
- 統合テストで書けるのは、`Resume` を子へ送ったあとに停止が追い越す形ではなく、
  停止したアクターの後片付けで system message が残る形が公開面から作れる場合だけである。
  作れない場合は、単体テストで足りる理由をテストファイルの冒頭に書く
  （`SystemMessage` がカーネルの内側の型であるため）。

### 7.3 bounded が満杯の経路のテスト

満杯で断られる道は、閉じて断られる道と同じ `route_refused` を通る。`spawn_at` は
`UnboundedQueue`（1289 行目）を作るため、キューが満杯で断る道はアクターシステムの
公開面からは作れない。この経路の本番化は段 5（#256）の範囲である。一方、stash の
あふれは `DispatchSettings` の mailbox 容量（`entry.capacities`、1283 行目）で
公開面からも起こりうる。それは §5.1 の victims と、既存の
`bounded_stash_stop_counts_the_queued_backlog_against_the_mailbox_capacity` が扱う。
ここで扱う「満杯で断られる封筒」とは別である。

- #256 が先に進む場合は、満杯のテストは #256 側に置く。置く場所は、
  送り手のスレッドのテストと同じファイルがよい。確かめることは、容量 1 の
  bounded メールボックスが満杯のときに 2 件目の送信が送り手のスレッドで
  dead letters へ流れ、`tell` が返る時点で購読者が受け取っていることである。
- #256 より先にこの issue で置く場合は、カーネルの単体テストに限る。
  `BoundedQueue` と `Mailbox::for_actor`（§4.1 で経路が必須になる）で組み立て、
  経路が満杯の封筒を受け取ることを確かめる。この場合も、公開面のテストは
  #256 へ残すとテストファイルに書く。

### 7.4 黙って消えることに頼る既存のテストの洗い出し

実装の段の前に、次の手順で洗う。洗い出し自体はテストの担当の仕事に含める。

1. `crates/fraktor-actor/tests` と `src` の単体テストから、dead letters の件数を
   確かめているテストをすべて挙げる（`DeadLetter` と `SuppressedDeadLetter` の
   購読、dead letter のログの記録の両方）。
2. そのうち、失敗と監督の流れ（suspend と resume を含む）か、アクターの停止の
   後片付けを通るテストを印する。§5.4 のあとでは、閉じたメールボックスに残った
   `Resume` も `DeadLetter(Resume)` として公開されるので、件数は増えうる。既存の
   `tests/ticket669_spec.rs::be_able_to_reply_on_failure_during_pre_restart` と
   `tests/supervision_all_for_one_regression.rs::sibling_restart_hook_cannot_stop_the_failed_child_before_its_reply`
   は、子へ送った `Resume` が停止に追い越されて閉じたメールボックスに残る道を通る。
   件数が動きうるのは、`Resume` の `DeadLetter` が増える場合、停止に追い越された
   `Create` の `DeadLetter` が増える場合、`watch_with` の閉じた監視者への通知が
   `DeadLetter(custom)` から `SuppressedDeadLetter` に変わる場合である。
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

- `dispatch/Mailboxes.scala`：dead letter mailbox の定義（54〜66 行目。`systemEnqueue`
  （62〜63 行目）が `deadLetters ! DeadLetter(handle, receiver, receiver)`、`enqueue` が
  `onDeadLetterMailboxEnqueue`（69〜73 行目）で、`DeadLetter` の積み荷は落とす）。
- `dispatch/Mailbox.scala`：`systemEnqueue`（500〜512 行目）が閉じた状態で dead letter
  mailbox へ振り替えること、`cleanUp`（354〜369 行目）が system を先に
  `messageQueue.cleanUp` をあとに流すこと、`processAllSystemMessages`（303〜347 行目、
  残りは 322〜341 行目）の残りが `dlm.systemEnqueue` で流れること。
- `actor/Stash.scala`：`UnrestrictedStash.postStop`（99〜101 行目）、`unstashAll`
  （226、244〜251 行目）、`enqueueFirst`（270〜276 行目）が stash をキューの先頭へ
  戻すこと。
- `actor/dungeon/Children.scala` と `actor/dungeon/Dispatch.scala`：`resumeChildren`
  （216〜219 行目）が子へ `Resume` を送ること（`Dispatch.scala` 156〜158 行目）。
- `actor/dungeon/FaultHandling.scala`：`finishRecreate`（278 行目以降、283 行目の
  `resumeNonRecursive()`）と `finishTerminate`（247〜276 行目、254 行目の
  `aroundPostStop`、261 行目の `dispatcher.detach`）。
- `dispatch/AbstractDispatcher.scala`：`unregister`（279〜285 行目）が
  `mailbox.cleanUp()` を呼ぶこと（284 行目）。
- `actor/ActorRef.scala`：`DeadLetter`、`SuppressedDeadLetter`、`DeadLetterSuppression` の
  定義と、`DeadLetterActorRef.!` と `EmptyLocalActorRef.specialHandle` の特別扱い
  （`Watch` への答え、`Unwatch` の無視、抑制の公開）。
- `dispatch/sysmsg/SystemMessage.scala`：system message の種類と、`Terminate` と
  `DeathWatchNotification` だけが `DeadLetterSuppression` を実装すること。
- `event/DeadLetterListener.scala`：`logDeadLetter` の `unwrapped.getClass.getName`
  （139〜140 行目）と、抑制された letter がログに数えられないこと。

## 10. 変更の記録（2026-11-27 の改訂）

独立レビュー（`pr298-design-review.md`）、再レビュー（`pr298-design-rereview.md`）の指摘と、
元帥の決め直しを受けて変えた点を、指摘ごとに書く。採らなかったものは理由も書く。
ここは変更の記録であり、実装の手順ではない（手順は §8）。

### 元帥の決め直し（再レビューのあと）

- 高 1（`Resume`）：再レビューで前提が逆だと分かったので、元帥が 2026-11-27 に読み替えた。
  移植の `SystemMessage::Resume` は原典の子への再開メッセージ（`resumeChildren`、
  `Children.scala` 216〜219 行目 → `Dispatch.scala` 156〜158 行目）であり、自分の再開は
  `resume_mailbox` の直接呼び出しである。閉じたメールボックスに残った `Resume` は
  `DeadLetter(Resume)` として公開し、抑制せず、リスナーが 1 行書く。§5.3 の表、§5.2、
  §5.4、決定 6、§7.2、§7.4 をこの向きにそろえた。前の版の「原典にメッセージとしての
  対応なし」「閉じたメールボックスでは破棄」は取り下げた。
- 高 4（後片付けの順序）：順序は「あふれた分 → system message → stash → 封筒」のまま。
  `close()` を先に呼ぶと queue が drain され、`discard_stash_overflow` の部屋の計算
  （`number_of_messages()`）が変わるので、`close()` の前に backlog を読む手順にした
  （§5.1）。bounded の既存テスト
  `bounded_stash_stop_counts_the_queued_backlog_against_the_mailbox_capacity`
  （`actor_with_bounded_stash_spec.rs` 277〜296 行目）が壊れないことを手順に書いた。
  §5.1 の行 1 は `Executor::unstash_all` の `discard_stash_overflow` と、
  `stash_to_dead_letters` の別の道に分けて書いた。
- 決定 6 は、段 3 で残る system message を `Terminate`・`Resume`・`Create` に統一し、
  `Resume` と `Create` が `DeadLetter` で 1 行ずつ出る（件数が増えうる）と直した。

### 設計の担当が決めたもの

- 再レビュー 3（入口）：`send_system` だけでなく、`stop`（1698 行目）と
  `finish_terminate`（1748 行目）の直接の `system_send` も変更の一覧に入れた。
  `SystemMessage` の列挙の doc（`system_message.rs` 27〜28 行目）と `send_system` の
  doc（`executor_value.rs` 570〜571 行目）の「drop」も直す対象にした。
- 再レビュー 4（トランポリン）：kernel 既定の `SharedTrampoline` はシステムごとの実体で
  あり、「待ち行列はスレッド単位で、システムを区別しない」は既定では成り立たない。
  §3.1 と §3.3 を kernel 既定と std の `SenderThreadTrampoline` で書き分けた。std が
  入れる道（`FraktorSpec` と、#234 の複数スレッド段の std コンストラクタ）を書き、
  可視性は `DeadLetterTrampoline`・`SenderThreadTrampoline`・
  `ActorSystem::with_dead_letter_trampoline`・`ActorSystem::set_dead_letter_trampoline`
  を `pub` にすると決めた（§7 の数え方も直した）。`DeadLettersShared` のトランポリンは
  `spin::Mutex<Option<...>>` にし、`new` は引数を取らず、`Executor::new` が既定を入れ、
  `with_dead_letter_trampoline`/`set_dead_letter_trampoline` が後から差し替える形にした。
  no_std の割り込みがロックを取る場合の前提も 1 行足した。
- 再レビュー 5（Quint）：`actor_termination.qnt` の `sendSys`（193〜195 行目）は閉じた
  メールボックスへの全 `Sys`（`Resume` を含む）を `sysDead` に数える。この向きのままで
  よい（`Resume` を `DeadLetter` にする設計と一致する）。抑制・文面・順序はモデルに無い。
  §3・§5.1・§5.2 に、どの `.qnt` の何を保つか（仕様を変えないか）を 1 行ずつ書いた。
- 高 2（kernel の std）：`#[cfg(feature = "std")]` と `std::thread_local!` を kernel に
  置く案を採らなかった。kernel に `DeadLetterTrampoline` のポートを置き、std の adaptor の
  `SenderThreadTrampoline` が `thread_local!` で印と待ち行列を持つ。kernel の既定は
  スレッドの無い環境向けの `SharedTrampoline` で、これもジョブを積んで反復で流すので、
  no_std でも #296 のスタックあふれは残らない。`kernel/sync/counts.rs` と
  `adaptor/ask_driver.rs` を先例に挙げた前の版は取り下げた。
- 高 3（ログの文面）：前の版のとおり `SystemMessage::class_name`。列の文字列は移植の
  名前空間であり、原典の文字列とは一致しないことを §5.3 に書いた。
- 中 1〜中 5、低 1〜低 2：前の版で直した点はそのまま。中 4 の rustdoc は、HEAD の
  `tell` の 84〜92 行目の文を名指しして書き換えることにした。低 4 の「待ち行列が
  システムを区別しない」は std の実装だけに当たると直した（再レビュー 4）。
- 低 3：§5.4 の 2 の入口が `Resume` を先に分ける形はやめ、`Resume` も 1 へ渡す形にした。

### 行番号・参照の直し（再レビュー 7）

v2.0.0 と HEAD で引き直した。`Mailboxes.scala` 54〜66（`systemEnqueue` 62〜63）、
`Mailbox.scala` の `systemEnqueue` 500〜512、`cleanUp` 354〜369、`processAllSystemMessages`
303〜347（残り 322〜341）、`enqueueFirst` は unbounded 620・bounded のあふれ 642〜650、
`Stash.scala` 244〜251、`AbstractDispatcher.scala` 279〜285、`FaultHandling.scala` の
`finishTerminate` 247〜276 と `finishRecreate` の `resumeNonRecursive()` 283、
`deliver` の `Err(refused)` 2839〜2843、`send_system` の呼び出し側は `restart_child`
（2988）・`apply_all_for_one`（3096）・`fail_supervisor`（3170）・`finish_recreate`
（3327・3336）、`Mailbox::for_actor` は `spawn_at`（1291）と `mailbox_run_test.rs:418`、
`ActorRef::for_mailbox` は `spawn_at`（1295）。§2・§5・§9 の該当箇所をこの値に直した。

### 範囲の外のまま

- レビューの別件（`Kill` が原典では `DeadLetterSuppression` を実装しないのに、移植の
  `dead_letters_shared.rs` が `"Kill"` を抑制している）は、この設計では触らない。
  別の issue で確かめる。
