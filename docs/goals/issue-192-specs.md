# ゴール G0issue192：actor の ProviderSelectionSpec／ActorWithBoundedStashSpec／FunctionRefSpec の移植

## 目的

Pekko v2.0.0 の `actor-tests` の3つの Spec（ProviderSelectionSpec、ActorWithBoundedStashSpec、FunctionRefSpec）を Rust へ移植し、対応する本体（ProviderSelection、DispatchSettings と bounded stash、FunctionRef）を `fraktor-actor` の kernel に足して、台帳の対象ノードを green にする。

設計は [docs/design/issue-192-specs.md](../design/issue-192-specs.md) にあり、独立レビュー（要修正、高3件・中11件・低7件）と再レビュー（条件つき承認）を通った。再レビューの条件は、実装の最初のコミットの前に設計へ反映した：mailbox の容量を dispatcher へフォールバックさせない（N1。原典で3段フォールバックをするのは stash の容量だけである）、FunctionRef の derive から Copy を外す（N2）。実装段の注意（N3、N4）も設計へ注記し、実装はその形にした：除去した FunctionRef への送信は生成時に持たせた dead letters 経路で分け、Terminated の sender 差し替えの見分けは watcher が function ref の登録表にあるかで行う。

## 対象ノード

| id | Rust の統合テスト | 原典のケース数 |
|:---|:---|---:|
| `actor-tests/src/test/scala/org/apache/pekko/actor/ProviderSelectionSpec.scala` | `crates/fraktor-actor/tests/provider_selection_spec.rs` | 4 |
| `actor-tests/src/test/scala/org/apache/pekko/actor/ActorWithBoundedStashSpec.scala` | `crates/fraktor-actor/tests/actor_with_bounded_stash_spec.rs` | 5 |
FunctionRefSpec（`actor-tests/src/test/scala/org/apache/pekko/actor/FunctionRefSpec.scala`、原典9件）は9件のうち8件を移植して green 相当の状態にあるが、残り1件（下記の E3）がオーナーの判断待ちであるため、台帳には書いていない。そのため、この表には台帳へ書いた2ノードだけを載せる。

ProviderSelectionSpec と ActorWithBoundedStashSpec は green である。

## 移植の忠実さの判断

- E1（ProviderSelectionSpec）：原典のケースは HOCON の設定文字列を `Settings` 経由で読み、選択の `fqcn` と `hasCluster` と、設定から読んだ provider クラスが一致することを確かめる。移植は設定ファイルを持たないため、数えている4件すべてを、HOCON の代わりの型付き入力（設定文字列そのもの）で、原典と同じことを確かめる形にした。ケースの除外ではない。
- E2（ActorWithBoundedStashSpec）：原典のケースは dispatcher と mailbox の定義を HOCON で与える。移植は設定ファイルを持たないため、数えている5件すべてを、HOCON の代わりの型付き入力（`DispatchSettings` と `MailboxDefinition`）で、原典と同じ容量の解決と振る舞いを確かめる形にした。ケースの除外ではない。
- ケースに出てこない振る舞い（bounded mailbox への通常の tell のあふれの扱い、FunctionRef の登録表の push timeout 相当）は移していない。

## 判断待ち

- E3（FunctionRefSpec の「not be found」）：原典が確かめているのは、登録されていない FunctionRef をパスから解決しても FunctionRef に戻らないことであり、fraktor にはパスの解決も直列化も無い。このケースはケースの除外にあたるため、オーナーの承認が出るまで台帳に書かず、PR の中で「判断待ち」と書く。残りの8件は進めてよいという前提で実装した。

## 成果物

- 本体：`crates/fraktor-actor/src/kernel/actor/` の `provider_selection.rs`（`ProviderSelection::from_setting`）、`dispatch_settings.rs`（`CapacityLimit`、`DispatchSettings`、`MailboxDefinition`）、`stash_overflow_exception.rs`、`dead_letter.rs`（event stream へ発行する観測手段）、`function_ref.rs`（`FunctionRef` newtype）。あわせて `ActorContext::try_stash`、`Props::with_mailbox`、`ActorSystem` と `ActorContext` の function ref の watch 系メソッドを足した
- 移植テスト：`provider_selection_spec.rs`（4件）、`actor_with_bounded_stash_spec.rs`（5件）、`function_ref_spec.rs`（8件）
- 原典の外のテスト：`provider_selection_property_test.rs`（任意文字列とモデルの照合）、`bounded_stash_property_test.rs`（VecDeque の参照モデルとの照合）、`function_ref_model_test.rs`（登録表モデルとの照合）、本体の単体テスト3件
- Quint：`crates/fraktor-actor/specs/bounded_stash.qnt`、`crates/fraktor-actor/specs/function_ref.qnt`（ProviderSelection には書かない。設計の判断）
- 台帳：ProviderSelectionSpec と ActorWithBoundedStashSpec を green にした。FunctionRefSpec は E3 の判断が出るまで書かない

## 進め方

red（失敗するテスト、`E0432` で本体が無いことによる失敗）→ green の順に進めた。

## 検証

Mac mini（`~/muse-work/fraktor-ng-rs-192`）で実行した。

| コマンド | 結果 |
|:---|:---|
| `cargo fmt --check` | 通過 |
| `cargo clippy --workspace --all-targets -- -D warnings` | 通過 |
| `cargo test --workspace` | 1176件、失敗0 |
| `bash scripts/quint-check-specs.sh` | bounded_stash は 5 passing、function_ref は 6 passing、どちらも `No violation found` |
| syn-check（`crates/*/`） | 違反0 |
| `npm run check:aislop` | 受理済みの指摘と一致 |
| `uv run --with pytest --with pyyaml python -m pytest ledger/tools` | 189件、全件通過 |
| `uv run --with pyyaml python3 ledger/tools/report_ledger_1to1.py check` | 検査1から5がすべて OK |
| `python3 tools/license-headers/license_headers.py --check --manifest docs/design/issue-206-file-origin.tsv` | 0 で終了（906ファイル） |
