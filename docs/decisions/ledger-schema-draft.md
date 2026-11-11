# 台帳スキーマ叩き台（テストクラス粒度）

状態: draft（2026-11-11）。粒度（ノード＝テストクラス）は設計設定ドラフト 3.2 の
提案デフォルトであり、Fable の判断層（依頼範囲1）で覆る可能性が残る。
覆った場合はこのスキーマも作り直す前提で、先に器を用意するもの。

## 設計原則

- 台帳が進捗の正。モジュール合格率などの集計値は台帳から**計算**するもので、
  台帳内に二重に持たない（ロールアップ定義は Fable 判断層の担当だが、器は
  「ノードの状態と件数から計算できる」形にしておく）
- baseline（`docs/baseline/pekko-v1.7.1/ledger.yaml`）から機械変換できる欄と、
  変換できない欄を分ける。変換できない欄は、無いものとして扱わず「未抽出」と明示する
- 状態遷移は TAKT の run の成果として記録する。人間が手で書き換えない

## ノード（テストクラス）スキーマ案

```yaml
- id: "actor-tests:ActorConfigurationVerificationSpec"   # <module>:<class>、一意キー
  pekko:
    module: actor-tests            # baseline の directory
    class: ActorConfigurationVerificationSpec
    file: "actor-tests/src/test/scala/org/apache/pekko/actor/ActorConfigurationVerificationSpec.scala"
    test_cases_approx: 10          # 近似であることを名前で残す
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
  rust:
    crate: null                    # 移植先クレート（決まり次第）
    test_file: null                # 移植した Rust テストのパス
  goal: null                       # TAKT manager のゴール識別子（割り当て時に記入）
```

## baseline から変換できるもの／できないもの

変換できる（`ledger/tools/convert_baseline.py` で機械変換）：

- id、pekko 配下（module / class / file / test_cases_approx）
- depends_on_impl.modules（referenced_modules。ただし取れているのは 292/1,262 クラス）

変換できない（正直な欠落として明記）：

- **needs.testkit / premise_flags のクラス単位の値**。baseline ではモジュール集計しか
  ない。クラス単位の再抽出が必要で、抽出スクリプトの拡張として別途行う
  （頻度集計の仕組み自体は baseline の抽出スクリプトにある）
- triage の判定。仕分けはタスクの最初の step で行い、結果をここへ戻す運用にする

## 状態と遷移の規則

- `unported → red`：Pekko テストを Rust に移植し、失敗を確認したとき
- `red → green`：対応する実装が入り、テストが通ったとき
- `* → do_not_port`：人間の判断でのみ遷移する。triage.reason への理由記入が必須で、
  TAKT の manager / run はこの遷移を行えない
- green の後に Pekko 側テストの理解が変わった場合は red に戻してよい（履歴は
  git の差分で追う。台帳内に履歴配列は持たない）

## サンプル

`ledger/sample-actor-tests.yaml` に actor-tests の先頭5クラス分の変換サンプルを置く。
全1,262クラス分の変換は、粒度の判断が確定してから converter で一括生成する
（判断前に全量を作ると、未確定の粒度が既成事実化するため）。
