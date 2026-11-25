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
  status: unported                 # unported | red | green | do_not_port | not_built
  triage:
    kind: pending                  # pending | as_is | adapt | do_not_port | not_built
    reason: null                   # do_not_port と not_built のとき必須。人間だけが書ける。run は already_green の理由も書く（下記）
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
    excluded: []                   # 人間が承認した除外。要素は {name, reason, approved_by, approved_at}
    not_built: []                  # 作らない区分のケース（下記）。要素は {name, reason, alternative, approved_by, approved_at}。欄が無いときは空と同じ
  rust:
    crate: null                    # 移植先クレート（決まり次第）
    test_file: null                # 移植した Rust テストのパス
  goal: null                       # TAKT manager のゴール識別子（割り当て時に記入）
  verified_against: null           # 照合に使った Pekko の基準タグ。判定済みの status で必須（下記）
```

## 承認内容との対応

| 承認論点 | この叩き台での扱い |
|---|---|
| 1. ノードはテストクラスファイル 1 件 | 1 ファイルを 1 ノードにする。全 1,262 ノード |
| 2. 一意キーはファイルパス、`<module>:<class>` は表示名 | `id` と `name` |
| 3. 種別 `test` と `fixture` | `kind` |
| 4. ケース単位の除外はノード属性、決めるのは人間だけ | `cases.excluded` と、勘定に要る `cases.total`、`cases.ported`、`cases.passing` |
| 8. import 近似は順序付けに使わない | `depends_on_impl.modules` はヒントである。ゲートにしない |
| 20. `docs` は do_not_port（2026-11-13 決定） | `docs` の 173 ノードは `do_not_port`。rollup は除外率に計上し、合格率の分母から除く |
| 作らない区分（オーナー決定、元帥の項目223、2026-11-25） | `status: not_built` と `cases.not_built`。rollup は作らない率とケース数を別に出し、合格率の分母から除く（下記「作らない区分」） |

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
- `excluded` は人間が承認した除外の一覧である。要素は `name`（Pekko 側のケース名）、
  `reason`、`approved_by`（承認した人間。空白以外の文字列）、`approved_at`
  （承認日。`YYYY-MM-DD` の暦日）を持つ。書けるのは人間だけである。run は候補と理由を出すだけにする
- `approved_by`・`approved_at` は人による承認の記録である。run は承認者を推測・捏造・代行しない。
  承認を記録するのは人間である
- `approved_by` と `approved_at` のどちらかが欠けた要素、空欄・null の要素は未承認として扱う。
  `approved_at` は暦日だけを受理し、時刻を含む timestamp は受理しない
- 承認記録のない既存の除外（`name`・`reason` だけの要素）は、記録を足すまで有効な除外にならない
  （fail closed）。つまり `verify_red.py` の `count_excluded` は未承認の要素が 1 件でもあれば
  件数を返さず、`already_green` にしない
- `count_excluded` は対象ノードの `cases.excluded` だけを検査する。同じノードの `cases` の外や、
  除外要素の下位（`metadata` など）にある承認欄は流用しない
- `count_excluded` は PyYAML を呼ぶときだけ遅延 import する。`verify_red` を import する
  `record_already_green.py`（`uv run python3`、`--with pyyaml` なし）は、除外の検査を行わない
  限り PyYAML なしで動く
- `not_built` は作らない区分にしたケースの一覧である（下記「作らない区分」）。要素は `name`（Pekko 側の
  ケース名）、`reason`、`alternative`（同じ振る舞いを得る Rust のコードの例）、`approved_by`、
  `approved_at` を持つ。書けるのは人間だけで、承認の欄の扱いは `excluded` と同じである
- green の条件は、`passing` が `ported` に等しく、かつ `ported` と承認済みの `excluded` の件数の和が
  `total` に等しいことである
- 候補の出し方は TAKT のワークフロー側で決める（未定）

### 作らない区分（`not_built`）

定義：原典のテストが検証しているのが原典の言語（Java、Scala）のための補助そのもので、観測できる
振る舞いを Rust の言語機能・標準ライブラリ・既存の API で満たせるため、対応する公開面を作らない。

- 当てはめる条件は次の2つを両方満たすことである。(1) 原典のテストが検証しているのが、原典の言語の
  ための補助そのもの（例：Java のラムダや型消去を補うビルダー）であること。(2) そのテストが観測する
  振る舞いを、Rust の言語機能（順序つきの `match`、ガード、`if let`）、標準ライブラリ（`Option` など）、
  既存の公開 API で満たせること。満たせることは、ケースごとの `alternative` のコード例で示す
- 既存の型への移植を先に検討する。観測できる振る舞いを持つものは移植する
- 判定はケース単位で行う。候補の一覧は未判定として扱い、まとめてこの区分に入れない。1件ずつ
  `reason` と `alternative` を書いたものだけを入れる
- 実行時の契約、JVM の代替の設計が絡むもの、Java からの到達性を確かめるだけのもの、Java の文書の
  サンプルは、この区分に自動では入れない
- 除外（`excluded`。原典と等価に検証できないケース）と移植しない（`do_not_port`。モジュールや
  ノードごと対象外にする決定）とは別の区分である。3つの件数を混ぜずに記録する
- 全ケースが作らない区分のノードは `status: not_built`、`triage.kind: not_built` にし、`triage.reason`
  に理由とゴール文書の場所を書く。`cases.total` は原典のケース数、`ported` と `passing` は 0、
  `cases.not_built` の件数は `total` に等しい。`rust` の欄は null のまま
- 一部のケースだけが作らない区分のノード（混在）は、残りのケースを移植し、status は移植の勘定で
  決める。green の条件は `ported` と承認済みの `excluded` と `not_built` の件数の和が `total` に
  等しいことになる。この勘定は `verify_red.py` と `verify_green.py` にまだ入っていない。最初に混在の
  ノードを扱うゴールで入れる
- `rollup.py` は、`not_built` のノードの記録を検査し、欠けたノードを一覧に出してゴールの完了に数えない。
  検査するのは、`triage` と `cases` がマッピングであること、`triage.kind` と `triage.reason`、`ported` と `passing` が整数の 0、
  `total` が正の整数であること、`cases.excluded` が空であること（同じケースを除外と作らないに二重に置かない）、`cases.not_built` が配列であること、`cases.not_built` の件数が `total` に
  等しいこと、各要素の `name`・`reason`・`alternative` が空白以外の文字列であること、`name` が
  前後に空白を持たず、ノード内で重複しないこと、承認の2欄が除外と同じ `excluded_approvals.is_approved_by_human`
  （空白以外の文字列の承認者、時刻を含まない実在の暦日）を満たすことである。台帳は除外の検査と
  同じ専用ローダで読む
- `rollup.py` のケースの勘定は、除外と作らないについて、承認記録の有効な件数と台帳に書かれた件数を
  分けて出す。作らないの有効な件数は、記録の揃ったノードの要素だけを数える。status が not_built のノードの除外は、有効な件数に数えない。
  マッピングでない記録は数えず、止まらずに診断へ回す（ほかのゴールの判定を続ける）
- TAKT の triage（`.takt/facets/instructions/fraktor-triage.md`、`.takt/workflows/fraktor-port-node.yaml`）は、
  記録の揃った not_built を処理済みとして移植し直さない。対象ゴールに属する、記録の不足した not_built は
  needs_human にする。判定には `rollup.py --goal` のそのゴールの未完了一覧を使い、ほかのゴールのノードの
  記録不足では止まらない

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
| not_built への遷移と、その取り消し | `triage.reason` と、全ケースの `cases.not_built`（理由、代替例、承認）を記入した | 人間だけ（記入の作業は担当が行い、承認の欄には人間の決定を写す） |

- TAKT の manager / run は、ノードを do_not_port にも not_built にもできない
- 履歴は git の差分で追う。台帳内に履歴配列は持たない
- `triage.reason` は、do_not_port の理由（人間だけが書ける）に加えて、run が
  `already_green: ` で始まる理由も書く。already_green は移植が正しく前提の実装が契約を
  満たしたことを確かめて green にした記録であり、status は green のまま（do_not_port に
  しない）。この判定は上の green の条件（`passing == ported` かつ
  `ported + len(excluded) == total`）と同じ勘定で行い、承認済みの除外は緑の判定に含める。
  run が書くのは `triage.reason` が null または空のときだけで、人間が書いた
  既存の理由は上書きしない。この用途は do_not_port の人間専用規則を変えない

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
  cases、rust、goal、verified_against）は id 一致で引き継がれる。状態を持つノードが新しい baseline に
  存在しない場合は、変換器は黙って落とさず、その id を出して失敗する
- `ledger/sample-actor-tests.yaml` は actor-tests の先頭 5 ノードのサンプルである
  （同じ変換器に `--module actor-tests --limit 5` を付けた出力）

## `verified_against`（照合済みタグ、2026-11-25 追加）

基準タグの切り替え（`docs/baseline/README.md`。2026-11-25 に v1.7.1 から v2.0.0）の後も、
ノードごとに「どのタグの原典と照合した結果か」を機械で判別するための欄である。

- 値は Pekko の公式タグ文字列か null。使えるタグは `tools/pekko-source/pekko-baseline.env` の
  現行タグ（`PEKKO_BASELINE_TAG`）と過去の基準（`PEKKO_HISTORICAL_BASELINES`）だけである
- 判定済みの status（green、do_not_port、not_built）のノードは必ずタグを持つ。
  それ以外の status（unported、red）のノードは欄を持たないか null にする
- 「現行タグで未照合」は、判定済みで `verified_against` が現行タグと違うノードである。
  `pekko_baseline.py check-ledger --list-unverified` が一覧を出す
- 2026-11-25 の切り替え時点の判定済みノード（green 50、do_not_port 173）は、過去の照合の
  記録として `v1.7.1` を持つ。v2.0.0 との照合を終えたノードだけ、その行を `v2.0.0` に変える。
  照合していないノードを v2.0.0 と書き換えない
- 新しく green にするノードは、照合に使った原典から決めたタグを書く。原典の場所は全ステップで
  `pekko_baseline.source_root()`（`PEKKO_ORIGINAL_ROOT`、無ければ `pekko/`）である。
  `verify_red.py` は数える前に、`record_already_green.py` は記録の前に `pekko_baseline.source_tag` で
  原典を検査し、ledger-update は `pekko_baseline.py source-tag` の出力を書く。HEAD が現行の基準
  commit でないか、追跡ファイルの変更・無視されない未追跡ファイル・assume-unchanged /
  skip-worktree の印があれば失敗し、green を記録しない（旧原典や手を加えた原典で照合したノードに
  現行タグを付けない）。`record_already_green.py` は、verify_red の結果が出所
  （`source_tag`、`source_commit`、`source_root`）を欠くとき、出所が不正なとき、出所が今の原典の
  場所・現行のタグと commit に一致しないときも拒否し、verify_red のやり直しを求める（出所の無い
  旧形式の結果を、置き直した現行の原典のタグで記録しない）。拒否されたら workflow は
  ledger-update へ進まない
- 行の位置はノードの最上位、`goal` の次である。欄が無いノードに書くときは、ノードの末尾に足す
- 検査は `uv run --with pyyaml python3 ledger/tools/pekko_baseline.py check-ledger --ledger ledger/nodes.yaml`
  で、違反があれば終了コード 1 になる。CI でも実行する

## 退役と引き継ぎ（`retired`、`cases.carried`、2026-11-25 追加）

基準の切り替えで、原典のファイルが現行の基準に無くなり、ケースが別のファイルに移ったノードの扱いである。最初の例は `ActorSystemScala213PlusSpec`（v2.0.0 には無く、同じケースが `ActorSystemSpec` にある）。

- 退役したノードは、`nodes` から台帳の最上位の `retired` へ移す。ノードの欄は照合した当時のまま残し、`retired` の下に `at`（暦日）、`reason`、`successor`（ケースを引き継いだ現行のノードの id）を足す。`retired` は集計（rollup の N、green、ケース数）と照合（`pekko_baseline.py check-ledger`）の対象外である。`goal` も残すので、閉じたゴールの文書の表は退役したノードを指したままになる
- 後継のノードは、引き継いだケースを `cases.carried` に記録する。要素は `name`（Pekko 側のケース名）、`from`（退役したノードの id）、`source`（現行の原典のパスと行）、`rust_test`、`verified_against`（そのケースを照合したタグ）、`evidence`（確かめた実行）である。ノード最上位の `verified_against` は suite 全体の照合を表すので、部分移植のノード（status が unported か red）には書かない
- 部分移植の後継は、`ported` と `passing` を引き継いだ件数にし、`total` は原典のケース数にする（`total > ported`）。suite 全体は green にしない。退役したノードは集計から外れるので、ケースは二重に数えない
- `rollup.py` の `lineage_problems` が検査する。退役したノードについては、同じ id が `nodes` に無いこと、3 つの欄、後継が現行のノードで、その `cases.carried` に自分からの要素があることを見る。`cases.carried` の要素は、欄が揃い、`from` が退役したノードで、名前が重複しないこと。部分移植の後継は、上の件数の規則を満たすこと。不足は一覧に出し、そのノードが属するゴールの完了に数えない。属するノードがすべて退役したゴールは、`rollup.py --goal` が退役を理由に 1 を返す（完了と判定しない）
- 検査の細則（PR #169 のレビューで足した）：`retired` は配列で（`{}` などは不正）、id は空でない文字列で一意である。`retired.at` は書式に加えて実在する暦日であること（`2026-02-30` や時刻つきは拒否）。`cases.carried` の各要素は、`from` の退役記録の `successor` がそのノード自身であること（指定された後継のほかに同じケースを置くと二重に数えるので拒否）、`verified_against` が基準ファイルの知るタグであること、ケース名は前後の空白を除いて重複しないこと。欄の型が違っても例外では止めず、診断にする
- 二重計上の防止（PR #169 の再レビューで足した）：同じ退役元・原典の位置（`source`）・Rust テスト（`rust_test`）の組を、名前だけ変えて 2 回置けない。指定された後継に置いた引き継ぎの件数は、退役元のノードの `cases.ported`（整数）を超えられない。同じ退役 id が重複して別々の後継を指すときは、最初の後継も含めて、その id を後継として指すすべてのノードに異常を伝える。引き継ぎを持つ green の後継も、ほかの green と同じく `total = ported + 承認済みの除外` と `passing = ported` を満たすこと（引き継いだ 1 件だけで suite 全体を green にできない）
- 引き継ぎを持てる status は、部分移植の `unported` か `red`、または件数のそろった `green` だけである。`do_not_port` や `not_built` など、ほかの判定済みの status にすると記録の不足として扱い、ゴールの完了にも能力の供給元にも数えない（PR #170 のレビューで足した）。どの status でも、引き継いだケースと同じ名前を `cases.excluded` に置くことはできない（同じケースを移植と除外の両方に数えない）
- ゴールの関門は、対象表の id を、現役のノードと退役したノード（同じ `goal` を持つもの）の両方と照合する。一部のノードだけが退役したゴールも、現役のノードがすべて完了していれば完了になる
- 後継の suite を移植するとき（記録の方法）：移植の途中は、後継を部分移植のまま（`unported` か `red`、`ported = passing = 引き継いだ件数`）に保ち、新しく移植したケースの件数だけを途中で台帳に足さない（足すと部分移植の件数の検査で拒否される）。引き継いだケースの Rust テスト（`cases.carried` の `rust_test`）はそのファイルに残し、後継の `rust.test_file` には残りのケースだけを移植する。すべてを移植したら、引き継いだケースと新しいケースの両方を同じ実行で確かめ（例：`cargo test -p <crate> --test <後継のテスト> --test <引き継いだテスト>`）、その結果で green への遷移を 1 回で記録する。`ported` と `passing` は両方の和、`total` は原典のケース数、承認済みの除外があれば合わせて total になる。green になった後継は、引き継ぎを持つ green の件数の検査（`total = ported + 有効で一意な承認済みの除外`、`passing = ported`、`ported >= 引き継いだ件数`）を通る。`verify_red` と `verify_green` は、引き継いだケースのファイルを別に数えない（`rust.test_file` だけを実行する）ので、この遷移は TAKT の already_green では記録せず、上の実行の結果を根拠にして台帳を更新する。後継のゴールを発行する前に、この手順を移植の指示に書く（例：`ActorSystemSpec` は 20 件のうち 1 件を `actor_system_scala213_plus_spec.rs` で引き継いでいる。残りの 19 件は、この PR では移植しない）
- 台帳の行単位の書き換え（`ledger/tools/ledger_lines.py`。verify_red の mark_green と record_already_green が使う）は、`nodes:` の外の最上位の行（`retired:` など）でノードの範囲を終える。末尾の現役ノードに欄を足しても `retired` の中に入らず、退役したノードを書き換えない
- `check_needs.py` は、退役と引き継ぎの記録に不足のあるノードを持つゴールを、能力の供給元（完了したゴール）に数えない。`retired` が配列でない台帳でも、`rollup.py --goal` は例外では止まらず、診断を出して 1 を返す
- 退役側だけの不正も、その `successor` のノードに理由を付けるので、後継が属するゴールは完了と判定しない。無関係なゴールは止めずに判定する。ゴールを指定しない `rollup.py` と `pekko_baseline.py check-ledger`（CI の通常の入口）は、不正があれば 0 以外で終わる
- ゴールの関門（`goal_complete_gate.py`）は、現行のノードに属するものが無く、属するノードがすべて退役したゴールでは、退役したノードの id で文書の対象表と照合する。照合を通った後、rollup が退役を理由に完了と判定しない（1）
- 台帳を扱うほかの道具：`convert_baseline.py --existing` は再生成で `retired` を残し、退役した id を現役のノードに戻さない。`extract_needs.py` は `retired` 以降を書き換えない
- 例：`actor-tests/src/test/scala-2.13+/org/apache/pekko/actor/ActorSystemScala213PlusSpec.scala` は退役し、後継は `actor-tests/src/test/scala/org/apache/pekko/actor/ActorSystemSpec.scala` である。後継は unported で、`cases` は 20/1/1、`cases.carried` に 1 件を持つ
