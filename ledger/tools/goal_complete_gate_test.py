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
     所属と一致する（文字列をテストへ埋め込まない）
 16. 失敗時も終了コードは 0 で、JSON に exit_code 非0 を載せる（takt の output: json 契約）
 17. `-t` 直渡し（order.md 無し）でも meta.json の task から完了判定できる
 18. order.md も meta.json も無ければ判定不能として失敗し COMPLETE にしない
 19. 台帳 YAML が不正でも終了コード 0 で JSON を出し exit_code 非0 と原因を返す
 20. `## 対象ノード` 節の後に別の表があっても、節外の表を id と解釈しない
"""
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

GATE = Path(__file__).resolve().parent / "goal_complete_gate.py"
ROLLUP = Path(__file__).resolve().parent / "rollup.py"
REPO = Path(__file__).resolve().parents[2]
GOALS = REPO / "docs" / "goals"
REAL_LEDGER = REPO / "ledger" / "nodes.yaml"

SLUG = "20260101-000000-test-run"
UUID = "11111111-2222-3333-4444-555555555555"


def write(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")


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
    write(root / "ledger" / "tools" / "rollup.py", ROLLUP.read_text(encoding="utf-8"))
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
    write(root / "ledger" / "tools" / "rollup.py", ROLLUP.read_text(encoding="utf-8"))
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
    """
    import re
    import yaml

    ledger_nodes = (yaml.safe_load(REAL_LEDGER.read_text(encoding="utf-8")) or {}).get("nodes", [])
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
            (root / "ledger" / "tools").mkdir(parents=True)
            (root / "ledger" / "tools" / "rollup.py").write_text(
                ROLLUP.read_text(encoding="utf-8"), encoding="utf-8")
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
            assert members, (doc_path.name, "no ledger member for goal", goal_id)

            result = run_gate(root, response)
            assert result["exit_code"] == 0, (doc_path.name, result)
            print(f"15. real doc {doc_path.name} ({goal_id}): exit_code =", result["exit_code"])


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
        write(root / "ledger" / "tools" / "rollup.py", ROLLUP.read_text(encoding="utf-8"))
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


def main() -> int:
    tests = [
        test_pending_node_is_not_complete,
        test_all_green_is_complete,
        test_green_and_do_not_port_is_complete,
        test_goal_without_members_is_not_complete,
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
