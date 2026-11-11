# 旧 fraktor-rs 開発ルールの棚卸しと引き継ぎ切り分け（案）

状態: draft（2026-11-11、未承認）。調査対象は j5ik2o/fraktor-rs の `fea328f` 時点。
これは事実の棚卸し＋切り分け案であり、決定ではない。承認されたものだけが設計設定になる。

## 1. ルールの全体構造（6層）

### 第1層：入口と振る舞い（AGENTS.md / CLAUDE.md）

- すべて日本語でやりとりする
- Matt Pocock の engineering skills 前提：Issue tracker は GitHub Issues、triage は
  default labels、domain docs は root `CONTEXT.md` + `docs/adr/`（single-context）
- Code Guidelines 4原則：Think Before Coding（前提を明示、黙って解釈を選ばない）/
  Simplicity First（頼まれていない抽象・柔軟性を足さない）/ Surgical Changes
  （関係ないコードを触らない）/ Goal-Driven Execution（成功条件を検証可能な形に変換）
- `.coderabbit.yaml` は人間の許可なく変更しない。その path_filters 対象は
  レビューも変更もしない
- 注：AGENTS.md は `scripts/generate-agents-md.sh` で一部生成される

### 第2層：ドメイン／仕様の統制（CONTEXT.md / ADR / SDD）

- 設計・レビュー・実装計画の前に必ず preflight：`CONTEXT.md`（canonical terms /
  avoid terms / invariants）、`docs/adr/`、`.kiro/steering/`、`.kiro/specs/**`、
  `openspec/**` を入力として確認する
- 未定義のドメイン用語は直接 CONTEXT.md に足さず、`/domain-modeling` か
  `/grill-with-docs` 経由で確定する。既存 ADR と矛盾する spec は黙って上書きせず明示する
- spec を作った後は次のフェーズに進む前に用語照合（Terminology Reconciliation）を必須とする
- SDD ルール（`.agents/rules/sdd.md`）：仕様書は最初から日本語で書く（英語下書き→翻訳は禁止）。
  基本は cc-sdd / Kiro 系（品質ゲートあり）、OpenSpec は「小さい契約変更＋設計判断が
  ほぼ残っていない」場合だけ。併用時は OpenSpec が public contract、Kiro が実装 task を所有
- cc-sdd / OpenSpec の配布物ファイルは人間の許可なく変更しない

### 第3層：アーキテクチャとプロジェクト規約（`.agents/rules/project.md`）

- 6ドメイン（utils / actor / persistence / remote / cluster / stream）×
  core / adaptor-std の2層。core は `#![cfg_attr(not(test), no_std)]` +
  `#![deny(cfg_std_forbid)]` で std 直接依存を禁止し、adaptor-std が Tokio 等を集約する
  （DIP：core が port を定義し、adaptor が実装する向きを維持）
- 参照実装は `references/protoactor-go/` と `references/pekko/` の2本。
  設計開始時・命名で迷ったとき・過剰設計が疑われるときに必ず参照する
- toolchain は nightly-2026-06-05 固定（dylint が rustc_private を要求するため）、edition 2024
- clippy disallowed-types：`Arc` / `Rc` / `std::sync::Mutex` / spin 系の直接使用を禁止し、
  `ArcShared` / `SpinSyncMutex` 等の Shared ラッパーへ寄せる
- workspace lints：`unused_must_use = deny`、`let_underscore_must_use = deny` 等
- 後方互換性は不要（pre-release、破壊的変更を恐れない）
- CHANGELOG.md は GitHub Actions の自動生成物なのでエージェントは編集しない
- lint の `#[allow]` は人間の許可が必要。`.agents/rules/` のルール変更自体も人間の許可が必要

### 第4層：Rust 個別ルール（`.agents/rules/rust/*`）

- 内部可変性の制限（`&mut self` を第1選択にし、共有が必要な場合のみ Shared ラッパー経由）
- CQS 厳守（`&self` は読み取り＋戻り値、`&mut self` は更新＋戻り値なし or `Result<(), E>`）
- 1ファイル1公開型、mod.rs 禁止（`foo.rs` + `foo/` 階層）、親モジュールでの
  バレル的再エクスポート集約の禁止
- テストは対象ファイルの sibling `*_test.rs` に分離（インライン `#[cfg(test)] mod tests` 禁止）
- `use` はファイル先頭に集約、本体での FQCN 禁止
- 曖昧サフィックス禁止（Manager / Util / Service / Runtime / Engine / Facade 等は
  責務別命名表から具体名を選ぶ）。ただし参照実装の用語が優先される場合はそちらに合わせる
- ドキュメント言語：rustdoc は英語、Markdown と通常コメントは日本語
- 戻り値の握りつぶし禁止（`let _ =` / `.ok()` で Result・Option・must_use を捨てない）
- TOCTOU 回避（read-then-act は `with_write` クロージャ等で原子化、ガードを外部に返さない）

### 第5層：機械的強制（dylint / ci-check）

- カスタム dylint 11本を `lints/` に実装し、10本を workspace で有効化
  （mod-file / module-wiring / type-per-file / tests-location / use-placement /
  redundant-fqcn / module-examples / rustdoc / cfg-std-forbid / ambiguous-suffix）
- 最終確認は `./scripts/ci-check.sh ai all`（1,786行のスクリプト。fmt / clippy /
  dylint / no-std / embedded / e2e 等を束ねる）。途中工程は対象範囲のチェックに留める
- ci-check は内部で cargo を呼ぶため複数同時起動は禁止。hang 防止ガードの環境変数あり
- TAKT 実行中は `final-ci` ムーブメント以外で `ci-check.sh ai all` を回さない

### 第6層：プロセス（AI の失敗様式への対処）

- AI アンチパターン検出基準（328行）：仮定の検証、もっともらしいが間違っているコード、
  コピペパターン、冗長な分岐などの検出観点と検証手順
- コーディング前の学習：類似実装を2〜3個読んでプロジェクトパターンを抽出してから書く
- 参照実装からの逆輸入手順：参照実装の公開型数を数え、fraktor 側が1.5倍を超えたら
  過剰設計を疑う。最小 API・YAGNI を優先
- 中間アーティファクトの置き場所：TAKT 中間物は `.takt/`、計画文書は `docs/plan/`、
  レポートは `docs/gap-analysis/`
- TAKT 設定（`.takt/config.yaml`）：provider は codex、model gpt-5.5、
  concurrency 1、base_branch main
- CodeRabbit：assertive プロファイル、口調指示は「Less is more / YAGNI、
  過剰設計や不要な抽象化を指摘」。references や各 AI ツール設定、openspec 等は
  レビュー対象から除外

## 2. fraktor-ng-rs への切り分け案

### そのまま引き継ぐ（進め方のルールは移植方法に依存しない）

- 日本語コミュニケーション、rustdoc 英語／コメント日本語の使い分け
- Code Guidelines 4原則、AI アンチパターンの検証観点
- `#[allow]` とルール変更に人間の許可が要る、という統制
- CHANGELOG をエージェントが編集しない、中間アーティファクトの置き場所の規律
- CodeRabbit の path_filters によるレビュー除外の考え方

### 形を変えて引き継ぐ

- **Rust 構造ルール＋dylint**：機械的強制の思想は ng の「機械的なロジック」そのもの。
  ただし11本＋1,786行の ci-check を初日に全部は入れず、TestKit / actor-core の
  縦切りに合わせて段階導入する
- **CONTEXT / ADR による用語統制**：仕組みは引き継ぐが、中身はコピーしない。
  ng の CONTEXT は Pekko の用語から作る。旧 CONTEXT の用語定義は旧側の解釈であり、
  そのまま持ち込むとハーネスになる
- **コーディング前の学習**：読む対象が変わる。ng では類似実装ではなく
  「対応する Pekko テストと Pekko 実装」が学習対象になる
- **no_std core / adaptor-std の2層**：旧側では決定済みだが、ng では設計設定 3.4 の
  未決定事項（実行基盤・no_std 要否）。引き継ぐかどうか自体が設計判断として残る

### 引き継がない

- **SDD 駆動の装置一式**（Kiro spec / OpenSpec を正として実装を合わせる進め方と、
  蓄積した100超の capability spec）。ng は正を Pekko テストに置くため、spec は従属物になる
- **protoactor-go の参照実装としての併用**。ng の正は Pekko のみ
- **旧 TAKT 設定値**（codex / gpt-5.5 / concurrency 1 は旧リポジトリの運用値）。
  ng の TAKT 構成は director / manager 前提で別途設計する
