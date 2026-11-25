#!/usr/bin/env python3
"""親ワークフローの完了関門（`fraktor-porting-ng` の system step から呼ばれる）。

ゴール完了を、run 内の記録ではなく台帳の `goal` 欄と `rollup.py --goal` の終了コードで決める。
triage や ledger-update の指示に頼らず、機械的に次を順に行う。

1. `TAKT_PREVIOUS_RESPONSE_FILE` の **3階層上** を run の context ディレクトリとする
   （`<context>/system-commands/<uuid>/response.txt`）。それより上は遡らない。
2. ゴール文書パスを `<context>/task/order.md` から探し、無ければ run 直下の
   `meta.json` の `task`（`-t` 直渡しの発行時タスク文字列）から探す。`docs/goals/<x>.md`
   を1件だけ採る（0件・複数件は失敗）。
3. ゴール文書の **先頭行だけ** から `# ゴール G0x：…` の ID を取り出す。
4. ゴール文書の `## 対象ノード` 節内の `id` 列と、台帳で `goal == <ID>` のノード集合が
   一致することを確かめる（要件10）。節外の表は読まない。表が読めない・空・不一致は失敗。
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

from excluded_approvals import ledger_loader

LEDGER = os.path.join("ledger", "nodes.yaml")
ROLLUP = os.path.join("ledger", "tools", "rollup.py")
GOAL_PATH = re.compile(r"(?<![\w.-])docs/goals/[A-Za-z0-9_-]+\.md(?![\w./-])")
TABLE_SEPARATOR = re.compile(r":?-{3,}:?")
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
    """タスク文書から `docs/goals/<x>.md` を集める。1件だけなら採る。

    `docs/goals/` に続く `<x>` は `.md` の直後まで1つのファイル名として扱い、
    `docs/goals/<x>.md/<sub>` や `docs/goals/<x>.md.<ext>` のような後続文字を
    持つ参照はゴール文書パスとして採らない（`GOAL_PATH` の前後の境界で判定する）。
    """
    found = sorted(set(GOAL_PATH.findall(order_text)))
    if len(found) != 1:
        return None, f"タスク文書のゴール文書パスが {len(found)} 件: {found}"
    return found[0], ""


def task_text_for_goal(context: str) -> tuple[str | None, str]:
    """ゴール文書パスを探す入力テキストを解決する。

    `<context>/task/order.md` を最優先する（taskSpec を伴う発行で作られる）。
    `-t` 直渡しの発行では taskSpec が作られず order.md が無いため、run 直下の
    `meta.json` の `task`（発行時に渡したタスク文字列）へ切り替える。どちらも
    無ければ判定不能として失敗する。
    """
    order = os.path.join(context, "task", "order.md")
    if os.path.exists(order):
        with open(order, encoding="utf-8") as f:
            return f.read(), ""
    meta = os.path.join(context, "..", "meta.json")
    if not os.path.exists(meta):
        return None, f"order.md も meta.json も無い: {order} / {meta}"
    with open(meta, encoding="utf-8") as f:
        task = json.load(f).get("task")
    if not isinstance(task, str):
        return None, f"meta.json の task が文字列ではない: {meta}"
    return task, ""


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
    """`## 対象ノード` 節内の最初の表の `id` 列を読む。見出し行で列位置を決める。

    走査は節の範囲（次の `## ` 見出しの直前まで）に限る。後続する別の節の表
    （検証結果など）を対象ノード表として解釈しない。

    節内で最初の表の見出し行を一度だけ確定し、その後に `|` で始まらない行
    （空行・本文・`### ` 小見出し）が来たら表の終端として走査を止める。節内に
    別の表が続いても、その見出し行を対象ノード表のヘッダとして上書きしない。
    """
    lines = text.splitlines()
    start = next((i for i, l in enumerate(lines) if l.startswith("## 対象ノード")), None)
    if start is None:
        return None, "対象ノードの節が無い"
    section = lines[start:]
    for offset, line in enumerate(section[1:], start=1):
        if line.startswith("## "):
            section = section[:offset]
            break
    header: list[str] | None = None
    ids: list[str] = []
    started = False
    for line in section[1:]:
        if not line.startswith("|"):
            if started:
                break
            continue
        started = True
        cells = [c.strip() for c in line.strip().strip("|").split("|")]
        if header is None:
            header = cells
            continue
        if all(TABLE_SEPARATOR.fullmatch(c) for c in cells):
            continue
        if "id" not in header:
            return None, "対象ノードの表に id 列が無い"
        index = header.index("id")
        if len(cells) <= index:
            return None, f"対象ノードの表に id 列が欠けた行がある: {line.strip()!r}"
        cell = cells[index]
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
    """台帳で `goal == goal_id` の node id を返す。

    最上位の `retired` にある退役したノードで同じ goal を持つものの id も返す。ゴール文書の対象表は
    退役した id を指し続けるので、現役と退役の両方で照合する。完了の判定は rollup に委ねる
    （属するノードがすべて退役したゴールは、rollup が退役を理由に完了と判定しない）。
    """
    if not os.path.exists(LEDGER):
        return None, f"台帳が無い: {LEDGER}"
    # rollup と同じ台帳専用のローダで読む（引用なしの不正な日付も文字列のまま届き、読み込みで止まらない）。
    with open(LEDGER, encoding="utf-8") as f:
        doc = yaml.load(f, Loader=ledger_loader()) or {}
    nodes = doc.get("nodes", [])
    members = [n["id"] for n in nodes if n.get("goal") == goal_id]
    retired = doc.get("retired")
    if isinstance(retired, list):
        # 退役したノードの goal と、ゴール文書の対象表の id は退役後も残る。現役と退役の両方で照合する。
        members += [r["id"] for r in retired
                    if isinstance(r, dict) and r.get("goal") == goal_id and isinstance(r.get("id"), str)]
    return members, ""


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

    task_text, error = task_text_for_goal(context)
    if task_text is None:
        return {"exit_code": FAIL_EXIT, "detail": error}
    goal_path, error = goal_path_from_order(task_text)
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
    try:
        result = evaluate(os.environ.get("TAKT_PREVIOUS_RESPONSE_FILE", ""))
    except Exception as error:  # 入力起因の失敗を JSON 契約へ変換する。原因は隠さない
        result = {"exit_code": FAIL_EXIT,
                  "detail": f"判定中に入力起因の例外: {type(error).__name__}: {error}"}
    result.setdefault("output", "")
    print(json.dumps(result, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
