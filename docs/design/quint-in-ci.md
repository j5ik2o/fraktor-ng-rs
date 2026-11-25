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
3. `quint run --invariant=invariant --max-steps=20 --max-samples=10000`（短い模擬。どの仕様にも流す）

`crates/*/specs/*.qnt` の仕様は、どれも `invariant` という名前の不変条件を定義する決まりにする。
`run` を飛ばす判定は置かない。名前が無い仕様は `quint run` が `Name 'invariant' not found` で
落ちる。前の版は `val invariant =` を正規表現で探し、無ければ飛ばしていたが、`val invariant: bool =`
のように型を書くと見落とし、反例があっても必須チェックが通っていた（PR #216 の Devin の指摘）。
Quint の名前解決（import、main モジュールの決め方）を手で真似ると同じ種類の見落としが残るので、
判定をやめて、無ければ落ちる側に倒した。不変条件の無い仕様を置く必要が出たら、そのときにジョブを直す。

対象は glob で毎回見つけるので、新しい crate に `specs/*.qnt` が増えてもジョブは書き換えない。
1 本も見つからなければ `::error::` で落とす（glob が壊れて黙って通るのを防ぐ）。

## 必須のチェックに入れるか：入れる

`ci.yml` に足したジョブは、`main-pr-ci.yml` の `Fraktor CI Gate` が `needs.ci.result` で
見るため、追加の配線なしで必須チェックの対象になる（必須チェックは `Fraktor CI Gate` の 1 本）。

入れる理由は、仕様そのものが壊れても（型の誤り、`run` の期待の誤り、不変条件の反例）気づけない
状態を解消するためである。必須にしないと、落ちていてもマージできる。

このジョブが見るのは仕様だけで、Rust の実装と仕様の突き合わせはしない。実装が仕様の性質を守るかは、
仕様の名前を書いた Rust の性質テスト（例：`test_kit_value_test.rs` の `receive_while.qnt` の性質）が
見る。Rust 側だけが変わったときに気づけるかは、それらのテスト次第である。実測で 3 本あわせて
約 1 秒（ローカル）なので、必須にしても待ち時間は増えない。npm レジストリに届かないときは
落ちるが、`check` ジョブの `npm ci` と同じ依存であり、新しい種類の risk ではない。

## Rust 評価器の取得

quint 0.32.0 の既定の評価器（Rust、`v0.6.0`）は npm の包みに入っておらず、初めて使うときに
`~/.quint/rust-evaluator-v0.6.0/` へ取得される。取得は認証なしの
`https://api.github.com/repos/informalsystems/quint/releases` の 1 ページ目（30 件）から探す。
新しいランナーでは毎回この API を呼ぶため、レート制限（認証なしは IP ごとに 1 時間 60 回）で落ちうる。
さらに、2026-11-26 の時点で `evaluator/v0.6.0` は 1 ページ目の 8 件目で、リリースがあと 23 件
増えると 1 ページ目から外れ、`Release v0.6.0 not found` で毎回落ちるようになる。

そこで、ジョブは次のようにする。

- 評価器の版は、入った quint の `QUINT_EVALUATOR_VERSION` から読む（lockfile と自動で揃う）
- `actions/cache` で `~/.quint/rust-evaluator-<版>/` を残す（鍵は OS、CPU、版）
- キャッシュが無いときは、ジョブのトークンで `gh release download` して置く。quint は実行ファイルが
  あれば取得しないので、API を呼ばない（空の HOME で流して、`Fetching Rust evaluator` が出ないことを確認）

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
- 範囲外：`docs/design/specs/` の仕様（`crates/*/specs/` の外）は対象にしない。これらは 1 ファイルに
  複数のモジュール（`fixed`／`buggy`）を持ち、`--main` と `--match` と不変条件の名前（`inv`）を仕様ごとに
  指定しないと検査にならない（`--match` が無い `quint test` は 0 件で exit 0）。負の対照（`buggy` で
  違反が出ること）も要る。glob で一律に流す形に合わないので、
  [#213](https://github.com/j5ik2o/fraktor-ng-rs/issues/213) で扱う。仕様の中身は変えない。
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
- 仕様が `invariant` を持たない → `quint run` が `Name 'invariant' not found` で落ちる（実測）
- 不変条件に型を書く（`val invariant: bool = …`）→ 判定を置かないので、そのまま `run` が流れる。
  反例を入れた写しで落ちることを実測した
- ファイル名と module 名がずれた仕様 → 0.32.0 は 1 ファイル 1 module ならそのまま
  typecheck・test・run する（`zz_name.qnt` に `module zz_run_break` を置いて実測）。ずれ自体は
  落とさないので、名付けはレビューで見る。今の 3 本は一致している
