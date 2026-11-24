#!/usr/bin/env python3
"""台帳（ledger/nodes.yaml）からロールアップとゴール完了判定を計算する。

使い方:
    python3 rollup.py --ledger ledger/nodes.yaml
    python3 rollup.py --ledger ledger/nodes.yaml --goal G0a

ロールアップの定義（承認済みグラフモデル）:
  主指標はクラス数ベースの合格率 G / (N - X - B)。N は kind=test のノード数、
  G はそのうち status=green の数、X は status=do_not_port の数、B は status=not_built
  （作らない区分）の数。除外率 X / N と作らない率 B / N を必ず並べて出す。
  fixture は分母に入らない。全体はモジュール平均ではなく、全ノードの合算で出す。
  ケースの勘定として、移植（cases.ported）、除外（cases.excluded）、作らない
  （cases.not_built）の件数を kind=test の全ノードで合算して出す。

作らない区分（not_built）の記録の検査:
  status=not_built のノードは、triage.kind が not_built で triage.reason があり、
  cases.ported が 0 で、cases.not_built の件数が cases.total に等しく、各要素が
  name・reason・alternative・approved_by・approved_at を持つこと。満たさないノードは
  一覧に出し、ゴールの完了に数えない。

ゴール完了判定:
  ゴールに属するノード（goal 欄が一致するもの）がすべて green か do_not_port か、
  記録の揃った not_built で、かつ1件以上あること。未完了の内訳（status 別の件数）も出す。
  終了コード: ゴールを指定した場合、完了していなければ 1。
"""
import argparse
import collections
import sys

import yaml

# ゴールの完了に数える status。check_needs.py も同じ集合を使う。
DONE_STATUSES = ("green", "do_not_port", "not_built")

NOT_BUILT_CASE_KEYS = ("name", "reason", "alternative", "approved_by", "approved_at")


def not_built_problems(node: dict) -> list[str]:
    """status=not_built のノードの記録の不足を返す。不足が無ければ空。"""
    if node.get("status") != "not_built":
        return []
    problems = []
    triage = node.get("triage") or {}
    if triage.get("kind") != "not_built":
        problems.append("triage.kind が not_built でない")
    if not str(triage.get("reason") or "").strip():
        problems.append("triage.reason が空")
    cases = node.get("cases") or {}
    records = cases.get("not_built") or []
    if cases.get("ported") != 0:
        problems.append("cases.ported が 0 でない")
    if not records or len(records) != cases.get("total"):
        problems.append("cases.not_built の件数が cases.total と違う")
    for record in records:
        missing = [k for k in NOT_BUILT_CASE_KEYS
                   if not isinstance(record, dict) or not str(record.get(k) or "").strip()]
        if missing:
            problems.append(f"cases.not_built の要素に {', '.join(missing)} が無い")
    return problems


def is_done(node: dict) -> bool:
    return node.get("status") in DONE_STATUSES and not not_built_problems(node)


def case_counts(test_nodes: list[dict]) -> dict:
    ported = excluded = not_built = 0
    for n in test_nodes:
        cases = n.get("cases") or {}
        ported += cases.get("ported") or 0
        excluded += len(cases.get("excluded") or [])
        not_built += len(cases.get("not_built") or [])
    return {"ported": ported, "excluded": excluded, "not_built": not_built}


def summarize(nodes: list[dict]) -> dict:
    test_nodes = [n for n in nodes if n.get("kind") == "test"]
    n_total = len(test_nodes)
    green = sum(1 for n in test_nodes if n.get("status") == "green")
    excluded = sum(1 for n in test_nodes if n.get("status") == "do_not_port")
    not_built = sum(1 for n in test_nodes if n.get("status") == "not_built")
    denom = n_total - excluded - not_built
    return {
        "n": n_total,
        "green": green,
        "excluded": excluded,
        "not_built": not_built,
        "pass_rate": (green / denom) if denom else None,
        "exclusion_rate": (excluded / n_total) if n_total else None,
        "not_built_rate": (not_built / n_total) if n_total else None,
        "cases": case_counts(test_nodes),
        "by_status": collections.Counter(n.get("status") for n in test_nodes),
        "fixtures": len(nodes) - n_total,
    }


def fmt_rate(x) -> str:
    return "-" if x is None else f"{x * 100:.1f}%"


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--ledger", required=True)
    ap.add_argument("--goal", default=None, help="ゴール ID を指定すると完了判定も行う")
    args = ap.parse_args()

    with open(args.ledger, encoding="utf-8") as f:
        nodes = (yaml.safe_load(f) or {}).get("nodes", [])

    overall = summarize(nodes)
    print(
        f"全体: N={overall['n']} green={overall['green']} do_not_port={overall['excluded']} "
        f"not_built={overall['not_built']} "
        f"合格率={fmt_rate(overall['pass_rate'])} 除外率={fmt_rate(overall['exclusion_rate'])} "
        f"作らない率={fmt_rate(overall['not_built_rate'])} "
        f"(fixture {overall['fixtures']} 件は分母外)"
    )
    print(f"  status 内訳: {dict(overall['by_status'])}")
    cases = overall["cases"]
    print(f"  ケース: 移植={cases['ported']} 除外={cases['excluded']} 作らない={cases['not_built']}")
    for n in nodes:
        problems = not_built_problems(n)
        if problems:
            print(f"  作らない区分の記録が不足: {n['id']}: {'; '.join(problems)}")

    by_module: dict[str, list[dict]] = collections.defaultdict(list)
    for n in nodes:
        by_module[n["pekko"]["module"]].append(n)
    print("モジュール別:")
    for module in sorted(by_module):
        s = summarize(by_module[module])
        if s["n"] == 0:
            continue
        print(
            f"  {module:28s} N={s['n']:4d} green={s['green']:4d} "
            f"合格率={fmt_rate(s['pass_rate']):>6s} 除外率={fmt_rate(s['exclusion_rate']):>6s} "
            f"作らない率={fmt_rate(s['not_built_rate']):>6s}"
        )

    if args.goal:
        members = [n for n in nodes if n.get("goal") == args.goal]
        if not members:
            print(f"ゴール {args.goal}: 属するノードが無い（未発行か ID の誤り）")
            return 1
        pending = [n for n in members if not is_done(n)]
        print(
            f"ゴール {args.goal}: ノード {len(members)} 件、"
            f"status 内訳 {dict(collections.Counter(n.get('status') for n in members))}"
        )
        if pending:
            print(f"  未完了 {len(pending)} 件:")
            for n in pending:
                print(f"    - {n['id']} ({n.get('status')})")
            return 1
        print(f"  完了: 全ノードが green か do_not_port か not_built")
        return 0
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
