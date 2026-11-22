# ゴール G0issue110：LoggingAdapterTest

[Issue #110](https://github.com/j5ik2o/fraktor-ng-rs/issues/110) の対象は Pekko の `LoggingAdapterTest`（Java）。原典は `LoggingAdapterTest.java` の 4 ケースと、その 2 ケースが使う `ActorWithMDC.java` の 1 アクターである。原典は、ロガーへテンプレートと未展開の引数を渡す書式化が購読イベントの本文・レベル・MDC に現れること、原因付きログが送信した原因オブジェクトそのものを届けること、MDC をメッセージごとに設定・全レベル発行・クリアするアクターの連続した発行が購読先で観測できること、未指定 MDC が空になること、旧コンストラクターが引き続きイベントを生成できることを確かめる。

- 原典（`LoggingAdapterTest.java`）: https://github.com/apache/pekko/blob/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/actor-tests/src/test/java/org/apache/pekko/event/LoggingAdapterTest.java
- 原典（`ActorWithMDC.java`）: https://github.com/apache/pekko/blob/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/actor-tests/src/test/java/org/apache/pekko/event/ActorWithMDC.java

## 対象ノード

| id | Rust の統合テスト | 原典のケース数 |
|:---|:---|---:|
| `actor-tests/src/test/java/org/apache/pekko/event/LoggingAdapterTest.java` | `crates/fraktor-actor/tests/logging_adapter_test.rs` | 4 |

原典のケース名は `mustFormatMessage`、`mustCallMdcForEveryLog`、`mustSupportMdcNull`、`mustBeAbleToCreateLogEventsWithOldConstructor`。除外は 0 件。`ActorWithMDC.java` はケースではなく、`mustCallMdcForEveryLog` と `mustSupportMdcNull` が使うアクターであり、独立したケースとしては数えない。この第 1 工程はテストの保存と初回 red の採証までで、台帳（`ledger/`）の登録・ステータス更新は行わない。

## write_scope

- 対象の統合テスト `crates/fraktor-actor/tests/logging_adapter_test.rs`。
- このゴール文書 `docs/goals/issue-110-logging-adapter.md`。

製品コード、既存テスト、依存定義、台帳、PR、`main`、ゴール登録状態は変更しない。

## 原典とテストの個別照合

Pekko commit `9a54d1294c3ac7821bbed3c940769c3d5027d1ff` の `LoggingAdapterTest.java` と `ActorWithMDC.java` の全文を取得し、作成した Rust テストと照合した。

| 原典のケース | Rust のケース | 駆動入口 | 購読・受信 | 直接観測する契約 |
|:---|:---|:---|:---|:---|
| `mustFormatMessage` | `must_format_message` | `LoggingAdapter` の単一入口 `log(level, message, args)` と原因付き `log_with_cause(level, cause, message, args)` | 同じ `ActorSystem` の `event_stream` に `TestProbe` を `Error`/`Warning`/`Info`/`Debug` で購読し、`probe.expect_msg_type::<E>` で受信 | 1 引数 Error 本文 `One arg message: the arg`・空 MDC／原因付き 2 引数 Error 本文 `Two args message: an arg, another arg`・`Arc::ptr_eq` で送信原因と受信原因の同一性／数値配列 Warning 本文 `Args as array of primitives: 10, 20, 30`／異種配列 Info 本文 `Args as array of objects: A String, Sun Feb 10 16:02:20 UTC 2019, 4192af46-487c-4132-84e8-87890cbb00a6`／4 引数 Debug 本文 `Four args message: 1, 2, 3, 4`。各イベントのレベルと空 MDC を比較 |
| `mustCallMdcForEveryLog` | `must_call_mdc_for_every_log` | `ActorWithMDC` 相当の `ActorWithMdc` を `Props::new` で 1 度だけ起動し、5 メッセージを順に送る（`Props` factory は `adapter.clone()` を捕捉し、`Actor::receive` が同じ adapter を保持する） | 同じ `ActorSystem` の `event_stream` に `TestProbe` を購読し、同じ probe のキューから期待レベルの具象型（`Error`/`Warning`/`Info`/`Debug`/`Debug`）で `expect_msg_type` を順に読む（`Debug` 一型で読まない／別キューで読まない） | 5 回の発行の本文・レベル・MDC 全体を比較。MDC は `messageLength` が `8`・`9`・`9`（空）・`29`。4 回目（`No MDC for 4th call`）が空、直後の 5 回目（`And now yes, a debug with MDC`）が `29` へ復帰することを同じアクターの連続処理として観測 |
| `mustSupportMdcNull` | `must_support_mdc_null` | `Props::new` で起動した `ActorWithMdc` へ `Null MDC` を 1 通送る（アダプターへ MDC 未指定＝`None` を渡す） | 同じ `ActorSystem` の `event_stream` に `TestProbe` を `Info` で購読し受信 | 未指定（null 相当）で実発行された Info の本文 `Null MDC`・レベル Info・空 MDC |
| `mustBeAbleToCreateLogEventsWithOldConstructor` | `must_be_able_to_create_log_events_with_old_constructor` | 既存コンストラクター `Error::with_cause`、`Error::new`、`Warning::new`、`Info::new`、`Debug::new` | 購読なし。生成したイベントを直接読む | 原典の 5 つの `new` に対応する 5 経路がイベントを生成し、本文 `The message` と各レベル、空 MDC を持つ。原因付き Error は `cause_name()` が原因の `Debug` を返す |

`mustFormatMessage` の原因付き 2 引数は、原典の `assertSame(cause, causedEvent.cause())` を `Arc::ptr_eq` で受信原因と送信原因を直接比較して維持する。原因の文字列比較や型名比較では代用しない。

## Java 固有表現の等価根拠

| 項目 | 原典（Java） | Rust の扱い | 等価根拠 |
|:---|:---|:---|:---|
| 購読の型 | `system.getEventStream().subscribe(getRef(), LogEvent.class)` で共通親 `LogEvent` を購読 | 期待する各イベントの具象クラス（`Error`/`Warning`/`Info`/`Debug`）を購読し、`mustCallMdcForEveryLog` は同じ probe のキューから期待レベルの具象型で `expect_msg_type` を順に読む | 原典の各ケースは 1 メッセージにつき 1 レベルだけを発行する。この port のストリームは購読型の完全一致で配送し親へ広げないため、具象クラス購読で同じイベントを同じ順序で受信する。受信対象と順序は原典と一致し、差異は観測できない |
| MDC の比較 | `log.getMDC().toString()` で `{messageLength=8}` や `{}` という文字列に変換して比較 | MDC を `BTreeMap<String, String>` のまま比較（空は `Mdc::new()`） | 原典が比較しているのは Map の内容である。port は Map を直接比較するので、キー `messageLength` の有無と値が一致することを確認できる。文字列表現の差（`{}` や `=`）は表現上の差にすぎない |
| `messageLength` の型 | `Int`（`log.message.length()`） | `String`（`"8"` などの ASCII 数字） | port の `Mdc` は値が `String` である。`messageLength` の数値 `8`・`9`・`9`・`29` を同じ値の文字列表現として比較する。`length()` は UTF-16 コード単位、port は `chars().count()` を使うが、原典の 5 メッセージはすべて ASCII なので結果は一致する |
| `new Date()` / `UUID.randomUUID()` | 実行時の値と `toString()` | `Display` が固定日時文字列 `Sun Feb 10 16:02:20 UTC 2019` と固定 UUID 文字列 `4192af46-487c-4132-84e8-87890cbb00a6` を返す型 | 原典が読むのは各値の `toString()` だけである。port は同じ種類の文字列出力を持つ別型（`JavaDate`・`JavaUuid`・`&str`）を渡し、配列が異種型のまま（全要素を文字列化しない）であることを保つ。時計と乱数は使わない |
| 原因の保持 | `Throwable` オブジェクトを保持し `assertSame` で同一性比較 | 原因を `Arc<dyn Any + Send + Sync>` として保持し `Arc::ptr_eq` で同一性比較 | 送信したオブジェクトそのものを受信イベントが参照しているかを直接比較する。文字列（`cause_name`）や型だけの比較では代替しない |
| ログクラス | `LoggingAdapterTest.class` を `LogClass` として渡す | ログソース名 `org.apache.pekko.event.LoggingAdapterTest` を渡し、ログクラスは原典が読まないため比較しない | 原典の 4 ケースはいずれも `logClass` を読まない。port は原典で読まれる事実だけを比較する |
| アクターからのロガー取得 | `Logging.getLogger(this)` でアクターのコンテキスト経由でロガーを取得 | 原典どおり `system.actorOf(Props.create(ActorWithMDC.class))` に対応する `Props::new` でアクターを起動する。`ActorContext` はシステムのイベントストリームを公開しないため、ケースが `Send + Sync` な `LoggingAdapter` を作り、`Props` factory に `adapter.clone()` を捕捉し、`Actor::receive` が `&mut adapter` を保持する | アクターは原典と同じく 1 個のアダプターを保持し、message ごとに MDC 設定→全レベル発行→クリアを行う。adapter を `Send + Sync` にするのは製品側の責務（第2工程で `EventStream` の公開設計を広げずに Send+Sync な発行ハンドル内部利用で対応する） |

## 原典との意図的な差異

| 項目 | 原典 | Rust の扱い |
|:---|:---|:---|
| 購読型の広さ | 共通親 `LogEvent` | 具象クラスを購読（上表のとおり配送結果は一致） |
| MDC と `messageLength` の型 | `Map` と `Int`、`toString` 比較 | `BTreeMap<String, String>` を直接比較（上表のとおり内容は一致） |
| 日時・UUID | 実行時生成 | 固定値の `Display`。異種型配列を保つ |
| アクターの起動経路 | `system.actorOf(Props.create(ActorWithMDC.class))` | `probe.child_actor_of(Props::new(move || ActorWithMdc::new(adapter.clone())))` で起動する。`Props::new` の factory は `Send + Sync` を要求し `EventStream` 自体は `!Send`（`Rc` を含む）なので、factory は `EventStream` ではなく `Send + Sync` な `LoggingAdapter` の clone を捕捉する。アクターは probe の子として起動され、イベントは probe のキューへ届く |
| 旧コンストラクターの原因型 | `new Error(new Exception(), ...)` | `Error::with_cause` の呼び出しに `Debug` のみを実装した `PlainException`（クラス名 `Exception`）を渡す。既存境界は `C: fmt::Debug` であり `Display` を要求しないため、原典の素の `Exception` に合わせて `Debug` だけの型にする（`red` を既存 API の誤用にしないため） |
| ログ発行の入口 | `error(message)`／`error(template, arg)` 等の arity オーバーロード | 単一入口 `log(level, message, args)` に統一する。Rust に同名 arity オーバーロードはないため、レベルを第1引数、テンプレートを第2引数、引数を `&[&dyn Display]` の第3引数にし、空引数は `&[]` で渡す。原因付きは `log_with_cause(level, cause, message, args)`。`int[]` は `[&dyn Display; 3]` の借用整数、`Object[]` は異種型の `[&dyn Display; 3]` として区別を保つ |
| `No MDC` と `Null MDC` の扱い | `Collections.emptyMap()`（明示的な空）と `null`（未指定）を別々に `setMDC` | アダプター入口 `set_mdc` を `Option<&Mdc>` とし、`No MDC` では `Some(&empty_mdc())`（明示的な空）、`Null MDC` では `None`（未指定）を渡して 2 つの入力を区別する。両方とも受信イベントの MDC は空になることを観測する。原典が観測する「空 MDC の発行」は一致しつつ、入力の区別も保つ |

## 第 1 工程の初回 red の採証

製品を変更する前に、指定コマンドを作業ツリー（HEAD `fac859141be2fa970cbc0beaf3fdf649c4db0e98`）で実行した。

| コマンド | 終了値 | 実行/失敗件数 | 診断 |
|:---|---:|:---|:---|
| `cargo test -p fraktor-actor --test logging_adapter_test` | 101 | 実行 0、失敗 0（コンパイル失敗） | `E0432: unresolved import fraktor_actor::kernel::actor::Logging::LoggingAdapter`（未実装入口）。`E0599: no method named cause found for struct fraktor_actor::kernel::actor::Logging::Error`（受信原因参照の取得入口なし）。ほかに警告・エラーなし |

終了値 101 はコンパイル失敗によるもので、4 ケースが実行時に失敗したことを意味しない。製品 API の不足は 2 種類（`LoggingAdapter` 入口と `Error::cause`）で、いずれも未実装である。意図的な red であり、この第 1 タスクの成功成果である。`already_green` ではない。

### 製品未変更の根拠

- `git status --short` でこの作業ツリーの未追跡・変更は `crates/fraktor-actor/tests/logging_adapter_test.rs` と本ゴール文書のみ。製品クレート、依存定義、既存テストは未変更。
- 診断コード `E0432`・`E0599` は「製品に入口が存在しない」ことを示し、テスト側の型・パス誤りではない。テスト本体の構文・型エラーは上記 2 件以外にない。
- レビュー修正後の red を再採証した。さらに、未実装の `LoggingAdapter` と `Error::cause` を一時的な stub（`unsafe impl Send/Sync` 付き）に置き換えた使い捨てコピーをビルドし、`Props::new` factory・`Actor::receive`・`probe.child_actor_of`・`expect_msg_type`・`Error::with_cause` など既存 API の使い方に型・境界の誤りがないこと（stub 下でコンパイル成功・エラー 0）を確認した。stub は確認後に元ファイルへ戻し、リポジトリに残していない。したがって red は未実装入口のみが原因である。
- 作業 SHA: `fac859141be2fa970cbc0beaf3fdf649c4db0e98`。

## 第 2 工程へ引き継ぐ未実装入口

| 未実装入口 | 診断 | 第 2 工程で必要な最小契約 |
|:---|:---|:---|
| `Logging::LoggingAdapter`（構成: イベントストリーム・ログソース。`Send + Sync` であること） | `E0432` | 単一入口 `log(level, message, args: &[&dyn Display])` と原因付き `log_with_cause(level, cause, message, args)`、`set_mdc(Option<&Mdc>)`（`Some` は明示的な空を含む設定、`None` は未指定）/`clear_mdc`。`None` と `Some(空)` のどちらも空 MDC のイベントを発行し、入力を区別して受け取る。`EventStream`（`!Send`）をそのまま保持せず、`EventStreamBus` の `Send + Sync` 発行ハンドル等を内部に使うことで `Send + Sync` を満たす |
| `Error::cause()`（受信イベントから原因参照を取り出す） | `E0599` | 原因を `Arc<dyn Any + Send + Sync>` として保持し、`Arc::ptr_eq` で送信側と比較できる参照を返す |

`LogLevel`・`LogEvent`・`Mdc`・`empty_mdc`・既存コンストラクター（`Error::new`、`Error::with_cause`、`Warning::new`、`Info::new`、`Debug::new`）は既に存在し、テストはこれらをそのまま使う。
