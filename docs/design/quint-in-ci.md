# 設計：CI で Quint 仕様を検査する（Issue #214）

[Issue #214](https://github.com/j5ik2o/fraktor-ng-rs/issues/214) の設計メモである。
「小さな修正」の扱い（設計は数行、状態遷移の変更ではないので Quint の新規モデルは無し）。

## 何を足すか

| ファイル | 変更 |
|:---|:---|
| `package.json` / `package-lock.json` | `@informalsystems/quint` を `0.32.0` に固定して devDependencies へ足す（`npm ci` で再現） |
| `.github/workflows/ci.yml` | `quint` ジョブを足す。`check` とは別ジョブで並行に走らせる |
| `scripts/quint-check-specs.sh` | 仕様を検査する本体。ジョブはこれを呼ぶだけ。手元でもリポジトリ直下で `bash scripts/quint-check-specs.sh` で同じものが流れる |
| `docs/design/quint-in-ci.md` | この文書 |

スクリプトは、次を `crates/*/specs/*.qnt` のそれぞれに流す。

1. `quint typecheck`（型・未定義名の検査）
2. `quint parse` の IR から main モジュールの `run` の名前を取り、決まりを確かめる（下の「`run` の決まり」）
3. `quint test`（名前が `Test` で終わる `run` の `expect` / `fail`）。通った件数が `run` の数と同じであることも確かめる
4. `quint run --invariant=invariant --max-steps=20 --max-samples=10000`（短い模擬。どの仕様にも流す）。
   どのトレースも 1 手も進まなかったとき（`Trace length statistics: max=1`）は落とす

1 本が落ちても残りの仕様は流し、最後に落ちた仕様を `::error file=…::` で並べて非ゼロで終える。

### `run` の決まり

`quint test` は `--match` が無いと、名前が `Test` で終わる定義だけを流す（0.32.0 の
`cliHelpers.js` の `isMatchingTest`）。名前が `Test` で終わらない `run` は、期待が壊れていても
黙って外れ、必須チェックが通っていた（設計の独立レビューの F1。`rejectedStaysAtHead` に改名して
期待を壊した写しで、前の版は exit 0）。`--match='.*'` は `run` 以外の定義まで流して落ちるので使えない。

そこで、`crates/*/specs/*.qnt` の仕様は次を守る決まりにし、スクリプトが確かめる。

- main モジュールに `run` が 1 本以上ある（無ければ落とす。`run` が無いと `quint test` は 0 件で exit 0 になる）
- どの `run` も名前が `Test` で終わる（終わらないものがあれば、その名前を出して落とす）

`run` の名前は、正規表現で探さず `quint parse` の IR（`qualifier == "run"`）から取る。
main モジュールは quint と同じく「ファイルの唯一のモジュール、無ければファイル名と同じ名前のモジュール」とする。
IR には import 先のモジュールも入るので、ファイル名と違う名前の 1 モジュールが別ファイルを import して
いる仕様は見分けられない。その場合は推測せず落とす（`cannot tell the main module`）。
さらに `quint test` の `N passing` が `run` の数と違えば落とすので、quint の選び方が想定と
ずれても黙っては通らない。

`crates/*/specs/*.qnt` の仕様は、どれも `invariant` という名前の不変条件を定義する決まりにする。
`run` を飛ばす判定は置かない。名前が無い仕様は `quint run` が `Name 'invariant' not found` で
落ちる。前の版は `val invariant =` を正規表現で探し、無ければ飛ばしていたが、`val invariant: bool =`
のように型を書くと見落とし、反例があっても必須チェックが通っていた（PR #216 の Devin の指摘）。
Quint の名前解決（import、main モジュールの決め方）を手で真似ると同じ種類の見落としが残るので、
判定をやめて、無ければ落ちる側に倒した。不変条件の無い仕様を置く必要が出たら、そのときにジョブを直す。
`init` や `step` を持たない補助のモジュールを `specs/` に置いた場合も、そのファイルが `run` の段で
落ちる（`Name 'init' not found`）。置く必要が出たら、そのときにジョブを直す。

### 1 手も進まない仕様

どの初期状態からも `step` が一度も起きない仕様では、`quint run` は不変条件を初期状態でしか見ずに
通る（レビューの F2。`step` の前提を満たせなくした写しで `max=1`、exit 0）。`run` の段の出力の
`Trace length statistics: max=N` を読み、N が 1 以下なら落とす。この行が出なければ（quint の版を
上げて出力が変わったとき）も落とす。今の 3 本の max は 9、21、11。

### seed：乱数のままにする

`quint test` も `quint run` も seed を渡さず、毎回ちがう乱数で流す。同じコミットでも、まれにしか
出ない違反は、ある回だけ出ることがある。固定しないのは、回を重ねるほど広く探せるからで、出た違反は
必ず本物の反例である（乱数で偽の違反は出ない）。違反が出たら、ログの
`Use --seed=0x… --backend=rust to reproduce.` の seed で同じ反例が出る（レビューで実測）。
関係の無い PR で初めて出たときは、その seed と反例を付けて issue を立て、仕様か実装を直す。
`docs/design/specs/` の重い仕様（1 回 50 秒ほど）は seed を固定する方針で、
[#213](https://github.com/j5ik2o/fraktor-ng-rs/issues/213) で扱う。

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
スクリプト全体が約 10 秒（ローカル）、CI のジョブ全体が約 20 秒で、`check` ジョブ（約 5 分）と並行に
終わるので、必須にしても待ち時間は増えない。npm レジストリに届かないときは
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
- `actions/cache` で `~/.quint/rust-evaluator-<版>/` を残す（鍵は OS、CPU、版、実行ファイルの digest）
- キャッシュが無いときは、ジョブのトークンで `gh release download` して置く。quint は実行ファイルが
  あれば取得しないので、API を呼ばない（空の HOME で流して、`Fetching Rust evaluator` が出ないことを確認）
- リリースの資産は差し替えられる（`evaluator/v0.6.0` は `isImmutable: false`）。そこで、版ごとに
  tar.gz と実行ファイルの sha256 を `ci.yml` に固定し、取得した tar.gz は展開の前に、実行ファイルは
  キャッシュから戻したときも含めて毎回、`sha256sum -c` で確かめる。quint を上げて評価器の版が
  変わると、digest が無いので落ちる。そのときは新しい版の digest を足す
- 資産は `x86_64-unknown-linux-gnu` だけを取るので、Linux の X64 以外のランナーでは最初の段で落とす

`check` ジョブに足さず別ジョブにしたのは、Rust のビルドキャッシュを待たずに並行で終わり、
落ちたときにどちらが原因か名前で分かるため。

## 守る不変条件

- 版は `package-lock.json` の `quint 0.32.0` だけを使う（`./node_modules/.bin/quint` を直接呼び、
  `npx` のように別版を取りに行く経路を使わない）
- 検査を弱めない：`typecheck` の失敗、`test` の失敗、`run` の不変条件違反は、どれもジョブを
  非ゼロで終わらせる
- 黙って外れる検査を作らない：`quint test` が拾わない `run`、`run` が 0 本、1 手も進まない模擬、
  main モジュールが決められない仕様は、どれも落とす
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
`scripts/quint-check-specs.sh` と `docs/design/quint-in-ci.md` を消す。他に依存するものは無い。

## 状態遷移

ワークフローは「仕様を検査する」だけで、モデル化する状態遷移を増やさない。Quint の新規モデルは不要。

## 壊しうる入力（検査そのものに対して）

- 仕様が 0 本（glob の取りこぼし、crate 名の変更）→ 明示的に落とす
- 仕様が別ファイルを import する → 相対 import は取り込む側のファイルを起点に解決されるので、
  どこから `quint` を呼んでも解決する（レビューで実測）
- 仕様が `invariant` を持たない → `quint run` が `Name 'invariant' not found` で落ちる（実測）
- 不変条件に型を書く（`val invariant: bool = …`）→ 判定を置かないので、そのまま `run` が流れる。
  反例を入れた写しで落ちることを実測した
- ファイル名と module 名がずれた仕様 → 0.32.0 は 1 ファイル 1 module ならそのまま
  typecheck・test・run する（`zz_name.qnt` に `module zz_run_break` を置いて実測）。ずれ自体は
  落とさないので、名付けはレビューで見る。今の 3 本は一致している。ずれていて別ファイルを import
  する仕様は、main モジュールを決められないので落とす
- 1 ファイルに複数のモジュールを持ち、どれもファイル名と違う（`docs/design/specs/` の形）→
  `cannot tell the main module` で落とす（`double_linked_list_test.qnt` を写して実測）
- 名前が `Test` で終わらない `run` → その名前を出して落とす（実測）
- `run` が 0 本 → 落とす（実測。`quint test` だけなら 0 件で exit 0）
- `step` が一度も起きない → `max=1` で落とす（実測）
- 手数の上限（20 手）より深くでしか破れない不変条件 → 通る。意図した限界で、深い仕様が増えたら
  その仕様の手数を上げる
