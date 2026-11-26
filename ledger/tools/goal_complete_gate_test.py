#!/usr/bin/env python3
"""goal_complete_gate.py（親ワークフローの完了関門）の判定を、一時ディレクトリで確かめる。

使い方:
    uv run --with pyyaml python3 ledger/tools/goal_complete_gate_test.py

関門は TAKT の system step から呼ばれ、TAKT_PREVIOUS_RESPONSE_FILE の3階層上にある
run の context ディレクトリの task/order.md からゴール文書をたどり、その1行目の ID で
rollup.py --goal を走らせる。ここでは takt も cargo も node も起動せず、配置と台帳を
一時ディレクトリに直接作って関門を subprocess で走らせる。

確かめること:
  1. 未移植ノードが残る          exit_code 非0、未完了 id が output に出る
  2. 全ノード green              exit_code 0
  3. green と do_not_port の混在 exit_code 0
  4. 属するノードが無い（goal 不一致） non 0
  5. order.md にゴール文書パスが 0 件  非0
  6. order.md にゴール文書パスが複数件 非0
  7. ゴール文書が存在しない            非0
  8. ゴール文書1行目から ID が取れない 非0
  9. TAKT_PREVIOUS_RESPONSE_FILE 未設定 非0（JSON は常に出る）
 10. TAKT_PREVIOUS_RESPONSE_FILE が指すファイルが無い 非0
 11. 2階層上（system-commands）では order.md が見つからない（3階層固定を観測）
 12. 要件10: 表 id と台帳 goal が不一致  非0
 13. 要件10: 表に id 見出し列が無い      非0
 14. 要件10: 2列表（g0a 形式）と5列表（g0b〜g0f 形式）の両方で一致 0
 15. 実物の docs/goals/*.md 全6本で、1行目から ID が取れ、表 id 集合が現行台帳の
     所属と一致する（文字列をテストへ埋め込まない）。終了値の期待は台帳のノード
     状態から求める（全ノードが rollup.is_done を満たすときだけ 0）
 16. 失敗時も終了コードは 0 で、JSON に exit_code 非0 を載せる（takt の output: json 契約）
 17. `-t` 直渡し（order.md 無し）でも meta.json の task から完了判定できる
 18. order.md も meta.json も無ければ判定不能として失敗し COMPLETE にしない
 19. 台帳 YAML が不正でも終了コード 0 で JSON を出し exit_code 非0 と原因を返す
 20. `## 対象ノード` 節の後に別の表があっても、節外の表を id と解釈しない
 21. `## 対象ノード` 節内で対象ノード表の後に別の表が続いても、最初の表だけを読む
 28. green と記録の揃った not_built（作らない区分）の混在 exit_code 0
 29. not_built でも、ケースの記録（承認・代替例）が欠けていれば完了にしない
 30. not_built の承認記録が不正（実在しない暦日、時刻つき、文字列でない・空白だけの承認者）なら完了にしない
 31. not_built のケース名が重複していれば、件数が合っていても完了にしない
 32. not_built のケース名が空白だけなら完了にしない
 33. TAKT の triage が処理済みとする status が rollup.DONE_STATUSES と一致し、記録不足の
     needs_human は対象ゴールのノードに限る
 34. not_built の passing が整数の 0 でない（1、null、false、欠落）なら完了にしない
 35. cases.not_built が配列でなくても集計は止まらず、そのノードの記録不足として出る。
     ほかのゴールのその誤記で、対象ゴールの完了判定は止まらない
 36. ほかのゴールの not_built の記録不足は、対象ゴールの未完了一覧に入らない。対象ゴール
     自身の記録不足は、未完了一覧に not_built と不足の内容つきで出る
 37. not_built の triage や cases がマッピングでなくても止まらない。対象ゴールなら未完了、
     ほかのゴールは判定を続ける
 38. check_needs も、ほかのゴールの壊れた not_built の記録で止まらない
 39. 全件を作らないノードに承認済みの除外があれば未完了で、除外の有効件数にも数えない
 40. not_built の total が正の整数でない（true）、前後に空白のあるケース名で重複を隠すなら完了にしない
 41. green の混在ノード（移植 + 承認済みの作らない区分、件数が一致）なら完了にする
 42. green の混在ノードでも、作らない要素に承認記録が無ければ完了にしない
 43. green の混在ノードで件数が合わなければ（total ≠ passing + 除外 + 作らない）完了にしない
"""
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

GATE = Path(__file__).resolve().parent / "goal_complete_gate.py"
ROLLUP = Path(__file__).resolve().parent / "rollup.py"
APPROVALS = Path(__file__).resolve().parent / "excluded_approvals.py"
TRIAGE = Path(__file__).resolve().parents[2] / ".takt" / "facets" / "instructions" / "fraktor-triage.md"
PORT_NODE = Path(__file__).resolve().parents[2] / ".takt" / "workflows" / "fraktor-port-node.yaml"
REPO = Path(__file__).resolve().parents[2]
GOALS = REPO / "docs" / "goals"
REAL_LEDGER = REPO / "ledger" / "nodes.yaml"

SLUG = "20260101-000000-test-run"
UUID = "11111111-2222-3333-4444-555555555555"


def write(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")


def copy_rollup(root: Path) -> None:
    """rollup.py と、それが読む excluded_approvals.py を一時ディレクトリへ写す。"""
    write(root / "ledger" / "tools" / "rollup.py", ROLLUP.read_text(encoding="utf-8"))
    write(root / "ledger" / "tools" / "excluded_approvals.py", APPROVALS.read_text(encoding="utf-8"))
    write(root / "ledger" / "tools" / "lineage.py", (ROLLUP.parent / "lineage.py").read_text(encoding="utf-8"))
    # 引き継ぎのタグの検査に、基準ファイルとその読み手を使う。
    write(root / "ledger" / "tools" / "pekko_baseline.py", (ROLLUP.parent / "pekko_baseline.py").read_text(encoding="utf-8"))
    baseline = ROLLUP.parents[2] / "tools" / "pekko-source" / "pekko-baseline.env"
    write(root / "tools" / "pekko-source" / "pekko-baseline.env", baseline.read_text(encoding="utf-8"))


def node(node_id: str, status: str, goal: str | None) -> str:
    """台帳の1ノード分の YAML。rollup.py が読む key だけを持たせる。"""
    goal_line = "null" if goal is None else goal
    return (
        f"- id: {node_id}\n"
        f"  kind: test\n"
        f"  pekko:\n"
        f"    module: stub\n"
        f"  status: {status}\n"
        f"  goal: {goal_line}\n"
    )


def ledger_yaml(nodes: list[tuple[str, str, str | None]]) -> str:
    return "nodes:\n" + "".join(node(*n) for n in nodes)


def goal_doc(goal_id: str, ids: list[str], columns: str = "five") -> str:
    """ゴール文書の雛形。先頭行で ID、対象ノード表で id を並べる。"""
    if columns == "two":
        header = "| id | ケース数（近似） |\n|---|---|\n"
        rows = "".join(f"| `{i}` | 1 |\n" for i in ids)
    elif columns == "five":
        header = "| 処理順 | id | kind | ケース数（近似） | needs.testkit |\n|---|---|---|---|---|\n"
        rows = "".join(f"| 1 | `{i}` | test | 1 | none |\n" for i in ids)
    else:
        raise ValueError(columns)
    return (
        f"# ゴール {goal_id}：テスト用のゴール\n\n"
        f"本文。\n\n"
        f"## 対象ノード（{len(ids)}件）\n\n"
        f"{header}{rows}\n"
    )


def make_run(root: Path, order_text: str, ledger: str,
             goal_docs: dict[str, str] | None = None) -> Path:
    """run の context と応答ファイルの配置を固定する。

    <root>/.takt/runs/<slug>/context/system-commands/<uuid>/response.txt
    <root>/.takt/runs/<slug>/context/task/order.md
    <root>/ledger/nodes.yaml
    <root>/ledger/tools/rollup.py
    """
    run = root / ".takt" / "runs" / SLUG
    response = run / "context" / "system-commands" / UUID / "response.txt"
    write(response, "dummy\n")
    write(run / "context" / "task" / "order.md", order_text)
    write(root / "ledger" / "nodes.yaml", ledger)
    copy_rollup(root)
    for name, text in (goal_docs or {}).items():
        write(root / "docs" / "goals" / name, text)
    return response


def make_run_without_order(root: Path, task_text: str, ledger: str,
                           goal_docs: dict[str, str] | None = None) -> Path:
    """`-t` 直渡しの発行を模す。order.md を作らず、run 直下に meta.json だけ置く。

    <root>/.takt/runs/<slug>/context/system-commands/<uuid>/response.txt
    <root>/.takt/runs/<slug>/meta.json （task に発行時タスク文字列）
    <root>/ledger/nodes.yaml
    <root>/ledger/tools/rollup.py
    """
    run = root / ".takt" / "runs" / SLUG
    response = run / "context" / "system-commands" / UUID / "response.txt"
    write(response, "dummy\n")
    write(run / "meta.json", json.dumps({"task": task_text}, ensure_ascii=False))
    write(root / "ledger" / "nodes.yaml", ledger)
    copy_rollup(root)
    for name, text in (goal_docs or {}).items():
        write(root / "docs" / "goals" / name, text)
    return response



def run_gate(root: Path, response: Path | None, set_env: bool = True) -> dict:
    env = {k: v for k, v in os.environ.items() if k != "TAKT_PREVIOUS_RESPONSE_FILE"}
    if set_env and response is not None:
        env["TAKT_PREVIOUS_RESPONSE_FILE"] = str(response)
    proc = subprocess.run(
        [sys.executable, str(GATE)],
        cwd=str(root), env=env, capture_output=True, text=True, check=False,
    )
    assert proc.returncode == 0, (
        f"gate must always exit 0 for takt output: json; got {proc.returncode}\n"
        f"stdout={proc.stdout}\nstderr={proc.stderr}"
    )
    return json.loads(proc.stdout)


def order_with(paths: list[str]) -> str:
    body = "\n".join(f"参照: {p}" for p in paths)
    return f"# タスク\n\nこのゴールを進める。\n\n{body}\n"


def one_goal_doc(goal_id: str, ids: list[str], columns: str = "five") -> dict[str, str]:
    return {f"{goal_id.lower()}-stub.md": goal_doc(goal_id, ids, columns)}


def test_pending_node_is_not_complete():
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        ids = ["a/A.scala", "a/B.scala"]
        ledger = ledger_yaml([(ids[0], "green", "G0z"), (ids[1], "unported", "G0z")])
        response = make_run(root, order_with(["docs/goals/g0z-stub.md"]), ledger,
                            one_goal_doc("G0z", ids))
        result = run_gate(root, response)
        assert result["exit_code"] != 0, result
        assert "a/B.scala" in result["output"], result
        print("1. pending node: exit_code =", result["exit_code"])


def test_all_green_is_complete():
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        ids = ["a/A.scala", "a/B.scala"]
        ledger = ledger_yaml([(ids[0], "green", "G0z"), (ids[1], "green", "G0z")])
        response = make_run(root, order_with(["docs/goals/g0z-stub.md"]), ledger,
                            one_goal_doc("G0z", ids))
        result = run_gate(root, response)
        assert result["exit_code"] == 0, result
        print("2. all green: exit_code =", result["exit_code"])


def test_green_and_do_not_port_is_complete():
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        ids = ["a/A.scala", "a/B.scala"]
        ledger = ledger_yaml([(ids[0], "green", "G0z"), (ids[1], "do_not_port", "G0z")])
        response = make_run(root, order_with(["docs/goals/g0z-stub.md"]), ledger,
                            one_goal_doc("G0z", ids))
        result = run_gate(root, response)
        assert result["exit_code"] == 0, result
        print("3. green + do_not_port: exit_code =", result["exit_code"])


NOT_BUILT_CASE = (
    "    total: 1\n"
    "    ported: 0\n"
    "    passing: 0\n"
    "    excluded: []\n"
    "    not_built:\n"
    "    - name: someCase\n"
    "      reason: the original checks a Java helper\n"
    "      alternative: a Rust match\n"
)


def not_built_node(node_id: str, goal: str, approval: str, cases: str = NOT_BUILT_CASE) -> str:
    """作らない区分のノード。approval はケースの要素に足す承認の行。"""
    return (
        f"- id: {node_id}\n"
        f"  kind: test\n"
        f"  pekko:\n"
        f"    module: stub\n"
        f"  status: not_built\n"
        f"  triage:\n"
        f"    kind: not_built\n"
        f"    reason: served by the language\n"
        f"  cases:\n"
        f"{cases}{approval}"
        f"  goal: {goal}\n"
    )


APPROVAL = "      approved_by: someone\n      approved_at: 2026-01-01\n"


def test_green_and_not_built_is_complete():
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        ids = ["a/A.java", "a/B.java"]
        ledger = ledger_yaml([(ids[0], "green", "G0w")]) + not_built_node(ids[1], "G0w", APPROVAL)
        response = make_run(root, order_with(["docs/goals/g0w-stub.md"]), ledger,
                            one_goal_doc("G0w", ids))
        result = run_gate(root, response)
        assert result["exit_code"] == 0, result
        print("28. green + not_built: exit_code =", result["exit_code"])


MIXED_GREEN_CASES = (
    "    total: 3\n"
    "    ported: 2\n"
    "    passing: 2\n"
    "    excluded: []\n"
    "    not_built:\n"
    "    - name: someCase\n"
    "      reason: the original checks a JVM-only helper\n"
    "      alternative: a compile_fail doctest\n"
)


def mixed_green_node(node_id: str, goal: str, approval: str, cases: str = MIXED_GREEN_CASES) -> str:
    """混在ノード。移植 2 件と作らない 1 件を持つ green。approval は要素へ足す承認の行。"""
    return (
        f"- id: {node_id}\n"
        f"  kind: test\n"
        f"  pekko:\n"
        f"    module: stub\n"
        f"  status: green\n"
        f"  cases:\n"
        f"{cases}{approval}"
        f"  goal: {goal}\n"
    )


def test_mixed_green_with_approved_not_built_is_complete():
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        ids = ["a/A.java"]
        ledger = "nodes:\n" + mixed_green_node(ids[0], "G0x", APPROVAL)
        response = make_run(root, order_with(["docs/goals/g0x-stub.md"]), ledger,
                            one_goal_doc("G0x", ids))
        result = run_gate(root, response)
        assert result["exit_code"] == 0, result
        print("41. mixed green + approved not_built: exit_code =", result["exit_code"])


def test_mixed_green_with_unapproved_not_built_is_not_complete():
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        ids = ["a/A.java"]
        ledger = "nodes:\n" + mixed_green_node(ids[0], "G0y", "")
        response = make_run(root, order_with(["docs/goals/g0y-stub.md"]), ledger,
                            one_goal_doc("G0y", ids))
        result = run_gate(root, response)
        assert result["exit_code"] != 0, result
        assert "a/A.java" in result.get("output", ""), result
        print("42. mixed green + unapproved not_built: exit_code =", result["exit_code"])


def test_mixed_green_with_count_mismatch_is_not_complete():
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        ids = ["a/A.java"]
        cases = MIXED_GREEN_CASES.replace("    total: 3\n", "    total: 4\n")
        ledger = "nodes:\n" + mixed_green_node(ids[0], "G0z", APPROVAL, cases)
        response = make_run(root, order_with(["docs/goals/g0z-stub.md"]), ledger,
                            one_goal_doc("G0z", ids))
        result = run_gate(root, response)
        assert result["exit_code"] != 0, result
        print("43. mixed green + count mismatch: exit_code =", result["exit_code"])


def test_mixed_green_with_unapproved_excluded_is_not_complete():
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        ids = ["a/A.java"]
        cases = (
            "    total: 4\n"
            "    ported: 2\n"
            "    passing: 2\n"
            "    excluded:\n"
            "    - name: excludedCase\n"
            "      reason: the original checks a JVM-only helper\n"
            "    not_built:\n"
            "    - name: someCase\n"
            "      reason: the original checks a JVM-only helper\n"
            "      alternative: a compile_fail doctest\n"
            "      approved_by: someone\n"
            "      approved_at: 2026-01-01\n"
        )
        ledger = "nodes:\n" + mixed_green_node(ids[0], "G0u", "", cases)
        response = make_run(root, order_with(["docs/goals/g0u-stub.md"]), ledger,
                            one_goal_doc("G0u", ids))
        result = run_gate(root, response)
        assert result["exit_code"] != 0, result
        print("44. mixed green + unapproved excluded: exit_code =", result["exit_code"])


def test_mixed_green_with_cross_list_duplicate_is_not_complete():
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        ids = ["a/A.java"]
        cases = (
            "    total: 4\n"
            "    ported: 2\n"
            "    passing: 2\n"
            "    excluded:\n"
            "    - name: sharedCase\n"
            "      reason: the original checks a JVM-only helper\n"
            "      approved_by: someone\n"
            "      approved_at: 2026-01-01\n"
            "    not_built:\n"
            "    - name: sharedCase\n"
            "      reason: the original checks a JVM-only helper\n"
            "      alternative: a compile_fail doctest\n"
            "      approved_by: someone\n"
            "      approved_at: 2026-01-01\n"
        )
        ledger = "nodes:\n" + mixed_green_node(ids[0], "G0t", "", cases)
        response = make_run(root, order_with(["docs/goals/g0t-stub.md"]), ledger,
                            one_goal_doc("G0t", ids))
        result = run_gate(root, response)
        assert result["exit_code"] != 0, result
        print("45. mixed green + cross-list duplicate: exit_code =", result["exit_code"])


def test_mixed_green_with_ported_above_passing_is_not_complete():
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        ids = ["a/A.java"]
        cases = MIXED_GREEN_CASES.replace("    ported: 2\n", "    ported: 3\n")
        ledger = "nodes:\n" + mixed_green_node(ids[0], "G0s", APPROVAL, cases)
        response = make_run(root, order_with(["docs/goals/g0s-stub.md"]), ledger,
                            one_goal_doc("G0s", ids))
        result = run_gate(root, response)
        assert result["exit_code"] != 0, result
        print("46. mixed green + ported above passing: exit_code =", result["exit_code"])


def test_mixed_green_with_negative_counts_is_not_complete():
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        ids = ["a/A.java"]
        cases = (
            "    total: 1\n"
            "    ported: -1\n"
            "    passing: -1\n"
            "    excluded: []\n"
            "    not_built:\n"
            "    - name: someCase\n"
            "      reason: the original checks a JVM-only helper\n"
            "      alternative: a compile_fail doctest\n"
            + APPROVAL +
            "    - name: otherCase\n"
            "      reason: the original checks a JVM-only helper\n"
            "      alternative: a compile_fail doctest\n"
        )
        ledger = "nodes:\n" + mixed_green_node(ids[0], "G0v", APPROVAL, cases)
        response = make_run(root, order_with(["docs/goals/g0v-stub.md"]), ledger,
                            one_goal_doc("G0v", ids))
        result = run_gate(root, response)
        assert result["exit_code"] != 0, result
        print("48. mixed green + negative counts: exit_code =", result["exit_code"])


def test_mixed_green_with_scalar_not_built_is_not_complete():
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        ids = ["a/A.java"]
        cases = (
            "    total: 3\n"
            "    ported: 2\n"
            "    passing: 2\n"
            "    excluded: []\n"
            "    not_built: someCase\n"
        )
        ledger = "nodes:\n" + mixed_green_node(ids[0], "G0r", "", cases)
        response = make_run(root, order_with(["docs/goals/g0r-stub.md"]), ledger,
                            one_goal_doc("G0r", ids))
        result = run_gate(root, response)
        assert result["exit_code"] != 0, result
        print("47. mixed green + scalar not_built: exit_code =", result["exit_code"])


def test_not_built_without_approval_is_not_complete():
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        ids = ["a/A.java"]
        ledger = "nodes:\n" + not_built_node(ids[0], "G0v", "")
        response = make_run(root, order_with(["docs/goals/g0v-stub.md"]), ledger,
                            one_goal_doc("G0v", ids))
        result = run_gate(root, response)
        assert result["exit_code"] != 0, result
        assert "a/A.java" in result.get("output", ""), result
        print("29. not_built without approval: exit_code =", result["exit_code"])


def assert_not_built_incomplete(label: str, approval: str, cases: str = NOT_BUILT_CASE,
                                names_node: bool = True) -> None:
    """関門が完了にしないことを確かめる。names_node なら未完了の一覧に id が出ることも確かめる。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        ids = ["a/A.java"]
        ledger = "nodes:\n" + not_built_node(ids[0], "G0u", approval, cases)
        response = make_run(root, order_with(["docs/goals/g0u-stub.md"]), ledger,
                            one_goal_doc("G0u", ids))
        result = run_gate(root, response)
        assert result["exit_code"] != 0, (label, result)
        if names_node:
            assert "a/A.java" in result.get("output", ""), (label, result)
        print(f"{label}: exit_code =", result["exit_code"])


def test_not_built_with_malformed_approval_is_not_complete():
    """承認記録は除外と同じ is_approved_by_human で検査する（空でないだけでは通さない）。"""
    assert_not_built_incomplete(
        "30a. not_built with a nonexistent quoted date",
        "      approved_by: someone\n      approved_at: '2026-02-30'\n")
    assert_not_built_incomplete(
        "30b. not_built with an unquoted nonexistent date",
        "      approved_by: someone\n      approved_at: 2026-02-30\n",
        # 関門自身の台帳の読み込み（PyYAML の既定）がこの日付で失敗し、判定不能で止まる。
        names_node=False)
    assert_not_built_incomplete(
        "30c. not_built with a timestamp instead of a date",
        "      approved_by: someone\n      approved_at: 2026-01-01T00:00:00Z\n")
    assert_not_built_incomplete(
        "30d. not_built with a list as the approver",
        "      approved_by: [someone]\n      approved_at: 2026-01-01\n")
    assert_not_built_incomplete(
        "30e. not_built with a blank approver",
        "      approved_by: '  '\n      approved_at: 2026-01-01\n")


def test_not_built_with_duplicated_case_names_is_not_complete():
    """件数が total と合っていても、同じケース名の重複で欠けたケースを埋めさせない。"""
    record = ("    - name: someCase\n"
              "      reason: the original checks a Java helper\n"
              "      alternative: a Rust match\n"
              "      approved_by: someone\n      approved_at: 2026-01-01\n")
    cases = ("    total: 2\n    ported: 0\n    passing: 0\n    excluded: []\n"
             "    not_built:\n" + record + record)
    assert_not_built_incomplete("31. not_built with duplicated case names", "", cases)


def test_not_built_with_a_blank_case_name_is_not_complete():
    cases = NOT_BUILT_CASE.replace("name: someCase", "name: '  '")
    assert_not_built_incomplete("32. not_built with a blank case name", APPROVAL, cases)


def test_not_built_with_a_nonzero_or_missing_passing_is_not_complete():
    """全件が作らない区分のノードは、ported と passing がどちらも整数の 0 である。"""
    for label, line in (("34a. passing 1", "    passing: 1\n"),
                        ("34b. passing null", "    passing: null\n"),
                        ("34c. passing false", "    passing: false\n"),
                        ("34d. passing missing", "")):
        cases = NOT_BUILT_CASE.replace("    passing: 0\n", line)
        assert_not_built_incomplete(f"{label}: not_built", APPROVAL, cases)


def test_non_list_not_built_record_is_diagnosed_without_crashing():
    """cases.not_built の配列でない誤記で集計が止まらず、そのノードの記録不足として出る。"""
    cases = ("    total: 1\n    ported: 0\n    passing: 0\n    excluded: []\n"
             "    not_built: 1\n")
    assert_not_built_incomplete("35a. not_built record that is not a list", "", cases)
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        ids = ["a/A.java"]
        ledger = (ledger_yaml([(ids[0], "green", "G0t")])
                  + not_built_node("b/B.java", "G0s", "", cases))
        response = make_run(root, order_with(["docs/goals/g0t-stub.md"]), ledger,
                            one_goal_doc("G0t", ids))
        result = run_gate(root, response)
        assert result["exit_code"] == 0, result
        print("35b. another goal's non-list not_built record: exit_code =", result["exit_code"])


def test_another_goals_not_built_shortfall_stays_out_of_the_goal_pending_list():
    """ほかのゴールの記録不足は、対象ゴールの未完了一覧に入らない（triage はその一覧で判定する）。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        ids = ["a/A.java"]
        ledger = (ledger_yaml([(ids[0], "unported", "G0r")])
                  + not_built_node("b/B.java", "G0q", ""))
        response = make_run(root, order_with(["docs/goals/g0r-stub.md"]), ledger,
                            one_goal_doc("G0r", ids))
        result = run_gate(root, response)
        assert result["exit_code"] != 0, result
        pending = result["output"].split("未完了", 1)[1]
        assert "a/A.java (unported)" in pending, result
        assert "b/B.java" not in pending, result
        own = (ledger_yaml([(ids[0], "unported", "G0r")])
               + not_built_node("b/B.java", "G0r", ""))
        response = make_run(root, order_with(["docs/goals/g0r-stub.md"]), own,
                            one_goal_doc("G0r", ids + ["b/B.java"]))
        result = run_gate(root, response)
        pending = result["output"].split("未完了", 1)[1]
        assert "b/B.java (not_built、作らない区分の記録が不足" in pending, result
        print("36. another goal's shortfall stays out, the goal's own is listed: exit_code =",
              result["exit_code"])


def raw_not_built_node(node_id: str, goal: str, triage: str, cases: str) -> str:
    """triage と cases の YAML をそのまま差し込んだ not_built のノード（壊れた記録の再現用）。"""
    return (
        f"- id: {node_id}\n"
        f"  kind: test\n"
        f"  pekko:\n"
        f"    module: stub\n"
        f"  status: not_built\n"
        f"{triage}{cases}"
        f"  goal: {goal}\n"
    )


GOOD_TRIAGE = "  triage:\n    kind: not_built\n    reason: served by the language\n"
GOOD_CASES = "  cases:\n" + NOT_BUILT_CASE + APPROVAL


def test_non_mapping_triage_or_cases_is_diagnosed_and_other_goals_continue():
    """triage や cases がマッピングでなくても止まらない。対象なら未完了、ほかのゴールは判定を続ける。"""
    for label, triage, cases in (("37a. triage that is not a mapping", "  triage: not_built\n", GOOD_CASES),
                                 ("37b. cases that is not a mapping", GOOD_TRIAGE, "  cases: 1\n")):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            broken = raw_not_built_node("b/B.java", "G0p", triage, cases)
            response = make_run(root, order_with(["docs/goals/g0p-stub.md"]), "nodes:\n" + broken,
                                one_goal_doc("G0p", ["b/B.java"]))
            result = run_gate(root, response)
            assert result["exit_code"] == 1, (label, result)
            pending = result["output"].split("未完了", 1)[1]
            assert "b/B.java (not_built、作らない区分の記録が不足" in pending, (label, result)
            ledger = ledger_yaml([("a/A.java", "green", "G0o")]) + broken
            response = make_run(root, order_with(["docs/goals/g0o-stub.md"]), ledger,
                                one_goal_doc("G0o", ["a/A.java"]))
            result = run_gate(root, response)
            assert result["exit_code"] == 0, (label, result)
            print(f"{label}: own goal incomplete, another goal exit_code =", result["exit_code"])


def test_check_needs_continues_past_another_goals_broken_not_built():
    """check_needs は、ほかのゴールの壊れた not_built の記録で止まらない。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        copy_rollup(root)
        write(root / "ledger" / "tools" / "check_needs.py",
              (ROLLUP.parent / "check_needs.py").read_text(encoding="utf-8"))
        write(root / "ledger" / "capabilities.yaml", "capabilities: []\n")
        ledger = ("nodes:\n"
                  "- id: a/A.java\n  kind: test\n  pekko:\n    module: stub\n"
                  "  status: green\n  needs:\n    testkit: []\n  goal: G0n\n"
                  + raw_not_built_node("b/B.java", "G0m", "  triage: not_built\n", "  cases: 1\n"))
        write(root / "ledger" / "nodes.yaml", ledger)
        proc = subprocess.run(
            [sys.executable, "ledger/tools/check_needs.py", "--ledger", "ledger/nodes.yaml",
             "--capabilities", "ledger/capabilities.yaml", "--goal", "G0n"],
            cwd=str(root), capture_output=True, text=True, check=False)
        assert proc.returncode == 0, (proc.stdout, proc.stderr)
        print("38. check_needs past another goal's broken not_built: exit", proc.returncode)


def test_not_built_node_with_an_exclusion_is_not_complete_or_double_counted():
    """全件を作らないノードに承認済みの除外を残すと未完了で、除外の有効件数にも数えない。"""
    excluded = ("    excluded:\n"
                "    - name: someCase\n      reason: also excluded\n"
                "      approved_by: someone\n      approved_at: 2026-01-01\n")
    cases = NOT_BUILT_CASE.replace("    excluded: []\n", excluded)
    assert_not_built_incomplete("39a. not_built with an approved exclusion of the same case",
                                APPROVAL, cases)
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        ledger = "nodes:\n" + not_built_node("a/A.java", "G0l", APPROVAL, cases)
        response = make_run(root, order_with(["docs/goals/g0l-stub.md"]), ledger,
                            one_goal_doc("G0l", ["a/A.java"]))
        result = run_gate(root, response)
        assert "除外=0（記録 1） 作らない=0（記録 1）" in result["output"], result
        print("39b. the double record is counted only as recorded")


def test_not_built_with_a_non_integer_total_or_padded_duplicate_name_is_not_complete():
    """total は正の整数で、ケース名の前後の空白で重複の判定をすり抜けさせない。"""
    assert_not_built_incomplete("40a. not_built with total: true", APPROVAL,
                                NOT_BUILT_CASE.replace("    total: 1\n", "    total: true\n"))
    record = ("    - name: {name}\n"
              "      reason: the original checks a Java helper\n"
              "      alternative: a Rust match\n"
              "      approved_by: someone\n      approved_at: 2026-01-01\n")
    cases = ("    total: 2\n    ported: 0\n    passing: 0\n    excluded: []\n"
             "    not_built:\n" + record.format(name="someCase")
             + record.format(name="'someCase '"))
    assert_not_built_incomplete("40b. not_built with a duplicated name padded by a space", "", cases)


def _lineage(successor_status: str = "unported", total=20, ported=1, passing=1, carried_from="old/A.scala",
             retired_extra: dict | None = None, active_extra: list | None = None):
    import rollup
    carried = [{"name": "case", "from": carried_from, "source": "new/S.scala L1-2", "rust_test": "t.rs",
                "verified_against": "v2.0.0", "evidence": "1 passed"}]
    successor = {"id": "new/S.scala", "status": successor_status,
                 "cases": {"total": total, "ported": ported, "passing": passing, "carried": carried}}
    retired = {"id": "old/A.scala", "goal": "G0r",
               "cases": {"ported": 1}, "retired": {"at": "2026-11-25", "reason": "moved", "successor": "new/S.scala", **(retired_extra or {})}}
    return rollup.lineage_problems([successor] + (active_extra or []), [retired])


def test_retired_node_and_carried_case_are_checked_and_not_double_counted():
    """退役したノードと、ケースを引き継いだ部分移植のノードの記録を検査する。"""
    assert _lineage() == {}, _lineage()
    assert "new/S.scala" in _lineage(ported=2, passing=2), "ported must equal the carried cases"
    assert "new/S.scala" in _lineage(total=1), "a partly ported suite must not add up to its total"
    assert "new/S.scala" in _lineage(carried_from="other/B.scala"), "carried from a node that is not retired"
    assert "old/A.scala" in _lineage(retired_extra={"successor": "missing/X.scala"})
    assert "old/A.scala" in _lineage(retired_extra={"at": "2026-02-30T00:00"})
    assert "old/A.scala" in _lineage(active_extra=[{"id": "old/A.scala", "status": "green", "cases": {}}])
    assert _lineage(successor_status="green", total=20, ported=20, passing=20) == {}
    print("43. retired and carried records are checked")


def test_retired_dates_must_be_real_calendar_days():
    """退役日は書式だけでなく、実在する暦日であること（時刻つきも拒否する）。"""
    assert "old/A.scala" in _lineage(retired_extra={"at": "2026-02-30"}), "a nonexistent day"
    assert "old/A.scala" in _lineage(retired_extra={"at": "2026-11-25T00:00:00"}), "a timestamp"
    print("43b. retired.at must be a real calendar day")


def test_a_case_carried_to_a_node_that_is_not_the_successor_is_refused():
    """退役記録の successor と違うノードへの引き継ぎは、二重計上になるので拒否する。"""
    import rollup
    carried = [{"name": "case", "from": "old/A.scala", "source": "s L1", "rust_test": "t.rs",
                "verified_against": "v2.0.0", "evidence": "1 passed"}]
    successor = {"id": "new/S.scala", "status": "unported",
                 "cases": {"total": 20, "ported": 1, "passing": 1, "carried": carried}}
    other = {"id": "other/O.scala", "status": "unported",
             "cases": {"total": 3, "ported": 1, "passing": 1, "carried": [dict(carried[0])]}}
    retired = {"id": "old/A.scala", "cases": {"ported": 1}, "retired": {"at": "2026-11-25", "reason": "moved", "successor": "new/S.scala"}}
    problems = rollup.lineage_problems([successor, other], [retired])
    assert "other/O.scala" in problems and "new/S.scala" not in problems, problems
    print("43c. a case carried to a node other than the successor is refused")


def test_malformed_lineage_fields_are_diagnosed_and_other_goals_continue():
    """successor・退役 id・carried・ケース名の型が違っても例外で止めず、診断を残してほかのゴールを判定する。"""
    import rollup
    good = {"name": "case", "from": "old/A.scala", "source": "s L1", "rust_test": "t.rs",
            "verified_against": "v2.0.0", "evidence": "1 passed"}
    cases = [
        ([{"id": "new/S.scala", "status": "unported", "cases": {"carried": 1}}],
         [{"id": "old/A.scala", "retired": {"at": "2026-11-25", "reason": "r", "successor": ["new/S.scala"]}}]),
        ([{"id": "new/S.scala", "status": "unported", "cases": {"total": 2, "ported": 1, "passing": 1,
                                                                 "carried": [dict(good, name=["x"])]}}],
         [{"id": ["old/A.scala"], "retired": {"at": "2026-11-25", "reason": "r", "successor": "new/S.scala"}}]),
    ]
    for nodes, retired in cases:
        problems = rollup.lineage_problems(nodes, retired)
        assert problems, (nodes, retired)
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        ledger = ("nodes:\n- id: a/A.scala\n  kind: test\n  pekko:\n    module: stub\n  status: green\n"
                  "  goal: G0k\n- id: new/S.scala\n  kind: test\n  pekko:\n    module: stub\n"
                  "  status: unported\n  cases:\n    carried: 1\n  goal: null\n"
                  "retired:\n- id: old/A.scala\n  kind: test\n  pekko:\n    module: stub\n  goal: G0z\n"
                  "  retired:\n    at: 2026-11-25\n    reason: moved\n    successor: [new/S.scala]\n")
        response = make_run(root, order_with(["docs/goals/g0k-stub.md"]), ledger, one_goal_doc("G0k", ["a/A.scala"]))
        result = run_gate(root, response)
        assert result["exit_code"] == 0, result
        assert "退役・引き継ぎの記録が不足" in result["output"], result
        print("43d. malformed lineage is diagnosed and another goal is still judged")


def test_lineage_refuses_duplicates_unknown_tags_and_a_non_list_retired():
    """退役 id の重複、空白だけが違うケース名、未知の基準タグ、配列でない retired を拒否する。"""
    import rollup
    record = {"name": "case", "from": "old/A.scala", "source": "s L1", "rust_test": "t.rs",
              "verified_against": "v2.0.0", "evidence": "1 passed"}
    successor = {"id": "new/S.scala", "status": "unported",
                 "cases": {"total": 20, "ported": 1, "passing": 1, "carried": [record]}}
    retired = {"id": "old/A.scala", "cases": {"ported": 1}, "retired": {"at": "2026-11-25", "reason": "moved", "successor": "new/S.scala"}}
    assert "old/A.scala" in rollup.lineage_problems([successor], [retired, dict(retired)]), "duplicated retired id"
    # 名前の重複だけが診断されるように、退役元の移植数を 2 にし、2 件目には別の証拠を持たせる。
    roomy = dict(retired, cases={"ported": 2})
    padded = dict(successor, cases=dict(successor["cases"], ported=2, passing=2,
                                        carried=[record, dict(record, name="case ", source="s L9", rust_test="u.rs")]))
    padded_problems = rollup.lineage_problems([padded], [roomy]).get("new/S.scala", [])
    assert padded_problems and all("ケース名が重複" in p for p in padded_problems), padded_problems
    unknown = dict(successor, cases=dict(successor["cases"], carried=[dict(record, verified_against="not-a-tag")]))
    assert "new/S.scala" in rollup.lineage_problems([unknown], [retired]), "an unknown baseline tag"
    assert rollup.lineage_problems([successor], {}), "retired: {} is not a list"
    print("43e. duplicates, padded names, unknown tags and a non-list retired are refused")


LINEAGE_LEDGER = ("nodes:\n- id: new/S.scala\n  kind: test\n  pekko:\n    module: stub\n  status: green\n"
                  "  cases:\n    total: 1\n    ported: 1\n    passing: 1\n    carried:\n"
                  "    - name: case\n      from: old/A.scala\n      source: new/S.scala L1\n"
                  "      rust_test: t.rs\n      verified_against: v2.0.0\n      evidence: 1 passed\n"
                  "  goal: G0s\n- id: a/A.scala\n  kind: test\n  pekko:\n    module: stub\n  status: green\n"
                  "  goal: G0t\nretired:\n- id: old/A.scala\n  kind: test\n  pekko:\n    module: stub\n"
                  "  cases:\n    ported: 1\n  goal: G0r\n  retired:\n    at: 2026-11-25\n    reason: {reason}\n    successor: new/S.scala\n")


def test_a_retired_side_problem_keeps_the_successors_goal_open_and_the_rollup_fails():
    """退役側だけの不正でも、後継が属するゴールは完了にせず、全体の rollup は 0 以外で終わる。

    無関係なゴールは止めずに判定する。
    """
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        ledger = LINEAGE_LEDGER.format(reason="''")
        response = make_run(root, order_with(["docs/goals/g0s-stub.md"]), ledger, one_goal_doc("G0s", ["new/S.scala"]))
        result = run_gate(root, response)
        assert result["exit_code"] == 1, result
        assert "退役・引き継ぎの記録が不足" in result["output"], result
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        ledger = LINEAGE_LEDGER.format(reason="''")
        response = make_run(root, order_with(["docs/goals/g0t-stub.md"]), ledger, one_goal_doc("G0t", ["a/A.scala"]))
        assert run_gate(root, response)["exit_code"] == 0
        path = root / "ledger" / "nodes.yaml"
        whole = subprocess.run([sys.executable, str(ROLLUP), "--ledger", str(path)], capture_output=True, text=True)
        assert whole.returncode != 0, whole.stdout
        path.write_text(LINEAGE_LEDGER.format(reason="moved"), encoding="utf-8")
        whole = subprocess.run([sys.executable, str(ROLLUP), "--ledger", str(path)], capture_output=True, text=True)
        assert whole.returncode == 0, whole.stdout
    print("43f. a retired-side problem keeps the successor's goal open, the whole rollup fails, others are judged")


def test_lineage_refuses_a_renamed_duplicate_and_more_cases_than_the_retired_node_ported():
    """名前だけを変えた同じケース（原典の位置・Rust テスト・退役元が同じ）と、退役元の移植数を超える引き継ぎを拒否する。"""
    import rollup
    record = {"name": "case", "from": "old/A.scala", "source": "s L1", "rust_test": "t.rs",
              "verified_against": "v2.0.0", "evidence": "1 passed"}
    successor = {"id": "new/S.scala", "status": "unported",
                 "cases": {"total": 20, "ported": 2, "passing": 2,
                           "carried": [record, dict(record, name="case (alias)")]}}
    retired = {"id": "old/A.scala", "cases": {"ported": 1},
               "retired": {"at": "2026-11-25", "reason": "moved", "successor": "new/S.scala"}}
    problems = rollup.lineage_problems([successor], [retired])
    assert any("同じ" in p for p in problems.get("new/S.scala", [])), problems
    assert any("移植数" in p for p in problems.get("new/S.scala", [])), problems
    print("43g. a renamed duplicate and more cases than the retired node ported are refused")


def test_a_duplicated_retired_id_reaches_every_successor_it_names():
    """同じ退役 id が別々の後継を指すとき、最初の後継も含め、すべての後継に異常を伝える。"""
    import rollup
    record = {"name": "case", "from": "old/A.scala", "source": "s L1", "rust_test": "t.rs",
              "verified_against": "v2.0.0", "evidence": "1 passed"}
    first = {"id": "new/S.scala", "status": "green",
             "cases": {"total": 1, "ported": 1, "passing": 1, "excluded": [], "carried": [record]}}
    second = {"id": "new/T.scala", "status": "unported", "cases": {}}
    retired = [{"id": "old/A.scala", "cases": {"ported": 1},
                "retired": {"at": "2026-11-25", "reason": "moved", "successor": successor}}
               for successor in ("new/S.scala", "new/T.scala")]
    problems = rollup.lineage_problems([first, second], retired)
    assert "new/S.scala" in problems and "new/T.scala" in problems, problems
    print("43h. a duplicated retired id reaches every successor it names")


def test_a_green_successor_must_cover_its_total_with_ported_and_approved_exclusions():
    """引き継ぎを持つ green の後継も、total = ported + 承認済みの除外、passing = ported を満たす。"""
    import rollup
    record = {"name": "case", "from": "old/A.scala", "source": "s L1", "rust_test": "t.rs",
              "verified_against": "v2.0.0", "evidence": "1 passed"}
    retired = {"id": "old/A.scala", "cases": {"ported": 1},
               "retired": {"at": "2026-11-25", "reason": "moved", "successor": "new/S.scala"}}
    def green(total, ported, passing, excluded=()):
        return {"id": "new/S.scala", "status": "green",
                "cases": {"total": total, "ported": ported, "passing": passing,
                          "excluded": list(excluded), "carried": [record]}}
    assert "new/S.scala" in rollup.lineage_problems([green(20, 1, 1)], [retired]), "only the carried case"
    assert "new/S.scala" in rollup.lineage_problems([green(2, 2, 1)], [retired]), "passing below ported"
    approved = {"name": "x", "reason": "r", "approved_by": "someone", "approved_at": "2026-01-01"}
    assert rollup.lineage_problems([green(3, 2, 2, [approved])], [retired]) == {}
    assert "new/S.scala" in rollup.lineage_problems([green(3, 2, 2, [{"name": "x", "reason": "r"}])], [retired])
    print("43i. a green successor must cover its total")


def test_a_scalar_retired_list_is_diagnosed_without_crashing_the_goal_check():
    """retired が整数でも、ゴールの判定は例外で止まらず、診断を出して 0 以外で終わる。"""
    with tempfile.TemporaryDirectory() as temporary:
        path = Path(temporary) / "nodes.yaml"
        path.write_text("nodes: []\nretired: 1\n", encoding="utf-8")
        result = subprocess.run([sys.executable, str(ROLLUP), "--ledger", str(path), "--goal", "G0issue99"],
                                capture_output=True, text=True)
        assert result.returncode == 1, (result.stdout, result.stderr)
        assert "Traceback" not in result.stderr, result.stderr
        assert "retired が配列でない" in result.stdout, result.stdout
    print("43j. a scalar retired is diagnosed and the goal check ends with 1")


CARRIED_YAML = ("    carried:\n    - name: case\n      from: old/A.scala\n      source: new/S.scala L1\n"
                "      rust_test: t.rs\n      verified_against: v2.0.0\n      evidence: 1 passed\n")
RETIRED_YAML = ("retired:\n- id: old/A.scala\n  kind: test\n  pekko:\n    module: stub\n  cases:\n    ported: 1\n"
                "  goal: G0r\n  retired:\n    at: 2026-11-25\n    reason: moved\n    successor: new/S.scala\n")


def lineage_node(node_id: str, goal: str, status: str, total: int, ported: int, excluded: str = "    excluded: []\n") -> str:
    return (f"- id: {node_id}\n  kind: test\n  pekko:\n    module: stub\n  status: {status}\n"
            f"  cases:\n    total: {total}\n    ported: {ported}\n    passing: {ported}\n{excluded}"
            f"{CARRIED_YAML}  goal: {goal}\n")


def gate_and_whole(ledger: str, goal: str, member: str) -> tuple[int, int]:
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        response = make_run(root, order_with([f"docs/goals/{goal.lower()}-stub.md"]), ledger, one_goal_doc(goal, [member]))
        gate = run_gate(root, response)["exit_code"]
        whole = subprocess.run([sys.executable, str(ROLLUP), "--ledger", str(root / "ledger" / "nodes.yaml")],
                               capture_output=True, text=True).returncode
        return gate, whole


def test_the_named_successor_is_judged_the_same_whichever_node_comes_first():
    """未指定の後継に同じケースを置いても、並び順によらず正式な後継のゴールは完了で、全体の検査は 0 以外。"""
    right = lineage_node("new/S.scala", "G0s", "green", 1, 1)
    wrong = lineage_node("new/W.scala", "G0w", "unported", 3, 1)
    for label, nodes in (("wrong first", wrong + right), ("wrong last", right + wrong)):
        gate, whole = gate_and_whole("nodes:\n" + nodes + RETIRED_YAML, "G0s", "new/S.scala")
        assert gate == 0, (label, gate)
        assert whole != 0, (label, whole)
        print(f"43k. {label}: the named successor's gate = {gate}, whole rollup = {whole}")


def test_a_green_successor_counts_only_valid_distinct_exclusions_and_its_carried_cases():
    """承認済みの同じ除外の繰り返しや、引き継いだのに ported が 0 の green は、ゴールでも全体でも通さない。"""
    approval = "      approved_by: someone\n      approved_at: 2026-01-01\n"
    repeated = "    excluded:\n" + "".join("    - name: same\n      reason: r\n" + approval for _ in range(19))
    distinct = "    excluded:\n" + "".join(f"    - name: case {i}\n      reason: r\n" + approval for i in range(20))
    for label, node in (("19 copies of one exclusion", lineage_node("new/S.scala", "G0s", "green", 20, 1, repeated)),
                        ("carried but 20/0/0", lineage_node("new/S.scala", "G0s", "green", 20, 0, distinct))):
        gate, whole = gate_and_whole("nodes:\n" + node + RETIRED_YAML, "G0s", "new/S.scala")
        assert gate != 0 and whole != 0, (label, gate, whole)
        print(f"43l. {label}: gate = {gate}, whole rollup = {whole}")


def test_the_gate_reads_an_invalid_unquoted_retired_date_without_crashing():
    """引用なしの存在しない退役日があっても、関門は台帳の読み込みで止まらず、無関係なゴールを判定する。"""
    ledger = ("nodes:\n- id: a/A.scala\n  kind: test\n  pekko:\n    module: stub\n  status: green\n  goal: G0t\n"
              "retired:\n- id: old/A.scala\n  kind: test\n  pekko:\n    module: stub\n  goal: G0r\n"
              "  retired:\n    at: 2026-02-30\n    reason: moved\n    successor: a/A.scala\n")
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        response = make_run(root, order_with(["docs/goals/g0t-stub.md"]), ledger, one_goal_doc("G0t", ["a/A.scala"]))
        result = run_gate(root, response)
        assert "Traceback" not in json.dumps(result), result
        # a/A.scala は不正な退役記録の指定後継なので、読み込みを続けたうえで未完了にする。
        assert result["exit_code"] == 1, result
        assert "a/A.scala" in result.get("output", ""), result
        print("43m. the gate reads an invalid unquoted date without crashing: exit_code =", result["exit_code"])


def test_a_retired_record_with_a_bad_id_still_reaches_its_named_successor():
    """退役 id が不正でも、特定できる指定後継には異常を伝える（後継のゴールを完了にしない）。"""
    import rollup
    successor = {"id": "new/S.scala", "status": "green", "cases": {"total": 1, "ported": 1, "passing": 1}}
    retired = [{"id": "", "retired": {"at": "2026-11-25", "reason": "moved", "successor": "new/S.scala"}}]
    assert "new/S.scala" in rollup.lineage_problems([successor], retired)
    print("43n. a bad retired id still reaches its named successor")


def test_a_non_list_retired_fails_a_goal_check_even_when_the_goal_is_green():
    """retired が配列でなければ、対象ゴールのノードがすべて green でも rollup --goal は 1 を返す。"""
    with tempfile.TemporaryDirectory() as temporary:
        path = Path(temporary) / "nodes.yaml"
        path.write_text("nodes:\n- id: a/A.scala\n  kind: test\n  pekko:\n    module: stub\n  status: green\n"
                        "  goal: G0t\nretired: {}\n", encoding="utf-8")
        result = subprocess.run([sys.executable, str(ROLLUP), "--ledger", str(path), "--goal", "G0t"],
                                capture_output=True, text=True)
        assert result.returncode == 1, (result.stdout, result.stderr)
    print("43o. a non-list retired fails the goal check")


def test_a_green_successor_cannot_count_a_carried_case_again_as_an_exclusion():
    """引き継いだケースと同じ名前の除外は、同じケースを 2 回数えるので拒否する（ゴールでも全体でも）。"""
    approval = "      approved_by: someone\n      approved_at: 2026-01-01\n"
    excluded = ("    excluded:\n" + "".join(f"    - name: other {i}\n      reason: r\n" + approval for i in range(18))
                + "    - name: case\n      reason: r\n" + approval)
    ledger = "nodes:\n" + lineage_node("new/S.scala", "G0s", "green", 20, 1, excluded) + RETIRED_YAML
    gate, whole = gate_and_whole(ledger, "G0s", "new/S.scala")
    assert gate != 0 and whole != 0, (gate, whole)
    # 集計の有効な除外の件数からは、引き継いだケースと重なる 1 件を外す（記録の件数には残す）。
    with tempfile.TemporaryDirectory() as temporary:
        path = Path(temporary) / "nodes.yaml"
        path.write_text(ledger, encoding="utf-8")
        output = subprocess.run([sys.executable, str(ROLLUP), "--ledger", str(path)], capture_output=True, text=True).stdout
        assert "除外=18（記録 19）" in output, output
    distinct = "    excluded:\n" + "".join(f"    - name: other {i}\n      reason: r\n" + approval for i in range(19))
    gate, whole = gate_and_whole("nodes:\n" + lineage_node("new/S.scala", "G0s", "green", 20, 1, distinct) + RETIRED_YAML,
                                 "G0s", "new/S.scala")
    assert gate == 0 and whole == 0, (gate, whole)
    print("43p. a carried case cannot be counted again as an exclusion; distinct exclusions still pass")


def test_a_goal_with_active_and_retired_members_is_judged_through_the_gate():
    """対象表に現役と退役のノードが並ぶゴールも、関門の照合を通り、現役が完了なら完了になる。"""
    ledger = ("nodes:\n- id: new/S.scala\n  kind: test\n  pekko:\n    module: stub\n  status: unported\n"
              "  cases:\n    total: 2\n    ported: 1\n    passing: 1\n" + CARRIED_YAML +
              "  goal: null\n- id: a/A.scala\n  kind: test\n  pekko:\n    module: stub\n  status: green\n"
              "  goal: G0r\n" + RETIRED_YAML)
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        response = make_run(root, order_with(["docs/goals/g0r-stub.md"]), ledger,
                            one_goal_doc("G0r", ["a/A.scala", "old/A.scala"]))
        result = run_gate(root, response)
        assert result["exit_code"] == 0, result
        print("43q. a goal with active and retired members: exit_code =", result["exit_code"])


def test_a_do_not_port_successor_with_carried_cases_does_not_complete_its_goal():
    """引き継ぎを持つ後継を do_not_port にしても、関門はそのゴールを完了にしない。"""
    node = ("- id: new/S.scala\n  kind: test\n  pekko:\n    module: stub\n  status: do_not_port\n"
            "  cases:\n    total: 20\n    ported: 1\n    passing: 1\n    excluded: []\n" + CARRIED_YAML +
            "  goal: G0s\n")
    gate, whole = gate_and_whole("nodes:\n" + node + RETIRED_YAML, "G0s", "new/S.scala")
    assert gate != 0 and whole != 0, (gate, whole)
    print("43r. a do_not_port successor: gate =", gate, "whole =", whole)


def test_goal_whose_nodes_are_all_retired_is_not_complete():
    """属するノードがすべて退役したゴールは、完了と判定しない（退役を理由に出す）。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        ledger = ("nodes:\n- id: new/S.scala\n  kind: test\n  pekko:\n    module: stub\n  status: unported\n"
                  "  cases:\n    total: 2\n    ported: 1\n    passing: 1\n    carried:\n"
                  "    - name: case\n      from: old/A.scala\n      source: new/S.scala L1\n"
                  "      rust_test: t.rs\n      verified_against: v2.0.0\n      evidence: 1 passed\n"
                  "  goal: null\nretired:\n- id: old/A.scala\n  kind: test\n  pekko:\n    module: stub\n"
                  "  status: green\n  cases:\n    ported: 1\n  goal: G0r\n  retired:\n    at: 2026-11-25\n    reason: moved\n"
                  "    successor: new/S.scala\n")
        response = make_run(root, order_with(["docs/goals/g0r-stub.md"]), ledger, one_goal_doc("G0r", ["old/A.scala"]))
        result = run_gate(root, response)
        # 対象表の照合を通って rollup の退役の分岐に届き、退役を理由に完了と判定しない（1）。
        assert result["exit_code"] == 1, result
        assert "退役した" in result.get("output", ""), result
        print("44. a goal whose nodes are all retired: exit_code =", result["exit_code"])


def test_takt_triage_treats_the_same_statuses_as_done():
    """TAKT の triage が処理済みとする status は、rollup の DONE_STATUSES と同じである。

    rollup が完了に数える status を triage が対象に選ぶと、作らない区分のノードを
    移植し直す。triage の指示文と、子ワークフローの goal_complete の条件の両方に、
    DONE_STATUSES の全 status と、記録の不足した not_built を needs_human にする規則がある。
    """
    import yaml

    import rollup

    triage = TRIAGE.read_text(encoding="utf-8")
    workflow = yaml.safe_load(PORT_NODE.read_text(encoding="utf-8"))
    step = next(s for s in workflow["steps"] if s["name"] == "triage")
    complete = [r["condition"] for r in step["rules"] if r.get("return") == "goal_complete"]
    human = [r["condition"] for r in step["rules"] if r.get("return") == "needs_human"]
    assert len(complete) == 1, complete
    for status in rollup.DONE_STATUSES:
        assert f"`{status}`" in triage, (status, "fraktor-triage.md")
        assert status in complete[0], (status, complete[0])
    assert "rollup.py" in triage, "fraktor-triage.md"
    assert any("not_built" in condition and "対象ゴール" in condition for condition in human), human
    assert "対象ゴールに属する" in triage, "fraktor-triage.md"
    print("33. TAKT triage statuses match rollup.DONE_STATUSES:", rollup.DONE_STATUSES)


def test_goal_without_members_is_not_complete():
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        ids = ["a/A.scala"]
        ledger = ledger_yaml([(ids[0], "green", "G0y")])
        response = make_run(root, order_with(["docs/goals/g0z-stub.md"]), ledger,
                            one_goal_doc("G0z", ids))
        result = run_gate(root, response)
        assert result["exit_code"] != 0, result
        print("4. no member for goal: exit_code =", result["exit_code"])


def test_order_without_goal_path_fails():
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        ids = ["a/A.scala"]
        ledger = ledger_yaml([(ids[0], "green", "G0z")])
        response = make_run(root, "# タスク\n\nゴール文書の指定が無い。\n", ledger,
                            one_goal_doc("G0z", ids))
        result = run_gate(root, response)
        assert result["exit_code"] != 0, result
        print("5. no goal path: exit_code =", result["exit_code"])


def test_order_with_multiple_goal_paths_fails():
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        ids = ["a/A.scala"]
        ledger = ledger_yaml([(ids[0], "green", "G0z")])
        docs = {
            "g0z-stub.md": goal_doc("G0z", ids),
            "g0w-stub.md": goal_doc("G0w", ids),
        }
        response = make_run(root, order_with(["docs/goals/g0z-stub.md",
                                              "docs/goals/g0w-stub.md"]), ledger, docs)
        result = run_gate(root, response)
        assert result["exit_code"] != 0, result
        print("6. multiple goal paths: exit_code =", result["exit_code"])


def test_missing_goal_document_fails():
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        ids = ["a/A.scala"]
        ledger = ledger_yaml([(ids[0], "green", "G0z")])
        # order は文書を指すが、文書は作らない。
        response = make_run(root, order_with(["docs/goals/g0z-stub.md"]), ledger, {})
        result = run_gate(root, response)
        assert result["exit_code"] != 0, result
        print("7. missing goal document: exit_code =", result["exit_code"])


def test_unreadable_goal_id_fails():
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        ids = ["a/A.scala"]
        ledger = ledger_yaml([(ids[0], "green", "G0z")])
        doc = "先頭行にゴール見出しが無い文書。\n\n" + goal_doc("G0z", ids).split("\n", 1)[1]
        response = make_run(root, order_with(["docs/goals/g0z-stub.md"]), ledger,
                            {"g0z-stub.md": doc})
        result = run_gate(root, response)
        assert result["exit_code"] != 0, result
        print("8. unreadable goal id: exit_code =", result["exit_code"])


def test_missing_environment_variable_fails():
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        ids = ["a/A.scala"]
        ledger = ledger_yaml([(ids[0], "green", "G0z")])
        make_run(root, order_with(["docs/goals/g0z-stub.md"]), ledger,
                 one_goal_doc("G0z", ids))
        result = run_gate(root, None, set_env=False)
        assert result["exit_code"] != 0, result
        print("9. no TAKT_PREVIOUS_RESPONSE_FILE: exit_code =", result["exit_code"])


def test_missing_response_file_fails():
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        ids = ["a/A.scala"]
        ledger = ledger_yaml([(ids[0], "green", "G0z")])
        make_run(root, order_with(["docs/goals/g0z-stub.md"]), ledger,
                 one_goal_doc("G0z", ids))
        missing = root / ".takt" / "runs" / SLUG / "context" / "system-commands" / UUID / "absent.txt"
        result = run_gate(root, missing)
        assert result["exit_code"] != 0, result
        print("10. missing response file: exit_code =", result["exit_code"])


def test_two_levels_up_does_not_reach_order():
    """3階層上を固定する。2階層上（system-commands）に order.md を置いても見つからない。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        ids = ["a/A.scala"]
        ledger = ledger_yaml([(ids[0], "green", "G0z")])
        response = make_run(root, order_with(["docs/goals/g0z-stub.md"]), ledger,
                            one_goal_doc("G0z", ids))
        run = root / ".takt" / "runs" / SLUG
        # 正しい位置の order.md を消し、2階層上（system-commands/task/order.md）だけ置く。
        (run / "context" / "task" / "order.md").unlink()
        write(run / "context" / "system-commands" / "task" / "order.md",
              order_with(["docs/goals/g0z-stub.md"]))
        result = run_gate(root, response)
        assert result["exit_code"] != 0, result
        print("11. order at two levels up is not used: exit_code =", result["exit_code"])


def test_table_ids_must_match_ledger_goal():
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        doc_ids = ["a/A.scala", "a/B.scala"]
        # 台帳は文書の A だけ G0z、B は別ゴール。表と台帳の所属が食い違う。
        ledger = ledger_yaml([(doc_ids[0], "green", "G0z"), (doc_ids[1], "green", "G0y")])
        response = make_run(root, order_with(["docs/goals/g0z-stub.md"]), ledger,
                            one_goal_doc("G0z", doc_ids))
        result = run_gate(root, response)
        assert result["exit_code"] != 0, result
        print("12. table/ledger mismatch: exit_code =", result["exit_code"])


def test_table_without_id_column_fails():
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        ids = ["a/A.scala"]
        ledger = ledger_yaml([(ids[0], "green", "G0z")])
        doc = (
            "# ゴール G0z：テスト用のゴール\n\n"
            "## 対象ノード（1件）\n\n"
            "| 名前 | ケース数 |\n|---|---|\n| `a/A.scala` | 1 |\n"
        )
        response = make_run(root, order_with(["docs/goals/g0z-stub.md"]), ledger,
                            {"g0z-stub.md": doc})
        result = run_gate(root, response)
        assert result["exit_code"] != 0, result
        print("13. table without id column: exit_code =", result["exit_code"])


def test_two_and_five_column_tables_both_pass():
    for columns in ("two", "five"):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            ids = ["a/A.scala", "a/B.scala"]
            ledger = ledger_yaml([(ids[0], "green", "G0z"), (ids[1], "do_not_port", "G0z")])
            response = make_run(root, order_with(["docs/goals/g0z-stub.md"]), ledger,
                                one_goal_doc("G0z", ids, columns))
            result = run_gate(root, response)
            assert result["exit_code"] == 0, (columns, result)
            print(f"14. {columns}-column table: exit_code =", result["exit_code"])


def test_real_goal_documents_match_current_ledger():
    """実物のゴール文書6本で、1行目 ID と表 id 集合が現行台帳の所属と一致する。

    文書・台帳の文字列はテストへ埋め込まず、ファイルから読む。関門を一時ディレクトリへ
    コピーし、実物の文書と台帳をそこへ写して走らせる（実リポジトリは書き換えない）。
    終了値の期待は台帳のノード状態から求める。未完了のゴール文書があっても、
    完了状態と終了値が一致することだけを確かめる（完了を前提にしない）。
    """
    import re

    import rollup

    ledger_nodes = rollup.load_nodes(str(REAL_LEDGER))
    ledger_retired = rollup.load_retired(str(REAL_LEDGER))
    docs = sorted(GOALS.glob("*.md"))
    assert docs, f"no goal documents under {GOALS}"
    for doc_path in docs:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            docs_dir = root / "docs" / "goals"
            docs_dir.mkdir(parents=True)
            (docs_dir / doc_path.name).write_text(
                doc_path.read_text(encoding="utf-8"), encoding="utf-8")
            (root / "ledger").mkdir(parents=True)
            (root / "ledger" / "nodes.yaml").write_text(
                REAL_LEDGER.read_text(encoding="utf-8"), encoding="utf-8")
            copy_rollup(root)
            (root / "context" / "task").mkdir(parents=True)
            (root / "context" / "task" / "order.md").write_text(
                order_with([f"docs/goals/{doc_path.name}"]), encoding="utf-8")
            # context/system-commands/<uuid>/response.txt の3階層上を context にする配置。
            (root / "context" / "system-commands" / UUID).mkdir(parents=True)
            (root / "context" / "system-commands" / UUID / "response.txt").write_text(
                "dummy\n", encoding="utf-8")
            response = root / "context" / "system-commands" / UUID / "response.txt"

            first_line = doc_path.read_text(encoding="utf-8").splitlines()[0]
            m = re.match(r"^# ゴール (G0[a-z0-9]+)：", first_line)
            assert m, (doc_path.name, first_line)
            goal_id = m.group(1)
            members = sorted(n["id"] for n in ledger_nodes if n.get("goal") == goal_id)
            if not members:
                # 属するノードがすべて退役したゴール（前の基準で閉じた記録）。表の id は退役した
                # ノードと一致し、関門は完了と判定しない（rollup は退役を理由に 1 を返す）。
                retired = sorted(r["id"] for r in ledger_retired if r.get("goal") == goal_id)
                assert retired, (doc_path.name, "no ledger member for goal", goal_id)
                import goal_complete_gate
                table_ids, error = goal_complete_gate.table_ids_from_document(doc_path.read_text(encoding="utf-8"))
                assert sorted(table_ids or []) == retired, (doc_path.name, table_ids, error, retired)
                result = run_gate(root, response)
                assert result["exit_code"] == 1, (doc_path.name, result)
                assert "退役した" in result.get("output", ""), (doc_path.name, result)
                print(f"15. real doc {doc_path.name} ({goal_id}): retired, exit_code =", result["exit_code"])
                continue

            member_nodes = [n for n in ledger_nodes if n.get("goal") == goal_id]
            complete = all(rollup.is_done(n) for n in member_nodes)
            expected = 0 if complete else 1
            result = run_gate(root, response)
            assert result["exit_code"] == expected, (doc_path.name, complete, result)
            print(f"15. real doc {doc_path.name} ({goal_id}): exit_code =", result["exit_code"],
                  "(complete)" if complete else "(incomplete)")


def test_without_order_md_uses_meta_task():
    """`-t` 直渡しの発行（order.md 無し）でも meta.json の task から完了判定できる。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        ids = ["a/A.scala", "a/B.scala"]
        ledger = ledger_yaml([(ids[0], "green", "G0z"), (ids[1], "green", "G0z")])
        response = make_run_without_order(
            root, order_with(["docs/goals/g0z-stub.md"]), ledger, one_goal_doc("G0z", ids))
        result = run_gate(root, response)
        assert result["exit_code"] == 0, result
        print("17. -t path via meta.json: exit_code =", result["exit_code"])


def test_without_order_md_and_without_meta_fails():
    """order.md も meta.json も無ければ判定不能として失敗し、COMPLETE にしない。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        ids = ["a/A.scala"]
        ledger = ledger_yaml([(ids[0], "green", "G0z")])
        run = root / ".takt" / "runs" / SLUG
        response = run / "context" / "system-commands" / UUID / "response.txt"
        write(response, "dummy\n")
        write(root / "ledger" / "nodes.yaml", ledger)
        copy_rollup(root)
        result = run_gate(root, response)
        assert result["exit_code"] != 0, result
        print("18. neither order.md nor meta.json: exit_code =", result["exit_code"])


def test_invalid_ledger_yaml_yields_json_failure():
    """台帳 YAML が不正でも終了コード 0 で JSON を出し、exit_code 非0 と原因を返す。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        ids = ["a/A.scala"]
        ledger = "nodes: [\n"
        response = make_run(root, order_with(["docs/goals/g0z-stub.md"]), ledger,
                            one_goal_doc("G0z", ids))
        result = run_gate(root, response)
        assert result["exit_code"] != 0, result
        assert "detail" in result and result["detail"], result
        print("19. invalid ledger yaml: exit_code =", result["exit_code"])


def test_table_after_target_section_is_ignored():
    """`## 対象ノード` 節の後に別の表を足しても、節外の表を id と解釈しない。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        ids = ["a/A.scala", "a/B.scala"]
        ledger = ledger_yaml([(ids[0], "green", "G0z"), (ids[1], "green", "G0z")])
        doc = one_goal_doc("G0z", ids)["g0z-stub.md"]
        doc += "\n## 検証結果\n\n| Check | Result |\n|---|---|\n| x | ok |\n"
        response = make_run(root, order_with(["docs/goals/g0z-stub.md"]), ledger,
                            {"g0z-stub.md": doc})
        result = run_gate(root, response)
        assert result["exit_code"] == 0, result
        print("20. table after target section: exit_code =", result["exit_code"])


def test_second_table_inside_target_section_is_ignored():
    """`## 対象ノード` 節内で対象ノード表の後に別の表が続いても、最初の表だけを読む。

    空行区切りの表と `### ` 小見出し区切りの表の両方を含む1文書で確かめる。
    """
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        ids = ["a/A.scala", "a/B.scala"]
        ledger = ledger_yaml([(ids[0], "green", "G0z"), (ids[1], "green", "G0z")])
        doc = one_goal_doc("G0z", ids)["g0z-stub.md"]
        doc += (
            "\n| 名前 | 備考 |\n|---|---|\n| 補足A | x |\n"
            "\n### 補足\n\n| 名前 | 備考 |\n|---|---|\n| 補足B | y |\n"
        )
        response = make_run(root, order_with(["docs/goals/g0z-stub.md"]), ledger,
                            {"g0z-stub.md": doc})
        result = run_gate(root, response)
        assert result["exit_code"] == 0, result
        print("21. second table inside target section: exit_code =", result["exit_code"])


def test_goal_path_with_trailing_subpath_is_not_adopted():
    """`docs/goals/<x>.md/<sub>` の後続部分パスを、ゴール文書パスとして採らない。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        ids = ["a/A.scala"]
        ledger = ledger_yaml([(ids[0], "green", "G0z")])
        response = make_run(
            root, "参照: docs/goals/g0z-stub.md/notes.md を確認する。\n", ledger,
            one_goal_doc("G0z", ids))
        result = run_gate(root, response)
        assert result["exit_code"] != 0, result
        assert "0 件" in result["detail"], result
        print("22. goal path with trailing subpath: exit_code =", result["exit_code"])


def test_goal_path_with_trailing_extension_is_not_adopted():
    """`docs/goals/<x>.md.<ext>` の後続拡張子を、ゴール文書パスとして採らない。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        ids = ["a/A.scala"]
        ledger = ledger_yaml([(ids[0], "green", "G0z")])
        response = make_run(root, "参照: docs/goals/g0z-stub.md.bak\n", ledger,
                            one_goal_doc("G0z", ids))
        result = run_gate(root, response)
        assert result["exit_code"] != 0, result
        assert "0 件" in result["detail"], result
        print("23. goal path with trailing extension: exit_code =", result["exit_code"])


def test_goal_path_with_sentence_punctuation_is_adopted():
    """整形式のゴール文書パスは、後ろに文の句読点が続いても1件だけ採用される。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        ids = ["a/A.scala", "a/B.scala"]
        ledger = ledger_yaml([(ids[0], "green", "G0z"), (ids[1], "green", "G0z")])
        response = make_run(root, "定義の正は docs/goals/g0z-stub.md。\n", ledger,
                            one_goal_doc("G0z", ids))
        result = run_gate(root, response)
        assert result["exit_code"] == 0, result
        print("24. well-formed goal path in sentence: exit_code =", result["exit_code"])


def test_separator_line_with_three_dashes_is_skipped():
    """`\\|---\\|` の区切り線は区切りとして扱い、id として数えない。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        ids = ["a/A.scala", "a/B.scala"]
        ledger = ledger_yaml([(ids[0], "green", "G0z"), (ids[1], "green", "G0z")])
        response = make_run(root, order_with(["docs/goals/g0z-stub.md"]), ledger,
                            one_goal_doc("G0z", ids))
        result = run_gate(root, response)
        assert result["exit_code"] == 0, result
        print("25. three-dash separator: exit_code =", result["exit_code"])


def test_single_dash_data_row_is_not_treated_as_separator():
    """`\\| - \\| - \\|` の行はデータ行として読み、id 列から読めない失敗にする。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        ids = ["a/A.scala"]
        ledger = ledger_yaml([(ids[0], "green", "G0z")])
        doc = (
            "# ゴール G0z：テスト用のゴール\n\n"
            "## 対象ノード（1件）\n\n"
            "| id | ケース数 |\n|---|---|\n| `a/A.scala` | 1 |\n| - | - |\n"
        )
        response = make_run(root, order_with(["docs/goals/g0z-stub.md"]), ledger,
                            {"g0z-stub.md": doc})
        result = run_gate(root, response)
        assert result["exit_code"] != 0, result
        assert "対象ノードの表に行が無い" not in result["detail"], result
        assert "'-'" in result["detail"], result
        print("26. single-dash data row: exit_code =", result["exit_code"])


def test_row_missing_id_column_fails_with_shape_reason():
    """id 列が欠けた行は、例外名ではなく表の形状を説明する理由で失敗する。"""
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        ids = ["a/A.scala"]
        ledger = ledger_yaml([(ids[0], "green", "G0z")])
        doc = (
            "# ゴール G0z：テスト用のゴール\n\n"
            "## 対象ノード（1件）\n\n"
            "| 処理順 | id | ケース数 |\n|---|---|---|\n"
            "| 1 | `a/A.scala` | 1 |\n| 2 |\n"
        )
        response = make_run(root, order_with(["docs/goals/g0z-stub.md"]), ledger,
                            {"g0z-stub.md": doc})
        result = run_gate(root, response)
        assert result["exit_code"] != 0, result
        assert "id 列が欠けた行" in result["detail"], result
        print("27. row missing id column: exit_code =", result["exit_code"])


def main() -> int:
    tests = [
        test_pending_node_is_not_complete,
        test_all_green_is_complete,
        test_green_and_do_not_port_is_complete,
        test_goal_without_members_is_not_complete,
        test_green_and_not_built_is_complete,
        test_mixed_green_with_approved_not_built_is_complete,
        test_mixed_green_with_unapproved_not_built_is_not_complete,
        test_mixed_green_with_count_mismatch_is_not_complete,
        test_mixed_green_with_unapproved_excluded_is_not_complete,
        test_mixed_green_with_cross_list_duplicate_is_not_complete,
        test_mixed_green_with_ported_above_passing_is_not_complete,
        test_mixed_green_with_scalar_not_built_is_not_complete,
        test_mixed_green_with_negative_counts_is_not_complete,
        test_not_built_without_approval_is_not_complete,
        test_not_built_with_malformed_approval_is_not_complete,
        test_not_built_with_duplicated_case_names_is_not_complete,
        test_not_built_with_a_blank_case_name_is_not_complete,
        test_retired_node_and_carried_case_are_checked_and_not_double_counted,
        test_goal_whose_nodes_are_all_retired_is_not_complete,
        test_retired_dates_must_be_real_calendar_days,
        test_a_case_carried_to_a_node_that_is_not_the_successor_is_refused,
        test_malformed_lineage_fields_are_diagnosed_and_other_goals_continue,
        test_lineage_refuses_duplicates_unknown_tags_and_a_non_list_retired,
        test_a_retired_side_problem_keeps_the_successors_goal_open_and_the_rollup_fails,
        test_lineage_refuses_a_renamed_duplicate_and_more_cases_than_the_retired_node_ported,
        test_a_duplicated_retired_id_reaches_every_successor_it_names,
        test_a_green_successor_must_cover_its_total_with_ported_and_approved_exclusions,
        test_a_scalar_retired_list_is_diagnosed_without_crashing_the_goal_check,
        test_the_named_successor_is_judged_the_same_whichever_node_comes_first,
        test_a_green_successor_counts_only_valid_distinct_exclusions_and_its_carried_cases,
        test_the_gate_reads_an_invalid_unquoted_retired_date_without_crashing,
        test_a_retired_record_with_a_bad_id_still_reaches_its_named_successor,
        test_a_non_list_retired_fails_a_goal_check_even_when_the_goal_is_green,
        test_a_green_successor_cannot_count_a_carried_case_again_as_an_exclusion,
        test_a_goal_with_active_and_retired_members_is_judged_through_the_gate,
        test_a_do_not_port_successor_with_carried_cases_does_not_complete_its_goal,
        test_takt_triage_treats_the_same_statuses_as_done,
        test_not_built_with_a_nonzero_or_missing_passing_is_not_complete,
        test_non_list_not_built_record_is_diagnosed_without_crashing,
        test_another_goals_not_built_shortfall_stays_out_of_the_goal_pending_list,
        test_non_mapping_triage_or_cases_is_diagnosed_and_other_goals_continue,
        test_check_needs_continues_past_another_goals_broken_not_built,
        test_not_built_node_with_an_exclusion_is_not_complete_or_double_counted,
        test_not_built_with_a_non_integer_total_or_padded_duplicate_name_is_not_complete,
        test_order_without_goal_path_fails,
        test_order_with_multiple_goal_paths_fails,
        test_missing_goal_document_fails,
        test_unreadable_goal_id_fails,
        test_missing_environment_variable_fails,
        test_missing_response_file_fails,
        test_two_levels_up_does_not_reach_order,
        test_table_ids_must_match_ledger_goal,
        test_table_without_id_column_fails,
        test_two_and_five_column_tables_both_pass,
        test_real_goal_documents_match_current_ledger,
        test_without_order_md_uses_meta_task,
        test_without_order_md_and_without_meta_fails,
        test_invalid_ledger_yaml_yields_json_failure,
        test_table_after_target_section_is_ignored,
        test_second_table_inside_target_section_is_ignored,
        test_goal_path_with_trailing_subpath_is_not_adopted,
        test_goal_path_with_trailing_extension_is_not_adopted,
        test_goal_path_with_sentence_punctuation_is_adopted,
        test_separator_line_with_three_dashes_is_skipped,
        test_single_dash_data_row_is_not_treated_as_separator,
        test_row_missing_id_column_fails_with_shape_reason,
    ]
    failures = 0
    for test in tests:
        try:
            test()
        except AssertionError as error:
            failures += 1
            print(f"FAIL {test.__name__}: {error}")
        except Exception:  # 未実装（スクリプトが無い）や想定外は、失敗として記録する
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
