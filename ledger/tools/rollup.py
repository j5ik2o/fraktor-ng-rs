#!/usr/bin/env python3
"""台帳（ledger/nodes.yaml）からロールアップとゴール完了判定を計算する。

使い方:
    python3 rollup.py --ledger ledger/nodes.yaml
    python3 rollup.py --ledger ledger/nodes.yaml --goal G0a

ロールアップの定義（承認済みグラフモデル）:
  主指標はクラス数ベースの合格率 G / (N - X)。N は kind=test のノード数、
  G はそのうち status=green の数、X は status=do_not_port の数。
  除外率 X / N を必ず並べて出す。fixture は分母に入らない。
  全体はモジュール平均ではなく、全ノードの合算で出す。

ゴール完了判定:
  ゴールに属するノード（goal 欄が一致するもの）がすべて green か do_not_port で、
  かつ1件以上あること。未完了の内訳（status 別の件数）も出す。
  終了コード: ゴールを指定した場合、完了していなければ 1。
"""
import argparse
import collections
import sys

import yaml


def summarize(nodes: list[dict]) -> dict:
    test_nodes = [n for n in nodes if n.get("kind") == "test"]
    n_total = len(test_nodes)
    green = sum(1 for n in test_nodes if n.get("status") == "green")
    excluded = sum(1 for n in test_nodes if n.get("status") == "do_not_port")
    denom = n_total - excluded
    return {
        "n": n_total,
        "green": green,
        "excluded": excluded,
        "pass_rate": (green / denom) if denom else None,
        "exclusion_rate": (excluded / n_total) if n_total else None,
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
        f"合格率={fmt_rate(overall['pass_rate'])} 除外率={fmt_rate(overall['exclusion_rate'])} "
        f"(fixture {overall['fixtures']} 件は分母外)"
    )
    print(f"  status 内訳: {dict(overall['by_status'])}")

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
            f"合格率={fmt_rate(s['pass_rate']):>6s} 除外率={fmt_rate(s['exclusion_rate']):>6s}"
        )

    if args.goal:
        members = [n for n in nodes if n.get("goal") == args.goal]
        if not members:
            print(f"ゴール {args.goal}: 属するノードが無い（未発行か ID の誤り）")
            return 1
        pending = [n for n in members if n.get("status") not in ("green", "do_not_port")]
        print(
            f"ゴール {args.goal}: ノード {len(members)} 件、"
            f"status 内訳 {dict(collections.Counter(n.get('status') for n in members))}"
        )
        if pending:
            print(f"  未完了 {len(pending)} 件:")
            for n in pending:
                print(f"    - {n['id']} ({n.get('status')})")
            return 1
        print(f"  完了: 全ノードが green か do_not_port")
        return 0
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
