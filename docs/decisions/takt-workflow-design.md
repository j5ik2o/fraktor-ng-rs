# TAKT ワークフロー設計（移植パイプライン）

状態: **設計決定済み（2026-11-11、判断ポイント a〜d はすべて決定、ファセットは
標準継承の方針）**。YAML 実装済み（下記「現行仕様への適合」参照）。
persona と policy は標準ファセットをそのまま使う。
なお差し戻し上限（決定 c）は、TAKT の loop_monitors（cycle＋threshold＋judge）で
表現した。judge が非生産的と判定したら ABORT し、人間に上がる。

### 現行仕様への適合（2026-11-11、takt 0.74.0 で再構成）

初版 YAML は旧版 takt の同梱資料を下敷きにしていたため、0.74.0 の仕様に
合わせて再構成した。検証は `takt workflow doctor` で両ワークフローとも OK。

- **サブワークフロー化**：root の `pekko-porting-ng` はゴール単位の薄い
  ループ（`workflow_call` でノード処理を繰り返す）とし、ノード1件の処理は
  callable な子 `pekko-port-node`（`subworkflow.returns` で
  node_green／goal_complete／needs_human を返す）に分けた
- **verify は system step 化**：verify_red／verify_green は agent step では
  なく、固定コマンドを実行する system step とした。実体は
  `ledger/tools/verify_red.py`／`verify_green.py` で、分類・合否は JSON で
  返し、`when(...)` のルールで遷移する。直前ステップの対象（ノード、
  クレート、テストファイル）は `TAKT_PREVIOUS_RESPONSE_FILE` 経由で渡る。
  分類できない場合だけ、軽量な agent step（classify-red）に落ちる（決定 d の形）
- **人間チェックポイントは子の中の step に**：triage の除外提案は、
  子の human-checkpoint step（`requires_user_input`）で承認／却下を確認する。
  却下なら子は needs_human を返し、root が ABORT して人間に上がる
- **設定の置き場所**：system step のコマンド実行許可
  （`workflow_system_commands.custom_scripts: true`）を `.takt/config.yaml` に
  追加した。provider やモデルの割り当ては workflow YAML には書かず、
  runtime.yaml／config 側で持つ（0.74.0 の流儀）
- 標準 persona の改名に追随：等価性レビューの persona は `qa-reviewer`
  （旧）ではなく `testing-reviewer` を使う

### builtin 資産の活用（v3、2026-11-11）

実装⇄レビュー⇄修正のループは自前で組み直さず、builtin の **mini-core** に
委譲する形へ再構成した（backend-cqrs が development-core へドメイン
ファセットを注入するのと同じ流儀）。品質の核は takt の資産が担い、
プロジェクト側はドメイン固有の部分だけを持つ。

- 子ワークフローの develop ステップが `workflow_call` で mini-core を呼ぶ。
  plan → implement → 並列レビュー（AI アンチパターン＋supervisor の要件照合）
  → 修正ループ（履歴つき loop monitor）が mini-core の内側で回る
- ドメイン規約は引数で注入する：`plan_knowledge` と `development_knowledge`
  に `pekko-porting-rules` を加える。これで implement、supervisor、fix の
  各ステップに規約が届く。プロジェクト専用の implement instruction は
  持たない（mini-core の implement は標準 instruction 固定のため）
- プロジェクト側に残るのは、台帳と原典と機械判定にまつわる部分だけ：
  triage、人間チェックポイント、テスト移植、verify の system step、
  等価性レビュー（原典との忠実さは builtin レビューの守備範囲外なので
  独立ステップとして残す。指摘は develop の再呼び出しで直す）、台帳更新
- ファセットは `{extends:...}` で標準を継承する形にした：
  pekko-port-test は write-tests-first を、pekko-review-equivalence は
  testing-review を継承し、差分だけを書く。triage は、継承候補の
  research-analyze が調査反復型で意味が合わないため、独立のままにする
- 大きな基盤ゴール（G0b の TestKit 等）では、testing 工程と companion、
  レビュースイートを持つ development-core への格上げを検討する。
  その場合も引数の注入で切り替える
G0a の発行コマンドで参照した旧 fraktor-rs の
`pekko-porting` ワークフローをそのまま使う前提は置き直す。ng の決定済み設計から
ワークフローを設計し直すための叩き台であり、決定ではない。
なお旧ワークフローのステップ定義自体は旧リポジトリには無く（persona の割り当てが
`.takt/config-claude.yaml` に残るのみ）、本書は ng の要件からの新規設計である。

## 1. ワークフローが守る不変条件（決定済み設計から）

- 1 run は 1 ゴール（台帳のノード集合）を扱い、ゴール内ノードは 1 件ずつ処理する。
  `write_scope` の外は書かない
- テスト先行。red を確認していない実装は存在しない
- ケース除外と `do_not_port` は人間だけが決める。ワークフローは提案を出して
  人間の確認を待つ。勝手に除外して green にしない
- 台帳の状態遷移は unported → red → green の決まった遷移だけ。ケース数は
  近似ではなく実数を記録する
- 機械検証を通らない成果物はレビューに進まない（安いゲートを先に置く）
- 実装ステップはテストファイルを編集できない（テスト改変による green 化を
  構造で塞ぐ）

## 2. ステップ構成（提案）

### 2.1 triage（編集不可）

- Pekko の原典テストと実装を読み、ケースの実数、needs（TestKit 能力・前提フラグ）、
  infra 辺（前提基盤が green か）を確認する。出力は triage レポート
- 分岐：前提基盤が未 green → ABORT（ゴール選定の誤りとして manager に返す）。
  除外提案（ケース除外・do_not_port）がある → 人間確認待ちで停止。
  問題なし → port-test へ
- provider: クラウド上位モデル（解釈と判断が重い工程）

### 2.2 port-test（編集可、テストファイルのみ）

- Rust テストを書く。置き場所は案Cの規則（検証対象クレートの `tests/`、
  ノード1件＝ファイル1件）。ファイル冒頭に出典（Pekko v1.7.1 のパス）を残す
- provider: 量産期は DGX（DeepSeek V4.1 Flash）。基盤期（G0a〜G0c）はクラウド

### 2.3 verify-red（機械検証）

- `cargo test` を実行し、失敗することと、失敗理由が想定どおりであること
  （未実装によるコンパイルエラー、またはアサーション失敗）を確認する
- 最初から通る、または無関係な理由で落ちる場合は port-test へ差し戻す
- 通過したら台帳の status を red に更新する（台帳に触れるのは 2.3 と 2.7 のみ）

### 2.4 implement（編集可、write_scope 内の実装のみ）

- テストを通す最小実装を書く。テストファイルは編集不可
- instruction に、決定済みの規約を明記する：所有権モデル（単独所有＋ハンドル型、
  Arc は限定箇所のみ）、port & adapter 段階1（kernel はプラットフォームに触れない、
  Clock / Scheduler は port 越し）、クレート構成の規則
- provider: 量産期は DGX、基盤期はクラウド

### 2.5 verify-green（機械検証）

- `cargo test`、`cargo clippy`、`cargo fmt --check`、syn checker
  （port-adaptor 境界、1ファイル1公開型ほか）、aislop を実行する
- どれか落ちたら implement へ差し戻す（ループ上限つき）

### 2.6 equivalence-review（編集不可、実装と別系統のモデル）

- Pekko 原典と、移植テスト・実装を突き合わせる。見る観点は2つ：
  移植の忠実さ（テストの弱体化、ケースの取りこぼし、近似の混入）と、
  振る舞い互換（API と意味論が Pekko とず
れていないか）
- 旧 fraktor-rs の persona 名（pekko-porting-qa-reviewer、
  pekko-porting-compat-reviewer）を踏襲する
- provider: 実装者と別系統のクラウド上位モデル固定
- 指摘あり → implement へ差し戻し（ループ上限つき）。なし → ledger-update へ

### 2.7 ledger-update（編集可、台帳の対象ノードのみ）

- status を green にし、`cases` の実数と `rust` 欄（crate、test_file）を記録する
- `ledger/tools/rollup.py --goal <ID>` で完了判定する。未完なら次のノードの
  triage へ戻る。全ノード完了で COMPLETE

## 3. 人間チェックポイント

- triage の除外提案（ケース除外、`do_not_port` の提案）
- 差し戻しループの上限到達（verify 系、review 系とも）
- `write_scope` の拡張が必要になった場合（前提基盤の追加実装が要る等）。
  スコープ変更は manager と人間の判断であり、run の裁量にしない

## 4. provider 配分の原則

- 判断系（triage、equivalence-review）はクラウド上位モデルに固定する。
  「設計・契約の判断はトップのクラウドモデル」というユーザーの方針と、
  承認済み論点19（基盤ゴールはクラウド、移植ゴールは DGX）の延長
- 量産系（port-test、implement）は DGX に下ろせる形にしておくが、基盤期は
  全ステップをクラウドで回し、手順が固まってから下ろす
- 機械系（verify-red、verify-green、rollup）はモデルを使わない

## 5. 旧 fraktor-rs の pekko-porting との違い

- 検証器が dylint から syn checker に替わる（決定済み）。aislop は継続する
- 進捗の正が gap-analysis 文書から台帳に替わり、状態遷移と完了判定が
  rollup.py の機械判定に接続される
- implement ステップからテストファイルの編集権限を外した
- 除外判断のエスカレーションをステップの分岐として明示した

## 6. 判断ポイント（未決定）

a. ワークフローは全ゴール共通の1本か、基盤用／量産用に分けるか。
   **決定済み（2026-11-11、ユーザー）：当面1本**。provider の切り替えは
   ゴール種別でステップ設定側が持つ
b. equivalence-review の2観点を、並列2ステップにするか統合1本にするか。
   **決定済み（2026-11-11、ユーザー）：基盤期は統合1本**、量産期に分離を検討する
c. 差し戻しループ上限の初期値。
   **決定済み（2026-11-11、ユーザー）：verify 系は 5 回、review 系は 3 回**。
   上限到達時は人間チェックポイントへ上げる
d. verify-red で「失敗理由が想定どおりか」の判定を機械だけで閉じるか、
   軽量モデルの判定を挟むか。
   **決定済み（2026-11-11、ユーザー）：機械判定を既定とし、分類できない場合のみ
   軽量モデルに回す**

## 7. ファセットの方針（標準ファセットの継承）

ワークフローのステップが使う persona、instruction、knowledge、policy、
output-contract は、ゼロから書かず **TAKT の標準ファセットを継承し、ng 固有の
差分だけをプロジェクトファセットとして置く**（2026-11-11、ユーザー指摘）。
標準側の更新に追随でき、書く量も減る。

ステップごとの割り当て案（標準ファセット名は takt-analyzer のビルトイン参照で確認）：

| ステップ | 継承する標準ファセット | ng 固有の差分 |
|---|---|---|
| triage | persona: research-analyzer 系 | knowledge: 台帳スキーマと triage の観点 |
| port-test | persona: coder | instruction は write-tests-first を継承＋差分：出典ヘッダ、1ノード1ファイル |
| implement | mini-core 内蔵（persona: coder、標準 instruction） | プロジェクト instruction は持たず、knowledge 注入で規約を届ける |
| equivalence-review | persona: testing-reviewer（旧 qa-reviewer）、policy に ai-antipattern | instruction 差分：Pekko 原典との突き合わせ観点 |
| ledger-update | 標準の最小 persona | policy: 台帳の遷移規則と write_scope |

ng 固有の規約は各 instruction に散らさず、knowledge ファセット
（例：`pekko-porting-rules`）に集約し、必要なステップから参照する。
