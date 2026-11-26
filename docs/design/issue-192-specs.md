# 設計：#192 ProviderSelectionSpec／ActorWithBoundedStashSpec／FunctionRefSpec

[Issue #192](https://github.com/j5ik2o/fraktor-ng-rs/issues/192) の設計の記録である。Pekko v2.0.0 の actor-tests にある actor の `ProviderSelectionSpec`、`ActorWithBoundedStashSpec`、`FunctionRefSpec` を Rust へ移植し、台帳の対象ノード3件を green にする。本書は第1段（設計）であり、実装は設計レビューが通ってから第2段で行う。本書の段では台帳を触らない。

原典（Pekko v2.0.0）:

- [ProviderSelectionSpec.scala](https://github.com/apache/pekko/blob/v2.0.0/actor-tests/src/test/scala/org/apache/pekko/actor/ProviderSelectionSpec.scala)／本体は `ActorSystem.scala` 内の `ProviderSelection`（`Settings` が `pekko.actor.provider` の値を読んで決める）
- [ActorWithBoundedStashSpec.scala](https://github.com/apache/pekko/blob/v2.0.0/actor-tests/src/test/scala/org/apache/pekko/actor/ActorWithBoundedStashSpec.scala)／本体は `Stash`／`StashSupport`、`BoundedDequeBasedMailbox`、dispatcher／mailbox の設定解決
- [FunctionRefSpec.scala](https://github.com/apache/pekko/blob/v2.0.0/actor-tests/src/test/scala/org/apache/pekko/actor/FunctionRefSpec.scala)／本体は `FunctionRef` と `ActorCell.addFunctionRef`／`removeFunctionRef`（`dungeon/Children.scala` の functionRefs 登録表）

移植の規約は `.takt/facets/knowledge/fraktor-porting-rules.md` に従う。形の前例は [PR #205](https://github.com/j5ik2o/fraktor-ng-rs/pull/205) の `docs/design/issue-177-frequency-sketch.md` と `docs/goals/issue-177-frequency-sketch.md`、および直近の `docs/design/issue-188-specs.md` である。

## 1. 何を移植するか

### 件数の数え直し

原典の `"..." in { ... }` を数え直した。**FunctionRefSpec は目安と違う。**

| Spec | issue／台帳の目安 | 数え直した実数（実行される件数） | 備考 |
|:---|---:|---:|:---|
| ProviderSelectionSpec | 4 | **4** | 一致 |
| ActorWithBoundedStashSpec | 5 | **5** | 一致。補助メソッド `testDeadLetters`／`testStashOverflowException` はケースではない |
| FunctionRefSpec | 5 | **9** | `commonTests` が定義する4件（forward messages／be watchable／be able to watch／terminate when their parent terminates）が、toplevel と non-toplevel の2か所で実行されるため 4×2＝8件。これに「not be found」1件を足して9件。目安5件は定義の置き場所（4＋1）を数えた値である |
| 合計 | 14 | **18** | 定義の置き場所で数えると14件、実行件数では18件。第2段の台帳の `cases.total` には実行件数の18（ノードごとには 4／5／9）を記録する |

Rust のテスト関数は実行件数に合わせて立てる（FunctionRefSpec は共通の補助関数を2つの生成経路から呼ぶ形にせず、生成経路ごとにテスト関数を分ける。§1.3）。

### 1.1 ProviderSelectionSpec の対応表

置き場所は `crates/fraktor-actor/tests/provider_selection_spec.rs`（ノード1件＝ファイル1件、冒頭に出典のパスをコメントで残す）。

原典の各ケースは3点を確認する。(1) 選択値の `fqcn`、(2) `hasCluster`、(3) 設定文字列を `Settings` に読ませたときの `ProviderClass` が (1) と一致すること。(3) は HOCON の設定ファイルと Typesafe Config に固有であり、fraktor-ng-rs には設定ファイルの仕組みが無い（§2.1）。移植では (3) を「キー文字列から選択値への変換 `ProviderSelection::from_key` が同じ対応を与えること」に置き換える。**(3) の置き換えは §5 の除外提案 E1 として挙げ、決めるのは上司である。** 承認が得られない場合、このノードは green にできない。

| # | 原典のケース | Rust のテスト名 | 移す／作らない |
|:---:|:---|:---|:---|
| 1 | create a Local ProviderSelection and set local provider fqcn in Settings | `provider_selection_local_has_local_fqcn_and_no_cluster` | 移す（(3) は E1 の置き換え） |
| 2 | create a Remote ProviderSelection and set remote provider fqcn in Settings | `provider_selection_remote_has_remote_fqcn_and_no_cluster` | 移す（同じく。原典は `ProviderSelection("remote") == ProviderSelection(RemoteActorRefProvider)` の往復も確かめる。移植も `from_key("remote")` と fqcn からの構築が一致することを確かめる） |
| 3 | create a Cluster ProviderSelection and set cluster provider fqcn in Settings | `provider_selection_cluster_has_cluster_fqcn_and_has_cluster` | 移す（同じく。`hasCluster` が真になるのはこのケースだけ） |
| 4 | create a Custom ProviderSelection and set custom provider fqcn in Settings | `provider_selection_custom_keeps_the_given_fqcn_and_has_no_cluster` | 移す（同じく。原典の値は `"other.ActorRefProvider"`。検証は利用時（dynamicAccess）に委ねられ、選択値そのものは検査しない） |

### 1.2 ActorWithBoundedStashSpec の対応表

置き場所は `crates/fraktor-actor/tests/actor_with_bounded_stash_spec.rs`。全5件を移す。ただし全件が §2.2 の新規実装（stash の容量、bounded な mailbox、dispatcher／mailbox のプログラム的な定義と alias 解決、DeadLetter の観測手段）を前提にする。設定は原典では HOCON の文字列であり、移植では Rust の値で与える（§5 の E2）。

原典の設定の要点: dispatcher 1／mailbox 1 は mailbox 容量10・stash 容量20、dispatcher 2／mailbox 2 は mailbox 容量100・stash 容量20、alias は `aliased-1 = dispatcher-1`、`aliased-2 = aliased-1` の2段。bounded mailbox の push timeout は 500ms（§5 の E2 で、timeout の意味論は移さない提案とする）。

| # | 原典のケース | Rust のテスト名 | 移す／作らない |
|:---:|:---|:---|:---|
| 1 | end up in DeadLetters in case of a capacity violation when configured via dispatcher | `bounded_stash_dead_letters_on_unstash_overflow_via_dispatcher` | 移す。stash に11件入れ、`world` で `unstashAll` したとき mailbox 容量10を超えた1件（`hello1`）が DeadLetter になり、停止後に残りの10件（`hello2`〜`hello11`）が DeadLetter になる順序まで確かめる |
| 2 | end up in DeadLetters in case of a capacity violation when configured via mailbox | `bounded_stash_dead_letters_on_unstash_overflow_via_mailbox` | 移す（ケース1と同じ流れを mailbox 指定で） |
| 3 | throw a StashOverflowException in case of a stash capacity violation when configured via dispatcher | `bounded_stash_overflows_at_the_twenty_first_stash_via_dispatcher` | 移す。20件までは成功し、21件目で容量違反になること、違反を捕らえた actor が `STASHOVERFLOW` と返して停止し、stash の20件が DeadLetter になることを確かめる。例外の受け方は §2.2 の `try_stash` で置き換える |
| 4 | throw a StashOverflowException in case of a stash capacity violation when configured via mailbox | `bounded_stash_overflows_at_the_twenty_first_stash_via_mailbox` | 移す（ケース3と同じ流れを mailbox 指定で） |
| 5 | get stash capacity from aliased dispatchers | `bounded_stash_resolves_stash_capacity_through_dispatcher_aliases` | 移す。alias の2段（aliased-2 → aliased-1 → dispatcher-1）を解決してケース1と同じ容量（mailbox 10／stash 20）が効くことを、ケース1と同じ流れで確かめる |

### 1.3 FunctionRefSpec の対応表

置き場所は `crates/fraktor-actor/tests/function_ref_spec.rs`。9件のうち8件を移し、1件は作らない提案とする（§5 の E3）。

原典の補助型: `Super` は `GetForwarder` を受けると自セルの `addFunctionRef` で転送関数を登録して FunctionRef を返し、`DropForwarder` で `removeFunctionRef` する。`SupSuper` は `Super` を子に持ち、すべてを転送する（non-toplevel の生成経路）。

| # | 原典のケース | Rust のテスト名 | 移す／作らない |
|:---:|:---|:---|:---|
| 1 | （toplevel）forward messages | `function_ref_created_by_a_toplevel_actor_forwards_messages` | 移す。転送で `Forwarded(msg, sender)` の組（元の送信者を保つ）まで確かめる |
| 2 | （toplevel）be watchable | `function_ref_created_by_a_toplevel_actor_is_watchable` | 移す。probe が watch し、`remove_function_ref` の後に `Terminated` を受ける |
| 3 | （toplevel）be able to watch | `function_ref_created_by_a_toplevel_actor_can_watch` | 移す。FunctionRef 自身が別の FunctionRef を watch し、相手の除去で `Terminated` を関数へのメッセージとして受ける。`is_watching` が除去の後も真であり、`unwatch` して初めて偽になること（原典の注記どおり、通常の actor と違う点）まで確かめる |
| 4 | （toplevel）terminate when their parent terminates | `function_ref_created_by_a_toplevel_actor_terminates_when_its_parent_terminates` | 移す。親を PoisonPill で止めると、watch していた probe が FunctionRef の `Terminated` を受ける |
| 5 | （non-toplevel）forward messages | `function_ref_created_by_a_non_toplevel_actor_forwards_messages` | 移す（ケース1と同じ流れを `SupSuper` 経由で） |
| 6 | （non-toplevel）be watchable | `function_ref_created_by_a_non_toplevel_actor_is_watchable` | 移す（ケース2と同じ） |
| 7 | （non-toplevel）be able to watch | `function_ref_created_by_a_non_toplevel_actor_can_watch` | 移す（ケース3と同じ） |
| 8 | （non-toplevel）terminate when their parent terminates | `function_ref_created_by_a_non_toplevel_actor_terminates_when_its_parent_terminates` | 移す（ケース4と同じ。親は中間の `Super` であり、`SupSuper` ではなく `Super` の停止で子 FunctionRef が終わることを確かめる） |
| 9 | （not registered）not be found | — | **作らない提案（E3）**。Java の直列化（`serialize-messages = on`）と `SerializationCheckFailedException` の EventFilter に固有であり、Rust には対応する仕組みが無い。§5 |


## 2. 本体の有無と、新しく足すもの

`crates/fraktor-actor` と `crates/fraktor-testkit` を調べた結果は次のとおりである。

| 対象 | 既存の有無 |
|:---|:---|
| ProviderSelection／Settings の provider 解決 | **無い。** `ActorSystem` は名前と境界から直接作り、設定ファイルも `ProviderClass` に当たる値も無い。`kernel::setup::ActorSystemSetup`（Setup 値の型つき集合）はある |
| stash 本体 | **ある（無制限）。** `ActorContext::stash`／`unstash_all` があり、FIFO、再起動での mailbox への返却、停止での dead letters 送りは実装済みで `tests/context_stash_test.rs` が固定している。ただし `actor_context.rs` の文書に「Pekko's `stash-capacity` setting and `StashOverflowException` are not ported」と明記され、**容量とあふれの例外は無い** |
| bounded mailbox | **無い。** mailbox は shard ごとに1本のキュー（`kernel::executor::shard_queue`）であり、actor ごとの mailbox も容量も無い。`Props` は `with_dispatcher(name)` の名前の記録だけを持ち、`with_mailbox` は無い。dispatcher／mailbox の定義（容量の表）も alias も無い |
| DeadLetter の観測 | **統合テストからは観測できない。** 停止した actor の stash は dead letters へ送られるが、dead letters は `DeadLetter` イベントを発行しない（`actor_context.rs` の文書どおり）。executor の単体テストは dead letters の宛先を記録用の function ref に差し替えて観測しているが、`tests/` の統合テストからは差し替えられない。testkit にも DeadLetter の購読手段は無い |
| FunctionRef | **部分的にある。** `ActorSystem::function_ref(name, handler)`（`kernel::executor::Executor::function_ref`）があり、システム直下の関数宛先として転送・watch・停止・watcher への `Terminated` は実装され、`actor_system_value_test.rs` が固定している。しかし原典の FunctionRef ではない。違いは3点ある。(1) 生成がシステム直下だけで、actor のセルから作る `addFunctionRef` が無い（親 actor との親子関係が無い）。(2) 返す型は通常の `ActorRef` であり、FunctionRef 自身が watch する API（`watch`／`unwatch`／`isWatching`）が無い。(3) `removeFunctionRef` に当たる除去が公開されていない（停止の経路で登録表から外れるだけ） |

### 2.1 ProviderSelection の公開 API（新規）

置き場所は `crates/fraktor-actor/src/kernel/actor/provider_selection.rs` と `provider_selection/provider_selection_value.rs`、`provider_selection_test.rs`（単体テスト）とする。原典の定義場所（`ActorSystem.scala`）に合わせ、公開面は `kernel::actor` の下に置く。

```rust
pub const LOCAL_PROVIDER_FQCN: &str = "org.apache.pekko.actor.LocalActorRefProvider";
pub const REMOTE_PROVIDER_FQCN: &str = "org.apache.pekko.remote.RemoteActorRefProvider";
pub const CLUSTER_PROVIDER_FQCN: &str = "org.apache.pekko.cluster.ClusterActorRefProvider";

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum ProviderSelection {
  Local,
  Remote,
  Cluster,
  Custom(String),
}

impl ProviderSelection {
  pub fn from_key(key: &str) -> Self;   // "local" -> Local、"remote" -> Remote、"cluster" -> Cluster、それ以外は Custom(key)
  pub fn from_fqcn(fqcn: &str) -> Self; // 3つの定数に一致すれば対応する選択値、それ以外は Custom(fqcn)
  pub fn fqcn(&self) -> &str;           // Local/Remote/Cluster は定数、Custom は保持した文字列
  pub fn has_cluster(&self) -> bool;    // Cluster のときだけ真
}
```

原典と違える点:

- 原典の `Settings.ProviderClass` は、設定文字列を Typesafe Config で読んだ結果である。移植は設定の読み取りを持たないので、`from_key` がその対応（キー → 選択値 → fqcn）を担う（E1）。
- 原典の `Custom` は利用時に dynamicAccess で検査される。移植の `Custom` も構築では検査しない。空文字、空白だけ、末尾のドット、ドットの連続、極端に長い文字列も、すべて `Custom` としてそのまま保持する（§3）。
- 大文字小文字は原典どおり区別する。`"LOCAL"` や `" local"` は組み込みのキーではなく `Custom` になる。
- `ActorSystem` の生成に ProviderSelection を結線することはしない。fraktor-ng-rs の actor system は remote／cluster の provider 実装を持たず、選択値を消費する先が無い。結線は provider 実装を持つ issue の仕事とし、本 issue では選択値の型と対応だけを移植する。この範囲の切り方は設計レビューで確認してほしい点である。

no_std: `Custom` の `String` が alloc、それ以外は core のみで書ける。新しい依存は足さない。

### 2.2 bounded stash の公開 API（新規と変更）

置き場所と中身は次のとおりである。

| ファイル | 中身 |
|:---|:---|
| `crates/fraktor-actor/src/kernel/actor/stash_overflow_exception.rs`（新規） | `StashOverflowException`（`InvalidActorNameException` の前例と同じく、例外の名前を保った値の型） |
| `crates/fraktor-actor/src/kernel/actor/dispatch_settings.rs`（新規） | `DispatchSettings`（dispatcher／mailbox の定義と alias の表。`Setup` として `ActorSystemSetup` に載る） |
| `crates/fraktor-actor/src/kernel/actor/props.rs`（変更） | `with_mailbox(name)` と `mailbox()` を足す（`with_dispatcher` と対の、名前の記録） |
| `crates/fraktor-actor/src/kernel/actor/actor_context.rs`（変更） | `try_stash()` を足し、`stash()` の文書の「not ported」を更新する |
| `crates/fraktor-actor/src/kernel/executor/executor_value.rs`（変更） | spawn 時に定義を解決して actor ごとに保持する。stash の容量検査、mailbox の件数えと `unstash_all` のあふれの dead letters 送り、DeadLetter の発行（§2.3） |
| `crates/fraktor-actor/src/kernel/actor/dead_letter.rs`（新規） | `DeadLetter` の値の型（§2.3） |
| `crates/fraktor-actor/tests/actor_with_bounded_stash_spec.rs` | 原典5ケース |
| `crates/fraktor-actor/tests/bounded_stash_property_test.rs` | PBT／モデル照合（§3。原典外で台帳の件数に含めない） |
| `crates/fraktor-actor/specs/bounded_stash.qnt` | Quint（§4） |

```rust
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct MailboxDefinition {
  pub mailbox_capacity: Option<usize>, // None は無制限。原典の Bounded10 は Some(10)、Bounded100 は Some(100)
  pub stash_capacity: Option<usize>,   // None は無制限。原典の設定は Some(20)
}

#[derive(Clone, Default)]
pub struct DispatchSettings {
  // 定義名 -> 定義、alias 名 -> 参照先の名前
  pub fn new() -> Self;
  pub fn define_dispatcher(&mut self, name: &str, definition: MailboxDefinition) -> &mut Self;
  pub fn define_mailbox(&mut self, name: &str, definition: MailboxDefinition) -> &mut Self;
  pub fn alias_dispatcher(&mut self, alias: &str, target: &str) -> &mut Self;
  pub fn resolve_dispatcher(&self, name: &str) -> Result<Option<MailboxDefinition>, DispatchResolutionError>;
  pub fn resolve_mailbox(&self, name: &str) -> Result<Option<MailboxDefinition>, DispatchResolutionError>;
}
impl Setup for DispatchSettings {}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum DispatchResolutionError {
  UnknownName { name: String },
  AliasCycle { name: String },
}

impl ActorContext<'_> {
  pub fn try_stash(&mut self) -> Result<(), StashOverflowException>;
  // stash() は従来どおり。容量が設定された actor では、あふれのときに
  // StashOverflowException で panic する（原典の throw に対応する経路）。
}
```

設計の判断:

- **例外の受け方は `try_stash` で置き換える。** 原典の overflow 用 actor は `stash()` の例外を try-catch で捕らえて数え、21件目で返信して自ら停止する。Rust の `receive` は戻り値を持たないので、panic では actor 自身が捕らえられない（監督の再起動になる）。`try_stash() -> Result` を正の経路とし、ケース3・4の actor はこれを使う。既存の `stash()` は互換のため残し、容量違反では panic する（原典で例外を捕らえなかった場合に監督へ伝わる動きに対応する）。既存の無制限の actor では振る舞いは変わらない。
- **容量の出どころは dispatcher と mailbox の両方であり、mailbox 指定があれば mailbox の定義を使う。** 原典では、actor に mailbox が指定されればその mailbox の設定から、無ければ dispatcher の設定から `stash-capacity` と mailbox 容量を取る。移植も同じ優先順にする。どちらも指定が無ければ無制限（現在の振る舞い）。
- **alias は名前の解決だけに効く。** alias の段数は任意とし、解決は定義に届くまで辿る。循環（自己参照を含む）と未定義の名前は、spawn の時点で `DispatchResolutionError` として失敗させる。原典は設定の読み取り時に失敗する経路であり、spawn 時の失敗が最も近い対応である。黙って既定に落とすことはしない（容量が効かないままテストが通る事故を防ぐため）。
- **bounded mailbox の容量は「その actor 宛てに滞留している件数」で数える。** 移植の実行系は shard ごとに1本のキューを持ち、actor ごとの mailbox が無い。そこで executor が actor ごとに滞留件数を持ち、`unstash_all` の前置で容量を超える分を dead letters へ送る（ケース1・2・5が確かめるのはこの経路だけである）。通常の `tell` が容量を超えた場合の push timeout（500ms 待って失敗）は、同期的な kernel の実行系には対応が無いので移さない（E2）。通常配送のあふれは、滞留件数を超えた分を dead letters へ送る形にそろえる（待たない）。この判断も設計レビューで確認してほしい点である。
- 容量0は正の設定として受け付ける。stash 容量0では最初の `try_stash` が容量違反になり、mailbox 容量0では `unstash_all` の全件が dead letters へ行く（§3）。

no_std: 定義の表は `BTreeMap`（alloc）、容量の検査と件数えは core のみで書ける。時刻（push timeout）には触れないので Clock の port も要らない。新しい依存は足さない。

### 2.3 DeadLetter の観測手段（新規）

ケース1・2・5は `DeadLetter(message, sender, recipient)` を testActor が受けることを確かめる。移植では、dead letters へ送られたメッセージを値として発行し、テストが購読できるようにする。

```rust
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DeadLetter {
  pub message: Message,   // 送られたメッセージそのもの
  pub sender: ActorRef,   // 元の送信者
  pub recipient: ActorRef // 宛先だった actor
}
```

発行先は、既存の event stream に `DeadLetter` を載せる案と、testkit に記録用の受け口を足す案の2つがある。**推奨は event stream 案である。** 原典も `system.eventStream.subscribe(testActor, classOf[DeadLetter])` で受ける形であり、対応が直接である。executor が dead letters へ送る箇所（停止時の stash とキューの始末、`unstash_all` のあふれ）で `DeadLetter` を発行する。実装の段では、event stream の購読が統合テストの probe から使えることを最初に確かめ、使えない場合は testkit 側の受け口を足す範囲もこの issue に含める（write_scope の判断が要れば設計レビューで示してほしい）。

### 2.4 FunctionRef の公開 API（新規）

置き場所は `crates/fraktor-actor/src/kernel/actor/function_ref.rs` と `function_ref/function_ref_value.rs`、単体テスト `function_ref_value_test.rs` とする。既存の `ActorSystem::function_ref`（システム直下）は互換のため残し、内部の登録表を共有する。

```rust
pub struct FunctionRef {
  inner: ActorRef,              // 宛先としてのハンドル（id＋世代）
  system: WeakActorSystem,      // watch の登録表へ届くための弱い参照
}

impl FunctionRef {
  pub fn as_actor_ref(&self) -> ActorRef;
  pub fn path(&self) -> ActorPath;
  pub fn tell<M: Any + Send + Sync>(&self, message: M, sender: ActorRef);
  pub fn watch(&self, target: &ActorRef) -> ActorRef;   // この FunctionRef を watcher として登録する
  pub fn unwatch(&self, target: &ActorRef) -> ActorRef;
  pub fn is_watching(&self, target: &ActorRef) -> bool;
}
impl Clone for FunctionRef {}
impl PartialEq for FunctionRef {} // inner の id＋世代で比べる

impl ActorContext<'_> {
  pub fn add_function_ref<H>(&mut self, handler: H) -> FunctionRef
  where
    H: FnMut(&ActorRef, Message) + Send + Sync + 'static;
  pub fn remove_function_ref(&mut self, target: &FunctionRef) -> bool; // 登録が有れば外して真、無ければ偽
}
```

設計の判断:

- **返す型は通常の `ActorRef` ではなく専用の `FunctionRef` にする。** ケース3が要る「FunctionRef 自身が watch する」操作の置き場所が要るためである。`ActorRef` は id＋世代の値であり、登録表への届き先を持たない。`FunctionRef` は弱い system 参照を持ち、`watch`／`unwatch`／`is_watching` を登録表に対して行う。宛先として使う箇所（probe の watch、メッセージの送信、等価の比較）は `as_actor_ref()` と `tell` でまかなう。
- **生成は親セルの下に置く。** `add_function_ref` は親 actor のパス配下に名前を生成して登録し、親の停止で一緒に終わる（ケース4・8）。名前は executor が一意に生成する（原典もセル内の一意な名前を生成する）。同じ親から複数作れる。
- **除去は冪等にする。** `remove_function_ref` は、すでに除去された FunctionRef や、ほかの親が作った FunctionRef に対しては何もせず偽を返す。原典は自セルの登録表からだけ外すので、他人の FunctionRef は外せない。
- **除去された FunctionRef への送信は dead letters へ行く。** 登録が無い宛先への配送は、既存の function ref の停止後の扱いと同じにする。
- **`is_watching` の意味は原典どおりにする。** watch 中の相手が終わると `Terminated` が FunctionRef へメッセージとして届くが、登録は自動では外れない。`unwatch` を呼ぶまで `is_watching` は真のままである（ケース3が固定する、通常の actor と違う点）。
- 既存の `ActorSystem::function_ref` の戻り値の型は変えない（公開 API の互換）。その宛先を watch する既存の単体テスト群にも触れない。

no_std: ハンドラは `Box<dyn FnMut>`（alloc）、登録表は alloc の表で書ける。弱い system 参照は既存の `WeakActorSystem` を使う。新しい依存は足さない。


## 3. 守る性質（不変条件）と壊しうる入力

### ProviderSelection

| 番号 | 不変条件 |
|:---|:---|
| S1 | 対応の固定：`from_key("local").fqcn()` は LOCAL の定数、`from_key("remote")` は REMOTE の定数、`from_key("cluster")` は CLUSTER の定数であり、`from_fqcn` はその逆を与える（往復が一致する） |
| S2 | `has_cluster()` が真になるのは `Cluster` のときだけである |
| S3 | `Custom` は与えられた文字列を1文字も変えずに保持し、`fqcn()` がそれを返す。組み込みの fqcn と一致する文字列を `from_key` に渡した場合は `Custom` のままである（キーと fqcn は別の名前空間） |
| S4 | 等価：選択値は値で比べる。`Custom("a") == Custom("a")` であり、`Custom("local") != Local` である |

壊しうる入力:

| 入力 | 何が起きうるか | 確かめているもの |
|:---|:---|:---|
| 空文字 `""` | `Custom("")` になる。原典も構築では検査しない | PBT、単体テスト |
| 大文字・空白の混入（`"LOCAL"`、`" local"`、`"local "`） | 組み込みキーとみなされると S1 が崩れる | PBT（キー候補に含める） |
| ワイルドカード的な文字列（`"cl*"`、`"*"`、ドットの連続 `"a..b"`、末尾のドット） | provider の選択はパターンマッチではないので、すべて `Custom` として保持する | PBT |
| 極端に長い文字列（10,000文字） | 切り詰めや複製の事故が無いこと | PBT（長さの候補に含める） |
| NUL や非 ASCII を含む文字列 | Rust の `String` は保持できる。そのまま `Custom` になる | PBT |

PBT は足す。`crates/fraktor-actor/tests/provider_selection_property_test.rs` に置き、固定 seed の SplitMix64（前例と同じく外部依存は足さない。`proptest` の採否は [#212](https://github.com/j5ik2o/fraktor-ng-rs/issues/212) で決める）で任意の文字列について S1〜S4 を確かめる。文字列の候補には、組み込みキー3つ、その大文字・空白違い、空文字、長い文字列、非 ASCII を混ぜる。

### bounded stash

容量を `cap_s`（stash）、`cap_m`（mailbox）、stash の中身を古い順の列とする。

| 番号 | 不変条件 |
|:---|:---|
| B1 | stash の件数は常に対象の容量以下である。`try_stash` は、件数が容量未満のときだけ成功し、容量に達しているときは状態を変えずに `Err(StashOverflowException)` を返す。容量が未設定（None）なら常に成功する |
| B2 | 順序：stash は FIFO である。`unstash_all` は古い順に mailbox の先頭へ戻す（既存の振る舞いを変えない） |
| B3 | `unstash_all` のあふれ：戻す件数と滞留件数の和が `cap_m` を超える分は、古い順の先頭から数えて「容量に収まる分」を残し、超えた分が dead letters へ行く。原典のケース1では、戻す11件のうち最も古い `hello1` が dead letters へ行く（前置で mailbox に入らなかった分が落ちる）。移植の実装はこの観測（落ちるのは最も古い1件、残り10件は処理される）と一致させる |
| B4 | 停止時の始末：actor が止まると、stash の全件が元の送信者を保って古い順に dead letters へ行く（既存の振る舞い）。`DeadLetter` の発行順も同じ順である |
| B5 | 設定の解決：mailbox 指定があれば mailbox の定義、無ければ dispatcher の定義を使う。alias は定義に届くまで辿り、循環と未定義は spawn 時にエラーになる。解決の結果は spawn の時点で固定され、後から定義を変えても既存の actor には効かない |
| B6 | 容量は actor ごとに数える。同じ dispatcher を使う別の actor の stash や滞留は、対象の容量を消費しない |

壊しうる入力:

| 入力 | 何が起きうるか | 確かめているもの |
|:---|:---|:---|
| stash 容量0／1 | 0では最初の stash が違反、1では2件目が違反。境界の数え違い（`<` と `<=`）が出る | 単体テスト、PBT（容量の候補に 0 と 1 を含める） |
| ちょうど容量まで／容量＋1 | 原典は20件成功・21件目が違反。off-by-one が出る | ケース3・4、PBT |
| mailbox 容量0／ちょうど滞留と一致／1だけ不足 | `unstash_all` の落ちる件数と、落ちるのが古い側か新しい側か | 単体テスト、PBT、Quint（§4） |
| alias の循環（自己参照、2つの相互参照）、未定義の名前、空文字の名前 | 無限ループや黙った既定落ちが起きうる | 単体テスト（解決の単体テストに置く） |
| dispatcher と mailbox の両方を指定 | 優先順の取り違え（B5） | 単体テスト |
| 同じメッセージの二重 stash | 既存の契約（最新の stash と同じ配送は panic）があり、容量検査と混ざると数えがずれる | 既存の `context_stash_test` の範囲。本 issue では変えない |
| PoisonPill と stash の始末の競合する順序 | 停止の始末で stash が二重に dead letters へ行く、または消える | ケース1・2（PoisonPill で止める流れ） |

PBT は足す。`bounded_stash_property_test.rs` で、固定 seed の操作列（stash、unstash_all、停止、容量 0／1／2／小さい値／未設定の組み合わせ）を `VecDeque` の参照モデルと照合し、B1〜B4 を毎手確かめる。公開 API から stash の中身は直接読めないので、照合は「actor が受けたメッセージの列と dead letters へ行った列」の観測でする。容量の解決（B5）は決定的な関数なので、PBT ではなく単体テストで表にする。

### FunctionRef

| 番号 | 不変条件 |
|:---|:---|
| F1 | 転送：登録されている FunctionRef への送信は、ハンドラを1回だけ呼び、元の送信者を保つ。除去の後はハンドラを呼ばず、dead letters へ行く |
| F2 | watch 可能性：FunctionRef は通常の actor と同じく watch でき、除去されると watcher が `Terminated` をちょうど1回受ける。二重の除去では2回目は来ない |
| F3 | watch する側：FunctionRef が watch した相手が終わると、`Terminated` がハンドラへのメッセージとして届く。登録は自動で外れず、`unwatch` の後だけ `is_watching` が偽になる。`unwatch` は冪等である |
| F4 | 親の停止：親 actor が止まると、その親が作ったすべての FunctionRef が終わり、それぞれの watcher が `Terminated` を受ける。ほかの親が作った FunctionRef は影響を受けない。親の子 actor の FunctionRef は、間の親（生成した actor）が止まったときに終わる |
| F5 | 除去の範囲：`remove_function_ref` が外せるのは、自セルが作った FunctionRef だけである。他人の FunctionRef への除去は無効果で偽を返す |
| F6 | 一意性：同じ親から作った2つの FunctionRef は別の宛先であり、片方の除去がもう片方の転送を止めない。パスの名前も衝突しない |

壊しうる入力:

| 入力 | 何が起きうるか | 確かめているもの |
|:---|:---|:---|
| 除去を2回呼ぶ／除去済みへの送信 | 二重の `Terminated`、panic、消えた宛先への配送 | 単体テスト、モデル照合 |
| ほかの親の FunctionRef の除去 | F5 の崩れ（他人の宛先が消える） | 単体テスト |
| 終わった相手への watch | Pekko では終わった宛先への watch は直ちに `Terminated` を送る。移植の既存の watch の扱いにそろえる | 単体テスト（既存の function ref の watch の単体テストと同じ扱いであることを確かめる） |
| `unwatch` を呼ばないままの放置 | F3 のとおり登録が残る。ケース3が固定する仕様であり、漏れとして直さない | ケース3・7 |
| 親の停止と除去の競合する順序（止まる直前の除去、停止後の除去の呼び出し） | 停止したセルの context からは除去を呼べないので、競合は「停止処理の中で登録表が二重に片付けられる」形になる。二重の `Terminated` が出ないこと | モデル照合、Quint（§4） |
| 大量の生成と除去（名前の生成の衝突） | 生成名の衝突で別人の宛先になる | PBT（生成と除去の列） |

PBT（モデル照合）は足す。`crates/fraktor-actor/tests/function_ref_model_test.rs` に置き、固定 seed の操作列（生成、除去、watch、unwatch、親の停止、送信）を、登録表の参照モデル（登録中の集合、watch の対応、届いた `Terminated` の集合）と照合して F1〜F6 を確かめる。乱数は固定 seed の SplitMix64 で、失敗時は seed と手の番号を表示する。

## 4. 状態遷移と Quint の判断

**bounded stash には Quint を書く。FunctionRef にも Quint を書く。ProviderSelection には書かない。**

### bounded stash に書く理由と形

stash は、件数・容量・滞留件数の3つの数と「stash 中／mailbox 滞留中／dead letters 済み／処理済み」の位置を持つ状態機械であり、B3（あふれで落ちるのが古い側であること）と B4（停止で全件が1回ずつ dead letters へ行くこと）は、遷移の列の全体で成り立つ性質である。容量の境界（ちょうど容量、容量＋1）を取り違える誤りは、固定のケースの組み合わせをすり抜けうる。

- 置き場所は `crates/fraktor-actor/specs/bounded_stash.qnt` の1ファイル、1モジュールとする（CI の `scripts/quint-check-specs.sh` の対象 `crates/*/specs/*.qnt` に載せる。TokenBucket の前例と同じ形）。
- 状態：`stashed: List[int]`（メッセージの通し番号、古い順）、`queued: int`（mailbox の滞留件数）、`dead: List[int]`、`handled: List[int]`、設定は `pure val`（stash 容量と mailbox 容量の組。0 と小さい値のインスタンスを置く）。
- 遷移：`stashNext`、`unstashAll`、`stop`、`handleOne`（滞留の先頭を処理済みにする）。
- 不変条件：`stashWithinCapacity`（B1）、`noLossNoDuplication`（すべての通し番号が、stash・滞留・dead・処理済みのどこか1か所にちょうど1回ずつ現れる）、`deadLettersAreOldestOnOverflow`（B3。あふれで dead へ行った分は、その時点で最も古い分である）、まとめて `val invariant`。
- 負の対照：`bugNewestDropped`（あふれで新しい側を落とす。`deadLettersAreOldestOnOverflow` で落ちる）、`bugOffByOneCapacity`（容量＋1まで許す。`stashWithinCapacity` で落ちる）、`bugStopDropsStash`（停止で stash を捨てる。`noLossNoDuplication` で落ちる）。対照は名前が `Test` で終わる `run` にし、誤りが不変条件を崩すことを確かめる。
- モデルにしないもの：push timeout の時間（時刻はモデルに入れない。500ms の意味論は E2 で移さない提案）、dispatcher／mailbox の名前解決（決定的な関数であり単体テストの表で確かめる）。

### FunctionRef に書く理由と形

FunctionRef の登録表は、生成・除去・親の停止という3つの遷移で watch の対応が変わる状態機械であり、F2／F4 の「`Terminated` がちょうど1回」は列の性質である。除去と親の停止の片付けが二重に走る誤りは、固定ケースでは順序の組み合わせ次第で通ってしまう。

- 置き場所は `crates/fraktor-actor/specs/function_ref.qnt` の1ファイル、1モジュールとする。
- 状態：`registered: Set[int]`（FunctionRef の通し番号）、`parentOf: int -> int`、`watching: Set[(int, int)]`（watcher と対象。watcher は FunctionRef の番号と外部の probe を表す番兵の両方を含む）、`terminatedSent: Set[(int, int)]`（届けた `Terminated` の組）。
- 遷移：`addRef(parent)`、`removeRef(ref)`、`stopParent(parent)`、`watch(watcher, ref)`、`unwatch(watcher, ref)`。
- 不変条件：`terminatedExactlyOnce`（同じ組の `Terminated` は1回まで）、`terminatedOnlyAfterEnd`（`Terminated` が届いた対象は登録から外れている）、`noForwardAfterRemoval`（登録外の番号へは転送が起きない。ゴーストの転送回数で表す）、`watchSurvivesUntilUnwatch`（F3。相手が終わっても watch の登録は `unwatch` まで残る）、まとめて `val invariant`。
- 負の対照：`bugDoubleCleanup`（除去と親の停止で二重に通知する）、`bugAutoUnwatch`（相手の終了で watch 登録を自動で外す。F3 に反する）、`bugOrphanRefs`（親の停止で FunctionRef を残す）。
- モデルにしないもの：ハンドラの中身（転送は回数だけを数える）、直列化（E3）、パスの文字列。

### ProviderSelection に書かない理由

ProviderSelection は状態を持たない純粋な対応（文字列 → 選択値 → fqcn）であり、遷移が無い。守る性質 S1〜S4 はすべて1回の変換で決まり、PBT が任意の文字列で直接確かめる。Quint で書いても同じ照合を別の記法で繰り返すことになり、新しく落ちる誤りの種類が増えない（ImmutableIntMap に Quint を書かなかった #188 の判断と同じ）。

## 5. 作らない・除外の提案（決めるのは上司）

次の3件は提案であり、**決めるのは上司である。** 承認が台帳の対象ノードの `cases.excluded` に `approved_by` と `approved_at` の両方つきで記録されるまで、第2段では除外として扱わない。承認が得られない項目がある場合、そのノードは green にできないので、第2段は needs_human として止まる。

| 番号 | 対象 | 提案 | 理由 |
|:---|:---|:---|:---|
| E1 | ProviderSelectionSpec の全4ケースのうち、設定読み取りの部分 | HOCON の設定文字列と Typesafe Config の `Settings.ProviderClass`、クラスローダ経由の解決は作らない。キー → 選択値 → fqcn の対応に置き換えてケース自体は移す | fraktor-ng-rs には設定ファイルの仕組みが無く、provider の選択はプログラム的に与える設計である。fqcn は JVM のクラス名であり、Rust では識別子の文字列として保持する以上の意味を持たない |
| E2 | ActorWithBoundedStashSpec の設定方法と push timeout | HOCON の文字列による dispatcher／mailbox の定義、クラス名（FQCN）による mailbox 型の生成（`Bounded10`／`Bounded100`）は作らない。定義は Rust の値（`DispatchSettings`）で与える。bounded mailbox の push timeout（500ms）の意味論も作らない。通常配送のあふれは待たずに dead letters へ送る | 設定ファイルとリフレクションによる型生成は JVM 固有である。timeout は同期的な kernel の実行系に対応が無く、ケースの観測（あふれの行き先と順序）には現れない |
| E3 | FunctionRefSpec のケース9「not be found」 | 作らない | 前提が Java の直列化の検証（`serialize-messages = on` で、登録されていない FunctionRef を含むメッセージの直列化が失敗し、`SerializationCheckFailedException` が EventFilter に掛かること）であり、Rust には Java 直列化も直列化検証の仕組みも無い。移植の `FunctionRef` は公開 API からは登録された状態でしか作れないので、「登録されていない FunctionRef」という入力自体が型で表せない |

自作だと思うファイルについて：本 issue で新しく作るファイルは、すべて Pekko の原典を参照する移植であり、自作と判断するファイルは無い。第2段で判断が変わるファイルが出たら、パスと理由を PR 本文と報告に書く。

## 6. 台帳の更新方針（第2段）

本書の段では台帳を触らない。第2段では、対象ノード3件を次のように更新する。

| ノード id | total／ported／passing | rust.test_file | status |
|:---|:---|:---|:---|
| `actor-tests/src/test/scala/org/apache/pekko/actor/ProviderSelectionSpec.scala` | 4／4／4（E1 が承認された場合） | `crates/fraktor-actor/tests/provider_selection_spec.rs` | green |
| `actor-tests/src/test/scala/org/apache/pekko/actor/ActorWithBoundedStashSpec.scala` | 5／5／5（E2 が承認された場合） | `crates/fraktor-actor/tests/actor_with_bounded_stash_spec.rs` | green |
| `actor-tests/src/test/scala/org/apache/pekko/actor/FunctionRefSpec.scala` | 9／8／8、除外1件（E3 が承認され、`cases.excluded` に承認の記録が入った場合） | `crates/fraktor-actor/tests/function_ref_spec.rs` | green |

あわせて `rust.crate` に `fraktor-actor`、`verified_against: v2.0.0` を書き、`docs/baseline/v2.0.0-undocumented-nodes.tsv` を作り直して対象ノードの記載をそろえる（#188 の前例と同じ）。内部 API を呼ぶ単体テスト（ProviderSelection の往復、DispatchSettings の解決、FunctionRef の除去の範囲）は `rust.cases_outside_test_file` に記録する。ゴールの完了判定は `ledger/tools/rollup.py --goal <ゴールID>` の終了コードで行う。

## 7. 第2段ですること（本書の範囲の外）

- red（失敗するテスト）→ green の順に実装する。先に3つの spec ファイルを書き、コンパイルの失敗かテストの失敗で red を確認してから本体を足す。ノードは1件ずつ処理する（ProviderSelection → bounded stash → FunctionRef の順を推奨する。前の2件の基盤（DeadLetter の発行）が FunctionRef の観測にも効くため）。
- §5 の除外提案 E1〜E3 について、上司の承認を台帳の記録で確かめてから反映する。承認が無い項目は除外にせず、needs_human として報告する。
- CI と同じ検査を流す（`cargo fmt --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`、ledger/tools の Python テスト、Quint は `scripts/quint-check-specs.sh` をそのまま流し、`bounded_stash.qnt` と `function_ref.qnt` が対象に入っていることを確かめる）。
- 既存の公開 API との互換を壊さない。変更は §2 に挙げた追加と、`stash()` の文書の更新、executor の内部（容量の保持と DeadLetter の発行）だけである。
- **PR 本文の1行目の扱い**：実装を終えてドラフトから外すときは、PR 本文の1行目を `Refs #192` から `Closes #192` に変える（マージで issue が閉じるように）。ただし、作らない・除外（§5 の E1〜E3）が承認待ちのものは、承認が出るまで `Refs #192` のままにする。
- **ライセンスヘッダー**：新しく作る `crates/**/*.rs` には、すべて Apache-2.0 のヘッダーを付ける。Pekko の移植では `python3 tools/license-headers/license_headers.py --apply --kind pekko --source <原典のパス> <ファイル>` を使い、原典を参照していないファイルは `--source` を省く。`--kind pekko` で付けるファイルには、`Mirrors org.apache.pekko.…` か移植のマーカーを1行入れる。MIT を含む式は書かない。push の前に `python3 tools/license-headers/license_headers.py --check --manifest docs/design/issue-206-file-origin.tsv` を流し、0 で終わることを確かめる（PR #215 で入った検査。手順の正は Mac mini の `~/.orca/bots/fraktor-ng-rs/skills/fraktor-migration-intake/SKILL.md` の「ライセンスヘッダーと第三者帰属」の節）。
- main を取り込むときは、PR #215 のライセンスヘッダーで競合が出たら両方を残す形で解消する。
