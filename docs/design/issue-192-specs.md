# 設計：#192 ProviderSelectionSpec／ActorWithBoundedStashSpec／FunctionRefSpec

> 2026-11-26 修正：独立レビュー（高3・中11・低7、全21件）と、元帥の決定（E1・E2 の扱い、E3 の扱い、H1 の方向）を受けて本書を直した。E3 は当初判断待ちとしたが、その後オーナーが除外として承認した（元帥の項目426、2026-11-26）ため、本書の E3 の記述は承認済みの除外として直した。指摘への対応は末尾の §8 回答表に1件ずつ書く。

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

原典の各ケースは3点を確認する。(1) 選択値の `fqcn`、(2) `hasCluster`、(3) 設定文字列を `Settings` に読ませたときの `ProviderClass` が (1) と一致すること。(3) は HOCON の設定ファイルと Typesafe Config に固有であり、fraktor-ng-rs には設定ファイルの仕組みが無い（§2.1）。移植では (3) を「設定文字列から選択値への変換 `ProviderSelection::from_setting`（原典の `ProviderSelection.apply` の写し、§2.1）が同じ対応を与えること」に置き換える。**この置き換えはケースの除外ではなく、移植の忠実さの判断として扱う（元帥の決定、2026-11-26。G0b の決定 E と #154 の D1 の前例。§5 の E1）。** 数えている4ケースはすべて、HOCON の代わりの型付きの入力で原典と同じことを確かめる。

| # | 原典のケース | Rust のテスト名 | 移す／作らない |
|:---:|:---|:---|:---|
| 1 | create a Local ProviderSelection and set local provider fqcn in Settings | `provider_selection_local_has_local_fqcn_and_no_cluster` | 移す（(3) は E1 の忠実さの判断による置き換え） |
| 2 | create a Remote ProviderSelection and set remote provider fqcn in Settings | `provider_selection_remote_has_remote_fqcn_and_no_cluster` | 移す（同じく。原典は `ProviderSelection("remote") == ProviderSelection(RemoteActorRefProvider)` を、同じ `apply` にキーと FQCN を渡して確かめる。移植も同じ入口 `from_setting` に両方を渡して比べる） |
| 3 | create a Cluster ProviderSelection and set cluster provider fqcn in Settings | `provider_selection_cluster_has_cluster_fqcn_and_has_cluster` | 移す（同じく。`hasCluster` が真になるのはこのケースだけ。キー `"cluster"` と Cluster の FQCN のどちらからも `Cluster` になる） |
| 4 | create a Custom ProviderSelection and set custom provider fqcn in Settings | `provider_selection_custom_keeps_the_given_fqcn_and_has_no_cluster` | 移す（同じく。原典の値は `"other.ActorRefProvider"`。検証は利用時（dynamicAccess）に委ねられ、選択値そのものは検査しない。なお Local の FQCN も原典では `Custom` になる非対称性があり、`from_setting(LOCAL_PROVIDER_FQCN) == Custom(..)` であることもこのケース群で確かめる） |

### 1.2 ActorWithBoundedStashSpec の対応表

置き場所は `crates/fraktor-actor/tests/actor_with_bounded_stash_spec.rs`。全5件を移す。ただし全件が §2.2 の新規実装（stash の容量、bounded な mailbox、dispatcher／mailbox のプログラム的な定義と alias 解決、DeadLetter の観測手段）を前提にする。設定は原典では HOCON の文字列であり、移植では Rust の値で与える（§5 の E2。ケースの除外ではなく忠実さの判断）。

原典の設定の要点: dispatcher 1／mailbox 1 は mailbox 容量10・stash 容量20、dispatcher 2／mailbox 2 は mailbox 容量100・stash 容量20、alias は `aliased-1 = dispatcher-1`、`aliased-2 = aliased-1` の2段。bounded mailbox の push timeout は 500ms だが、ケースの観測には現れない振る舞いであり、移さない（§5 の E2 に1行で書く忠実さの判断）。

| # | 原典のケース | Rust のテスト名 | 移す／作らない |
|:---:|:---|:---|:---|
| 1 | end up in DeadLetters in case of a capacity violation when configured via dispatcher | `bounded_stash_dead_letters_on_unstash_overflow_via_dispatcher` | 移す。stash に11件入れ、`world` で `unstashAll` したとき mailbox 容量10を超えた1件（`hello1`）が DeadLetter になり、停止後に残りの10件（`hello2`〜`hello11`）が DeadLetter になる順序まで確かめる |
| 2 | end up in DeadLetters in case of a capacity violation when configured via mailbox | `bounded_stash_dead_letters_on_unstash_overflow_via_mailbox` | 移す（ケース1と同じ流れを mailbox 指定で） |
| 3 | throw a StashOverflowException in case of a stash capacity violation when configured via dispatcher | `bounded_stash_overflows_at_the_twenty_first_stash_via_dispatcher` | 移す。20件までは成功し、21件目で容量違反になること、違反を捕らえた actor が `STASHOVERFLOW` と返して停止し、stash の20件が DeadLetter になることを確かめる。例外の受け方は §2.2 の `try_stash` で置き換える |
| 4 | throw a StashOverflowException in case of a stash capacity violation when configured via mailbox | `bounded_stash_overflows_at_the_twenty_first_stash_via_mailbox` | 移す（ケース3と同じ流れを mailbox 指定で） |
| 5 | get stash capacity from aliased dispatchers | `bounded_stash_resolves_stash_capacity_through_dispatcher_aliases` | 移す。alias の2段（aliased-2 → aliased-1 → dispatcher-1）を解決してケース1と同じ容量（mailbox 10／stash 20）が効くことを、ケース1と同じ流れで確かめる |

### 1.3 FunctionRefSpec の対応表

置き場所は `crates/fraktor-actor/tests/function_ref_spec.rs`。9件のうち8件を移し、1件（ケース9）は除外としてオーナーが承認した（§5 の E3）。

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
| 8 | （non-toplevel）terminate when their parent terminates | `function_ref_created_by_a_non_toplevel_actor_terminates_when_its_parent_terminates` | 移す。原典は `SupSuper`（`s`）へ PoisonPill を送る。PoisonPill は転送されず `SupSuper` 自身が止まり、その子 `Super` が止まり、それで `Super` が作った FunctionRef が終わる、という**祖先の停止の連鎖**を確かめる（レビュー M8 で読み違いを修正） |
| 9 | （not registered）not be found | — | **除外（E3、オーナーの承認済み）**。理由の文は §5 の E3 にある |


## 2. 本体の有無と、新しく足すもの

`crates/fraktor-actor` と `crates/fraktor-testkit` を調べた結果は次のとおりである。

| 対象 | 既存の有無 |
|:---|:---|
| ProviderSelection／Settings の provider 解決 | **無い。** `ActorSystem` は名前と境界から直接作り、設定ファイルも `ProviderClass` に当たる値も無い。`kernel::setup::ActorSystemSetup`（Setup 値の型つき集合）はある |
| stash 本体 | **ある（無制限）。** `ActorContext::stash`／`unstash_all` があり、FIFO、再起動での mailbox への返却、停止での dead letters 送りは実装済みで `tests/context_stash_test.rs` が固定している。ただし `actor_context.rs` の文書に「Pekko's `stash-capacity` setting and `StashOverflowException` are not ported」と明記され、**容量とあふれの例外は無い** |
| bounded mailbox | **無い。** mailbox は shard ごとに1本のキュー（`kernel::executor::shard_queue`）であり、actor ごとの mailbox も容量も無い。`Props` は `with_dispatcher(name)` の名前の記録だけを持ち、`with_mailbox` は無い。dispatcher／mailbox の定義（容量の表）も alias も無い |
| DeadLetter の観測 | **統合テストからは観測できない。** 停止した actor の stash は dead letters へ送られるが、dead letters は `DeadLetter` イベントを発行しない（`actor_context.rs` の文書どおり）。executor の単体テストは dead letters の宛先を記録用の function ref に差し替えて観測しているが、`tests/` の統合テストからは差し替えられない。testkit にも DeadLetter の購読手段は無い |
| FunctionRef | **部分的にある。** `ActorSystem::function_ref(name, handler)`（`kernel::executor::Executor::function_ref`）があり、システム直下の関数宛先として転送・watch・停止・watcher への `Terminated` は実装され、`actor_system_value_test.rs` が固定している。また `ActorSystem::watch(watcher, target)` はどの `ActorRef` でも watcher にできるので、「FunctionRef が watch される」側と「任意の宛先を watch する」側の土台はある（レビュー L3 で記述を修正）。しかし原典の FunctionRef ではない。違いは3点ある。(1) 生成がシステム直下だけで、actor のセルから作る `addFunctionRef` が無い（親 actor との親子関係が無い）。(2) 返す型は通常の `ActorRef` であり、FunctionRef 自身を watcher として登録する操作と、`unwatch` と `isWatching` が無い。(3) `removeFunctionRef` に当たる除去が公開されていない（停止の経路で登録表から外れるだけ） |

### 2.1 ProviderSelection の公開 API（新規）

置き場所は `crates/fraktor-actor/src/kernel/actor/provider_selection.rs` と `provider_selection/provider_selection_value.rs`、`provider_selection_test.rs`（単体テスト）とする。原典の定義場所（`ActorSystem.scala`）に合わせ、公開面は `kernel::actor` の下に置く。

**入口は1つにし、原典の `ProviderSelection.apply`（`ActorSystem.scala:136-143`）をそのまま写す（レビュー H3 で修正）。** 以前の設計の `from_key`／`from_fqcn` の2関数への分割は取り下げる。2つに分けると、Local の FQCN の扱いと、FQCN をキー側へ渡した場合の結果が原典と変わってしまうためである。

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
  // 原典の apply の写し。対応は次のとおりである。
  //   "local"                                  -> Local
  //   "remote" | REMOTE_PROVIDER_FQCN          -> Remote
  //   "cluster" | CLUSTER_PROVIDER_FQCN        -> Cluster
  //   それ以外すべて                           -> Custom(入力そのまま)
  // 注意：LOCAL_PROVIDER_FQCN も「それ以外」であり、Custom になる（Local にはならない）。
  pub fn from_setting(value: &str) -> Self;
  pub fn fqcn(&self) -> &str;           // Local/Remote/Cluster は定数、Custom は保持した文字列
  pub fn has_cluster(&self) -> bool;    // Cluster のときだけ真
}
```

原典と違える点:

- 原典の `Settings.ProviderClass` は、設定文字列を Typesafe Config で読んだ結果である。移植は設定の読み取りを持たないので、`from_setting` がその対応（設定文字列 → 選択値 → fqcn）を担う。これはケースの除外ではなく忠実さの判断として扱う（E1、§5）。
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
| `crates/fraktor-actor/src/kernel/actor/actor_system_value.rs`（変更） | `with_setup` の文書（「setup は起動と実行に影響しない」）を更新する。`DispatchSettings` は実行に影響する Setup であるためである（レビュー L5） |
| `crates/fraktor-actor/src/kernel/executor/executor_value.rs`（変更） | spawn 時に定義を解決して actor ごとに保持する。stash の容量検査、`unstash_all` のあふれの dead letters 送り、DeadLetter の発行（§2.3） |
| `crates/fraktor-actor/src/kernel/actor/dead_letter.rs`（#191 で新設。本 issue では新設しない） | #191 の `DeadLetter` の値の型をそのまま使う（§2.3。再レビュー2の中2で実物に合わせて修正） |
| `crates/fraktor-actor/tests/actor_with_bounded_stash_spec.rs` | 原典5ケース |
| `crates/fraktor-actor/tests/bounded_stash_property_test.rs` | PBT／モデル照合（§3。原典外で台帳の件数に含めない） |
| `crates/fraktor-actor/specs/bounded_stash.qnt` | Quint（§4） |

容量の型は、0を型で表せなくする形にする（レビュー M1）。原典の `Stash.scala:185` は `capacity <= 0` を**無制限**として扱い（既定の `stash-capacity` は -1）、mailbox の容量0は `LinkedBlockingDeque(0)` が例外を投げて作れない。したがって「容量0」を境界の意味として固定する以前の設計は取り下げる。

```rust
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CapacityLimit {
  Unlimited,                  // 原典の capacity <= 0 に対応する
  Limited(NonZeroUsize),      // 正の容量だけを表せる
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct MailboxDefinition {
  // None は「未指定」。解決ではキーごとに次の定義へフォールバックする（M9）。
  // Some(CapacityLimit::Unlimited) は「無制限と明示」であり、未指定とは区別する。
  pub mailbox_capacity: Option<CapacityLimit>,
  pub stash_capacity: Option<CapacityLimit>,
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
  AliasCycle { name: String },        // alias の循環
  AliasTargetUnknown { name: String },// alias の行き先が未定義
  AliasTooDeep { name: String },      // alias の段数が原典の上限20を超えた
}

impl ActorContext<'_> {
  pub fn try_stash(&mut self) -> Result<(), StashOverflowException>;
  // stash() は従来どおり。容量が設定された actor では、あふれのときに
  // StashOverflowException で panic する（原典の throw に対応する経路）。
}
```

設計の判断:

- **例外の受け方は `try_stash` で置き換える。** 原典の overflow 用 actor は `stash()` の例外を try-catch で捕らえて数え、21件目で返信して自ら停止する。Rust の `receive` は戻り値を持たないので、panic では actor 自身が捕らえられない（監督の再起動になる）。`try_stash() -> Result` を正の経路とし、ケース3・4の actor はこれを使う。既存の `stash()` は互換のため残し、容量違反では panic する（原典で例外を捕らえなかった場合に監督へ伝わる動きに対応する）。既存の無制限の actor では振る舞いは変わらない。
- **同じメッセージの二重 stash は、容量違反とは別の契約違反として panic のままにする（レビュー L4）。** 既存の契約（最新の stash と同じ配送の二重 stash は panic）があり、`try_stash` でも `Err` にはしない。容量の数えが契約違反と混ざってずれることを防ぐためである。
- **容量の解決は、stash 容量だけがキーごとの3段フォールバックをする（レビュー M9、再レビュー N1）。** mailbox 容量は、原典の `getMailboxType` のとおり、mailbox が指定されていればその定義だけ（未指定は既定＝無制限）で決まり、dispatcher へはフォールバックしない。mailbox が指定されていない場合だけ dispatcher の mailbox 定義を使う。
- **キーごとのフォールバックは stash 容量だけにする（レビュー M9、再レビュー N1）。** 原典の `Mailboxes.stashCapacityFromConfig`（`Mailboxes.scala:314-321`）は、mailbox の設定、dispatcher の設定、既定の mailbox の設定の順に、キーごとに未指定を次の段で埋める。ただし移植では、キーごとのフォールバックは stash 容量にだけ適用する。mailbox 容量は、mailbox が指定されていればその定義の値（未指定なら無制限）で決まり、dispatcher の定義へはフォールバックしない（直前の項目と同じ）。`Option` の `None`（未指定）と `Some(Unlimited)`（無制限の明示）を区別するのはこのためである。どちらも指定が無ければ無制限（現在の振る舞い）。
- **定義に無い名前はエラーにしない（レビュー H2 で修正）。** `DispatchSettings` に載っていない dispatcher／mailbox 名は、現在の振る舞い（名前の記録だけ、無制限）のままにする。green の `SupervisorMiscSpec`（`crates/fraktor-actor/tests/supervisor_misc_spec.rs`）が、定義の無い `"pinned-dispatcher"` と `"test-dispatcher"` を使っており、未定義を spawn のエラーにするとこのノードが壊れるためである。エラーにするのは、alias の循環、alias の行き先が未定義の場合、alias の段数が原典の上限20（`MaxDispatcherAliasDepth`）を超えた場合だけである。
- **解決エラーの出し方は、既存の署名を変えない panic とする（H2）。** `ActorSystem::actor_of` は `ActorRef` を返し、`ActorContext::actor_of` は `Result<_, InvalidActorNameException>` を返すため、どちらの署名にも新しいエラー型を載せる余地が無い。spawn 時の解決で `DispatchResolutionError` が出たら、その値をメッセージに含めて panic する（原典の `ConfigurationException` に対応する経路）。解決そのものは `resolve_dispatcher`／`resolve_mailbox` が `Result` で返すので、単体テストでは panic を介さずに確かめられる。別の `try_` 系 API は足さない。
- **alias は名前の解決だけに効く。** alias の段数は原典どおり上限20とし、解決は定義に届くまで辿る。解決の結果は spawn の時点で固定され、後から定義を変えても既存の actor には効かない。同じ名前は定義と別名のどちらか一方にだけ存在する。定義を登録すると同名の別名が消え、別名を登録すると同名の定義が消える（後から登録した方が残る）。
- **容量が効くのは `unstash_all` の時点だけにする（レビュー M10 で修正）。** 以前の設計の「通常の `tell` のあふれを即座に dead letters へ送る」は取り下げる。どのケースもその振る舞いを観測せず、配送ごとの件数えはホットパスに入り、PR #236（actor ごとの mailbox）の数え方とも置き換わる見込みが高いためである。移植の実行系は shard ごとに1本のキューを持ち actor ごとの mailbox が無いので、`unstash_all` の前置で、その時点の滞留件数を走査して数え、容量を超える分を dead letters へ送る（ケース1・2・5が確かめるのはこの経路だけである）。通常の `tell` は今の無制限のままにし、その旨をゴールの文書と PR 本文に書く。bounded mailbox の push timeout（500ms 待って失敗）は、同期的な kernel の実行系には対応が無いので移さない（E2）。
- **テストの組み立ては `TestKit::new(ActorSystem::with_setup(..))` とする（レビュー L5）。** `FraktorSpec` には setup を受ける生成の口が無いため、bounded stash のテストは `DispatchSettings` を載せた `ActorSystem::with_setup` を `TestKit` へ渡して組む。testkit に新しい口は足さない。あわせて `with_setup` の文書の「setup は起動と実行に影響しない」という記述を、`DispatchSettings` が実行に影響する Setup であることに合わせて更新する。

no_std: 定義の表は `BTreeMap`（alloc）、容量の検査は core のみで書ける。時刻（push timeout）には触れないので Clock の port も要らない。新しい依存は足さない。

### 2.3 DeadLetter の観測手段（新規）

ケース1・2・5は `DeadLetter(message, sender, recipient)` を testActor が受けることを確かめる。移植では、dead letters へ送られたメッセージを値として発行し、テストが購読できるようにする。

```rust
// PartialEq / Eq は付けない（レビュー L1）。Message は PartialEq<Message> を
// 持たないため、付けるとコンパイルできない。前例の UnhandledMessage と同じく、
// Clone と読み取りのメソッドの形にする。
#[derive(Clone, Debug)]
pub struct DeadLetter {
  message: Message,     // 送られたメッセージそのもの
  sender: ActorRef,     // 元の送信者
  recipient: ActorRef,  // 宛先だった actor
}
impl DeadLetter {
  pub fn message(&self) -> &Message;
  pub fn sender(&self) -> ActorRef;
  pub fn recipient(&self) -> ActorRef;
}
```

テストは `expect_msg_type::<DeadLetter>()` で受けてから、読み取りのメソッドで中身を比べる（`expect_msg` は `Message: PartialEq<E>` を要求するため使わない）。

発行先は既存の event stream とする。原典も `system.eventStream.subscribe(testActor, classOf[DeadLetter])` で受ける形であり、対応が直接である。event stream の購読は統合テストの probe から使える（`EventStream::subscribe(Some(ActorRef), EventClass::of::<T>())` があり、executor には `publish_unhandled` の前例がある。レビュー L6）。したがって「使えるかを最初に確かめ、使えない場合は testkit に受け口を足す」という分岐は置かない。testkit には何も足さない。

DeadLetter の発行は、#191 の型と経路を使う（再レビュー2の中2で実物に合わせて書き直した）。発行は executor の `publish_dead_letter` が dead letters のハンドルへ包んで送り、ハンドル側の `route_at_dead_letters` が publish する。発行する場所は、停止時の始末（stash の始末とキューの始末）、`unstash_all` のあふれ、閉じた FunctionRef の境界の4か所である。閉じた FunctionRef の境界だけは、原典の `deadLetterMessageHandler` のとおり包まずに生のメッセージを dead letters のハンドルへ tell し、`route_at_dead_letters` が recipient＝deadLetters 自身を付ける（再レビュー2の中1）。`system.dead_letters()` への直接の tell は、`route_at_dead_letters` がそのまま処理する。

### 2.4 FunctionRef の公開 API（新規）

置き場所は `crates/fraktor-actor/src/kernel/actor/function_ref.rs` と `function_ref/function_ref_value.rs`、単体テスト `function_ref_value_test.rs` とする。既存の `ActorSystem::function_ref`（システム直下）は互換のため残し、内部の登録表を共有する。

**`FunctionRef` は `ActorRef` の newtype（id＋世代の値）だけにする（レビュー H1 で修正。方向は元帥の決定、2026-11-26）。** 以前の設計は `FunctionRef` に `WeakActorSystem`（`alloc::rc::Weak<RefCell<Executor>>`）を持たせていたが、この形は `!Send`／`!Sync` になり、`Message::new` の `M: Any + Send + Sync` を満たせないため、FunctionRef そのものをメッセージとして運ぶ原典のケース（`replyTo ! ref`、`DropForwarder(ref)`）が書けない。レビューは一時クレートの `cargo check` で E0277 を確認している。所有権の決定（`docs/decisions/ownership-model-arc-overhead.md` の「ref は実体へのポインタではなくハンドル（id＋世代）」「処理系への参照はその場で渡す」）にも反する。

```rust
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct FunctionRef {
  inner: ActorRef, // 宛先としてのハンドル（id＋世代）だけを持つ。処理系への参照は持たない
}

impl FunctionRef {
  pub fn as_actor_ref(&self) -> ActorRef;
  pub fn path(&self) -> ActorPath;
  pub fn tell<M: Any + Send + Sync>(&self, message: M, sender: ActorRef);
}

// watch 系の操作は FunctionRef 自身には置かない。処理系への参照が要るため、
// ActorSystem（&mut self）と ActorContext（処理時に executor から貸し出される）に、
// &FunctionRef を受けるメソッドとして置く。
impl ActorSystem {
  pub fn function_ref_watch(&mut self, watcher: &FunctionRef, target: &ActorRef) -> ActorRef;
  pub fn function_ref_unwatch(&mut self, watcher: &FunctionRef, target: &ActorRef) -> ActorRef;
  pub fn function_ref_is_watching(&self, watcher: &FunctionRef, target: &ActorRef) -> bool;
}
impl ActorContext<'_> {
  pub fn function_ref_watch(&mut self, watcher: &FunctionRef, target: &ActorRef) -> ActorRef;
  pub fn function_ref_unwatch(&mut self, watcher: &FunctionRef, target: &ActorRef) -> ActorRef;
  pub fn function_ref_is_watching(&self, watcher: &FunctionRef, target: &ActorRef) -> bool;
}

impl ActorContext<'_> {
  pub fn add_function_ref<H>(&mut self, handler: H) -> FunctionRef
  where
    H: Fn(&ActorRef, Message) + Send + Sync + 'static; // FunctionBoundary の Handler = Arc<dyn Fn + Send + Sync> にそろえる（L2）
  pub fn remove_function_ref(&mut self, target: &FunctionRef);
}
```

この形を選ぶ理由と、所有権モデルへの影響:

- **PR #236（複数スレッドの実行器）の段1を前提にしない。** 中身が `Rc` のままか `Arc<spin::Mutex>` に変わるかは #236 の D1・D2 で決めている最中であり、以前の形では Send 性がその結果に依存した。newtype＋貸し出しメソッドの形は、#236 がどちらに決まっても変わらない。元帥の決定（2026-11-26）により、この「前提にしない形」を選び、「#236 の判断を待つ部分として分ける」案は採らない。#236 の判断待ちのせいで #192 全体が止まることもない。
- **所有権モデルと一致する。** ハンドルは値であり、処理系への参照は `ActorContext`／`ActorSystem` がその場で渡す。`FunctionRef` を保持しても循環所有は生まれない。Clone＋Eq＋Send＋Sync が成り立つので、メッセージとして運べる。
- **receive の中からの watch で借用 panic が起きない。** 以前の形では、`watch` が `RefCell` を `borrow_mut` し、処理中の借用とぶつかる恐れがあった。context 経由のメソッドは、executor が処理時に貸し出す経路で登録表へ届くため、その衝突が無い。

設計の判断:

- **返す型は通常の `ActorRef` ではなく専用の `FunctionRef` にする。** ケース3が要る「FunctionRef 自身が watch する」操作の受け手の型が要るためである。操作そのものは上のとおり context／system のメソッドに置く。宛先として使う箇所（probe の watch、メッセージの送信、等価の比較）は `as_actor_ref()` と `tell` でまかなう。
- **生成は親セルの下に置く。** `add_function_ref` は親 actor のパス配下に名前を生成して登録し、親の停止で一緒に終わる（ケース4・8）。名前は executor が一意に生成する（原典もセル内の一意な名前を生成する）。同じ親から複数作れる。
- **ハンドラは `Fn` にそろえる（レビュー L2）。** `FunctionBoundary` の `Handler = Arc<dyn Fn + Send + Sync>` と、既存の `ActorSystem::function_ref` の `Fn` に合わせる。`FnMut`／`Box<dyn FnMut>` にはしない（post にロックが要る形になるため）。
- **他人の FunctionRef の除去は、黙って偽を返すのではなく panic にする（レビュー M7）。** 原典（`Children.scala:98`）は `require(ref.path.parent eq self.path, ...)` で `IllegalArgumentException`（actor の失敗）になる。移植でも、`remove_function_ref` に他セルの FunctionRef を渡したら panic する（actor の失敗として監督へ伝わる経路）ことを、移植の忠実さの判断としてゴールの文書に書く。自セルの FunctionRef の除去は冪等であり、すでに除去済みの自セルの FunctionRef への再除去は何もしない。
- **除去された FunctionRef への送信は dead letters へ行く。その経路は boundary 側に作る（レビュー M6）。** 既存の扱いは「既存と同じ」ではない。`FunctionBoundary::close` の後の `post` は何もせず、dead letters の handler も何もしない。原典の `stop()` は宛先を `deadLetterMessageHandler` に差し替える。移植では、登録の除去のときに boundary の宛先を dead letters への経路に差し替え、`ActorRef::tell` の直接の post でも `DeadLetter` が発行されるようにする。実装の置き場所は boundary の差し替えであることをここに明記する。
- **除去後の宛先の差し替えは、ロックなしで実装する（再レビュー N3）。** `FunctionBoundary` の `handler` は差し替えられない `Arc` なので、post のたびに差し替えを読む形にはしない。すでにある `open` の原子フラグで分け、閉じた後は生成時に持たせた dead letters への経路（Send＋Sync の届き先。前例 `start_unhandled_message_forwarder` の `EventStreamBusWeak` の役割）を使う。
- **Terminated の sender の差し替えの見分けは登録表で行う（再レビュー N4）。** `enqueue_to` の boundary 経由の枝は別の shard の actor にも使われるため、枝では見分けず、watcher が function ref の登録表にあるかどうかで見分ける。
- **FunctionRef に届く `Terminated` の sender は、終わった宛先自身とする（レビュー M5）。** 原典（`ActorRef.scala:879-880`）は `Terminated(f)(existenceConfirmed = true, addressTerminated = false)` を sender＝終わった `f` として関数に渡し、テストも `Forwarded(Terminated(f)(true, false), f)` を期待する。既存の `enqueue_to` は sender＝受け手にしているので、この差し替えは **function ref の watcher への通知の経路だけ**に絞る（通常の actor への通知の sender は変えない）。
- **`is_watching` の意味は原典どおりにする。** watch 中の相手が終わると `Terminated` が FunctionRef へメッセージとして届くが、登録は自動では外れない。`function_ref_unwatch` を呼ぶか、FunctionRef 自身が止まるまで真のままである（ケース3が固定する、通常の actor と違う点。自分の停止での片付けは §3 の F3）。
- 既存の `ActorSystem::function_ref` の戻り値の型は変えない（公開 API の互換）。その宛先を watch する既存の単体テスト群にも触れない。

no_std: ハンドラは `Arc<dyn Fn>`（alloc）、登録表は alloc の表で書ける。`FunctionRef` 自体は `ActorRef`（alloc の `Arc` を持つ）の値であり、`Copy` は付けられない（`ActorRef` が `Copy` でないため E0204 になる。再レビュー N2）。`Clone` のままで Send＋Sync であり、メッセージとして運べる。新しい依存は足さない。

## 3. 守る性質（不変条件）と壊しうる入力

### ProviderSelection

| 番号 | 不変条件 |
|:---|:---|
| S1 | 対応の固定（原典の `apply` の写し）：`from_setting("local")` は `Local`、`from_setting("remote")` と `from_setting(REMOTE_PROVIDER_FQCN)` はどちらも `Remote`、`from_setting("cluster")` と `from_setting(CLUSTER_PROVIDER_FQCN)` はどちらも `Cluster` であり、各選択値の `fqcn()` は対応する定数を返す |
| S2 | `has_cluster()` が真になるのは `Cluster` のときだけである |
| S3 | キーと fqcn は別の名前空間ではない。入口は1つであり、Remote／Cluster の fqcn は組み込みの選択値になる。例外は Local の fqcn で、`from_setting(LOCAL_PROVIDER_FQCN)` は `Local` ではなく `Custom(LOCAL_PROVIDER_FQCN)` になる（原典の非対称性）。それ以外の文字列は `Custom` が1文字も変えずに保持し、`fqcn()` がそれを返す |
| S4 | 等価：選択値は値で比べる。`Custom("a") == Custom("a")` であり、`Custom("local") != Local` である |

壊しうる入力:

| 入力 | 何が起きうるか | 確かめているもの |
|:---|:---|:---|
| 空文字 `""` | `Custom("")` になる。原典も構築では検査しない | PBT、単体テスト |
| 大文字・空白の混入（`"LOCAL"`、`" local"`、`"local "`） | 組み込みキーとみなされると S1 が崩れる | PBT（キー候補に含める） |
| ワイルドカード的な文字列（`"cl*"`、`"*"`、ドットの連続 `"a..b"`、末尾のドット） | provider の選択はパターンマッチではないので、すべて `Custom` として保持する | PBT |
| 極端に長い文字列（10,000文字） | 切り詰めや複製の事故が無いこと | PBT（長さの候補に含める） |
| NUL や非 ASCII を含む文字列 | Rust の `String` は保持できる。そのまま `Custom` になる | PBT |

PBT は足す。`crates/fraktor-actor/tests/provider_selection_property_test.rs` に置き、固定 seed の SplitMix64（前例と同じく外部依存は足さない。`proptest` の採否は [#212](https://github.com/j5ik2o/fraktor-ng-rs/issues/212) で決める）で任意の文字列について S1〜S4 を確かめる。文字列の候補には、組み込みキー3つ、**3つの FQCN（Local の FQCN が `Custom` になる非対称性を含む）**、その大文字・空白違い、空文字、長い文字列、非 ASCII を混ぜる。以前の設計の S3（キーと fqcn は別の名前空間）を PBT で固定することはしない。違う意味論を固めてしまうためである。

### bounded stash

容量を `cap_s`（stash）、`cap_m`（mailbox）、stash の中身を古い順の列とする。

| 番号 | 不変条件 |
|:---|:---|
| B1 | stash の件数は常に対象の容量以下である。`try_stash` は、件数が容量未満のときだけ成功し、容量に達しているときは状態を変えずに `Err(StashOverflowException)` を返す。容量が `Unlimited`（原典の `capacity <= 0` に対応）なら常に成功する。容量0という入力は型（`NonZeroUsize`）で表せない |
| B2 | 順序：stash は FIFO である。`unstash_all` は古い順に mailbox の先頭へ戻す（既存の振る舞いを変えない） |
| B3 | `unstash_all` のあふれ（原典は `theStash.reverseIterator` で `enqueueFirst` する。レビュー M2 で順序を修正）：すでに滞留しているメッセージはそのまま残る。stash のうち**新しい側**の (cap_m − 滞留) 件が残る。落ちるのは古い側の分であり、`DeadLetter` の発行順はその中で**新しい被害者から先**である。原典のケース1では、戻す11件のうち最も古い `hello1` だけが落ち（発行は1件だけ）、残り10件は処理される。試作の確認：stash [0,1,2]、滞留 [3,4]、cap_m 3 では、滞留は [2,3,4] になり、dead は [1,0] の順に出る |
| B4 | 停止時の始末：actor が止まると、原典は postStop の unstashAll で bounded mailbox へ戻す。入りきらない古い k 件は、逆順（新しい被害者から先）に DeadLetter になり、残りは掃除の段で古い順に出る。dispatcher-1 の設定（stash 20、mailbox 10）で15件を stash して PoisonPill を送ると、原典の順序は hello5, 4, 3, 2, 1, hello6..15 になる。無制限の mailbox では、従来どおり stash の全件が元の送信者を保って古い順に dead letters へ行く（既存の振る舞い） |
| B5 | 設定の解決：**stash 容量だけ**が、mailbox の定義、dispatcher の定義、既定の定義の順に3段フォールバックする（M9、再レビュー N1 で範囲を修正）。**mailbox 容量は dispatcher へフォールバックしない**：mailbox が指定されていればその定義（容量が未指定なら既定＝無制限）を使い、mailbox が指定されていない場合だけ dispatcher の定義を使う。定義に無い名前はエラーではなく、無制限のままである（H2）。エラーになるのは alias の循環、alias の行き先の未定義、alias の段数が上限20を超えた場合だけであり、出し方は署名を変えない panic である。解決の結果は spawn の時点で固定され、後から定義を変えても既存の actor には効かない |
| B6 | 容量は actor ごとに数える。同じ dispatcher を使う別の actor の stash や滞留は、対象の容量を消費しない |

壊しうる入力:

| 入力 | 何が起きうるか | 確かめているもの |
|:---|:---|:---|
| stash 容量1／容量の境界 | 容量0は型で表せない（M1）。usize の入力を受ける境界では、0以下は原典どおり無制限として扱うことを単体テストで固定する。容量1では2件目が違反になり、境界の数え違い（`<` と `<=`）が出る | 単体テスト、PBT（容量の候補に Unlimited と Limited(1) を含める） |
| ちょうど容量まで／容量＋1 | 原典は20件成功・21件目が違反。off-by-one が出る | ケース3・4、PBT |
| mailbox 容量／ちょうど滞留と一致／1だけ不足 | mailbox 容量0は型で表せない（原典でも作れない）。`unstash_all` の落ちる件数と、落ちる集合（古い側）と発行順（新しい被害者から先） | 単体テスト、PBT、Quint（§4） |
| alias の循環（自己参照、2つの相互参照）、alias の行き先の未定義、段数21以上、空文字の名前 | 無限ループや黙った既定落ちが起きうる。定義に無い dispatcher 名そのものはエラーではない（H2） | 単体テスト（解決の単体テストに置く） |
| dispatcher と mailbox の両方を指定（片方のキーだけが定義されている場合を含む） | キーごとのフォールバックの取り違え（B5、M9） | 単体テスト |
| 同じメッセージの二重 stash | 既存の契約（最新の stash と同じ配送は panic）があり、容量検査と混ざると数えがずれる | 既存の `context_stash_test` の範囲。本 issue では変えない |
| PoisonPill と stash の始末の競合する順序 | 停止の始末で stash が二重に dead letters へ行く、または消える | ケース1・2（PoisonPill で止める流れ） |

PBT は足す。`bounded_stash_property_test.rs` で、固定 seed の操作列（stash、unstash_all、停止、容量 Unlimited／Limited(1)／Limited(2)／小さい値の組み合わせ）を `VecDeque` の参照モデルと照合し、B1〜B4 を毎手確かめる。参照モデルの dead の並びは B3／B4 の順序（落ちる集合は古い側、発行は新しい被害者から先）に合わせる。公開 API から stash の中身は直接読めないので、照合は「actor が受けたメッセージの列と dead letters へ行った列」の観測でする。容量の解決（B5）は決定的な関数なので、PBT ではなく単体テストで表にする。

### FunctionRef

| 番号 | 不変条件 |
|:---|:---|
| F1 | 転送：登録されている FunctionRef への送信は、ハンドラを1回だけ呼び、元の送信者を保つ。除去の後はハンドラを呼ばず、dead letters へ行く |
| F2 | watch 可能性：FunctionRef は通常の actor と同じく watch でき、除去されると watcher が `Terminated` をちょうど1回受ける。二重の除去では2回目は来ない |
| F3 | watch する側：FunctionRef が watch した相手が終わると、`Terminated` がハンドラへのメッセージとして届く（sender は終わった相手自身。M5）。登録は相手の終了では消えず、`function_ref_unwatch` か、**FunctionRef 自身の停止**で消える（原典の `FunctionRef.sendTerminated` は自身が止まるときに watching を空にする。レビュー M4 で例外を追記）。`unwatch` は冪等である。なお watching の集合は FunctionRef 専用に持ち、executor の watchers 表（通知のときに組を消す）とは分ける。watchers 表から引くと Terminated の直後に偽になり、ケース3・7が落ちるためである |
| F4 | 親の停止と連鎖：親 actor が止まると、その親が作ったすべての FunctionRef が終わり、それぞれの watcher が `Terminated` を受ける。ほかの親が作った FunctionRef は影響を受けない。連鎖もある。祖先が止まると、その子（生成した actor）が止まり、それでその FunctionRef が終わる（ケース8。`SupSuper` の停止 → `Super` の停止 → FunctionRef の終了） |
| F5 | 除去の範囲：`remove_function_ref` が外せるのは、自セルが作った FunctionRef だけである。他人の FunctionRef への除去は、原典の `require` に対応して panic する（actor の失敗。M7）。自セルの FunctionRef の除去は冪等である |
| F6 | 一意性：同じ親から作った2つの FunctionRef は別の宛先であり、片方の除去がもう片方の転送を止めない。パスの名前も衝突しない |

壊しうる入力:

| 入力 | 何が起きうるか | 確かめているもの |
|:---|:---|:---|
| 除去を2回呼ぶ／除去済みへの送信 | 二重の `Terminated`、panic、消えた宛先への配送 | 単体テスト、モデル照合 |
| ほかの親の FunctionRef の除去 | F5 の崩れ（他人の宛先が消える）、または黙って成功してしまうこと。原典どおり panic になること | 単体テスト |
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
- 状態：`stashed: List[int]`（メッセージの通し番号、古い順）、`queued: List[int]`（mailbox の滞留、古い順）、`dead: List[int]`（発行順）、`handled: List[int]`、設定は `pure val`（stash 容量と mailbox 容量の組。容量は `Unlimited` と小さい `Limited` 値のインスタンスを置き、0は置かない。M1）。
- 遷移：`tell`（mailbox の上限つきで滞留へ積む。通常配送が滞留している状態を作るために要る。M3）、`stashNext`、`unstashAll`、`stop`、`handleOne`（滞留の先頭を処理済みにする）。`init` と `step` を明記する。`step` は `any { init, tell, stashNext, unstashAll, stop, handleOne }` の形で、stop の後も1手以上進むことを CI（`scripts/quint-check-specs.sh` の `quint run`）が確かめられるようにする（token_bucket の前例）。
- 不変条件：`stashWithinCapacity`（B1）、`noLossNoDuplication`（すべての通し番号が、stash・滞留・dead・処理済みのどこか1か所にちょうど1回ずつ現れる）、`deadLettersAreOldestOnOverflow`（B3。あふれで dead へ行った集合は、その時点で最も古い分であり、dead の並びはその中で新しい被害者から先である）、まとめて `val invariant`。
- 負の対照：`bugNewestDropped`（あふれで新しい側を落とす。`deadLettersAreOldestOnOverflow` で落ちる）、`bugOffByOneCapacity`（容量＋1まで許す。`stashWithinCapacity` で落ちる）、`bugStopDropsStash`（停止で stash を捨てる。`noLossNoDuplication` で落ちる）。対照は名前が `Test` で終わる `run` にし、誤りが不変条件を崩すことを確かめる。
- モデルにしないもの：push timeout の時間（時刻はモデルに入れない。500ms の意味論は E2 で移さない提案）、dispatcher／mailbox の名前解決（決定的な関数であり単体テストの表で確かめる）。

### FunctionRef に書く理由と形

FunctionRef の登録表は、生成・除去・親の停止という3つの遷移で watch の対応が変わる状態機械であり、F2／F4 の「`Terminated` がちょうど1回」は列の性質である。除去と親の停止の片付けが二重に走る誤りは、固定ケースでは順序の組み合わせ次第で通ってしまう。

- 置き場所は `crates/fraktor-actor/specs/function_ref.qnt` の1ファイル、1モジュールとする。
- 状態：`registered: Set[int]`（FunctionRef の通し番号）、`parentOf: int -> int`（actor と FunctionRef の親子の木。祖先の停止の連鎖を探索するために置く。M8）、`deliveryWatchers: Set[(int, int)]`（配送の登録。通知のときに消える）、`watching: Set[(int, int)]`（FunctionRef の watching 集合。配送の登録とは別の変数であり、相手の終了では消えず、unwatch か自分の停止で消える。M4）、`terminatedSent: Set[(int, int)]`（届けた `Terminated` の組）。watcher は FunctionRef の番号と外部の probe を表す番兵の両方を含む。
- 遷移：`addRef(parent)`、`removeRef(ref)`、`stopParent(parent)`（子と、その子が作った FunctionRef へ連鎖する）、`stopRef(ref)`（FunctionRef 自身の停止。watching を空にする）、`watch(watcher, ref)`、`unwatch(watcher, ref)`。`init` と `step` は bounded stash と同じ形で明記する。
- 不変条件：`terminatedExactlyOnce`（同じ組の `Terminated` は1回まで）、`terminatedOnlyAfterEnd`（`Terminated` が届いた対象は登録から外れている）、`noForwardAfterRemoval`（登録外の番号へは転送が起きない。ゴーストの転送回数で表す）、`watchSurvivesUntilUnwatchOrSelfStop`（F3。相手が終わっても watching は残り、unwatch か自分の停止でだけ消える）、まとめて `val invariant`。
- 負の対照：`bugDoubleCleanup`（除去と親の停止で二重に通知する）、`bugAutoUnwatch`（相手の終了で watch 登録を自動で外す。F3 に反する）、`bugOrphanRefs`（親の停止で FunctionRef を残す）。
- モデルにしないもの：ハンドラの中身（転送は回数だけを数える）、直列化（E3）、パスの文字列。

### ProviderSelection に書かない理由

ProviderSelection は状態を持たない純粋な対応（文字列 → 選択値 → fqcn）であり、遷移が無い。守る性質 S1〜S4 はすべて1回の変換で決まり、PBT が任意の文字列で直接確かめる。Quint で書いても同じ照合を別の記法で繰り返すことになり、新しく落ちる誤りの種類が増えない（ImmutableIntMap に Quint を書かなかった #188 の判断と同じ）。

## 5. 移し方を違える点と、作らない提案（E1〜E3）

E1・E2 と E3 では扱いが違う。混同しないように分けて書く。

### E1・E2 はケースの除外ではない（元帥の決定、2026-11-26）

E1（ProviderSelection の Settings／HOCON の部分）と E2（bounded stash の HOCON の定義・FQCN による型生成）は、**`cases.excluded` には入れない**。移植の忠実さの判断として、第2段でゴールの文書（`docs/goals/issue-192-specs.md`）と PR 本文に書く。前例は、G0b の決定 E（HOCON の設定の体系をどこまで再現するかは未決であり、設定体系は作らない）と、#154 の D1（原典の設定キーを写した型付きの入力にし、HOCON のパーサーは作らない。DnsSettingsSpec は 10／10／10、excluded [] で green）である。台帳では、ProviderSelectionSpec は 4／4／4 で excluded []、ActorWithBoundedStashSpec は 5／5／5 で excluded [] として green にできる。

この扱いの条件は、**数えているケースがすべて、HOCON の代わりの型付きの入力で、原典と同じことを確かめていること**である。本設計のケースはこの条件を満たす（ProviderSelection は `from_setting`、bounded stash は `DispatchSettings` の値で、原典の設定と同じ対応・同じ容量を確かめる）。push timeout（500ms）のように**ケースに出てこない振る舞い**については、ゴールの文書と PR 本文に1行で書く。通常の tell のあふれは「移さない」で閉じず、既知のずれとして issue #246 で #234 の段で入れる（元帥の決定、2026-11-26。再レビュー2への対応）。今回の PR #238 の本文にも、E1・E2 の扱いを1行で整合させてある（本文1行目の `Refs #192` は変えていない）。

| 番号 | 対象 | 扱い | 理由 |
|:---|:---|:---|:---|
| E1 | ProviderSelectionSpec の全4ケースのうち、設定読み取りの部分 | 除外にしない。HOCON の設定文字列と Typesafe Config の `Settings.ProviderClass`、クラスローダ経由の解決は作らず、キーと FQCN を受ける型付きの入口 `from_setting`（原典の `apply` の写し）に置き換えて、ケース自体は移す。忠実さの判断としてゴールの文書と PR 本文に書く | fraktor-ng-rs には設定ファイルの仕組みが無く、provider の選択はプログラム的に与える設計である。fqcn は JVM のクラス名であり、Rust では識別子の文字列として保持する以上の意味を持たない |
| E2 | ActorWithBoundedStashSpec の設定方法と、ケースに出ない振る舞い | 除外にしない。HOCON の文字列による dispatcher／mailbox の定義、クラス名（FQCN）による mailbox 型の生成（`Bounded10`／`Bounded100`）は作らず、定義は Rust の値（`DispatchSettings`）で与える。push timeout（500ms）と、通常の tell のあふれの扱い（M10）はケースに出てこない振る舞いであり、その旨をゴールの文書と PR 本文に1行で書く。とくに通常の tell のあふれ（原典では push の時間切れのあと DeadLetter）は未移植の既知のずれであり、「移さない」では閉じず、issue #246 で #234 の段で入れる（元帥の決定、2026-11-26） | 設定ファイルとリフレクションによる型生成は JVM 固有である。timeout は同期的な kernel の実行系に対応が無く、ケースの観測（あふれの行き先と順序）には現れない |

### E3 は除外として承認済み（オーナーの承認、2026-11-26）

| 番号 | 対象 | 扱い | 理由 |
|:---|:---|:---|:---|
| E3 | FunctionRefSpec のケース9「not be found」 | **除外として承認済み。** ケースの除外なのでオーナーの承認が要り、オーナー本人（j5ik2o）が元帥の項目426で「今は除外として承認する」と回答した（2026-11-26）。台帳の FunctionRefSpec ノードに、承認者と承認日を添えて除外1件として記録する | 原典は、登録されていない FunctionRef をパスから解決しても FunctionRef に戻らないことを、Java の直列化（serialize-messages = on）の往復で確かめている。fraktor には宛先のパスからの解決もメッセージの直列化も無い。言語の補助を確かめるケースではないので not_built には当たらず、ノード単位でもないので do_not_port でもない。以前の設計の理由文「登録されていない FunctionRef は型で表せない」は誤りであり取り下げる（除去した FunctionRef がまさに登録されていない FunctionRef である）。宛先のパスからの解決か、メッセージの直列化が入ったら見直す |

自作だと思うファイルについて：本 issue で新しく作るファイルは、すべて Pekko の原典を参照する移植であり、自作と判断するファイルは無い。第2段で判断が変わるファイルが出たら、パスと理由を PR 本文と報告に書く。

## 6. 台帳の更新方針（第2段）

本書の段では台帳を触らない。第2段では、対象ノード3件を次のように更新する。

| ノード id | total／ported／passing | rust.test_file | status |
|:---|:---|:---|:---|
| `actor-tests/src/test/scala/org/apache/pekko/actor/ProviderSelectionSpec.scala` | 4／4／4、excluded []（E1 は除外ではなく忠実さの判断） | `crates/fraktor-actor/tests/provider_selection_spec.rs` | green |
| `actor-tests/src/test/scala/org/apache/pekko/actor/ActorWithBoundedStashSpec.scala` | 5／5／5、excluded []（E2 は除外ではなく忠実さの判断） | `crates/fraktor-actor/tests/actor_with_bounded_stash_spec.rs` | green |
| `actor-tests/src/test/scala/org/apache/pekko/actor/FunctionRefSpec.scala` | 9／8／8、除外1件（`cases.excluded` に `approved_by: j5ik2o` と `approved_at: 2026-11-26` を記録する） | `crates/fraktor-actor/tests/function_ref_spec.rs` | green（除外1件を含む） |

あわせて `rust.crate` に `fraktor-actor`、`verified_against: v2.0.0` を書き、`docs/baseline/v2.0.0-undocumented-nodes.tsv` を作り直して対象ノードの記載をそろえる（#188 の前例と同じ）。内部 API を呼ぶ単体テスト（ProviderSelection の対応、DispatchSettings の解決、FunctionRef の除去の範囲）は `rust.cases_outside_test_file` に記録する。ゴールの完了判定は `ledger/tools/rollup.py --goal <ゴールID>` の終了コードで行う。

なお、台帳の FunctionRefSpec の `needs.premise_flags` が [] である点は、原典が Java の直列化を前提にしていることとの不整合であり、抽出の取りこぼしとして台帳側の別件である（レビュー L7）。本 issue の設計では扱わない。

## 7. 第2段ですること（本書の範囲の外）

- red（失敗するテスト）→ green の順に実装する。先に3つの spec ファイルを書き、コンパイルの失敗かテストの失敗で red を確認してから本体を足す。ノードは1件ずつ処理する（ProviderSelection → bounded stash → FunctionRef の順を推奨する。前の2件の基盤（DeadLetter の発行）が FunctionRef の観測にも効くため）。
- §5 の E1・E2 は、第2段でゴールの文書（`docs/goals/issue-192-specs.md`）と PR 本文に忠実さの判断として書く。E3 は、オーナーの承認（元帥の項目426、2026-11-26）を受けて、台帳へ除外1件として記録する。
- CI と同じ検査を流す（`cargo fmt --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`、ledger/tools の Python テスト、Quint は `scripts/quint-check-specs.sh` をそのまま流し、`bounded_stash.qnt` と `function_ref.qnt` が対象に入っていることを確かめる）。
- 既存の公開 API との互換を壊さない。変更は §2 に挙げた追加と、`stash()` の文書の更新、executor の内部（容量の保持と DeadLetter の発行）だけである。
- **PR 本文の1行目の扱い**：実装を終えてドラフトから外すときは、PR 本文の1行目を `Refs #192` から `Closes #192` に変える（マージで issue が閉じるように）。ただし、E3 の除外が判断待ちの間は、承認が出るまで `Refs #192` のままにする（E1・E2 は除外ではないので、この保留の理由にはならない）。
- **ライセンスヘッダー**：新しく作る `crates/**/*.rs` には、すべて Apache-2.0 のヘッダーを付ける。Pekko の移植では `python3 tools/license-headers/license_headers.py --apply --kind pekko --source <原典のパス> <ファイル>` を使い、原典を参照していないファイルは `--source` を省く。`--kind pekko` で付けるファイルには、`Mirrors org.apache.pekko.…` か移植のマーカーを1行入れる。MIT を含む式は書かない。push の前に `python3 tools/license-headers/license_headers.py --check --manifest docs/design/issue-206-file-origin.tsv` を流し、0 で終わることを確かめる（PR #215 で入った検査。手順の正は Mac mini の `~/.orca/bots/fraktor-ng-rs/skills/fraktor-migration-intake/SKILL.md` の「ライセンスヘッダーと第三者帰属」の節）。
- main を取り込むときは、PR #215 のライセンスヘッダーで競合が出たら両方を残す形で解消する。

## 8. レビュー回答表（2026-11-26）

独立レビュー（`pr238-design-review.md`、判定「要修正」、高3・中11・低7の全21件）と、元帥の決定（2026-11-26）への対応である。直したものは20件、採らなかったものは1件（L7。理由を書く）。

| 番号 | 指摘 | 対応 | 理由・直した場所 |
|:---|:---|:---|:---|
| H1 | FunctionRef が `WeakActorSystem` を持つと `!Send`／`!Sync` になりメッセージとして運べない | **直した** | `FunctionRef` を `ActorRef` の newtype（id＋世代の値）だけにし、`watch`／`unwatch`／`is_watching` は `ActorSystem` と `ActorContext` に `&FunctionRef` を受けるメソッドとして置く形にした（§2.4）。PR #236 の段1を前提にしない形を選んだ。理由は、中身の `Rc`／`Arc` の決定に Send 性を依存させないためであり、所有権の決定（ハンドルは値、処理系への参照はその場で渡す）とも一致する。方向の確定は元帥の決定（2026-11-26）による。「判断を待つ部分として分ける」案は採らず、#192 は #236 の判断待ちで止まらない |
| H2 | 未定義の dispatcher 名を spawn エラーにすると green の SupervisorMiscSpec が壊れる | **直した** | 定義に無い名前は現在の「名前の記録だけ（無制限）」のままにし、エラーは alias の循環・行き先の未定義・段数上限超過だけにした。エラーの出し方は既存の署名を変えない panic（原典の `ConfigurationException` に対応）と明記した（§2.2 の B5 と設計の判断） |
| H3 | ProviderSelection の対応が原典の `apply` と違う | **直した** | 入口を `from_setting` の1つにし、原典 `ActorSystem.scala:136-143` の `apply` を写す形にした。Local の FQCN は `Custom` になる非対称性も明記し、S1 と S3 を書き直し、PBT の候補に3つの FQCN を入れた（§2.1、§3） |
| M1 | stash の容量0の意味が原典と逆 | **直した** | 容量を `CapacityLimit::{Unlimited, Limited(NonZeroUsize)}` にし、0を型で表せなくした。usize の境界では0以下を原典どおり無制限として扱う（§2.2、§3 の B1 と壊しうる入力、§4 の Quint の候補） |
| M2 | B3 の文が矛盾し、dead letters の順序が原典と違う | **直した** | B3 と B4 に、原典の `reverseIterator`＋`enqueueFirst` の順序（残るのは新しい側、落ちる集合は古い側、発行は新しい被害者から先、停止時は入りきらない古い k 件が逆順に先に出る）を明記し、PBT の参照モデルと Quint の dead の並びを合わせた（§3） |
| M3 | Quint の bounded_stash に tell の遷移が無い | **直した** | mailbox の上限つきの `tell` を足し、`init` と `step` を明記した（§4） |
| M4 | `is_watching` の出どころと、自分が止まったときの扱い | **直した** | F3 を「相手の終了では消えず、unwatch か自分の停止で消える」に直し、watching 集合を FunctionRef 専用に持ち、Quint でも配送の登録と watching 集合を別の変数にした（§2.4、§3、§4） |
| M5 | FunctionRef に届く Terminated の sender | **直した** | sender＝終わった宛先自身、`existenceConfirmed = true`、`addressTerminated = false` とし、差し替えの範囲を function ref の watcher への通知の経路だけに絞ると明記した（§2.4、§3 の F3） |
| M6 | 除去された FunctionRef への送信は「既存と同じ」ではない | **直した** | 既存の post は何もしないことを認め、実装の置き場所を「登録の除去のときに boundary の宛先を dead letters への経路に差し替える」と明記した（§2.4） |
| M7 | 他人の FunctionRef の除去 | **直した** | 黙って偽を返す形を取り下げ、原典の `require` に対応して panic（actor の失敗）にする形にし、忠実さの判断としてゴールの文書に書くとした（§2.4、§3 の F5） |
| M8 | ケース8の読み違いと、親の停止の連鎖 | **直した** | ケース8を「SupSuper の停止 → Super の停止 → FunctionRef の終了」の連鎖として書き直し、F4 に連鎖を入れ、Quint に親子の木を足した（§1.3、§3、§4） |
| M9 | 定義の解決は原典ではキーごとのフォールバック | **直した** | `Option` で「未指定」と「無制限の明示」を区別し、キーごとに mailbox → dispatcher → 既定の順で解決する形にした。alias の段数の上限20も入れた（§2.2、§3 の B5） |
| M10 | 通常の tell のあふれを即座に dead letters へ送る新しい振る舞い | **直した（推奨を採用）** | 通常の tell のあふれの扱いを取り下げ、容量の効き目を `unstash_all` の時点の走査に絞った。通常の tell は無制限のままであることをゴールの文書と PR 本文に書く（§2.2、§5 の E2） |
| M11 | E1・E2 の承認を `cases.excluded` に記録する形は台帳に合わない | **直した** | E1・E2 を除外にせず、忠実さの判断としてゴールの文書と PR 本文に書く形にした（§5、§6）。扱いの確定は元帥の決定（2026-11-26）による |
| L1 | `DeadLetter` に `PartialEq, Eq` を付けるとコンパイルできない | **直した** | derive を外し、Clone と読み取りのメソッドの形にし、テストは `expect_msg_type::<DeadLetter>()` で受けてから比べるとした（§2.3） |
| L2 | ハンドラの `FnMut`／`Box<dyn FnMut>` が既存と合わない | **直した** | ハンドラを `Fn`（`Arc<dyn Fn + Send + Sync>`）にそろえた（§2.4） |
| L3 | §2 の表の「watch する API が無い」は半分だけ正しい | **直した** | `ActorSystem::watch` はどの `ActorRef` でも watcher にできることを書き、足りないものを unwatch と is_watching と context 側の操作に絞った（§2 の表） |
| L4 | 二重 stash の扱いが書かれていない | **直した** | 既存の契約どおり panic のまま（`try_stash` でも `Err` にしない）と明記した（§2.2） |
| L5 | `with_setup` の文書の更新と、テストの組み立て方 | **直した** | `with_setup` の文書を更新対象に入れ、テストは `TestKit::new(ActorSystem::with_setup(..))` で組み、testkit に口を足さないとした（§2.2） |
| L6 | event stream の購読は確かめるまでもなく使える | **直した** | 確かめる分岐と testkit 側の受け口の案を消し、DeadLetter の発行範囲を executor の3か所だけ（`system.dead_letters()` への直接の tell は含めない）と明記した（§2.3） |
| L7 | 台帳の FunctionRefSpec の `needs.premise_flags` の取りこぼし | **採らない（別件）** | 抽出の取りこぼしであり台帳側の別件であるため、本 issue の設計では扱わない。§6 に別件である旨だけを書いた |
| E1 | ProviderSelection の Settings／HOCON の部分 | **除外にしない（元帥の決定）** | 忠実さの判断として第2段でゴールの文書と PR 本文に書く。条件（数えているケースがすべて型付きの入力で原典と同じことを確かめていること）を §5 に明記し、PR #238 の本文にも1行で整合させた。台帳は 4／4／4、excluded [] |
| E2 | bounded stash の HOCON・FQCN の型生成と push の時間切れ | **除外にしない（元帥の決定）** | E1 と同じ形で記録する。ケースに出てこない振る舞い（push timeout、通常の tell のあふれ）を移さないことは、ゴールの文書と PR 本文に1行で書く。台帳は 5／5／5、excluded [] |
| E3 | FunctionRef の「not be found」 | **判断待ち** | ケースの除外なのでオーナーの承認が要る。上司（元帥）がオーナーに伺い中であり、それまで台帳には書かず、PR の中では「判断待ち」と書く。理由の文は直した（「登録されていない FunctionRef は型で表せない」は誤りであり取り下げ、正しい理由を §5 に書いた）。残りの8件は進めてよい |

## 9. 再レビュー回答（2026-11-26、条件つき承認）

再レビュー（`pr238-design-rereview.md`、判定「条件つき承認」）の新しい指摘 N1〜N4 への対応である。N1 と N2 は実装の最初のコミットの前に本書へ反映した。N3 と N4 は実装の段での注意として §2.4 に注記した。

| 番号 | 指摘 | 対応 |
|:---|:---|:---|
| N1（中） | mailbox 容量は原典では dispatcher へフォールバックしない。3段フォールバックは stash 容量だけ | **直した**。B5 と §2.2 を分け、stash 容量だけが mailbox → dispatcher → 既定の3段フォールバックをし、mailbox 容量は mailbox 指定があればその定義（未指定は既定＝無制限）、無ければ dispatcher の定義で決まるとした。単体テストの表に「dispatcher が容量10の bounded、mailbox が stash 20 だけを定義する」入力を足す |
| N2（低） | `FunctionRef` の `#[derive(Copy)]` は E0204 でコンパイルできない | **直した**。derive から `Copy` を外し、「core の値である」という文も直した（§2.4） |
| N3（低） | 除去した FunctionRef の宛先の差し替えの実装（ホットパスにロックを入れない） | **実装の注意として注記した**（§2.4）。`open` の原子フラグと生成時に持たせた dead letters 経路で分け、ロックは入れない |
| N4（低） | Terminated の sender を差し替える範囲の見分け方 | **実装の注意として注記した**（§2.4）。`enqueue_to` の枝ではなく、watcher が function ref の登録表にあるかで見分ける |

## 10. 再々レビュー回答（2026-11-26、条件つき承認）

再々レビュー（`pr238-design-rereview2.md`、判定「条件つき承認」）への対応である。中1と中2は直した。低は、低2を含めてゴールの文書への記録まで行った（元帥の決定、2026-11-26）。

| 番号 | 指摘 | 対応 |
|:---|:---|:---|
| 中1 | 止まった FunctionRef 宛ての DeadLetter の recipient が原典と違う | **直した**。閉じた FunctionRef の境界は、包まずに生のメッセージを dead letters のハンドルへ渡し、`route_at_dead_letters` が recipient＝deadLetters 自身を付ける形にした。回帰テスト2件の期待も dead letters 自身に直し、モデルテストの DeadLetter の分類も合わせた（§2.3） |
| 中2 | DeadLetter を #191 に寄せたことが設計に反映されていない | **直した（文書だけ）**。§2.2 の表と §2.3 を #191 の型と経路に書き直し、本節を足した。#191 の設計の D1 にも、`add_function_ref` の FunctionRef は dead letter になる旨の注記を足した。ゴールの文書の検証表の function_ref の件数も現在の 11 passing に直した |
| 低1 | `alias_mailbox` は設計にも原典にも無い | **直した**。`alias_mailbox` と mailbox の alias の表を消した。原典の別名は dispatcher だけであり、mailbox は `getConfig(id)` で直接読む |
| 低2 | M7 と H2 の忠実さの判断がゴールの文書に無い | **書いた**。ゴールの文書の「既知のずれ」の節に、M7（他のセルの FunctionRef の除去は原典の `require` どおり panic）と、H2（定義に無い名前は無制限のまま。既知のずれとして issue #247 で直す）を書いた |
| 低3 | 二重通知の重複を除くと M5 の sender が失われる | **直した**。両方の表にいる watcher は function ref の表の通知（sender＝終わった相手）を残す形にし、回帰テストに sender の確認を足した。Quint にも両方の表に同じ watcher がいる状態（`watchByRefDelivery`）と、その run を足した |
| 低4 | 除去のあとの「他のセルの FunctionRef」の判定が登録表に頼っている | **直した**。`remove_function_ref` の判定を、原典の `require` どおりパスの親との比較にした。除去のあとでも他のセルからの除去は panic する |
| 低5 | `actor_capacities` が DispatchSettings の無いシステムでも全 spawn で足され、`try_stash` のたびに走査される | **一部直した**。容量が両方とも無制限の actor には項目を作らない形にし、無制限のシステムでは表が育たないようにした。容量をセルの側へ移すことは、#234 で mailbox が actor ごとになる段でそちらへ移す |
| 判断材料1（E2） | bounded mailbox への通常の tell のあふれ | **#192 では直さない（元帥の決定、2026-11-26）**。既知のずれとしてゴールの文書に書き、issue #246 で #234 の段で入れる。「移さない」で閉じる扱いにはしない。台帳の `cases.excluded` には入れない |
| 判断材料2（H2） | 定義に無い dispatcher／mailbox の名前は無制限のまま | **#192 では直さない（元帥の決定、2026-11-26）**。一時的な互換の判断として既知のずれに明記したうえで残し、issue #247 で直す。台帳の `cases.excluded` には入れない |
