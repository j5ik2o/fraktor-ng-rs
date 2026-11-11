# Pekko 移植の規約（fraktor-ng-rs）

Apache Pekko v1.7.1 のテストスイートを正として Rust へ移植する際の、
全ステップ共通の規約。設計の正は `docs/decisions/` 配下の決定文書であり、
本ファセットはその実行用要約である。矛盾したら決定文書が優先する。

## 台帳とゴール

- 進捗の正は `ledger/nodes.yaml`。ノードは Pekko のテストクラス1件で、
  id は Pekko リポジトリ相対のファイルパス
- 1 run は 1 ゴール（ゴール定義文書に列挙されたノード集合）を扱い、
  ノードは 1 件ずつ処理する
- 状態遷移は unported → red → green の順だけ。`do_not_port` とケース除外は
  人間だけが決める。エージェントは提案までで、承認なく台帳に書かない
- ケース数は近似ではなく実数を数えて `cases` に記録する
- ゴールの完了判定は `python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml
  --goal <ゴールID>` の終了コードで行う

## クレート構成（案C＋port & adapter 段階1）

- ライブラリ本体の Pekko モジュールは 1 対 1 でクレートにする
  （`crates/fraktor-actor`、`crates/fraktor-actor-typed`、
  `crates/fraktor-testkit`、`crates/fraktor-stream` 等）
- 移植テストは検証対象クレートの `tests/` 配下に、ノード1件＝ファイル1件で置く。
  ファイル冒頭に出典（Pekko v1.7.1 のファイルパス）をコメントで残す
- 各クレート内部は kernel（プラットフォーム非依存）と adaptor（std 実装）に
  分ける。kernel は時刻・スケジューリング・IO に直接触れず、Clock や
  Scheduler の port（トレイト）越しに使う。具体実装は adaptor が持つ
- ゴール定義の write_scope の外は書かない。範囲外の変更が必要になったら、
  勝手に広げず報告して止まる

## 所有権モデル（accepted）

- 既定は executor の単独所有＋ハンドル型。ActorRef は id＋世代番号の値とし、
  Context は保持させず処理時に `&mut` で貸し出す
- Arc / Rc は、どうしても共有せざるを得ない場所（制御プレーン、スレッド境界の
  送信端、Weak による戻り辺）に限る。Rc はシャード内のみ
- 親子・DeathWatch・購読は、共有ポインタではなく所有テーブル／登録表で表す

## 移植の作法

- テスト先行。red を確認していない実装は書かない
- 実装ステップはテストファイルを編集しない。テストが間違っていると
  判断したら、実装でごまかさず triage への差し戻しとして報告する
- 移植したテストを弱めない（アサーションの削除、ケースの丸め、近似への
  置き換えは、意味論が本当にプラットフォーム依存の場合だけ。その場合も
  除外提案として人間に出す）
- untyped（classic）レイヤーを先に作り、typed はその上に作る
- 旧 fraktor-rs からのコードの持ち込みはしない
