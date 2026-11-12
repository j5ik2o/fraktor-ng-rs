#!/usr/bin/env python3
"""ゴールの needs 閉性を判定する。

使い方:
    uv run --with pyyaml python3 ledger/tools/check_needs.py \
      --ledger ledger/nodes.yaml --capabilities ledger/capabilities.yaml --goal G0b

goal 欄が指定 ID の各ノードについて、needs.testkit の各能力の provided_by が
(a) 指定 ID 自身、または (b) 所属ノードがすべて green のゴール、なら充足とする。
不足があれば一覧を出して exit 1、全充足なら exit 0。
needs.testkit が null（未抽出）のノードがあれば exit 1。該当ノードが無ければ exit 1。
"""
import argparse
import collections
import sys

import yaml


def load_capabilities(path: str) -> dict[str, dict]:
    with open(path, encoding="utf-8") as f:
        doc = yaml.safe_load(f) or {}
    return {c["id"]: c for c in doc.get("capabilities") or []}


def green_goals(nodes: list[dict]) -> set[str]:
    """所属ノードがすべて green か do_not_port のゴール（rollup の完了判定と同じ意味論）。"""
    by_goal: dict[str, list[dict]] = collections.defaultdict(list)
    for n in nodes:
        if n.get("goal"):
            by_goal[n["goal"]].append(n)
    return {g for g, members in by_goal.items()
            if all(m.get("status") in ("green", "do_not_port") for m in members)}


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--ledger", required=True)
    ap.add_argument("--capabilities", required=True)
    ap.add_argument("--goal", required=True)
    args = ap.parse_args()

    with open(args.ledger, encoding="utf-8") as f:
        nodes = (yaml.safe_load(f) or {}).get("nodes", [])
    capabilities = load_capabilities(args.capabilities)
    done = green_goals(nodes) | {args.goal}

    members = [n for n in nodes if n.get("goal") == args.goal]
    if not members:
        print(f"ゴール {args.goal}: 該当ノードなし（未発行か ID の誤り）")
        return 1

    unevaluated = [n["id"] for n in members if (n.get("needs") or {}).get("testkit") is None]
    if unevaluated:
        print(f"ゴール {args.goal}: needs.testkit が未抽出のノード {len(unevaluated)} 件:")
        for node_id in unevaluated:
            print(f"  - {node_id}")
        return 1

    shortfalls: list[tuple[str, str, object]] = []
    for n in members:
        for cap_id in n["needs"]["testkit"]:
            provided_by = capabilities.get(cap_id, {}).get("provided_by")
            if provided_by not in done:
                shortfalls.append((n["id"], cap_id, provided_by))

    print(f"ゴール {args.goal}: ノード {len(members)} 件")
    print("  前提フラグ（判定には使わない）:")
    for n in members:
        print(f"    - {n['id'].split('/')[-1]}: {n['needs'].get('premise_flags')}")

    if shortfalls:
        print(f"  未充足 {len(shortfalls)} 件:")
        for node_id, cap_id, provided_by in shortfalls:
            print(f"    - {node_id.split('/')[-1]}: {cap_id} (provided_by={provided_by})")
        return 1

    print(f"ゴール {args.goal}: needs 閉性 OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
