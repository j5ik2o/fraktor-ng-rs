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
  （cases.not_built）の件数を kind=test の全ノードで合算して出す。除外と作らないは、
  承認記録の有効な件数と、台帳に書かれた件数（括弧内）を分けて出す（case_counts）。

作らない区分（not_built）の記録の検査:
  status=not_built のノードは、triage.kind が not_built で triage.reason があり、
  cases.ported と cases.passing が整数の 0 で、cases.not_built の件数が cases.total に等しく、各要素の
  name・reason・alternative が空白以外の文字列で、承認記録（approved_by、approved_at）が
  除外と同じ is_approved_by_human を満たし、ケース名がノード内で重複しないこと。
  一部のケースだけを作らない green のノード（混在）は、要素の記録が揃い、passing が ported に
  等しく、ported と承認済みの excluded と not_built の件数の和が total に等しいこと。
  満たさないノードは一覧に出し、ゴールの完了に数えない。台帳は除外の検査と同じ
  専用ローダ（excluded_approvals.ledger_loader）で読む。

ゴール完了判定:
  ゴールに属するノード（goal 欄が一致するもの）がすべて green か do_not_port か、
  記録の揃った not_built で、かつ1件以上あること。未完了の内訳（status 別の件数）も出す。
  終了コード: ゴールを指定した場合、完了していなければ 1。
"""
import argparse
import collections
import sys

import yaml

from excluded_approvals import is_approved_by_human, ledger_loader

# ゴールの完了に数える status。check_needs.py も同じ集合を使い、
# TAKT の triage（fraktor-triage.md、fraktor-port-node.yaml）も同じ status を処理済みとする。
DONE_STATUSES = ("green", "do_not_port", "not_built")

# 作らないケースの要素のうち、空白以外の文字列であるべき欄。承認の2欄は is_approved_by_human が検査する。
NOT_BUILT_TEXT_KEYS = ("name", "reason", "alternative")


def load_nodes(path: str) -> list[dict]:
    """台帳を読み、ノードの一覧を返す。

    除外の承認検査（excluded_approvals）と同じ専用ローダで読む。timestamp は文字列のまま
    届くので、引用なしの `approved_at: 2026-11-25` も is_approved_by_human が暦日として
    検査できる。重複キー、merge キー、alias は読み込みで拒否する。
    """
    with open(path, encoding="utf-8") as f:
        return (yaml.load(f, Loader=ledger_loader()) or {}).get("nodes", [])


def is_text(value) -> bool:
    return isinstance(value, str) and bool(value.strip())


def mapping(value) -> dict:
    """マッピングならそのまま、そうでなければ空のマッピングを返す（壊れた記録で止めない）。"""
    return value if isinstance(value, dict) else {}


def is_case_name(value) -> bool:
    """ケース名は空白以外の文字列で、前後に空白を持たない（重複の判定をすり抜けさせない）。"""
    return is_text(value) and value == value.strip()


def is_zero(value) -> bool:
    """整数の 0 か（bool の False と、欠落・null は 0 と見なさない）。"""
    return type(value) is int and value == 0


def is_valid_not_built_case(record) -> bool:
    """作らないケースの1要素が、3つの文字列欄と人による承認記録を持つかを返す。"""
    return (isinstance(record, dict)
            and all(is_text(record.get(k)) for k in NOT_BUILT_TEXT_KEYS)
            and is_case_name(record.get("name"))
            and is_approved_by_human(record))


def not_built_records_problems(records: list) -> list[str]:
    """作らないケースの要素の不足と、ノード内のケース名の重複を返す。"""
    problems = []
    for index, record in enumerate(records):
        if not is_valid_not_built_case(record):
            problems.append(f"cases.not_built[{index}] の name・reason・alternative・承認記録が揃っていない")
    names = [r.get("name").strip() for r in records
             if isinstance(r, dict) and is_text(r.get("name"))]
    duplicated = sorted({name for name in names if names.count(name) > 1})
    if duplicated:
        problems.append(f"cases.not_built のケース名が重複している: {', '.join(duplicated)}")
    return problems


def mixed_not_built_problems(node: dict) -> list[str]:
    """一部のケースだけを作らない区分にした green のノード（混在）の記録の不足を返す。

    cases.not_built が空か欄が無ければ混在ではなく、空を返す。混在のノードは、作らない
    要素の記録が揃い、passing が ported に等しく、ported と承認済みの excluded と not_built の
    件数の和が total に等しいときだけ green として完了に数える。同じケースを除外と作らないの
    両方に置くことは許さない。
    """
    cases = mapping(node.get("cases"))
    records = cases.get("not_built")
    if records is None or records == []:
        return []
    if not isinstance(records, list):
        return ["cases.not_built が配列でない"]
    problems = not_built_records_problems(records)
    excluded = cases.get("excluded") or []
    if not isinstance(excluded, list):
        return problems + ["cases.excluded が配列でない"]
    not_built_names = {r.get("name").strip() for r in records if isinstance(r, dict) and is_text(r.get("name"))}
    excluded_names = {e.get("name").strip() for e in excluded if isinstance(e, dict) and is_text(e.get("name"))}
    overlap = sorted(not_built_names & excluded_names)
    if overlap:
        problems.append(f"同じケースが cases.excluded と cases.not_built の両方にある: {', '.join(overlap)}")
    total, ported, passing = cases.get("total"), cases.get("ported"), cases.get("passing")
    if not all(type(v) is int and v >= 0 for v in (total, ported, passing)):
        return problems + ["cases.total・ported・passing が 0 以上の整数でない"]
    if passing != ported:
        problems.append("cases.passing が cases.ported と違う")
    approved_excluded = sum(1 for e in excluded if isinstance(e, dict) and is_approved_by_human(e))
    if ported + approved_excluded + len(records) != total:
        problems.append("cases.ported と承認済みの excluded と not_built の件数の和が cases.total と違う")
    return problems


def not_built_problems(node: dict) -> list[str]:
    """作らない区分の記録の不足を返す。不足が無ければ空。

    status=not_built のノード（全件を作らない）と、status=green で cases.not_built を持つ
    ノード（混在、mixed_not_built_problems）を検査する。
    """
    if node.get("status") == "green":
        return mixed_not_built_problems(node)
    if node.get("status") != "not_built":
        return []
    problems = []
    if not isinstance(node.get("triage"), dict):
        problems.append("triage がマッピングでない")
    if not isinstance(node.get("cases"), dict):
        problems.append("cases がマッピングでない")
    triage = mapping(node.get("triage"))
    if triage.get("kind") != "not_built":
        problems.append("triage.kind が not_built でない")
    if not is_text(triage.get("reason")):
        problems.append("triage.reason が空")
    cases = mapping(node.get("cases"))
    records = cases.get("not_built")
    if not isinstance(records, list) or not records:
        return problems + ["cases.not_built が空"]
    for key in ("ported", "passing"):
        if not is_zero(cases.get(key)):
            problems.append(f"cases.{key} が整数の 0 でない")
    total = cases.get("total")
    if type(total) is not int or total <= 0:
        problems.append("cases.total が正の整数でない")
    elif len(records) != total:
        problems.append("cases.not_built の件数が cases.total と違う")
    # 全件を作らないノードでは、同じケースを除外にも記録できない（区分が二重になる）。
    if cases.get("excluded"):
        problems.append("全件を作らないノードに cases.excluded がある")
    return problems + not_built_records_problems(records)


def is_done(node: dict) -> bool:
    return node.get("status") in DONE_STATUSES and not not_built_problems(node)


def case_counts(test_nodes: list[dict]) -> dict:
    """ケースの勘定。除外と作らないは、承認記録の有効な要素の数と、記録された要素の数を分けて出す。

    有効な要素の判定は、除外が is_approved_by_human、作らないが is_valid_not_built_case である。
    作らないは、記録の揃ったノード（not_built_problems が空）の要素だけを有効に数える。
    status が not_built のノードの除外は、区分が二重になるので有効に数えない（記録件数には入る）。
    """
    counts = {"ported": 0, "excluded": 0, "excluded_recorded": 0,
              "not_built": 0, "not_built_recorded": 0}
    for n in test_nodes:
        cases = mapping(n.get("cases"))
        ported = cases.get("ported")
        if type(ported) is int:
            counts["ported"] += ported
        # 配列でない記録は数えない。not_built の不正な記録は not_built_problems が診断に出す。
        excluded = cases.get("excluded")
        if isinstance(excluded, list):
            counts["excluded_recorded"] += len(excluded)
            if n.get("status") != "not_built":
                counts["excluded"] += sum(1 for e in excluded
                                          if isinstance(e, dict) and is_approved_by_human(e))
        not_built = cases.get("not_built")
        if isinstance(not_built, list):
            counts["not_built_recorded"] += len(not_built)
            if n.get("status") in ("not_built", "green") and not not_built_problems(n):
                counts["not_built"] += len(not_built)
    return counts


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

    nodes = load_nodes(args.ledger)

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
    print(
        f"  ケース: 移植={cases['ported']} "
        f"除外={cases['excluded']}（記録 {cases['excluded_recorded']}） "
        f"作らない={cases['not_built']}（記録 {cases['not_built_recorded']}）"
        f"  ※除外・作らないは承認記録の有効な件数、括弧内は台帳に書かれた件数"
    )
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
                problems = not_built_problems(n)
                detail = f"、作らない区分の記録が不足: {'; '.join(problems)}" if problems else ""
                print(f"    - {n['id']} ({n.get('status')}{detail})")
            return 1
        print(f"  完了: 全ノードが green か do_not_port か not_built")
        return 0
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
