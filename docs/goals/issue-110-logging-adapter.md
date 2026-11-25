# ゴール G0issue110：LoggingAdapterTest

[Issue #110](https://github.com/j5ik2o/fraktor-ng-rs/issues/110) の対象は Pekko の `LoggingAdapterTest`（Java）。原典は `LoggingAdapterTest.java` の 4 ケースと、その 2 ケースが使う `ActorWithMDC.java` の 1 アクターである。原典は、ロガーへテンプレートと未展開の引数を渡す書式化が購読イベントの本文・レベル・MDC に現れること、原因付きログが送信した原因オブジェクトそのものを届けること、MDC をメッセージごとに設定・全レベル発行・クリアするアクターの連続した発行が購読先で観測できること、未指定 MDC が空になること、旧コンストラクターが引き続きイベントを生成できることを確かめる。

- 原典（`LoggingAdapterTest.java`）: https://github.com/apache/pekko/blob/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/actor-tests/src/test/java/org/apache/pekko/event/LoggingAdapterTest.java
- 原典（`ActorWithMDC.java`）: https://github.com/apache/pekko/blob/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/actor-tests/src/test/java/org/apache/pekko/event/ActorWithMDC.java

## 対象ノード

| id | Rust の統合テスト | 原典のケース数 |
|:---|:---|---:|
| `actor-tests/src/test/java/org/apache/pekko/event/LoggingAdapterTest.java` | `crates/fraktor-actor/tests/logging_adapter_test.rs` | 4 |

原典のケース名は `mustFormatMessage`、`mustCallMdcForEveryLog`、`mustSupportMdcNull`、`mustBeAbleToCreateLogEventsWithOldConstructor`。除外は 0 件。`ActorWithMDC.java` はケースではなく、`mustCallMdcForEveryLog` と `mustSupportMdcNull` が使うアクターであり、独立したケースとしては数えない。第1工程でテストを移植して red を採証し、第2工程で実装と台帳更新を行った。

## 変更範囲

対象テスト、書式化・MDC・原因の保持に必要なログ発行口と既存イベントの拡張、補助テスト、対象ノードの台帳欄、このゴール文書。

## 原典とテストの個別照合

Pekko commit `9a54d1294c3ac7821bbed3c940769c3d5027d1ff` の `LoggingAdapterTest.java` と `ActorWithMDC.java` の全文を取得し、作成した Rust テストと照合した。

| 原典のケース | Rust のケース | 駆動入口 | 購読・受信 | 直接観測する契約 |
|:---|:---|:---|:---|:---|
| `mustFormatMessage` | `must_format_message` | `LogPublisher` の単一入口 `log(level, message, args)` と Error 専用の原因付き `error(cause, message, args)` | 同じ `ActorSystem` の `event_stream` に `TestProbe` を `Error`/`Warning`/`Info`/`Debug` で購読し、`probe.expect_msg_type::<E>` で受信 | 1 引数 Error 本文 `One arg message: the arg`・空 MDC／原因付き 2 引数 Error 本文 `Two args message: an arg, another arg`・`Arc::ptr_eq` で送信原因と受信原因の同一性／数値配列 Warning 本文 `Args as array of primitives: 10, 20, 30`／異種配列 Info 本文 `Args as array of objects: A String, Sun Feb 10 16:02:20 UTC 2019, 4192af46-487c-4132-84e8-87890cbb00a6`／4 引数 Debug 本文 `Four args message: 1, 2, 3, 4`。各イベントのレベルと空 MDC を比較 |
| `mustCallMdcForEveryLog` | `must_call_mdc_for_every_log` | `ActorWithMDC` 相当の `ActorWithMdc` を `Props::new` で 1 度だけ起動し、5 メッセージを順に送る（`Props` factory は `adapter.clone()` を捕捉し、`Actor::receive` が同じ adapter を保持する） | 同じ `ActorSystem` の `event_stream` に `TestProbe` を購読し、同じ probe のキューから期待レベルの具象型（`Error`/`Warning`/`Info`/`Debug`/`Debug`）で `expect_msg_type` を順に読む（`Debug` 一型で読まない／別キューで読まない） | 5 回の発行の本文・レベル・MDC 全体を比較。MDC は `messageLength` が `8`・`9`・`9`・空・`29`。4 回目（`No MDC for 4th call`）が空、直後の 5 回目（`And now yes, a debug with MDC`）が `29` へ復帰することを同じアクターの連続処理として観測 |
| `mustSupportMdcNull` | `must_support_mdc_null` | `Props::new` で起動した `ActorWithMdc` へ `Null MDC` を 1 通送る（アダプターへ MDC 未指定＝`None` を渡す） | 同じ `ActorSystem` の `event_stream` に `TestProbe` を `Info` で購読し受信 | 未指定（null 相当）で実発行された Info の本文 `Null MDC`・レベル Info・空 MDC |
| `mustBeAbleToCreateLogEventsWithOldConstructor` | `must_be_able_to_create_log_events_with_old_constructor` | 既存コンストラクター `Error::with_cause`、`Error::new`、`Warning::new`、`Info::new`、`Debug::new` | 購読なし。生成したイベントを直接読む | 原典の 5 つの `new` に対応する 5 経路がイベントを生成し、本文 `The message` と各レベル、空 MDC を持つ。原因付き Error は `cause_name()` が原因の `Debug` を返す |

`mustFormatMessage` の原因付き 2 引数は、原典の `assertSame(cause, causedEvent.cause())` を `Arc::ptr_eq` で受信原因と送信原因を直接比較して維持する。原因の文字列比較や型名比較では代用しない。

## Java 固有表現の等価根拠

| 項目 | 原典（Java） | Rust の扱い | 等価根拠 |
|:---|:---|:---|:---|
| 購読の型 | `system.getEventStream().subscribe(getRef(), LogEvent.class)` で共通親 `LogEvent` を購読 | 期待する各イベントの具象クラス（`Error`/`Warning`/`Info`/`Debug`）を購読し、`mustCallMdcForEveryLog` は同じ probe のキューから期待レベルの具象型で `expect_msg_type` を順に読む | 原典の各ケースは 1 メッセージにつき 1 レベルだけを発行する。Rust の `LogEvent` trait の実装はイベントストリームのクラス階層に登録されないため、4つの具象クラスを直接購読する。受信対象と順序は原典と一致する |
| MDC の比較 | `log.getMDC().toString()` で `{messageLength=8}` や `{}` という文字列に変換して比較 | MDC を `BTreeMap<String, String>` のまま比較（空は `Mdc::new()`） | 原典が比較しているのは Map の内容である。port は Map を直接比較するので、キー `messageLength` の有無と値が一致することを確認できる。文字列表現の差（`{}` や `=`）は表現上の差にすぎない |
| `messageLength` の型 | `Int`（`log.message.length()`） | `String`（`"8"` などの ASCII 数字） | port の `Mdc` は値が `String` である。`messageLength` の数値 `8`・`9`・`9`・`29` を同じ値の文字列表現として比較する。`length()` は UTF-16 コード単位、port は `chars().count()` を使うが、原典の 5 メッセージはすべて ASCII なので結果は一致する |
| `new Date()` / `UUID.randomUUID()` | 実行時の値と `toString()` | `Display` が固定日時文字列 `Sun Feb 10 16:02:20 UTC 2019` と固定 UUID 文字列 `4192af46-487c-4132-84e8-87890cbb00a6` を返す型 | 原典が読むのは各値の `toString()` だけである。port は同じ種類の文字列出力を持つ別型（`JavaDate`・`JavaUuid`・`&str`）を渡し、配列が異種型のまま（全要素を文字列化しない）であることを保つ。時計と乱数は使わない |
| 原因の保持 | `Throwable` オブジェクトを保持し `assertSame` で同一性比較 | 原因を `Arc<dyn Any + Send + Sync>` として保持し `Arc::ptr_eq` で同一性比較 | 送信したオブジェクトそのものを受信イベントが参照しているかを直接比較する。文字列（`cause_name`）や型だけの比較では代替しない |
| ログクラス | `LoggingAdapterTest.class` を `LogClass` として渡す | ログソース名 `org.apache.pekko.event.LoggingAdapterTest` を渡し、ログクラスは原典が読まないため比較しない | 原典の 4 ケースはいずれも `logClass` を読まない。port は原典で読まれる事実だけを比較する |
| アクターからのロガー取得 | `Logging.getLogger(this)` でアクターのコンテキスト経由でロガーを取得 | 原典どおり `system.actorOf(Props.create(ActorWithMDC.class))` に対応する `Props::new` でアクターを起動する。`ActorContext` はシステムのイベントストリームを公開しないため、ケースが `Send + Sync` な `LogPublisher` を作り、`Props` factory に `adapter.clone()` を捕捉し、`Actor::receive` が `&mut adapter` を保持する | アクターは原典と同じく 1 個のロガーを保持し、message ごとに MDC 設定→全レベル発行→クリアを行う。`LogPublisher` を `Send + Sync` にするのは製品側の責務（第2工程で `EventStream` の公開設計を広げず、`EventStreamBusWeak` の `Send + Sync` な発行ハンドルを内部利用して対応する） |

## 原典との意図的な差異

| 項目 | 原典 | Rust の扱い |
|:---|:---|:---|
| 購読型の広さ | 共通親 `LogEvent` | 具象クラスを購読（上表のとおり配送結果は一致） |
| MDC と `messageLength` の型 | `Map` と `Int`、`toString` 比較 | `BTreeMap<String, String>` を直接比較（上表のとおり内容は一致） |
| 日時・UUID | 実行時生成 | 固定値の `Display`。異種型配列を保つ |
| アクターの起動経路 | `system.actorOf(Props.create(ActorWithMDC.class))` | `probe.child_actor_of(Props::new(move || ActorWithMdc::new(adapter.clone())))` で起動する。`Props::new` の factory は `Send + Sync` を要求し `EventStream` 自体は `!Send`（`Rc` を含む）なので、factory は `EventStream` ではなく `Send + Sync` な `LogPublisher` の clone を捕捉する。アクターは probe の子として起動され、イベントは probe のキューへ届く |
| 旧コンストラクターの原因型 | `new Error(new Exception(), ...)` | `Error::with_cause` の呼び出しに `Debug` のみを実装した `PlainException`（クラス名 `Exception`）を渡す。既存境界は `C: fmt::Debug` であり `Display` を要求しないため、原典の素の `Exception` に合わせて `Debug` だけの型にする（`red` を既存 API の誤用にしないため） |
| ログ発行の入口 | `error(message)`／`error(template, arg)` 等の arity オーバーロード | 単一入口 `log(level, message, args)` に統一する。Rust に同名 arity オーバーロードはないため、レベルを第1引数、テンプレートを第2引数、引数を `&[impl Display]` の第3引数にする。`impl Display` なので、`int[]`（Rust `[i32; 3]`）は `let primitive_args = [10_i32, 20, 30];` を `&primitive_args` のまま渡せ、異種型は `[&dyn Display; 3]` を直接渡せる。引数なしは `&[] as &[&str]` で型を明示する。原因付きは Error 専用の `error(cause, message, args)` で、他のレベルへ原因を持たせない。 |
| `No MDC` と `Null MDC` の扱い | `Collections.emptyMap()`（明示的な空）と `null`（未指定）を別々に `setMDC`。`clearMDC()` は空 Map を設定する | ロガー入口 `set_mdc` を `Option<&Mdc>` とし、`No MDC` では `Some(&empty_mdc())`（明示的な空）、`Null MDC` では `None`（未指定）を 2 つの異なる入力として渡してテストで区別する。内部はどちらも空の `Mdc` に正規化し、両者とも受信イベントの MDC は空になることを観測する。`clear_mdc` は原典 `clearMDC()` と同じく空 Map を設定する（null ではなく空へのリセット）。 |

## 第 1 工程の初回 red の採証

製品を変更する前に、指定コマンドを作業ツリー（HEAD `fac859141be2fa970cbc0beaf3fdf649c4db0e98`）で実行した。

| コマンド | 終了値 | 実行/失敗件数 | 診断 |
|:---|---:|:---|:---|
| `cargo test -p fraktor-actor --test logging_adapter_test` | 101 | 実行 0、失敗 0（コンパイル失敗） | `E0432: unresolved import fraktor_actor::kernel::actor::Logging::LoggingAdapter`（未実装入口）。`E0599: no method named cause found for struct fraktor_actor::kernel::actor::Logging::Error`（受信原因参照の取得入口なし）。ほかに警告・エラーなし |

終了値 101 はコンパイル失敗によるもので、4 ケースが実行時に失敗したことを意味しない。製品 API の不足は 2 種類（`LoggingAdapter` 入口と `Error::cause`）で、いずれも未実装である。意図的な red であり、この第 1 タスクの成功成果である。`already_green` ではない。

### 製品未変更の根拠

- `git status --short` でこの作業ツリーの未追跡・変更は `crates/fraktor-actor/tests/logging_adapter_test.rs` と本ゴール文書のみ。製品クレート、依存定義、既存テストは未変更。
- 診断コード `E0432`・`E0599` は「製品に入口が存在しない」ことを示し、テスト側の型・パス誤りではない。テスト本体の構文・型エラーは上記 2 件以外にない。
- 第1工程の統合 SHA `50859f1c4d0817858f46a4f355ce1dfc79038f38` を Orca 子ワークスペースで再実行し、同じ2診断と終了値101を確認した。対象テスト以外の製品差分がないことを Git で照合した。生出力は正規チェックアウトの `.takt/issue-110-evidence/stage1-orca-red.log` に保存した。
- 作業 SHA: `fac859141be2fa970cbc0beaf3fdf649c4db0e98`。

## 第 2 工程で実装した最小契約

| 入口 | 変更前の不足 | 第 2 工程で実装した最小契約 |
|:---|:---|:---|
| `Logging::LogPublisher`（構成: イベントストリーム・ログソース・`LogClass`。`Props` factory から捕捉できるよう `Send + Sync`） | `E0432`（`LoggingAdapter` 未定義） | 単一入口 `log(level, message, args: &[impl Display])` と Error 専用の `error(cause: Arc<C>, message, args)`、`set_mdc(Option<&Mdc>)`（`Some` は明示的な空を含む設定、`None` は未指定。内部はどちらも空 `Mdc` に正規化）/`clear_mdc`（空 Map へリセット）。`EventStream`（`!Send`）をそのまま保持せず、`EventStreamBusWeak`（`Send + Sync`）を内部に使うことで、`Props` factory の closure と捕捉値として `Send + Sync` を満たす |
| `Error::cause()`（受信イベントから原因参照を取り出す） | `E0599`（`cause` 未定義） | 原因を `Arc<dyn Any + Send + Sync>` として `Error` に保持し、`Arc::ptr_eq` で送信側と比較できる参照を返す |

`LogLevel`・`LogEvent`・`Mdc`・`empty_mdc`・既存コンストラクター（`Error::new`、`Error::with_cause`、`Error::with_cause_name`、`Warning::new`、`Info::new`、`Debug::new`）は既存のまま維持した。4 イベントに crate 内限定の `with_mdc` を追加し、`Error` に `with_shared_cause` と `cause()` を追加した。既存 `LogMarkerAdapter`・`EventFilter` の原因名前解決・marker・旧コンストラクターの挙動は変更していない。

### 実装箇所

| 対象 | ファイル | 内容 |
|:---|:---|:---|
| `LogPublisher` と書式化 | `crates/fraktor-actor/src/kernel/actor/Logging/log_publisher.rs` | `new`/`log`/`error`/`set_mdc`/`clear_mdc`、`format_template`（不足 `{}` 保持、余剰 ` WARNING arguments left: N`） |
| 4 イベントの MDC | `Logging/{error,warning,info,debug}.rs` | `with_mdc`（crate 内限定） |
| 共有原因 | `Logging/error.rs` | `shared_cause` フィールド、`with_shared_cause`、`cause()` |
| 弱参照公開 | `crates/fraktor-actor/src/kernel/event.rs` | `pub(crate) use event_stream_bus::EventStreamBusWeak;` |
| 公開 | `Logging.rs` | `pub use log_publisher::LogPublisher;` |

原典は DEBUG 設定で4レベルのログを観測する。Rust では `LogPublisher` が発行する4クラスをテストで直接購読する。`EventStream::set_log_level` は既定ロガーの購読を調整し、個別の購読先は購読したレベルのイベントを受け取る。

## 第 2 工程の実測結果

第 2 工程のテストとゴールを確定させた後に、指定コマンドを実行した。

| コマンド | 終了値 | 実行/失敗件数 | 摘要 |
|:---|---:|:---|:---|
| `cargo test -p fraktor-actor --test logging_adapter_test` | 0 | 4 実行 / 0 失敗 | 原典 4 ケース全通過（`must_format_message`、`must_call_mdc_for_every_log`、`must_support_mdc_null`、`must_be_able_to_create_log_events_with_old_constructor`） |
| `cargo test -p fraktor-actor --test log_publisher_format_test` | 0 | 3 実行 / 0 失敗 | 書式化の不足・余剰・空引数 3 ケース通過 |
| `cargo test -p fraktor-actor` | 0 | 全テストターゲット通過（失敗 0） | `must_format_message` 等を含む |
| `cargo test -p fraktor-testkit` | 0 | 全テストターゲット通過（失敗 0） | |
| `cargo fmt --check` | 0 | 差分なし | |
| `cargo clippy --workspace --all-targets -- -D warnings` | 0 | 警告・エラーなし | |
| `cargo run --manifest-path tools/syn-check/Cargo.toml --release --locked -- crates/fraktor-actor` | 0 | `violations: 0`（117 ファイル） | 構造規約（type-per-file・module-wiring 等）に適合 |
| `uv run --with pyyaml python3 ledger/tools/check_needs.py --ledger ledger/nodes.yaml --capabilities ledger/capabilities.yaml --goal G0issue110` | 0 | `needs 閉性 OK`（ノード 1 件） | |
| `uv run --with pyyaml python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal G0issue110` | 0 | `status 内訳 {'green': 1}`、完了 | |
| `uv run --with pyyaml python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal G0a`〜`G0f` | 0 | 各ゴール `status 内訳` は全て green、完了 | 既存ゴールの status は変更していない |

### 第 2 工程の入力・入口名の変更理由

- 第 1 工程のテストは未実装名 `LoggingAdapter`、`log_with_cause(level, cause, message, args)` を使っていた。要件が「製品名に原典の `LoggingAdapter` をそのまま移さない」「原因の付く経路は Error 専用とし、他レベルへ原因の扱いを増やさない」と定めているため、製品名を `LogPublisher`、原因付き入口を Error 専用の `error(cause, message, args)` に確定し、テストを同じ変更で移行した。
- 空引数の呼び出しは `&[] as &[&str]` で要素型を明示し、スライスを実際に空として渡す。`log`/`error`/`format_template` の引数は `&[impl Display]` とし、`[10_i32, 20, 30]` を `&primitive_args` のまま渡せ、異種型は `[&dyn Display; 3]` を直接渡せる。
- `set_mdc` の公開入力は `Some(empty)`（明示的な空）と `None`（未指定）を区別してテストし、内部はどちらも空 `Mdc` に正規化する。`clear_mdc` は原典 `clearMDC()` に合わせて空 Map へリセットする（null リセットではない）。
- `Send + Sync` を要求するのは `Props` の factory closure とそこへ捕捉する値であり、`Actor` 自体への一般的な制約ではない。
- 追加テスト `log_publisher_format_test.rs` は、原典 4 ケースが踏まない書式化 arity 分岐（不足・余剰・空）を、同じ公開発行入口と EventStream/TestProbe 経由で観測する。原典のケース数（4/4/4）には加算しない。

### 反例（拒否した誤実装）の観測

- 原因の代用: `must_format_message` が受信 Error の `cause()` を `Arc<IllegalStateException>` に downcast し `Arc::ptr_eq` を真で確認。文字列・型名だけの代用や二重 `Arc` 包みは本 assertion で失敗する（実装途中で二重 `Arc` 化により実際に失敗し、`Arc<C>` をそのまま保持する形へ修正して通過した）。
- MDC の前回値混入: `must_call_mdc_for_every_log` が同一アクターの連続 5 発行で各イベントの MDC（`8`・`9`・`9`・空・`29`）を直接比較。clear 後の空、直後の復帰を区別。
- 書式化の余剰・不足: `log_publisher_format_test` が `Only 1 WARNING arguments left: 2`、`First one, second {}`、`Value {}` を完全一致で確認。

### 再現コマンド

```sh
cargo test -p fraktor-actor --test logging_adapter_test
cargo test -p fraktor-actor --test log_publisher_format_test
cargo test -p fraktor-actor
cargo test -p fraktor-testkit
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo run --manifest-path tools/syn-check/Cargo.toml --release --locked -- crates/fraktor-actor
uv run --with pyyaml python3 ledger/tools/check_needs.py --ledger ledger/nodes.yaml --capabilities ledger/capabilities.yaml --goal G0issue110
uv run --with pyyaml python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal G0issue110
```

## 台帳更新

`ledger/nodes.yaml` の `actor-tests/src/test/java/org/apache/pekko/event/LoggingAdapterTest.java` のみを `goal: G0issue110`、`status: green`、`cases 4/4/4`、`excluded: []`、`rust.crate: fraktor-actor`、`rust.test_file: crates/fraktor-actor/tests/logging_adapter_test.rs` に更新した。既存の `needs`（`testkit: ['test-kit', 'expect-msg']`、`premise_flags: []`）と `depends_on_impl` は維持し、対象外ノード・G0a〜G0f は変更していない。

## 第 1 工程の初回 red の記録（歴史的記録）

第 1 工程の記録は上記「第 1 工程の初回 red の採証」に残す。その時点のテストは未実装名 `LoggingAdapter` と `log_with_cause` を使っていたが、第 2 工程で製品名と入口を上記のとおり確定し、テストを同じ変更で移行した。

## EventStream 借用への修正と最終検証

`CODE-NEW-log-publisher-L53` に対応し、`LogPublisher::new` の引数を `&EventStream` に変更した。既存の対象テスト3箇所と補助テスト3箇所は、購読に使う `stream` を借用する。内部は従来どおり `EventStreamBusWeak` のみを保持し、所有者を消費せず、寿命も延長しない。配送は購読テーブルへの強参照が残っている間続く。購読解除用のアクターもテーブルを保持できるため、最後の `EventStream` ハンドルを破棄した時点と配送停止の時点は一致するとは限らない。

追加した補助ケース `independent_stream_without_unsubscriber_stops_delivering_after_drop` は、システム既定ストリームとは独立した `EventStream::new(spec.system().downgrade(), false)` を唯一の所有者として保持する。DebugとErrorを購読し、同じ発行型から `owned {}` に `[7_i32]` を渡す。受信したDebug/Errorの本文 `owned 7`・レベル・空MDC、Errorの共有原因 `Arc<String>` の同一性を確認する。キューを読み切ってから `drop(stream)` だけを行い、同じ発行型・入力・原因・probeで再発行し、`run_until_idle` 後の `expect_no_message(Duration::ZERO)` で無配送を確認した。追加clone・固定sleep・モックは使っていない。原典4ケースと台帳の4/4/4には加算しない。

以下は借用化と全利用側移行後に新規実行した結果であり、上記の修正前の成功記録とは別の証拠である。生出力の保存先はリポジトリ内の `.takt/issue-110-evidence/fix-borrow/`。表のログ名はこのディレクトリからの相対パスで、対応する `.exit` に終了コード、`rust-results.json` / `ledger-results.json` に引数列を保存した。台帳コマンドには作業ツリー内の `UV_CACHE_DIR=target/issue-110-uv-cache` を設定した。

| 再現コマンド | 終了コード | 結果 | 生出力 |
|:---|---:|:---|:---|
| `cargo test -p fraktor-actor --test logging_adapter_test` | 0 | 4 passed / 0 failed / 0 ignored | `logging-adapter.log` |
| `cargo test -p fraktor-actor --test log_publisher_format_test` | 0 | 4 passed / 0 failed / 0 ignored（既存3件＋所有権1件） | `log-publisher-format.log` |
| `cargo test -p fraktor-actor` | 0 | 34結果、計278 passed / 0 failed / 0 ignored | `actor.log` |
| `cargo test -p fraktor-testkit` | 0 | 7結果、計61 passed / 0 failed / 0 ignored | `testkit.log` |
| `cargo fmt --check` | 0 | 整形差分なし。stableでは利用できない既存rustfmt設定の警告あり | `fmt.log` |
| `cargo clippy --workspace --all-targets -- -D warnings` | 0 | 型検査成功、警告・エラーなし | `clippy.log` |
| `cargo run --manifest-path tools/syn-check/Cargo.toml --release --locked -- crates/fraktor-actor` | 0 | 117ファイル、violations 0、parse_failures 0、suppressions 0 | `syn-check.log` |
| `uv run --with pyyaml python3 ledger/tools/check_needs.py --ledger ledger/nodes.yaml --capabilities ledger/capabilities.yaml --goal G0issue110` | 0 | 1ノード、needs閉性OK | `check-needs.log` |
| `uv run --with pyyaml python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal G0issue110` | 0 | 1ノード、green 1、完了 | `rollup-G0issue110.log` |
| `uv run --with pyyaml python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal G0a` | 0 | 2ノード、green 2、完了 | `rollup-G0a.log` |
| `uv run --with pyyaml python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal G0b` | 0 | 4ノード、green 4、完了 | `rollup-G0b.log` |
| `uv run --with pyyaml python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal G0c` | 0 | 2ノード、green 2、完了 | `rollup-G0c.log` |
| `uv run --with pyyaml python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal G0d` | 0 | 2ノード、green 2、完了 | `rollup-G0d.log` |
| `uv run --with pyyaml python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal G0e` | 0 | 2ノード、green 2、完了 | `rollup-G0e.log` |
| `uv run --with pyyaml python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal G0f` | 0 | 2ノード、green 2、完了 | `rollup-G0f.log` |

セルフスキャン対象は `Logging::log_publisher` と移行した2統合テスト。全定義・全7呼び出しを検索し、値受け取りの互換入口・未移行呼び出し・未使用importの残存はない。追加importはテスト側の標準ライブラリと公開kernel APIのみで、製品の依存方向は変更していない。対応するmodule mock / doubleはなく、既存 `ActorWithMdc` fixtureを同じ借用入口へ移行して上記Cargoの対象テストで確認した。npmにはRust統合テストの実行・分類スクリプトがなく、Cargoの `tests/` 自動検出と上記対象・クレート回帰で配線を確認した。今回の修正では台帳を編集せず、対象ノードのgreen・4/4/4・除外0・既存needsを保持した。

## PR レビューで追加した MDC クリアの回帰テスト

`clearing_diagnostic_context_removes_it_from_later_events` は同じロガーで MDC を設定して発行し、クリア後は再設定せずに発行する。最初の受信イベントが設定値を保持し、次の受信イベントが空になることを確認する。原典の4ケースに加え、`clear_mdc` 自体の作用を観測する補助テストである。隔離ワークスペースで `clear_mdc` を空処理にした場合にこのテストが失敗することを確認し、製品コードを復元した。

第1工程の無加工ログ `stage1-orca-red.log` / `.exit` と `stage1-integration.json` はこの作業ツリーにはなく、原本の照合は未確認としてsuperviseへ引き継ぐ。managerによる成果SHAの受け入れ確認は完了イベント後に行われるもので、本節の検証成功に含めない。

## Pekko v2.0.0 との照合（2026-11-25 追記）

基準を Pekko v2.0.0（`9fbd2283b1cf701c90a1223bf621942c191db61f`）に切り替えた後に照合した。v1.7.1 は v2.0.0 の祖先ではないので、共通祖先 `0ca804c7af` から両方の系統を比べた。上の本文は v1.7.1 で照合したときの記録なので、そのまま残す。ここでは v2.0.0 の位置と判定だけを書く。台帳の `verified_against` は v2.0.0 にした（第3まとまり、`docs/baseline/pekko-v2.0.0-migration/batch-3.md`。第1・第2まとまりで扱ったノードは、それぞれの記録にある）。

- テスト `LoggingAdapterTest.java` の変更は、JUnit 5 化（`ec27d6dd8c` #2724）、`assertInstanceOf`（`4d942a0ca7` #3088）、javafmt（`00b0a99278` #2081）だけである。ケースと入力は同じ（4 件）。
- `ActorWithMDC.java` の変更は switch 構文だけで、分岐と呼ぶ logger のメソッドは同じである。
- 記録済みの差異 7 件（購読する型の広さ、MDC と `messageLength` の型、日時と UUID の固定値、アクターの起動経路、旧コンストラクタの原因型、ログの入口を 1 つにまとめること、`No MDC` と `Null MDC`）は、すべて維持する。`LoggingAdapter.format1` の primitive 配列の分岐は書き方が変わったが、結果は同じである。
