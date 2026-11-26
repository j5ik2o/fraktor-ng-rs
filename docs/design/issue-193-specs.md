# 設計：#193 actor の PropsCreationSpec、ActorSystemDispatcherSpec、ExtensionSpec

[Issue #193](https://github.com/j5ik2o/fraktor-ng-rs/issues/193) の設計の記録である。対象は Pekko v2.0.0（`9fbd2283b1cf701c90a1223bf621942c191db61f`）の actor-tests にある actor の3つの Spec で、依存する issue は無いとされている。この文書は第1段（設計だけ）であり、実装と台帳の更新は、設計レビューが通ってから第2段で行う。

結論を先に書く。**数え直したケースは合計20件であり、issue の目安21件と1件違う。** PropsCreationSpec は目安6件に対して実数5件である。内訳は、PropsCreationSpec の5件と ExtensionSpec の8件をこの issue で進め、ActorSystemDispatcherSpec の7件はすべて、複数スレッドの実行器の設計 [Issue #234](https://github.com/j5ik2o/fraktor-ng-rs/issues/234)（設計は [PR #236](https://github.com/j5ik2o/fraktor-ng-rs/pull/236)）の段5の後（1件は段6の後）に分ける。PropsCreationSpec は本体の新規追加が無く、既存の `Props::new` で全件を移す。ExtensionSpec は、拡張を型で登録する仕組みが fraktor-ng-rs に無いため、本体の新規追加が要る。公開 API の形は §2 に書く。ExtensionSpec のうち、設定に書いたクラス名で拡張を読み込む2件は、作らない区分（`not_built`）として §7 に提案する。作らない区分は人間が決めるものであり、この設計は提案までである。

進め方の規約は `.takt/facets/knowledge/fraktor-porting-rules.md`、設計文書の前例は [PR #205](https://github.com/j5ik2o/fraktor-ng-rs/pull/205) の [issue-177-frequency-sketch.md](issue-177-frequency-sketch.md) と、直近の [issue-191-specs.md](issue-191-specs.md) に従う。ノード1件＝テストファイル1件とし、ファイル冒頭に出典（Pekko v2.0.0 のファイルパス）をコメントで残す。設定を写す部分は、HOCON の文字列ではなく型付きの入力にする（#154 の決定 D1 と、G0b の決定 E の前例、および #234 の設計 §1.3 の「dispatcher とメールボックスの設定はコードで組み立てて渡す」）。速さより Pekko の意味論を優先する指針（`docs/decisions/pekko-semantics-first.md`、[PR #236](https://github.com/j5ik2o/fraktor-ng-rs/pull/236) で追加中）に従い、JVM と Scala に固有の振る舞いだけを見るケースは、理由を添えて区分の提案としてだけ書く。

## 1. 何を移植するか

### 件数の数え直し

原典の `" in {"` の数を数え直した。PropsCreationSpec だけが目安と違う。

| Spec | 目安（issue／台帳） | 数え直し |
|:---|---:|---:|
| `actor-tests/src/test/scala/org/apache/pekko/actor/PropsCreationSpec.scala`（77行） | 6／6 | **5** |
| `actor-tests/src/test/scala/org/apache/pekko/actor/ActorSystemDispatcherSpec.scala`（178行） | 7／7 | **7** |
| `actor-tests/src/test/scala/org/apache/pekko/actor/ExtensionSpec.scala`（177行） | 8／8 | **8** |
| 合計 | 21 | **20** |

PropsCreationSpec の5件は、「Props」の下の4件と「Props Java API」の下の1件である。目安の6件目に当たるケースは原典に無い。台帳の `test_cases_approx` は近似値であり、第2段で台帳を更新するときは `cases.total: 5` として記録する。

ActorSystemDispatcherSpec は、ファイル名と台帳の id が単数形（`ActorSystemDispatcherSpec`）であるのに対して、原典の中のクラス名は複数形の `ActorSystemDispatchersSpec` である。台帳の id と移す先のファイル名は、台帳に合わせて単数形のままにする。

台帳の対象ノードは次の3件で、どれも `status: unported`、`triage.kind: pending` である。

- `actor-tests/src/test/scala/org/apache/pekko/actor/PropsCreationSpec.scala`
- `actor-tests/src/test/scala/org/apache/pekko/actor/ActorSystemDispatcherSpec.scala`
- `actor-tests/src/test/scala/org/apache/pekko/actor/ExtensionSpec.scala`

### 対応表：PropsCreationSpec（5件）

移す先は `crates/fraktor-actor/tests/props_creation_spec.rs` である。原典のアクターは、引数を1つ取る `OneParamActor`（引数の型は中身を持たない `A`）と、引数を2つ取る `TwoParamActor`（`A` と `B`）であり、どちらの `receive` も `Actor.emptyBehavior` である。移植では、引数をクロージャが捕まえる形にする。引数の型 `A` と `B` は、テストの中に置く中身を持たない構造体で写す。

| # | 原典のケース | Rust のテスト名 | 移す／作らない |
|:---:|:---|:---|:---|
| 1 | work with creator | `props_work_with_creator` | 移す |
| 2 | work with classOf | `props_work_with_class_of` | 移す（形を変える。下の段落） |
| 3 | work with classOf + 1 `null` param | `props_work_with_class_of_and_one_null_param` | 移す（形を変える） |
| 4 | work with classOf + 2 `null` params | `props_work_with_class_of_and_two_null_params` | 移す（形を変える） |
| 5 | work with create(class, creator)（Java API） | `props_work_with_create_class_and_creator` | 移す（形を変える） |

形を変える点は3つある。第1に、原典の `Props(classOf[OneParamActor], null)` の形（実行時のクラスと、コンストラクタの引数の列で組み立てる形）は、JVM のリフレクションでコンストラクタを探して呼ぶ仕組みであり、Rust には無い。移植は、引数を捕まえたクロージャを `Props::new` へ渡す形にする。確かめる性質は「引数を取るアクターを、引数を与えて作れること」であり、リフレクションそのものではない、というのがこの設計の立場である。この立場を妥当と見るかは §8 の判断材料1に置く。認められない場合の代案も §8 に書く。第2に、原典の `null` は、Rust では `Option::None` を捕まえる形で写す。引数の有無が型で決まるため、「引数の型と数が合わない」という失敗はコンパイル時に出る。第3に、原典が前提にする設定（`pekko.actor.serialize-creators = on` などの、作り手の直列化の検証）は移さない。直列化そのものは fraktor-ng-rs に無く、ケースの検査にも現れないためである。

ケース2とケース3は、原典で中身が同じである。どちらも `Props(classOf[OneParamActor], null)` を `system.actorOf` へ渡すだけであり、題名だけが違う。移植でも、題名に対応するテストを2件に分けて書き、中身が同じであることをテストのコメントに残す。件数を水増ししないため、この重なりは台帳の記録にもそのまま書く。

ケース5の原典は、Java API の `Props.create(classOf[Actor], creator)` であり、`creator` は `pekko.japi.function.Creator[Actor]` を実装したオブジェクトである。移植は、`create()` に当たるメソッドを持つ構造体をテストの中に置き、そのメソッドの戻り値を `Props::new` のクロージャから返す形にする。Java のインターフェースそのものは作らない。

どのケースも、原典の検査は `system.actorOf(p)` が例外を出さずに終わることだけである。移植は検査を弱めず、生成が終わることに加えて、作ったアクターがシステムの中で動いていること（`is_actor_terminated` が false であること）までを確かめる形にしない。原典に無い検査を足すと、生成の成否以外の理由で落ちるテストになるためである。生成の確認は原典どおり `actor_of` の戻り値が得られることだけにする。

### 対応表：ActorSystemDispatcherSpec（7件）

移す先は `crates/fraktor-actor/tests/actor_system_dispatcher_spec.rs` である。ただし7件はすべて [Issue #234](https://github.com/j5ik2o/fraktor-ng-rs/issues/234) の段の後になる。この issue の第2段では移さず、台帳のノードは `unported` のまま残す。理由と段の対応は次のとおりである。

| # | 原典のケース | 前提 | 理由 |
|:---:|:---|:---|:---|
| 1 | work with a passed in ExecutionContext | **#234 の段5の後** | システムへ渡した実行の場所（原典の `ExecutionContext`）の上でアクターが動き、その `execute` が呼ばれたことを、記録する実行の場所で確かめる。fraktor-ng-rs には実行の場所の port がまだ無く、アクターはテストのスレッドが `run_until_idle` で回す。段5で std のスレッドプールと実行の口が入ってからでないと、「渡した実行の場所が使われた」ことを観測できない |
| 2 | not use passed in ExecutionContext if executor is configured | **#234 の段5の後** | 既定の dispatcher の executor が設定で指定されているときは、渡した実行の場所を使わないことを確かめる。設定は型付きの入力で写す（§2）。前提はケース1と同じである |
| 3 | provide a single place to override the internal dispatcher | **#234 の段5の後** | 内部 dispatcher の id を既定の dispatcher へ差し替える設定の置き場所が1つあることを確かめる。dispatcher の登録表と、内部 dispatcher の id は段5で入る。あわせて、利用者ガーディアンがどの dispatcher で動くかを外から読む方法が要る。fraktor-ng-rs にはガーディアンの概念がまだ無い（§2） |
| 4 | provide internal execution context instance through BootstrapSetup | **#234 の段6の後** | 原典は `BootstrapSetup` で渡した実行の場所が、内部 dispatcher 経由のアクター（`withDispatcher(InternalDispatcherId)`）の実行に使われることを確かめる。段5の登録表に加えて、段6の「dispatcher を、クロージャを実行する場所として使う口」と、setup から実行の場所を dispatcher へ配る配線が要る |
| 5 | use an internal dispatcher for the guardian by default | **#234 の段5の後** | 既定では利用者ガーディアンが内部 dispatcher（`pekko.actor.internal-dispatcher`）で動くことを確かめる。ケース3と同じく、登録表とガーディアンの観測方法が要る |
| 6 | use the default dispatcher by a user provided user guardian | **#234 の段5の後** | 利用者ガーディアンを利用者が与えた場合、そのガーディアンは既定の dispatcher で動くことを確かめる。登録表に加えて、利用者ガーディアンを与えてシステムを作る構築子が要る。今の `ActorSystem` にはその構築子は無い |
| 7 | provide a good error on an dispatcher alias loop in the config | **#234 の段5の後** | dispatcher の別名が循環しているとき、`Dispatchers.lookup` が `ConfigurationException` を出すことを確かめる。別名の解決は登録表の仕事であり、#234 の設計 §3.2 が `Dispatchers`（id から dispatcher への表、別名、既定の id）を段5で入れる形を決めている。この issue で登録表だけを先に作ると、#234 の設計と二重になる |

#234 の設計 §8 の対応表には、この Spec は載っていない。上の段の対応は、#234 の設計 §6 の段の表（段5が「std のスレッドプール、`Dispatchers` の登録表」、段6が「dispatcher の寿命、クロージャを実行する口」）から、この設計が引いたものである。レビューで段の対応が違うと判断された場合は、この表を直す。

この切り分けの結果、この issue の第2段が終っても台帳の ActorSystemDispatcherSpec のノードは green にならず、issue の完了条件（対象ノードがすべて green）は #234 の段5と段6が終るまで満たせない。第2段の進め方は §10 に書く。

### 対応表：ExtensionSpec（8件）

移す先は `crates/fraktor-actor/tests/extension_spec.rs` である。原典の拡張は3つある。`TestExtension` はシステムを保持するだけの拡張、`FailingTestExtension` は生成の途中で名前 `uniqueName` のアクターを作ってから例外を投げる拡張、`InstanceCountingExtension` は作られた回数を数える拡張である。移植でも、テストの中に同じ3つの拡張と、それぞれの識別子を置く。

| # | 原典のケース | Rust のテスト名 | 移す／作らない |
|:---:|:---|:---|:---|
| 1 | support extensions | `supports_extensions` | 移す（形を変える。下の段落） |
| 2 | handle extensions that fail to initialize | `handles_extensions_that_fail_to_initialize` | 移す（形を変える） |
| 3 | fail the actor system if an extension listed in pekko.extensions fails to start | `fails_the_actor_system_if_a_listed_extension_fails_to_start` | 移す（形を変える） |
| 4 | log an error if an extension listed in pekko.extensions cannot be loaded | — | **作らない区分を提案する**（§7） |
| 5 | allow for auto-loading of library-extensions from reference.conf | `allows_auto_loading_of_library_extensions` | 移す（形を変える） |
| 6 | not create duplicate instances when auto-loading of library-extensions | `does_not_create_duplicate_instances_when_auto_loading_library_extensions` | 移す |
| 7 | fail the actor system if a library-extension fails to start | `fails_the_actor_system_if_a_library_extension_fails_to_start` | 移す（形を変える） |
| 8 | fail the actor system if a library-extension cannot be loaded | — | **作らない区分を提案する**（§7） |

形を変える点は4つある。第1に、原典は設定文字列（`pekko.extensions = ["org.apache.pekko.actor.TestExtension"]`）にクラス名を書いて読み込むが、移植は型付きの入力にする。システムの生成時に、開始時に読み込む拡張の識別子の列を値で渡す（§2 の `ExtensionStartup`）。クラス名による動的な読み込みは Rust に無いため、「読み込めない」状態そのものが型の上で作れない。これがケース4とケース8を作らない区分として提案する理由である。第2に、原典の `reference.conf`（ライブラリが自分の設定ファイルで申し出る仕組み）は Rust に無い。ケース5とケース6が守る性質は「ライブラリ側の拡張が、システムの開始時に、利用者が個別に頼まなくても作られること」と「同じ拡張が複数回申し出されても作られるのは1回だけであること」であり、移植はライブラリ側の列を、通常の列とは別の列として型付きで渡す形でこの性質を写す。第3に、ケース3とケース7の原典は、システムの生成が例外で失敗することを確かめる。移植の `ActorSystem::new` は失敗しない形（戻り値が `Self`）であるため、開始時の拡張を受ける構築子は `Result` を返す形で新しく足す（§2）。第4に、ケース2の原典は、投げられた例外を2回とも `intercept` で受け、同じ例外であることを確かめる。移植の失敗は値（§2 の `ExtensionError`）であり、2回とも同じ原因の名前と文面の `Err` が返ることで写す。例外の同一性そのものは写さない。

ケース2の副作用（失敗する拡張が、例外を出す前に名前 `uniqueName` のアクターを作ること）は、移植でも同じ順序で書く。生成が1回しか走らないことは、作られた回数を数える仕組みを失敗する拡張にも置き、2回の取得のあとで回数が1であることを確かめる形で、原典より弱めずに写す。原典は回数を数えていないため、この検査は移植で足す検査である。足す理由は、失敗の保存（2回目に生成をやり直さないこと）がこのケースの性質そのものであり、回数がその直接の観測になるためである。

ケース1の原典は、設定で申し出された拡張が開始時に読み込まれていること（`hasExtension` が true）、取得した拡張が同じシステムを保持すること、取得の経路が2通り（識別子の `apply` と `system.extension`）とも同じ拡張を返すことを確かめる。移植の取得の経路は `ActorSystem::extension` の1通りとし、識別子側の入り口は作らない。経路が2通りあることは Scala と Java の API の都合であり、性質は「同じ識別子では同じ実体が返ること」であるためである。

## 2. 対応する本体がすでにあるか

| 原典の対象 | fraktor-ng-rs の現状 | この issue で足すもの |
|:---|:---|:---|
| `org.apache.pekko.actor.Props`（作り手、`classOf` と引数、Java の `Creator`） | `crates/fraktor-actor/src/kernel/actor/props.rs` の `Props` がすでにある。`Props::new<F, A>(factory: F) -> Self`（`F: Fn() -> A + Send + Sync`、`A: Actor`）、`Props::empty()`、`with_dispatcher(name)`、`dispatcher()` を持つ。型の文書に書かれているとおり、移植は作り手だけを保持し、クラスと引数の列は保持しない。単体テスト `props_test.rs` もすでにある | 無し |
| `org.apache.pekko.actor.ActorSystem` の dispatcher（`Dispatchers.lookup`、別名、内部 dispatcher、ガーディアン） | 無い。`ActorSystem`（`crates/fraktor-actor/src/kernel/actor_system/actor_system_value.rs`）の型の文書は「Dispatchers, guardians, and extensions belong to later goals」と明記する。`Props::with_dispatcher` は名前を記録するだけで、解釈はしない。実行は `Executor`（`crates/fraktor-actor/src/kernel/executor/executor_value.rs`）が1つのシャードの列を持ち、テストのスレッドが回す形であり、dispatcher の登録表も、id の解決も、ガーディアンという概念も無い | 無し（#234 の段5と段6の担当であり、この issue では足さない） |
| `org.apache.pekko.actor.Extension`、`ExtensionId`、`ExtensionIdProvider`、システムの拡張の登録表（`registerExtension`、`hasExtension`） | 無い。`crates/` の全体に `Extension` を名前に含む型は、testkit の `TestKitSettings`（文書では `TestKitExtension` の設定と説明されるが、型としてはシステムから取得する形ではなく `TestKitSettings::from_system(&system)` で直接作る）と、event の購読まわりだけであり、拡張の登録表は無い。型で引く表の前例は `ActorSystemSetup`（`crates/fraktor-actor/src/kernel/setup/actor_system_setup.rs`）であり、具体型の `TypeId` をキーにして setup を保持する | 拡張の登録表と公開 API を新しく足す（下の節） |
| testkit（`PekkoSpec`、`TestProbe`、`EventFilter`、暗黙の送信者） | `crates/fraktor-testkit` に `FraktorSpec`、`TestKit`、`TestProbe`、`EventFilter` がすでにある。台帳の `needs` は、PropsCreationSpec が `extends-pekko-spec` と `pekko-spec`、ActorSystemDispatcherSpec がそれらに加えて `test-actor`、`implicit-sender`、`expect-msg`、`expect-no-message`、`test-probe`、ExtensionSpec が `test-kit` と `event-filter` である。ExtensionSpec の移植6件は `EventFilter` を使わない。原典が `EventFilter` を使うのは、作らない区分を提案するケース4だけである | 無し |

### 新しく足す拡張の API

置き場所は、リポジトリの1型1ファイルの並びに合わせて、`crates/fraktor-actor/src/kernel/extension.rs`（モジュールの入口）と、その下の3ファイルとする。

- `extension/extension_trait.rs`：`Extension` トレイト
- `extension/extension_id_trait.rs`：`ExtensionId` トレイト
- `extension/extension_error.rs`：`ExtensionError`
- `extension/extension_startup.rs`：`ExtensionStartup`

公開 API の形は次のとおりである。

```rust
/// Mirrors `org.apache.pekko.actor.Extension`. A marker trait: an extension
/// is any value the system keeps once per extension id.
pub trait Extension: Any + Send + Sync {}

/// Mirrors `org.apache.pekko.actor.ExtensionId`. The concrete type of the
/// id is the key: a system holds at most one extension per id type, as
/// `ActorSystemSetup` holds at most one setup per concrete type.
pub trait ExtensionId: Any + Send + Sync {
  type Ext: Extension;
  /// Mirrors `ExtensionId.createExtension`. Returning `Err` is the port of
  /// throwing from `createExtension`: the system keeps the error and
  /// returns it again for every later request for the same id.
  fn create_extension(&self, system: &ActorSystem) -> Result<Self::Ext, ExtensionError>;
}

/// The failure of creating one extension, kept as a value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtensionError {
  cause_name: &'static str,
  message: String,
}

impl ExtensionError {
  pub fn new(cause_name: &'static str, message: impl Into<String>) -> Self;
  pub fn cause_name(&self) -> &'static str;
  pub fn message(&self) -> &str;
}
```

`ActorSystem` へ足すメソッドと構築子は次のとおりである。既存の構築子（`new`、`with_setup`、`with_failure_catcher`）の署名と振る舞いは変えない。

```rust
impl ActorSystem {
  /// Creates a running actor system, then creates every extension in
  /// `startup`. Mirrors the `pekko.extensions` and
  /// `pekko.library-extensions` lists, given as typed values instead of
  /// class names. When creating one extension fails, the system is
  /// terminated and the error is returned; no system is handed out.
  pub fn with_extensions(
    name: &str,
    boundary: Arc<dyn ShardBoundary>,
    startup: ExtensionStartup,
  ) -> Result<Self, ExtensionError>;

  /// Returns the extension for `id`, creating it on the first request.
  /// Mirrors `ActorSystem.extension(id)` and `ExtensionId.apply(system)`.
  /// A later request returns the same `Arc`, or the same error when the
  /// first creation failed.
  pub fn extension<E: ExtensionId>(&self, id: &E) -> Result<Arc<E::Ext>, ExtensionError>;

  /// Returns whether the extension for `id` has been created successfully.
  /// Mirrors `ActorSystem.hasExtension(id)`. A creation that failed does
  /// not count.
  pub fn has_extension<E: ExtensionId>(&self, id: &E) -> bool;
}

/// The extensions a system creates at startup, as typed values.
#[derive(Default)]
pub struct ExtensionStartup { /* erased entries, each with its kind */ }

impl ExtensionStartup {
  pub fn new() -> Self;
  /// Adds an id to the list that mirrors `pekko.extensions`.
  pub fn extension<E: ExtensionId>(self, id: E) -> Self;
  /// Adds an id to the list that mirrors `pekko.library-extensions`.
  pub fn library_extension<E: ExtensionId>(self, id: E) -> Self;
}
```

設計の要点は5つある。

1. **キーは識別子の具体型の `TypeId` である。** Pekko は識別子のオブジェクトの同一性で引く。Rust には単一の実体を表すオブジェクトの慣習が無いため、`ActorSystemSetup` の前例どおり具体型で引く。識別子は中身を持たない構造体として定義し、型が違えば別の拡張になる。
2. **登録表は `ActorSystem` が持ち、内側は `Rc<RefCell<BTreeMap<TypeId, ExtensionEntry>>>` とする。** `ExtensionEntry` は `Created(Arc<dyn Extension>)` と `Failed(ExtensionError)` の2通りを持つ非公開の enum である。取得が `&self` であるため、内側の可変性が要る。`Executor` を `Rc<RefCell<..>>` で持つ今の `ActorSystem` の形に合わせる。[Issue #234](https://github.com/j5ik2o/fraktor-ng-rs/issues/234) の段1（`Rc` から `Arc` への置き換え）では、この登録表も置き換えの対象に加える必要がある。その旨は段1の issue へ引き継ぐ事項として §9 に書く。
3. **生成は、登録表の借用を外してから呼ぶ。** 失敗する拡張は生成の途中で `actor_of` を呼び、別の拡張を取得する生成もありうる。借用を持ったまま生成を呼ぶと、再入した取得が `RefCell` の二重借用で panic する。そこで、取得の手順は「表を見て、無ければ『生成中』の印を入れて借用を外し、生成を呼び、結果を表へ戻す」順にする。生成中の同じ識別子への再入は、生成の循環として `ExtensionError` を返す（§4）。
4. **開始時の読み込みは、システムの値を組み立てたあとに行う。** 拡張の生成は `&ActorSystem` を受け取り、`actor_of` を呼べる必要があるためである。`with_extensions` は、既存の生成の手順でシステムを作り、次に `ExtensionStartup` の列を順に取得し、失敗したらシステムを `terminate` してから `Err` を返す。列の中で同じ識別子が重なった場合は、列の種類（通常かライブラリか）を問わず、作るのは1回だけである（ケース6の性質）。
5. **失敗は値で保存し、生成はやり直さない。** 生成が `Err` を返した識別子については、その `ExtensionError` を保存し、後の取得はすべて同じ値を返す。原典が同じ例外を2回出すことの写しである。生成の中で panic した場合は、失敗として保存しない。panic は巻き戻しとしてそのまま伝える（§4）。

### dispatcher の型付きの入力（#234 の段の後で使う形）

この issue では足さないが、ActorSystemDispatcherSpec のケース2とケース3が使う設定の写し方を、ここに決めておく。原典は HOCON の文字列（`pekko.actor.default-dispatcher.executor = "fork-join-executor"`、`pekko.actor.internal-dispatcher = pekko.actor.default-dispatcher`）で書く。#234 の設計 §3.2 が決めた `Dispatchers` と `DispatcherConfig`（id から dispatcher への表、別名、既定の id。設定はコードで組む）が入ったあとは、この2件は文字列ではなく、`DispatcherConfig` の値と、内部 dispatcher の id を指す値で写す。新しい設定の型をこの issue で先に作ることはしない。二重の設計になるためである。

### no_std

新しく足す型はすべて kernel に置き、`core` と `alloc` だけで書ける。使うのは `BTreeMap`、`Arc`、`Rc`、`RefCell`、`TypeId` であり、どれも今の kernel がすでに使っている。時刻、スケジューリング、IO には触れないため、port は要らない。`ExtensionStartup` の列の保持も `alloc` の `Vec` で足りる。

## 3. 守る性質（不変条件）

| 番号 | 性質 | 主に確かめるケース |
|:---|:---|:---|
| I1 | 同じシステムと同じ識別子について、拡張の生成は最大で1回だけ走る。成功した場合は、後の取得はすべて同じ `Arc` を返す（`Arc::ptr_eq` で等しい） | ExtensionSpec のケース1、ケース6 |
| I2 | 生成が失敗した識別子については、生成はやり直されず、後の取得はすべて原因の名前と文面が等しい `Err` を返す | ExtensionSpec のケース2 |
| I3 | 開始時の列に挙がった拡張は、システムが返る前に作られている。開始時の生成が失敗した場合は、システムは返らず、失敗したシステムは終了済みである | ExtensionSpec のケース1、ケース3、ケース5、ケース7 |
| I4 | 開始時の列で同じ識別子が何回現れても、作られるのは1回だけである。列の種類は問わない | ExtensionSpec のケース6 |
| I5 | `has_extension` が true を返すのは、その識別子の拡張が成功して作られた場合に限る。未取得の場合と、生成が失敗した場合とは false である | ExtensionSpec のケース1 |
| I6 | 拡張の生成は、生成の途中でシステムを使う（アクターを作る、別の拡張を取得する）ことができる。借用の保持で panic しない | ExtensionSpec のケース2 |
| I7 | `Props` の作り手は再利用できる。同じ `Props` からアクターを2つ作ると、作り手は2回呼ばれ、別々のアクターが動く。監督による再起動も、同じ作り手から新しい実体を作る | PropsCreationSpec の全件（性質そのものは既存の `props_test.rs` と再起動のテストが担う。この issue のテストは生成の経路ごとに1回ずつ確かめる） |
| I8 | 引数を取るアクターは、与えた引数を保持したまま作られる。引数が `None` の場合も作れる | PropsCreationSpec のケース1からケース4 |

dispatcher についての性質（別名の解決が循環で止まること、別名の連鎖の深さの上限、ガーディアンの dispatcher の既定値）は、#234 の設計が担う。この設計では、ActorSystemDispatcherSpec のケース7が使う上限の値だけを原典から記録する。原典の `Dispatchers.scala` の `MaxDispatcherAliasDepth` は20であり、深さが20を超えると `ConfigurationException` になる。

## 4. 壊しうる入力

| 入力 | 何が起きうるか | どう扱うか |
|:---|:---|:---|
| 空文字列の識別子名、極端に長い文字列 | 拡張の識別子は型であり、名前の文字列を持たないため、当てはまらない。dispatcher の id は文字列であり、空の id と極端に長い id がありうる | 拡張については対象外と明記する。dispatcher の id は #234 の段の担当であり、この設計では扱わない |
| ワイルドカードの連続（dispatcher の id やアクター名のパターン） | この3つの Spec には、パターン照合で id や名前を引くケースが無い | 対象外と明記する |
| 拡張の生成が、同じ識別子の拡張を再入して取得する | 生成の循環である。素朴な実装では `RefCell` の二重借用で panic し、利用者のコードの欠陥と仕組みの欠陥が区別できない | §2 の「生成中」の印で検出し、`ExtensionError`（原因の名前は生成の循環であることを示す値）として返す。テストは足さない。原典に無い入力であり、設計の防御としてだけ書く |
| 拡張の生成が panic する | Pekko では生成の例外が、そのまま開始の失敗と失敗の保存になる。Rust の panic を失敗として保存するには、kernel が panic を捕まえる必要があり、kernel は捕まえる仕組みを持たない（捕まえるのは adaptor の `UnwindCatcher` で、監督の経路だけが使う） | 生成の失敗は `Err` の戻り値で表すことを API の契約とし、panic は保存せずに伝える。原典との差として、API の文書に書く |
| 拡張の生成が、名前の重なるアクターを作ってから失敗する（ケース2の `uniqueName`） | 生成をやり直すと、名前の重なりで2回目の失敗の原因が変わる | I2 のとおり生成はやり直さない。ケース2のテストは、作られた回数が1であることまで確かめる |
| 作り手（`Props` の factory）が panic するアクター生成 | 既存の振る舞いである。`FraktorSpec` のシステムでは捕まえる仕組みが監督の判断に変え、素のシステムでは panic が伝わる | この issue では変えない。PropsCreationSpec のケースは、panic しない作り手だけを使う |
| 境界の時間の値 | この3つの Spec には、時間の値そのものを検査するケースが無い。ActorSystemDispatcherSpec の `expectMsg(1.second, ...)` は待ちの上限であり、性質ではない | 対象外と明記する。移植の待ちは、testkit の既存の待ち方（手で回す実行器と `WakeSignal` の前例）に従い、実時間の上限だけを使う |
| 開始時の列が空、列に同じ識別子が多数回現れる | 空の場合は今の `ActorSystem::new` と同じになる。多数回の重なりは I4 で覆う | ケース6が3回の重なりを確かめる。それ以上の回数は §5 の性質で覆う |

## 5. PBT を足すか

ExtensionSpec の登録表にだけ足す。対象は I1、I2、I4 であり、置き場所は `crates/fraktor-actor/tests/extension_property_test.rs` とする。原典外のテストなので、台帳の件数には含めない。外部の依存は足さず、既存の性質テスト（`frequency_sketch_property_test.rs` など）と同じく固定 seed の SplitMix64 で入力を作る。

| 番号 | 性質 | 確かめる不変条件 |
|:---|:---|:---|
| P1 | 成功する識別子と失敗する識別子を混ぜた取得の列を任意の順序と回数で実行しても、識別子ごとの生成の回数は最大で1であり、成功した識別子は常に対象の `Arc` と `ptr_eq` で等しく、失敗した識別子は常に等しい `Err` を返す | I1、I2 |
| P2 | 開始時の列に、成功する識別子と失敗しない識別子を任意の重なり回数で並べても、成功する識別子が作られる回数は1であり、`has_extension` は列に現れた成功する識別子についてだけ true になる（開始時に失敗する識別子を含む列は、P2 の対象から外す。その列はシステム自体が返らないためであり、失敗の列はケース3とケース7が固定の手順で確かめる） | I3、I4、I5 |

入力は、成功する識別子2つと失敗する識別子1つの、最大で32回までの取得の列とする。有効な試行の下限は設けない。どの入力の列でも、生成の回数の主張は全件に成り立つためである。

PropsCreationSpec には PBT を足さない。作り手は入力の空間を持たず、ケースは生成の経路の列挙が本体であるためである。ActorSystemDispatcherSpec については、この issue では足さない。別名の連鎖（循環、深さの上限、別名の指す先が無い場合）は性質として書く価値があるが、登録表が #234 の段5で入るときに、その段の設計で扱う。

## 6. Quint を書くかどうか

この issue では Quint を書かない。理由は3つある。

第1に、ActorSystemDispatcherSpec が関わる状態遷移（メールボックスの予約、suspend と再開、dispatcher の寿命）は、#234 の設計がすでに Quint で担っている。段2までの仕様は `crates/fraktor-actor/specs/mailbox_scheduling.qnt`（[PR #236](https://github.com/j5ik2o/fraktor-ng-rs/pull/236) で追加中）であり、dispatcher の寿命の仕様 `dispatcher_lifecycle.qnt` は段6で足す計画である。この issue で別の仕様を書くと、同じ状態遷移を2つの仕様が持つことになる。

第2に、ExtensionSpec の登録表は、状態遷移を持つものの、単一スレッドで動く3つの状態（未生成、生成中、生成済み、失敗を保存済み）だけであり、並行する実行の重なりも、時間で変わる状態も無い。確かめる性質は I1 から I6 で尽きており、固定のテストと §5 の性質で覆える。行き詰まりや取りこぼしのような、順序の組み合わせでしか出ない欠陥の形が無い。

第3に、PropsCreationSpec には状態遷移が無い。作り手の呼び出しは1回の生成であり、stash のような保持の仕組みも使わない。なお、この3つの Spec のケースはどれも stash を使わない。stash の容量とあふれたときの振る舞い（`StashOverflowException`）は、既存の実装でも未移植であり（`crates/fraktor-actor/src/kernel/actor/actor_context.rs` の `stash` の文書）、この issue の対象にはならない。

## 7. 台帳の見込みと、区分の提案

第2段で台帳を更新する。ゴールの識別子は、#177 と #185 の命名に倣って `G0issue193` とする。承認の欄（`approved_by` と `approved_at`）は、移植の規約により人間とレビューの決定を写す欄であり、この設計文書の作成者が自分で書くものではない。

| ノード | 総数 | 移植 | 作らない区分の提案 | 前提の後 | 承認後の状態の見込み |
|:---|---:|---:|---:|---:|:---|
| `actor-tests/src/test/scala/org/apache/pekko/actor/PropsCreationSpec.scala` | 5 | 5 | 0 | 0 | `green` |
| `actor-tests/src/test/scala/org/apache/pekko/actor/ActorSystemDispatcherSpec.scala` | 7 | 0 | 0 | 7（#234 の段5の後が6件、段6の後が1件） | `unported` のまま（#234 の段の後、別の issue で扱う） |
| `actor-tests/src/test/scala/org/apache/pekko/actor/ExtensionSpec.scala` | 8 | 6 | 2 | 0 | `green`（作らない区分の提案が承認された場合） |

台帳の `needs` の見込みも書いておく。PropsCreationSpec の移植5件は `FraktorSpec` を使うため、`needs.testkit` は原典どおり `extends-pekko-spec` と `pekko-spec` のままにする。ExtensionSpec の移植6件は `FraktorSpec` を使わず `ActorSystem` を直接使うため、`needs.testkit` は空にし、`event-filter` も外す。ActorSystemDispatcherSpec は、この issue では台帳を変えない。

作らない区分の提案は次の2件である。要素の形（`name`、`reason`、`alternative`、承認の欄）は `docs/decisions/ledger-schema-draft.md` の定義に従う。

| ケース | 理由 | 代替例 |
|:---|:---|:---|
| ExtensionSpec「log an error if an extension listed in pekko.extensions cannot be loaded」 | 設定に書いた完全修飾クラス名をクラスローダで読み込む仕組みそのものの検査である。読み込めないクラス名を書いたとき、システムは開始を続け、エラーのログを1件出すことを確かめる。移植の設定は型付きの入力であり、存在しない拡張の識別子を書くことはコンパイル時にできない。読み込みの失敗という状態が型の上で作れないため、等価なテストは書けない。ログを出す経路（event stream への `Error` の発行）自体は fraktor-ng-rs にあるが、このケースが確かめるのは経路ではなく、名前で読み込めない場合の扱いである | 存在しない拡張を指すコードはコンパイルが通らないこと自体が代替である。型付きの列に書けるのは、実在する識別子の型だけである |
| ExtensionSpec「fail the actor system if a library-extension cannot be loaded」 | ケース4と同じ仕組みの、ライブラリ側の列での検査である。読み込めないクラス名をライブラリ側の列に書いたとき、システムの生成が `RuntimeException` で失敗することを確かめる。ケース4と同じく、失敗の状態が型の上で作れない | ケース4と同じである。実在しない識別子は列に書けない |

2件とも、作らない区分として提案する。理由は、検証の対象が JVM のクラスローダという言語と実行時のための補助であり、その振る舞い（存在しないものを指せないこと）を Rust の言語機能が満たすためであり、台帳の作らない区分の条件に当たるためである。開始時に読み込む列の扱いそのもの（失敗したらシステムが失敗すること、エラーを出すこと）のうち、失敗の部分はケース3とケース7が移す。ケース4の「エラーのログを出して開始を続ける」部分に対応する振る舞いは、型付きの入力では要らなくなる。読み込めないものが列に入らないためである。

PropsCreationSpec と ExtensionSpec の2ノードが green になると、`docs/baseline/v2.0.0-undocumented-nodes.tsv` は2行増える見込みである。ActorSystemDispatcherSpec の行は増えない。

## 8. レビューで決めてほしい点

判断材料は次の4点である。

1. **PropsCreationSpec のケース2からケース4を、クロージャが引数を捕まえる形で移す立場でよいか。** この設計の立場は、ケースが守る性質は「引数を取るアクターを引数を与えて作れること」であり、`classOf` とリフレクションは JVM の手段である、というものである。認められない場合は、ケース2からケース4は作らない区分の提案に変える。その場合、ノードは移植2件と作らない区分3件の混在になり、§9 に書く台帳のツールの確認が要る。ケース5も同じ扱いにするかは、この判断に合わせて決めてほしい。
2. **ExtensionSpec のケース4とケース8の作らない区分の提案を認めるか。** 認めない場合の代案は、開始時の列に「読み込めない拡張」を表す値を入れられる形にすることである。たとえば、名前の文字列と生成の関数を組にした登録の口を別に足す案がある。ただしその案は、型付きの入力にするという #154 の決定 D1 と #234 の設計の方針に反するうえ、利用者が実際に使うことのない失敗の形を公開 API に残すことになる。この設計はその案を採らない。
3. **ActorSystemDispatcherSpec の7件をすべて #234 の段の後に分ける切り分けでよいか。** 分ける場合、この issue は PropsCreationSpec と ExtensionSpec の2ノードで第2段を終え、ActorSystemDispatcherSpec のノードは `unported` のまま残る。issue の完了条件は、この issue の中では満たせない。完了の扱い（この issue を開いたままにするか、dispatcher の分を別の issue へ分けるか）は、人間が決める事項として残す。段の対応（段5の後が6件、段6の後がケース4の1件）についても、この表でよいかを確認してほしい。
4. **新しく足す拡張の API の形でよいか。** 争点は3つある。失敗を値の型 `ExtensionError` で返すこと（生成の例外を、原因の名前と文面に落とすこと）、識別子のキーをオブジェクトの同一性ではなく具体型の `TypeId` にすること、開始時の拡張を受ける構築子 `with_extensions` が `Result` を返すこと（既存の構築子は変えないこと）である。どれも、既存の公開 API の互換は壊さない。

## 9. 影響

- **公開 API**：追加だけである。`fraktor_actor::kernel::extension` の型4つと、`ActorSystem` のメソッド3つ（`with_extensions`、`extension`、`has_extension`）を足す。既存の型、関数、構築子の署名と振る舞いは変えない。
- **台帳**：対象の3ノードのうち2ノード（PropsCreationSpec、ExtensionSpec）の状態、件数、`verified_against: v2.0.0` を更新する。ActorSystemDispatcherSpec のノードは変えない。
- **台帳のツール**：変えない見込みである。ただし、ExtensionSpec のノードは移植6件と作らない区分2件の混在になる。#185 の設計 §8 が記録したとおり、混在の勘定は `verify_red.py` と `verify_green.py` に未実装とされていた。その後の実装で入っているかは、第2段の最初にツールの現物で確かめる。入っていない場合は、ツールの変更とその Python テストが第2段の範囲に加わる。
- **#234 への引き継ぎ**：拡張の登録表（`Rc<RefCell<..>>`）は、#234 の段1で `Rc` から `Arc` への置き換えの対象に加える必要がある。この設計の PR で #234 の設計文書は書き換えず、段1の issue が立つときに引き継ぐ事項として、ここに記録するだけにする。
- **保存済みのデータと上流**：影響は無い。保存済みの形式は変えず、Pekko の上流へ送るものも無い。
- **守りの仕組み**：CI の必須チェックは変えない。第2段で、移植したテストと性質テストが必須チェックの中で動くようになる。Quint の仕様は足さないため、quint のジョブの対象も変わらない。
- **新しいファイルのヘッダー**：第2段で作る `crates/**/*.rs` の新しいファイルには、自作のものも含めて Apache-2.0 のヘッダーを付ける。Pekko の移植であるファイルは、原典のファイルを指定して付ける。

## 10. 第2段の進め方（設計レビューが通ってから）

1. `crates/fraktor-actor/tests/props_creation_spec.rs` に移植する5件を書き、台帳の PropsCreationSpec を red にして、失敗を確認する（red）。本体は既存であるため、テストは最初の実行から通る可能性がある。その場合は #185 の DurationSpec と同じく、red が立たないことをそのまま記録する。
2. `crates/fraktor-actor/tests/extension_spec.rs` に移植する6件を書き、台帳の ExtensionSpec を red にして、失敗を確認する（red）。新しい API が無いため、コンパイルの失敗（E0432、E0599）が red になる。
3. §2 の拡張の API を実装し、テストを通す（green）。実装の途中で設計と違う形が必要になった場合は、実装でごまかさず、設計への差し戻しとして報告する。
4. `crates/fraktor-actor/tests/extension_property_test.rs` に §5 の性質を足す。
5. 台帳の2ノードを §7 の見込みどおりに更新し、`docs/baseline/v2.0.0-undocumented-nodes.tsv` を作り直す。台帳のツールの混在の勘定の有無を、ここで現物で確かめる。
6. CI と同じ検査を流す。`cargo fmt --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`、台帳の Python テスト（`ledger/tools`）、ライセンスヘッダーの検査（`python3 tools/license-headers/license_headers.py --check --manifest docs/design/issue-206-file-origin.tsv`）である。Quint の仕様は無いので、Quint の検査は無い。
7. PR をドラフトから外し、必須のチェックが通る状態にする。マージはしない。ActorSystemDispatcherSpec の分は、この PR には含めない。
