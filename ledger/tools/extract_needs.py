#!/usr/bin/env python3
"""台帳の各ノードについて、Pekko 原典ファイルから needs を抽出する。

使い方:
    uv run --with pyyaml python3 ledger/tools/extract_needs.py \
      --ledger ledger/nodes.yaml --capabilities ledger/capabilities.yaml --pekko pekko

needs.testkit には capabilities.yaml の regex に 1 回以上一致した能力 id を、
needs.premise_flags には premise_flags の一致した id を、それぞれ一致順で書く。
一致が無ければ空リスト [] を書く。null は「未抽出」を意味する。

nodes.yaml は全体を書き直さず、各ノードの needs 配下の testkit 行と
premise_flags 行だけを置き換える（verify_red.py の status 行置換と同じ流儀）。
したがって再実行しても差分は出ない。
"""
import argparse
import os
import re
import sys

import yaml

Capability = dict


def load_ledger(path: str) -> tuple[list[dict], list[str]]:
    with open(path, encoding="utf-8") as f:
        lines = f.read().splitlines(keepends=True)
    with open(path, encoding="utf-8") as f:
        nodes = (yaml.safe_load(f) or {}).get("nodes", [])
    return nodes, lines


def load_capabilities(path: str) -> tuple[list[Capability], list[Capability]]:
    with open(path, encoding="utf-8") as f:
        doc = yaml.safe_load(f) or {}
    return doc.get("capabilities") or [], doc.get("premise_flags") or []


def render_list(ids: list[str]) -> str:
    if not ids:
        return "[]"
    return "[" + ", ".join(f"'{i}'" for i in ids) + "]"


def match_ids(patterns: list[Capability], text: str) -> list[str]:
    return [c["id"] for c in patterns if re.search(c["regex"], text)]


def replace_needs(lines: list[str], by_node: dict[str, dict[str, list[str]]]) -> list[str]:
    out: list[str] = []
    current_id: str | None = None
    in_needs = False
    saw_needs = False
    for line in lines:
        if line.startswith("- id: "):
            current_id = line.split(":", 1)[1].strip()
            in_needs = False
            saw_needs = False
        elif current_id is None:
            out.append(line)
            continue
        elif re.match(r"^  needs:\s*$", line):
            in_needs = True
            saw_needs = True
            out.append(line)
            continue
        elif in_needs and re.match(r"^\s{4}(testkit|premise_flags):\s*", line):
            indent = line[: len(line) - len(line.lstrip())]
            key = "testkit" if "testkit:" in line else "premise_flags"
            needs = by_node[current_id][key]
            out.append(f"{indent}{key}: {render_list(needs)}\n")
            continue
        elif in_needs and re.match(r"^  \S", line):
            in_needs = False
        if in_needs and not saw_needs:
            raise ValueError(f"needs ブロックの書式が想定外: {current_id}")
        out.append(line)
    return out


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--ledger", required=True)
    ap.add_argument("--capabilities", required=True)
    ap.add_argument("--pekko", required=True)
    args = ap.parse_args()

    nodes, lines = load_ledger(args.ledger)
    capabilities, premise_flags = load_capabilities(args.capabilities)

    missing = [n["id"] for n in nodes if not os.path.isfile(os.path.join(args.pekko, n["id"]))]
    if missing:
        print(f"原典ファイルが見つからないノード {len(missing)} 件:", file=sys.stderr)
        for node_id in missing:
            print(f"  - {node_id}", file=sys.stderr)
        return 1

    counts: dict[str, int] = {c["id"]: 0 for c in capabilities}
    flag_counts: dict[str, int] = {f["id"]: 0 for f in premise_flags}
    by_node: dict[str, dict[str, list[str]]] = {}
    for n in nodes:
        with open(os.path.join(args.pekko, n["id"]), encoding="utf-8", errors="replace") as f:
            text = f.read()
        testkit = match_ids(capabilities, text)
        flags = match_ids(premise_flags, text)
        by_node[n["id"]] = {"testkit": testkit, "premise_flags": flags}
        for i in testkit:
            counts[i] += 1
        for i in flags:
            flag_counts[i] += 1

    lines = replace_needs(lines, by_node)
    with open(args.ledger, "w", encoding="utf-8") as f:
        f.write("".join(lines))

    with open(args.ledger, encoding="utf-8") as f:
        written = (yaml.safe_load(f) or {}).get("nodes", [])
    mismatched = [n["id"] for n in written if (n.get("needs") or {}) != by_node[n["id"]]]
    if mismatched:
        print(f"書き込み結果が一致しないノード {len(mismatched)} 件: {mismatched[:5]}", file=sys.stderr)
        return 1

    print(f"ノード {len(nodes)} 件、能力 {len(capabilities)} 件、前提フラグ {len(premise_flags)} 件")
    print(f"{'能力':30s} {'種別':14s} {'一致ノード数':>6s}  provided_by")
    for c in capabilities:
        print(f"{c['id']:30s} {'testkit':14s} {counts[c['id']]:6d}  {c.get('provided_by')}")
    for p in premise_flags:
        print(f"{p['id']:30s} {'premise':14s} {flag_counts[p['id']]:6d}  -")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
