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


def line_sections(lines: list[str]) -> list[str | None]:
    """各行が属する最上位の欄の名前を返す（`nodes`、`retired` など。最初の欄より前は None）。

    最上位の欄は列 0 の `<名前>:` の行で始まり、次の最上位の欄の行まで続く。列 0 の注記や空行は
    欄を変えない。最上位の欄の順序は問わない（`retired` が `nodes` より前でもよい）。
    """
    sections: list[str | None] = []
    current = None
    for line in lines:
        top = TOP_LEVEL_KEY.match(line)
        if top:
            current = line.split(":", 1)[0].strip()
        sections.append(current)
    return sections


def node_span(lines: list[str], node_id: str) -> tuple[int, int] | None:
    """`nodes` の中で `- id: <node_id>` のノードが占める行の範囲 [start, end) を返す。無ければ None。

    ノードは次の `- id:` か、次の最上位の欄の行で終わる。`nodes` の外（退役したノードなど）は探さない。
    """
    sections = line_sections(lines)
    start = next((i for i, line in enumerate(lines)
                  if sections[i] == "nodes" and line.startswith("- id: ")
                  and line.split(":", 1)[1].strip() == node_id), None)
    if start is None:
        return None
    end = next((j for j in range(start + 1, len(lines))
                if lines[j].startswith("- id: ") or TOP_LEVEL_KEY.match(lines[j])), len(lines))
    return start, end


def _insert_after_key(lines: list[str], node_id: str, key: str, new_line: str) -> list[str]:
    """対象ノードの key 行の直後に new_line を差し込む（key が無ければ何もしない）。"""
    span = node_span(lines, node_id)
    if span is None:
        return lines
    for i in range(span[0] + 1, span[1]):
        if re.match(rf"\s+{re.escape(key)}\s*$", lines[i]):
            return lines[: i + 1] + [new_line] + lines[i + 1 :]
    return lines


def _set_top_level_key(lines: list[str], node_id: str, key: str, value: str) -> list[str]:
    """対象ノードの最上位の key 行を value に置き換える。無ければノードの末尾に足す。"""
    span = node_span(lines, node_id)
    if span is None:
        return lines
    start, end = span
    new_line = f"  {key}: {_yaml_scalar(value)}\n"
    for i in range(start + 1, end):
        if re.match(rf"  {re.escape(key)}:", lines[i]):
            return lines[:i] + [new_line] + lines[i + 1:]
    # ノードの後ろの空行と注記（次の欄や退役の節の見出しの注記）の前に足す。
    while end > start + 1 and _is_filler(lines[end - 1]):
        end -= 1
    return lines[:end] + [new_line] + lines[end:]
