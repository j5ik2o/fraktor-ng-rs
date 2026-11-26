# Pekko 移植の規約（fraktor-ng-rs）

Apache Pekko v2.0.0 のテストスイートを正とする Rust 移植のステップ共通規約。決定文書の要約で、矛盾したら決定文書が優先する。

## 移植の作法
- 速さより Pekko の意味論（テストと文書が利用者に約束する振る舞い）を優先し、速さのために意味論を弱めたり強めたりしない。JVM・Scala 固有の振る舞いは1件ずつオーナーに伺う（docs/decisions/pekko-semantics-first.md）
- テスト先行。red を確認していない実装は書かない
- 実装ステップはテストファイルを編集しない。テストが誤りなら、実装でごまかさず triage へ差し戻して報告する
- 移植したテストを弱めない（アサーション削除、ケースの丸め、近似は、意味論がプラットフォーム依存の場合だけ。その場合も除外提案を人間に出す）
- untyped（classic）を先に作り、typed はその上に作る
- 旧 fraktor-rs のコードは持ち込まない
## 台帳とゴール
- 進捗の正は `ledger/nodes.yaml`。ノードはテストクラス1件。id は Pekko リポジトリ相対パス
- 1 run は1ゴール（ゴール定義文書のノード集合）を扱い、ノードは1件ずつ処理する
- 状態遷移は unported → red → green の順だけ。`do_not_port` とケース除外は人間だけが決める。エージェントは提案まで。承認なく台帳に書かない
- 人間の判断は run の外で受ける。未承認の提案が出たら、run は提案をレポートに残して needs_human で終わる。除外は、人間が対象ノードの `cases.excluded` に `approved_by` と `approved_at` の両方を書いて承認済みになり、再 run で反映してよい。欠けたら未承認で needs_human に戻す。`do_not_port` も人間が status に直接書く
- `not_built`（作らない区分）も人間だけが決める。Java・Scala 用の補助を検証し、振る舞いを Rust の言語機能・標準ライブラリ・既存 API で満たせるケースを、人間が1件ずつ理由・代替例・承認つきで `cases.not_built` に書く（エージェントは提案まで）。記録の揃ったノードは移植し直さない。定義は `ledger-schema-draft.md`
- ケース数は近似でなく実数を数えて `cases` に記録する
- 完了判定は `ledger/tools/rollup.py --goal <ゴールID>` の終了コードで行う
## クレート構成（案C＋port & adapter 段階1）
- ライブラリ本体の Pekko モジュールは1対1で `crates/` のクレートにする
- 移植テストはクレートの `tests/` 配下に、ノード1件＝ファイル1件で置く。冒頭に出典（Pekko v2.0.0 のパス）をコメントで残す。例外：内部 API を呼ぶケースは単体テストに置き、台帳の `rust.cases_outside_test_file` に書く
- 各クレートは kernel（非依存）と adaptor（std 実装）に分ける。kernel は時刻・スケジューリング・IO に Clock や Scheduler の port 越しで触れ、実装は adaptor が持つ
- write_scope の外は書かない。範囲外が必要になったら、広げず報告して止まる
- syn チェッカー（tools/syn-check）のルールを後出しにしない。同内容を workflow rule `fraktor-coding-rules` で実装前に示す。ルールの正はチェッカー側
## 所有権モデル（accepted）
- 既定は executor の単独所有＋ハンドル型。ActorRef の等しさとハッシュは、id と世代で決まる。複数スレッドの dispatcher では、送り先に Arc<Mailbox> を持ってよい（docs/decisions/ownership-model-arc-overhead.md の改訂（2026-11-26））。Context は保持させず処理時に `&mut` で貸し出す
- Arc / Rc は、共有せざるを得ない場所（制御プレーン、スレッド境界の送信端、Weak 戻り辺）に限る。Rc はシャード内のみ
- 親子・DeathWatch・購読は、共有ポインタではなく所有テーブル／登録表で表す
