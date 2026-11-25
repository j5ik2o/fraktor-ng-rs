#!/usr/bin/env python3
"""verified_against を現行タグへ揃える道具（ledger/tools/reverify_against.py）の回帰テスト。

確かめること:
- 変えるのは対象ノードの verified_against の値だけで、行数・ほかの欄・行頭の注記は変わらない
- 欄が無いノードには、ノードの末尾（`goal` の後）に足す。`retired` が `nodes` より前にある台帳でも、
  退役したノードを書き換えない
- 判定済みでない status（unported など）と、nodes に無い id は拒否し、台帳を書き換えない
- plan は現行タグで未照合の判定済みノードだけを出す
- PyYAML なし（`python3 -S`）でも apply と plan が動く（report だけが PyYAML を使う）

使い方:
    uv run --with pyyaml python3 ledger/tools/reverify_against_test.py
"""
import subprocess
import sys
import tempfile
from pathlib import Path

TOOLS = Path(__file__).resolve().parent
sys.path.insert(0, str(TOOLS))

import pekko_baseline  # noqa: E402
import reverify_against  # noqa: E402

# 過去の基準のタグは基準ファイルから取る（現役の経路に過去のタグを書かない。check-refs の規則）。
# このテストは、その値を入力の文字列として使うだけで、過去の原典を読まない。
PAST = sorted(pekko_baseline.load().historical)[0]

NODE = """- id: {id}
  name: m:{name}
  kind: test
  pekko:
    module: m
    class: {name}
    test_cases_approx: 1
  status: {status}
  triage:
    kind: {kind}
    reason: null
  needs:
    testkit: []
    premise_flags: []
  depends_on_impl:
    modules: []
    coverage: approx
  cases:
    total: null
    ported: null
    passing: null
    excluded: []
  rust:
    crate: null
    test_file: null
  goal: null{tag}
"""

RETIRED = """- id: retired/Old.scala
  name: m:Old
  kind: test
  pekko:
    module: m
    class: Old
    test_cases_approx: 1
  status: green
  triage:
    kind: pending
    reason: null
  needs:
    testkit: []
    premise_flags: []
  depends_on_impl:
    modules: []
    coverage: approx
  cases:
    total: 1
    ported: 1
    passing: 1
    excluded: []
  rust:
    crate: c
    test_file: t.rs
  goal: null
  verified_against: {past}
  retired:
    at: 2026-11-25
    reason: "現行の原典にファイルが無い"
    successor: m/New.scala
"""


def ledger(*, retired_first: bool = False, tag: str | None = None) -> str:
    """2 ノード（do_not_port と unported）の台帳。tag=False で verified_against の欄を省く。"""
    tag = PAST if tag is None else tag
    tag_line = "" if tag is False else f"\n  verified_against: {tag}"
    body = (
        NODE.format(id="m/A.scala", name="A", status="do_not_port", kind="do_not_port", tag=tag_line)
        + NODE.format(id="m/B.scala", name="B", status="unported", kind="pending", tag="")
    )
    head = "# 台帳の見出し（行頭の注記）\n\n"
    if retired_first:
        return head + "retired:\n" + retired() + "\n" + "nodes:\n" + body
    return head + "nodes:\n" + body + "\n" + "# 退役したノード\nretired:\n" + retired()


def retired() -> str:
    """退役したノード（過去の基準で照合した記録。書き換えないことを確かめる対象）。"""
    return RETIRED.format(past=PAST)


def write_ledger(root: Path, text: str) -> Path:
    path = root / "nodes.yaml"
    path.write_text(text, encoding="utf-8")
    return path


def write_ids(root: Path, ids: list[str]) -> Path:
    path = root / "ids.txt"
    path.write_text("# 対象\n" + "\n".join(ids) + "\n", encoding="utf-8")
    return path


def apply(root: Path, ids: list[str], *extra: str) -> subprocess.CompletedProcess:
    return subprocess.run(
        [sys.executable, str(TOOLS / "reverify_against.py"), "apply",
         "--ledger", str(root / "nodes.yaml"), "--tag", "v2.0.0",
         "--ids", str(write_ids(root, ids)), *extra],
        capture_output=True, text=True, check=False,
    )


def test_apply_changes_only_the_value():
    """do_not_port のノードの値だけが変わり、行数もほかの行もそのまま残る。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        before = ledger()
        path = write_ledger(root, before)
        proc = apply(root, ["m/A.scala"])
        assert proc.returncode == 0, (proc.stdout, proc.stderr)
        after = path.read_text(encoding="utf-8")
        expected = before.replace(
            f"  verified_against: {PAST}\n- id: m/B.scala",
            "  verified_against: v2.0.0\n- id: m/B.scala", 1,
        )
        assert after == expected, after
        assert after.count(f"verified_against: {PAST}") == 1, after
        print("1. do_not_port の値だけを書き換える")


def test_apply_adds_the_field_after_goal():
    """欄が無いノードには、`goal` の後（ノードの末尾）に足す。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        before = ledger(tag=False)
        path = write_ledger(root, before)
        proc = apply(root, ["m/A.scala"])
        assert proc.returncode == 0, (proc.stdout, proc.stderr)
        after = path.read_text(encoding="utf-8")
        assert after == before.replace(
            "  goal: null\n- id: m/B.scala", "  goal: null\n  verified_against: v2.0.0\n- id: m/B.scala",
        ), after
        print("2. 欄が無ければ goal の後に足す")


def test_apply_refuses_a_status_that_is_not_judged():
    """unported のノードは拒否し、台帳を 1 バイトも書き換えない。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        before = ledger()
        path = write_ledger(root, before)
        proc = apply(root, ["m/B.scala"])
        assert proc.returncode == 1, (proc.stdout, proc.stderr)
        assert "unported" in proc.stderr, proc.stderr
        assert path.read_text(encoding="utf-8") == before
        print("3. unported は拒否する")


def test_apply_refuses_an_id_that_is_not_in_nodes():
    """nodes に無い id（退役したノードの id）は拒否し、retired を書き換えない。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        before = ledger()
        path = write_ledger(root, before)
        proc = apply(root, ["retired/Old.scala"])
        assert proc.returncode == 1, (proc.stdout, proc.stderr)
        assert "nodes に無い" in proc.stderr, proc.stderr
        assert path.read_text(encoding="utf-8") == before
        print("4. retired の id は拒否する")


def test_apply_keeps_retired_first_and_comments():
    """`retired` が `nodes` より前にある台帳でも、注記と退役したノードをそのまま残す。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        before = ledger(retired_first=True)
        path = write_ledger(root, before)
        proc = apply(root, ["m/A.scala"])
        assert proc.returncode == 0, (proc.stdout, proc.stderr)
        after = path.read_text(encoding="utf-8")
        # retired の節（nodes: より前）と、ノードの見出しの注記は 1 バイトも変わらない。
        assert after[:after.index("nodes:")] == before[:before.index("nodes:")], after[:200]
        # 変わるのは対象ノードの 1 行だけで、退役したノードの verified_against は過去のタグのまま。
        assert after == before.replace(
            f"  verified_against: {PAST}\n- id: m/B.scala", "  verified_against: v2.0.0\n- id: m/B.scala",
        ), after
        assert f"verified_against: {PAST}" in after, after
        print("5. retired が先頭でも注記と退役を残す")


def test_apply_is_idempotent_and_reports_no_change():
    """既に現行タグのノードは書き換えず、変更 0 件として成功する。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        before = ledger()
        path = write_ledger(root, before)
        assert apply(root, ["m/A.scala"]).returncode == 0
        once = path.read_text(encoding="utf-8")
        proc = apply(root, ["m/A.scala"])
        assert proc.returncode == 0, (proc.stdout, proc.stderr)
        assert "変更した: 0 件" in proc.stdout, proc.stdout
        assert path.read_text(encoding="utf-8") == once
        print("6. 再実行しても変わらない")


def test_apply_refuses_duplicate_ids_and_writes_nothing():
    """--ids の重複は拒否する（同じノードを 2 回書かない）。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        before = ledger()
        path = write_ledger(root, before)
        proc = apply(root, ["m/A.scala", "m/A.scala"])
        assert proc.returncode == 1, (proc.stdout, proc.stderr)
        assert "重複" in proc.stderr, proc.stderr
        assert path.read_text(encoding="utf-8") == before
        print("7. --ids の重複は拒否する")


def test_dry_run_writes_nothing():
    """--dry-run は変更件数だけを出し、台帳を書き換えない。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        before = ledger()
        path = write_ledger(root, before)
        proc = apply(root, ["m/A.scala"], "--dry-run")
        assert proc.returncode == 0, (proc.stdout, proc.stderr)
        assert "変更する: 1 件" in proc.stdout, proc.stdout
        assert path.read_text(encoding="utf-8") == before
        print("8. --dry-run は書かない")


def test_plan_lists_judged_nodes_with_another_tag():
    """plan は判定済みで現行タグと違うノードだけを、render せずに id で出す。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        write_ledger(root, ledger(retired_first=True))
        proc = subprocess.run(
            [sys.executable, str(TOOLS / "reverify_against.py"), "plan",
             "--ledger", str(root / "nodes.yaml"), "--tag", "v2.0.0"],
            capture_output=True, text=True, check=False,
        )
        assert proc.returncode == 0, (proc.stdout, proc.stderr)
        assert proc.stdout.splitlines() == ["m/A.scala"], proc.stdout
        print("9. plan は判定済みの未照合だけを出す")


def test_report_table_marks_missing_original():
    """report は原典の有無と行数差を表にし、原典にファイルが無い件数を stderr に出す。"""
    import yaml  # noqa: F401  (report は PyYAML を使う)

    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        write_ledger(root, ledger())
        pekko = root / "pekko"
        (pekko / "m").mkdir(parents=True)
        (pekko / "m" / "A.scala").write_text("object A\n", encoding="utf-8")
        proc = subprocess.run(
            [sys.executable, str(TOOLS / "reverify_against.py"), "report",
             "--ledger", str(root / "nodes.yaml"), "--pekko", str(pekko),
             "--tag", "v2.0.0", "--historical", PAST],
            capture_output=True, text=True, check=False,
        )
        assert proc.returncode == 0, (proc.stdout, proc.stderr)
        assert "| `m/A.scala` | do_not_port | あり（1 行） |" in proc.stdout, proc.stdout
        assert "| `m/B.scala` | unported |" not in proc.stdout, proc.stdout
        print("10. report は原典の有無と行数差を出す")
        (pekko / "m" / "A.scala").unlink()
        proc = subprocess.run(
            [sys.executable, str(TOOLS / "reverify_against.py"), "report",
             "--ledger", str(root / "nodes.yaml"), "--pekko", str(pekko),
             "--tag", "v2.0.0", "--historical", PAST],
            capture_output=True, text=True, check=False,
        )
        assert proc.returncode == 0, (proc.stdout, proc.stderr)
        assert "| `m/A.scala` | do_not_port | なし |" in proc.stdout, proc.stdout
        assert "原典にファイルが無い 1 件" in proc.stderr, proc.stderr
        print("10b. report は原典に無いノードを数える")


def test_apply_and_plan_work_without_pyyaml():
    """PyYAML を解決できない起動（python3 -S）でも、apply と plan が動く。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        path = write_ledger(root, ledger())
        write_ids(root, ["m/A.scala"])
        code = (
            "import sys; sys.path.insert(0, %r);\n"
            "import reverify_against;\n"
            "assert 'yaml' not in sys.modules, 'yaml imported eagerly';\n"
            "sys.exit(reverify_against.main())\n" % str(TOOLS)
        )
        proc = subprocess.run(
            [sys.executable, "-S", "-c", code, "apply",
             "--ledger", str(path), "--tag", "v2.0.0", "--ids", str(root / "ids.txt")],
            capture_output=True, text=True, check=False,
        )
        assert proc.returncode == 0, (proc.stdout, proc.stderr)
        assert "変更した: 1 件" in proc.stdout, proc.stdout
        assert "verified_against: v2.0.0" in path.read_text(encoding="utf-8")
        print("11. PyYAML なしでも apply が動く")


def main() -> int:
    tests = [
        test_apply_changes_only_the_value,
        test_apply_adds_the_field_after_goal,
        test_apply_refuses_a_status_that_is_not_judged,
        test_apply_refuses_an_id_that_is_not_in_nodes,
        test_apply_keeps_retired_first_and_comments,
        test_apply_is_idempotent_and_reports_no_change,
        test_apply_refuses_duplicate_ids_and_writes_nothing,
        test_dry_run_writes_nothing,
        test_plan_lists_judged_nodes_with_another_tag,
        test_report_table_marks_missing_original,
        test_apply_and_plan_work_without_pyyaml,
    ]
    assert reverify_against.FIELD == "verified_against"
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
    raise SystemExit(main())
