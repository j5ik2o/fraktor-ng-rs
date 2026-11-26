# 設計：#195 DeployerSpec（deployment の読み取りと Deploy の引き当て）

[Issue #195](https://github.com/j5ik2o/fraktor-ng-rs/issues/195) の設計の記録である。Pekko v2.0.0 の actor-tests にある actor の `DeployerSpec` を Rust へ移植し、台帳の対象ノードを green にする。本書は第1段（設計）であり、実装は設計レビューが通ってから第2段で行う。本書の段では台帳を触らない。

原典（Pekko v2.0.0）:

- [DeployerSpec.scala](https://github.com/apache/pekko/blob/v2.0.0/actor-tests/src/test/scala/org/apache/pekko/actor/DeployerSpec.scala)
- 本体は [Deployer.scala](https://github.com/apache/pekko/blob/v2.0.0/actor/src/main/scala/org/apache/pekko/actor/Deployer.scala) の `Deploy`、`Scope`（`LocalScope`／`NoScopeGiven`）、`Deployer`（`deploy`、`lookup`、`parseConfig`、`createRouterConfig`、`routerTypeMapping`）
- ルーター側は [RouterConfig.scala](https://github.com/apache/pekko/blob/v2.0.0/actor/src/main/scala/org/apache/pekko/routing/RouterConfig.scala) の `RouterConfig`／`Pool`、各プールの `Config` コンストラクタ（`RoundRobinPool`、`RandomPool`、`ScatterGatherFirstCompletedPool`、`ConsistentHashingPool`）、[Resizer.scala](https://github.com/apache/pekko/blob/v2.0.0/actor/src/main/scala/org/apache/pekko/routing/Resizer.scala) の `DefaultResizer` と `Resizer.fromConfig`
- 既定値は `actor/src/main/resources/reference.conf` の `pekko.actor.deployment.default`（`router = "from-code"`、`nr-of-instances = 1`、`within = 5 seconds`、`virtual-nodes-factor = 10`、`resizer { enabled = off, lower-bound = 1, upper-bound = 10, pressure-threshold = 1, rampup-rate = 0.2, backoff-threshold = 0.3, backoff-rate = 0.1, messages-per-resize = 10 }`）と `pekko.actor.router.type-mapping` である

移植の規約は `.takt/facets/knowledge/fraktor-porting-rules.md` に従う。形の前例は [PR #205](https://github.com/j5ik2o/fraktor-ng-rs/pull/205) の `docs/design/issue-177-frequency-sketch.md` と `docs/goals/issue-177-frequency-sketch.md`、および直近の `docs/design/issue-192-specs.md`（特に HOCON を型付きの入力に写す忠実さの判断 E1／E2）である。方針は `docs/decisions/pekko-semantics-first.md`（速さより Pekko の意味論を優先する。JVM・Scala に固有の振る舞いは1件ずつオーナーに伺う）に従う。

作業場所は Mac mini の `~/muse-work/fraktor-ng-rs-195`、ブランチは `main` から切った `j5ik2o/issue-195-deployer` である。ほかのボットやワーカーの作業フォルダでは流さず、書き込みもしない。

## 1. 何を移植するか

### 件数の数え直し

原典の `"..." in { ... }` を数え直した。**目安16件と一致する。**

| Spec | issue／台帳の目安 | 数え直した実数 | 備考 |
|:---|---:|---:|:---|
| DeployerSpec | 16 | **16** | 一致。補助メソッド `assertRouting`／`assertNoRouting` はケースではない。`assertRouting` の呼び出しはケース9〜13と14・15の中にあり、件数は呼び出し回数ではなく `in` の個数で数える |

台帳の対象ノードは `actor-tests/src/test/scala/org/apache/pekko/actor/DeployerSpec.scala`（`status: unported`、`test_cases_approx: 16`、`needs.testkit: [extends-pekko-spec, pekko-spec]`）の1件である。第2段の台帳の `cases.total` には実数の16を記録する。16件すべてを移す設計であり（§5）、除外は0件である。

> 2026-11-26 修正：独立レビュー（Opus。高1・中7・低5）と上司の判断（F1 を採る、ケース16は除外せず型付きの対応で移す）を受けて本書を直した。指摘への対応は末尾の §8 回答表に1件ずつ書く。
> 2026-11-26 再修正：再レビュー（Opus。中3・低5）と、狭めと判断の11件について中将（2026-11-26）の決定を受けて本書を直した。対応は §9 に書く。

### ケースの分け方：設定の書式を見るか、読んだ結果の Deploy を見るか

DeployerSpec の大半は HOCON の `pekko.actor.deployment` の節を読むケースである。本書はケースごとに次の2つに分ける。

- **区分A（設定ファイルの書式そのものを見る）**: HOCON の文法、型の誤りの検出の仕方、短い名前からクラス名への対応表そのものを見るケースである。移植では HOCON を読まない。前例（#154 の D1、G0b の決定 E、#192 の E1／E2）どおり**型付きの入力に写す**。本件の区分Aはケース5とケース16の2件であり、どちらも忠実さの判断として写す形が決まった（§5 の F1、F3）。
- **区分B（読んだ結果の Deploy を見る）**: 節を読んだ結果として得られる `Deploy`（`router`、`dispatcher`、`mailbox`、`scope`、引き当てたパス）を見るケースである。こちらは**そのまま移す**。入力は HOCON の文字列ではなく、節1件分を表す型付きの値（§2.2 の `DeploymentSection`）で与え、原典の節と1対1に対応させる。

### 1.1 対応表

置き場所は `crates/fraktor-actor/tests/deployer_spec.rs`（ノード1件＝ファイル1件、冒頭に出典のパスをコメントで残す）。内部 API（§2 の `Deployer`）を直接呼ぶケースであるため、規約の例外（内部 API を呼ぶケースは単体テストに置き、台帳の `rust.cases_outside_test_file` に書く）に当たるかどうかは §6 に判断材料を書く。原典も `system.provider.deployer.lookup(...)` という内部 API を直接呼んでおり、アクターを1つも作らない。テストはシステムを立てず、`Deployer` の値を直接作って引く形を基本とする（§2.3）。

原典の設定（`DeployerSpec.deployerConf`）の節と、移植の入力値の対応は §1.2 にまとめる。各ケースが確かめる要点は次のとおりである。

| # | 原典のケース | Rust のテスト名 | 区分 | 移す／作らない |
|:---:|:---|:---|:---|:---|
| 1 | be able to parse 'pekko.actor.deployment._' with all default values（`/service1` の空の節） | `deployer_empty_section_yields_the_default_deploy` | B | 移す。`Deploy { path: "/service1", router: NoRouter相当, scope: NoScopeGiven, dispatcher: "", mailbox: "" }` と一致することを、保持した入力値（§2.2 の `config` 相当）を含めて確かめる |
| 2 | use None deployment for undefined service（`/undefined`） | `deployer_lookup_of_an_undefined_service_is_none` | B | 移す |
| 3 | be able to parse 'pekko.actor.deployment._' with dispatcher config（`/service3`、`dispatcher = my-dispatcher`） | `deployer_section_with_a_dispatcher_names_that_dispatcher` | B | 移す |
| 4 | be able to parse 'pekko.actor.deployment._' with mailbox config（`/service4`、`mailbox = my-mailbox`） | `deployer_section_with_a_mailbox_names_that_mailbox` | B | 移す |
| 5 | detect invalid number-of-instances（`nr-of-instances = boom` で `ConfigException.WrongType`） | `deployer_rejects_a_non_numeric_number_of_instances` | A | **型付きの入力に写して移す**（§5 の F1。上司の判断で採用が決まった）。HOCON の型検査そのものではなく、「数値でない `nr-of-instances` を、引き当て可能な `Deploy` にせず誤りとして返す」ことを確かめる。誤りになるのは件数を読む種類（全プール）だけであり、`from-code` とグループ系では誤りにならない（§2.6、I7）。数の範囲は §2.2（`i32`）に書く |
| 6 | detect invalid deployment path（`/gul/ubåt` で `InvalidActorNameException`。メッセージに `[ubåt]` と `[/gul/ubåt]` を含む） | `deployer_rejects_an_invalid_deployment_path` | B | 移す。パス要素の検証は既存の `ActorPath::validate_path_element` を使い、誤りの型は既存の `InvalidActorNameException` を使う。メッセージに要素と全体パスを含むことまで確かめる |
| 7 | be able to parse 'pekko.actor.deployment._' with from-code router（`/service-direct`） | `deployer_from_code_router_is_no_router` | B | 移す |
| 8 | ignore nr-of-instances with from-code router（`/service-direct2`、`nr-of-instances = 2`） | `deployer_from_code_router_ignores_the_number_of_instances` | B | 移す。`from-code` のときは `nr-of-instances` を読まず、結果がケース7と同じ `NoRouter` 相当になることを確かめる |
| 9 | be able to parse 'pekko.actor.deployment._' with round-robin router（`/service-round-robin`） | `deployer_round_robin_section_yields_a_round_robin_pool_of_one` | B | 移す。引き当てたパスが節のキー `/service-round-robin` であり、プールの種類と `resizer` が無いことを確かめる |
| 10 | be able to parse 'pekko.actor.deployment._' with random router（`/service-random`） | `deployer_random_section_yields_a_random_pool_of_one` | B | 移す（ケース9と同じ形） |
| 11 | be able to parse 'pekko.actor.deployment._' with scatter-gather router（`/service-scatter-gather`、`within = 2 seconds`） | `deployer_scatter_gather_section_yields_a_pool_with_a_two_second_within` | B | 移す。`within` が2秒であることまで確かめる |
| 12 | be able to parse 'pekko.actor.deployment._' with consistent-hashing router（`/service-consistent-hashing`） | `deployer_consistent_hashing_section_yields_a_consistent_hashing_pool_of_one` | B | 移す。ただし既存の `PoolKind` に consistent-hashing が無いため、§2.4 の追加を前提にする |
| 13 | be able to parse 'pekko.actor.deployment._' with router resizer（`/service-resizer`、round-robin、`lower-bound = 1`、`upper-bound = 10`） | `deployer_section_with_a_resizer_section_yields_a_pool_with_the_default_resizer` | B | 移す。原典の期待値は `RoundRobinPool(nrOfInstances = 0, resizer = Some(DefaultResizer()))` である。`nrOfInstances = 0` になる理由と、`DefaultResizer()` の既定値（下限1・上限10・残りは reference.conf の既定値）が節の指定（下限1・上限10）と一致する理由を §2.5 に書く。`resizer` の型は新規である |
| 14 | be able to use wildcards（`/some/*` と `/*/some`） | `deployer_single_wildcards_match_one_path_element` | B | 移す。`/some/wildcardmatch` が `/some/*`（random）へ、`/somewildcardmatch/some` が `/*/some`（scatter-gather、`within` は既定の5秒）へ引けることと、返る `Deploy` の `path` が**節のキー（パターン）そのもの**であることを確かめる |
| 15 | be able to use double wildcards（`/double/**` と `/double/more/**`） | `deployer_double_wildcards_match_the_remaining_path_elements` | B | 移す。`/double/wildcardmatch` と `/double/wildcardmatch/anothermatch` が `/double/**`（random）へ、`/double/more/anothermatch` が `/double/more/**`（round-robin）へ引けることと、`/double` 自体は引けない（`assertNoRouting`）ことを確かめる |
| 16 | have correct router mappings（短い名前13件からクラス名への対応） | `deployer_router_type_mapping_matches_the_original_table` | A | **型付きの対応に写して移す**（§5 の F3。上司の判断で除外の提案は取り下げた）。短い名前13件が、原典の期待どおりの FQCN を持つ選択値に写ることを、`RouterSelection::from_setting` と `RouterSelection::fqcn`（§2.6）で確かめる。#192 の `ProviderSelection`（定数の FQCN と `fqcn()`、`from_setting` が短い名前と FQCN の両方を受ける）と同じ形である。対応表自体は15件あり、ケースが比べるのはそのうち tail-chopping の2件を除く13件である |


原典より強く確かめる点（レビューの低の指摘）：原典の `assertRouting` が比べるのは、引き当てたパス、ルーターのクラス、scope、resizer だけである。本書のケース9〜12のテスト名にある「件数が1であること」、ケース11の「`within` が2秒であること」、ケース14の「`/*/some` の `within` が既定の5秒であること」は、原典より強い確認である。強める理由は、解釈した値が節と既定値から正しく作られていることを固定するためであり、表とテストのコメントに「原典より強い確認」と明記する。特にケース14では、原典の期待値の式が `within = 2.seconds` と書かれているが実際の値は既定の5秒であり、原典は比べていない。式をそのまま写すと失敗するため、期待値は5秒で書く。

### 1.2 原典の設定と移植の入力値の対応

原典の `deployerConf` の各節は、§2.2 の `DeploymentSection` の値1件に対応する。`default` の節は原典では `reference.conf` が与え、移植では `DeploymentSection::default()` 相当（§2.2 の既定値）が与える。

| 節のキー | 節の中身 | 対応するケース |
|:---|:---|:---|
| `/service1` | 空 | 1 |
| `/service-direct` | `router = from-code` | 7 |
| `/service-direct2` | `router = from-code`、`nr-of-instances = 2` | 8 |
| `/service3` | `dispatcher = my-dispatcher` | 3 |
| `/service4` | `mailbox = my-mailbox` | 4 |
| `/service-round-robin` | `router = round-robin-pool` | 9 |
| `/service-random` | `router = random-pool` | 10 |
| `/service-scatter-gather` | `router = scatter-gather-pool`、`within = 2 seconds` | 11 |
| `/service-consistent-hashing` | `router = consistent-hashing-pool` | 12 |
| `/service-resizer` | `router = round-robin-pool`、`resizer { lower-bound = 1, upper-bound = 10 }` | 13 |
| `/some/random-service` | `router = round-robin-pool` | 14・15の土台（完全一致がワイルドカードより優先されることは原典のケースでは直接確かめていない。§3 の不変条件I5として PBT 側で扱う） |
| `/some/*` | `router = random-pool` | 14 |
| `/*/some` | `router = scatter-gather-pool` | 14 |
| `/double/**` | `router = random-pool` | 15 |
| `/double/more/**` | `router = round-robin-pool` | 15 |

ケース5とケース6は `deployerConf` の外で、それぞれ別の設定を作ってシステムを立て直すケースである。移植ではシステムを立て直さず、誤りの入力を `Deployer` の構築／登録の入口へ直接渡し、返る誤りを確かめる形に写す。これは入口の違いであり、確かめる振る舞い（誤った節が引き当て可能な `Deploy` にならないこと）は変えない。

### 1.3 重なりと前提の切り分け

- **ルーターの振り分けそのもの（#269）とは重ならない。** DeployerSpec は `lookup` の結果の `Deploy` だけを見て、ルーターへメッセージを送らない。既存の `RouterConfig` の文書も「振り分けは [Issue #269](https://github.com/j5ik2o/fraktor-ng-rs/issues/269)」と切り分けている。本件はその切り分けを保ち、`Deploy` の中身の一致までで止める。プールの種類の値が #269 の実装と共有されるため、§2.4 の追加は #269 と同じ型を広げる形にし、別型を作らない。
- **#270 と重なるケースは、本書では作らない。** 指示で挙げられた後続 issue（#270）と重なるケースがある場合は第2段の着手前に照合し、重なったケースはそちらへ譲って本件では重ねて作らない。本書の16件の中では、ルーターの振る舞いそのものを見るケースは無く、直接の重なりは見当たらない。
- **複数スレッドの実行器・dispatcher の段（#234、段の issue #250〜#260）には依存しない。** DeployerSpec のケースは dispatcher を**名前の文字列**としてだけ見て、dispatcher の実体もスレッドも使わない。ケース3（`my-dispatcher`）も名前の一致だけである。したがって本件の16件はすべて「#234 の段の後」に回さず、先に進められる。逆に、引き当てた `Deploy` を実際の生成へつなぐ配線（`LocalActorRefProvider` が `lookup` の結果で `Props` を上書きする経路）は本件の範囲外であり、その配線が複数スレッドの実行器を前提にする場合はそちらで扱う。
- **`PoolKind` を広げることの接点（#269 と #194）**：振る舞いとしては #269 と重ならないが、`PoolKind` は `#[non_exhaustive]` の付いていない公開の列挙であり、種類を足すと接点が2つできる。(1) #269 の振り分けの `match` は新しい種類を扱わなければならなくなる。#269 の範囲は5種の振り分けであり、consistent-hashing の振り分けを #269 で作るか別の issue に回すかは、#269 の設計で決める事項として本書から申し送る。本件では振り分けを作らない。(2) #194 の `tests/actor_configuration_verification_property_test.rs` は `PoolKind` の全種を手で並べて組み合わせを確かめており、新しい種類はその表から黙って漏れる。第2段では、この性質の表へ新しい種類を足す作業を本件に含める（できれば網羅を `match` で強制する形に直す。§7）。また、種類を足した時点で `Props::with_router` に新しい種類を渡せるようになり、head だけの最小のルーターとして通る点も影響として記録する。
- **testkit への依存は無い。** 台帳の `needs.testkit` は `extends-pekko-spec`／`pekko-spec` だが、原典が `PekkoSpec` から使うのはシステムの保持だけであり、ケースは probe も latch も使わない。移植は `FraktorSpec` を使わず、`Deployer` を直接作る単体に近い形で足りる（§2.3）。この点は台帳の `needs` と実態の差として第2段で記録する。

## 2. 本体の有無と、新しく足すもの

`crates/fraktor-actor` と `crates/fraktor-testkit` を調べた結果は次のとおりである。

| 対象 | 既存の有無 |
|:---|:---|
| `Deploy`／`Scope`／`Deployer` | **無い。** `deploy`、`deployer`、`scope` を名前に含むファイルは `crates/` に無い。`Props` は `dispatcher`／`mailbox`／`router` の名前と値を保持するが、パスから引き当てる仕組みは無い |
| ルーターの設定値 | **部分的にある。** `kernel::actor::RouterConfig { pool: PoolKind, nr_of_instances: usize, router_dispatcher: Option<String> }` と `kernel::actor::PoolKind`（`RoundRobin`、`Broadcast`、`Random`、`SmallestMailbox`、`ScatterGatherFirstCompleted { within }`）が #194 で入っている。ただし形が原典と違う（§2.4）。`ConsistentHashing` と `Balancing`、グループ系、`resizer` は無い |
| ワイルドカードの引き当て | **ある。** `kernel::wildcard_index::WildcardIndex<T>` が Pekko の `org.apache.pekko.util.WildcardIndex` の移植であり、`insert`（不変。新しい索引を返す）と `find`（要素列から引く）を持ち、単一ワイルドカード、末尾の二重ワイルドカード、接頭辞パターンを原典の貪欲な一致で扱う。原典の `Deployer` が使う索引そのものである |
| パス要素の検証 | **ある。** `ActorPath::validate_path_element`（空、非 ASCII、禁止文字と位置を誤りの文で返す）と `ActorPath::is_valid_path_element`、`InvalidActorNameException::invalid_name`／`empty_name` がある。原典の `deploy` が投げる検証と同じ入口である |
| 設定の誤りの型 | **部分的にある。** `ConfigurationException`（設定の検証の誤り）がある。原典のケース5の `ConfigException.WrongType`（Typesafe Config の型）に対応する専用の型は無い（§2.6） |
| 型付きの設定値の前例 | **ある。** `DispatchSettings`（dispatcher／mailbox の定義と alias を型付きの値で与える。#192 の E2）、`ProviderSelection::from_setting`（設定文字列から選択値へ。#192 の E1）、`PoolKind` の文書（クラス名ではなく種類の列挙で選ぶ。#154 の D1）。本件の `DeploymentSection` はこの3つの前例に倣う |
| `ActorSystem` への配線 | **範囲外。** `ActorSystem` は `ActorSystemSetup`（`Setup` の集合）を受け取る。`Deployer` を `Setup` として載せ、生成時に引き当てる配線は本件では作らない（§2.3）。原典のケースは配線を経ず `deployer.lookup` を直接呼ぶため、配線が無くても16件の意味は確かめられる |

### 2.1 置き場所とファイルの分け方

原典の定義場所（`org.apache.pekko.actor`）に合わせ、公開面は `kernel::actor` の下に置く。syn チェッカーの type-per-file の決まりに従い、型ごとにファイルを分ける。

| ファイル | 中身 |
|:---|:---|
| `crates/fraktor-actor/src/kernel/actor/deploy.rs` | モジュールの宣言と再エクスポート |
| `crates/fraktor-actor/src/kernel/actor/deploy/deploy_value.rs` | `Deploy` |
| `crates/fraktor-actor/src/kernel/actor/deploy/scope.rs` | `Scope` |
| `crates/fraktor-actor/src/kernel/actor/deploy/deployment_section.rs` | `DeploymentSection`（§2.2） |
| `crates/fraktor-actor/src/kernel/actor/deploy/router_selection.rs` | `RouterSelection` と FQCN の定数（§2.2、§2.6） |
| `crates/fraktor-actor/src/kernel/actor/deploy/number_of_instances.rs` | `NumberOfInstances`（§2.2） |
| `crates/fraktor-actor/src/kernel/actor/deploy/resizer_section.rs` | `ResizerSection`（§2.2） |
| `crates/fraktor-actor/src/kernel/actor/deploy/deployed_router.rs` | `DeployedRouter`（§2.4） |
| `crates/fraktor-actor/src/kernel/actor/deploy/group_kind.rs` | `GroupKind`（§2.4） |
| `crates/fraktor-actor/src/kernel/actor/deploy/default_resizer.rs` | `DefaultResizer`（§2.4、§2.5） |
| `crates/fraktor-actor/src/kernel/actor/deploy/deployer_value.rs` | `Deployer` |
| `crates/fraktor-actor/src/kernel/actor/deploy/deploy_error.rs` | `DeployError`（§2.6） |
| `crates/fraktor-actor/src/kernel/actor/deploy/*_test.rs` | 原典外の単体テスト（境界の固定。§3） |
| `crates/fraktor-actor/tests/deployer_spec.rs` | 原典16件のうち移す分（§6 の置き場所の判断に従う） |
| `crates/fraktor-actor/tests/deployer_property_test.rs` | PBT（§3。原典外で、台帳の件数に含めない） |
| `ledger/nodes.yaml` | 対象ノードを green に（第2段） |

既存の `RouterConfig` と `PoolKind` への変更は §2.4 に限る。新しく足す型はすべて `core` と `alloc` だけで書ける（§2.7）。

### 2.2 型付きの入力：`DeploymentSection`

HOCON の節1件分を表す型付きの値である。原典の `Deployer.parseConfig(key, config)` が節と `default` の節を `withFallback` で重ねて読む読み方を、値の合成として写す。

```rust
/// HOCON の `pekko.actor.deployment` の節1件分を表す型付きの値。
/// Mirrors one section of `pekko.actor.deployment` together with the
/// `default` section it falls back to (reference.conf).
#[derive(Clone, PartialEq, Debug)]  // f64 を含むため Eq は導けない
pub struct DeploymentSection {
  pub router: RouterSelection,
  pub nr_of_instances: NumberOfInstances,
  pub within: Duration,              // 既定は 5 秒
  pub virtual_nodes_factor: u32,     // 既定は 10
  pub dispatcher: String,            // 既定は ""（Deploy::NoDispatcherGiven）
  pub mailbox: String,               // 既定は ""（Deploy::NoMailboxGiven）
  pub resizer: Option<ResizerSection>,  // Some であること自体が有効を表す（§2.5）
  pub pool_dispatcher_given: bool,   // 原典の `config.hasPath("pool-dispatcher")`
  pub routees_paths: Vec<String>,    // グループ系が読む `routees.paths`。既定は空
  pub tail_chopping_interval: Duration,  // `tail-chopping-router.interval`。既定は 10 ミリ秒
}

/// 節の `router` の値。原典は文字列であり、`router.type-mapping` で
/// クラス名へ写して反射で生成する。移植は種類を直接選ぶ
/// （#154 の D1、G0b の決定 E、#192 の E1 と同じ忠実さの判断）。
pub enum RouterSelection {
  FromCode,               // "from-code"
  RoundRobinPool,         // "round-robin-pool"
  RoundRobinGroup,        // "round-robin-group"
  RandomPool,             // "random-pool"
  RandomGroup,            // "random-group"
  BalancingPool,          // "balancing-pool"
  SmallestMailboxPool,    // "smallest-mailbox-pool"
  BroadcastPool,          // "broadcast-pool"
  BroadcastGroup,         // "broadcast-group"
  ScatterGatherPool,      // "scatter-gather-pool"
  ScatterGatherGroup,     // "scatter-gather-group"
  TailChoppingPool,       // "tail-chopping-pool"
  TailChoppingGroup,      // "tail-chopping-group"
  ConsistentHashingPool,  // "consistent-hashing-pool"
  ConsistentHashingGroup, // "consistent-hashing-group"
  Custom(String),         // それ以外の文字列（原典では FQCN として反射に渡る）
}

/// 節の `nr-of-instances` の値。ケース5（区分A）を型で表すための形。
/// 原典の `getInt` は符号つき32ビットであり、負数を受ける
/// （`nr-of-instances = -1` で `RoundRobinPool(-1)` になる。再レビューが
/// 原典の jar で確かめた）。結果の `DeployedRouter::Pool` の件数も
/// `i32` で保持し、-1 をそのまま持てる（再レビュー中2）。
/// 範囲を超える値（`3000000000`）は原典では `ConfigException.WrongType`
/// （out-of-range）になる。`Value(i32)` では作れないため、範囲外の値は
/// `NotANumber` で渡す。すなわち `NotANumber` が表すのは「`i32` に
/// 直せない値」であり、数でない文字列（"boom"）と範囲外の数の
/// 両方を含む。HOCON の文字列 "2" は原典で数として読めるが、
/// 型付きの入力では `Value(2)` で渡す。
/// 移植では未解釈の値を保持する変種を用意し、解釈の段で
/// 誤りにする（§2.6）。
pub enum NumberOfInstances {
  Value(i32),
  NotANumber(String),
}

型で表せない値（入力の型による狭め。中将（2026-11-26）が意図した狭めとして決定した）：負の `within`（`Duration` で表す）と、resizer の負の下限と上限（`u32` で表す）は、型付きの入力では利用者がそもそも書けない。原典ではどちらも誤りになる値だが、入口が型である以上、観察できる入力としては存在しない。件数だけを原典の `Int` に合わせて `i32` にした理由は、ケース5と F1 の条件が件数の範囲を検証の対象にしているためである。

pub struct ResizerSection {
  // `enabled` の欄は持たない。節に resizer があれば常に有効である（§2.5）。
  pub lower_bound: u32,         // 既定は 1
  pub upper_bound: u32,         // 既定は 10
  pub pressure_threshold: u32,  // 既定は 1
  pub rampup_rate: f64,         // 既定は 0.2
  pub backoff_threshold: f64,   // 既定は 0.3
  pub backoff_rate: f64,        // 既定は 0.1
  pub messages_per_resize: u32, // 既定は 10
}
```

設計上の決めは次の3点である。

1. **`default` の節は型の既定値として持ち、`default` のキーの上書きは扱わない。** 原典は節ごとに `config.withFallback(default)` をし、利用者が `pekko.actor.deployment.default` を上書きすると全部の節に効く。移植の `DeploymentSection` はすべての項目が値を持つ型であり、どの項目を節が書いたのかを区別できないため、利用者が渡した `default` の節を他の節の土台として重ねる方法が無い。そこで本書は、`from_sections` で `default` のキーは読み飛ばすだけにし、既定の上書きは扱わないと決める（レビュー中3の推奨 (a)）。これは HOCON の設定の体系の狭めであり、§5 の F2 に挙げる。ケースは既定の上書きを使わず、G0b の決定 E（設定の体系は作らない）とも合う。移植では `DeploymentSection::default()` が reference.conf の `default` と同じ値を持ち、節を作る側は変える項目だけを上書きする。原典の「節に無い項目は既定から来る」という振る舞いは、既定値を持つフィールドとして写る。なお、`from_sections` に同じキーを2回渡すと後のものが丸ごと勝つ。HOCON では同じキーの節は項目ごとに合成されるため、この入力は原典の HOCON からは作れない入力であり、ずれとして記録する。
2. **`router` は文字列ではなく `RouterSelection` で選ぶ。** 原典の `createRouterConfig` は `routerTypeMapping` で短い名前をクラス名へ写し、`DynamicAccess` の反射で `RouterConfig` を生成する。Rust には反射による生成が無い。種類を列挙で選ぶ形は `PoolKind` の前例どおりである。短い名前と組み込みの FQCN は `RouterSelection::from_setting`（§2.6）で選択値に写り、知らない名前と組み込みでない FQCN だけが `Custom(String)` に保持され、解釈の段で誤りになる（原典では反射の失敗が `IllegalArgumentException` になる）。`Custom` を黙って `FromCode` に丸めない。
3. **グループ系と tail-chopping と balancing も値として保持する。** 原典の `createRouterConfig` は名前で分岐せず反射に渡すため、グループ系の節も `Deploy` になる。入口で拒むと原典より狭くなるため、選択値としては全15件を受け、生成も値の保持まで行う（レビュー中4を受けて、保持に必要な欄と種類を足した）。原典の `Config` コンストラクタが読む項目は種類ごとに違い、グループ系は `routees.paths`、tail-chopping は `within` と `tail-chopping-router.interval`、balancing は `nr-of-instances` だけを読む。`DeploymentSection` には `routees_paths` と `tail_chopping_interval` を足し（§2.2）、`PoolKind` には `Balancing` と `TailChopping` を足す（§2.4）。件数を読むのはプール系だけであり、グループ系は `nr-of-instances` を読まない（§2.6 の規則）。`Custom`（知らない名前と、組み込みでない FQCN。NoRouter の FQCN を含む）だけは保持できず、生成の段で `DeployError::UnsupportedRouter` を返す。これは反射生成を作らない範囲の帰結であり、組み込みの FQCN のうち14件は `Custom` にしない（NoRouter の FQCN の扱いは §2.6）。


### 2.3 結果の型：`Deploy`、`Scope`、`Deployer`

```rust
/// Mirrors `org.apache.pekko.actor.Deploy`.
#[derive(Clone, PartialEq, Debug)]
pub struct Deploy {
  pub path: String,                 // 節のキーそのもの（ワイルドカードを含む）
  pub section: DeploymentSection,   // 原典の `config`（default と合成済みの節）
  pub router: DeployedRouter,       // §2.4
  pub scope: Scope,
  pub dispatcher: String,           // "" が NoDispatcherGiven
  pub mailbox: String,              // "" が NoMailboxGiven
  // 原典の `tags` は本件のケースに出ないため持たない（§6）。
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Scope {
  NoScopeGiven,  // withFallback は相手を返す
  Local,         // withFallback は自分を返す
}

impl Scope {
  pub fn with_fallback(self, other: Scope) -> Scope;
}

/// Mirrors `org.apache.pekko.actor.Deployer` narrowed to construction
/// from typed sections, `deploy`, and `lookup`.
pub struct Deployer {
  index: WildcardIndex<Deploy>,
  // 名前の対応は値を持たず、`RouterSelection::from_setting` と
  // `fqcn` の関数として置く（§2.6）。
}

impl Deployer {
  /// 節のキーと値の列から作る。原典の構築子が
  /// `pekko.actor.deployment` の各節を `parseConfig` して
  /// `deploy` する流れの写し。キーが "default" の節は読み飛ばし、
  /// 引き当ての対象にしない（原典の構築子の分岐どおり。
  /// 既定の上書きを扱わない点は §2.2 の決め1）。
  /// 注意（分け方の罠）：キーは Java の `split` の規則で分けるため、
  /// 先頭に `/` の無いキー（"a/b"）は `/b` に縮み、要素が1つだけの
  /// キー（"svc"）は要素列が空になって黙って登録されない（§2.3）。
  /// これは原典どおりの挙動であり、誤りにはならない。
  pub fn from_sections(
    sections: impl IntoIterator<Item = (String, DeploymentSection)>,
  ) -> Result<Self, DeployError>;

  /// 既に解釈済みの `Deploy` を登録する。原典の `deploy` の写し。
  /// パスの各要素を検証し、空の要素と不正な要素は誤りにする。
  pub fn deploy(&mut self, deploy: Deploy) -> Result<(), DeployError>;

  /// 要素列で引く。原典の `lookup(path: Iterable[String])` の写し。
  pub fn lookup(&self, elements: &[&str]) -> Option<&Deploy>;

  /// `/` で始まるパス文字列で引く補助。原典のテストが
  /// `service.split("/").drop(1)` で要素列を作る手続きの写しであり、
  /// テスト側に分割の手続きを重ねて書かせないために置く。
  pub fn lookup_path(&self, path: &str) -> Option<&Deploy>;
}
```

パスの分け方は、専用の関数1つに固定する（レビュー中1）。原典の `deploy` は `d.path.split("/").drop(1)`、テストは `service.split("/").drop(1)` で要素列を作る。Java の `String.split` は末尾の空文字列を捨てるため、Rust の `str::split('/')` と結果が違う。移植は Java の規則（末尾の空文字列を捨て、先頭の1要素を落とす）を写した関数を用意し、`deploy` と `lookup_path` の両方で使う。

```rust
/// Java の `path.split("/").drop(1)` の写し。
/// 末尾の空文字列は捨てる。先頭の要素は落とす。
fn split_deployment_path(path: &str) -> Vec<String>;
```

この分け方による期待値は次のとおりであり、§3 の壊しうる入力にも同じ表を置く。

| キー | 要素列 | 結果 |
|:---|:---|:---|
| `"/service/"` | `[service]` | 誤りにならない。`/service` で引け、返る `path` は `"/service/"` |
| `"/"` | `[]` | 索引への挿入が何もせず、黙って登録されない |
| `""` | `[]` | 同上 |
| `"service"`（先頭に `/` が無い） | `[]` | 同上（黙って捨てる） |
| `"a/b"` | `[b]` | 先頭の要素が消え、`/b` で引ける |
| `"//a"` | `["", a]` | 空の要素の誤り（`Actor name in deployment [//a] must not be empty`） |

この分け方は JVM に固有の振る舞いである。写すか、先頭の `/` の無いキーなどを誤りにする狭めにするかは、`docs/decisions/pekko-semantics-first.md` の「JVM・Scala に固有の振る舞いは1件ずつオーナーに伺う」の対象として §6 に挙げる。本書の実装案は原典どおり写す形で書き、判断はオーナーに残す。

`Deploy` と `DeploymentSection` は `f64` を含むため、`Eq` は導けず `PartialEq` だけを導く（§2.2 の derive の行どおり）。

原典の `lookup(path: ActorPath)`（`path.elements.drop(1)` で guardian を落とし、キーは guardian からの相対になる）は本件では作らない。生成への配線の段で必要になることを、範囲外としてここに記録する。

原典との違いは3点ある。

1. **不変の索引と `&mut self` の違い。** 原典は `AtomicReference<WildcardIndex[Deploy]>` を CAS で差し替える。移植の `WildcardIndex::insert` は新しい索引を返す不変の更新であり、`Deployer` はそれを `&mut self` で保持する形で足りる。ケースは単一スレッドで引き当てだけを見るため、原子的な差し替えは意味に出ない。複数スレッドから `deploy` する配線が必要になる段で、共有の形はそちらが決める。本件で `Arc` やロックを持ち込まない。
2. **`settings` と `dynamicAccess` を持たない。** 原典の構築子は `ActorSystem.Settings` から設定全体を受け、`DynamicAccess` でルーターを反射生成する。移植は節の列だけを受け、生成は §2.4 の決められた対応でする。設定全体の読み取りと反射は JVM／Typesafe Config に固有であり、型付きの入力に写す範囲の外である。
3. **`Deploy.config` は `DeploymentSection` として保持する。** 原典のケース1は `deployment.get.config` を期待値の `Deploy` の `config` と比べる（同じ節から読んだ値同士の一致）。移植でも、保持した節の値をそのまま比べることで同じ意味を確かめる。HOCON の `Config` の等価性（書式の違いを吸収する等しさ）は写らない。これは §5 の F1 と同じ忠実さの判断の範囲である。

`withFallback`（`Deploy` と `Scope` と `RouterConfig` の合成）は本件のケースに出ないため、本書では `Scope::with_fallback` だけを形として置き、`Deploy::with_fallback` は作らない。必要になる段（コード側の `Deploy` と設定側の `Deploy` を重ねる配線の段）で、原典の優先順位（`dispatcher`／`mailbox` は空でない側、`scope` は `Scope::with_fallback`、それ以外は左を優先する）をそのまま写して足す。

### 2.4 ルーターの値：既存の `RouterConfig` との関係

原典の `assertRouting` は3点を見る。(1) 引き当てた `Deploy` の `path` が期待する節のキーであること、(2) `routerConfig` の**クラス**が期待するプールのクラスであること、(3) `scope` が `NoScopeGiven` であること。プールについては `resizer` だけを追加で比べる。`nrOfInstances` は `assertRouting` では比べていない（ケース13の期待値にだけ現れる）。

既存の `RouterConfig { pool, nr_of_instances, router_dispatcher }` は #194 の検証規則のために狭めた形であり、DeployerSpec が必要とする次の情報を持たない。

- プールでないこと（`NoRouter`）と、プールであることの区別。現在は `pool: PoolKind` が必須であり、`NoRouter` を表せない。
- `ConsistentHashing` の種類と `virtual-nodes-factor`。
- `resizer` の有無と値。
- グループ系（パスで振り分ける）の種類。

本書の案は、既存の `RouterConfig` を壊さず、`Deploy` 側に次の列挙を新しく置くことである。

```rust
/// `Deploy` が保持するルーターの解釈結果。
/// Mirrors the `routerConfig` of a `Deploy` produced by
/// `Deployer.createRouterConfig`.
#[derive(Clone, PartialEq, Debug)]
pub enum DeployedRouter {
  /// `router = from-code`。原典の `NoRouter`。
  NoRouter,
  /// プール。原典の `Pool` の各実装。
  Pool {
    kind: PoolKind,                 // §2.4 の追加分を含む
    nr_of_instances: i32,           // 原典の `Int` のまま（-1 を保持できる。中2）
    resizer: Option<DefaultResizer>,
    use_pool_dispatcher: bool,      // balancing では常に false（§2.5）
  },
  /// グループ。原典の `Group` の各実装。経路の列は節の
  /// `routees.paths` から来る。
  Group { kind: GroupKind, paths: Vec<String> },
}

/// グループ系の種類。原典の `Group` の各実装に対応する。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GroupKind {
  RoundRobin,
  Random,
  Broadcast,
  ScatterGatherFirstCompleted { within: Duration },
  TailChopping { within: Duration, interval: Duration },
  /// 値は常に 0。原典の `ConsistentHashingGroup` の `Config`
  /// コンストラクタは `routees.paths` しか読まない（低2）。
  ConsistentHashing { virtual_nodes_factor: u32 },
}

/// Mirrors `org.apache.pekko.routing.DefaultResizer` as a value.
/// 動的な大きさ変更の振る舞い（`isTimeForResize`／`resize`）は
/// 本件では作らない。値の保持と検証だけである（§2.5）。
#[derive(Clone, PartialEq, Debug)]
pub struct DefaultResizer {
  pub lower_bound: u32,
  pub upper_bound: u32,
  pub pressure_threshold: u32,
  pub rampup_rate: f64,
  pub backoff_threshold: f64,
  pub backoff_rate: f64,
  pub messages_per_resize: u32,
}
impl DefaultResizer {
  pub fn new(/* 上の7値 */) -> Result<Self, DeployError>;  // 原典の構築時の検証
  pub fn defaults() -> Self;  // lower 1, upper 10, pressure 1, 0.2, 0.3, 0.1, 10
}
```

`PoolKind` への追加は次の3点である。

- `ConsistentHashing { virtual_nodes_factor: u32 }` を足す。原典の `ConsistentHashingPool` の `virtualNodesFactor` は `Config` コンストラクタでは読まれず（`nr-of-instances` と `resizer` と `pool-dispatcher` の有無だけを読む）、既定の `0` のままである。reference.conf の `virtual-nodes-factor = 10` は、このコンストラクタでは使われない。したがってケース12の期待値は `virtualNodesFactor = 0` であり、節の `virtual_nodes_factor` を写すと原典とずれる。この点は独立レビューが原典（`ConsistentHashing.scala`）で裏取り済みであり、確定として書く。
- `Balancing` を足す。原典の `BalancingPool` の `Config` コンストラクタは `nr-of-instances` だけを読む。
- `TailChopping { within: Duration, interval: Duration }` を足す。原典の `TailChoppingPool` の `Config` コンストラクタは `within` と `tail-chopping-router.interval` を読む。

既存の `RouterConfig`（#194）と `DeployedRouter` が二重になる点は否めない。統合の案は、`Props::with_router` が取る型を将来 `DeployedRouter` 側へ寄せることだが、#194 の検証規則と `ActorConfigurationVerificationSpec` の移植が既存の形に依存しているため、本件では触らない。二重である理由と統合の置き場所を本書に残し、統合そのものは別の issue で扱う提案とする。

`f64` の率（`rampup_rate` など）は `PartialEq` で比べられるが、`NaN` を含む入力では等価性が崩れる。原典の `DefaultResizer` は率の範囲を検証する（負の率は誤り、`backoffThreshold > 1.0` は誤り）が `NaN` は素通りする。移植も同じ検証に留め、`NaN` を特別扱いしない（§3 の壊しうる入力に挙げる）。

### 2.5 ケース13の `nrOfInstances = 0` と resizer の解釈

原典の期待値は `RoundRobinPool(nrOfInstances = 0, resizer = Some(DefaultResizer()))` である。理由は2段ある。

1. `RoundRobinPool` の `Config` コンストラクタは `nr-of-instances` を読むが、`Deployer.createRouterConfig` は節に `resizer` があり `resizer.enabled` が偽なら、前に `resizer.enabled = on` を重ねた設定で生成する（原典の後方互換の処理）。節 `/service-resizer` は `resizer { lower-bound = 1, upper-bound = 10 }` を持ち `enabled` を書かないため、この重ねが効いて resizer が有効になる。
2. `assertRouting` は `nrOfInstances` を比べないため、期待値の `0` は「resizer があるプールでは初期件数が意味を持たない」という原典側の書き方であり、検証されるのはクラスと `resizer` の値だけである。

移植では、**節に `resizer` が少しでもあれば常に有効**として解釈する規則を明示する（レビュー中2で訂正）。原典の条件は `config.hasPath("resizer") && !deployment.getBoolean("resizer.enabled")` のときに `resizer.enabled = on` を重ねるものであり、`enabled = off` と明示してあっても有効になる。「既定を真にする」ではない。したがって `ResizerSection` は `enabled` の欄を持たず、`resizer: Option<ResizerSection>` が `Some` であること自体が有効を表す。壊しうる入力の例は、`resizer { enabled = off, lower-bound = 2 }` を持つ round-robin-pool の節であり、原典では `Some(DefaultResizer(lowerBound = 2))` になる。`nr_of_instances` は節の値（既定1）を保持する。期待値の `0` は写さない。写すと、節に書かれていない値を結果に作ることになるためである。この点は原典との見かけの差として、テストのコメントと本書に理由を残す。`DefaultResizer()` の既定値（下限1・上限10・圧力1・rampup 0.2・backoff 0.3・backoff rate 0.1・messages-per-resize 10）は reference.conf の `resizer` の既定値と一致し、節の指定（下限1・上限10）とも一致するため、ケース13は「節で一部を指定し、残りは既定値で埋まった resizer」を確かめるケースとして移せる。

resizer の**振る舞い**（メッセージ数による判定、プールの大きさの増減）は本件では作らない。作るのは値の解釈と検証だけであり、これは原典の `DefaultResizer` の構築時の検証の写しである。

**resizer を読む種類は限られる（再レビュー中1）。** resizer を読むのは、balancing を除くプール（round-robin、random、smallest-mailbox、broadcast、scatter-gather、tail-chopping、consistent-hashing）だけである。原典の `BalancingPool` の `Config` コンストラクタは `nr-of-instances` しか読まず、グループ系と `from-code` も resizer を読まない。そのため、balancing、グループ、from-code の節に壊れた resizer（下限が上限より大きいなど）を書いても、原典では誤りにならず、結果の resizer は付かない。`DefaultResizer::new` の検証（`DeployError::InvalidResizer`）は、resizer を読む種類にだけかける。検証の内容は、上限が下限以上であること、率の範囲、`messages_per_resize > 0` である（下限と上限の「非負」の検証は、型で表せないため書かない。§2.2）。あわせて、`use_pool_dispatcher` は balancing では常に `false` になる（原典の `BalancingPool` は `pool-dispatcher` を読まない）。

`Resizer.fromConfig` のもう一方の分岐である `optimal-size-exploring-resizer` は本件では作らない。原典では `DefaultResizer` と両方が有効だと `ResizerInitializationException` になる点も、範囲外として記録する。

### 2.6 誤りの型と、名前の対応表

```rust
pub enum DeployError {
  /// 原典の `deploy` が投げる `InvalidActorNameException`。
  InvalidName(InvalidActorNameException),
  /// 索引への挿入の誤り。原典では `IllegalArgumentException` が
  /// 出て、システムの起動が失敗する（レビュー中7）。
  Index(WildcardIndexError),
  /// 原典の `ConfigException.WrongType`（ケース5）。
  WrongType { key: &'static str, value: String },
  /// 原典では反射の失敗による `IllegalArgumentException`
  /// （"Cannot instantiate router [...]")。
  UnsupportedRouter { selection: String, key: String },
  /// 原典の `DefaultResizer` の構築時の `IllegalArgumentException`。
  InvalidResizer { reason: String },
}
```

誤りの出る順序は原典どおり、**名前の検証のあとに索引への挿入**である。`"***"` と末尾が `"**"` で終わる要素（`"a**"`）は、名前の検証（`*` は `ActorPath` の有効な記号に含まれる）を通り、挿入の段で `DeployError::Index(WildcardIndexError::DoubleWildcardAsSuffix)` になる。途中の `"**"`（`/a/**/b`）は誤りにならず通常の子になり、`/a/x/b` は引けず、`/a/**/b` そのものは引ける。これらを期待値として §3 に書く。

- ケース6は `DeployError::InvalidName` を返し、中身の文が要素 `[ubåt]` と全体パス `[/gul/ubåt]` を含むことを確かめる。文の組み立ては `Deployer` 側で付け足さず、原典の2引数の `validatePathElement(element, fullPath)` を `ActorPath` 側に写す形にする（レビュー低2）。原典の文は `Invalid actor path element [ubåt] (in path [/gul/ubåt]), illegal character [å] at position: 2. ...` であり、全体のパスが文の途中に入る。空の要素の文は `Actor name in deployment [<path>] must not be empty` であり、既存の `InvalidActorNameException::empty_name()` の文（`Children.checkName` の文）とは違うため、deployment 用の文を `Deployer` の検証で作る。
- ケース5は `NumberOfInstances::NotANumber("boom")` を解釈する段で `DeployError::WrongType` を返す。**誤りになるのは件数を読む種類（全プール）だけ**であり、`from-code` とグループ系は件数を読まないため、同じ値でも誤りにならない（I7 と対になる規則）。Typesafe Config の例外型そのものは写らない（区分A。§5 の F1）。
- `routerTypeMapping` は、原典では `pekko.actor.router.type-mapping` の節を `Map[String, String]` として公開する値である。移植では文字列の表ではなく、型付きの対応として公開する（ケース16。§5 の F3）。

```rust
/// 短い名前15件と、組み込みの FQCN 14件を受ける。
/// 原典の `routerTypeMapping.getOrElse(routerType, routerType)` の
/// 写しであり、組み込みの FQCN で書いても同じ選択値になる。
/// それ以外は `Custom` に保持する。NoRouter の FQCN は例外であり、
/// `Custom` に保持する（下記）。
pub fn from_setting(value: &str) -> RouterSelection;
/// 選択値の FQCN。`Custom` は保持した文字列をそのまま返す。
/// `FromCode` は "org.apache.pekko.routing.NoRouter" を返す。
pub fn fqcn(&self) -> &str;
```

FQCN は定数として持つ（例：`ROUND_ROBIN_POOL_FQCN = "org.apache.pekko.routing.RoundRobinPool"`）。#192 の `ProviderSelection` と同じ形であり、組み込みの FQCN を `Custom` にして狭めることを避ける。

**NoRouter の FQCN は受ける側に入れない（再レビュー中3）。** 原典の `createRouterConfig` が `NoRouter` を返すのは文字列が `"from-code"` のときだけであり、`router = "org.apache.pekko.routing.NoRouter"` は反射に回って生成に失敗し、`IllegalArgumentException` でシステムが起動しない（再レビューが原典の jar で確かめた）。そのため `from_setting("org.apache.pekko.routing.NoRouter")` は `Custom` を返し、生成の段で `UnsupportedRouter` になる。一方で `fqcn(FromCode)` は、ケース16のために `"org.apache.pekko.routing.NoRouter"` を返したままにする。したがって往復の性質 `from_setting(x.fqcn()) == x` は、`FromCode` だけ成り立たない。性質として書くときは `FromCode` を外すと明記する。


### 2.7 no_std で書けるか

書ける。`Deployer`、`Deploy`、`DeploymentSection`、`DeployedRouter`、`DefaultResizer` はすべて値と文字列と `WildcardIndex`（`alloc` の `BTreeMap` を使う既存の型）だけであり、時刻は `core::time::Duration`、率は `f64` である。ファイル読み取り、環境変数、スレッド、乱数は使わない。`fraktor-actor` の kernel は `#![cfg_attr(not(feature = "std"), no_std)]` の下にあり、新規ファイルは `alloc::string::String`、`alloc::vec::Vec`、`core::time::Duration` の範囲で書く。テストファイルと testkit は `std` 側であり、本件は testkit を使わない。

## 3. 守る性質（不変条件）と、壊しうる入力

### 不変条件

- **I1（引き当ての決定性）**: 同じ節の列から作った `Deployer` は、同じ要素列に対して常に同じ `Deploy` を返す。登録の順序が結果を変えるのは、同じキーへ重ねて登録した場合だけであり、その場合は後の登録が勝つ（原典の `WildcardIndex.insert` の上書き）。
- **I2（未定義は None。ただし一致の判定は I10 の貪欲な進み方でする）**: 登録されていないパスは `None` である。空の `Deployer` はすべてに `None` を返す。ただし「登録されたパターンのどれかに一致するパスは `Some`」は成り立たない。引き当ては後戻りしないため（I10）、素朴には一致して見えるパスでも `None` になることがある。
- **I3（返るパスは節のキー）**: `lookup` が返す `Deploy.path` は、引いたパスではなく、一致した節のキー（ワイルドカードを含む）そのものである。ケース14・15が確かめる中心の性質である。
- **I4（scope は常に NoScopeGiven）**: 設定から作る `Deploy` の `scope` は常に `NoScopeGiven` である（原典の `parseConfig` が固定で渡す）。
- **I5（完全一致の優先は段ごと）**: 各段では完全一致の子が `"*"` の子と接頭辞の子より優先される。ただし要素列全体で「完全一致する節が必ず勝つ」わけではない。完全一致の子を選んだ先で行き止まると、戻らずに `None` になる（I10 の反例4・5）。`/some/random-service` の節が土台にあるのはこの段ごとの優先による。
- **I6（二重ワイルドカードの範囲。その節だけを登録した場合）**: 末尾の `"**"` の節だけを登録した `Deployer` では、その節は接頭辞に1要素以上を足した要素列に一致し、接頭辞そのものには一致しない。`/double` が `/double/**` に一致しないことが境界である。他の節と一緒では、I10 のため一致しないことがある（I10 の反例1）。末尾でない位置の `"**"` は通常の子として扱われる（既存の `WildcardIndex` の文書どおり）。
- **I10（後戻りしない貪欲な一致）**: 引き当ては段ごとに、完全一致の子、`"*"` の子、接頭辞の子の順で1本だけを選び、一度選んだら戻らない。接頭辞の子の順序は単一ワイルドカードの木だけの話であり、二重ワイルドカードの木は接頭辞の子を見ない（#187 の W3）。単一ワイルドカードの木で着いた節に子があれば、そこに値が無くても二重ワイルドカードの木へは回らない。原典の `WildcardTree.findWithSingleWildcard` と移植済みの `WildcardIndex::find` の形であり、本件はこの規則を変えない。反例（独立レビューが原典と移植済みの索引で試作して確かめた）は次のとおりである。

  | 節のキー | 引くパス | 結果 |
  |:---|:---|:---|
  | `/a/**`、`/a/b/c` | `/a/b` | **None**（`/a/**` に一致するはずが、単一の木の節 `b` が受け止める） |
  | `/a/**`、`/**` | `/a` | **None**（`/**` だけなら `Some`） |
  | `/a/**`、`/*/b` | `/a/b` | `Some("/*/b")`（単一の木が二重の木に勝つ） |
  | `/a/b/c`、`/*/b/d` | `/a/b/d` | **None**（`a` を選んだあと `*` へ戻らない） |
  | `/some/x/y`、`/some/*` | `/some/x` | **None** |
  | `/double/**`、`/double/more/**` | `/double/more/x` | `Some("/double/more/**")` |
- **I7（from-code は件数を読まない）**: `router = from-code` の節は `nr-of-instances` の値に関わらず `NoRouter` になり、数値でない値でも誤りにならない。原典の `createRouterConfig` が `from-code` を先に分岐し、プールのコンストラクタを通らないためである。ケース8が固定し、ケース5との対（プールの節では誤りになる）で意味を持つ。
- **I8（空と不正な要素の拒否）**: パスの要素に空文字、非 ASCII、禁止文字を含む節は登録できず、誤りを返す。登録に失敗した節は引き当て可能にならない（失敗が索引を壊さない）。
- **I9（既定値の充填）**: 節に無い項目は reference.conf の `default` の値になる。`dispatcher` と `mailbox` は空文字、`router` は `from-code`、`nr-of-instances` は1、`within` は5秒である。

### 壊しうる入力

- パスの分け方（§2.3 の表が期待値である）：`"/service/"` は誤りにならず `[service]` になる。`"/"`、`""`、先頭に `/` の無い `"service"` は要素列が空になり、黙って登録されない。`"a/b"` は `[b]` に縮む。`"//a"` は途中の空の要素で誤りになる。
- ワイルドカードの連続と組み合わせ：`"***"` と `"a**"` は挿入の誤り（`DeployError::Index`）になる。`"**"` の途中への配置（`/a/**/b`）は誤りにならず通常の子になる。`"*"` だけの節、接頭辞パターン（`"a*"`）と `"*"` の競合、同じパターンの重ねての登録。
- 極端な長さ：要素数の多いパスと長い要素名。深さについては、既存の `WildcardIndex` が再帰する `Clone` と `Drop` を持ち、2 MB のスタックでは深さ2000でプロセスが止まる性質が既にある（#187 の既存の性質。原典も再帰する挿入で `StackOverflowError` になる）。本件では直さず、PBT の生成器の深さに上限32を付ける。
- 境界の数値：`nr-of-instances = 0`（原典のプールは受け付ける。ケース13の期待値が示す）、`nr-of-instances = -1`（原典の `getInt` は受ける。`i32` で表す）、`i32` の範囲を超える件数（原典では `WrongType`）、`within = 0` 秒、極端に大きな `within`、`virtual-nodes-factor = 0`。
- resizer の境界：下限が上限より大きい、下限・上限が0、率が負、率が1を超える、`NaN` の率、`messages-per-resize = 0`、`enabled = off` の明示（それでも有効になる。§2.5）。
- 文字の境界：非 ASCII の要素（`ubåt`）、`$` で始まる要素、`%` のエスケープを含む要素、空白だけの要素。
- 名前の境界：大文字小文字だけが違うルーター名、空白を含むルーター名、空文字のルーター名、組み込みの FQCN で書いたルーター名（同じ選択値になる。§2.6）。

### PBT を足すか

**足す。** 置き場所は `crates/fraktor-actor/tests/deployer_property_test.rs` で、原典外の件として台帳の件数に含めない。対象は次の8性質である。

- **P1（差分検査）**: 任意のキーの列から作った `Deployer` について、`lookup_path(q).map(|d| &d.path)` が、同じキーを同じ分け方で入れた `WildcardIndex<String>` の `find` と常に等しい。照合の規則は #187 で `wildcard_index_model_test.rs` に固定済みであり、本件で確かめるのは「`Deployer` が索引を正しく使っていること（分け方、返す値、I3）」である。あわせて、返る `path` は登録したキーのどれかであることを確かめる。
- **P2**: 任意の要素列について、完全一致の節だけを登録した `Deployer` では、その要素列の引き当てがその節になる。
- **P3**: 末尾 `"**"` の節だけを登録した `Deployer` では、その節は接頭辞に1要素以上を足したすべての要素列に一致し、接頭辞そのものには一致しない（I6 の範囲どおり。他の節と一緒の場合は P1 の差分検査が受け持つ）。ただし接頭辞は接頭辞パターン（`x*` の形の要素）を含まない場合に限る。二重ワイルドカードの木は接頭辞の子を見ないため、`"/a*/**"` だけを登録しても `"/ab/x"` は引けない（再レビュー低1）。
- **P4**: 不正な要素を含むキーは常に `Err` になり、その後の `lookup` の結果は登録前と変わらない（I8）。生成器には `"***"`、`"a**"`、途中の空の要素（`"//a"`）を含める。
- **P5**: 任意の文字列 `s` について、`lookup_path(s)` が `lookup(&split_deployment_path(s))` と等しい（中1）。
- **P6**: 任意の `NumberOfInstances`（`NotANumber` を含む）について、`FromCode` の節とグループ系の節は誤りにならず、前者は `NoRouter`、後者はグループの値になる（I7 と §2.6 の規則）。
- **P7**: resizer を読む種類（balancing を除くプール）の節に resizer があれば、`enabled` の有無と値によらず、検証を通る値なら解釈結果の resizer は `Some` になり、通らない値なら `InvalidResizer` になる（中2。再レビュー中1で範囲を絞り、再々レビューで検証の条件を明記した）。
- **P8**: balancing、グループ、`from-code` の節では、resizer の節があっても（壊れた値でも）結果の resizer は付かず、誤りにもならない。balancing では `use_pool_dispatcher` も常に `false` になる（再レビュー中1）。

生成器は既存の property test（`glob_property_test.rs`、`wildcard_index` の単体テスト）の範囲に倣い、要素は短い ASCII の名前と `"*"`、`"**"` から作り、深さの上限は32とする（§3 の深さの性質）。非 ASCII と禁止文字は固定の表だけに頼らず、`ActorPath::is_valid_path_element` が偽になる要素を生成器で作って P4 を回す。

## 4. Quint を書くかどうか

**本件では Quint を書かない。** 理由は次のとおりである。

- `Deployer` の中核は、節の列から不変の索引を作り、要素列で引く**純粋な関数**であり、状態遷移を持たない。原典の `AtomicReference` の差し替えは並行の都合であり、意味としての遷移（登録の前後で引き当てがどう変わるか）は I1とI2と、登録の上書きの規則だけで尽きる。
- 既存の `WildcardIndex` は移植済みであり、その一致の規則はその設計（#187）と単体テストで固定されている。本件は索引を再検証しない。
- 状態遷移として書く価値があるのは「登録→引き当て→上書き後の引き当て」の列だが、これは §3 の P1・P2 が実行で確かめる範囲と重なる。Quint で書くと、索引の木構造を Quint 側に写す作業が本体になり、確かめる性質に対して割に合わない。
- 対照的に、FSM や FrequencySketch のように内部状態が時間で変わる対象では Quint が効いた。本件の `Deployer` には時刻も内部状態の変化も無い（`within` は保持する値であり、待ちの振る舞いはルーター側＝#269 の範囲である）。

将来、生成の配線（引き当てた `Deploy` で `Props` を上書きし、ルーターの head と routee を作る段）が入ると、その段では生成と停止の遷移が現れるため、その段の設計で Quint の要否を改めて判断する。同じく、**`Deployer` を複数スレッドで共有する段（共有と CAS、ロックで `deploy` する形にする段）では Quint が要る**。原典の CAS の再試行と、登録と引き当ての競合が、その段では状態遷移として現れるためである。

## 5. 忠実さの判断

本件に除外の提案は無い。16件すべてを移す。承認なく台帳の `cases.excluded` や `cases.not_built` に書かない原則は変わらない。

### F1（忠実さの判断。採用が決まった）：ケース5「detect invalid number-of-instances」

- 原典は HOCON の値の型（文字列 `"boom"`）を Typesafe Config が読み取り時に検査し、`ConfigException.WrongType` を投げることを見る。
- 移植では HOCON を読まないため、型の検査は `NumberOfInstances::NotANumber(String)` という未解釈の値を解釈の段で誤りにする形に写る（§2.2、§2.6）。確かめる意味は「数値でない件数は、引き当て可能な `Deploy` にならず、誤りとして返る」であり、これはケースの中心である。
- この置き換えは除外ではなく忠実さの判断として扱う（#154 の D1、G0b の決定 E、#192 の E1 と同じ扱い）。上司の判断で採用が決まった。条件は2つあり、どちらも本書に書いた。(1) 誤りになるのは件数を読む種類（全プール）だけであり、`from-code` とグループ系では誤りにならない規則（§2.6、I7、P6）。(2) 数の範囲は原典の `getInt` に合わせ `i32` とし、負数を受け、範囲を超える値は誤りになること（§2.2）。数える16件の中に含める。

### F3（忠実さの判断）：ケース16「have correct router mappings」

- 当初は除外を提案したが、独立レビューの指摘と上司の判断で取り下げ、型付きの対応で移す。理由は3つある。(1) #192 の `ProviderSelection` が、Pekko の FQCN を定数で持ち、`from_setting` で短い名前と FQCN の両方を受ける形で、同種のケースを除外ではなく忠実さの判断として移した前例がある。(2) 原典は FQCN を `router` に直接書くことを許すため、組み込みの FQCN を受けないと狭めになる。(3) 移せる形があるものを除外として上げる理由が弱い。
- 形は §2.6 のとおりである。`RouterSelection::from_setting` が短い名前15件と FQCN 14件を受け、それ以外（NoRouter の FQCN を含む）を `Custom` に保持する。`RouterSelection::fqcn` が対応する FQCN を返す。ケース16は、原典が比べる13件の短い名前が、期待どおりの FQCN を持つ選択値に写ることを確かめる。
- 失われるものは、Typesafe Config の節として書かれた対応表が実行時に読まれることの確認だけであり、対応そのものは型付きの関数として固定される。

### F2（忠実さの判断）：入力全体（ケース1〜15）

- 原典の入力は HOCON の文字列であり、引用符で囲んだキー（`"/some/*"`）やコメントを含む書式そのものは、どのケースも検証していない（書式の誤りを見るケースは DeployerSpec に無い）。入力を `DeploymentSection` の値に写すことで失われるのは HOCON の構文の解釈だけであり、節の意味（キーと項目の対応）は §1.2 の対応表で1対1に保つ。
- ケース1の `config` の比較は、保持した `DeploymentSection` の値の比較に写る（§2.3 の違い3）。
- `default` の節の上書きは扱わない（§2.2 の決め1）。原典では `pekko.actor.deployment.default` の上書きが全部の節に効くため、これは設定の体系の狭めであり、ここに記録する。**中将（2026-11-26）が意図した狭めとして採ると決めた。** 条件として、設定を読む段が入るときに、項目を `Option` にして重ねる形を考え直すことをここに書く。同じキーを2回渡したときの合成の違い（§2.2 の決め1）も同じ扱いであり、原典の入口からは作れない入力であるため、ずれではない。

### 範囲外として作らないもの（ケースの除外ではない）

- `Deploy` の `tags` と `withTags`：ケースに出ない。必要になる段で足す。
- `Deploy::with_fallback` と `RouterConfig::withFallback` の合成：ケースに出ない（§2.3）。
- カスタムルーターの反射生成（FQCN を渡して任意の `RouterConfig` を作る経路）：JVM の反射に固有であり、型付きの入力の範囲外である。`Custom(String)` は保持し、生成の段で誤りにする。
- resizer の動的な振る舞い、ルーターの振り分け（#269）、生成への配線：§1.3 と §2.5 のとおり範囲外である。

## 6. 第2段へ残す判断材料

1. **テストの置き場所**：規約は「内部 API を呼ぶケースは単体テストに置き、台帳の `rust.cases_outside_test_file` に書く」とする。原典は `deployer.lookup` という内部 API を直接呼ぶ。案は2つある。(a) 規約どおり `deployer_value_test.rs` の単体テストに置く。(b) `Deployer` を公開 API として確定させ、`tests/deployer_spec.rs` に置く。本書の推奨は (b) であり、独立レビューも、#187 の `WildcardIndex`（Pekko では `@InternalApi` だが公開して `tests/wildcard_index_spec.rs` に置いた）の前例を引いて (b) を支持した。**中将（2026-11-26）が (b) を採ると決めた**ため、第2段は (b) で進める。外に出すのは型だけで、振る舞いを強める約束は足さない。
2. **グループ系・tail-chopping・balancing の生成**：値として保持する案で確定した（§2.2 の決め3、§2.4）。
3. **`ConsistentHashingPool` の `virtualNodesFactor`**：閉じた。原典の `Config` コンストラクタは読まず、期待値は `0` である（§2.4。独立レビューが原典で裏取り済み）。
4. **既存の `RouterConfig` と `DeployedRouter` の統合**：§2.4。本件では統合しない提案である。
5. **台帳の分母**：`total: 16`、`ported: 16`、除外0件で記録する。ケース5とケース16は忠実さの判断（F1、F3）として `ported` に含める。
6. **パスの分け方（Java の `split` の挙動）**：**中将が写す側を推奨し、元帥経由でオーナーの判断待ち（2026-11-26）である。** `pekko-semantics-first.md` では、JVM に固有の振る舞いは1件ずつオーナーに伺う決まりであり、写すか狭めるかに関わらず伺う対象である。狭めの案（先頭の `/` の無いキーや末尾の `/` を誤りにする）は、原典が通す設定を止めるためずれになる、というのが推奨の理由である。罠（`"a/b"` が `/b` に縮む、`"svc"` が黙って消える）は `from_sections` の文書に注意として書いた（§2.3）。判断が出るまでは写す形で実装してよいが、台帳にもテストにも承認済みとは書かない。
7. **`PoolKind` の拡張の申し送り**：consistent-hashing の振り分けを #269 で作るか別の issue に回すかは #269 の設計で決める（§1.3）。#194 の性質の表への追加は第2段の作業に含めた（§7）。

## 7. 第2段の進め方（設計レビューが通ってから）

1. red：`tests/deployer_spec.rs`（または §6 の判断による単体テスト）に移すケースを先に書き、失敗を確認する。
2. green：§2 の型を `kernel::actor::deploy` に足し、`PoolKind` へ `ConsistentHashing`、`Balancing`、`TailChopping` を足す。新しいファイルには自作のものも含めて Apache-2.0 のヘッダーを付ける。
3. #194 の `tests/actor_configuration_verification_property_test.rs` の種類の表へ、新しい種類を足す（できれば網羅を `match` で強制する形に直す）。ただし balancing は routee 用の dispatcher を自分で作る特別な種類であり、表へ足すときに期待値を他のプールと同じにしてよいかを、原典の `ActorConfigurationVerificationSpec` と `BalancingPool` で確かめてから書く（再レビュー低5）。機械的に1行足すだけでは済まない可能性がある。
4. 台帳の対象ノードを更新し、`ledger/tools/rollup.py` で完了判定をする。
5. CI と同じ検査を流す：`cargo fmt --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`、`ledger/tools` の Python テスト。Quint は本件では書かないため対象外である。
6. PR をドラフトから外し、必須のチェックが落ちたら他の完了を待たずに直す。ボットの指摘とトップレベルのコメントには「直した／別件／採らない」と理由を書いて返信し、スレッドを解決する。マージはしない。

## 8. 独立レビューへの回答表（2026-11-26）

| 指摘 | 対応 |
|:---|:---|
| 高1：後戻りしない貪欲な一致の書き落とし。P3 が偽 | I10 を足し、反例の表を書いた。I2・I5・I6 を書き直した。P1 を差分検査に置き換え、P3 の範囲を「その節だけを登録した `Deployer`」に絞った |
| 中1：Java の `split` の挙動 | 分け方を関数1つとして §2.3 に書き、期待値の表を §2.3 と §3 に置いた。P5 を足した。写すかどうかはオーナーへの事項として §6 の6に挙げた |
| 中2：resizer の後方互換の取り違え | 「節に resizer があれば常に有効」に直し、`ResizerSection` から `enabled` を外した。P7 を足した。`optimal-size-exploring-resizer` を範囲外に足した |
| 中3：`default` の節の自己矛盾 | 推奨 (a) を採り、`default` のキーは読み飛ばすだけにして既定の上書きは扱わないと §2.2 の決め1と §5 の F2 に書いた |
| 中4：グループ系などを値で持つ型が足りない | `routees_paths` と `tail_chopping_interval`、`GroupKind`、`PoolKind` の `Balancing` と `TailChopping` を足した。件数の誤りの規則を「件数を読む種類だけ」に限った |
| 中5：ケース16の除外の理由と FQCN の狭め | 上司の判断どおり、除外を取り下げ、#192 の形（`from_setting` と `fqcn`、定数の FQCN）で移す設計にした（§2.6、§5 の F3）。「実際に使う7件」の数え違いも、F3 の書き直しで解消した（ケースが比べるのは13件、対応表は15件） |
| 中6：`PoolKind` を広げたときの接点 | §1.3 に #269 との調整点と #194 の表の漏れを書いた。#194 の表への追加を §7 の作業に入れた |
| 中7：索引の挿入の誤りの受け口 | `DeployError::Index(WildcardIndexError)` を足し、誤りの順序と `"***"`、`"a**"`、途中の `"**"` の期待値を §2.6 と §3 に書いた |
| 低（対応表）：原典より強い確認 | ケース9〜12の件数、ケース11の `within`、ケース14の `within` 5秒を「原典より強い確認」として §1.1 に明記した |
| 低1：数の型と範囲 | `NumberOfInstances::Value` を `i32` にした。`NotANumber` の意味と、`within` の `Duration` の扱いを §2.2 に書いた |
| 低2：ケース6の文の組み立て場所 | `ActorPath` 側に2引数の検証を写す形に直し、空の要素の文が deployment 用である点を §2.6 に書いた |
| 低3：深いパスでプロセスが止まる | #187 の既存の性質として §3 に書き、PBT の生成器の深さに上限32を付けた |
| 低4：PBT の範囲と抜け | P4 の生成器に `"***"`、`"a**"`、`"//a"` を入れ、不正な要素を生成器で作る形にした。P5、P6、P7 を足した |
| 低5：その他 | `virtualNodesFactor` の裏取りを済んだものとして §2.4 と §6 に書いた。同じキーを2回渡すずれを §2.2 に書いた。`lookup(ActorPath)` を作らないことを §2.3 に書いた。`DeploymentSection` の derive を §2.2 に書いた |
| Quint の条件 | 「`Deployer` を複数スレッドで共有する段では Quint が要る」を §4 に足した |

## 9. 再レビューへの回答表と、狭めと判断の決定（2026-11-26）

再レビュー（Opus。Pekko 2.0.0 の jar で原典の `deployer.lookup` を実行して確認）の指摘への対応は次のとおりである。

| 指摘 | 対応 |
|:---|:---|
| 中1：balancing、グループ、from-code は resizer を読まない。P7 が偽 | P7 の範囲を「balancing を除くプール」に絞り、P8 を足した。resizer の検証（`InvalidResizer`）は resizer を読む種類にだけかけることを §2.5 に書いた。balancing では `use_pool_dispatcher` が常に `false` になることも §2.5 に書いた |
| 中2：件数の型が入力 `i32` と結果 `u32` で食い違う | 結果の `DeployedRouter::Pool` の件数を `i32` に直し、-1 が `RoundRobinPool(-1)` として保持されることを §2.2 に書いた。範囲外の値（`3000000000`）は `NotANumber` で渡し、`NotANumber` の意味を「`i32` に直せない値（数でない文字列と範囲外の数の両方）」に直した |
| 中3：NoRouter の FQCN を受けると原典より広い | `from_setting` が受ける FQCN を14件にし、NoRouter の FQCN は `Custom` にして生成の段で `UnsupportedRouter` になる形に §2.6 を直した。`fqcn(FromCode)` は NoRouter の FQCN を返したままにし、往復の性質は `FromCode` だけ成り立たないことを明記した |
| 低1：P3 に接頭辞パターンを外す条件 | P3 に「接頭辞は接頭辞パターンを含まない場合に限る」を足し、I10 に「二重の木は接頭辞の子を見ない」を書き足した |
| 低2：`GroupKind::ConsistentHashing` の係数も 0 | §2.4 のグループ側の定義に、原典の `ConsistentHashingGroup` のコンストラクタは `routees.paths` しか読まず値は常に0であることを書いた |
| 低3：古い文3か所 | 「クラス名も無い」の文を直した。`Deployer` の `router_type_mapping` の欄を消した。「対象は次の4性質」を8性質に直した |
| 低4：resizer の下限と上限の型 | 下の判断の4のとおり、型の狭めとして採り、「非負」の検証の文を §2.5 から消した。表せないことは §2.2 に書いた |
| 低5：balancing を #194 の表に足すときの期待値 | §7 の3に、期待値を原典で確かめてから書くことを入れた |

狭めと判断の11件は、レビューの推奨どおりに**中将（2026-11-26）が決めた**。ただし1件目（Java の `split`）だけは、中将が写す側を推奨し、元帥経由でオーナーの判断待ちである。内訳は次のとおりである。

1. Java の `split` の挙動は、中将が写す側を推奨し、元帥経由でオーナーの判断待ち（2026-11-26）である（§6 の6）。写す形で実装してよいが、承認済みとは書かない。
2. `default` の節の上書きを扱わないことは、意図した狭めとして採る。条件は F2 に書いた（設定を読む段で `Option` にして重ねる形を考え直す）。
3. 同じキーを2回渡すと後のものが丸ごと勝つことは採る。原典の入口からは作れない入力であり、ずれではない。
4. 型で表せない値（負の `within`、resizer の負の下限と上限）は、意図した狭めとして採る。表せないことを §2.2 に書き、「非負」の検証の文は消した。
5. `Custom`（組み込みでない FQCN。NoRouter の FQCN を含む）が `UnsupportedRouter` になることは採る。原典でも起動に失敗し、観察できる結果は同じである。
6. ケース13の `nrOfInstances = 0` を写さず節の値1を持つことは採る。原典の実物の値は1である。
7. HOCON の `Config` の等価性を写さないことは採る。原典のケース1は同じ値を自分自身と比べているだけである。
8. `optimal-size-exploring-resizer`、`tags`、`with_fallback`、`lookup(ActorPath)` を作らないことは、範囲外として採る。
9. `NaN` の率を素通しにすることは、原典どおりとして採る。
10. `Deployer` を公開 API にして `tests/` に置くことは、#187 の前例どおり採る（§6 の1）。
11. `PoolKind` に種類を足すことの影響は、記録で足りるものとして採る。振り分けは #269 への申し送りのままとし、balancing の検証の期待値は低5で確かめる。

## 10. 実装の記録（第2段、2026-11-26）

- 16件すべてを `crates/fraktor-actor/tests/deployer_spec.rs` に移し、台帳の対象ノードを green にした（16件中16件が通過、除外0件）。性質 P1〜P8 は `crates/fraktor-actor/tests/deployer_property_test.rs` に置き、境界の単体テストは `deploy/deployer_value_test.rs` に置いた。
- 公開面は syn チェッカーの module-wiring の規則（再エクスポートは末端モジュールの直属親だけ）に従い、`kernel::actor::deploy` を公開モジュールとして出し、型はその下に置いた。`dispatch_settings` の前例と同じ形である。§2.1 の「公開面は `kernel::actor` の下に置く」は、この形で満たす。
- 型ごとにファイルを分ける規則に従い、§2.2 の4型は別々のファイルに分けた（§2.1 の表を実装に合わせて直した）。
- `PoolKind` に `Balancing`、`ConsistentHashing`、`TailChopping` を足し、#194 の `actor_configuration_verification_property_test.rs` の表にもこの3種を足した。規則は dispatcher の種類についてのものであり、原典の `ActorConfigurationVerificationSpec` にプールの種類ごとの期待値の違いは無い（balancing についての記述は balancing dispatcher のものである）ことを確かめたうえで、同じ期待値の行を適用した。
- Java の `split` の挙動は写す形で実装した。オーナーの判断は元帥経由で待ちのままであり、承認済みとは書かない（§6 の6、§9 の決定1）。
- 台帳の `needs.testkit`（`extends-pekko-spec`／`pekko-spec`）は元のままにした。移植は testkit を使わないため、実態との差としてゴール文書（`docs/goals/issue-195-deployer.md`）に記録した。
- `docs/baseline/v2.0.0-undocumented-nodes.tsv` は `report_ledger_1to1.py list-undocumented` で作り直し、本ノードを一覧へ足した（#194 の前例と同じ扱い）。
