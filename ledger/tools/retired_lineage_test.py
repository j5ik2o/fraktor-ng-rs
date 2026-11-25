#!/usr/bin/env python3
"""退役したノード（台帳の最上位 `retired`）を、台帳を扱うほかの道具が正しく扱うかの回帰テスト。

- `convert_baseline.py --existing`：再生成で `retired` を落とさず、退役した id を現役のノードに戻さない
- `extract_needs.py`：退役したノードの needs を書き換えようとせず、記録をそのまま残す
- `pekko_baseline.py check-ledger`：承継の記録の不正を、CI の通常の入口で 0 以外にする

使い方:
    uv run --with pyyaml python3 ledger/tools/retired_lineage_test.py
"""
import subprocess
import sys
import tempfile
import traceback
from pathlib import Path

import yaml

TOOLS = Path(__file__).resolve().parent
REPO = TOOLS.parents[1]

NODE = """- id: {id}
  name: m:{name}
  kind: test
  pekko:
    module: m
    class: {name}
    test_cases_approx: 1
  status: {status}
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
    total: {total}
    ported: {ported}
    passing: {ported}
    excluded: []
{carried}  rust:
    crate: null
    test_file: null
  goal: {goal}
{tag}"""

CARRIED = """    carried:
    - name: case
      from: m/Old.scala
      source: m/New.scala L1-2
      rust_test: t.rs
      verified_against: v2.0.0
      evidence: 1 passed
"""


def ledger(*, carried_twice: bool = False) -> str:
    nodes = [
        NODE.format(id="m/New.scala", name="New", status="unported", total=2, ported=1, carried=CARRIED,
                    goal="null", tag=""),
        NODE.format(id="m/Other.scala", name="Other", status="unported", total=3,
                    ported=1 if carried_twice else "null", carried=CARRIED if carried_twice else "",
                    goal="null", tag=""),
    ]
    retired = NODE.format(id="m/Old.scala", name="Old", status="green", total=1, ported=1, carried="",
                          goal="G0old", tag="  verified_against: v2.0.0\n")
    retired += "  retired:\n    at: 2026-11-25\n    reason: moved\n    successor: m/New.scala\n"
    return "nodes:\n" + "".join(nodes) + "retired:\n" + retired


def test_convert_baseline_keeps_retired_and_does_not_revive_the_retired_id() -> None:
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        (root / "nodes.yaml").write_text(ledger(), encoding="utf-8")
        baseline = {"modules": [{"directory": "m", "test_classes": [
            {"name": n, "file": f"m/{n}.scala", "test_cases_approx": 1} for n in ("New", "Other", "Old")]}]}
        (root / "baseline.yaml").write_text(yaml.safe_dump(baseline), encoding="utf-8")
        result = subprocess.run(
            [sys.executable, str(TOOLS / "convert_baseline.py"), "--baseline", str(root / "baseline.yaml"),
             "--existing", str(root / "nodes.yaml")], capture_output=True, text=True)
        assert result.returncode == 0, result.stderr
        out = yaml.safe_load(result.stdout)
        assert [n["id"] for n in out["nodes"]] == ["m/New.scala", "m/Other.scala"], out["nodes"]
        assert [r["id"] for r in out["retired"]] == ["m/Old.scala"], out.get("retired")
        assert out["retired"][0]["retired"]["successor"] == "m/New.scala"
        assert out["nodes"][0]["cases"]["carried"][0]["from"] == "m/Old.scala"
    print("1. convert_baseline keeps retired and does not revive the retired id")


def test_extract_needs_leaves_retired_nodes_untouched() -> None:
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        text = ledger()
        (root / "nodes.yaml").write_text(text, encoding="utf-8")
        (root / "capabilities.yaml").write_text("capabilities: []\npremise_flags: []\n", encoding="utf-8")
        for name in ("New", "Other"):
            path = root / "pekko" / "m" / f"{name}.scala"
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("class X\n", encoding="utf-8")
        result = subprocess.run(
            [sys.executable, str(TOOLS / "extract_needs.py"), "--ledger", str(root / "nodes.yaml"),
             "--capabilities", str(root / "capabilities.yaml"), "--pekko", str(root / "pekko")],
            capture_output=True, text=True)
        assert result.returncode == 0, result.stderr
        after = (root / "nodes.yaml").read_text(encoding="utf-8")
        assert after[after.index("retired:\n"):] == text[text.index("retired:\n"):]
    print("2. extract_needs leaves retired nodes untouched")


def test_check_ledger_fails_on_a_case_carried_twice() -> None:
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        for twice, expected in ((False, 0), (True, 1)):
            (root / "nodes.yaml").write_text(ledger(carried_twice=twice), encoding="utf-8")
            result = subprocess.run(
                [sys.executable, str(TOOLS / "pekko_baseline.py"), "check-ledger", "--ledger", str(root / "nodes.yaml")],
                capture_output=True, text=True, cwd=REPO)
            assert result.returncode == expected, (twice, result.stdout, result.stderr)
        assert "m/Other.scala" in result.stdout + result.stderr
    print("3. check-ledger fails when a case is carried to a node that is not the successor")


PROVIDER_LEDGER = """nodes:
- id: m/P.scala
  kind: test
  pekko:
    module: m
  status: green
  needs:
    testkit: []
    premise_flags: []
  cases:
    total: 1
    ported: 1
    passing: 1
    excluded: []
    carried:
    - name: case
      from: m/Old.scala
      source: m/P.scala L1
      rust_test: t.rs
      verified_against: v2.0.0
      evidence: 1 passed
  goal: G0p
- id: m/Q.scala
  kind: test
  pekko:
    module: m
  status: unported
  needs:
    testkit: ['cap']
    premise_flags: []
  goal: G0q
retired:
- id: m/Old.scala
  kind: test
  pekko:
    module: m
  cases:
    ported: 1
  retired:
    at: 2026-11-25
    reason: {reason}
    successor: m/P.scala
"""


def test_check_needs_does_not_count_a_goal_with_a_lineage_problem_as_a_provider() -> None:
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        (root / "capabilities.yaml").write_text("capabilities:\n- id: cap\n  provided_by: G0p\n", encoding="utf-8")
        for reason, expected in (("moved", 0), ("''", 1)):
            (root / "nodes.yaml").write_text(PROVIDER_LEDGER.replace("{reason}", reason), encoding="utf-8")
            result = subprocess.run(
                [sys.executable, str(TOOLS / "check_needs.py"), "--ledger", str(root / "nodes.yaml"),
                 "--capabilities", str(root / "capabilities.yaml"), "--goal", "G0q"],
                capture_output=True, text=True)
            assert result.returncode == expected, (reason, result.stdout, result.stderr)
    print("4. check_needs does not count a goal with a lineage problem as a provider")


PLAIN_PROVIDER = """nodes:
- id: m/P.scala
  kind: test
  pekko:
    module: m
  status: green
  needs:
    testkit: []
    premise_flags: []
  goal: G0p
- id: m/Q.scala
  kind: test
  pekko:
    module: m
  status: unported
  needs:
    testkit: ['cap']
    premise_flags: []
  goal: G0q
retired: {retired}
"""


def test_check_needs_does_not_count_any_provider_when_retired_is_not_a_list() -> None:
    """retired が配列でなければ rollup はどのゴールも完了にしないので、check_needs も供給元に数えない。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        (root / "capabilities.yaml").write_text("capabilities:\n- id: cap\n  provided_by: G0p\n", encoding="utf-8")
        for retired, expected in (("[]", 0), ("null", 0), ("1", 1), ("{}", 1)):
            (root / "nodes.yaml").write_text(PLAIN_PROVIDER.replace("{retired}", retired), encoding="utf-8")
            result = subprocess.run(
                [sys.executable, str(TOOLS / "check_needs.py"), "--ledger", str(root / "nodes.yaml"),
                 "--capabilities", str(root / "capabilities.yaml"), "--goal", "G0q"],
                capture_output=True, text=True)
            assert result.returncode == expected, (retired, result.stdout, result.stderr)
    print("5. check_needs does not count a provider when retired is not a list")


def test_check_ledger_diagnoses_an_unquoted_invalid_retired_date_without_crashing() -> None:
    """CI の check-ledger も台帳専用のローダで読み、引用なしの 2026-02-30 を例外ではなく診断にする。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        text = ledger().replace("    at: 2026-11-25\n", "    at: 2026-02-30\n")
        (root / "nodes.yaml").write_text(text, encoding="utf-8")
        result = subprocess.run(
            [sys.executable, str(TOOLS / "pekko_baseline.py"), "check-ledger", "--ledger", str(root / "nodes.yaml")],
            capture_output=True, text=True, cwd=REPO)
        assert result.returncode == 1, (result.stdout, result.stderr)
        assert "Traceback" not in result.stderr, result.stderr
        assert "m/Old.scala" in result.stdout + result.stderr and "暦日" in result.stdout + result.stderr
    print("6. check-ledger diagnoses an unquoted invalid retired date without crashing")


APPROVED_CASE = ("    excluded:\n    - name: case\n      reason: r\n"
                 "      approved_by: someone\n      approved_at: 2026-01-01\n")


def run(*args: str) -> int:
    return subprocess.run([sys.executable, *args], capture_output=True, text=True, cwd=REPO).returncode


def test_a_partly_ported_successor_cannot_exclude_its_carried_case() -> None:
    """部分移植（unported）の後継でも、引き継いだケースを承認済みの除外に入れると拒否する。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        text = ledger().replace("    excluded: []\n    carried:", APPROVED_CASE + "    carried:", 1)
        assert APPROVED_CASE in text
        (root / "nodes.yaml").write_text(text, encoding="utf-8")
        assert run(str(TOOLS / "pekko_baseline.py"), "check-ledger", "--ledger", str(root / "nodes.yaml")) == 1
        assert run(str(TOOLS / "rollup.py"), "--ledger", str(root / "nodes.yaml")) == 1
    print("7. a partly ported successor cannot exclude its carried case")


def test_a_successor_with_carried_cases_must_be_unported_red_or_counted_green() -> None:
    """引き継ぎを持つノードを do_not_port などにすると、全体・ゴール・CI・能力の供給のどれでも完了にしない。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        text = ledger().replace(
            "- id: m/New.scala\n  name: m:New\n  kind: test\n  pekko:\n    module: m\n    class: New\n"
            "    test_cases_approx: 1\n  status: unported\n",
            "- id: m/New.scala\n  name: m:New\n  kind: test\n  pekko:\n    module: m\n    class: New\n"
            "    test_cases_approx: 1\n  status: do_not_port\n", 1)
        text = text.replace("  goal: null\n  rust:", "  goal: null\n  rust:")
        text = text.replace("    test_file: null\n  goal: null\n- id: m/Other", "    test_file: null\n  goal: G0n\n"
                            "  verified_against: v2.0.0\n- id: m/Other", 1)
        assert "status: do_not_port" in text and "goal: G0n" in text
        text = text.replace("retired:\n", "- id: m/Q.scala\n  kind: test\n  pekko:\n    module: m\n  status: unported\n"
                            "  needs:\n    testkit: ['cap']\n    premise_flags: []\n  goal: G0q\nretired:\n", 1)
        (root / "nodes.yaml").write_text(text, encoding="utf-8")
        (root / "capabilities.yaml").write_text("capabilities:\n- id: cap\n  provided_by: G0n\n", encoding="utf-8")
        ledger_path = str(root / "nodes.yaml")
        assert run(str(TOOLS / "pekko_baseline.py"), "check-ledger", "--ledger", ledger_path) == 1
        assert run(str(TOOLS / "rollup.py"), "--ledger", ledger_path) == 1
        assert run(str(TOOLS / "rollup.py"), "--ledger", ledger_path, "--goal", "G0n") == 1
        assert run(str(TOOLS / "check_needs.py"), "--ledger", ledger_path, "--capabilities",
                   str(root / "capabilities.yaml"), "--goal", "G0q") == 1
    print("8. a do_not_port successor with carried cases is not complete anywhere")


def main() -> int:
    tests = [
        test_convert_baseline_keeps_retired_and_does_not_revive_the_retired_id,
        test_extract_needs_leaves_retired_nodes_untouched,
        test_check_ledger_fails_on_a_case_carried_twice,
        test_check_needs_does_not_count_a_goal_with_a_lineage_problem_as_a_provider,
        test_check_needs_does_not_count_any_provider_when_retired_is_not_a_list,
        test_check_ledger_diagnoses_an_unquoted_invalid_retired_date_without_crashing,
        test_a_partly_ported_successor_cannot_exclude_its_carried_case,
        test_a_successor_with_carried_cases_must_be_unported_red_or_counted_green,
    ]
    failed = 0
    for test in tests:
        try:
            test()
        except Exception:  # noqa: BLE001 — 全テストを走らせて失敗を数える
            failed += 1
            print(f"FAIL {test.__name__}")
            traceback.print_exc()
    print(f"{len(tests) - failed} passed, {failed} failed")
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
