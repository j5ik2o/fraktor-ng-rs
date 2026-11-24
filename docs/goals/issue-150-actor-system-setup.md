# ゴール G0issue150：classic ActorSystemSetupSpec と Java ActorSystemSetupTest

[Issue #150](https://github.com/j5ik2o/fraktor-ng-rs/issues/150) と [Issue #151](https://github.com/j5ik2o/fraktor-ng-rs/issues/151)（Orca 項目228）の対象は、Pekko v1.7.1 の次の2つのテストである。どちらも同じ `ActorSystemSetup` を確かめるので、2つのノードを1つのゴールにまとめた。

- `ActorSystemSetupSpec`: [原典のテスト](https://github.com/apache/pekko/blob/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/actor-tests/src/test/scala/org/apache/pekko/actor/setup/ActorSystemSetupSpec.scala)。「The ActorSystemSettings should」の下に5ケース
- `ActorSystemSetupTest`（Java）: [原典のテスト](https://github.com/apache/pekko/blob/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/actor-tests/src/test/java/org/apache/pekko/actor/setup/ActorSystemSetupTest.java)。`apiMustBeUsableFromJava` の1ケース
- 原典の実装: [ActorSystemSetup.scala](https://github.com/apache/pekko/blob/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/actor/src/main/scala/org/apache/pekko/actor/setup/ActorSystemSetup.scala)（`Setup` L30–39、`ActorSystemSetup` L41–97）

## 対象ノード

| id | Rust の統合テスト | 原典 | 移植 | 成功 | 除外 |
|:---|:---|---:|---:|---:|---:|
| `actor-tests/src/test/scala/org/apache/pekko/actor/setup/ActorSystemSetupSpec.scala` | `crates/fraktor-actor/tests/actor_system_setup_spec.rs` | 5 | 5 | 5 | 0 |
| `actor-tests/src/test/java/org/apache/pekko/actor/setup/ActorSystemSetupTest.java` | `crates/fraktor-actor/tests/actor_system_setup_test.rs` | 1 | 1 | 1 | 0 |

合計は原典6件・移植6件・成功6件・除外0件。Scala の原典件数は `" in {"` の数で数えた（5件。`verify_red.py` の `count_original_cases` は WordSpec の `"..." should {` を数えきれず `None` を返す）。Java は `@Test` の1件で、`verify_red.py` も1件と数えた。`ignore` と `pending` はない。`ActorSystemSetupSpec` の `needs` は `test-kit`（`TestKit.shutdownActorSystem`）で、G0b が提供済み。`ActorSystemSetupTest` の `needs` は空。台帳の件数は原典の6ケースだけを数える。後述の補足の単体テスト7件は台帳に数えない。

## 実行経路

TAKT を使わず、担当（Claude Opus 5.5）が直接実装した。TAKT にはゴールもタスクも投入していない。起動仕様が直接実装と決めていた。

| 段階 | コミット | 内容 |
|:---|:---|:---|
| タグ付け | `32ba11f` | 2ノードの `goal: G0issue150`（台帳2行） |
| テストと台帳 red | `b59b20f` | 移植テスト2本、2ノードの status `red` と rust の欄。製品差分0 |
| 実装と台帳 green | `7fc1398` | `kernel::setup`（`Setup`、`ActorSystemSetup`）、`ActorSystem::with_setup` と `ActorSystem::setup`、単体テスト6件、台帳 green、本書 |
| レビュー指摘の修正 | 本書の更新のコミット | Java 移植テストに、登録したインスタンスそのものが返ることの確認を足した。単体テスト1件を追加。製品コードは変えていない |

### 着手時の調査と、調整担当の決定

`ActorSystem` には、生成時に設定を受け取る経路も、Pekko の `settings` に当たる層もなかった。原典の5件目「be available from the ExtendedActorSystem」は `ActorSystem("name", ActorSystemSetup(setup))` で作り、`system.settings.setup.get[DummySetup]` で読む。そのため、`Setup` と `ActorSystemSetup` の新設に加えて、`ActorSystem` の公開 API の追加が要る。着手前に調整担当へ次の3案を示した。

- A（推奨）：既存の API は変えず、`ActorSystem` に構築子 `with_setup(name, boundary, setup)` と参照 `setup()` を足す。`new` は空の設定を持つ。
- B：settings 型を新設し、`system.settings().setup()` とする。
- C：5件目を除外する（除外1）。

調整担当の決定は A（2026-11-25）。範囲は、原典の設定付きの生成と生成後の取得に一対一で対応する `with_setup` と `setup()` の追加に限る。既存の起動シグネチャと既定動作を保ち、既存の経路では空の設定になることを確かめる。settings 層を省くことを本書に明記する。設定が起動の動作を変える仕組みや、新しい settings 型が要るとわかった場合は、実測と影響を報告する（そうした必要は生じなかった）。

## 原典との照合

### API の対応

| 原典 | 移植 |
|:---|:---|
| `abstract class Setup` | `trait Setup: Any + Send + Sync` |
| `Setup.and(other)` | `Setup::and(self, other) -> ActorSystemSetup`（提供メソッド） |
| `ActorSystemSetup.empty`、`ActorSystemSetup()`、Java `ActorSystemSetup.create()` | `ActorSystemSetup::empty()`（`Default` も同じ） |
| `ActorSystemSetup(settings*)`、Java `create(settings*)` | `ActorSystemSetup::from_setups(impl IntoIterator<Item = Arc<dyn Setup>>)` |
| `get[T: ClassTag]: Option[T]`、Java `get(Class<T>): Optional<T>` | `get::<T>() -> Option<&T>` |
| `withSetup(t)` | `with_setup(self, t) -> Self` |
| `and(t)`（`withSetup` の別名） | `and(self, t) -> Self` |
| 内部のキー `Map[Class[_], AnyRef]`（キーは `getClass`） | `BTreeMap<TypeId, Arc<dyn Setup>>`（キーは具体型の `TypeId`） |
| `ActorSystem(name, setup)` | `ActorSystem::with_setup(name, boundary, setup)` |
| `system.settings.setup` | `system.setup() -> &ActorSystemSetup` |
| `TestKit.shutdownActorSystem(system)` | `system.terminate()`。アサーションが失敗したときは `Drop` が止める |

`Setup` と `ActorSystemSetup` は Pekko と同じ `actor.setup` に当たる `kernel::setup` に置いた。既存の公開 API（`ActorSystem::new`、`with_failure_catcher` ほか）のシグネチャは変えていない。

### ケース対応

| 原典のケース | Rust のテスト | 原典のアサーション | 移植 |
|:---|:---|:---|:---|
| "store and retrieve a setup"（L31–37） | `store_and_retrieve_a_setup` | `get[DummySetup]` が `Some(setup)`、`get[DummySetup2]` が `None` | `Some(&setup)`、`None` |
| "replace setup if already defined"（L39–45） | `replace_setup_if_already_defined` | 同じ型を2回 `withSetup` すると後の値 | `Some(&setup2)` |
| "provide a fluent creation alternative"（L47–55） | `provide_a_fluent_creation_alternative` | `a and b and c` で、同型の `b` が `a` を置き換え、`c` も入る | `a.and(b).and(c)` で `Some(&b)`、`Some(&c)` |
| "be created with a set of setups"（L57–65） | `be_created_with_a_set_of_setups` | `ActorSystemSetup(s1, s2)` で2型が入り、3型目はない | `from_setups([..])` で `is_some` 2件、`is_none` 1件 |
| "be available from the ExtendedActorSystem"（L67–78） | `be_available_from_the_extended_actor_system` | 設定つきで作ったシステムの `settings.setup.get` が `Some(setup)`、最後に停止 | `with_setup` で作り、`setup().get` が `Some(&setup)`、`terminate` 後に `is_terminated` |
| `apiMustBeUsableFromJava`（Java L33–41） | `api_must_be_usable_from_java` | `create().withSetup(s).get(JavaSetup.class)` が存在し、`s` と等しい | `empty().with_setup(s).get::<JavaSetup>()` が `Some` で、`name` が `"Jasmine Rice"`。返る値の `name` のバイト列が、登録前の `s` のものと同じアドレスにある（同一のインスタンス） |

### 原典との差異（意図した差）

原典6ケースの観測は変わらない。

- **settings 層がない**：原典は `system.settings.setup` で読むが、移植は `system.setup()` で直接読む。移植には `Settings` に当たる型がなく、本ゴールでも作らない（調整担当の決定 A）。設定はシステムの起動や動作を変えず、生成後に読めるように保持するだけである。原典でも `ActorSystemSetup` の値を読むのは各拡張で、本スイートは保持と取得だけを確かめる。
- **`get` の戻り値**：原典は値（参照）を `Option[T]` で返す。移植は設定の中の値への参照 `Option<&T>` を返す。Scala のテストは `Option[Setup]` に広げてから `===` で比べるが、比べる値は具体型の値なので、移植は具体型のまま比べる。
- **Java の同一性の比較**：原典の `JavaSetup`（Java L25–31）は `equals` を上書きしないので、`assertEquals(javaSetting, result.get())`（L40）は `Object.equals`、つまり同一のインスタンスかを比べる。移植では設定の値が `ActorSystemSetup` へムーブされるので、ムーブ元の変数とは比べられない。そこで、ムーブの前に設定が持つヒープ上のデータ（`name` のバイト列）のアドレスを取り、`get` が返す値が同じアドレスのデータを持つことを `std::ptr::eq` で確かめる。ムーブではヒープ上のデータは動かないが、複製や、同じ名前で作り直した別の `JavaSetup` は別のバイト列を持つ。同じ名前の別インスタンスを登録するとこの確認だけが失敗することを、一時的にテストを書き換えて実測した（名前の確認は通り、同一性の確認で panic）。名前の確認も残した。`Arc` を手元に残せる `from_setups` の経路では、単体テスト `get_returns_the_registered_instance` が、返る参照と保持した `Arc` の指す先が同じであることを確かめる。
- **不変性**：原典の `withSetup` は新しい値を返し、元の値は変わらない。移植の `with_setup` と `and` は `self` を受け取って返す。元の値を残したいときは `clone` してから足す（値は `Arc` で共有するので、`clone` は設定の値をコピーしない）。単体テストで、`clone` した元が変わらないことを固定した。
- **`toString`**：原典は `ActorSystemSettings(クラス名,...)` を返す。移植の `Debug` は保持している設定の数だけを出す。`dyn Setup` から型名を得るには `Setup` に公開メソッドを足す必要があり、原典の公開面を超えるので足さない。
- **`Setup` の境界**：`Setup` は `Any + Send + Sync` を要求する。`TypeId` で具体型を引くために `Any`（つまり `'static`）が要る。設定はスレッドをまたいで共有されうるので、`Send + Sync` を課した。
- **キーの重複の扱い**：原典の `apply(settings*)` は `toMap` で後の値を残す。移植の `from_setups` も後の値を残す。単体テストで固定した。

## 実装

- `kernel::setup::Setup`（`crates/fraktor-actor/src/kernel/setup/setup_trait.rs`）：印の trait。`and` を提供メソッドとして持つ。
- `kernel::setup::ActorSystemSetup`（`crates/fraktor-actor/src/kernel/setup/actor_system_setup.rs`）：`BTreeMap<TypeId, Arc<dyn Setup>>` を持つ。`get` は `TypeId` で引き、`&dyn Any` へアップキャストして `downcast_ref` する。
- `ActorSystem`（既存）：`setup: ActorSystemSetup` の欄を足した。`new` は `with_setup(name, boundary, ActorSystemSetup::empty())` を呼ぶだけになり、生成の手順は変えていない。`with_failure_catcher` は `new` を通るので、空の設定を持つ。
- kernel に std は持ち込んでいない（syn-check の違反0）。

### 補足の単体テスト（台帳の件数に含めない）

| テスト | 固定する契約 |
|:---|:---|
| `an_empty_setup_holds_nothing` | `empty()` と `default()` は何も持たない |
| `the_later_of_two_setups_of_one_type_wins_when_created_from_a_set` | `from_setups` で同じ型が2つあれば後の値が残る |
| `adding_a_setup_leaves_a_clone_taken_before_unchanged` | 足す前に `clone` した値は変わらない |
| `a_setup_of_one_type_is_not_returned_for_another` | `Setup::and` で同型が置き換わり、別の型では返らない |
| `get_returns_the_registered_instance` | `from_setups` に渡した `Arc` の指す先そのものを `get` が返す。`and` で足した後も、`clone` した値からも同じ |
| `a_system_created_without_a_setup_holds_an_empty_one`（`ActorSystem`） | 既存の `new` で作ったシステムの設定は空 |
| `a_system_created_with_a_setup_returns_it_and_runs_as_one_created_without`（`ActorSystem`） | `with_setup` で作ったシステムは設定を返し、名前、起動状態、配送、停止は `new` と同じ。停止後も設定を読める |

## 等価性レビュー

### 実装者自身のレビュー

実施者は本 issue の担当（Claude Opus 5.5）。Pekko 固定 SHA の原典テスト2本と `ActorSystemSetup.scala` を、移植の実装・テストと突き合わせた。判定は「等価」。原典6ケースは観測上等価で、除外は0。差異は前節のとおりで、どれも原典6ケースの観測には現れない。

### 独立レビュー（別系統のモデル）

PR 作成後、既出の指摘に対応した HEAD を固定して行う（`docs/decisions/takt-workflow-design.md` §2.6）。結果はここに追記する。

## 製品編集前の実測

`b59b20f` は `32ba11f` からの製品差分が0である（`git diff --stat 32ba11f b59b20f -- 'crates/*/src'` は空）。テストの blob は `actor_system_setup_spec.rs` が `175a4c94`、`actor_system_setup_test.rs` が `47079efa`。実装のコミット `7fc1398` では2本の統合テストを変えていない。その後、レビュー指摘への対応で `actor_system_setup_test.rs` に同一性の確認を足した（製品コードは変えていない）。

| コミット | コマンド | 終了値 | 結果 |
|:---|:---|---:|:---|
| `b59b20f` | `cargo test -p fraktor-actor --test actor_system_setup_spec -- --test-threads=1` | 101 | compile-red。`E0432`（`fraktor_actor::kernel::setup` が未解決）1件 |
| `b59b20f` | `cargo test -p fraktor-actor --test actor_system_setup_test -- --test-threads=1` | 101 | compile-red。同じ `E0432` 1件 |
| `b59b20f` | `verify_red.py`（原典を `PEKKO_ORIGINAL_ROOT` に置き、ノードごとに実行） | 0 | 2ノードとも `expected_compile_error`、`excluded: 0`。Java は `original_cases: 1`、Scala は `null`（上記の数え方の制約） |

already_green ではない。`kernel::setup` がなく、コンパイルに失敗した。

## 実装後の実測

実装と台帳 green のコミット `7fc1398` と、レビュー指摘を直したコミットで、担当が次を実行した。件数は修正後のもの。

| コマンド | 終了値 | 結果 |
|:---|---:|:---|
| `cargo test -p fraktor-actor --test actor_system_setup_spec -- --test-threads=1` | 0 | 5 passed / 0 failed / 0 ignored |
| `cargo test -p fraktor-actor --test actor_system_setup_test -- --test-threads=1` | 0 | 1 passed / 0 failed / 0 ignored |
| `cargo test -p fraktor-actor --lib kernel::setup` | 0 | 5 passed（`ActorSystemSetup` の単体テスト） |
| `cargo test -p fraktor-actor --lib actor_system` | 0 | 63 passed（既存61、新規2） |
| `cargo test --workspace` | 0 | 69 スイート、696 passed / 0 failed / 0 ignored |
| `cargo fmt --check` | 0 | 差分なし |
| `cargo clippy --workspace --all-targets -- -D warnings` | 0 | 警告なし |
| syn-check（`crates/fraktor-actor`、`crates/fraktor-testkit`） | 0 | 両方 `clean: true`、違反0 |
| `npm run check:aislop` | 0 | warn 以上の指摘は受理済み一覧と一致 |
| `goal_complete_gate_test.py`、`verify_red_test.py`、`verify_green_test.py`、`verify_green_test_aislop.py`、`verify_green_test_crate.py` | 0 | 成功 |
| `verify_green.py --crate fraktor-actor` | 0 | `all_pass: true` |

台帳は、`goal:` を持つ30ゴール（`G0issue150`、`G0a`〜`G0f`、既存の `G0issue*` すべて）の各々で `check_needs.py` と `rollup.py` を実行し、60コマンドすべて終了値0だった。`G0issue150` は2件で `{'green': 2}`、needs 閉性 OK。

## 再現手順

```text
cargo test -p fraktor-actor --test actor_system_setup_spec -- --test-threads=1
cargo test -p fraktor-actor --test actor_system_setup_test -- --test-threads=1
cargo test -p fraktor-actor --lib kernel::setup
cargo test --workspace
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
uv run --with pyyaml python3 ledger/tools/check_needs.py --ledger ledger/nodes.yaml --capabilities ledger/capabilities.yaml --goal G0issue150
uv run --with pyyaml python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal G0issue150
```
