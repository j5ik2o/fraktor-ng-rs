# 設計：CI で Quint 仕様を検査する（Issue #214）

[Issue #214](https://github.com/j5ik2o/fraktor-ng-rs/issues/214) の設計メモである。
「小さな修正」の扱い（設計は数行、状態遷移の変更ではないので Quint の新規モデルは無し）。

## 何を足すか

| ファイル | 変更 |
|:---|:---|
| `package.json` / `package-lock.json` | `@informalsystems/quint` を `0.32.0` に固定して devDependencies へ足す（`npm ci` で再現） |
| `.github/workflows/ci.yml` | `quint` ジョブを足す。`check` とは別ジョブで並行に走らせる |
| `docs/design/quint-in-ci.md` | この文書 |

ジョブの中身は 1 ステップで、次を `crates/*/specs/*.qnt` のそれぞれに流す。

1. `quint typecheck`（型・未定義名の検査）
2. `quint test`（各 `run` の `expect` / `fail`。決定論的）
3. `quint run --invariant=invariant --max-steps=20 --max-samples=10000`（短い模擬。仕様が
   `val invariant` を定義しているときだけ。無い仕様は飛ばしたことをログに残す）

対象は glob で毎回見つけるので、新しい crate に `specs/*.qnt` が増えてもジョブは書き換えない。
1 本も見つからなければ `::error::` で落とす（glob が壊れて黙って通るのを防ぐ）。

## 必須のチェックに入れるか：入れる

`ci.yml` に足したジョブは、`main-pr-ci.yml` の `Fraktor CI Gate` が `needs.ci.result` で
見るため、追加の配線なしで必須チェックの対象になる（必須チェックは `Fraktor CI Gate` の 1 本）。

入れる理由は、Issue #214 の目的が「仕様と実装がずれても気づけない」ことの解消だからである。
必須にしないと、落ちていてもマージでき、気づく手段が無いままになる。実測で 3 本あわせて
約 1 秒（ローカル）なので、必須にしても待ち時間は増えない。npm レジストリに届かないときは
落ちるが、`check` ジョブの `npm ci` と同じ依存であり、新しい種類の risk ではない。

`check` ジョブに足さず別ジョブにしたのは、Rust のビルドキャッシュを待たずに並行で終わり、
落ちたときにどちらが原因か名前で分かるため。

## 守る不変条件

- 版は `package-lock.json` の `quint 0.32.0` だけを使う（`./node_modules/.bin/quint` を直接呼び、
  `npx` のように別版を取りに行く経路を使わない）
- 検査を弱めない：`typecheck` の失敗、`test` の失敗、`run` の不変条件違反は、どれもジョブを
  非ゼロで終わらせる
- 既存の 3 本（`receive_while.qnt`、`test_latch.qnt`、`watch_handle.qnt`）がすべて通る

## 影響と範囲外

- 影響：CI にジョブが 1 つ増える。`npm ci` が quint を入れる分だけ `check` ジョブも数秒伸びる
- 範囲外：`docs/design/specs/` の仕様（`crates/*/specs/` の外）は対象にしない。仕様の中身は変えない。
  `quint verify`（Apalache）は入れない（時間と導入の重さが別物）
- 上流・保存済みデータ・公開 API への影響は無し

## 戻し方

`ci.yml` の `quint` ジョブと `package.json` の 1 行を消し、`npm install` で lockfile を作り直す。
`docs/design/quint-in-ci.md` を消す。他に依存するものは無い。

## 状態遷移

ワークフローは「仕様を検査する」だけで、モデル化する状態遷移を増やさない。Quint の新規モデルは不要。

## 壊しうる入力（検査そのものに対して）

- 仕様が 0 本（glob の取りこぼし、crate 名の変更）→ 明示的に落とす
- 仕様が別ファイルを import する → リポジトリ直下から `quint` を呼ぶので相対 import は解決する
- 仕様が `val invariant` を持たない → `run` を飛ばし、ログに残す（`typecheck` と `test` は流す）
- ファイル名と module 名がずれた仕様 → 0.32.0 は 1 ファイル 1 module ならそのまま
  typecheck・test・run する（`zz_name.qnt` に `module zz_run_break` を置いて実測）。ずれ自体は
  落とさないので、名付けはレビューで見る。今の 3 本は一致している
