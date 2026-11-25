#!/usr/bin/env python3
"""台帳（ledger/nodes.yaml）を行単位で書き換える小さな道具（verify_red の mark_green などが使う）。

ノードの範囲は `- id:` から次の `- id:` まで、または `nodes:` の外の最上位の行（`retired:` など）
までである。最上位の欄に入ったら、それ以降の行（退役したノード）は書き換えない。
"""
import json
import re


def _is_empty_reason(line: str) -> bool:
    """`reason:` 行の値が null または空であるかを返す。"""
    value = line.split(":", 1)[1].strip() if ":" in line else ""
    value = re.split(r"\s+#", value, maxsplit=1)[0].strip()
    return value in ("", "null", "~", '""', "''")


def _yaml_scalar(text: str) -> str:
    """triage.reason に書ける 1 行の YAML 値にする。"""
    if re.search(r"[:#]|^\s|\s$", text):
        return json.dumps(text, ensure_ascii=False)
    return text


# 最上位の欄（`retired:` など）。列 0 の注記や空行は欄ではない。
TOP_LEVEL_KEY = re.compile(r"[A-Za-z_][\w-]*\s*:")


def _is_filler(line: str) -> bool:
    """空行か、注記だけの行か（どの位置にあっても、ノードの境界にも欄にもならない）。"""
    return not line.strip() or line.lstrip().startswith("#")


def _leaves_nodes(line: str) -> bool:
    """`nodes:` の外の最上位の欄の行か（`retired:` など）。

    ノードは `- id:` で始まり、次の `- id:` かこの行で終わる。最上位の欄に入ったら、
    以降の行（退役したノードなど）は現役のノードとして扱わない。列 0 の注記や空行は
    境界にしない（ノードの中の注記を挟んでも、同じノードの欄を書き換えられる）。
    """
    return bool(TOP_LEVEL_KEY.match(line)) and not line.startswith("nodes:")


def _insert_after_key(lines: list[str], node_id: str, key: str, new_line: str) -> list[str]:
    """対象ノードの key 行の直後に new_line を差し込む（key が無ければ先頭に追記しない）。"""
    in_block = False
    for i, line in enumerate(lines):
        if _leaves_nodes(line):
            break
        if line.startswith("- id: "):
            in_block = line.split(":", 1)[1].strip() == node_id
            continue
        if in_block and re.match(rf"\s+{re.escape(key)}\s*$", line):
            return lines[: i + 1] + [new_line] + lines[i + 1 :]
    return lines


def _set_top_level_key(lines: list[str], node_id: str, key: str, value: str) -> list[str]:
    """対象ノードの最上位の key 行を value に置き換える。無ければノードの末尾に足す。"""
    start = end = None
    for i, line in enumerate(lines):
        if _leaves_nodes(line):
            end = i if start is not None else None
            break
        if line.startswith("- id: "):
            if start is not None:
                end = i
                break
            if line.split(":", 1)[1].strip() == node_id:
                start = i
    if start is None:
        return lines
    end = len(lines) if end is None else end
    new_line = f"  {key}: {_yaml_scalar(value)}\n"
    for i in range(start + 1, end):
        if re.match(rf"  {re.escape(key)}:", lines[i]):
            return lines[:i] + [new_line] + lines[i + 1:]
    # ノードの後ろの空行と注記（次の欄や退役の節の見出しの注記）の前に足す。
    while end > start + 1 and _is_filler(lines[end - 1]):
        end -= 1
    return lines[:end] + [new_line] + lines[end:]
