#!/usr/bin/env python3
"""報告書と台帳の 1 対 1 を検査する道具（ledger/tools/report_ledger_1to1.py）の回帰テスト。

確かめること:
- いまの main の入力で check が 0 で終わる（報告書 3 件・台帳の v2.0.0・未記載の一覧）
- 検査ごとに、わざと壊した入力で落ちる
  （報告書の id が台帳に無い／同じ id が 2 報告書／宣言件数と行数の不一致／
  表の照合タグが台帳と違う／一覧に id を足す／一覧から id を消す）
- 表が機械で読めない報告書は飛ばさずに止まる（表が無い・セルの数が合わない・宣言件数が
  1 つに決まらない）
- 照合タグの列を持たない報告書は存在だけを見る（台帳が unported のままでも落ちない）
- 報告書の中の、報告書の表でない表（id の列だけの決定表、コードブロックの中の行）は読まない
- list-undocumented は id の順に書き、2 回目は同じ内容（冪等）。category は分類器の値を使い、
  規則外の category は拒否する

使い方:
    uv run --with pyyaml python3 ledger/tools/report_ledger_1to1_test.py
"""
import contextlib
import io
import sys
import tempfile
from pathlib import Path

TOOLS = Path(__file__).resolve().parent
sys.path.insert(0, str(TOOLS))

import report_ledger_1to1 as tool  # noqa: E402
import pekko_baseline  # noqa: E402

REPO = TOOLS.parents[1]
# 過去の基準のタグは基準ファイルから取る（現役の経路に過去のタグや commit を固定して書かない。
# check-refs の規則）。このテストは、その値を入力の文字列として使うだけで、過去の原典を読まない。
PAST = sorted(pekko_baseline.load().historical)[0]

LEDGER = """nodes:
- id: docs/src/test/A1.scala
  status: do_not_port
  verified_against: v2.0.0
- id: docs/src/test/A2.scala
  status: do_not_port
  verified_against: v2.0.0
- id: actor-tests/src/test/B1.scala
  status: not_built
  verified_against: v2.0.0
- id: docs/src/test/D1.scala
  status: do_not_port
  verified_against: v2.0.0
- id: stream-tests/src/test/M1.scala
  status: unported
  verified_against: null
- id: testkit/src/test/N1.scala
  status: green
  verified_against: v2.0.0
"""

TABLE = "| id | status | verified_against | 原典 v2.0.0 のパス | 過去の基準からの行数差 |\n|:---|:---|:---|:---|:---|\n"


def row(node_id, status="do_not_port", tag="v2.0.0"):
    return f"| `{node_id}` | {status} | {tag} | あり（1 行） | 変更なし |\n"


def reverification(declared="3", rows=None):
    """①-a（2 行）と ①-b（1 行）の 2 つの表。id だけの決定表とコードブロックは読ませない。"""
    rows = [
        row("docs/src/test/A1.scala"),
        row("docs/src/test/A2.scala"),
        row("actor-tests/src/test/B1.scala", status="not_built"),
    ] if rows is None else rows
    return (
        f"# docs/baseline — 判定済み {declared} ノードの v2.0.0 再照合\n\n"
        "## ①-a `docs` のノード\n\n"
        + TABLE
        + "".join(rows[:2])
        + "\n## ①-b `not_built` のノード\n\n"
        "| id | `triage.reason`（要約） | 決定の記録 |\n|:---|:---|:---|\n"
        "| `actor-tests/src/test/B1.scala` | 理由 | 記録 |\n\n"
        "### 表\n\n"
        + TABLE
        + "".join(rows[2:])
        + "\n```sh\n# この中の `| grep` は表ではない\nawk -F'\\t' '$3==\"x\"' table | grep -c id\n"
        "| これはコードブロックの中の行で、表ではない |\n```\n"
    )


def missing_unported(declared="1", rows=None):
    rows = ["| `stream-tests/src/test/M1.scala` | 原典で削除 | 根拠 | do_not_port |\n"] if rows is None else rows
    return (
        f"# docs/baseline — 原典 v2.0.0 にファイルが無い unported {declared} ノード\n\n"
        "| 項目 | 値 |\n|:---|:---|\n| 原典 | Apache Pekko **v2.0.0**（commit `9fbd228`） |\n\n"
        "## 1 ノードずつの記録\n\n"
        "| id（処理後の台帳） | 分類 | 原典 v2.0.0 に無い根拠 | 台帳 |\n|:---|:---|:---|:---|\n"
        + "".join(rows)
    )


def new_docs_tests(declared="1", rows=None):
    rows = ["| 1 | A | `docs/src/test/D1.scala` | — | 0 |\n"] if rows is None else rows
    return (
        f"# docs/baseline — 原典 v2.0.0 で新しく入った docs のテスト {declared} 件\n\n"
        "| 項目 | 値 |\n|:---|:---|\n| 原典 | Apache Pekko **v2.0.0** |\n\n"
        "## 一覧\n\n"
        "| # | 状態 | パス（v2.0.0） | 旧パス | ケース数 |\n|--:|:---|:---|:---|--:|\n"
        + "".join(rows)
    )


TSV = "id\tcategory\ntestkit/src/test/N1.scala\tno-report\n"


def build(root: Path, ledger=LEDGER, tsv=TSV, **reports) -> dict:
    """わざと壊した入力で使う、台帳・報告書 3 件・一覧を持つ作業ツリーを作る。"""
    (root / "docs" / "baseline").mkdir(parents=True, exist_ok=True)
    (root / "ledger").mkdir(parents=True, exist_ok=True)
    (root / "ledger" / "nodes.yaml").write_text(ledger, encoding="utf-8")
    contents = {
        "docs/baseline/v2.0.0-reverification.md": reports.get("reverification", reverification()),
        "docs/baseline/v2.0.0-missing-unported.md": reports.get("missing_unported", missing_unported()),
        "docs/baseline/v2.0.0-new-docs-tests.md": reports.get("new_docs_tests", new_docs_tests()),
        "docs/baseline/v2.0.0-undocumented-nodes.tsv": tsv,
    }
    for name, text in contents.items():
        (root / name).write_text(text, encoding="utf-8")
    return {
        "root": root,
        "ledger": root / "ledger" / "nodes.yaml",
        "undocumented": root / "docs" / "baseline" / "v2.0.0-undocumented-nodes.tsv",
    }


def check_exit(paths) -> int:
    """check の終了コード。CI の出力はテストの表示を混ぜないため捕まえる。"""
    with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
        return tool.cmd_check(_args(paths))


def rules(paths) -> set:
    found, _ = tool.violations(paths["root"], paths["ledger"], paths["undocumented"])
    return {violation.rule for violation in found}


def test_check_passes_on_the_fixture():
    with tempfile.TemporaryDirectory() as tmp:
        paths = build(Path(tmp))
        found, summary = tool.violations(paths["root"], paths["ledger"], paths["undocumented"])
        assert found == [], found
        assert summary["rows"] == 5, summary
        assert summary["tagged"] == 5, summary
        assert summary["undocumented"] == 1, summary
        assert check_exit(paths) == 0
        print("1. 正常な入力（報告書 3 件 / 5 行、台帳の v2.0.0 5 件、未記載 1 件）で 0")


def test_check_passes_on_this_repository():
    """いまの main の入力で 0（これが CI で回る入力）。"""
    found, summary = tool.violations(REPO, REPO / "ledger" / "nodes.yaml",
                                     REPO / "docs" / "baseline" / "v2.0.0-undocumented-nodes.tsv")
    assert found == [], found
    assert summary["reports"] == len(tool.REPORTS), summary
    assert summary["undocumented"] == summary["listed"], summary
    print(f"2. このリポジトリで 0（報告書 {summary['reports']} 件 / 表 {summary['rows']} 行、"
          f"台帳の v2.0.0 {summary['tagged']} 件、未記載 {summary['undocumented']} 件）")


def test_report_id_missing_from_the_ledger_fails():
    with tempfile.TemporaryDirectory() as tmp:
        paths = build(Path(tmp), missing_unported=missing_unported(
            declared="2",
            rows=["| `stream-tests/src/test/M1.scala` | 原典で削除 | 根拠 | do_not_port |\n",
                  "| `zzz/src/test/NotInLedger.scala` | 原典で削除 | 根拠 | do_not_port |\n"],
        ))
        assert rules(paths) == {4}, rules(paths)
        assert check_exit(paths) == 1
        print("3. 報告書の id が台帳に無ければ検査4 で落ちる")


def test_same_id_in_two_reports_fails():
    with tempfile.TemporaryDirectory() as tmp:
        paths = build(Path(tmp), reverification=reverification(
            declared="4",
            rows=[row("docs/src/test/A1.scala"),
                  row("docs/src/test/A2.scala"),
                  row("docs/src/test/D1.scala"),
                  row("actor-tests/src/test/B1.scala", status="not_built")],
        ))
        assert rules(paths) == {2}, rules(paths)
        assert check_exit(paths) == 1
        print("4. 同じ id が 2 報告書に載れば検査2 で落ちる")


def test_declared_count_mismatch_fails():
    with tempfile.TemporaryDirectory() as tmp:
        paths = build(Path(tmp), reverification=reverification(declared="4"))
        assert rules(paths) == {3}, rules(paths)
        assert check_exit(paths) == 1
        print("5. 宣言件数と行数が合わなければ検査3 で落ちる")


def test_tag_disagreeing_with_the_ledger_fails():
    with tempfile.TemporaryDirectory() as tmp:
        paths = build(Path(tmp), reverification=reverification(
            rows=[row("docs/src/test/A1.scala", tag=PAST),
                  row("docs/src/test/A2.scala"),
                  row("actor-tests/src/test/B1.scala", status="not_built")],
        ))
        assert rules(paths) == {1}, rules(paths)
        assert check_exit(paths) == 1
        print("6. 表の照合タグが台帳と違えば検査1 で落ちる")


def test_report_without_the_tag_column_checks_existence_only():
    """照合タグの列が無い報告書は、台帳が unported / null のままでよい。"""
    with tempfile.TemporaryDirectory() as tmp:
        paths = build(Path(tmp))
        rows = tool.collect_rows(paths["root"])
        assert [r.tag for r in rows["docs/baseline/v2.0.0-missing-unported.md"]] == [None]
        assert rows["docs/baseline/v2.0.0-missing-unported.md"][0].id.endswith("M1.scala")
        print("7. 照合タグの列が無い報告書は存在だけを見る")


def test_listed_extra_id_fails():
    with tempfile.TemporaryDirectory() as tmp:
        paths = build(Path(tmp), tsv=TSV + "docs/src/test/A1.scala\tno-report\n")
        assert rules(paths) == {5}, rules(paths)
        assert check_exit(paths) == 1
        print("8. 一覧に id を足すと検査5 で落ちる")


def test_listed_missing_id_fails():
    with tempfile.TemporaryDirectory() as tmp:
        paths = build(Path(tmp), tsv="id\tcategory\n")
        assert rules(paths) == {5}, rules(paths)
        assert check_exit(paths) == 1
        print("9. 一覧から id を消すと検査5 で落ちる")


def test_report_without_a_table_stops():
    with tempfile.TemporaryDirectory() as tmp:
        paths = build(Path(tmp), new_docs_tests="# docs/baseline — docs のテスト 1 件\n\n表が無い。\n")
        try:
            tool.violations(paths["root"], paths["ledger"], paths["undocumented"])
        except tool.ReportUnreadable as error:
            assert "報告書の表が無い" in str(error), error
        else:
            raise AssertionError("読めない報告書を飛ばした")
        assert check_exit(paths) == 1
        print("10. 表が無い報告書は飛ばさずに止まる")


def test_ragged_row_stops():
    with tempfile.TemporaryDirectory() as tmp:
        paths = build(Path(tmp), new_docs_tests=new_docs_tests(
            declared="2",
            rows=["| 1 | A | `docs/src/test/D1.scala` | — | 0 |\n",
                  "| 2 | A | `docs/src/test/D2.scala` |\n"],
        ))
        try:
            tool.violations(paths["root"], paths["ledger"], paths["undocumented"])
        except tool.ReportUnreadable as error:
            assert "セルが" in str(error), error
        else:
            raise AssertionError("列数が合わない行を飛ばした")
        print("11. セルの数が合わない行は飛ばさずに止まる")


def test_ambiguous_declared_count_stops():
    with tempfile.TemporaryDirectory() as tmp:
        paths = build(Path(tmp), new_docs_tests=(
            new_docs_tests() + "\n# docs/baseline — 原典 v2.0.0 で新しく入った docs のテスト 2 件\n"))
        try:
            tool.violations(paths["root"], paths["ledger"], paths["undocumented"])
        except tool.ReportUnreadable as error:
            assert "宣言件数の行が 1 つに決まらない" in str(error), error
        else:
            raise AssertionError("宣言件数が 2 つの報告書を読んだ")
        print("12. 宣言件数が 1 つに決まらない報告書は止まる")


def test_report_tables_ignore_other_tables_and_code_blocks():
    """id だけの決定表と、コードブロックの中の `|` の行は報告書の表にしない。"""
    with tempfile.TemporaryDirectory() as tmp:
        paths = build(Path(tmp))
        rows = tool.collect_rows(paths["root"])
        assert [r.id for r in rows["docs/baseline/v2.0.0-reverification.md"]] == [
            "docs/src/test/A1.scala",
            "docs/src/test/A2.scala",
            "actor-tests/src/test/B1.scala",
        ], rows
        print("13. 報告書の表でない表（決定表・コードブロック）は読まない")


def test_list_undocumented_writes_the_set_in_order():
    with tempfile.TemporaryDirectory() as tmp:
        paths = build(Path(tmp))
        seen = []

        def classify(node_id):
            seen.append(node_id)
            return "other"

        categories = tool.list_undocumented(
            paths["root"], paths["ledger"], paths["undocumented"], classify=classify,
            out=io.StringIO())
        assert categories == {"testkit/src/test/N1.scala": "other"}, categories
        assert seen == ["testkit/src/test/N1.scala"], seen
        text = paths["undocumented"].read_text(encoding="utf-8")
        assert text == "id\tcategory\ntestkit/src/test/N1.scala\tother\n", text
        assert tool.list_undocumented(
            paths["root"], paths["ledger"], paths["undocumented"], classify=classify,
            out=io.StringIO()) == categories
        assert paths["undocumented"].read_text(encoding="utf-8") == text
        print("14. list-undocumented は id の順に書き、2 回目も同じ内容（冪等）")


def test_list_undocumented_uses_the_given_categories_and_rejects_others():
    with tempfile.TemporaryDirectory() as tmp:
        paths = build(Path(tmp))
        tool.list_undocumented(paths["root"], paths["ledger"], paths["undocumented"],
                               classify=lambda node_id: "before-baseline", out=io.StringIO())
        assert "before-baseline" in paths["undocumented"].read_text(encoding="utf-8")
        try:
            tool.list_undocumented(paths["root"], paths["ledger"], paths["undocumented"],
                                   classify=lambda node_id: "bogus", out=io.StringIO())
        except tool.ListUnreadable as error:
            assert "bogus" in str(error), error
        else:
            raise AssertionError("規則外の category を書いた")
        print("15. list-undocumented は規則外の category を拒否する")


def test_the_list_must_have_the_header_and_known_categories():
    with tempfile.TemporaryDirectory() as tmp:
        paths = build(Path(tmp), tsv="category\tid\ntestkit/src/test/N1.scala\tno-report\n")
        try:
            tool.violations(paths["root"], paths["ledger"], paths["undocumented"])
        except tool.ListUnreadable as error:
            assert "見出し" in str(error), error
        else:
            raise AssertionError("見出しの違う一覧を読んだ")
        paths = build(Path(tmp), tsv="id\tcategory\ntestkit/src/test/N1.scala\tnope\n")
        try:
            tool.violations(paths["root"], paths["ledger"], paths["undocumented"])
        except tool.ListUnreadable as error:
            assert "category" in str(error), error
        else:
            raise AssertionError("規則外の category を読んだ")
        print("16. 一覧の見出しと category を検査する")


def test_the_undocumented_set_is_the_tagged_minus_the_reports():
    with tempfile.TemporaryDirectory() as tmp:
        paths = build(Path(tmp))
        ledger = tool.load_ledger(paths["ledger"])
        rows = tool.collect_rows(paths["root"])
        assert tool.undocumented_set(ledger, rows) == {"testkit/src/test/N1.scala"}
        print("17. 未記載は「台帳で v2.0.0 かつ報告書に無い id」である")


def test_the_ledger_rejects_a_duplicate_id():
    """台帳の同じ id が 2 行あると、1 報告書 1 行の対応が崩れるので止める。"""
    with tempfile.TemporaryDirectory() as tmp:
        paths = build(Path(tmp), ledger=LEDGER + "- id: docs/src/test/A1.scala\n  status: green\n")
        try:
            tool.violations(paths["root"], paths["ledger"], paths["undocumented"])
        except tool.ReportUnreadable as error:
            assert "id が重複している" in str(error), error
        else:
            raise AssertionError("台帳の id の重複を読んだ")
        print("18. 台帳の id が重複していれば止まる")


def test_an_empty_tag_cell_is_compared_with_the_ledger():
    """空欄は null として台帳と比べる（列が無い表とは区別する）。"""
    with tempfile.TemporaryDirectory() as tmp:
        paths = build(Path(tmp), reverification=reverification(
            rows=[row("docs/src/test/A1.scala"), row("docs/src/test/A2.scala"),
                  row("actor-tests/src/test/B1.scala", status="not_built", tag="")],
        ))
        assert rules(paths) == {1}, rules(paths)
        ledger = LEDGER + "- id: stream-tests/src/test/M2.scala\n  status: unported\n  verified_against: null\n"
        paths = build(Path(tmp), ledger=ledger, missing_unported=missing_unported(
            declared="0", rows=[]), reverification=reverification(
            declared="4", rows=[row("docs/src/test/A1.scala"), row("docs/src/test/A2.scala"),
                                row("actor-tests/src/test/B1.scala", status="not_built"),
                                row("stream-tests/src/test/M2.scala", status="unported", tag="")]))
        assert rules(paths) == set(), rules(paths)
        print("19. 空欄の照合タグは null として台帳と比べる")


def test_rows_without_a_leading_pipe_are_read_and_line_numbers_are_exact():
    with tempfile.TemporaryDirectory() as tmp:
        paths = build(Path(tmp), new_docs_tests=new_docs_tests(
            declared="2",
            rows=["| 1 | A | `docs/src/test/D1.scala` | — | 0 |\n",
                  "2 | A | `docs/src/test/NotInLedger.scala` | — | 0\n"],
        ))
        text = paths["undocumented"].parent / "v2.0.0-new-docs-tests.md"
        rows = tool.parse_report(tool.REPORTS[2], paths["root"])
        assert [r.id for r in rows] == ["docs/src/test/D1.scala", "docs/src/test/NotInLedger.scala"], rows
        body = text.read_text(encoding="utf-8").splitlines()
        assert body[rows[1].line - 1].startswith("2 |"), body[rows[1].line - 1]
        assert rules(paths) == {4}, rules(paths)
        print("20. 先頭の | を省いた行も読み、行番号はその行を指す")


def _args(paths):
    return _Args(paths["root"], paths["ledger"], paths["undocumented"])


class _Args:
    def __init__(self, root, ledger, undocumented):
        self.root = str(root)
        self.ledger = str(ledger)
        self.undocumented = str(undocumented)


def main() -> int:
    tests = [
        test_check_passes_on_the_fixture,
        test_check_passes_on_this_repository,
        test_report_id_missing_from_the_ledger_fails,
        test_same_id_in_two_reports_fails,
        test_declared_count_mismatch_fails,
        test_tag_disagreeing_with_the_ledger_fails,
        test_report_without_the_tag_column_checks_existence_only,
        test_listed_extra_id_fails,
        test_listed_missing_id_fails,
        test_report_without_a_table_stops,
        test_ragged_row_stops,
        test_ambiguous_declared_count_stops,
        test_report_tables_ignore_other_tables_and_code_blocks,
        test_list_undocumented_writes_the_set_in_order,
        test_list_undocumented_uses_the_given_categories_and_rejects_others,
        test_the_list_must_have_the_header_and_known_categories,
        test_the_undocumented_set_is_the_tagged_minus_the_reports,
        test_the_ledger_rejects_a_duplicate_id,
        test_an_empty_tag_cell_is_compared_with_the_ledger,
        test_rows_without_a_leading_pipe_are_read_and_line_numbers_are_exact,
    ]
    failures = 0
    for test in tests:
        try:
            test()
        except AssertionError as error:
            failures += 1
            print(f"FAIL {test.__name__}: {error}")
        except Exception:  # noqa: BLE001
            failures += 1
            print(f"FAIL {test.__name__}:")
            import traceback

            traceback.print_exc()
        else:
            print(f"ok   {test.__name__}")
    print(f"{len(tests) - failures} passed, {failures} failed")
    return 0 if failures == 0 else 1


if __name__ == "__main__":
    with contextlib.suppress(BrokenPipeError):
        raise SystemExit(main())
