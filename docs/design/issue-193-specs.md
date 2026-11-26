# 設計：#193 actor の PropsCreationSpec、ActorSystemDispatcherSpec、ExtensionSpec

[Issue #193](https://github.com/j5ik2o/fraktor-ng-rs/issues/193) の設計の記録である。対象は Pekko v2.0.0（`9fbd2283b1cf701c90a1223bf621942c191db61f`）の actor-tests にある actor の3つの Spec で、依存する issue は無いとされている。この文書は第1段（設計だけ）であり、実装と台帳の更新は、設計レビューが通ってから第2段で行う。

この文書は、設計の独立レビュー（固定 `e3926b7`、判定は要修正。高1件、中7件、低9件）への対応で書き直した版である。指摘への回答は末尾の §11 に置く。あわせて、元帥の判断（2026-11-26）を2つ反映した。ActorSystemDispatcherSpec の7件は #193 から外し、[Issue #268](https://github.com/j5ik2o/fraktor-ng-rs/issues/268) へ移した（[Issue #256](https://github.com/j5ik2o/fraktor-ng-rs/issues/256) と [Issue #267](https://github.com/j5ik2o/fraktor-ng-rs/issues/267) に依存する）。#193 の完了条件は、PropsCreationSpec と ExtensionSpec の2ノードである。ExtensionSpec の作らない区分の2件と、PropsCreationSpec の JVM に固有の検査を移さないことは、オーナーに伺っている途中で、**オーナーの判断待ち**である。台帳にもテストにも、承認済みとは書かない。

結論を先に書く。**数え直したケースは合計20件であり、issue の目安21件と1件違う。** PropsCreationSpec は目安6件に対して実数5件である。PropsCreationSpec の5件は、本体の新規追加が名前付きの生成の口1つだけで、既存の `Props::new` で移す。ExtensionSpec は、拡張を型で登録する仕組みが fraktor-ng-rs に無いため、本体の新規追加が要る。公開 API の形は §2 に書く。生成と取得は `&mut` の形にし、登録表は `ActorSystem` の通常のフィールドに置く。ActorSystemDispatcherSpec の7件は、この issue では扱わない（§1 の対応表と §8 の判断材料3に、移管の記録と、レビューで直した前提の対応を書く）。

進め方の規約は `.takt/facets/knowledge/fraktor-porting-rules.md`、設計文書の前例は [PR #205](https://github.com/j5ik2o/fraktor-ng-rs/pull/205) の [issue-177-frequency-sketch.md](issue-177-frequency-sketch.md) と、直近の [issue-191-specs.md](issue-191-specs.md) に従う。ノード1件＝テストファイル1件とし、ファイル冒頭に出典（Pekko v2.0.0 のファイルパス）をコメントで残す。設定を写す部分は、HOCON の文字列ではなく型付きの入力にする（#154 の決定 D1 と、G0b の決定 E の前例、および #234 の設計 §1.3 の「dispatcher とメールボックスの設定はコードで組み立てて渡す」）。速さより Pekko の意味論を優先する指針（`docs/decisions/pekko-semantics-first.md`、[PR #236](https://github.com/j5ik2o/fraktor-ng-rs/pull/236) で main に入り済み）に従い、JVM と Scala に固有の振る舞いだけを見るケースは、理由を添えて区分の提案としてだけ書く。

## 1. 何を移植するか

### 件数の数え直し

原典の `" in {"` の数を数え直した。PropsCreationSpec だけが目安と違う。

| Spec | 目安（issue／台帳） | 数え直し |
|:---|---:|---:|
| `actor-tests/src/test/scala/org/apache/pekko/actor/PropsCreationSpec.scala`（77行） | 6／6 | **5** |
| `actor-tests/src/test/scala/org/apache/pekko/actor/ActorSystemDispatcherSpec.scala`（178行） | 7／7 | **7** |
| `actor-tests/src/test/scala/org/apache/pekko/actor/ExtensionSpec.scala`（177行） | 8／8 | **8** |
| 合計 | 21 | **20** |

PropsCreationSpec の5件は、「Props」の下の4件と「Props Java API」の下の1件である。目安の6件目に当たるケースは v2.0.0 の原典に無い。目安の出どころは、Pekko の main（v2.0.0 より後）に足された6件目「work with create(creator)」である可能性が高い（レビューが `git diff 9fbd2283b1 HEAD` で確かめた）。件数を数えるときは、`git show 9fbd2283b1:<パス>` で v2.0.0 の実物を直接使う。台帳の `test_cases_approx` は近似値であり、第2段で台帳を更新するときは `cases.total: 5` として記録する。

ActorSystemDispatcherSpec は、ファイル名と台帳の id が単数形（`ActorSystemDispatcherSpec`）であるのに対して、原典の中のクラス名は複数形の `ActorSystemDispatchersSpec` である。台帳の id は台帳に合わせて単数形のままにする（このノード自体は #268 へ移った）。

台帳の対象ノードは次の3件で、どれも `status: unported`、`triage.kind: pending` である。このうち ActorSystemDispatcherSpec は #268 の対象であり、#193 の第2段では台帳を変えない。

- `actor-tests/src/test/scala/org/apache/pekko/actor/PropsCreationSpec.scala`
- `actor-tests/src/test/scala/org/apache/pekko/actor/ActorSystemDispatcherSpec.scala`（#268 へ移管）
- `actor-tests/src/test/scala/org/apache/pekko/actor/ExtensionSpec.scala`

### 対応表：PropsCreationSpec（5件）

移す先は `crates/fraktor-actor/tests/props_creation_spec.rs` である。原典のアクターは、引数を1つ取る `OneParamActor`（引数の型は中身を持たない `A`）と、引数を2つ取る `TwoParamActor`（`A` と `B`）であり、どちらの `receive` も `Actor.emptyBehavior` である。移植では、引数をクロージャが捕まえる形にする。引数の型 `A` と `B` は、テストの中に置く中身を持たない構造体で写す。

| # | 原典のケース | Rust のテスト名 | 移す／作らない |
|:---:|:---|:---|:---|
| 1 | work with creator | `props_work_with_creator` | 移す |
| 2 | work with classOf | `props_work_with_class_of` | 移す（形を変える。下の段落。JVM に固有の検査の扱いはオーナーの判断待ち） |
| 3 | work with classOf + 1 `null` param | `props_work_with_class_of_and_one_null_param` | 移す（形を変える。同上） |
| 4 | work with classOf + 2 `null` params | `props_work_with_class_of_and_two_null_params` | 移す（形を変える。同上） |
| 5 | work with create(class, creator)（Java API） | `props_work_with_create_class_and_creator` | 移す（形を変える。同上） |

形を変える点は3つある。第1に、原典の `Props(classOf[OneParamActor], null)` の形（実行時のクラスと、コンストラクタの引数の列で組み立てる形）は、JVM のリフレクションでコンストラクタを探して呼ぶ仕組みであり、Rust には無い。移植は、引数を捕まえたクロージャを `Props::new` へ渡す形にする。確かめる性質は「引数を取るアクターを、引数を与えて作れること」であり、リフレクションそのものではない、というのがこの設計の立場である。第2に、原典の `null` は、Rust では `Option::None` を捕まえる形で写す。引数の有無が型で決まるため、「引数の型と数が合わない」という失敗はコンパイル時に出る。第3に、原典が前提にする設定（`pekko.actor.serialize-creators = on`）の検査は移さない。**この検査は、ケースの検査に現れないものではない。** `serialize-creators = on` のとき、`actorOf` はその場で、Props の引数と作り手が直列化して戻せるかを確かめ、戻せなければ `IllegalArgumentException` を投げる（原典 `dungeon/Children.scala`。レビューが v2.0.0 の実物で確かめた）。Spec の頭のコメントが示すとおり、この設定とリフレクションでのコンストラクタの検索が、ケース2からケース5の主な中身の一部である。どちらも JVM に固有であるため、この2つを移さないことは、§8 の判断材料1に置いて**オーナーの判断待ち**とする。オーナーが移さないことを認めない場合は、ケース2からケース5は作らない区分の提案に変える。その場合の代替例は `Props::new(move || OneParamActor::new(None))` である。

Rust では、ケース1からケース4は実質的に同じ形（引数を捕まえたクロージャを `Props::new` へ渡す形）になる。題名ごとのテストは分けて書くが、この同型であることは台帳の記録に書く。件数の見かけを実態より大きく見せないためである。

ケース2とケース3は、原典で中身が同じである。どちらも `Props(classOf[OneParamActor], null)` を `system.actorOf` へ渡すだけであり、題名だけが違う。移植でも、題名に対応するテストを2件に分けて書き、中身が同じであることをテストのコメントに残す。

ケース5の原典は、Java API の `Props.create(classOf[Actor], creator)` であり、`creator` は `pekko.japi.function.Creator[Actor]` を実装したオブジェクトである。移植は、`create()` に当たるメソッドを持つ構造体をテストの中に置き、そのメソッドの戻り値を `Props::new` のクロージャから返す形にする。Java のインターフェースそのものは作らない。

検査の形は次のとおりにする。原典の検査は `system.actorOf(p)` が例外を出さずに終わることだけである。移植は、生成が終わることに加えて、作り手が捕まえた引数を観測する。作り手は、呼ばれたときに捕まえた値をテスト側の共有の記録（`Arc<Mutex<..>>`）へ書き、テストは `actor_of` と `run_until_idle` のあとでその記録を読み、与えた引数（`None` の場合は `None` であること）と一致することを確かめる。これが §3 の性質 I8 を実際に確かめる方法である。検査を足す方向であり、原典より弱める部分は無い。

### 対応表：ActorSystemDispatcherSpec（7件）

この7件は #193 から外し、[Issue #268](https://github.com/j5ik2o/fraktor-ng-rs/issues/268) へ移した（元帥の判断、2026-11-26）。#268 は [Issue #256](https://github.com/j5ik2o/fraktor-ng-rs/issues/256)（#234 の段5）と [Issue #267](https://github.com/j5ik2o/fraktor-ng-rs/issues/267) に依存する。この節は、移管の記録と、レビュー（指摘 M7）で直した前提の対応を残すために置く。#193 の第2段ではこのノードを扱わず、台帳も変えない。

前提の対応は、初版の「段5の後／段6の後」から直した。ケース1からケース6には、#234 のどの段の計画にも入っていない前提がある。システムへ実行の場所（原典の `ExecutionContext`）を渡す口、ガーディアンとガーディアンがどの dispatcher で動くかを外から読む口、利用者がガーディアンを与えてシステムを作る構築子、の3つである。ケース4に段6（[Issue #257](https://github.com/j5ik2o/fraktor-ng-rs/issues/257)。dispatcher をクロージャを実行する場所として使う口）は要らない。中身はケース1と同じ仕組み（既定の dispatcher が、渡された実行の場所を使う）に、内部 dispatcher の別名を足したものである。

| # | 原典のケース | 前提（#268 側の記録） |
|:---:|:---|:---|
| 1 | work with a passed in ExecutionContext | #256（段5）の後 ＋ 計画外の前提（実行の場所を渡す口） |
| 2 | not use passed in ExecutionContext if executor is configured | #256（段5）の後 ＋ 計画外の前提（実行の場所を渡す口） |
| 3 | provide a single place to override the internal dispatcher | #256（段5）の後 ＋ 計画外の前提（ガーディアンの dispatcher を読む口） |
| 4 | provide internal execution context instance through BootstrapSetup | #256（段5）の後 ＋ 計画外の前提（実行の場所を渡す口）。段6（#257）は要らない |
| 5 | use an internal dispatcher for the guardian by default | #256（段5）の後 ＋ 計画外の前提（ガーディアンの dispatcher を読む口） |
| 6 | use the default dispatcher by a user provided user guardian | #256（段5）の後 ＋ 計画外の前提（ガーディアンを与える構築子、ガーディアンの dispatcher を読む口） |
| 7 | provide a good error on an dispatcher alias loop in the config | #256（段5）の後。別名の解決は #234 の設計 §3.2 の `Dispatchers` に含まれる |

計画外の前提をどの issue が受け持つかは、#268 の側で決める事項である。この設計では受け持ちを決めない。

### 対応表：ExtensionSpec（8件）

移す先は `crates/fraktor-actor/tests/extension_spec.rs` である。原典の拡張は3つある。`TestExtension` はシステムを保持する拡張、`FailingTestExtension` は生成の途中で名前 `uniqueName` のアクターを作ってから例外を投げる拡張、`InstanceCountingExtension` は作られた回数を数える拡張である。移植でも、テストの中に同じ3つの拡張と、それぞれの識別子を置く。形の写し方は §2 の API に合わせる。`TestExtension` が保持するのはシステムの名前であり、ケース1は名前の一致で観測する。回数を数える識別子は、数え手（`Arc<AtomicUsize>`）を識別子の値の中に持つ（§2 の要点7）。

| # | 原典のケース | Rust のテスト名 | 移す／作らない |
|:---:|:---|:---|:---|
| 1 | support extensions | `supports_extensions` | 移す（形を変える。下の段落） |
| 2 | handle extensions that fail to initialize | `handles_extensions_that_fail_to_initialize` | 移す（形を変える） |
| 3 | fail the actor system if an extension listed in pekko.extensions fails to start | `fails_the_actor_system_if_a_listed_extension_fails_to_start` | 移す（形を変える） |
| 4 | log an error if an extension listed in pekko.extensions cannot be loaded | — | **作らない区分を提案する（オーナーの判断待ち）**（§7） |
| 5 | allow for auto-loading of library-extensions from reference.conf | `allows_auto_loading_of_library_extensions` | 移す（弱めた形。§8 の判断材料5） |
| 6 | not create duplicate instances when auto-loading of library-extensions | `does_not_create_duplicate_instances_when_auto_loading_library_extensions` | 移す |
| 7 | fail the actor system if a library-extension fails to start | `fails_the_actor_system_if_a_library_extension_fails_to_start` | 移す（形を変える） |
| 8 | fail the actor system if a library-extension cannot be loaded | — | **作らない区分を提案する（オーナーの判断待ち）**（§7） |

形を変える点は5つある。第1に、原典は設定文字列（`pekko.extensions = ["org.apache.pekko.actor.TestExtension"]`）にクラス名を書いて読み込むが、移植は型付きの入力にする。システムの生成時に、開始時に読み込む拡張の識別子の列を値で渡す（§2 の `ExtensionStartup`）。クラス名による動的な読み込みは Rust に無いため、「読み込めない」状態そのものが型の上で作れない。これがケース4とケース8を作らない区分として提案する理由である。第2に、原典の `reference.conf`（ライブラリが自分の設定ファイルで申し出る仕組み）は Rust に無い。ケース5とケース6が守る性質は「ライブラリ側の拡張が、システムの開始時に、利用者が個別に頼まなくても作られること」と「同じ拡張が複数回申し出されても作られるのは1回だけであること」である。ケース6は、ライブラリ側の列に同じ識別子を複数回並べる形でそのまま写せる。ケース5は、利用者が何も渡さないシステムでも作られるという形が型付きの入力では作れないため、弱めた形になる。弱め方の判断は §8 の判断材料5に置く。第3に、開始時の2つの列の順序を決める。Pekko はライブラリ側の列を先に読み込む（原典 `ActorSystem.scala` の開始の順序）。移植も、ライブラリ側の列を先に、通常の列を後に処理することを API の契約とする。型付きの入力では「読み込めないときの扱い」の違いは消えるため、2つの列に残る違いは、この順序だけである。第4に、ケース3とケース7の原典は、システムの生成が例外で失敗することを確かめる。移植の `ActorSystem::new` は失敗しない形（戻り値が `Self`）であるため、開始時の拡張を受ける構築子は `Result` を返す形で新しく足す（§2）。第5に、ケース2の原典は、投げられた例外を2回とも `intercept` で受け、同じ例外であることを確かめる。移植の失敗は値（§2 の `ExtensionError`）であり、2回とも同じ原因の名前と文面の `Err` が返ることで写す。例外の同一性そのものは写さない。

ケース5とケース6にある、設定の列を読み戻す検査（`listedExtensions.count(...)` が1／3であること）は移さない。読み戻す対象は HOCON の設定であり、型付きの入力では列そのものが値として手元にあるため、読み戻しは検査にならない。2つのケースが守る性質は、作られた回数（どちらも1回）であり、その検査は残す。

ケース2の副作用（失敗する拡張が、例外を出す前に名前 `uniqueName` のアクターを作ること）は、移植でも同じ順序で書く。そのために、名前付きでトップレベルのアクターを作る口を `ActorSystem` へ足す（§2）。生成が1回しか走らないことは、作られた回数を数える仕組みを失敗する拡張にも置き、2回の取得のあとで回数が1であることを確かめる形で、原典より弱めずに写す。原典は回数を数えていないため、この検査は移植で足す検査である。足す理由は、失敗の保存（2回目に生成をやり直さないこと）がこのケースの性質そのものであり、回数がその直接の観測になるためである。

ケース1の原典は、設定で申し出された拡張が開始時に読み込まれていること（`hasExtension` が true）、取得した拡張が同じシステムを保持すること、取得の経路が2通り（識別子の `apply` と `system.extension`）とも同じ拡張を返すことを確かめる。移植の取得の経路は2つに分ける。生成する経路（`register_extension`。原典の `apply` と `registerExtension` の写し）と、探すだけの経路（`extension`。原典の `extension` の写しであり、登録されていなければ失敗する）である。ケース1は、開始時に登録された拡張を `extension` で探して同じ `Arc` が返ること（`Arc::ptr_eq`）で写す。「同じシステムを保持すること」は、拡張が生成時に受け取ったシステムの名前を保持し、テストがその名前とシステムの名前の一致を確かめることで観測する。システムそのものを保持する形は採らない。`ActorSystem` は `Clone` ではなく、今の main では `Send` でも `Sync` でもないためである（§2 の前提）。

## 2. 対応する本体がすでにあるか

| 原典の対象 | fraktor-ng-rs の現状 | この issue で足すもの |
|:---|:---|:---|
| `org.apache.pekko.actor.Props`（作り手、`classOf` と引数、Java の `Creator`） | `crates/fraktor-actor/src/kernel/actor/props.rs` の `Props` がすでにある。`Props::new<F, A>(factory: F) -> Self`（`F: Fn() -> A + Send + Sync`、`A: Actor`）、`Props::empty()`、`with_dispatcher(name)`、`dispatcher()` を持つ。型の文書に書かれているとおり、移植は作り手だけを保持し、クラスと引数の列は保持しない。単体テスト `props_test.rs` もすでにある | 名前付きでトップレベルのアクターを作る口（`ActorSystem::actor_of_with_name`）だけを足す。`Props` 自体は変えない |
| `org.apache.pekko.actor.ActorSystem` の dispatcher（`Dispatchers.lookup`、別名、内部 dispatcher、ガーディアン） | 無い。`ActorSystem`（`crates/fraktor-actor/src/kernel/actor_system/actor_system_value.rs`）の型の文書は「Dispatchers, guardians, and extensions belong to later goals」と明記する。`Props::with_dispatcher` は名前を記録するだけで、解釈はしない。実行は `Executor`（`crates/fraktor-actor/src/kernel/executor/executor_value.rs`）が1つのシャードの列を持ち、テストのスレッドが回す形であり、dispatcher の登録表も、id の解決も、ガーディアンという概念も無い | 無し（#268 の担当であり、この issue では足さない） |
| `org.apache.pekko.actor.Extension`、`ExtensionId`、`ExtensionIdProvider`、システムの拡張の登録表（`registerExtension`、`extension`、`hasExtension`） | 無い。`crates/` の全体に `Extension` を名前に含む型は、testkit の `TestKitSettings`（文書では `TestKitExtension` の設定と説明されるが、型としてはシステムから取得する形ではなく `TestKitSettings::from_system(&system)` で直接作る）と、event の購読まわりだけであり、拡張の登録表は無い。型で引く表の前例は `ActorSystemSetup`（`crates/fraktor-actor/src/kernel/setup/actor_system_setup.rs`）であり、具体型の `TypeId` をキーにして setup を保持する | 拡張の登録表と公開 API を新しく足す（下の節） |
| testkit（`PekkoSpec`、`TestProbe`、`EventFilter`、暗黙の送信者） | `crates/fraktor-testkit` に `FraktorSpec`、`TestKit`、`TestProbe`、`EventFilter` がすでにある。台帳の `needs` は、PropsCreationSpec が `extends-pekko-spec` と `pekko-spec`、ActorSystemDispatcherSpec がそれらに加えて `test-actor`、`implicit-sender`、`expect-msg`、`expect-no-message`、`test-probe`、ExtensionSpec が `test-kit` と `event-filter` である。ExtensionSpec の移植6件は `EventFilter` を使わない。原典が `EventFilter` を使うのは、作らない区分を提案するケース4だけである | 無し |

### この設計の前提（どの main を前提にするか）

この設計は、この PR の基点の main（`40b1870`）を前提にする。段1の [PR #266](https://github.com/j5ik2o/fraktor-ng-rs/pull/266)（`ActorSystem` と `WeakActorSystem` を `Send + Sync` にする）は、まだマージされていない。この設計の拡張の API は、どちらの main でも書ける形にした。生成と取得を `&mut` の形にしたため、登録表に `Rc` と `RefCell` が要らず、`ActorSystem` が `Send + Sync` かどうかに API が依存しない。#266 が入ったあとも、この API は変えない。拡張がシステムへの弱い参照（`WeakActorSystem`）を保持する形は、#266 の後なら書けるが、この設計は採らない。ケース1の観測は名前で足りるためである。

### 新しく足す拡張の API

置き場所は、リポジトリの1型1ファイルの並びに合わせて、`crates/fraktor-actor/src/kernel/extension.rs`（モジュールの入口）と、その下の3ファイルとする。

- `extension/extension_trait.rs`：`Extension` トレイト
- `extension/extension_id_trait.rs`：`ExtensionId` トレイト
- `extension/extension_error.rs`：`ExtensionError`
- `extension/extension_startup.rs`：`ExtensionStartup`

公開 API の形は次のとおりである。生成は `&mut ActorSystem` を受け取る。`actor_of` が `&mut self` である今の `ActorSystem` の形に合わせたものであり、レビューの試作（`&mut` の形）でケース1とケース2が書けることを確かめてある。

```rust
/// Mirrors `org.apache.pekko.actor.Extension`. A marker trait: an extension
/// is any value the system keeps once per extension id.
pub trait Extension: Any + Send + Sync {}

/// Mirrors `org.apache.pekko.actor.ExtensionId`. The concrete type of the
/// id is the key: a system holds at most one extension per id type, as
/// `ActorSystemSetup` holds at most one setup per concrete type. Two values
/// of the same id type are the same id; state an id needs while creating
/// (such as a counter) travels in the id value itself.
pub trait ExtensionId: Any + Send + Sync {
  type Ext: Extension;
  /// Mirrors `ExtensionId.createExtension`. The system is handed over as
  /// `&mut ActorSystem`, so creating an extension can already start actors,
  /// including named ones through `ActorSystem::actor_of_with_name`.
  /// Returning `Err` is the port of throwing from `createExtension`: the
  /// system keeps the error and returns it again for every later request
  /// for the same id.
  fn create_extension(&self, system: &mut ActorSystem) -> Result<Self::Ext, ExtensionError>;
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
  /// `startup`, library extensions first and the listed extensions after
  /// them, mirroring the order Pekko loads `pekko.library-extensions`
  /// before `pekko.extensions`. When creating one extension fails, the
  /// system is terminated and the error is returned; no system is handed
  /// out.
  pub fn with_extensions(
    name: &str,
    boundary: Arc<dyn ShardBoundary>,
    startup: ExtensionStartup,
  ) -> Result<Self, ExtensionError>;

  /// Returns the extension for `id`, creating it on the first request.
  /// Mirrors `ExtensionId.apply(system)` and
  /// `ActorSystem.registerExtension(id)`.
  pub fn register_extension<E: ExtensionId>(&mut self, id: &E) -> Result<Arc<E::Ext>, ExtensionError>;

  /// Returns the extension for `id` without creating it. Mirrors
  /// `ActorSystem.extension(id)`, whose documentation promises a failure
  /// when the extension is not registered: an unregistered id, and an id
  /// whose creation is still running, return `Err`. An id whose creation
  /// failed returns the kept error.
  pub fn extension<E: ExtensionId>(&self, id: &E) -> Result<Arc<E::Ext>, ExtensionError>;

  /// Returns whether the extension for `id` has been created successfully.
  /// Mirrors `ActorSystem.hasExtension(id)`. Pekko's `hasExtension` goes
  /// through the lookup that rethrows a kept failure; this port returns
  /// `false` for a failed id instead, which stays inside what the Pekko
  /// documentation promises ("whether the extension is already
  /// registered").
  pub fn has_extension<E: ExtensionId>(&self, id: &E) -> bool;

  /// Starts a top-level actor under `/user` with `name`. Mirrors
  /// `ActorSystem.actorOf(props, name)`. `actor_of` keeps handing out
  /// generated names; this is the named form.
  pub fn actor_of_with_name(&mut self, props: Props, name: &str) -> ActorRef;
}

/// The extensions a system creates at startup, as typed values.
#[derive(Default)]
pub struct ExtensionStartup { /* erased entries, library list and listed list */ }

impl ExtensionStartup {
  pub fn new() -> Self;
  /// Adds an id to the list that mirrors `pekko.extensions`.
  pub fn extension<E: ExtensionId>(self, id: E) -> Self;
  /// Adds an id to the list that mirrors `pekko.library-extensions`.
  /// Library extensions are created before the listed ones.
  pub fn library_extension<E: ExtensionId>(self, id: E) -> Self;
}
```

設計の要点は7つある。

1. **キーは識別子の具体型の `TypeId` である。** Pekko は識別子のオブジェクトの同一性で引く。Rust には単一の実体を表すオブジェクトの慣習が無いため、`ActorSystemSetup` の前例どおり具体型で引く。同じ型の識別子の値は、値が違っても同じ識別子である。このことを API の文書に書く。
2. **登録表は `ActorSystem` の通常のフィールド `BTreeMap<TypeId, ExtensionEntry>` であり、`Rc` も `RefCell` も使わない。** `ExtensionEntry` は `Creating`、`Created(Arc<dyn Any + Send + Sync>)`、`Failed(ExtensionError)` の3通りを持つ非公開の enum である。生成と登録が `&mut` であるため、内側の可変性は要らない。#234 の設計 §2.4 が段1で `Arc` にすると決めた4か所には触れず、所有権モデルの「極力、単独所有」にも合う。保持する値が `Arc<dyn Any + Send + Sync>` であるため、表そのものは `Send + Sync` の条件を満たし、段1の置き換えの対象にも入らない。取得のときに `E::Ext` へ `downcast` して返す。
3. **生成の手順は「印を入れてから生成を呼ぶ」順にする。** 登録は、表に無ければ `Creating` を入れてから `create_extension` を呼び、結果で `Created` か `Failed` へ置き換える。生成の途中で同じ識別子が再入してきた場合は、`Creating` を見て、生成中の要求であることを示す `ExtensionError` を返す。生成の途中で別の識別子を取得することはできる（入れ子の登録がそのまま動く）。
4. **生成の途中で panic した場合は、panic をそのまま伝え、`Creating` の印は残る。** kernel は panic を捕まえる仕組みを持たないため、失敗として保存することはできない（捕まえるのは adaptor の `UnwindCatcher` で、監督の経路だけが使う）。印が残った識別子への後の要求は、生成中の要求と同じ `Err` を返す。Pekko は例外を受け止めて失敗として保存するため、ここは原典と違う。この違いを API の文書に書く。巻き戻しと再入の振る舞いは、台帳の外の単体テスト（`crates/fraktor-actor/src/kernel/extension/` の下の `*_test.rs`）で確かめる。再入した生成が `Err` を返し、外側の生成がその `Err` を伝えて `Failed` になることと、panic が伝わり、印が残って後の要求が `Err` になることの2件である。
5. **開始時の読み込みは、システムの値を組み立てたあとに、ライブラリ側の列から先に行う。** 拡張の生成は `&mut ActorSystem` を受け取り、`actor_of` と `actor_of_with_name` を呼べる必要があるためである。`with_extensions` は、既存の生成の手順でシステムを作り、次に `ExtensionStartup` の列を順に `register_extension` し、失敗したらシステムを `terminate` してから `Err` を返す。列の中で同じ識別子が重なった場合は、列の種類（通常かライブラリか）を問わず、作るのは1回だけである（ケース6の性質）。
6. **失敗は値で保存し、生成はやり直さない。** 生成が `Err` を返した識別子については、その `ExtensionError` を保存し、後の `register_extension` と `extension` はすべて同じ値を返す。原典が同じ例外を2回出すことの写しである。
7. **回数を数える識別子は、数え手を識別子の値の中に持つ。** `static` の数え手は使わない。Rust のテストは同じバイナリの中で並行して走るため、原典の `InstanceCountingExtension` のような大域の数え手は、テストの間で数が混ざる。識別子が `Arc<AtomicUsize>` を持ち、テストは識別子を渡す前にその `Arc` の複製を手元に残して、後で回数を読む。性質テスト（§5）も同じ持ち方にする。

`ExtensionStartup` を `ActorSystemSetup` に入れる `Setup` の値にする形（Pekko の `BootstrapSetup` と同じ置き場所）は、レビューで検討を求められたため検討した。採らない。理由は、開始時の拡張の失敗は `Result` で返さなければならず、`Setup` を読む既存の構築子 `with_setup` は失敗しない形で、型の文書も「setup は開始の仕方を変えない」と明記しているためである。`Setup` の形にすると、失敗を panic に変えるか、`with_setup` の署名を壊すかのどちらかになる。構築子が増える欠点は残るが、失敗の返し方を守ることを優先する。

### no_std

新しく足す型はすべて kernel に置き、`core` と `alloc` だけで書ける。使うのは `BTreeMap`、`Arc`、`TypeId`、`Any` であり、どれも今の kernel がすでに使っている。時刻、スケジューリング、IO には触れないため、port は要らない。`ExtensionStartup` の列の保持も `alloc` の `Vec` で足りる。

## 3. 守る性質（不変条件）

| 番号 | 性質 | 主に確かめるケース |
|:---|:---|:---|
| I1 | 同じシステムと同じ識別子について、拡張の生成は最大で1回だけ走る。成功した場合は、後の取得はすべて同じ `Arc` を返す（`Arc::ptr_eq` で等しい） | ExtensionSpec のケース1、ケース6 |
| I2 | 生成が失敗した識別子については、生成はやり直されず、後の取得はすべて原因の名前と文面が等しい `Err` を返す | ExtensionSpec のケース2 |
| I3 | 開始時の列に挙がった拡張は、システムが返る前に作られている。開始時の生成が失敗した場合は、システムは返らない | ExtensionSpec のケース1、ケース3、ケース5、ケース7 |
| I4 | 開始時の列で同じ識別子が何回現れても、作られるのは1回だけである。列の種類は問わない | ExtensionSpec のケース6 |
| I5 | `has_extension` が true を返すのは、その識別子の拡張が成功して作られた場合に限る。未取得の場合と、生成が失敗した場合とは false である | ExtensionSpec のケース1 |
| I6 | 拡張の生成は、生成の途中でシステムを使う（名前付きでアクターを作る、別の拡張を取得する）ことができる | ExtensionSpec のケース2 |
| I7 | `Props` の作り手は再利用できる。同じ `Props` からアクターを2つ作ると、作り手は2回呼ばれ、別々のアクターが動く。監督による再起動も、同じ作り手から新しい実体を作る | PropsCreationSpec の全件（性質そのものは既存の `props_test.rs` と再起動のテストが担う。この issue のテストは生成の経路ごとに1回ずつ確かめる） |
| I8 | 引数を取るアクターは、与えた引数を保持したまま作られる。引数が `None` の場合も作れる。観測は、作り手が捕まえた値を記録し、`run_until_idle` のあとで読むことで行う | PropsCreationSpec のケース1からケース4 |

dispatcher についての性質（別名の解決が循環で止まること、別名の連鎖の深さの上限、ガーディアンの dispatcher の既定値）は、#268 と #234 の設計が担う。この設計では、ActorSystemDispatcherSpec のケース7が使う上限の値だけを原典から記録する。原典の `Dispatchers.scala` の `MaxDispatcherAliasDepth` は20であり、深さが20を超えると `ConfigurationException` になる。

## 4. 壊しうる入力

| 入力 | 何が起きうるか | どう扱うか |
|:---|:---|:---|
| 空文字列の識別子名、極端に長い文字列 | 拡張の識別子は型であり、名前の文字列を持たないため、当てはまらない。dispatcher の id は文字列であり、空の id と極端に長い id がありうる | 拡張については対象外と明記する。dispatcher の id は #268 の担当であり、この設計では扱わない |
| ワイルドカードの連続（dispatcher の id やアクター名のパターン） | この2つの Spec には、パターン照合で id や名前を引くケースが無い | 対象外と明記する |
| 拡張の生成が、同じ識別子の拡張を再入して取得する | 生成の循環である。印が無ければ、際限なく再帰して落ちる | §2 の要点3のとおり `Creating` の印で検出し、生成中の要求であることを示す `ExtensionError` を返す。台帳の外の単体テストで確かめる（§2 の要点4） |
| 拡張の生成が panic する | Pekko では生成の例外が、そのまま開始の失敗と失敗の保存になる。Rust の panic を失敗として保存するには、kernel が panic を捕まえる必要があり、kernel は捕まえる仕組みを持たない | §2 の要点4のとおり、panic は保存せずに伝え、印は残る。原典との差として、API の文書に書く。台帳の外の単体テストで確かめる |
| まだ登録されていない識別子での `extension` の呼び出し | Pekko の文書は失敗を約束している。生成する形にまとめると、その約束から外れる | `extension` は探すだけにし、未登録は `Err` を返す（§2）。ケース1のテストは、登録の前に `extension` が `Err` になることも確かめる |
| 拡張の生成が、名前の重なるアクターを作ってから失敗する（ケース2の `uniqueName`） | 生成をやり直すと、名前の重なりで2回目の失敗の原因が変わる | I2 のとおり生成はやり直さない。ケース2のテストは、作られた回数が1であることまで確かめる |
| 回数を数える拡張を、並行して走るテストが共有する | 大域の数え手は、テストの間で数が混ざり、テストが不安定になる | §2 の要点7のとおり、数え手は識別子の値の中に持つ。`static` の数え手は使わない |
| 別のスレッドからの、同時の拡張の要求 | 今の形は `&mut` であり、同時に要求することはできない。#234 の段5で複数のスレッドから使えるようになると、Pekko の文書が約束する「初期化の完了を待つ」振る舞いとの差が出る | この issue では扱わない。§9 の引き継ぎに書く。段5で待つ形に直すときに、Quint の仕様を書く（§6） |
| 作り手（`Props` の factory）が panic するアクター生成 | 既存の振る舞いである。`FraktorSpec` のシステムでは捕まえる仕組みが監督の判断に変え、素のシステムでは panic が伝わる | この issue では変えない。PropsCreationSpec のケースは、panic しない作り手だけを使う |
| 境界の時間の値 | この2つの Spec には、時間の値そのものを検査するケースが無い。ActorSystemDispatcherSpec の `expectMsg(1.second, ...)` は待ちの上限であり、性質ではない（その Spec は #268 へ移った） | 対象外と明記する。移植の待ちは、testkit の既存の待ち方（手で回す実行器と `WakeSignal` の前例）に従い、実時間の上限だけを使う |
| 開始時の列が空、列に同じ識別子が多数回現れる | 空の場合は今の `ActorSystem::new` と同じになる。多数回の重なりは I4 で覆う | ケース6が3回の重なりを確かめる。それ以上の回数は §5 の性質で覆う |

## 5. PBT を足すか

ExtensionSpec の登録表にだけ足す。対象は I1、I2、I4 であり、置き場所は `crates/fraktor-actor/tests/extension_property_test.rs` とする。原典外のテストなので、台帳の件数には含めない。外部の依存は足さず、既存の性質テスト（`frequency_sketch_property_test.rs` など）と同じく固定 seed の SplitMix64 で入力を作る。数え手は §2 の要点7のとおり識別子の値の中に持ち、試行の間でもテストの間でも数が混ざらないようにする。

| 番号 | 性質 | 確かめる不変条件 |
|:---|:---|:---|
| P1 | 成功する識別子と失敗する識別子を混ぜた取得の列を任意の順序と回数で実行しても、識別子ごとの生成の回数は最大で1であり、成功した識別子は常に対象の `Arc` と `ptr_eq` で等しく、失敗した識別子は常に等しい `Err` を返す | I1、I2 |
| P2 | 開始時の列に、成功する識別子だけを任意の重なり回数で並べても、成功する識別子が作られる回数は1であり、`has_extension` は列に現れた成功する識別子についてだけ true になる（開始時に失敗する識別子を含む列は、P2 の対象から外す。その列はシステム自体が返らないためであり、失敗の列はケース3とケース7が固定の手順で確かめる） | I3、I4、I5 |

入力は、成功する識別子2つと失敗する識別子1つの、最大で32回までの取得の列とする。有効な試行の下限は設けない。どの入力の列でも、生成の回数の主張は全件に成り立つためである。

PropsCreationSpec には PBT を足さない。作り手は入力の空間を持たず、ケースは生成の経路の列挙が本体であるためである。別名の連鎖（循環、深さの上限、別名の指す先が無い場合）の性質は、登録表が #256 で入るときに、#268 の側で扱う。

## 6. Quint を書くかどうか

この issue では Quint を書かない。理由は3つある。レビューは、この判断を条件つきで認めた。条件は、巻き戻しと再入に単体テストを置くこと（§2 の要点4で置く）と、段5で複数のスレッドから使えるようになるときの仕様を §9 の引き継ぎに書くこと（書いた）である。

第1に、ActorSystemDispatcherSpec が関わる状態遷移（メールボックスの予約、suspend と再開、dispatcher の寿命）は、#234 の設計がすでに Quint で担っている。段2までの仕様は `crates/fraktor-actor/specs/mailbox_scheduling.qnt`（[PR #236](https://github.com/j5ik2o/fraktor-ng-rs/pull/236) で main に入り済み）であり、dispatcher の寿命の仕様 `dispatcher_lifecycle.qnt` は段6で足す計画である。この issue で別の仕様を書くと、同じ状態遷移を2つの仕様が持つことになる。その Spec 自体も #268 へ移った。

第2に、ExtensionSpec の登録表は、状態遷移を持つものの、単一スレッドで動く4つの状態（未生成、生成中、生成済み、失敗を保存済み）だけであり、並行する実行の重なりも、時間で変わる状態も無い。確かめる性質は I1 から I6 で尽きており、固定のテストと §5 の性質で覆える。行き詰まりや取りこぼしのような、順序の組み合わせでしか出ない欠陥の形が無い。

第3に、PropsCreationSpec には状態遷移が無い。作り手の呼び出しは1回の生成であり、stash のような保持の仕組みも使わない。なお、この2つの Spec のケースはどれも stash を使わない。stash の容量とあふれたときの振る舞い（`StashOverflowException`）は、既存の実装でも未移植であり（`crates/fraktor-actor/src/kernel/actor/actor_context.rs` の `stash` の文書）、この issue の対象にはならない。

## 7. 台帳の見込みと、区分の提案

第2段で台帳を更新する。ゴールの識別子は、#177 と #185 の命名に倣って `G0issue193` とする。承認の欄（`approved_by` と `approved_at`）は、移植の規約により人間とレビューの決定を写す欄であり、この設計文書の作成者が自分で書くものではない。作らない区分の2件は**オーナーの判断待ち**であり、承認済みとは書かない。

| ノード | 総数 | 移植 | 作らない区分の提案 | 承認後の状態の見込み |
|:---|---:|---:|---:|:---|
| `actor-tests/src/test/scala/org/apache/pekko/actor/PropsCreationSpec.scala` | 5 | 5 | 0 | `green` |
| `actor-tests/src/test/scala/org/apache/pekko/actor/ExtensionSpec.scala` | 8 | 6 | 2（オーナーの判断待ち） | `green`（作らない区分の提案が承認された場合） |
| `actor-tests/src/test/scala/org/apache/pekko/actor/ActorSystemDispatcherSpec.scala` | 7 | — | — | #193 では変えない（#268 の対象） |

PropsCreationSpec の移植5件は、ケース2からケース5について JVM に固有の検査（直列化の検証と、リフレクションでのコンストラクタの検索）を移さない形である。その扱いは**オーナーの判断待ち**であり（§8 の判断材料1）、オーナーが認めない場合はケース2からケース5が作らない区分の提案に変わり、このノードの見込みも変わる。

台帳の `needs` は手で変えない。`ledger/tools/extract_needs.py` が原典から機械で抜き出して書き直すものであり、手で直しても再実行で戻る。前例の #188（TokenBucketSpec）も、移植が testkit を使わないのに `needs` を変えていない。ExtensionSpec の移植6件が `FraktorSpec` と `EventFilter` を使わないことは、テストファイルの冒頭の記録で示すに留める。

作らない区分の提案は次の2件である。要素の形（`name`、`reason`、`alternative`、承認の欄）は `docs/decisions/ledger-schema-draft.md` の定義に従う。代替例は、実際に検査される形にする。`ExtensionStartup` の rustdoc に `compile_fail` の doctest を置き、存在しない識別子の型を列へ渡すコードがコンパイルできないことを示す。

| ケース | 理由 | 代替例 |
|:---|:---|:---|
| ExtensionSpec「log an error if an extension listed in pekko.extensions cannot be loaded」 | 設定に書いた完全修飾クラス名をクラスローダで読み込む仕組みそのものの検査である。読み込めないクラス名を書いたとき、システムは開始を続け、エラーのログを1件出すことを確かめる。移植の設定は型付きの入力であり、存在しない拡張の識別子を書くことはコンパイル時にできない。読み込みの失敗という状態が型の上で作れないため、等価なテストは書けない。ログを出す経路（event stream への `Error` の発行）自体は fraktor-ng-rs にあるが、このケースが確かめるのは経路ではなく、名前で読み込めない場合の扱いである | 存在しない識別子の型を `ExtensionStartup` へ渡すコードがコンパイルできないことを、`compile_fail` の doctest で示す。型付きの列に書けるのは、実在する識別子の型だけである |
| ExtensionSpec「fail the actor system if a library-extension cannot be loaded」 | ケース4と同じ仕組みの、ライブラリ側の列での検査である。読み込めないクラス名をライブラリ側の列に書いたとき、システムの生成が `RuntimeException` で失敗することを確かめる。ケース4と同じく、失敗の状態が型の上で作れない | ケース4と同じ `compile_fail` の doctest である。実在しない識別子は列に書けない |

2件とも、作らない区分として提案する。理由は、検証の対象が JVM のクラスローダという言語と実行時のための補助であり、その振る舞い（存在しないものを指せないこと）を Rust の言語機能が満たすためであり、台帳の作らない区分の条件に当たるためである。開始時に読み込む列の扱いそのもの（失敗したらシステムが失敗すること、エラーを出すこと）のうち、失敗の部分はケース3とケース7が移す。ケース4の「エラーのログを出して開始を続ける」部分に対応する振る舞いは、型付きの入力では要らなくなる。読み込めないものが列に入らないためである。レビューは、代替例を `compile_fail` の doctest にすることを条件に、この提案を認めるよう推奨した。決めるのはオーナーであり、この提案は**オーナーの判断待ち**である。

PropsCreationSpec と ExtensionSpec の2ノードが green になると、`docs/baseline/v2.0.0-undocumented-nodes.tsv` は2行増える見込みである。

## 8. レビューと判断の記録

判断材料と、その状態を置く。

1. **PropsCreationSpec のケース2からケース5を、クロージャが引数を捕まえる形で移す立場でよいか。あわせて、JVM に固有の検査（`serialize-creators` の直列化の検証と、リフレクションでのコンストラクタの検索）を移さないことでよいか。** レビューは、条件つきで移植を認めるよう推奨した。条件は、JVM に固有の検査を判断材料に足してオーナーに伺うこと（足した）、作り手が捕まえた値を観測して I8 を実際に確かめること（§1 の検査の形に入れた）、ケース1からケース4が実質的に同じ形になることを台帳の記録に書くこと（§1 に書いた）の3つである。この件は元帥がオーナーに伺っている途中であり、**オーナーの判断待ち**である。オーナーが移植を認めない場合は、ケース2からケース5は作らない区分の提案に変える。その場合、ノードは移植1件と作らない区分4件の混在になる。
2. **ExtensionSpec のケース4とケース8の作らない区分の提案を認めるか。** レビューは、代替例を `compile_fail` の doctest にすることを条件に、認めるよう推奨した（§7 に反映した）。決めるのはオーナーであり、**オーナーの判断待ち**である。認めない場合の代案は、開始時の列に「読み込めない拡張」を表す値を入れられる形にすることである。たとえば、名前の文字列と生成の関数を組にした登録の口を別に足す案がある。ただしその案は、型付きの入力にするという #154 の決定 D1 と #234 の設計の方針に反するうえ、利用者が実際に使うことのない失敗の形を公開 API に残すことになる。この設計はその案を採らない。
3. **ActorSystemDispatcherSpec の7件の扱い。** 元帥の判断（2026-11-26）で、7件は #193 から外し、[Issue #268](https://github.com/j5ik2o/fraktor-ng-rs/issues/268) へ移した。#193 の完了条件は、PropsCreationSpec と ExtensionSpec の2ノードである。前提の対応は、レビューの指摘 M7 のとおりに直した（§1 の対応表）。計画外の前提（実行の場所を渡す口、ガーディアンの口）をどの issue が受け持つかは、#268 の側で決める。
4. **新しく足す拡張の API の形でよいか。** レビューの推奨に沿って、`&mut` の形に直した（§2）。あわせて、生成する経路（`register_extension`）と探すだけの経路（`extension`）を分け、登録表から `Rc` と `RefCell` を外し、開始時の順序（ライブラリ側の列が先）を契約に書いた。レビューが認めた点は、識別子のキーを具体型の `TypeId` にすること、失敗を値の型 `ExtensionError` で返すこと、開始時の拡張を受ける構築子が `Result` を返すこと（既存の構築子は変えないこと）の3つである。再レビューで、この形でよいかを確かめてほしい。
5. **ExtensionSpec のケース5を、弱めた形で移すか、除外の提案にするか。** 原典が確かめるのは、ライブラリが `reference.conf` に書いた拡張が、利用者が何も渡さないシステムでも作られることである。型付きの入力では「利用者が何も渡さない」形が作れない。この設計の案は、弱めた形で移すことである。テストの中で、ライブラリ側の列を組み立てる関数を分けて置き（ライブラリの申し出に当たる部分）、利用者側のコードはその拡張に触れない形にし、開始時に作られていることと回数が1であることを確かめる。弱める点は、申し出の経路（設定ファイルと classpath）が値の受け渡しに変わることである。もう1つの案は、ケース5を除外の提案にすることである。その場合、このノードは移植5件、作らない区分2件、除外1件になる。決めるのはオーナーであり、**オーナーの判断待ち**として置く。第2段は、この判断が出るまでケース5を除いた形で進められる。

## 9. 影響

- **公開 API**：追加だけである。`fraktor_actor::kernel::extension` の型4つと、`ActorSystem` のメソッド4つ（`with_extensions`、`register_extension`、`extension`、`has_extension`）と、名前付きの生成の口（`actor_of_with_name`）を足す。既存の型、関数、構築子の署名と振る舞いは変えない。
- **台帳**：対象の3ノードのうち2ノード（PropsCreationSpec、ExtensionSpec）の状態、件数、`verified_against: v2.0.0` を更新する。ActorSystemDispatcherSpec のノードは #193 では変えない（#268 の対象）。
- **台帳のツール**：変更が必ず入る。ExtensionSpec のノードは移植6件と作らない区分2件の混在になり、レビューが現物で確かめたとおり、混在の勘定は `ledger/tools/verify_green.py` と `verify_red.py` に未実装である（`not_built` の語が0件）。第2段の範囲に、ツールの変更とその Python テストを含める。PropsCreationSpec の判断（§8 の判断材料1）の結果によっては、そちらのノードも混在になる。
- **#234 への引き継ぎ**：2件ある。第1に、拡張の登録表は段1（[PR #266](https://github.com/j5ik2o/fraktor-ng-rs/pull/266)）の `Arc` への置き換えの対象には入らない。登録表は通常のフィールドであり、保持する値は `Arc<dyn Any + Send + Sync>` であるためである。第2に、段5（[Issue #256](https://github.com/j5ik2o/fraktor-ng-rs/issues/256)）で `ActorSystem` が複数のスレッドから使えるようになったら、生成中の識別子への要求は、Pekko の文書の約束どおり完了を待つ形に直す。そのときに、Quint の仕様 `crates/fraktor-actor/specs/extension_registry.qnt` を書く（§6 の条件）。
- **アクターの中から拡張を取る経路**：Pekko の `context.system.extension` に当たる経路は、この issue の範囲の外である。今回のケースはどれも使わない。必要になったときに、別の issue で扱う。
- **保存済みのデータと上流**：影響は無い。保存済みの形式は変えず、Pekko の上流へ送るものも無い。
- **守りの仕組み**：CI の必須チェックは変えない。第2段で、移植したテストと性質テストが必須チェックの中で動くようになる。Quint の仕様は足さないため、quint のジョブの対象も変わらない。
- **新しいファイルのヘッダー**：第2段で作る `crates/**/*.rs` の新しいファイルには、自作のものも含めて Apache-2.0 のヘッダーを付ける。Pekko の移植であるファイルは、原典のファイルを指定して付ける。

## 10. 第2段の進め方（設計レビューが通ってから）

1. `crates/fraktor-actor/tests/props_creation_spec.rs` に移植する5件を書き、台帳の PropsCreationSpec を red にして、失敗を確認する（red）。本体は既存であるため、テストは最初の実行から通る可能性がある。その場合は #185 の DurationSpec と同じく、red が立たないことをそのまま記録する。オーナーの判断（§8 の判断材料1）が出ていない場合は、判断が出るまでケース2からケース5の台帳の記録に「オーナーの判断待ち」と書く。
2. `crates/fraktor-actor/tests/extension_spec.rs` に移植する6件を書き、台帳の ExtensionSpec を red にして、失敗を確認する（red）。新しい API が無いため、コンパイルの失敗（E0432、E0599）が red になる。
3. §2 の拡張の API と `actor_of_with_name` を実装し、テストを通す（green）。あわせて、登録表の単体テスト（再入と巻き戻し）を `crates/fraktor-actor/src/kernel/extension/` の下に置き、`ExtensionStartup` の rustdoc に代替例の `compile_fail` の doctest を置く。実装の途中で設計と違う形が必要になった場合は、実装でごまかさず、設計への差し戻しとして報告する。
4. `crates/fraktor-actor/tests/extension_property_test.rs` に §5 の性質を足す。
5. 台帳のツールの混在の勘定（§9）を実装し、その Python テストを足す。台帳の2ノードを §7 の見込みどおりに更新し、`docs/baseline/v2.0.0-undocumented-nodes.tsv` を作り直す。
6. CI と同じ検査を流す。`cargo fmt --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`、台帳の Python テスト（`ledger/tools`）、ライセンスヘッダーの検査（`python3 tools/license-headers/license_headers.py --check --manifest docs/design/issue-206-file-origin.tsv`）である。Quint の仕様は無いので、Quint の検査は無い。
7. PR をドラフトから外し、必須のチェックが通る状態にする。マージはしない。PR 本文の1行目は、作らない区分の承認が済むまで `Refs #193` のままにする。

## 11. 設計レビューへの回答

固定 `e3926b7` への独立レビュー（判定は要修正。高1件、中7件、低9件）への回答である。採らなかった指摘は無い。

| 指摘 | 対応 | 内容 |
|:---|:---|:---|
| 高 H1（`create_extension(&self, &ActorSystem)` では原典の拡張2つが書けない） | 直した | 生成を `create_extension(&self, system: &mut ActorSystem)` にし、登録と取得も `&mut` を基本の形にした。名前付きでトップレベルのアクターを作る口 `actor_of_with_name` を足す設計にした。ケース1の「同じシステムを保持する」は、拡張がシステムの名前を保持し、名前の一致で観測する形にした。この設計が基点の main を前提にし、段1の [PR #266](https://github.com/j5ik2o/fraktor-ng-rs/pull/266) のマージを待たないことも書いた（§2 の前提の節） |
| 中 M1（`extension` が Pekko の文書と違って生成してしまう） | 直した | 生成する経路 `register_extension`（原典の `apply` と `registerExtension`）と、探すだけの経路 `extension`（未登録は `Err`）に分けた（§2） |
| 中 M2（生成中の扱いと `Rc<RefCell<..>>` が #234 の計画と合わない） | 直した | 登録表を通常のフィールドの `BTreeMap` にし、`Rc` と `RefCell` を外した。保持する値は `Arc<dyn Any + Send + Sync>` で、段1の置き換えの対象に入らない。生成中の要求を待つ形への直しと、そのときの Quint の仕様は、段5への引き継ぎとして §9 に書いた |
| 中 M3（panic で印が残り、循環と巻き戻しにテストが無い） | 直した | 巻き戻しのときの振る舞いを決めて §2 の要点4に書いた（panic は伝え、印は残り、後の要求は `Err`）。再入と巻き戻しの単体テストを、台帳の外に置くことにした |
| 中 M4（ケース5の写し方が弱く、2つの列の違いが決まっていない） | 直した | 開始時の順序（ライブラリ側の列が先）を API の契約に書いた。ケース5の弱め方は §8 の判断材料5に置き、オーナーの判断待ちとした |
| 中 M5（`serialize-creators` の誤認と、I8 が確かめられない） | 直した | 直列化の検証が `actorOf` の場で効くことを §1 に正しく書
...[truncated 2089 chars]