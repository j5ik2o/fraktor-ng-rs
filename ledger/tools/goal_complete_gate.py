#!/usr/bin/env python3
"""親ワークフローの完了関門（`fraktor-porting-ng` の system step から呼ばれる）。

ゴール完了を、run 内の記録ではなく台帳の `goal` 欄と `rollup.py --goal` の終了コードで決める。
triage や ledger-update の指示に頼らず、機械的に次を順に行う。

1. `TAKT_PREVIOUS_RESPONSE_FILE` の **3階層上** を run の context ディレクトリとする
   （`<context>/system-commands/<uuid>/response.txt`）。それより上は遡らない。
2. `<context>/task/order.md` を読み、`docs/goals/<x>.md` を1件だけ採る（0件・複数件は失敗）。
3. ゴール文書の **先頭行だけ** から `# ゴール G0x：…` の ID を取り出す。
4. ゴール文書の `## 対象ノード` 表の `id` 列と、台帳で `goal == <ID>` のノード集合が
   一致することを確かめる（要件10）。表が読めない・空・不一致は失敗。
5. `rollup.py --goal <ID>` を実行し、その終了コードと出力（未完了 id の一覧を含む）を返す。

`output: json` の system step は終了コード 0 のときだけ stdout の JSON を読む。したがって
この script は **常に終了コード 0** で JSON を1つ出す。完了可否は JSON の `exit_code`
（rollup の終了コード、失敗時は 2）が持つ。workflow の rule は `exit_code == 0` で COMPLETE、
それ以外は port_node へ戻す。
"""
import json
import os
import re
import subprocess
import sys

import yaml

LEDGER = os.path.join("ledger", "nodes.yaml")
ROLLUP = os.path.join("ledger", "tools", "rollup.py")
GOAL_PATH = re.compile(r"docs/goals/[A-Za-z0-9._-]+\.md")
GOAL_ID = re.compile(r"^# ゴール (G0[a-zA-Z0-9]+)：")
TABLE_ID = re.compile(r"`([^`]+)`")
FAIL_EXIT = 2


def context_directory(response: str) -> str | None:
    """応答ファイルの3階層上を context ディレクトリとして返す。名前の確認も行う。

    `<context>/system-commands/<uuid>/response.txt` の3階層上が `<context>`。
    ここを超えて遡らない（`.takt/runs` や別 run を拾わない）。
    """
    base = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(response))))
    return base if os.path.basename(base) == "context" else None


def goal_path_from_order(order_text: str) -> tuple[str | None, str]:
    """order.md から `docs/goals/<x>.md` を集める。1件だけなら採る。"""
    found = sorted(set(GOAL_PATH.findall(order_text)))
    if len(found) != 1:
        return None, f"order.md のゴール文書パスが {len(found)} 件: {found}"
    return found[0], ""


def goal_id_from_document(text: str) -> tuple[str | None, str]:
    """ゴール文書の先頭行だけから ID を取る。"""
    lines = text.splitlines()
    if not lines:
        return None, "ゴール文書が空"
    m = GOAL_ID.match(lines[0])
    if not m:
        return None, f"ゴール文書の1行目から ID を取れない: {lines[0]!r}"
    return m.group(1), ""


def table_ids_from_document(text: str) -> tuple[list[str] | None, str]:
    """`## 対象ノード` 直後の表の `id` 列を読む。見出し行で列位置を決める。"""
    lines = text.splitlines()
    start = next((i for i, l in enumerate(lines) if l.startswith("## 対象ノード")), None)
    if start is None:
        return None, "対象ノードの節が無い"
    header: list[str] | None = None
    ids: list[str] = []
    for line in lines[start:]:
        if not line.startswith("|"):
            continue
        cells = [c.strip() for c in line.strip().strip("|").split("|")]
        if header is None:
            header = cells
            continue
        if all(set(c) <= set("-: ") for c in cells):
            continue
        if "id" not in header:
            return None, "対象ノードの表に id 列が無い"
        cell = cells[header.index("id")]
        m = TABLE_ID.search(cell)
        if not m:
            return None, f"表の id 列からパスを読めない: {cell!r}"
        ids.append(m.group(1))
    if header is None:
        return None, "対象ノードの表が無い"
    if not ids:
        return None, "対象ノードの表に行が無い"
    return ids, ""


def ledger_goal_ids(goal_id: str) -> tuple[list[str] | None, str]:
    """台帳で `goal == goal_id` の node id を返す。"""
    if not os.path.exists(LEDGER):
        return None, f"台帳が無い: {LEDGER}"
    with open(LEDGER, encoding="utf-8") as f:
        nodes = (yaml.safe_load(f) or {}).get("nodes", [])
    return [n["id"] for n in nodes if n.get("goal") == goal_id], ""


def run_rollup(goal_id: str) -> tuple[int, str]:
    """`rollup.py --goal` を実行し、終了コードと出力を返す。"""
    proc = subprocess.run(
        [sys.executable, ROLLUP, "--ledger", LEDGER, "--goal", goal_id],
        capture_output=True, text=True,
    )
    return proc.returncode, (proc.stdout + proc.stderr).strip()


def evaluate(response: str | None) -> dict:
    """関門の判定を組み立てる。成功時は rollup の終了コードと出力を返す。"""
    if not response:
        return {"exit_code": FAIL_EXIT, "detail": "TAKT_PREVIOUS_RESPONSE_FILE が未設定"}
    if not os.path.exists(response):
        return {"exit_code": FAIL_EXIT, "detail": f"応答ファイルが無い: {response}"}

    context = context_directory(response)
    if context is None:
        return {"exit_code": FAIL_EXIT,
                "detail": f"3階層上が context ディレクトリではない: {response}"}

    order = os.path.join(context, "task", "order.md")
    if not os.path.exists(order):
        return {"exit_code": FAIL_EXIT, "detail": f"order.md が無い: {order}"}
    with open(order, encoding="utf-8") as f:
        goal_path, error = goal_path_from_order(f.read())
    if goal_path is None:
        return {"exit_code": FAIL_EXIT, "detail": error}

    if not os.path.exists(goal_path):
        return {"exit_code": FAIL_EXIT, "detail": f"ゴール文書が無い: {goal_path}"}
    with open(goal_path, encoding="utf-8") as f:
        document = f.read()
    goal_id, error = goal_id_from_document(document)
    if goal_id is None:
        return {"exit_code": FAIL_EXIT, "detail": error}

    document_ids, error = table_ids_from_document(document)
    if document_ids is None:
        return {"exit_code": FAIL_EXIT, "detail": error}
    member_ids, error = ledger_goal_ids(goal_id)
    if member_ids is None:
        return {"exit_code": FAIL_EXIT, "detail": error}
    if sorted(document_ids) != sorted(member_ids):
        return {"exit_code": FAIL_EXIT,
                "detail": f"ゴール {goal_id} の対象ノードが台帳の goal と一致しない: "
                          f"文書 {sorted(document_ids)} / 台帳 {sorted(member_ids)}"}

    exit_code, output = run_rollup(goal_id)
    return {"exit_code": exit_code, "goal": goal_id, "output": output}


def main() -> int:
    result = evaluate(os.environ.get("TAKT_PREVIOUS_RESPONSE_FILE", ""))
    result.setdefault("output", "")
    print(json.dumps(result, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
