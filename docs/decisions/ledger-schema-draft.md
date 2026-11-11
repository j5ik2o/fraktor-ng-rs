# 台帳スキーマ叩き台（テストクラス粒度）

状態: draft（2026-11-11 改訂）。ノード粒度、ID、ノード種別、ケース単位の除外は、
承認済みの `fable-graph-model-proposal.md`（approved 2026-11-11、論点 1 から 4）に基づく。
欄の名前と並びは叩き台のままで、承認の対象ではない。

## 設計原則

- 台帳が進捗の正。モジュール合格率などの集計値は台帳から**計算**するもので、
  台帳内に二重に持たない（ロールアップの定義は提案書の 5 章）
- baseline（`docs/baseline/pekko-v1.7.1/ledger.yaml`）から機械変換できる欄と、
  変換できない欄を分ける。変換できない欄は、無いものとして扱わず「未抽出」と明示する
- 状態遷移は TAKT の run の成果として記録する。人間が手で書き換えない
- 同じ事実を 2 つの欄に持たない。ファイルパスを持つのは `id` だけである

## ノード（テストクラスファイル）スキーマ案

```yaml
- id: "actor-tests/src/test/scala/org/apache/pekko/actor/ActorConfigurationVerificationSpec.scala"
                                   # 一意キー。Pekko リポジトリ相対のファイルパス
  name: "actor-tests:ActorConfigurationVerificationSpec"
                                   # 表示名 <module>:<class>。一意ではない
  kind: test                       # test | fixture。変換器は全ノードを test で出す
  pekko:
    module: actor-tests            # baseline の directory。集計キー
    class: ActorConfigurationVerificationSpec
    test_cases_approx: 10          # 近似。taggedAs つきのケースを含まない
  status: unported                 # unported | red | green | do_not_port
  triage:
    kind: pending                  # pending | as_is | adapt | do_not_port
    reason: null                   # do_not_port のとき必須。人間だけが書ける
  needs:
    testkit: null                  # 未抽出（下記）。判明したら機能名の配列
    premise_flags: null            # 未抽出（下記）
  depends_on_impl:
    modules: []                    # baseline の referenced_modules（import 近似）
    coverage: approx               # 近似であることを落とさない
  cases:                           # ケース単位の勘定。移植するまで未記入
    total: null                    # Pekko 側のケース総数。移植時に Pekko のソースから数える
    ported: null                   # Rust へ移植したケース数
    passing: null                  # 直近の実行で通ったケース数
    excluded: []                   # 人間が承認した除外。要素は {name, reason}
  rust:
    crate: null                    # 移植先クレート（決まり次第）
    test_file: null                # 移植した Rust テストのパス
  goal: null                       # TAKT manager のゴール識別子（割り当て時に記入）
```

## 承認内容との対応

| 承認論点 | この叩き台での扱い |
|---|---|
| 1. ノードはテストクラスファイル 1 件 | 1 ファイルを 1 ノードにする。全 1,262 ノード |
| 2. 一意キーはファイルパス、`<module>:<class>` は表示名 | `id` と `name` |
| 3. 種別 `test` と `fixture` | `kind` |
| 4. ケース単位の除外はノード属性、決めるのは人間だけ | `cases.excluded` と、勘定に要る `cases.total`、`cases.ported`、`cases.passing` |
| 8. import 近似は順序付けに使わない | `depends_on_impl.modules` はヒントである。ゲートにしない |
| 20. `docs` は保留 | `docs` の 173 ノードは `unported` のまま分母に残す |

## 欄の規則

### `id` と `name`

- `id` は Pekko リポジトリ相対のファイルパスそのものである。baseline の 1,262 件ですべて一意である
- `name` は表示名 `<module>:<class>` である。一意ではない。31 個の名前が 64 ノードで重複する
  （例は `remote:RemoteDeathWatchSpec` の 2 ノード）。`name` をキーに使わない
- 旧案の `pekko.file` は削除した。`id` と同じ値になるためである
- `id` は `pekko.module` のディレクトリ名で始まる
- 変換器は、`id` が一意であることと、上の接頭辞の条件を検査する。破れていれば何も出さずに失敗する

### `kind`

- `test` はテストケースを持つノードである。`fixture` はテストケースを持たない基底クラスや
  ヘルパである（例は `testkit` の `PekkoSpec`）
- 変換器は全ノードを `test` で出す。baseline には fixture を見分ける情報が無い。
  `test_cases_approx` が 0 であることは分類ではない（184 ノードが 0 だが、14 ノードは
  taggedAs つきのケースだけを持つテストである）
- `fixture` への変更は人間の承認を要する。合格率の分母を変えるためである。
  run は候補と理由を出すだけにする
- `fixture` は合格率の分母に入れない。`status` は unported、green、do_not_port の 3 つを使い、
  移植済みをもって green とする。red と `cases` は使わない

### `cases`

- `total` は Pekko 側のケース総数である。移植時に run が Pekko のソースから数えて記入する。
  taggedAs つきのケースも数える。`pekko.test_cases_approx` とは別の値である
- `ported` は Rust へ移植したケース数、`passing` は直近の実行で通ったケース数である。
  どちらも Rust 側のテスト実行結果から取る
- `excluded` は人間が承認した除外の一覧である。要素は `name`（Pekko 側のケース名）と
  `reason` を持つ。書けるのは人間だけである。run は候補と理由を出すだけにする
- green の条件は、`passing` が `ported` に等しく、かつ `ported` と `excluded` の件数の和が
  `total` に等しいことである
- 候補の出し方は TAKT のワークフロー側で決める（未定）

## baseline から変換できるもの／できないもの

変換できる（`ledger/tools/convert_baseline.py` で機械変換）：

- `id`、`name`、`pekko` 配下（module / class / test_cases_approx）
- `depends_on_impl.modules`（referenced_modules。取れているのは 223/1,262 クラス。
  参照型が取れたクラスは 292 だが、モジュールまで解決できたのは 223 である）

変換できない（正直な欠落として明記）：

- **`needs.testkit` / `needs.premise_flags` のクラス単位の値**。baseline ではモジュール集計しか
  ない。クラス単位の再抽出が必要で、抽出スクリプトの拡張として別途行う。変換器は null で出す
- **`triage` の判定**。仕分けはタスクの最初の step で行い、結果をここへ戻す運用にする。
  変換器は `pending` で出す
- **`kind` の fixture 判定**。変換器は `test` で出す
- **`cases`**。移植するまで値が無い。変換器は null と空配列で出す
- **taggedAs つきのケース数**。`test_cases_approx` は 314 ケースを数えていない。
  凍結済みの baseline は書き換えない。再計測の結果は補遺
  `docs/baseline/pekko-v1.7.1/supplement-recount.md` にある

## 状態と遷移の規則

| 遷移 | 条件 | 行える者 |
|---|---|---|
| unported から red | Rust のテストファイルを `rust.test_file` に登録し、実行して 1 件以上が失敗した | takt run |
| red から green | 上の green の条件を満たした | takt run |
| green から red | 全体の再実行で失敗した。Pekko 側テストの理解が変わった場合も戻してよい | 自動、または takt run |
| do_not_port への遷移と、その取り消し | `triage.reason` に理由を記入した | 人間だけ |

- TAKT の manager / run は、ノードを do_not_port にできない
- 履歴は git の差分で追う。台帳内に履歴配列は持たない

## 生成物

- `ledger/nodes.yaml` が台帳の本体である。全 1,262 ノードを次のコマンドで生成した

  ```sh
  python3 ledger/tools/convert_baseline.py --baseline docs/baseline/pekko-v1.7.1/ledger.yaml > ledger/nodes.yaml
  ```

- 同じ baseline からは同じ内容が出る。ただし生成は初回の種まきである。run が状態を
  書き込んだ後に同じコマンドで上書きすると、進捗が消える。以後の更新は run が行う
- 状態保持つきの再生成（2026-11-11 に変換器を改修済み）：baseline を差し替える
  必要が生じたら、`--existing ledger/nodes.yaml` を付けて変換し、出力で置き換える。
  baseline 由来の欄は作り直され、運用で育つ欄（kind、status、triage、needs、
  cases、rust、goal）は id 一致で引き継がれる。状態を持つノードが新しい baseline に
  存在しない場合は、変換器は黙って落とさず、その id を出して失敗する
- `ledger/sample-actor-tests.yaml` は actor-tests の先頭 5 ノードのサンプルである
  （同じ変換器に `--module actor-tests --limit 5` を付けた出力）
