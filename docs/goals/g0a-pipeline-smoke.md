# ゴール G0a：台帳パイプラインの通し（最初の縦切り）

状態: 定義済み・未発行（2026-11-11）。根拠は承認済みグラフモデル提案の 5.2。
目的はプロダクトの前進ではなく、**台帳の状態遷移・完了判定・ロールアップの
一巡を、実物で検証すること**。

## 対象ノード（2件）

| id | ケース数（近似） |
|---|---|
| `actor-tests/src/test/scala/org/apache/pekko/actor/ActorPathSpec.scala` | 15 |
| `actor-tests/src/test/scala/org/apache/pekko/actor/RelativeActorPathSpec.scala` | 4 |

選定理由：TestKit も ActorSystem も使わない（テスト基盤の層なし）。
依存するのは ActorPath の値の意味論だけで、実行基盤の暫定方針にも触れない。

## 前提（発行時点で揃っていること）

- クレート構成は案C＋port & adapter 段階1で決定済み
- 台帳変換器は状態保持型に改修済み、ロールアップ／完了判定は
  `ledger/tools/rollup.py` で計算できる

## 仕事の内容

1. ワークスペースの骨格を作る：`crates/fraktor-actor`（kernel／adaptor の
   モジュール分けを含む、port & adapter 段階1の形）。他のクレートは作らない
2. 2件のテストを移植する。置き場所は案Cの規則により
   `crates/fraktor-actor/tests/` にノード1件＝ファイル1件。
   移植時にケースの実数を数え、台帳の `cases` に記録する
3. テストが赤くなることを確認する（red）。台帳の status を red にする
4. ActorPath の最小実装で緑にする（green）。台帳の status と `rust` 欄
   （crate と test_file）を更新する
5. `rollup.py --goal G0a` で完了判定が 0 を返すことを確認する

## write_scope

- `crates/fraktor-actor/**`
- `ledger/nodes.yaml`（ただし対象2ノードの欄のみ。他ノードへの変更は差分で検出され、
  完了条件を満たさない）
- ワークスペース直下の `Cargo.toml`、`rust-toolchain.toml` 等の初期設定ファイル

## provider

基盤ゴール扱いでクラウド（承認済み論点19の配分）。実効並行は1本でよい。

## 完了条件（機械判定できる形）

- `cargo test -p fraktor-actor` が通る
- `python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal G0a` が 0 を返す
- 台帳の差分が対象2ノードに限られる
- 2ノードの `cases.total` が実数で記録されている（近似のままにしない）

## やらないこと

- ActorSystem、TestKit、executor の実装（G0b の仕事）
- 他のクレートの新設、台帳スキーマの変更
- 旧 fraktor-rs からのコードの持ち込み

## 発行コマンド

Mac 上の clone で、最新を取り込んでから発行する。フラグの形は旧 fraktor-rs の
実績（`--pipeline --skip-git -w <workflow> -t <task>`）に合わせた。

```sh
cd /Users/j5ik2o/Sources/github.com/j5ik2o/fraktor-ng-rs
git pull

takt --pipeline --skip-git -w pekko-porting-ng -t "docs/goals/g0a-pipeline-smoke.md のゴール定義に従って G0a を実施してください。完了条件は同文書の機械判定4項目です。対象ノード以外の台帳変更と、範囲外のクレート新設は禁止です。"
```

ワークフロー `pekko-porting-ng` は `.takt/workflows/` に実装済み
（設計は `docs/decisions/takt-workflow-design.md`）。
`.takt/config.yaml` は最小構成（provider: codex、concurrency: 1、base_branch: main）
を置いてある。

前提として、run から Pekko v1.7.1 のソースを参照できること。clone の隣に
`../pekko` としてタグ `v1.7.1` をチェックアウトしておく（triage が原典を
参照できなければ ABORT して報告する）。

run の後に人間が確認するコマンド：

```sh
cargo test -p fraktor-actor
python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal G0a
git diff --stat ledger/nodes.yaml
```
