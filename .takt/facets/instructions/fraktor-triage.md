ゴール定義文書（タスク本文で指定される）に列挙されたノードのうち、
台帳 `ledger/nodes.yaml` の `status` が `green` でも `do_not_port` でもない
最初の1件について triage を行ってください。対象を選ぶ前に、必ず台帳で
列挙された各ノードの `status` を確認してください。`green` のノードは
処理済みであり、再び triage や port-test や verify_red へ投入しません。
列挙されたノードがすべて `green` または `do_not_port` の場合は、
処理対象が残っていないことを報告して終了してください。
編集はしません。読み取りと報告だけです。

## 参照するもの

- ゴール定義文書（タスク本文でパスが指定される）の write_scope 節。
  前提基盤の判定（bootstrap か前提不足か）に使う
- 台帳 `ledger/nodes.yaml` の対象ノード
- Pekko 原典（v2.0.0）：`python3 ledger/tools/pekko_baseline.py source-root` が出す
  場所だけを使う（`PEKKO_ORIGINAL_ROOT`、無ければ作業ツリー直下の `pekko/`。後続の
  verify_red と台帳記録も同じ場所を使う）。このコマンドは、変更のない基準 commit で
  なければ終了コード 1 になる。そのときや、別の場所（`../pekko` など）しか無いときは、
  原典を参照できないものとして扱う。ノード id のファイルと、そのテストが検証する
  実装側のソースを見る
- 原典を参照できなければ、「原典を参照できない」と報告して
  終了してください（ABORT 相当）

## 調べること

1. ケースの実数：テストメソッドの数を数える。台帳の
   `pekko.test_cases_approx`（近似）との差も記録する
2. needs：このテストが要求する TestKit 能力（TestProbe、expectMsg 系、
   EventFilter 等）と前提フラグ（Java 直列化、multi-JVM 等）
3. 前提基盤：台帳の `depends_on_impl` と `needs`（seed 値。null は
   未抽出）、原典の依存から必要基盤を特定する。未整備（未作成または
   未 green）の基盤があるときは、ゴール定義文書の write_scope と
   突き合わせる。その作成が write_scope 内なら bootstrap として扱い、
   前提不足にしない。write_scope 外なら前提不足（ゴール選定の順序ミス）。
   needs の実測が台帳の値と違えば差を報告する
4. 除外の要否：プラットフォーム依存などで移植できないケースが
   あるか。ある場合は「除外提案」として理由つきで報告する。
   自分で除外を決めたり台帳に書いたりはしない。
   ただし、台帳の `cases.excluded` に既に項目がある場合は、人間が
   `approved_by` と `approved_at` の両方を記録した要素だけを承認済みの除外として扱う。
   その範囲では提案ではなく承認済みとして扱い、新たな除外が不要なら通常どおり進めてよい。
   承認記録（どちらか一方の欠落・空欄・null・不正な承認日）がそろっていない除外は
   未承認として扱い、needs_human に戻す

## 判定の優先順位

複数に該当する場合は次の順で判定する。(1) 原典を参照できない → ABORT。
(2) 未整備（未作成または未 green）の前提基盤に、作成が本ゴールの
write_scope の外のものが 1 つでもある → 前提不足（ゴール選定の順序ミス、
ABORT）。(3) 未承認の除外提案がある → needs_human（除外の判断は人間だけが
行うため、bootstrap に優先する）。(4) 未整備の基盤がすべて write_scope 内 →
bootstrap として進む。(5) すべて green → 通常どおり進む。

## 報告

output-contract `triage-report` の形式で報告してください。
前提基盤の状態は「すべて green」「bootstrap」「前提不足」のいずれかを
明記してください。
