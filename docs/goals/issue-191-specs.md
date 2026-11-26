# ゴール G0issue191：DeadLetterSuspensionSpec と DeadLetterSupressionSpec

状態：**green**（2 ノード）。DeadLetterSuspensionSpec は 1 件、DeadLetterSupressionSpec は 2 件を移植し、全件が通っている。除外はない。

[Issue #191](https://github.com/j5ik2o/fraktor-ng-rs/issues/191)。原典は Pekko v2.0.0（commit `9fbd2283b1cf701c90a1223bf621942c191db61f`）。TAKT は使っていない。設計は [docs/design/issue-191-specs.md](../design/issue-191-specs.md) にあり、設計の独立レビュー（高 3・中 8・低 8）と再レビュー（低 7）の指摘の扱いは、設計の §9 と §10 に記録した。

## 対象ノード

| id | Rust | 原典のケース数 | 状態 |
|:---|:---|---:|:---|
| `actor-tests/src/test/scala/org/apache/pekko/actor/DeadLetterSuspensionSpec.scala` | `crates/fraktor-actor/tests/dead_letter_suspension_spec.rs` | 1 | green（1/1/1） |
| `actor-tests/src/test/scala/org/apache/pekko/actor/DeadLetterSupressionSpec.scala` | `crates/fraktor-actor/tests/dead_letter_supression_spec.rs` | 2 | green（2/2/2） |

ケースの数は v2.0.0 の原典で数えた。台帳の近似値（SupressionSpec は 0 と誤っていた）は使っていない。

ConsistencySpec は、このゴールには含めていない。fraktor の `ActorSystem` は `!Send` で、1 つのシステムを複数のスレッドで回せないため、「アクターが 2 つ以上のスレッドで同時に動く」という green の条件を満たせない。上司（元帥）の判断で #191 から外して未移植のまま後回しにし、[Issue #235](https://github.com/j5ik2o/fraktor-ng-rs/issues/235) に分け、複数スレッドの実行器の設計（[Issue #234](https://github.com/j5ik2o/fraktor-ng-rs/issues/234)）の後に置いた。台帳の ConsistencySpec ノードは `unported` のまま変えていない。理由の詳細は設計の §1 にある。

## 何を変えたか

すべて `crates/fraktor-actor` と `crates/fraktor-testkit` である。設計の §3 の不変条件（配送 D1〜D7、リスナー L1〜L7）に沿って実装した。

- dead letter の型（新規）：`DeadLetter`、`SuppressedDeadLetter`、`Dropped`、`AllDeadLetters`（チャンネルの目印と `AnyDeadLetter` のビュー）、抑制の目印トレイト `DeadLetterSuppression`、`DeadLetterSettings` と `DeadLetterLogLimit`。
- 振り分け（新規）：`DeadLettersShared` が抑制の集合（`BTreeSet<TypeId>`、no_std）とバスへの弱い参照を持ち、配送できない封筒、deadLetters 宛ての配送、解決できない宛先を、設計の規則どおりに振り分ける。停止したアクター宛ての `DeadLetter` は D7 により捨て、バスへ流さない（原典 `Mailboxes.scala` の 69〜71 行）。
- リスナー（新規）：`DeadLetterListener` が、原典 `DeadLetterListener.scala` の文面（`PrettyDuration` の書式、再開の文面、期限の厳密な比較を含む）でログを出し、上限で止め、期限の後に再開して数え直す。状態は `Arc<spin::Mutex<…>>` で持ち、`Send + Sync` の境界を通る。
- `ActorSelection`（新規）：封筒で送り、配送のときにセルと関数参照の両方へ解決する。生成時点で共有境界と宛先の参照を取り、`tell` では executor を借りない。
- executor と `ActorSystem`：deadLetters のハンドラが共有状態経由でバスへ出し、セルが無い封筒と stash／queue の残りを `DeadLetter` に包んで deadLetters へ送る。`ActorSystem` は 4 つの dead letter クラスを `AllDeadLetters` の子として宣言し、`register_dead_letter_suppression`、`start_dead_letter_listener`、`actor_selection` を足した。
- testkit：`EventFilter::info()` と `.start(prefix)` を足した（Info レベルの接頭辞一致）。

変えていないもの：

- 既存の公開 API のシグネチャ。足したのは新しい型とメソッドだけである。
- ConsistencySpec ノードと、そのテストファイル（作っていない）。

範囲の外（設計 §6 に記録）：

- `Unlimited` かつ Info の購読者が停止していると、D7 の外側で dead letter が巡る経路が残る。
- シャットダウン時の抑制は移植していない（原典の `DeadLetterSupressionSpec` の範囲外）。

## Quint

`DeadLetterListener` の数え上げは状態遷移なので、Quint で書いて検査した。

| 仕様 | モデルにしたもの | 不変条件 |
|:---|:---|:---|
| `crates/fraktor-actor/specs/dead_letter_listener.qnt` | モード（Logging／Suspended）、数、時刻、期限、上限と止める期間（状態変数として `initWith` で入れる。`const` にすると CI が QNT500 で落ちる） | `countIsUnsuppressedSeen`（L1）、`noLogWhileSuspended`（L2）、`suspendAtLimit`（L3）、`resumeResetsCount`（L4）、`deadlineIsStrict`（L5） |

検査の結果（quint 0.32.0、CI の `scripts/quint-check-specs.sh` 経由）：

| コマンド | 結果 |
|:---|:---|
| `quint typecheck crates/fraktor-actor/specs/dead_letter_listener.qnt` | 成功 |
| `quint test`（`limit4Test`、`limit1Test`、`limit2shortTest`、`unlimitedTest`） | 4 件 pass |
| `quint run --invariant=invariant --max-steps=20 --max-samples=10000` | 違反なし |

不変条件が違反を捕まえることも確かめた。`docs/design/specs/dead_letter_listener_test.qnt` に壊したモジュールを 5 本置き、各 run が `.expect(not(<不変条件>))` で通ることを確かめた（`quint verify` は Apalache が要るので使っていない）。

| モジュール | 壊し方 | 捕まえる不変条件 |
|:---|:---|:---|
| `bugNoReset` | 再開で数を 0 に戻さない | `resumeResetsCount` |
| `bugCountsSuppressed` | 抑制した到着も数える | `countIsUnsuppressedSeen` |
| `bugLogsWhileSuspended` | 止めている間もログを出す | `noLogWhileSuspended` |
| `bugResumeAtDeadline` | 期限ちょうどで再開する（`>=`） | `deadlineIsStrict` |
| `bugOffByOneNotLogged` | 「記録しなかった件数」が 1 件多い | 再開の文面の件数の式 |

## PBT の性質

`crates/fraktor-actor/tests/dead_letter_routing_property_test.rs`。新しい依存は足していない。乱数は SplitMix64 で、種は固定の 24 個、各 40 手順である。

| 性質 | 内容 |
|:---|:---|
| 配送のモデル | セルの有無、宛先の種類（通常・deadLetters・選択）、抑制の有無を混ぜた手順列で、バスへ出た dead letter の種類と数がモデルと一致する |
| リスナーの中核のモデル | 上限・期間・到着の列で、ログの有無、再開、数え直しがモデルと一致する |

## 先行テストと red の証拠

テストは製品より先に置いた（`675361e`、製品は未変更）。

| コマンド | 結果 |
|:---|:---|
| `cargo test -p fraktor-actor --test dead_letter_supression_spec` | コンパイルエラー（`DeadLetter` などの型が無い）で red |
| `cargo test -p fraktor-actor --test dead_letter_suspension_spec` | コンパイルエラー（`EventFilter::info` が無い）で red |

## 検証（ローカル）

| コマンド | 結果 |
|:---|:---|
| `cargo test -p fraktor-actor --test dead_letter_supression_spec` | 2 passed |
| `cargo test -p fraktor-actor --test dead_letter_suspension_spec` | 1 passed |
| `cargo test -p fraktor-actor --test dead_letter_routing_property_test` | 9 passed（モデル 2 件と、レビュー対応の回帰 7 件） |
| `cargo test -p fraktor-actor --lib` | 542 passed / 0 failed（レビュー対応の回帰 3 件を含む） |
| `cargo test --workspace` | 全スイートで失敗なし |
| `cargo clippy --workspace --all-targets -- -D warnings` | 成功 |
| `cargo fmt --check` | 成功（stable。nightly の rustfmt は既存ファイルの書式と違うので使っていない） |
| `scripts/quint-check-specs.sh` | 全 6 仕様 pass（この仕様を含む） |
| syn checker（`crates/fraktor-actor`、`crates/fraktor-testkit`） | 違反 0、抑制 0 |
| `npm run check:aislop` | 既知の指摘と一致 |
| 台帳の検査（`pekko_baseline.py check-ledger`、`check-refs`、`relocate_nodes.py check`、`report_ledger_1to1.py check`） | 成功 |
| ライセンスヘッダー検査（`license_headers.py --check --manifest docs/design/issue-206-file-origin.tsv`） | 成功（903 件、missing 0、errors 0）。線引き一覧の再生成とも一致 |

## main の取り込みとライセンスヘッダー（2026-11-26）

PR #215（ライセンスヘッダー、ab8a616）を含む main を取り込んだ。競合は自動で解け、両方の変更が残っている。CI に入ったヘッダー検査に合わせ、新規ファイル 13 件に `Apache-2.0` のヘッダーを原典を指定して付けた（`tools/license-headers/license_headers.py --apply --kind pekko --source <原典>`）。`DeadLetterListener` は、main 側で入った #186 の `PrettyDurationInput` を使うように変え、自前の書式関数を消した。

- `crates/fraktor-actor/tests/dead_letter_routing_property_test.rs` は自作である。当初は線引き一覧で「自分で書いた／保留」になり、ヘッダーを付けていなかったが、元帥の規則（2026-11-26）で、新しく足すファイルは自作も含めてまず `Apache-2.0` と決まった。`保留` は既存のファイルで出どころが決まらないものの区分であり、新しいファイルをヘッダー無しにしてよいという意味ではない。このファイルに `Apache-2.0` のヘッダーを付け、一覧では「要確認／Apache-2.0」（決定 2 の一括）になった。自作であることは、PR 本文にパスと理由を書いて宣言している。
- 線引き一覧を作る `tools/license-headers/classify_file_origin.py` が、新しいファイルを `保留` と出していたので、あわせて直した。前の一覧に無い crates のファイルには「要確認」の印を付け、決定 2 の一括で `Apache-2.0` と出す。印は次回以降の作り直しでも引き継ぐ。テスト 4 件を足した。CodeRabbit の指摘で、引き継ぎの条件（一括の決定で `Apache-2.0` になっていた行は、途中で別の理由の印が付いて消えても扱いを続ける）と、履歴の読み元（出力先ではなく決まった場所の一覧）も直し、テストを 1 件足した。
- PR #229 がマージされて main が進んだため、main を取り込み直した（`a1761c6`）。線引き一覧は競合したので、両方の親の一覧の和集合を前の状態として作り直し、差分が上の PBT の 1 行だけであることを確かめた。
- main 側で #188 の新規ファイル 10 件（`ImmutableIntMap` と `TokenBucket` の実装とテスト）にヘッダーが無く、取り込んだ時点で検査が落ちる状態だった。この PR で原典を指定してヘッダーを付けた（判断が要る変更ではなく、検査を通すための機械的な付与）。
- `actor_system_value.rs` は、取り込みで `Mirrors` の言及が増えて帰属する原典が広がったため、ヘッダーの表示を作り直した。
