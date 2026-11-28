#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""台帳が、差の一覧の一時的なずれを移植済みに数えていないかを検査する。

使い方:
    uv run --with pyyaml python3 scripts/check_pekko_differences.py
    uv run --with pyyaml python3 scripts/check_pekko_differences.py \
      --differences docs/pekko-differences.md --ledger ledger/nodes.yaml

`docs/pekko-differences.md` の表から、区分が「一時的」の行の issue 番号を読む。台帳
（`ledger/nodes.yaml`）のノードのうち、その issue 番号を参照していて（ノードの記述に
`#N` か `issues/N` を含む。値・引用・コメントのどれでもよい）、移植済みのものを一覧に
する。移植済みは `status: green` とする（`docs/decisions/ledger-schema-draft.md` の
「移植済みをもって green とする」）。

1件でも食い違えば一覧を出して exit 1、無ければ exit 0。表の形が崩れている、
または入力ファイルを読めない場合は exit 2。台帳は直さない（直すかは人間が決める）。
"""
from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parents[1]

TABLE_COLUMNS = (
    "区分",
    "話題",
    "原典の行（Pekko v2.0.0）",
    "fraktor の今の振る舞い",
    "issue",
    "戻す段・入る PR",
    "状態",
)
TEMPORARY_PREFIX = "一時的"
PORTED_STATUS = "green"

SEPARATOR_CELL = re.compile(r":?-+:?")
ISSUE_CELL = re.compile(r"#(\d+)")
# `#311` と `issues/311`（URL の中も）を受ける。`#3110` や `#1311` は数えない。
REFERENCE = re.compile(r"(?<![\w#])#(\d+)(?!\d)|(?<!\w)issues/(\d+)(?!\d)")


class InputError(Exception):
    """表か台帳を読めない、または表の形が崩れている。"""


def split_row(line: str) -> list[str] | None:
    """Markdown の表の1行を欄に分ける。表の行でなければ None。"""
    stripped = line.strip()
    if not stripped.startswith("|"):
        return None
    inner = stripped[1:-1] if stripped.endswith("|") else stripped[1:]
    return [cell.strip() for cell in inner.split("|")]


def parse_temporary_issues(text: str) -> list[tuple[int, str]]:
    """差の一覧の表から、区分が「一時的」の行の (issue 番号, 話題) を順に返す。"""
    lines = text.splitlines()
    header_index = None
    for index, line in enumerate(lines):
        cells = split_row(line)
        if cells is not None and tuple(cells) == TABLE_COLUMNS:
            header_index = index
            break
    if header_index is None:
        raise InputError("差の一覧の表の見出しが見つからない（見出しの欄: " + "・".join(TABLE_COLUMNS) + "）")

    separator = split_row(lines[header_index + 1]) if header_index + 1 < len(lines) else None
    if separator is None or len(separator) != len(TABLE_COLUMNS) or not all(
        SEPARATOR_CELL.fullmatch(cell) for cell in separator
    ):
        raise InputError("差の一覧の表の見出しの次の行が区切りの行になっていない")

    temporary: list[tuple[int, str]] = []
    for line in lines[header_index + 2 :]:
        cells = split_row(line)
        if cells is None:
            break
        if len(cells) != len(TABLE_COLUMNS):
            raise InputError(f"表の行の欄が {len(TABLE_COLUMNS)} 個でない: {line}")
        if not cells[0]:
            raise InputError(f"表の行の区分が空: {line}")
        if not cells[0].startswith(TEMPORARY_PREFIX):
            continue
        match = ISSUE_CELL.fullmatch(cells[4])
        if match is None:
            raise InputError(f"「一時的」の行の issue の欄が #N でない: {line}")
        temporary.append((int(match.group(1)), cells[1]))
    return temporary


def strings(value) -> list[str]:
    """ノードの値（入れ子の dict と list を含む）から、文字列をすべて集める。"""
    if isinstance(value, str):
        return [value]
    if isinstance(value, dict):
        found: list[str] = []
        for item in value.values():
            found.extend(strings(item))
        return found
    if isinstance(value, list):
        found = []
        for item in value:
            found.extend(strings(item))
        return found
    return []


def referenced_issues(node: dict, block: str) -> set[int]:
    """ノードが参照している issue 番号（`#N` か `issues/N`）の集合を返す。

    YAML では空白の後の `#` から行末までがコメントになるので、値だけを読むと
    `reason: 直すのは #311` の参照が落ちる。ノードの生の行（値・引用・コメント）を読む。
    """
    found: set[int] = set()
    for text in strings(node) + [block]:
        for match in REFERENCE.finditer(text):
            found.add(int(match.group(1) or match.group(2)))
    return found


def node_blocks(text: str) -> dict[str, str]:
    """台帳の生の行を、`- id:` で始まるノードごとに分ける。"""
    blocks: dict[str, str] = {}
    current: str | None = None
    lines: list[str] = []
    for line in text.splitlines():
        if line.startswith("- id: "):
            if current is not None:
                blocks[current] = "\n".join(lines)
            current = line[len("- id: "):].strip()
            lines = [line]
        elif current is not None:
            lines.append(line)
    if current is not None:
        blocks[current] = "\n".join(lines)
    return blocks


def load_nodes(path: Path, text: str) -> list[dict]:
    try:
        doc = yaml.safe_load(text) or {}
    except yaml.YAMLError as error:
        raise InputError(f"台帳を読めない: {error}") from error
    nodes = doc.get("nodes") if isinstance(doc, dict) else None
    if not isinstance(nodes, list):
        raise InputError(f"台帳に nodes の一覧が無い: {path}")
    return [node for node in nodes if isinstance(node, dict)]


def find_conflicts(
    temporary: list[tuple[int, str]], nodes: list[dict], blocks: dict[str, str]
) -> list[tuple[int, str, str]]:
    """移植済みのノードが一時的なずれの issue を参照している組を返す。"""
    conflicts: list[tuple[int, str, str]] = []
    for node in nodes:
        if node.get("status") != PORTED_STATUS:
            continue
        node_id = str(node.get("id", ""))
        references = referenced_issues(node, blocks.get(node_id, ""))
        for issue, topic in temporary:
            if issue in references:
                conflicts.append((issue, topic, node_id))
    return sorted(conflicts)


def run(differences: Path, ledger: Path) -> int:
    try:
        text = differences.read_text(encoding="utf-8")
    except OSError as error:
        raise InputError(f"差の一覧を読めない: {error}") from error
    temporary = parse_temporary_issues(text)
    try:
        ledger_text = ledger.read_text(encoding="utf-8")
    except OSError as error:
        raise InputError(f"台帳を読めない: {error}") from error
    nodes = load_nodes(ledger, ledger_text)
    ported = sum(1 for node in nodes if node.get("status") == PORTED_STATUS)
    conflicts = find_conflicts(temporary, nodes, node_blocks(ledger_text))

    print(f"差の一覧（{differences}）の一時的なずれ: {len(temporary)} 件")
    print(f"台帳（{ledger}）のノード: {len(nodes)} 件（うち移植済み status: {PORTED_STATUS}: {ported} 件）")
    if not conflicts:
        print("食い違い: 0 件")
        return 0

    print(f"食い違い: {len(conflicts)} 件（移植済みのノードが一時的なずれの issue を参照している）")
    for issue, topic, node_id in conflicts:
        print(f"  - issue #{issue}（{topic}）: {node_id}")
    return 1


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="台帳が差の一覧の一時的なずれを移植済みに数えていないか検査する")
    parser.add_argument("--differences", type=Path, default=ROOT / "docs/pekko-differences.md")
    parser.add_argument("--ledger", type=Path, default=ROOT / "ledger/nodes.yaml")
    args = parser.parse_args(argv)

    try:
        return run(args.differences, args.ledger)
    except InputError as error:
        print(f"ERROR {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
